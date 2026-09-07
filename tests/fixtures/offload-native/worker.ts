import { dispatchOffload } from "../../../tools/offload-provider.ts";
declare const self: { onmessage: (event: MessageEvent) => void; postMessage(value: unknown): void };
self.onmessage = async event => {
  if (event.data.init) return;
  self.postMessage(await dispatchOffload({
    "test.image": () => ({ width: 256, height: 256, pixels: Uint8Array.from({ length: 256 * 256 * 2 }, (_, n) => n & 255), format: "r5g6b5" }),
    "test.mesh": () => { const bytes=new Uint8Array(16);bytes.set([80,77,72,49,0,1,0,1]);return {format:"mesh2d-v1",bytes}; },
    "test.text": () => "network-ok",
  }, event.data));
};
