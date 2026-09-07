// Real PICA render-target readbacks, using an isolated emulator SD/config.
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { resolve } from "node:path";
import { encodePNG } from "../../../../../tests/png.ts";
import { PocketRuntimeClient } from "../../../../../tools/3ds-runtime-client.ts";
import { encodePocketRuntimePackageBegin, POCKET_RUNTIME_MSG } from "../../../../../contracts/spec/pocket-runtime-wire.ts";
const root = resolve(import.meta.dir, "../../../../..");
const out = `${root}/dist/island/e2e`;
mkdirSync(out, { recursive: true });
const fixture = mkdtempSync(`${out}/run-`);
const user = `${fixture}/Library/Application Support/Azahar`;
const source = `${homedir()}/Library/Application Support/Azahar`;
const app = process.env.AZAHAR ?? "/Applications/Azahar.app";
const live = process.env.ISLAND_LINK_E2E === "1";
const rom = `${root}/dist/island/${live ? "release" : "capture"}/pocket-island.3dsx`;
if (!existsSync(rom)) throw new Error("Build the capture binary with bun tools/island.ts capture");
mkdirSync(`${user}/config`, { recursive: true });
for (const dir of ["nand", "sysdata"]) if (existsSync(`${source}/${dir}`)) cpSync(`${source}/${dir}`, `${user}/${dir}`, { recursive: true });
let config = readFileSync(`${source}/config/qt-config.ini`, "utf8");
// Software captures provide the pixel reference. Live timing-window tests use
// OpenGL: software rasterization advances emulated time too slowly for deadlines.
for (const [key, value] of Object.entries({ graphics_api: process.env.ISLAND_GRAPHICS_API ?? (live ? "1" : "0"), resolution_factor: "1", use_vsync: "false", frame_limit: "1000", use_disk_shader_cache: "false", check_for_update_on_start: "false" })) {
  const line = new RegExp(`^${key}=.*$`, "m");
  if (!line.test(config)) throw new Error(`Missing emulator setting: ${key}`);
  config = config.replace(line, `${key}=${value}`);
  const def = new RegExp(`^${key}\\\\default=.*$`, "m");
  config = def.test(config) ? config.replace(def, `${key}\\default=false`) : config.replace(line, `${key}=${value}\n${key}\\default=false`);
}
writeFileSync(`${user}/config/qt-config.ini`, config);
const token = crypto.getRandomValues(new Uint8Array(32));
if (live) {
  mkdirSync(`${user}/sdmc/pocketjs/runtime`, { recursive: true });
  writeFileSync(`${user}/sdmc/pocketjs/runtime/dev.key`, Buffer.from(token).toString("hex") + "\n", { mode: 0o600 });
}
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
  if (live) {
    let client: PocketRuntimeClient | undefined;
    const deadline = Date.now() + 60000;
    while (!client && Date.now() < deadline) {
      const candidate = new PocketRuntimeClient({ host: "127.0.0.1", token, timeoutMs: 3000 });
      try { await candidate.connect(); client = candidate; }
      catch { candidate.close(); await Bun.sleep(500); }
    }
    if (!client) throw new Error(`Native dev link did not listen: ${fixture}`);
    try {
      let seq = 0;
      const request = async (t: string, fields: Record<string, unknown> = {}, response = "island.reply") => {
        const id = `test-${++seq}`;
        const reply = client!.waitForCtrl(m => m.t === response && m.id === id, 30000);
        await client!.sendCtrl({ t, id, ...fields });
        return await reply;
      };
      const initial = await request("island.stats", {}, "island.stats");
      if (initial.skinning !== "gpu-rigid-indexed" || initial.dynamicVertexUploadBytes !== 0) throw new Error("Release is not using resident GPU skinning");
      const packageVerdict = client.waitForCtrl(m => m.t === "runtime.install" && m.phase === "rejected");
      await client.sendFrame(POCKET_RUNTIME_MSG.packageBegin, encodePocketRuntimePackageBegin(100, 123n));
      await packageVerdict;
      const original = readFileSync(`${root}/engine/pocket3d/examples/island/app.js`, "utf8");
      const changed = original.replace("A little island, together.", "Linked without FTP.").replace("span: 7.2", "span: 7.8")
        .replace("yaw: 0", "yaw: 0.65").replace("tilt: 0", "tilt: 0.1");
      const reload = await request("island.reload", { source: changed });
      if (reload.ok !== true) throw new Error(`Valid reload rejected: ${JSON.stringify(reload)}`);
      const after = await request("island.stats", {}, "island.stats");
      if (Math.abs(Number(after.cameraSpan) - 7.8) > 0.001) throw new Error("Live camera edit was not applied");
      if (Math.abs(Number(after.cameraYaw) - .65) > .001 || Math.abs(Number(after.cameraTilt) - .1) > .001) throw new Error("Live orbit was not applied");
      if (after.title !== "Linked without FTP." || Number(after.tick) < Number(initial.tick) || after.x !== initial.x || after.z !== initial.z) throw new Error("JS hot replacement reset native state or retained old UI");
      const rejected = await request("island.reload", { source: "globalThis.islandApp = {" });
      if (rejected.ok !== false || rejected.scriptHash !== reload.scriptHash) throw new Error("Invalid replacement discarded the running script");
      const badCamera = await request("island.reload", { source: original.replace("span: 7.2", "span: -1") });
      if (badCamera.ok !== false || badCamera.scriptHash !== reload.scriptHash) throw new Error("Invalid camera discarded the running script");
      for (const [field, value] of [["yaw", 4], ["tilt", 1]] as const) {
        const badOrbit = await request("island.reload", { source: original.replace(`${field}: 0`, `${field}: ${value}`) });
        if (badOrbit.ok !== false || badOrbit.scriptHash !== reload.scriptHash) throw new Error(`Invalid ${field} discarded the running script`);
      }
      const loop = await request("island.reload", { source: "while(true) {}" });
      if (loop.ok !== false || loop.scriptHash !== reload.scriptHash) throw new Error("Unbounded script was accepted");
      await request("island.event", { event: "message", text: "Live TCP message" });
      const chat = await request("island.stats", {}, "island.stats");
      if (Number(chat.messages) <= Number(after.messages)) throw new Error("Remote JS message did not reach native chat");
      const tapeId = "move-tape";
      const moved = client.waitForCtrl(m => m.t === "island.stats" && m.id === tapeId, 60000);
      await client.sendCtrl({ t: "island.input", id: tapeId, x: 1, z: 0, frames: 15, flags: 1 });
      const motion = await moved;
      if (Number(motion.x) <= Number(chat.x)) throw new Error("Remote input did not move the character");
      const dx = Number(motion.x) - Number(chat.x), dz = Number(motion.z) - Number(chat.z);
      if (dz >= 0 || Math.abs(dz / dx + Math.tan(.65)) > .02) throw new Error("Movement did not follow the rotated screen basis");
      const shot = client.waitForScreenshot(90000);
      await client.sendCtrl({ t: "screenshot" });
      const screenshot = await shot;
      writeFileSync(`${fixture}/live-screen.png`, screenshot.png);
      await Bun.sleep(8000); // Let the bubble expire before the building/bench review.
      for (const yaw of [-.45, .45]) {
        const wide = await request("island.reload", { source: original.replace("span: 7.2", "span: 16").replace("targetHeight: 0.8", "targetHeight: 1.8").replace("yaw: 0", `yaw: ${yaw}`).replace("tilt: 0", "tilt: 0.2") });
        if (wide.ok !== true) throw new Error("Wide orbit rejected");
        await Bun.sleep(300);
        const wideShot = client.waitForScreenshot(90000);
        await client.sendCtrl({ t: "screenshot" });
        writeFileSync(`${fixture}/orbit-${yaw < 0 ? "left" : "right"}.png`, (await wideShot).png);
      }
      const restored = await request("island.reload", { source: original });
      if (restored.ok !== true || restored.scriptHash !== initial.scriptHash) throw new Error("Original script restoration failed");
      const restoredState = await request("island.stats", {}, "island.stats");
      if (Math.abs(Number(restoredState.cameraSpan) - 7.2) > 0.001) throw new Error("Camera did not restore with the script");
      if (restoredState.messages !== chat.messages || restoredState.x !== motion.x || Number(restoredState.tick) < Number(motion.tick)) throw new Error("Reload lost moved position, chat or simulation progress");
      const crowdStart = await request("island.benchmark", { enabled: true, actors: 2, motion: "walk", terrain: true });
      if (crowdStart.ok !== true) throw new Error("Two-avatar load test rejected");
      await Bun.sleep(1200);
      const crowdMoving = await request("island.stats", {}, "island.stats");
      const actors = crowdMoving.actors as {x:number,z:number,action:number,actionTime:number}[];
      if (crowdMoving.actorCount !== 2 || actors.length !== 2 || actors.some(a => a.action !== 1) || actors[0].actionTime === actors[1].actionTime) throw new Error("Crowd actors did not walk with independent phases");
      const crowdShot = client.waitForScreenshot(90000);
      await client.sendCtrl({ t: "screenshot" });
      writeFileSync(`${fixture}/crowd-screen.png`, (await crowdShot).png);
      const crowdInvalid = await request("island.benchmark", { enabled: true, actors: 9, motion: "walk", terrain: true });
      if (crowdInvalid.ok !== false) throw new Error("Unbounded actor count accepted");
      const invalidPanel = await request("island.benchmark", { enabled: true, actors: 2, motion: "walk", terrain: true, panel: "false" });
      if (invalidPanel.ok !== false) throw new Error("Invalid panel flag accepted");
      await request("island.benchmark", { enabled: true, actors: 8, motion: "frozen", terrain: true });
      await Bun.sleep(1200);
      const frozen = await request("island.stats", {}, "island.stats");
      await Bun.sleep(800);
      const frozenLater = await request("island.stats", {}, "island.stats");
      if (frozen.actorCount !== 8 || JSON.stringify(frozen.actors) !== JSON.stringify(frozenLater.actors)) throw new Error("Frozen actors changed or eight slots failed");
      await request("island.benchmark", { enabled: true, actors: 0, motion: "frozen", terrain: false });
      await Bun.sleep(1200);
      const empty = await request("island.stats", {}, "island.stats");
      if (empty.actorCount !== 0 || empty.terrainEnabled !== 0) throw new Error("Empty-scene probe failed");
      await request("island.benchmark", { enabled: false });
      const crowdRestored = await request("island.stats", {}, "island.stats");
      if (crowdRestored.benchmarkEnabled !== 0 || crowdRestored.x !== restoredState.x || crowdRestored.z !== restoredState.z || crowdRestored.messages !== restoredState.messages) throw new Error("Load test changed player state");
      writeFileSync(`${fixture}/live-receipt.json`, JSON.stringify({ environment: "Azahar", initial, after, rejected, badCamera, loop, chat, motion, restored, restoredState, screenshotFrame: screenshot.frame,
        crowdMoving, crowdInvalid, frozen, frozenLater, empty, crowdRestored }, null, 2));
      console.log(`PASS: authenticated TCP, JS replacement/rejection, live chat, remote movement and paired GPU screenshot. ${fixture}`);
    } finally { client.close(); }
    const crowd = Bun.spawn([process.execPath, `${root}/engine/pocket3d/examples/island/scripts/dev.ts`, "crowd",
      "--host", "127.0.0.1", "--key", `${user}/sdmc/pocketjs/runtime/dev.key`,
      "--actors", "2", "--windows", "3", "--panel", "both", "--out", `${fixture}/crowd-probe`],
      { stdout: "inherit", stderr: "inherit" });
    if (await crowd.exited !== 0) throw new Error("Crowd CLI did not collect panel-on/off windows and restore player state");
  } else {
  const deadline = Date.now() + Number(process.env.ISLAND_E2E_TIMEOUT_MS ?? 120000);
  while (!existsSync(`${captures}/done`)) {
    if (Date.now() > deadline) throw new Error(`Timed out; inspect ${fixture}/console.log and ${user}/log/azahar_log.txt`);
    if (Date.now() > deadline - Number(process.env.ISLAND_E2E_TIMEOUT_MS ?? 120000) + 10000 && ownedPids().length === 0) throw new Error(`Emulator exited before completion: ${fixture}`);
    await Bun.sleep(500);
  }
  const receipts = readFileSync(`${captures}/receipt.jsonl`, "utf8").trim().split("\n").map(line => JSON.parse(line));
  if (receipts.length !== 11) throw new Error("Missing capture receipts");
  const at = (frame: number) => receipts.find(r => r.frame === frame);
  if (at(31).x < 0.5 || at(61).action !== 2 || at(91).action !== 6 || at(121).messages !== 1 || at(181).action !== 4 || at(241).action !== 0 || at(301).expression !== 5) throw new Error(`Interaction receipt mismatch: ${JSON.stringify(receipts)}`);
  if (at(331).perf_panel !== 1) throw new Error("Performance chord did not open the native panel");
  const perf = readFileSync(`${captures}/perf.csv`, "utf8");
  if (!perf.includes("capture=1") || !perf.includes("update_skin_ms") || !perf.includes("gpu_previous_ms")) throw new Error("Missing performance export provenance or columns");
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
  writeFileSync(`${out}/latest/perf.csv`, perf);
  console.log(`PASS: 11 paired GPU captures; movement, run, wave, sit, stand, expressions, chat and performance panel/export. ${out}/latest`);
  }
} finally {
  for (const pid of ownedPids()) { try { process.kill(pid, "SIGKILL"); } catch {} }
}
