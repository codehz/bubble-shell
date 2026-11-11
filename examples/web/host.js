const canvas = document.querySelector("#c");
const ctx = canvas.getContext("2d");
const decoder = new TextDecoder();

let wasm;
let worker;
const queue = [];
let dirty = true;
let drag = null;

function pass(bytes) {
  const ptr = wasm.alloc(bytes.length);
  new Uint8Array(wasm.memory.buffer, ptr, bytes.length).set(bytes);
  return ptr;
}

function errorText() {
  const len = wasm.error_len();
  if (!len) return "unknown error";
  return decoder.decode(new Uint8Array(wasm.memory.buffer, wasm.error_ptr(), len));
}

function fileTable(files) {
  const encoder = new TextEncoder();
  const parts = [];
  for (const [path, data] of files) {
    const encoded = encoder.encode(path);
    const header = new ArrayBuffer(4);
    new DataView(header).setUint32(0, encoded.length, true);
    parts.push(new Uint8Array(header), encoded);
    const size = new ArrayBuffer(4);
    new DataView(size).setUint32(0, data.length, true);
    parts.push(new Uint8Array(size), data);
  }
  const total = parts.reduce((sum, part) => sum + part.length, 0);
  const out = new Uint8Array(total);
  let offset = 0;
  for (const part of parts) {
    out.set(part, offset);
    offset += part.length;
  }
  return out;
}

function cssColor(rgba) {
  const a = ((rgba >>> 24) & 255) / 255;
  const r = (rgba >>> 16) & 255;
  const g = (rgba >>> 8) & 255;
  const b = rgba & 255;
  return `rgba(${r}, ${g}, ${b}, ${a})`;
}

function paint() {
  const len = wasm.paint();
  const bytes = new Uint8Array(wasm.memory.buffer, wasm.scratch_ptr(), len).slice();
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const dpr = window.devicePixelRatio || 1;
  const width = canvas.clientWidth;
  const height = canvas.clientHeight;
  canvas.width = Math.max(1, Math.floor(width * dpr));
  canvas.height = Math.max(1, Math.floor(height * dpr));
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, width, height);
  let offset = 0;
  const count = view.getUint32(offset, true);
  offset += 4;
  for (let index = 0; index < count; index += 1) {
    const tag = view.getUint8(offset);
    offset += 1;
    if (tag === 1) {
      const x = view.getFloat32(offset, true); offset += 4;
      const y = view.getFloat32(offset, true); offset += 4;
      const w = view.getFloat32(offset, true); offset += 4;
      const h = view.getFloat32(offset, true); offset += 4;
      const radius = view.getFloat32(offset, true); offset += 4;
      const color = view.getUint32(offset, true); offset += 4;
      ctx.fillStyle = cssColor(color);
      ctx.beginPath();
      ctx.roundRect(x, y, w, h, radius);
      ctx.fill();
    } else if (tag === 2) {
      const x = view.getFloat32(offset, true); offset += 4;
      const y = view.getFloat32(offset, true); offset += 4;
      const fontSize = view.getFloat32(offset, true); offset += 4;
      const color = view.getUint32(offset, true); offset += 4;
      const textLen = view.getUint32(offset, true); offset += 4;
      const text = decoder.decode(bytes.subarray(offset, offset + textLen));
      offset += textLen;
      ctx.fillStyle = cssColor(color);
      ctx.font = `${fontSize}px sans-serif`;
      ctx.textBaseline = "top";
      ctx.fillText(text, x, y);
    } else if (tag === 3) {
      const x = view.getFloat32(offset, true); offset += 4;
      const y = view.getFloat32(offset, true); offset += 4;
      const w = view.getFloat32(offset, true); offset += 4;
      const h = view.getFloat32(offset, true); offset += 4;
      ctx.save();
      ctx.beginPath();
      ctx.rect(x, y, w, h);
      ctx.clip();
    } else if (tag === 4) {
      ctx.restore();
    }
  }
}

function showError(message) {
  const dpr = window.devicePixelRatio || 1;
  canvas.width = Math.max(1, Math.floor(canvas.clientWidth * dpr));
  canvas.height = Math.max(1, Math.floor(canvas.clientHeight * dpr));
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.fillStyle = "#111";
  ctx.fillRect(0, 0, canvas.clientWidth, canvas.clientHeight);
  ctx.fillStyle = "#f87171";
  ctx.font = "16px sans-serif";
  ctx.textBaseline = "top";
  ctx.fillText(message, 16, 16);
}

function resize() {
  wasm.set_viewport(canvas.clientWidth, canvas.clientHeight);
  dirty = true;
}

function frame() {
  if (dirty) {
    for (const buffer of queue) {
      const bytes = new Uint8Array(buffer);
      const ptr = pass(bytes);
      if (wasm.apply_ops(ptr, bytes.length) !== 0) showError(errorText());
      wasm.dealloc(ptr, bytes.length);
    }
    queue.length = 0;
    if (wasm.layout() !== 0) showError(errorText());
    else paint();
    dirty = false;
  }
  requestAnimationFrame(frame);
}

function localPoint(event) {
  const rect = canvas.getBoundingClientRect();
  return { x: event.clientX - rect.left, y: event.clientY - rect.top };
}

canvas.addEventListener("pointerdown", (event) => {
  canvas.setPointerCapture(event.pointerId);
  const point = localPoint(event);
  drag = { x: point.x, y: point.y, moved: false };
});

canvas.addEventListener("pointermove", (event) => {
  if (!drag) return;
  const point = localPoint(event);
  const dx = point.x - drag.x;
  const dy = point.y - drag.y;
  if (Math.hypot(dx, dy) > 4) drag.moved = true;
  if (dy !== 0 && wasm.scroll_wheel(point.x, point.y, -dy)) dirty = true;
  drag.x = point.x;
  drag.y = point.y;
});

canvas.addEventListener("pointerup", (event) => {
  const point = localPoint(event);
  if (drag && !drag.moved) {
    const handler = wasm.tap(point.x, point.y);
    if (handler) worker.postMessage({ type: "tap", handlerId: handler, handle: 0 });
  }
  drag = null;
});

canvas.addEventListener("wheel", (event) => {
  event.preventDefault();
  const point = localPoint(event);
  const scale = event.deltaMode === 1 ? 16 : event.deltaMode === 2 ? canvas.clientHeight : 1;
  if (wasm.scroll_wheel(point.x, point.y, event.deltaY * scale)) dirty = true;
}, { passive: false });

const wasmBytes = await fetch("./shell.wasm").then((response) => response.arrayBuffer());
const manifestBytes = await fetch("./bundle/manifest.json").then((response) => response.arrayBuffer());
const template = new Uint8Array(await fetch("./bundle/template.ir.bin").then((response) => response.arrayBuffer()));
const script = new Uint8Array(await fetch("./bundle/main.js").then((response) => response.arrayBuffer()));
const instantiated = await WebAssembly.instantiate(wasmBytes, {
  env: {
    host_measure(ptr, len, fontSize, mode, avail, outPtr) {
      const text = decoder.decode(new Uint8Array(wasm.memory.buffer, ptr, len));
      ctx.font = `${fontSize}px sans-serif`;
      const natural = ctx.measureText(text).width;
      const line = fontSize * 1.2;
      let width = natural;
      let height = line;
      if (mode && avail > 0 && natural > avail) {
        width = avail;
        height = Math.ceil(natural / avail) * line;
      }
      const floats = new Float32Array(wasm.memory.buffer);
      floats[outPtr >> 2] = width;
      floats[(outPtr >> 2) + 1] = height;
    },
  },
});
wasm = instantiated.instance.exports;

const table = fileTable([
  ["template.ir.bin", template],
  ["main.js", script],
]);
const manifestPtr = pass(new Uint8Array(manifestBytes));
const tablePtr = pass(table);
if (wasm.validate(manifestPtr, manifestBytes.byteLength, tablePtr, table.length) !== 0) {
  showError(errorText());
  throw new Error(errorText());
}
wasm.dealloc(manifestPtr, manifestBytes.byteLength);
wasm.dealloc(tablePtr, table.length);

const irPtr = pass(template);
if (wasm.mount(irPtr, template.length) !== 0) {
  showError(errorText());
  throw new Error(errorText());
}
wasm.dealloc(irPtr, template.length);
resize();
new ResizeObserver(resize).observe(canvas);

worker = new Worker("./worker.js");
worker.onmessage = (event) => {
  if (event.data.type === "ops") {
    queue.push(event.data.buffer);
    dirty = true;
  }
};
worker.postMessage({ type: "boot", wasm: wasmBytes, entry: new URL("./bundle/main.js", import.meta.url).href }, [wasmBytes]);
requestAnimationFrame(frame);
