let api;
const parents = [1];
const handlers = new Map();
const bindings = [];
let nextHandler = 1;
const encoder = typeof TextEncoder === "function" ? new TextEncoder() : null;

export function setApi(next) {
  api = next;
}

export function signal(initial) {
  const id = api.signal_new(initial);
  const read = () => api.signal_get(id);
  read.update = (fn) => {
    api.signal_set(id, fn(api.signal_get(id)));
  };
  return read;
}

function parent() {
  return parents[parents.length - 1];
}

function utf8(value) {
  if (encoder) return encoder.encode(value);
  const out = [];
  for (const char of value) {
    let code = char.codePointAt(0);
    if (code < 0x80) {
      out.push(code);
    } else if (code < 0x800) {
      out.push(0xc0 | (code >> 6), 0x80 | (code & 0x3f));
    } else if (code < 0x10000) {
      out.push(0xe0 | (code >> 12), 0x80 | ((code >> 6) & 0x3f), 0x80 | (code & 0x3f));
    } else {
      out.push(
        0xf0 | (code >> 18),
        0x80 | ((code >> 12) & 0x3f),
        0x80 | ((code >> 6) & 0x3f),
        0x80 | (code & 0x3f),
      );
    }
  }
  return Uint8Array.from(out);
}

// BNOD version 1: header (magic + u16 version) + the `NodeDesc` body.
// `id`, `parent`, and `handler` are zero; Rust assigns the handle and parent.
function specBytes(kind, props, text) {
  const bytes = utf8(text);
  const buffer = new Uint8Array(6 + 67 + bytes.length);
  buffer[0] = 0x42;
  buffer[1] = 0x4e;
  buffer[2] = 0x4f;
  buffer[3] = 0x44;
  const view = new DataView(buffer.buffer);
  view.setUint16(4, 1, true);
  let offset = 6;
  view.setUint32(offset, 0, true);
  offset += 4;
  view.setUint32(offset, 0, true);
  offset += 4;
  view.setUint8(offset, kind);
  offset += 1;
  view.setUint8(offset, props.scrollY ? 1 : 0);
  offset += 1;
  view.setUint32(offset, (props.background ?? 0) >>> 0, true);
  offset += 4;
  view.setUint32(offset, (props.color ?? 0xffffffff) >>> 0, true);
  offset += 4;
  view.setFloat32(offset, props.fontSize ?? 16, true);
  offset += 4;
  view.setFloat32(offset, props.radius ?? 0, true);
  offset += 4;
  view.setUint32(offset, 0, true);
  offset += 4;
  view.setFloat32(offset, finite(props.width), true);
  offset += 4;
  view.setFloat32(offset, finite(props.height), true);
  offset += 4;
  view.setFloat32(offset, props.flexGrow ?? 0, true);
  offset += 4;
  view.setFloat32(offset, props.flexShrink ?? 1, true);
  offset += 4;
  view.setUint8(offset, props.row ? 1 : 0);
  offset += 1;
  const pad = props.padding ?? [0, 0, 0, 0];
  for (let index = 0; index < 4; index += 1) {
    view.setFloat32(offset, pad[index] ?? 0, true);
    offset += 4;
  }
  view.setUint32(offset, bytes.length, true);
  offset += 4;
  buffer.set(bytes, offset);
  return buffer;
}

function finite(value) {
  return typeof value === "number" && Number.isFinite(value) ? value : NaN;
}

function pack(kind, props, text) {
  props = props ?? {};
  const handle = api.spawn(parent(), specBytes(kind, props, text));
  if (!handle) throw new Error(api.error() || "spawn failed");
  if (props.onPress) {
    const id = nextHandler++;
    handlers.set(id, props.onPress);
    if (api.bind_event(handle, id) !== 0) throw new Error(api.error() || "bind failed");
  }
  return handle;
}

export function view(props, build) {
  const handle = pack(1, props, "");
  parents.push(handle);
  build();
  parents.pop();
  return handle;
}

export function text(props, content) {
  if (typeof content === "function") {
    const handle = pack(2, props, "");
    bind(handle, content);
    return handle;
  }
  return pack(2, props, content == null ? "" : String(content));
}

export function component(type, props, children) {
  if (typeof type !== "function") throw new Error(`${String(type)} is not a component`);
  return type({ ...(props ?? {}), children });
}

function bind(handle, fn) {
  const id = bindings.length + 1;
  const run = () => {
    api.track_begin(id);
    let value = "";
    try {
      value = String(fn());
    } finally {
      api.track_end();
    }
    writeText(handle, value);
  };
  bindings.push(run);
  run();
}

function writeText(handle, value) {
  if (typeof api.queue_text_utf8 === "function") {
    if (api.queue_text_utf8(handle, value) !== 0) throw new Error(api.error() || "text failed");
    return;
  }
  const bytes = utf8(value);
  const ptr = api.alloc(bytes.length);
  new Uint8Array(api.memory.buffer, ptr, bytes.length).set(bytes);
  api.queue_text(handle, ptr, bytes.length);
  api.dealloc(ptr, bytes.length);
}

export function flush() {
  for (let guard = 0; guard < 8; guard += 1) {
    const ids = api.takeDirty();
    if (ids.length === 0) break;
    for (const id of ids) bindings[id - 1]();
  }
  if (api.commit() === 0 && api.error()) throw new Error(api.error());
  if (typeof api.layout === "function" && api.layout() !== 0) throw new Error(api.error() || "layout failed");
}

export function mount(component) {
  if (api.boot() !== 0) throw new Error(api.error() || "boot failed");
  api.prepare?.();
  parents.length = 0;
  parents.push(1);
  component();
  flush();
}

export function press(handler) {
  const fn = handlers.get(handler);
  if (!fn) return false;
  fn();
  flush();
  return true;
}

globalThis.__bubbleSetApi = setApi;
globalThis.__bubblePress = press;
