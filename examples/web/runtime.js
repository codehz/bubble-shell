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

function pack(kind, props, text) {
  const flags = (props.scrollY ? 1 : 0) | (props.row ? 2 : 0);
  const pad = props.padding ?? [0, 0, 0, 0];
  const handle = typeof api.spawn_text === "function"
    ? api.spawn_text({
      kind,
      parent: parent(),
      flags,
      background: props.background ?? 0,
      color: props.color ?? 0xffffffff,
      fontSize: props.fontSize ?? 16,
      radius: props.radius ?? 0,
      width: props.width ?? NaN,
      height: props.height ?? NaN,
      flexGrow: props.flexGrow ?? 0,
      flexShrink: props.flexShrink ?? 1,
      p0: pad[0] ?? 0,
      p1: pad[1] ?? 0,
      p2: pad[2] ?? 0,
      p3: pad[3] ?? 0,
      text,
    })
    : spawnWasm(kind, flags, pad, props, text);
  if (!handle) throw new Error(api.error() || "spawn failed");
  if (props.onPress) {
    const id = nextHandler++;
    handlers.set(id, props.onPress);
    if (api.bind_event(handle, id) !== 0) throw new Error(api.error() || "bind failed");
  }
  return handle;
}

function spawnWasm(kind, flags, pad, props, text) {
  const bytes = encoder.encode(text);
  const ptr = bytes.length === 0 ? 0 : api.alloc(bytes.length);
  if (bytes.length) {
    new Uint8Array(api.memory.buffer, ptr, bytes.length).set(bytes);
  }
  const handle = api.spawn(
    kind,
    parent(),
    flags,
    props.background ?? 0,
    props.color ?? 0xffffffff,
    props.fontSize ?? 16,
    props.radius ?? 0,
    0,
    props.width ?? NaN,
    props.height ?? NaN,
    props.flexGrow ?? 0,
    props.flexShrink ?? 1,
    pad[0] ?? 0,
    pad[1] ?? 0,
    pad[2] ?? 0,
    pad[3] ?? 0,
    ptr,
    bytes.length,
  );
  if (bytes.length) api.dealloc(ptr, bytes.length);
  return handle;
}

export function view(props, build) {
  const handle = pack(1, props ?? {}, "");
  parents.push(handle);
  build();
  parents.pop();
  return handle;
}

export function text(props, content) {
  if (typeof content === "function") {
    const handle = pack(2, props ?? {}, "");
    bind(handle, content);
    return handle;
  }
  return pack(2, props ?? {}, content == null ? "" : String(content));
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
  const bytes = encoder.encode(value);
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
