import { scrollActionForKey } from "./a11y.js";
import { commandsForDom, dragSelection } from "./ime.js";
import { dispatchEditEvents, dispatchGestureEvents, setApi } from "bubble-shell";
import { layoutSnapshot, wrapLines } from "./textlayout.js";

const dev = window.__bubbleDev ?? null;
const canvas = document.querySelector("canvas");
const ctx = canvas.getContext("2d");
const decoder = new TextDecoder();
const encoder = new TextEncoder();
const images = new Map();

canvas.tabIndex = 0;
canvas.setAttribute("role", "application");
canvas.setAttribute("aria-label", "Bubble canvas");

const a11yRoot = document.createElement("div");
Object.assign(a11yRoot.style, {
  position: "fixed",
  left: "0",
  top: "0",
  width: "100%",
  height: "100%",
  pointerEvents: "none",
  zIndex: "1",
  overflow: "hidden",
});
document.body.append(a11yRoot);
const a11yElements = new Map();
const a11yNodes = new Map();

const A11Y_ROLES = {
  generic: "group",
  text: "paragraph",
  button: "button",
  link: "link",
  image: "img",
  textbox: "textbox",
  searchbox: "searchbox",
  checkbox: "checkbox",
  radio: "radio",
  switch: "switch",
  heading: "heading",
  list: "list",
  listitem: "listitem",
  group: "group",
  dialog: "dialog",
  tab: "tab",
  menuitem: "menuitem",
  progressbar: "progressbar",
  slider: "slider",
};

function a11yAction(handle, action, text = "") {
  const bytes = encoder.encode(text);
  const ptr = bytes.length ? copyBytes(bytes) : 0;
  const result = exports.a11y_action(handle, action, ptr, bytes.length);
  if (bytes.length) exports.dealloc(ptr, bytes.length);
  pumpGestures();
  if (exports.layout() === 0) paint();
  return result === 0;
}

function syncA11y() {
  const len = exports.semantics();
  if (!len) return;
  let data;
  try {
    data = JSON.parse(
      decoder.decode(new Uint8Array(memory().buffer, exports.scratch_ptr(), len)),
    );
  } catch {
    return;
  }
  const rect = canvas.getBoundingClientRect();
  const hadA11yFocus = a11yRoot.contains(document.activeElement);
  a11yNodes.clear();
  for (const node of data.nodes) a11yNodes.set(node.handle, node);
  const seen = new Set();
  for (const node of data.nodes) {
    seen.add(node.handle);
    let element = a11yElements.get(node.handle);
    if (!element) {
      element = document.createElement("div");
      element.dataset.handle = String(node.handle);
      element.addEventListener("focus", () => {
        const current = a11yNodes.get(node.handle);
        if (current && !current.hidden && !current.disabled) {
          if (current.focusable) a11yAction(node.handle, 0);
          else {
            exports.key(4, performance.now());
            if (exports.layout() === 0) paint();
          }
        }
      });
      element.addEventListener("click", () => {
        const current = a11yNodes.get(node.handle);
        if (!current || current.hidden || current.disabled) return;
        if (a11yAction(node.handle, 1) && current.editable) placeCaret(node.handle);
      });
      // Attach after all elements exist, following the semantic parent chain.
      a11yElements.set(node.handle, element);
    }
    const role = node.handle === data.modal && node.role === "generic"
      ? "dialog"
      : A11Y_ROLES[node.role] || "group";
    if (element.getAttribute("role") !== role) element.setAttribute("role", role);
    const [x, y, w, h] = node.bounds;
    Object.assign(element.style, {
      position: "fixed",
      left: `${rect.left + x}px`,
      top: `${rect.top + y}px`,
      width: `${Math.max(0, w)}px`,
      height: `${Math.max(0, h)}px`,
      color: "transparent",
      fontSize: "1px",
      overflow: "hidden",
    });
    const label = node.role === "textbox" || node.role === "searchbox"
      ? [node.label, node.value].filter(Boolean).join(": ")
      : node.label || node.value;
    if (label) element.setAttribute("aria-label", label);
    else element.removeAttribute("aria-label");
    element.tabIndex = -1;
    element.setAttribute("aria-hidden", node.hidden ? "true" : "false");
    element.hidden = node.hidden;
    if (node.handle === data.modal) element.setAttribute("aria-modal", "true");
    else element.removeAttribute("aria-modal");
    if (node.disabled) element.setAttribute("aria-disabled", "true");
    else element.removeAttribute("aria-disabled");
    if (node.selected) element.setAttribute("aria-selected", "true");
    else element.removeAttribute("aria-selected");
    if (node.checked == null) element.removeAttribute("aria-checked");
    else element.setAttribute("aria-checked", node.checked ? "true" : "false");
    if (node.expanded == null) element.removeAttribute("aria-expanded");
    else element.setAttribute("aria-expanded", node.expanded ? "true" : "false");
    if (node.focused) element.setAttribute("data-focused", "true");
    else element.removeAttribute("data-focused");
    if (node.focusable) element.setAttribute("tabindex", "-1");
  }
  for (const node of data.nodes) {
    const element = a11yElements.get(node.handle);
    const parent = a11yElements.get(node.parent) || a11yRoot;
    if (element.parentElement !== parent) parent.append(element);
  }
  for (const [handle, element] of a11yElements) {
    if (!seen.has(handle)) {
      element.remove();
      a11yElements.delete(handle);
    }
  }
  if ((hadA11yFocus && (!a11yRoot.contains(document.activeElement) || document.activeElement.hidden))
      || (document.activeElement === ime && !a11yNodes.get(data.focused)?.editable)) {
    canvas.focus({ preventScroll: true });
  }
}

function drawSelectionHandles() {
  const len = exports.selection_handles();
  if (len !== 32) return;
  const view = new DataView(memory().buffer, exports.scratch_ptr(), len);
  ctx.fillStyle = "rgba(77, 163, 255, 0.95)";
  ctx.strokeStyle = "rgba(255, 255, 255, 0.95)";
  ctx.lineWidth = 2;
  for (let index = 0; index < 2; index += 1) {
    const x = view.getFloat32(index * 16, true);
    const y = view.getFloat32(index * 16 + 4, true);
    const w = view.getFloat32(index * 16 + 8, true);
    const h = view.getFloat32(index * 16 + 12, true);
    ctx.beginPath();
    ctx.arc(x + w / 2, y + h / 2, 7, 0, Math.PI * 2);
    ctx.fill();
    ctx.stroke();
  }
}

function selectionHandleHit(x, y) {
  const len = exports.selection_handles();
  if (len !== 32) return null;
  const view = new DataView(memory().buffer, exports.scratch_ptr(), len);
  for (let index = 0; index < 2; index += 1) {
    const cx = view.getFloat32(index * 16, true) + view.getFloat32(index * 16 + 8, true) / 2;
    const cy = view.getFloat32(index * 16 + 4, true) + view.getFloat32(index * 16 + 12, true) / 2;
    if (Math.hypot(x - cx, y - cy) <= 16) return index;
  }
  return null;
}

const KEY_ACTIONS = {
  ArrowLeft: 5,
  ArrowRight: 6,
  Home: 7,
  End: 8,
};

document.addEventListener(
  "keydown",
  (event) => {
    const element = event.target;
    const node = a11yRoot.contains(element)
      ? a11yNodes.get(Number(element.dataset.handle))
      : null;
    if (element !== canvas && element !== ime && !node) return;
    if (event.isComposing) return;
    if (node) {
      if (node.hidden || node.disabled) return;
      const scroll = scrollActionForKey(event, node, a11yNodes);
      if (scroll !== undefined) {
        if (a11yAction(node.handle, scroll)) event.preventDefault();
        return;
      }
    }
    let action;
    if (event.key === "Tab") action = event.shiftKey ? 1 : 0;
    else if (event.key === "Enter") action = 2;
    else if (event.key === " " || event.key === "Spacebar") action = 3;
    else if (event.key === "Escape") action = 4;
    else action = KEY_ACTIONS[event.key];
    if (action === undefined) return;
    if (event.target === ime && action !== 0 && action !== 1 && action !== 4) return;
    const handled = exports.key(action, performance.now());
    if (handled < 0) return;
    pumpGestures();
    if (exports.layout() === 0) paint();
    if (handled > 0) {
      event.preventDefault();
      const handle = exports.focused();
      const focused = a11yNodes.get(handle);
      if (focused?.editable) placeCaret(handle);
      else if (handle) a11yElements.get(handle)?.focus({ preventScroll: true });
      else if (!handle) {
        ime.blur();
        canvas.focus({ preventScroll: true });
      }
    }
  },
  true,
);

const bundleBase = new URL(dev?.base ?? "./fixtures/", import.meta.url);

function resolveSource(src) {
  if (/^[a-z][a-z0-9+.-]*:/i.test(src) || src.startsWith("//")) return src;
  return new URL(src, bundleBase).href;
}

function imageFor(source) {
  const src = resolveSource(source);
  let entry = images.get(src);
  if (!entry) {
    const img = new Image();
    entry = { img, ready: false };
    img.onload = () => {
      entry.ready = true;
      requestAnimationFrame(() => paint());
    };
    img.src = src;
    images.set(src, entry);
  }
  return entry;
}

function fontOf(fontSize, weight) {
  return `${weight || 400} ${fontSize}px sans-serif`;
}

function lineHeightOf(fontSize, lineHeight) {
  return lineHeight > 0 ? lineHeight : fontSize * 1.2;
}

function wrapText(text, fontSize, weight, maxWidth) {
  ctx.font = fontOf(fontSize, weight);
  return wrapLines(text, maxWidth, (line) => ctx.measureText(line).width).map((line) => line.text);
}

function hostLayout(textPtr, textLen, width, fontSize, lineHeight, weight, align, density, revisionLo, revisionHi, outPtr, outCap) {
  const text = decoder.decode(new Uint8Array(memory().buffer, textPtr, textLen));
  const revision = revisionLo + revisionHi * 2 ** 32;
  ctx.font = fontOf(fontSize, weight);
  const laid = layoutSnapshot({
    text,
    width,
    fontSize,
    lineHeight,
    weight,
    align,
    density,
    revision,
    font: fontOf(fontSize, weight),
    measure: (line) => ctx.measureText(line).width,
  });
  if (laid.bytes.length > outCap) return laid.bytes.length;
  new Uint8Array(memory().buffer, outPtr, laid.bytes.length).set(laid.bytes);
  return laid.bytes.length;
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
    host_layout: hostLayout,
    host_run: (id) => globalThis.__bubbleRun?.(id) ?? 0,
    host_cleanup: (id) => globalThis.__bubbleCleanup?.(id),
    host_body: () => globalThis.__bubbleBody?.(),
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

const requestedFixture = new URLSearchParams(location.search).get("fixture") ?? "counter";
const fixture = /^[a-z0-9-]+$/.test(requestedFixture) ? requestedFixture : "counter";

async function checkBundle() {
  const manifest = new Uint8Array(await (await fetch(`./fixtures/${fixture}.json`)).arrayBuffer());
  const main = new Uint8Array(await (await fetch(`./fixtures/${fixture}.js`)).arrayBuffer());
  const files = packFile(`${fixture}.js`, main);
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
  focus: exports.focus,
  set_editing_value(handle, epoch, value, start, end, cancel) {
    const bytes = encoder.encode(value ?? "");
    const ptr = bytes.length ? copyBytes(bytes) : 0;
    const code = exports.set_editing_value(handle, epoch, ptr, bytes.length, start, end, cancel);
    if (bytes.length) exports.dealloc(ptr, bytes.length);
    return code;
  },
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
  gesture_pending: exports.gesture_pending,
  take_gesture_events: exports.take_gesture_events,
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

// `bubble dev` remounts the tree outside this page's event loop.
globalThis.__bubbleRepaint = () => paint();

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
      const color = readColor();
      fill(x, y, w, h, radius, color);
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
      ctx.fillStyle = css(color);
      ctx.font = fontOf(fontSize, weight);
      ctx.textBaseline = "top";
      const lineH = lineHeightOf(fontSize, lineHeight);
      const laid = layoutSnapshot({
        text,
        width: w,
        fontSize,
        lineHeight,
        weight,
        align,
        density: 1,
        revision: 0,
        font: ctx.font,
        measure: (line) => ctx.measureText(line).width,
      });
      for (const line of laid.lines) {
        if (line.text && (index === 0 || line.y < h || h <= 0)) {
          ctx.fillText(line.text, x + (line.x || 0), y + line.y);
        }
      }
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
      const entry = imageFor(src);
      if (entry.ready) {
        const img = entry.img;
        const iw = img.naturalWidth;
        const ih = img.naturalHeight;
        let sx = 0;
        let sy = 0;
        let sw = iw;
        let sh = ih;
        let dx = x;
        let dy = y;
        let dw = w;
        let dh = h;
        if (fit === 0) {
          const scale = Math.min(w / iw, h / ih);
          dw = iw * scale;
          dh = ih * scale;
          dx = x + (w - dw) / 2;
          dy = y + (h - dh) / 2;
        } else if (fit === 1) {
          const scale = Math.max(w / iw, h / ih);
          sw = w / scale;
          sh = h / scale;
          sx = (iw - sw) / 2;
          sy = (ih - sh) / 2;
        }
        ctx.save();
        ctx.beginPath();
        ctx.roundRect(x, y, w, h, radius);
        ctx.clip();
        ctx.drawImage(img, sx, sy, sw, sh, dx, dy, dw, dh);
        ctx.restore();
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

  function readColor() {
    return { r: f32(), g: f32(), b: f32(), a: f32() };
  }
  drawSelectionHandles();
  syncA11y();
}

function fill(x, y, w, h, radius, color) {
  ctx.beginPath();
  ctx.roundRect(x, y, w, h, radius);
  ctx.fillStyle = css(color);
  ctx.fill();
}

const supportsColorFunction =
  typeof CSS !== "undefined" &&
  typeof CSS.supports === "function" &&
  CSS.supports("color", "color(srgb 0 0 0)");

function css(color) {
  if (supportsColorFunction) {
    return `color(srgb ${color.r} ${color.g} ${color.b} / ${color.a})`;
  }
  const channel = (value) => Math.round(Math.min(1, Math.max(0, value)) * 255);
  return `rgba(${channel(color.r)},${channel(color.g)},${channel(color.b)},${Math.min(1, Math.max(0, color.a)).toFixed(3)})`;
}

function point(event) {
  const rect = canvas.getBoundingClientRect();
  return [event.clientX - rect.left, event.clientY - rect.top];
}

const activePointers = new Set();
let ticking = false;

function pointerKind(event) {
  if (event.pointerType === "mouse") return 1;
  if (event.pointerType === "pen") return 2;
  return 0;
}

function pumpGestures() {
  let len = exports.take_gesture_events();
  while (len > 0) {
    const bytes = new Uint8Array(memory().buffer, exports.scratch_ptr(), len).slice();
    dispatchGestureEvents(bytes);
    len = exports.take_gesture_events();
  }
  len = exports.take_edit_events();
  while (len > 0) {
    const bytes = new Uint8Array(memory().buffer, exports.scratch_ptr(), len).slice();
    dispatchEditEvents(bytes);
    len = exports.take_edit_events();
  }
}

function gestureFrame() {
  exports.tick(performance.now());
  pumpGestures();
  if (exports.layout() === 0) paint();
  if (activePointers.size > 0 || exports.gesture_pending() !== 0) {
    requestAnimationFrame(gestureFrame);
  } else {
    ticking = false;
  }
}

function startTicking() {
  if (ticking) return;
  ticking = true;
  requestAnimationFrame(gestureFrame);
}

function sendPointer(action, event) {
  const [x, y] = point(event);
  exports.pointer(
    action,
    event.pointerId,
    x,
    y,
    performance.now(),
    pointerKind(event),
    event.buttons,
  );
  pumpGestures();
  if (exports.layout() === 0) paint();
}

let selectDrag = null;

canvas.addEventListener("pointerdown", (event) => {
  event.preventDefault();
  if (event.button !== 0) return;
  canvas.setPointerCapture(event.pointerId);
  editMenu.hidden = true;
  editMenu.style.display = "none";
  activePointers.add(event.pointerId);
  const [x, y] = point(event);
  if (event.button === 0 && exports.focused() && selectionHandleHit(x, y) !== null) {
    const which = selectionHandleHit(x, y);
    const len = exports.selection();
    const view = new DataView(memory().buffer, exports.scratch_ptr(), len);
    const start = view.getUint32(0, true);
    const end = view.getUint32(4, true);
    selectDrag = { id: event.pointerId, anchor: which === 0 ? end : start };
    return;
  }
  if (event.button === 0 && exports.field_at(x, y)) {
    const handle = exports.focus_at(x, y);
    let anchor = 0;
    if (exports.selection() >= 16) {
      const view = new DataView(memory().buffer, exports.scratch_ptr(), 16);
      anchor = view.getUint32(0, true);
    }
    selectDrag = { id: event.pointerId, anchor };
    if (handle) placeCaret(handle);
    return;
  }
  selectDrag = null;
  canvas.focus({ preventScroll: true });
  sendPointer(0, event);
  startTicking();
});

canvas.addEventListener("pointermove", (event) => {
  if (selectDrag && selectDrag.id === event.pointerId) {
    const [x, y] = point(event);
    const unit = exports.unit_at(x, y);
    const command = dragSelection(selectDrag.anchor, unit === 4294967295 ? -1 : unit);
    if (command) applyEditCommands([command]);
    return;
  }
  if (!activePointers.has(event.pointerId)) return;
  sendPointer(1, event);
});

function releasePointer(action, event) {
  if (selectDrag && selectDrag.id === event.pointerId) {
    activePointers.delete(event.pointerId);
    return;
  }
  if (!activePointers.has(event.pointerId)) return;
  sendPointer(action, event);
  activePointers.delete(event.pointerId);
}

canvas.addEventListener("pointerup", (event) => {
  if (event.button === 0) releasePointer(2, event);
});

const ime = document.createElement("textarea");
ime.setAttribute("aria-hidden", "true");
ime.autocomplete = "off";
ime.autocapitalize = "off";
ime.spellcheck = false;
Object.assign(ime.style, {
  position: "fixed",
  left: "0",
  top: "0",
  width: "1px",
  height: "1px",
  opacity: "0",
  pointerEvents: "none",
});
document.body.append(ime);

function editSnapshot() {
  const len = exports.edit_snapshot();
  if (len < 36) return null;
  const view = new DataView(memory().buffer, exports.scratch_ptr(), len);
  return {
    session: view.getBigUint64(0, true),
    revision: view.getBigUint64(8, true),
    anchor: view.getUint32(16, true),
    active: view.getUint32(20, true),
    compositionStart: view.getUint32(24, true),
    compositionEnd: view.getUint32(28, true),
    composing: view.getUint32(32, true) !== 0,
  };
}

function focusedValue() {
  if (!exports.focused()) return "";
  const len = exports.focused_text();
  if (!len) return "";
  return decoder.decode(new Uint8Array(memory().buffer, exports.scratch_ptr(), len));
}

function syncIme() {
  const snapshot = editSnapshot();
  if (!snapshot || snapshot.composing) return;
  const value = focusedValue();
  if (ime.value !== value) ime.value = value;
  ime.selectionStart = Math.min(snapshot.anchor, snapshot.active);
  ime.selectionEnd = Math.max(snapshot.anchor, snapshot.active);
}

function applyEditCommands(commands) {
  const snapshot = editSnapshot();
  if (!snapshot) return;
  for (const command of commands) {
    const bytes = encoder.encode(command.text ?? "");
    const ptr = bytes.length ? copyBytes(bytes) : 0;
    if (command.op === "commit") {
      exports.edit_commit(snapshot.session, ptr, bytes.length, 1);
    } else if (command.op === "compose") {
      exports.edit_compose(snapshot.session, ptr, bytes.length, 1);
    } else if (command.op === "replace") {
      exports.edit_replace(snapshot.session, command.start, command.end, ptr, bytes.length, 0, 0);
    } else if (command.op === "delete") {
      if (snapshot.anchor !== snapshot.active) {
        exports.edit_commit(snapshot.session, 0, 0, 1);
      } else {
        exports.edit_delete_surrounding(snapshot.session, command.before ?? 0, command.after ?? 0);
      }
    } else if (command.op === "select") {
      exports.edit_select(snapshot.session, command.anchor, command.active);
    } else if (command.op === "undo") {
      exports.edit_undo(snapshot.session);
    } else if (command.op === "redo") {
      exports.edit_redo(snapshot.session);
    } else if (command.op === "finish") {
      exports.edit_finish_composition(snapshot.session);
    }
    if (bytes.length) exports.dealloc(ptr, bytes.length);
  }
  pumpGestures();
  if (exports.layout() === 0) paint();
  syncIme();
}

for (const type of ["beforeinput", "compositionstart", "compositionupdate", "compositionend"]) {
  ime.addEventListener(type, (event) => {
    const commands = commandsForDom(event);
    if (commands.length === 0) return;
    event.preventDefault();
    applyEditCommands(commands);
  });
}

function placeCaret(handle) {
  if (exports.layout() === 0) paint();
  const flags = exports.field_flags(handle);
  const kind = (flags >> 8) & 0xff;
  ime.inputMode = ["text", "numeric", "email", "url", "tel"][kind] || "text";
  const autofill = (flags >> 16) & 0xff;
  ime.autocomplete = ["off", "username", "current-password", "new-password", "email", "tel", "one-time-code"][autofill] || "off";
  const len = exports.caret_box();
  if (len >= 16) {
    const view = new DataView(memory().buffer, exports.scratch_ptr(), 16);
    const rect = canvas.getBoundingClientRect();
    ime.style.left = `${rect.left + view.getFloat32(0, true)}px`;
    ime.style.top = `${rect.top + view.getFloat32(4, true)}px`;
    ime.style.height = `${Math.max(16, view.getFloat32(12, true))}px`;
  }
  syncIme();
  ime.focus();
}

canvas.addEventListener("pointerup", (event) => {
  if (event.button !== 0) return;
  if (selectDrag && selectDrag.id === event.pointerId) {
    selectDrag = null;
    const handle = exports.focused();
    if (handle) placeCaret(handle);
    return;
  }
  const [x, y] = point(event);
  const handle = exports.focus_at(x, y);
  if (!handle) return;
  placeCaret(handle);
});

const editMenu = document.createElement("div");
editMenu.hidden = true;
Object.assign(editMenu.style, {
  display: "none",
  position: "fixed",
  zIndex: "2",
  gap: "4px",
  padding: "4px",
  background: "#111",
  color: "#fff",
});
for (const [name, label] of [["cut", "Cut"], ["copy", "Copy"], ["paste", "Paste"], ["selectAll", "Select all"]]) {
  const button = document.createElement("button");
  button.type = "button";
  button.textContent = label;
  button.addEventListener("click", () => runEditMenu(name));
  editMenu.append(button);
}
document.body.append(editMenu);

function focusedField() {
  const handle = exports.focused();
  if (!handle) return null;
  const snapshot = editSnapshot();
  if (!snapshot) return null;
  const flags = exports.field_flags(handle);
  return {
    value: focusedValue(),
    selection: [snapshot.anchor, snapshot.active],
    secure: (flags & 2) !== 0,
  };
}

async function runEditMenu(name) {
  editMenu.hidden = true;
  editMenu.style.display = "none";
  if (!exports.focused()) return;
  const action = { cut: 0, copy: 1, paste: 2, selectAll: 3 }[name];
  if (action === undefined) return;
  let clip = "";
  if (name === "paste") {
    try { clip = await navigator.clipboard.readText(); } catch { clip = ""; }
  }
  const bytes = encoder.encode(clip);
  const ptr = bytes.length ? copyBytes(bytes) : 0;
  const code = exports.edit_menu(action, ptr, bytes.length);
  if (bytes.length) exports.dealloc(ptr, bytes.length);
  if (code < 0) return;
  if ((name === "copy" || name === "cut") && code >= 0) {
    const len = exports.scratch_len();
    const text = len
      ? decoder.decode(new Uint8Array(memory().buffer, exports.scratch_ptr(), len))
      : "";
    if (text) {
      try { await navigator.clipboard.writeText(text); } catch { return; }
    }
  }
  pumpGestures();
  if (exports.layout() === 0) paint();
  syncIme();
  ime.focus();
}

canvas.addEventListener("contextmenu", (event) => {
  if (!exports.focused()) return;
  event.preventDefault();
  editMenu.hidden = false;
  editMenu.style.display = "flex";
  editMenu.style.left = `${event.clientX}px`;
  editMenu.style.top = `${event.clientY}px`;
});
canvas.addEventListener("pointercancel", (event) => releasePointer(3, event));

function cancelPointers() {
  for (const id of activePointers) {
    exports.pointer(3, id, 0, 0, performance.now(), 0, 0);
  }
  activePointers.clear();
  pumpGestures();
  if (exports.layout() === 0) paint();
}

window.addEventListener("blur", cancelPointers);
document.addEventListener("visibilitychange", () => {
  if (document.visibilityState === "hidden") cancelPointers();
});

function wheelDelta(event, extent) {
  if (event.deltaMode === 1) return event.deltaY * 16;
  if (event.deltaMode === 2) return event.deltaY * extent;
  return event.deltaY;
}

canvas.addEventListener(
  "wheel",
  (event) => {
    const [x, y] = point(event);
    const weight = event.deltaMode === 2 ? canvas.clientHeight : 1;
    const dx = event.deltaMode === 0 ? event.deltaX : event.deltaX * (event.deltaMode === 1 ? 16 : weight);
    const dy = wheelDelta(event, canvas.clientHeight);
    if (exports.scroll_wheel(x, y, dx, dy)) {
      event.preventDefault();
      if (exports.layout() === 0) paint();
    }
  },
  { passive: false },
);

window.addEventListener("resize", resize);

const started = performance.now();
if (dev) {
  await import(`${dev.bundle}?v=${Date.now()}`);
} else {
  await checkBundle();
  await import(`./fixtures/${fixture}.js`);
}
paint();
const firstFrameMs = performance.now() - started;
console.info(`first frame ${firstFrameMs.toFixed(1)} ms`);
if (firstFrameMs >= 400) console.error("first frame exceeded 400 ms");
