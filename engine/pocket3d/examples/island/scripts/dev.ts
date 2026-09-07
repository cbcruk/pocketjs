import { existsSync, mkdirSync, readFileSync, readdirSync, watch, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { PocketRuntimeClient, discoverPocketRuntimes, parsePocketRuntimeToken } from "../../../../../tools/3ds-runtime-client.ts";
import { pocketRuntimeDeviceId } from "../../../../../contracts/spec/pocket-runtime-wire.ts";

const root = resolve(import.meta.dir, "../../../../..");
const args = process.argv.slice(2);
const command = args[0] ?? "probe";
const option = (key: string) => args[args.indexOf(key) + 1];
const value = (key: string, fallback: string) => args.includes(key) ? option(key) : fallback;
const keys = `${root}/.pocket/3ds/devices`;
const port = Number(value("--port", "8131"));
if (!Number.isInteger(port) || port < 1 || port > 65535) throw new Error("Invalid --port");
let host = value("--host", process.env.POCKET_3DS_HOST ?? "");
let token: Uint8Array | undefined;
if (args.includes("--key")) token = parsePocketRuntimeToken(readFileSync(option("--key"), "utf8"));
if (!token && host && existsSync(`${keys}/${host}-${port}.key`)) token = parsePocketRuntimeToken(readFileSync(`${keys}/${host}-${port}.key`, "utf8"));
if (!host || !token) {
  const devices = (await discoverPocketRuntimes({ port, addresses: host ? [host] : undefined })).filter(d => d.target === "p3d-island");
  const files = existsSync(keys) ? readdirSync(keys).filter(f => f.endsWith(".key")) : [];
  const matches = devices.flatMap(device => {
    const candidates = token ? [token] : files.map(file => parsePocketRuntimeToken(readFileSync(`${keys}/${file}`, "utf8")));
    const key = candidates.find(key => pocketRuntimeDeviceId(key) === device.deviceId);
    return key ? [{ device, key }] : [];
  });
  if (matches.length !== 1) throw new Error("Select a paired Pocket Island with --host and --key; start the app, not ftpd");
  host = matches[0].device.address;
  token = matches[0].key;
}
const client = new PocketRuntimeClient({ host, port, token, timeoutMs: 15000 });
let sequence = 0;
async function rpc(t: string, fields: Record<string, unknown> = {}, response = "island.reply") {
  const id = `island-${++sequence}`;
  const result = client.waitForCtrl(m => m.t === response && m.id === id);
  await client.sendCtrl({ t, id, ...fields });
  return await result;
}
function sourceHash(text: string) {
  let hash = 0xcbf29ce484222325n;
  for (const byte of new TextEncoder().encode(text)) hash = BigInt.asUintN(64, (hash ^ BigInt(byte)) * 0x100000001b3n);
  return hash.toString(16).padStart(16, "0");
}
async function push(file: string) {
  const source = readFileSync(file, "utf8");
  if (Buffer.byteLength(source) > 8192) throw new Error("Application JavaScript exceeds the 8192-byte native boundary");
  const result = await rpc("island.reload", { source });
  if (result.ok !== true) throw new Error(`Reload rejected; current script retained: ${result.message}`);
  if (result.scriptHash !== sourceHash(source)) throw new Error("Device accepted a different script hash");
  console.log(`JavaScript accepted: ${result.scriptHash}; generation ${result.generation}`);
}
try {
  const ack = await client.connect();
  const status = client.waitForCtrl(m => m.t === "runtime.status");
  await client.requestStatus();
  const target = await status;
  if (ack.hostAbi !== 0 || target.target !== "p3d-island") throw new Error("Connected runtime is not Pocket Island; refusing native commands");
  console.log(`Connected Pocket Island ${host}:${port}`);
  const source = resolve(value("--file", `${root}/engine/pocket3d/examples/island/app.js`));
  if (command === "push") await push(source);
  else if (command === "dev") {
    await push(source);
    console.log(`Watching ${source}; edits replace JavaScript without rebuilding the 3DSX`);
    let work = Promise.resolve();
    let timer: ReturnType<typeof setTimeout>;
    const watcher = watch(source, () => {
      clearTimeout(timer);
      timer = setTimeout(() => { work = work.then(() => push(source)).catch(error => console.error(String(error))); }, 200);
    });
    await new Promise<void>(resolve => process.once("SIGINT", resolve));
    watcher.close(); clearTimeout(timer!); await work;
  } else if (command === "crowd") {
    const out = resolve(value("--out", `${root}/dist/island/hardware/crowd-${Date.now()}`));
    mkdirSync(out, { recursive: true });
    const before = await rpc("island.stats", {}, "island.stats");
    if (!("benchmarkGeneration" in before)) throw new Error("Install the native crowd-probe build once before using this command");
    const windows = Number(value("--windows", "3"));
    const minFps = Number(value("--min-fps", "0"));
    const maxFrameMs = Number(value("--max-frame-ms", "0"));
    const panelMode = value("--panel", "on");
    const count = args.includes("--actors") ? Number(option("--actors")) : undefined;
    if (!Number.isInteger(windows) || windows < 3 || windows > 60 || !Number.isFinite(minFps) || minFps < 0 ||
        !Number.isFinite(maxFrameMs) || maxFrameMs < 0 || !["on", "off", "both"].includes(panelMode) ||
        (count !== undefined && (!Number.isInteger(count) || count < 1 || count > 8))) throw new Error("Invalid crowd measurement options");
    const configurations = count !== undefined ? [{ actors: count, motion: "walk", terrain: true }] : [
      { actors: 0, motion: "frozen", terrain: true },
      ...[1, 2, 3, 4, 6, 8].map(actors => ({ actors, motion: "walk", terrain: true })),
      ...[1, 2, 3, 4, 6, 8].map(actors => ({ actors, motion: "frozen", terrain: true })),
      ...[2, 4, 8].map(actors => ({ actors, motion: "frozen", terrain: false })),
    ];
    const cases = configurations.flatMap(config => (panelMode === "both" ? [false, true] : [panelMode === "on"])
      .map(panel => ({ ...config, panel })));
    const results: Record<string, any>[] = [];
    const persist = () => writeFileSync(`${out}/crowd.json`, JSON.stringify({ host, observedAt: new Date().toISOString(), input: "remote autonomous load test", criteria: { windows, minFps, maxFrameMs, panelMode }, before, results }, null, 2));
    try {
      for (const config of cases) {
        const name = `${config.actors}-${config.motion}-${config.terrain ? "island" : "avatars-only"}${config.panel ? "" : "-panel-off"}`;
        const accepted = await rpc("island.benchmark", { enabled: true, ...config });
        if (accepted.ok !== true) throw new Error(`Load test rejected: ${accepted.message}`);
        // Exclude configuration, first uploads, and the preceding GPU queue.
        await Bun.sleep(2300);
        const samples: Record<string, any>[] = [];
        const deadline = Date.now() + windows * 1200 + 10000;
        let reported = 0;
        while (samples.length < windows && Date.now() < deadline) {
          const stats = await rpc("island.stats", {}, "island.stats");
          if (stats.actorCount !== config.actors || stats.actorMotion !== config.motion || stats.terrainEnabled !== Number(config.terrain)) throw new Error("Load test changed while measuring");
          if (Number(stats.samples) > 0 && stats.panel === Number(config.panel) && stats.measuredBenchmarkGeneration === stats.benchmarkGeneration &&
              !samples.some(row => row.elapsedMs === stats.elapsedMs)) samples.push(stats);
          if (samples.length >= reported + 10) {
            reported = samples.length;
            console.log(`${name}: ${samples.length}/${windows} windows; latest ${Number(stats.fps).toFixed(2)} FPS`);
          }
          if (samples.length < windows) await Bun.sleep(450);
        }
        if (samples.length !== windows) throw new Error("Missing complete measurement windows");
        const frames = samples.reduce((sum, row) => sum + row.samples, 0);
        const average = (key: string) => samples.reduce((sum, row) => sum + row[key] * row.samples, 0) / frames;
        const result = { ...config, name, frames, fps: 1000 / average("frameMs"),
          updateSkinMs: average("updateSkinMs"), uploadMs: average("uploadMs"),
          drawUiMs: average("drawUiMs"), gpuMs: average("gpuPreviousMs"),
          triangles: (samples.at(-1)!.avatarVertices + samples.at(-1)!.terrainVertices) / 3,
          minWindowFps: Math.min(...samples.map(row => row.fps)),
          maxFrameMs: Math.max(...samples.map(row => row.maxMs)),
          maxP95Ms: Math.max(...samples.map(row => row.p95Ms)),
          passed: samples.every(row => (!minFps || row.fps >= minFps) && (!maxFrameMs || row.maxMs <= maxFrameMs)),
          samples };
        results.push(result); persist();
        console.log(`${name}: ${result.fps.toFixed(2)} FPS / skin ${result.updateSkinMs.toFixed(2)} ms / upload ${result.uploadMs.toFixed(2)} ms / GPU ${result.gpuMs.toFixed(2)} ms`);
        if (config.terrain && [2, 4, 8].includes(config.actors) && config.motion === "walk") {
          const shot = client.waitForScreenshot(30000);
          await client.sendCtrl({ t: "screenshot" });
          writeFileSync(`${out}/${name}.png`, (await shot).png);
        }
      }
    } finally {
      const stopped = await rpc("island.benchmark", { enabled: false });
      if (stopped.ok !== true) throw new Error("Could not stop load test");
      const after = await rpc("island.stats", {}, "island.stats");
      results.push({ restored: after }); persist();
    }
    console.log(`Saved crowd scaling and GPU isolation receipts: ${out}`);
    if (results.some(result => result.passed === false)) throw new Error("Crowd measurement did not meet the requested FPS/frame-time criteria; see saved receipts");
  } else if (command === "probe" || command === "bench") {
    const out = resolve(value("--out", `${root}/dist/island/hardware/${Date.now()}`));
    mkdirSync(out, { recursive: true });
    const samples: Record<string, unknown>[] = [];
    const sample = async () => {
      const stats = await rpc("island.stats", {}, "island.stats");
      samples.push({ host, observedAt: new Date().toISOString(), ...stats });
      return stats;
    };
    if (command === "bench") {
      // These are declared remote input receipts, not physical gesture proof.
      const phases = [
        { name: "idle", x: 0, z: 0, flags: 0 },
        { name: "walk", x: 1, z: 0, flags: 0 },
        { name: "run", x: -1, z: 0, flags: 1 },
        { name: "wave", x: 0, z: 0, flags: 2 },
        { name: "sit", x: 0, z: 0, flags: 4 },
        { name: "stand", x: 0, z: 0, flags: 4 },
      ];
      for (const phase of phases) {
        console.log(`Measuring remote-input phase: ${phase.name}`);
        const id = `tape-${++sequence}`;
        let finished = false;
        const done = client.waitForCtrl(m => m.t === "island.stats" && m.id === id, 90000);
        done.then(() => { finished = true; }, () => { finished = true; });
        const accepted = client.waitForCtrl(m => m.t === "island.reply" && m.id === id);
        await client.sendCtrl({ t: "island.input", id, ...phase, frames: 90 });
        if ((await accepted).ok !== true) throw new Error("Remote input tape rejected");
        while (!finished) {
          await sample();
          samples[samples.length - 1].phase = phase.name;
          // Persist partial evidence even if WiFi drops during the next phase.
          writeFileSync(`${out}/samples.json`, JSON.stringify({ host, input: "remote", samples }, null, 2));
          await Bun.sleep(1000);
        }
        await done;
      }
    }
    const stats = await sample();
    const screenshot = client.waitForScreenshot(30000);
    await client.sendCtrl({ t: "screenshot" });
    const shot = await screenshot;
    writeFileSync(`${out}/screen.png`, shot.png);
    writeFileSync(`${out}/stats.json`, JSON.stringify({ host, input: command === "bench" ? "remote" : "unchanged", screenshotFrame: shot.frame, stats, samples }, null, 2));
    console.log(JSON.stringify(stats, null, 2));
    console.log(`Saved paired GPU capture and runtime receipts: ${out}`);
  } else throw new Error("Usage: bun island [probe|push|dev|bench] [--host IP] [--key file] [--file app.js] [--out directory]");
} finally {
  client.close();
}
