import { readFileSync } from "node:fs";
import { press, setApi } from "./runtime.js";

const wasmBytes = readFileSync(new URL("./bubble.wasm", import.meta.url));
const manifest = readFileSync(new URL("./bundle/manifest.json", import.meta.url));
const main = readFileSync(new URL("./bundle/main.js", import.meta.url));
const decoder = new TextDecoder();

const started = performance.now();
const { instance } = await WebAssembly.instantiate(wasmBytes, {
  env: {
    host_measure(textPtr, textLen, fontSize, mode, avail, outPtr) {
      const text = decoder.decode(new Uint8Array(memory().buffer, textPtr, textLen));
      let width = text.length * fontSize * 0.5;
      let height = fontSize * 1.2;
      if (mode && avail > 0 && width > avail) {
        const lines = Math.max(1, Math.ceil(width / avail));
        width = avail;
        height = lines * height;
      }
      const out = new DataView(memory().buffer, outPtr, 8);
      out.setFloat32(0, width, true);
      out.setFloat32(4, height, true);
    },
  },
});
const exports = instance.exports;

function memory() {
  return exports.memory;
}

function error() {
  const len = exports.error_len();
  if (!len) return "";
  return decoder.decode(new Uint8Array(memory().buffer, exports.error_ptr(), len));
}

function scratch(len) {
  return new DataView(memory().buffer, exports.scratch_ptr(), len);
}

function copyBytes(bytes) {
  const ptr = exports.alloc(bytes.length);
  new Uint8Array(memory().buffer, ptr, bytes.length).set(bytes);
  return ptr;
}

function packFile(path, data) {
  const name = new TextEncoder().encode(path);
  const packed = new Uint8Array(8 + name.length + data.length);
  const view = new DataView(packed.buffer);
  view.setUint32(0, name.length, true);
  packed.set(name, 4);
  view.setUint32(4 + name.length, data.length, true);
  packed.set(data, 8 + name.length);
  return packed;
}

const files = packFile("main.js", main);
const manifestPtr = copyBytes(manifest);
const filesPtr = copyBytes(files);
if (exports.validate(manifestPtr, manifest.length, filesPtr, files.length) !== 0) {
  throw new Error(error() || "bundle rejected");
}
exports.dealloc(manifestPtr, manifest.length);
exports.dealloc(filesPtr, files.length);

setApi({
  memory: exports.memory,
  alloc: exports.alloc,
  dealloc: exports.dealloc,
  boot: exports.boot,
  signal_new: exports.signal_new,
  signal_get: exports.signal_get,
  signal_set: exports.signal_set,
  track_begin: exports.track_begin,
  track_end: exports.track_end,
  spawn(parent, spec) {
    const ptr = copyBytes(spec);
    const handle = exports.spawn(parent, ptr, spec.length);
    exports.dealloc(ptr, spec.length);
    return handle;
  },
  spawn_at(parent, index, spec) {
    const ptr = copyBytes(spec);
    const handle = exports.spawn_at(parent, index, ptr, spec.length);
    exports.dealloc(ptr, spec.length);
    return handle;
  },
  dispose: exports.dispose,
  move_node: exports.move_node,
  bind_event: exports.bind_event,
  queue_text: exports.queue_text,
  commit: exports.commit,
  layout: exports.layout,
  error,
  prepare() {
    exports.set_viewport(320, 220);
  },
  takeDirty() {
    const len = exports.take_dirty();
    const view = scratch(len);
    const count = view.getUint32(0, true);
    const ids = [];
    for (let index = 0; index < count; index += 1) {
      ids.push(view.getUint32(4 + index * 4, true));
    }
    return ids;
  },
});

await import("./bundle/main.js");

function paintList() {
  const len = exports.paint();
  const view = scratch(len);
  let offset = 0;
  const count = view.getUint32(offset, true);
  offset += 4;
  const cmds = [];
  for (let index = 0; index < count; index += 1) {
    const tag = view.getUint8(offset);
    offset += 1;
    if (tag === 1) {
      const x = f32();
      const y = f32();
      const w = f32();
      const h = f32();
      const radius = f32();
      const color = view.getUint32(offset, true);
      offset += 4;
      cmds.push({ tag, x, y, w, h, radius, color });
    } else if (tag === 2) {
      const x = f32();
      const y = f32();
      const w = f32();
      const fontSize = f32();
      const color = view.getUint32(offset, true);
      offset += 4;
      const textLen = view.getUint32(offset, true);
      offset += 4;
      const text = decoder.decode(new Uint8Array(memory().buffer, exports.scratch_ptr() + offset, textLen));
      offset += textLen;
      cmds.push({ tag, x, y, w, fontSize, color, text });
    } else if (tag === 3) {
      const x = f32();
      const y = f32();
      const w = f32();
      const h = f32();
      cmds.push({ tag, x, y, w, h });
    } else if (tag === 4) {
      cmds.push({ tag });
    } else {
      throw new Error(`unknown paint tag ${tag}`);
    }
  }
  function f32() {
    const value = view.getFloat32(offset, true);
    offset += 4;
    return value;
  }
  return cmds;
}

function textOf(cmds, value) {
  const found = cmds.find((cmd) => cmd.tag === 2 && cmd.text === value);
  if (!found) throw new Error(`missing text ${value}`);
  return found;
}

const first = paintList();
const firstFrameMs = performance.now() - started;
textOf(first, "Count: 0");
const button = first.find((cmd) => cmd.tag === 1 && cmd.color === 0xff3b82f6);
if (!button) throw new Error("increment button was not painted");
const handler = exports.tap(button.x + button.w / 2, button.y + button.h / 2);
if (!handler || !press(handler)) throw new Error("click did not run a handler");
textOf(paintList(), "Count: 1");

const before = textOf(paintList(), "Alpha");
const scrolled = exports.scroll_wheel(before.x + 4, before.y + 4, 80);
if (!scrolled) throw new Error("wheel did not scroll");
if (exports.layout() !== 0) throw new Error(error() || "layout failed");
const after = textOf(paintList(), "Alpha");
if (!(after.y < before.y)) throw new Error(`Alpha did not move (${before.y} -> ${after.y})`);

const samples = [];
for (let frame = 0; frame < 60; frame += 1) {
  const mark = performance.now();
  if (exports.layout() !== 0) throw new Error(error() || "layout failed");
  exports.paint();
  samples.push(performance.now() - mark);
}
samples.sort((a, b) => a - b);
const p95 = samples[Math.ceil(samples.length * 0.95) - 1];
if (firstFrameMs >= 400) throw new Error(`first frame ${firstFrameMs.toFixed(1)} ms`);
if (p95 >= 16.7) throw new Error(`frame p95 ${p95.toFixed(2)} ms`);
console.log(`first frame ${firstFrameMs.toFixed(1)} ms, paint p95 ${p95.toFixed(2)} ms`);
