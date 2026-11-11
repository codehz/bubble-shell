import { press, setApi } from "./runtime.js";

const canvas = document.querySelector("canvas");
const ctx = canvas.getContext("2d");
const decoder = new TextDecoder();

const wasm = await WebAssembly.instantiateStreaming(fetch("./bubble.wasm"), {
  env: {
    host_measure(textPtr, textLen, fontSize, mode, avail, outPtr) {
      const text = decoder.decode(new Uint8Array(memory().buffer, textPtr, textLen));
      ctx.font = `${fontSize}px sans-serif`;
      let width = ctx.measureText(text).width;
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

const exports = wasm.instance.exports;

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
  spawn: exports.spawn,
  bind_event: exports.bind_event,
  queue_text: exports.queue_text,
  commit: exports.commit,
  layout: exports.layout,
  error,
  prepare() {
    const dpr = window.devicePixelRatio || 1;
    const width = canvas.clientWidth;
    const height = canvas.clientHeight;
    canvas.width = Math.max(1, Math.floor(width * dpr));
    canvas.height = Math.max(1, Math.floor(height * dpr));
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    exports.set_viewport(width, height);
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

function resize() {
  const dpr = window.devicePixelRatio || 1;
  const width = canvas.clientWidth;
  const height = canvas.clientHeight;
  canvas.width = Math.max(1, Math.floor(width * dpr));
  canvas.height = Math.max(1, Math.floor(height * dpr));
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  exports.set_viewport(width, height);
  if (exports.layout() === 0) paint();
}

function paint() {
  const len = exports.paint();
  const view = scratch(len);
  let offset = 0;
  const count = view.getUint32(offset, true);
  offset += 4;
  ctx.clearRect(0, 0, canvas.clientWidth, canvas.clientHeight);
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
      fill(x, y, w, h, radius, color);
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
      ctx.fillStyle = css(color);
      ctx.font = `${fontSize}px sans-serif`;
      ctx.textBaseline = "top";
      ctx.fillText(text, x, y, w);
    } else if (tag === 3) {
      const x = f32();
      const y = f32();
      const w = f32();
      const h = f32();
      ctx.save();
      ctx.beginPath();
      ctx.rect(x, y, w, h);
      ctx.clip();
    } else if (tag === 4) {
      ctx.restore();
    }
  }

  function f32() {
    const value = view.getFloat32(offset, true);
    offset += 4;
    return value;
  }
}

function fill(x, y, w, h, radius, color) {
  ctx.beginPath();
  ctx.roundRect(x, y, w, h, radius);
  ctx.fillStyle = css(color);
  ctx.fill();
}

function css(color) {
  const a = (color >>> 24) & 255;
  const r = (color >>> 16) & 255;
  const g = (color >>> 8) & 255;
  const b = color & 255;
  return `rgba(${r},${g},${b},${(a / 255).toFixed(3)})`;
}

function point(event) {
  const rect = canvas.getBoundingClientRect();
  return [event.clientX - rect.left, event.clientY - rect.top];
}

canvas.addEventListener("pointerdown", (event) => {
  const [x, y] = point(event);
  const handler = exports.tap(x, y);
  if (handler && press(handler)) paint();
});

canvas.addEventListener(
  "wheel",
  (event) => {
    const [x, y] = point(event);
    if (exports.scroll_wheel(x, y, event.deltaY)) {
      event.preventDefault();
      paint();
    }
  },
  { passive: false },
);

window.addEventListener("resize", resize);

await import("./bundle/main.js");
paint();
