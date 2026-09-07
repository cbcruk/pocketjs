// Native Pocket3D example; the UI guest build tool is a separate target.
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const app = `${root}/engine/pocket3d/examples/island`;
// Same digest as the 3DS host: devkitARM, libctru, citro3d, citro2d, picasso.
const image = "devkitpro/devkitarm@sha256:116afba8df8453961de2936ffab20dd441edf4d682856c1ec8b0e53d7ed0bbf5";
async function run(cmd: string[], cwd = root, env = process.env) {
  const result = Bun.spawn(cmd, { cwd, env, stdout: "inherit", stderr: "inherit" });
  if (await result.exited !== 0) throw new Error(`Failed: ${cmd[0]}`);
}
const command = process.argv[2] ?? "build";
if (command === "assets") {
  await run([process.env.BLENDER ?? "/Applications/Blender.app/Contents/MacOS/Blender", "--background", "--factory-startup", "--python", `${app}/assets/build_island.py`]);
} else if (command === "test") {
  const testDir = mkdtempSync(`${tmpdir()}/island-perf-`);
  try {
    await run([process.env.CC ?? "cc", "-std=c11", "-Wall", "-Wextra", "-Werror", `${app}/scripts/perf-test.c`, "-o", `${testDir}/perf-test`]);
    await run([`${testDir}/perf-test`]);
  } finally {
    rmSync(testDir, { recursive: true, force: true });
  }
  await run(["bun", `${app}/scripts/validate_assets.ts`]);
  await run(["cargo", "test", "--locked", "--manifest-path", "engine/Cargo.toml", "-p", "pocket-island", "-p", "pocket3d-anim"]);
} else if (["build", "capture", "run"].includes(command)) {
  const flavor = command === "capture" ? "capture" : "release";
  const rustc = Bun.spawnSync(["rustup", "which", "--toolchain", "nightly-2026-07-02", "rustc"]).stdout.toString().trim();
  if (!existsSync(rustc)) throw new Error("Install nightly-2026-07-02 with rust-src");
  await run(["rustup", "run", "nightly-2026-07-02", "cargo", "build", "-Z", "build-std=core,alloc,compiler_builtins", "-Z", "build-std-features=compiler-builtins-mem", "--release", "--locked"], `${app}/3ds/core`, { ...process.env, RUSTC: rustc });
  const revision = Bun.spawnSync(["git", "rev-parse", "--short=12", "HEAD"], { cwd: root }).stdout.toString().trim();
  const dirty = Bun.spawnSync(["git", "status", "--porcelain"], { cwd: root }).stdout.length > 0;
  const buildId = `${revision}${dirty ? "+dirty" : ""}`;
  await run(["docker", "run", "--rm", "-v", `${root}:/repo`, "-w", "/repo/engine/pocket3d/examples/island/3ds", image, "make", "-j4", `FLAVOR=${flavor}`, `BUILD_ID=${buildId}`]);
  const rom = `${root}/dist/island/${flavor}/pocket-island.3dsx`;
  if (!existsSync(rom)) throw new Error("Build did not produce the 3DSX");
  console.log(`Pocket Island: ${rom}`);
  if (command === "run") await run(["open", "-a", process.env.AZAHAR ?? "/Applications/Azahar.app", "--args", rom]);
} else if (command === "e2e") {
  await run(["bun", `${app}/scripts/azahar.ts`]);
} else {
  throw new Error("Usage: bun tools/island.ts [assets|test|build|capture|run|e2e]");
}
