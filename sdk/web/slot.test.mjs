import { readFileSync } from "node:fs";
import { registerHooks } from "node:module";
import assert from "node:assert/strict";
import test from "node:test";
import { entryUrl, fixture, wasmBytes } from "./fixtures.mjs";
import { setApi } from "../../crates/native/js/runtime.js";
import { layoutSnapshot } from "./src/textlayout.js";

// Fixture bundles import the host modules by specifier; map them like the
// page's import map does.
registerHooks({
  resolve(specifier, context, next) {
    if (specifier === "bubble-shell" || specifier === "bubble:rt") {
      return {
        url: new URL("../../crates/native/js/runtime.js", import.meta.url).href,
        shortCircuit: true,
      };
    }
    return next(specifier, context);
  },
});

const wasm = wasmBytes();
const { manifest, entry } = fixture("slot");
const decoder = new TextDecoder();
const encoder = new TextEncoder();

const { instance } = await WebAssembly.instantiate(wasm, {
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
    host_gesture_events: (ptr, len) =>
      globalThis.__bubbleGesture?.(new Uint8Array(memory().buffer, ptr, len).slice()),
    host_edit_events: (ptr, len) =>
      globalThis.__bubbleEdit?.(new Uint8Array(memory().buffer, ptr, len).slice()),
    host_var_events: (ptr, len) =>
      globalThis.__bubbleVar?.(new Uint8Array(memory().buffer, ptr, len).slice()),
    host_ui_calls: (ptr, len) =>
      globalThis.__bubbleUiCalls?.(new Uint8Array(memory().buffer, ptr, len).slice()),
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

const files = packFile("slot.js", entry);
const manifestPtr = copyBytes(manifest);
const filesPtr = copyBytes(files);
if (exports.validate(manifestPtr, manifest.length, filesPtr, files.length) !== 0) {
  throw new Error(error() || "bundle rejected");
}
exports.dealloc(manifestPtr, manifest.length);
exports.dealloc(filesPtr, files.length);

setApi({
  boot: exports.boot,
  signal_alloc: exports.signal_alloc,
  signal_read: exports.signal_read,
  signal_notify: exports.signal_notify,
  alloc_node: exports.alloc_node,
  mount_node: exports.mount_node,
  dispose_node: exports.dispose_node,
  on_cleanup: exports.on_cleanup,
  run_init: exports.run_init,
  run_updates: exports.run_updates,
  spawn(parent, kind, text) {
    const bytes = encoder.encode(text ?? "");
    const ptr = bytes.length ? copyBytes(bytes) : 0;
    const handle = exports.spawn(parent, kind, ptr, bytes.length);
    if (bytes.length) exports.dealloc(ptr, bytes.length);
    return handle;
  },
  spawn_at(parent, index, kind, text) {
    const bytes = encoder.encode(text ?? "");
    const ptr = bytes.length ? copyBytes(bytes) : 0;
    const handle = exports.spawn_at(parent, index, kind, ptr, bytes.length);
    if (bytes.length) exports.dealloc(ptr, bytes.length);
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
  create_var(...args) {
    return exports.create_var(...args);
  },
  bind_var: exports.bind_var,
  dispose_var: exports.dispose_var,
  spawn_style_decl: exports.spawn_style_decl,
  set_style_prop(decl, key, tag, number, text) {
    const keyBytes = encoder.encode(key);
    const textBytes = tag === 4 ? encoder.encode(text) : new Uint8Array(0);
    const keyPtr = copyBytes(keyBytes);
    const textPtr = textBytes.length ? copyBytes(textBytes) : 0;
    const code = exports.set_style_prop(decl, keyPtr, keyBytes.length, tag, number, textPtr, textBytes.length);
    exports.dealloc(keyPtr, keyBytes.length);
    if (textBytes.length) exports.dealloc(textPtr, textBytes.length);
    return code;
  },
  set_style_prop_program(decl, key, bytes) {
    const keyBytes = encoder.encode(key);
    const data = Uint8Array.from(bytes);
    const keyPtr = copyBytes(keyBytes);
    const dataPtr = copyBytes(data);
    const code = exports.set_style_prop_program(decl, keyPtr, keyBytes.length, dataPtr, data.length);
    exports.dealloc(keyPtr, keyBytes.length);
    exports.dealloc(dataPtr, data.length);
    return code;
  },
  move_style_decl: exports.move_style_decl,
  dispose_style_decl: exports.dispose_style_decl,
  pointer: exports.pointer,
  tick: exports.tick,
  queue_text(handle, text) {
    const bytes = encoder.encode(text ?? "");
    const ptr = bytes.length ? copyBytes(bytes) : 0;
    const code = exports.queue_text(handle, ptr, bytes.length);
    if (bytes.length) exports.dealloc(ptr, bytes.length);
    return code;
  },
  commit: exports.commit,
  layout: exports.layout,
  error,
  prepare() {
    exports.set_viewport(320, 480);
  },
});

await import(entryUrl("slot"));

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
      const radius = f32();
      cmds.push({ tag, x, y, w, h, radius });
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

const cmds = paintList();
const texts = cmds.filter((cmd) => cmd.tag === 2);
// Card background is #1b1e27; three Card instances paint three rects.
const cards = cmds
  .filter((cmd) => cmd.tag === 1 && argb(cmd.color) === 0xff1b1e27)
  .sort((a, b) => a.y - b.y);

function cardOf(textCmd, index) {
  const card = cards[index];
  return textCmd.y >= card.y && textCmd.y < card.y + card.h;
}

function textsOf(value) {
  return texts.filter((cmd) => cmd.text === value);
}

test("children render inside the component's slot, not at the call site", () => {
  // Card 1: header slot thunk + forwarded children + tail.
  const head = textsOf("head");
  const body = textsOf("body");
  const tails = textsOf("tail");
  assert.equal(head.length, 1, "header thunk renders exactly once");
  assert.equal(body.length, 1, "forwarded children render exactly once");
  assert.equal(tails.length, 3, "each Card renders its own tail");
  assert.ok(cardOf(head[0], 0), "header lands in card 1");
  assert.ok(cardOf(body[0], 0), "children land in card 1, not after all cards");
  assert.ok(head[0].y < body[0].y, "slot sits before children in authored order");
  assert.ok(body[0].y < tails[0].y, "children render before the card's own tail");
});

test("named slot fallback covers a missing thunk", () => {
  // Card 2 and Panel's Card both omit the header prop.
  const fallbacks = textsOf("no header");
  assert.equal(fallbacks.length, 2);
  assert.ok(cardOf(fallbacks[0], 1), "fallback renders in card 2's slot position");
  assert.ok(cardOf(fallbacks[1], 2), "fallback renders in card 3's slot position");
});

test("control flow inside forwarded children keys and renders per item", () => {
  const one = textsOf("item one");
  const two = textsOf("item two");
  assert.equal(one.length, 1);
  assert.equal(two.length, 1);
  assert.ok(cardOf(one[0], 1) && cardOf(two[0], 1));
  assert.ok(one[0].y < two[0].y);
});

test("forwarding chains keep passing children through", () => {
  const chained = textsOf("chained");
  assert.equal(chained.length, 1);
  assert.ok(cardOf(chained[0], 2), "Panel's children land inside Card's slot");
  assert.ok(chained[0].y < textsOf("tail")[2].y);
});

test("slot thunks never leak as stringified text", () => {
  for (const cmd of texts) {
    assert.ok(!cmd.text.includes("undefined"), `leaked: ${cmd.text}`);
    assert.ok(!cmd.text.includes("=>"), `leaked function text: ${cmd.text}`);
  }
});
