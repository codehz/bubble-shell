import { press, setApi } from "./runtime.js";

const canvas = document.querySelector("canvas");
const ctx = canvas.getContext("2d");
const decoder = new TextDecoder();
const encoder = new TextEncoder();

function fontOf(fontSize, weight) {
  return `${weight || 400} ${fontSize}px sans-serif`;
}

function lineHeightOf(fontSize, lineHeight) {
  return lineHeight > 0 ? lineHeight : fontSize * 1.2;
}

function wrapText(text, fontSize, weight, maxWidth) {
  ctx.font = fontOf(fontSize, weight);
  const lines = [];
  for (const paragraph of text.split("\n")) {
    if (maxWidth === null || maxWidth <= 0) {
      lines.push(paragraph);
      continue;
    }
    let line = "";
    for (const char of paragraph) {
      const next = line + char;
      if (line && ctx.measureText(next).width > maxWidth) {
        const space = line.lastIndexOf(" ");
        if (space > 0) {
          lines.push(line.slice(0, space));
          line = line.slice(space + 1) + char;
        } else {
          lines.push(line);
          line = char;
        }
      } else {
        line = next;
      }
    }
    lines.push(line);
  }
  return lines.length ? lines : [""];
}

const wasm = await WebAssembly.instantiateStreaming(fetch("./bubble.wasm"), {
  env: {
    host_measure(textPtr, textLen, fontSize, lineHeight, weight, mode, avail, outPtr) {
      const text = decoder.decode(new Uint8Array(memory().buffer, textPtr, textLen));
      const maxWidth = mode && avail > 0 ? avail : null;
      const lines = wrapText(text, fontSize, weight, maxWidth);
      let width = 0;
      ctx.font = fontOf(fontSize, weight);
      for (const line of lines) {
        width = Math.max(width, ctx.measureText(line).width);
      }
      if (maxWidth !== null) width = Math.min(width, maxWidth);
      const height = lines.length * lineHeightOf(fontSize, lineHeight);
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

async function checkBundle() {
  const manifest = new Uint8Array(await (await fetch("./bundle/manifest.json")).arrayBuffer());
  const main = new Uint8Array(await (await fetch("./bundle/main.js")).arrayBuffer());
  const files = packFile("main.js", main);
  const manifestPtr = copyBytes(manifest);
  const filesPtr = copyBytes(files);
  const failed = exports.validate(manifestPtr, manifest.length, filesPtr, files.length);
  exports.dealloc(manifestPtr, manifest.length);
  exports.dealloc(filesPtr, files.length);
  if (failed) throw new Error(error() || "bundle rejected");
}


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
      const h = f32();
      const fontSize = f32();
      const color = view.getUint32(offset, true);
      offset += 4;
      const lineHeight = f32();
      const weight = view.getUint16(offset, true);
      offset += 2;
      const align = view.getUint8(offset);
      offset += 1;
      const textLen = view.getUint32(offset, true);
      offset += 4;
      const text = decoder.decode(new Uint8Array(memory().buffer, exports.scratch_ptr() + offset, textLen));
      offset += textLen;
      ctx.fillStyle = css(color);
      ctx.font = fontOf(fontSize, weight);
      ctx.textBaseline = "top";
      const lineH = lineHeightOf(fontSize, lineHeight);
      const lines = wrapText(text, fontSize, weight, w > 0 ? w : null);
      for (let index = 0; index < lines.length; index += 1) {
        const line = lines[index];
        const lineWidth = ctx.measureText(line).width;
        let lineX = x;
        if (align === 1) lineX = x + Math.max(0, (w - lineWidth) / 2);
        else if (align === 2) lineX = x + Math.max(0, w - lineWidth);
        if (index * lineH < h || h <= 0) ctx.fillText(line, lineX, y + index * lineH);
      }
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
    } else if (tag === 5) {
      const opacity = f32();
      ctx.save();
      ctx.globalAlpha *= opacity;
    } else if (tag === 6) {
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
  if (handler && press(handler, x, y)) paint();
});

canvas.addEventListener(
  "wheel",
  (event) => {
    const [x, y] = point(event);
    if (exports.scroll_wheel(x, y, event.deltaY)) {
      event.preventDefault();
      if (exports.layout() === 0) paint();
    }
  },
  { passive: false },
);

window.addEventListener("resize", resize);

const started = performance.now();
await checkBundle();
await import("./bundle/main.js");
paint();
const firstFrameMs = performance.now() - started;
console.info(`first frame ${firstFrameMs.toFixed(1)} ms`);
if (firstFrameMs >= 400) console.error("first frame exceeded 400 ms");
