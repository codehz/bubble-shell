import { readFileSync } from "node:fs";
import { registerHooks } from "node:module";
import { dispatchGestureEvents, setApi } from "./runtime.js";
import { layoutSnapshot } from "../../sdk/web/textlayout.js";

// Fixture bundles import the host modules by specifier; map them like the
// page's import map does.
registerHooks({
  resolve(specifier, context, next) {
    if (specifier === "bubble-shell" || specifier === "bubble:rt") {
      return {
        url: new URL("./runtime.js", import.meta.url).href,
        shortCircuit: true,
      };
    }
    return next(specifier, context);
  },
});

const fixture = process.argv[2] ?? "counter";
const wasmBytes = readFileSync(new URL("./bubble.wasm", import.meta.url));
const manifest = readFileSync(new URL(`./fixtures/${fixture}.json`, import.meta.url));
const main = readFileSync(new URL(`./fixtures/${fixture}.js`, import.meta.url));
const decoder = new TextDecoder();
const encoder = new TextEncoder();

const started = performance.now();
const { instance } = await WebAssembly.instantiate(wasmBytes, {
  env: {
    host_measure(textPtr, textLen, fontSize, lineHeight, weight, mode, avail, outPtr) {
      const text = decoder.decode(new Uint8Array(memory().buffer, textPtr, textLen));
      const line = lineHeight > 0 ? lineHeight : fontSize * 1.2;
      const width = text.length * fontSize * 0.5;
      const lines = mode && avail > 0 && width > avail
        ? Math.max(1, Math.ceil(width / avail))
        : Math.max(1, text.split("\n").length);
      const out = new DataView(memory().buffer, outPtr, 8);
      out.setFloat32(0, mode && avail > 0 ? Math.min(width, avail) : width, true);
      out.setFloat32(4, lines * line, true);
    },
    host_layout(textPtr, textLen, width, fontSize, lineHeight, weight, align, density, revisionLo, revisionHi, outPtr, outCap) {
      const text = decoder.decode(new Uint8Array(memory().buffer, textPtr, textLen));
      const laid = layoutSnapshot({
        text,
        width,
        fontSize,
        lineHeight,
        weight,
        align,
        density,
        revision: revisionLo + revisionHi * 2 ** 32,
        measure: (line) => line.length * fontSize * 0.5,
      });
      if (laid.bytes.length > outCap) return laid.bytes.length;
      new Uint8Array(memory().buffer, outPtr, laid.bytes.length).set(laid.bytes);
      return laid.bytes.length;
    },
    host_run: (id) => globalThis.__bubbleRun?.(id) ?? 0,
    host_cleanup: (id) => globalThis.__bubbleCleanup?.(id),
    host_body: () => globalThis.__bubbleBody?.(),
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

const files = packFile(`${fixture}.js`, main);
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
  spawn_props() {
    const len = exports.spawn_props();
    return decoder
      .decode(new Uint8Array(memory().buffer, exports.scratch_ptr(), len))
      .split("\n");
  },
  deferred_props() {
    const len = exports.deferred_props();
    return decoder
      .decode(new Uint8Array(memory().buffer, exports.scratch_ptr(), len))
      .split("\n");
  },
  signal_alloc: exports.signal_alloc,
  signal_read: exports.signal_read,
  signal_notify: exports.signal_notify,
  alloc_node: exports.alloc_node,
  mount_node: exports.mount_node,
  dispose_node: exports.dispose_node,
  on_cleanup: exports.on_cleanup,
  run_init: exports.run_init,
  run_updates: exports.run_updates,
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
  set_prop(handle, key, tag, number, text) {
    const keyBytes = encoder.encode(key);
    const textBytes = tag === 4 ? encoder.encode(text) : new Uint8Array(0);
    const keyPtr = copyBytes(keyBytes);
    const textPtr = textBytes.length ? copyBytes(textBytes) : 0;
    const code = exports.set_prop(handle, keyPtr, keyBytes.length, tag, number, textPtr, textBytes.length);
    exports.dealloc(keyPtr, keyBytes.length);
    if (textBytes.length) exports.dealloc(textPtr, textBytes.length);
    return code;
  },
  bind_event: exports.bind_event,
  spawn_gesture: exports.spawn_gesture,
  set_gesture_prop(handle, key, tag, number, text) {
    const keyBytes = encoder.encode(key);
    const textBytes = tag === 4 ? encoder.encode(text) : new Uint8Array(0);
    const keyPtr = copyBytes(keyBytes);
    const textPtr = textBytes.length ? copyBytes(textBytes) : 0;
    const code = exports.set_gesture_prop(handle, keyPtr, keyBytes.length, tag, number, textPtr, textBytes.length);
    exports.dealloc(keyPtr, keyBytes.length);
    if (textBytes.length) exports.dealloc(textPtr, textBytes.length);
    return code;
  },
  bind_gesture_event: exports.bind_gesture_event,
  dispose_gesture: exports.dispose_gesture,
  move_gesture: exports.move_gesture,
  pointer: exports.pointer,
  tick: exports.tick,
  take_gesture_events: exports.take_gesture_events,
  queue_text: exports.queue_text,
  commit: exports.commit,
  layout: exports.layout,
  error,
  prepare() {
    exports.set_viewport(320, 220);
  },

});

await import(new URL(`./fixtures/${fixture}.js`, import.meta.url));

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
      const color = readColor();
      cmds.push({ tag, x, y, w, h, radius, color });
    } else if (tag === 2) {
      const x = f32();
      const y = f32();
      const w = f32();
      const h = f32();
      const fontSize = f32();
      const color = readColor();
      const lineHeight = f32();
      const weight = view.getUint16(offset, true);
      offset += 2;
      const align = view.getUint8(offset);
      offset += 1;
      const textLen = view.getUint32(offset, true);
      offset += 4;
      const text = decoder.decode(new Uint8Array(memory().buffer, exports.scratch_ptr() + offset, textLen));
      offset += textLen;
      cmds.push({ tag, x, y, w, h, fontSize, color, lineHeight, weight, align, text });
    } else if (tag === 3) {
      const x = f32();
      const y = f32();
      const w = f32();
      const h = f32();
      cmds.push({ tag, x, y, w, h });
    } else if (tag === 7) {
      const x = f32();
      const y = f32();
      const w = f32();
      const h = f32();
      const radius = f32();
      const fit = view.getUint8(offset);
      offset += 1;
      const srcLen = view.getUint32(offset, true);
      offset += 4;
      const src = decoder.decode(new Uint8Array(memory().buffer, exports.scratch_ptr() + offset, srcLen));
      offset += srcLen;
      cmds.push({ tag, x, y, w, h, radius, fit, src });
    } else if (tag === 4) {
      cmds.push({ tag });
    } else if (tag === 5) {
      const opacity = f32();
      cmds.push({ tag, opacity });
    } else if (tag === 6) {
      cmds.push({ tag });
    } else {
      throw new Error(`unknown paint tag ${tag}`);
    }
  }
  function readColor() {
    return { r: f32(), g: f32(), b: f32(), a: f32() };
  }
  function f32() {
    const value = view.getFloat32(offset, true);
    offset += 4;
    return value;
  }
  return cmds;
}

function argb(color) {
  const channel = (value) => Math.round(Math.min(1, Math.max(0, value)) * 255);
  return (((channel(color.a) << 24) | (channel(color.r) << 16) | (channel(color.g) << 8) | channel(color.b)) >>> 0);
}
function textOf(cmds, value) {
  const found = cmds.find((cmd) => cmd.tag === 2 && cmd.text === value);
  if (!found) throw new Error(`missing text ${value}`);
  return found;
}

const first = paintList();
const firstFrameMs = performance.now() - started;

function pumpGestures() {
  let len = exports.take_gesture_events();
  while (len > 0) {
    dispatchGestureEvents(new Uint8Array(memory().buffer, exports.scratch_ptr(), len).slice());
    len = exports.take_gesture_events();
  }
}

function sendPointer(action, id, x, y, time) {
  if (exports.pointer(action, id, x, y, time, 0, action === 2 || action === 3 ? 0 : 1) !== 0) {
    throw new Error(error() || "pointer failed");
  }
  pumpGestures();
}

if (fixture === "counter") {
  const countText = textOf(first, "Count: 0");
  if (argb(countText.color) !== 0xffffffff) throw new Error("CSS string text color was not applied");
  const button = first.find((cmd) => cmd.tag === 1 && argb(cmd.color) === 0xff3b82f6);
  if (!button) throw new Error("increment button was not painted");
  const x = button.x + button.w / 2;
  const y = button.y + button.h / 2;
  sendPointer(0, 1, x, y, 0);
  sendPointer(2, 1, x, y, 20);
  textOf(paintList(), "Count: 1");

  const before = textOf(paintList(), "Item 1");
  const scrolled = exports.scroll_wheel(before.x + 4, before.y + 4, 0, 80);
  if (!scrolled) throw new Error("wheel did not scroll");
  if (exports.layout() !== 0) throw new Error(error() || "layout failed");
  const after = textOf(paintList(), "Item 1");
  if (!(after.y < before.y)) throw new Error(`Item 1 did not move (${before.y} -> ${after.y})`);
} else if (fixture === "gestures") {
  const pad = first.find((cmd) => cmd.tag === 1 && argb(cmd.color) === 0xff26364a);
  if (!pad) throw new Error("gesture pad was not painted");
  const px = pad.x + pad.w / 2;
  const py = pad.y + pad.h / 2;
  sendPointer(0, 2, px, py, 100);
  sendPointer(2, 2, px, py, 120);
  const status = paintList()
    .filter((cmd) => cmd.tag === 2)
    .map((cmd) => cmd.text)
    .find((text) => text.startsWith("{"));
  if (!status) throw new Error("gesture status was not painted");
  const events = JSON.parse(status).events ?? [];
  if (!events.some((event) => event.name === "tap" && event.type === "tap")) {
    throw new Error(`tap was not recorded: ${status}`);
  }
} else {
  throw new Error(`unknown fixture ${fixture}`);
}

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
console.log(`${fixture}: first frame ${firstFrameMs.toFixed(1)} ms, paint p95 ${p95.toFixed(2)} ms`);
