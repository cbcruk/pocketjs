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
