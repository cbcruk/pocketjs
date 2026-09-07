// Real PICA render-target readbacks, using an isolated emulator SD/config.
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { resolve } from "node:path";
import { encodePNG } from "../../../../../tests/png.ts";
const root = resolve(import.meta.dir, "../../../../..");
const out = `${root}/dist/island/e2e`;
mkdirSync(out, { recursive: true });
const fixture = mkdtempSync(`${out}/run-`);
const user = `${fixture}/Library/Application Support/Azahar`;
const source = `${homedir()}/Library/Application Support/Azahar`;
const app = process.env.AZAHAR ?? "/Applications/Azahar.app";
const rom = `${root}/dist/island/capture/pocket-island.3dsx`;
if (!existsSync(rom)) throw new Error("Build the capture binary with bun tools/island.ts capture");
mkdirSync(`${user}/config`, { recursive: true });
for (const dir of ["nand", "sysdata"]) if (existsSync(`${source}/${dir}`)) cpSync(`${source}/${dir}`, `${user}/${dir}`, { recursive: true });
let config = readFileSync(`${source}/config/qt-config.ini`, "utf8");
for (const [key, value] of Object.entries({ graphics_api: process.env.ISLAND_GRAPHICS_API ?? "0", resolution_factor: "1", use_vsync: "false", frame_limit: "1000", use_disk_shader_cache: "false", check_for_update_on_start: "false" })) {
  const line = new RegExp(`^${key}=.*$`, "m");
  if (!line.test(config)) throw new Error(`Missing emulator setting: ${key}`);
  config = config.replace(line, `${key}=${value}`);
  const def = new RegExp(`^${key}\\\\default=.*$`, "m");
  config = def.test(config) ? config.replace(def, `${key}\\default=false`) : config.replace(line, `${key}=${value}\n${key}\\default=false`);
}
writeFileSync(`${user}/config/qt-config.ini`, config);
const captures = `${user}/sdmc/pocket-island`;
const launchRom = `${fixture}/pocket-island.3dsx`;
cpSync(rom, launchRom);
// Azahar exposes no user-directory flag on macOS. The child process receives
// its own HOME; the shell and the developer's emulator data are unchanged.
const launch = Bun.spawnSync(["open", "-n", "-a", app, "--env", `HOME=${fixture}`, "--stdout", `${fixture}/console.log`, "--stderr", `${fixture}/console.log`, "--args", launchRom]);
if (launch.exitCode) throw new Error(launch.stderr.toString());
console.log(`Azahar fixture: ${fixture}`);
function ownedPids(): number[] {
  return Bun.spawnSync(["ps", "-axo", "pid=,command="]).stdout.toString().split("\n").filter(line => line.includes(`${app}/Contents/MacOS/azahar`) && line.includes(launchRom)).map(line => Number(line.trim().split(/\s+/)[0]));
}
try {
  const deadline = Date.now() + Number(process.env.ISLAND_E2E_TIMEOUT_MS ?? 120000);
  while (!existsSync(`${captures}/done`)) {
    if (Date.now() > deadline) throw new Error(`Timed out; inspect ${fixture}/console.log and ${user}/log/azahar_log.txt`);
    if (Date.now() > deadline - Number(process.env.ISLAND_E2E_TIMEOUT_MS ?? 120000) + 10000 && ownedPids().length === 0) throw new Error(`Emulator exited before completion: ${fixture}`);
    await Bun.sleep(500);
  }
  const receipts = readFileSync(`${captures}/receipt.jsonl`, "utf8").trim().split("\n").map(line => JSON.parse(line));
  if (receipts.length !== 10) throw new Error("Missing capture receipts");
  const at = (frame: number) => receipts.find(r => r.frame === frame);
  if (at(31).x < 0.5 || at(61).action !== 2 || at(91).action !== 6 || at(121).messages !== 1 || at(181).action !== 4 || at(241).action !== 0 || at(301).expression !== 5) throw new Error(`Interaction receipt mismatch: ${JSON.stringify(receipts)}`);
  mkdirSync(`${out}/latest`, { recursive: true });
  for (const r of receipts) for (const [name, width] of [["top", 400], ["bottom", 320]] as const) {
    const bytes = readFileSync(`${captures}/${name}-${String(r.frame).padStart(3, "0")}.bgr`);
    if (bytes.length !== width * 240 * 3) throw new Error("Truncated GPU capture");
    const rgba = new Uint8Array(width * 240 * 4);
    for (let y = 0; y < 240; y++) for (let x = 0; x < width; x++) {
      const src = (x * 240 + 239 - y) * 3, dst = (y * width + x) * 4;
      rgba[dst] = bytes[src + 2]; rgba[dst + 1] = bytes[src + 1]; rgba[dst + 2] = bytes[src]; rgba[dst + 3] = 255;
    }
    writeFileSync(`${out}/latest/${name}-${String(r.frame).padStart(3, "0")}.png`, encodePNG(rgba, width, 240));
  }
  writeFileSync(`${out}/latest/receipt.json`, JSON.stringify({ fixture, renderer: process.env.ISLAND_GRAPHICS_API ?? "0", frames: receipts }, null, 2));
  console.log(`PASS: 10 paired GPU captures; movement, run, wave, sit, stand, expressions and chat. ${out}/latest`);
} finally {
  for (const pid of ownedPids()) { try { process.kill(pid, "SIGKILL"); } catch {} }
}
