import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { strict as assert } from "node:assert";
const assets = resolve(import.meta.dir, "../assets");
const manifest = JSON.parse(readFileSync(`${assets}/manifest.json`, "utf8"));
for (const stem of ["mira", "island"]) {
  const bytes = readFileSync(`${assets}/${stem}.glb`);
  assert.equal(bytes.toString("ascii", 0, 4), "glTF");
  assert.equal(bytes.readUInt32LE(4), 2);
  assert.equal(bytes.readUInt32LE(8), bytes.length);
  assert.equal(bytes.toString("ascii", 16, 20), "JSON");
  const gltf = JSON.parse(bytes.toString("utf8", 20, 20 + bytes.readUInt32LE(12)));
  assert(gltf.meshes.length > 0);
  assert(gltf.buffers.every((b: any) => !b.uri), "GLB must be self-contained");
  if (stem === "mira") {
    assert.equal(gltf.skins.length, 1);
    assert(gltf.skins[0].joints.length >= 20);
    assert.deepEqual(gltf.animations.map((a: any) => a.name).sort(), [...manifest.character_asset.clips].sort());
    for (const animation of gltf.animations) {
      assert(animation.channels.length > 0);
      for (const sampler of animation.samplers) {
        const time = gltf.accessors[sampler.input];
        assert(time.count >= 2 && time.max[0] > time.min[0], `${animation.name} needs a time span`);
      }
    }
  }
}
console.log("PASS: self-contained GLBs, skinned Mira, 18 authored action/expression clips");
