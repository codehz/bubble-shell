let api;
const parents = [1];
const handlers = new Map();
const handlersOfNode = new Map();
const bindings = [];
const bindingHandles = new Map();
const childrenOf = new Map([[1, []]]);
const parentOf = new Map();
const cursors = [];
const collectors = [];
let nextHandler = 1;
const encoder = typeof TextEncoder === "function" ? new TextEncoder() : null;

export function setApi(next) {
  api = next;
}

let spawnProps;
let deferredProps;

function allowedSpawnProps() {
  if (!spawnProps) {
    if (typeof api.spawn_props !== "function") {
      throw new Error("host does not expose spawn property names");
    }
    spawnProps = new Set(api.spawn_props());
    if (
      spawnProps.size !== SPEC_PROPS.size ||
      [...spawnProps].some((name) => !SPEC_PROPS.has(name))
    ) {
      throw new Error("host and runtime spawn property schemas differ");
    }
  }
  return spawnProps;
}

function allowedDeferredProps() {
  if (!deferredProps) {
    if (typeof api.deferred_props !== "function") {
      throw new Error("host does not expose deferred property names");
    }
    deferredProps = new Set(api.deferred_props());
  }
  return deferredProps;
}

function allowedNodeProps() {
  return new Set([...allowedSpawnProps(), ...allowedDeferredProps()]);
}

export class Dynamic {
  constructor(run) {
    this.run = run;
  }
}

export function dynamic(run) {
  if (typeof run !== "function") throw new Error("dynamic expects a function");
  return new Dynamic(run);
}

function validateProps(kind, props) {
  const allowed = allowedNodeProps();
  for (const key of Object.keys(props)) {
    if (!allowed.has(key)) {
      throw new Error(`${kind === 2 ? "Text" : "View"} has no property ${key}`);
    }
  }
}

export function signal(initial) {
  if (typeof initial === "string") {
    const id = api.signal_text_new(initial);
    const read = () => api.signal_text_get(id);
    read.id = id;
    read.update = (fn) => {
      api.signal_text_set(id, String(fn(api.signal_text_get(id))));
    };
    return read;
  }
  if (typeof initial === "boolean") {
    const id = api.signal_bool_new(initial ? 1 : 0);
    const read = () => api.signal_bool_get(id);
    read.id = id;
    read.update = (fn) => {
      api.signal_bool_set(id, fn(api.signal_bool_get(id)) ? 1 : 0);
    };
    return read;
  }
  const id = api.signal_new(initial);
  const read = () => api.signal_get(id);
  read.id = id;
  read.update = (fn) => {
    api.signal_set(id, fn(api.signal_get(id)));
  };
  return read;
}

export function effect(run) {
  const id = addBinding(() => run(), null);
  runBinding(id);
  return () => {
    const binding = bindings[id - 1];
    if (binding) binding.active = false;
  };
}

export function computed(run) {
  const out = signal(0);
  effect(() => {
    api.signal_set(out.id, run());
  });
  return out;
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
const SPEC_PROPS = new Set([
  "scrollY",
  "background",
  "color",
  "fontSize",
  "radius",
  "opacity",
  "width",
  "height",
  "flexGrow",
  "flexShrink",
  "row",
  "padding",
  "onPress",
]);

function specBytes(kind, props, text) {
  const bytes = utf8(text);
  const buffer = new Uint8Array(6 + 71 + bytes.length);
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
  view.setFloat32(offset, props.opacity ?? 1, true);
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

function registerChild(parentHandle, handle, index) {
  let list = childrenOf.get(parentHandle);
  if (!list) {
    list = [];
    childrenOf.set(parentHandle, list);
  }
  if (index === undefined || index >= list.length) list.push(handle);
  else list.splice(index, 0, handle);
  parentOf.set(handle, parentHandle);
  const collector = collectors[collectors.length - 1];
  if (collector && collector.parent === parentHandle) collector.handles.push(handle);
}

function pack(kind, props, text) {
  props = props ?? {};
  const parentHandle = parent();
  const cursor = cursors[cursors.length - 1];
  const staticProps = {};
  const deferredValues = [];
  const dynamics = [];
  const deferred = allowedDeferredProps();
  for (const key of Object.keys(props)) {
    validateProps(kind, { [key]: true });
    const value = props[key];
    if (value instanceof Dynamic) dynamics.push([key, value.run]);
    else if (deferred.has(key)) deferredValues.push([key, value]);
    else staticProps[key] = value;
  }
  const spec = specBytes(kind, staticProps, text);
  let handle;
  if (cursor && cursor.parent === parentHandle) {
    handle = api.spawn_at(parentHandle, cursor.index, spec);
    if (!handle) throw new Error(api.error() || "spawn_at failed");
    registerChild(parentHandle, handle, cursor.index);
    cursor.index += 1;
  } else {
    handle = api.spawn(parentHandle, spec);
    if (!handle) throw new Error(api.error() || "spawn failed");
    registerChild(parentHandle, handle);
  }
  if (props.onPress) {
    const id = nextHandler++;
    handlers.set(id, props.onPress);
    let owned = handlersOfNode.get(handle);
    if (!owned) {
      owned = new Set();
      handlersOfNode.set(handle, owned);
    }
    owned.add(id);
    if (api.bind_event(handle, id) !== 0) throw new Error(api.error() || "bind failed");
  }
  for (const [key, value] of deferredValues) setProp(handle, key, value);
  for (const [key, run] of dynamics) bindProp(handle, key, run);
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

const primitives = new Map();

export function registerPrimitive(name, factory) {
  if (typeof factory !== "function") throw new Error("primitive factory must be a function");
  primitives.set(name, factory);
}

export function primitive(name, props, children) {
  const factory = primitives.get(name);
  if (!factory) throw new Error(`unknown primitive ${name}`);
  return factory(props, children);
}

registerPrimitive("View", view);
registerPrimitive("Text", text);

function propParts(value) {
  if (value == null) return [0, 0, ""];
  if (typeof value === "boolean") return [1, value ? 1 : 0, ""];
  if (typeof value === "number") return [Number.isInteger(value) ? 2 : 3, value, ""];
  if (typeof value === "string") return [4, 0, value];
  throw new Error(`unsupported property value ${typeof value}`);
}

const BOX_PROPS = {
  padding: ["paddingLeft", "paddingRight", "paddingTop", "paddingBottom"],
  margin: ["marginLeft", "marginRight", "marginTop", "marginBottom"],
  inset: ["top", "right", "bottom", "left"],
};

export function setProp(handle, key, value) {
  const edges = BOX_PROPS[key];
  if (edges) {
    if (!Array.isArray(value)) throw new Error(`${key} must be an array`);
    for (let index = 0; index < edges.length; index += 1) {
      setProp(handle, edges[index], value[index] ?? (key === "inset" ? null : 0));
    }
    return;
  }
  const [tag, number, text] = propParts(value);
  if (api.set_prop(handle, key, tag, number, text) !== 0) {
    throw new Error(api.error() || "set_prop failed");
  }
}

export function bindProp(handle, key, fn) {
  const id = addBinding(() => setProp(handle, key, fn()), handle);
  runBinding(id);
}

function addBinding(run, handle) {
  const id = bindings.length + 1;
  bindings.push({ run, active: true });
  if (handle != null) {
    let set = bindingHandles.get(handle);
    if (!set) {
      set = new Set();
      bindingHandles.set(handle, set);
    }
    set.add(id);
  }
  return id;
}

function runBinding(id) {
  const binding = bindings[id - 1];
  if (!binding || !binding.active) return;
  api.track_begin(id);
  try {
    binding.run();
  } finally {
    api.track_end();
  }
}

function deactivateHandleBindings(handle) {
  const set = bindingHandles.get(handle);
  if (!set) return;
  for (const id of set) {
    const binding = bindings[id - 1];
    if (binding) binding.active = false;
  }
  bindingHandles.delete(handle);
}

function bind(handle, fn) {
  const id = addBinding(() => writeText(handle, String(fn())), handle);
  runBinding(id);
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

function disposeHandle(handle) {
  if (handle === 1) return;
  deactivateHandleBindings(handle);
  const owned = handlersOfNode.get(handle);
  if (owned) {
    for (const id of owned) handlers.delete(id);
    handlersOfNode.delete(handle);
  }
  const kids = childrenOf.get(handle);
  if (kids) {
    for (const kid of [...kids]) disposeHandle(kid);
  }
  const parentHandle = parentOf.get(handle);
  if (parentHandle !== undefined) {
    const list = childrenOf.get(parentHandle);
    const index = list ? list.indexOf(handle) : -1;
    if (index >= 0) list.splice(index, 1);
  }
  childrenOf.delete(handle);
  parentOf.delete(handle);
  if (api.dispose(handle) !== 0) throw new Error(api.error() || "dispose failed");
}

function moveHandle(handle, parentHandle, index) {
  const oldParent = parentOf.get(handle);
  if (oldParent !== undefined) {
    const old = childrenOf.get(oldParent);
    const position = old ? old.indexOf(handle) : -1;
    if (position >= 0) old.splice(position, 1);
  }
  let list = childrenOf.get(parentHandle);
  if (!list) {
    list = [];
    childrenOf.set(parentHandle, list);
  }
  list.splice(index, 0, handle);
  parentOf.set(handle, parentHandle);
  if (api.move_node(handle, parentHandle, index) !== 0) {
    throw new Error(api.error() || "move failed");
  }
}

function anchorIndex(parentHandle, anchor) {
  const list = childrenOf.get(parentHandle);
  const index = list ? list.indexOf(anchor) : -1;
  if (index < 0) throw new Error("fragment anchor is detached");
  return index;
}

function renderAt(parentHandle, index, build) {
  const collector = { parent: parentHandle, handles: [] };
  collectors.push(collector);
  cursors.push({ parent: parentHandle, index });
  parents.push(parentHandle);
  try {
    build();
  } finally {
    parents.pop();
    cursors.pop();
    collectors.pop();
  }
  return collector.handles;
}

function createAnchor() {
  return pack(1, { width: 0, height: 0, flexGrow: 0, flexShrink: 0 }, "");
}

export function when(cond, thenBranch, elseBranch) {
  const condFn = typeof cond === "function" ? cond : () => cond;
  const parentHandle = parent();
  const anchor = createAnchor();
  let handles = [];
  const render = () => {
    for (const handle of handles) disposeHandle(handle);
    handles = [];
    const branch = condFn() ? thenBranch : elseBranch;
    if (typeof branch !== "function") return;
    handles = renderAt(parentHandle, anchorIndex(parentHandle, anchor), branch);
  };
  runBinding(addBinding(render, anchor));
  return anchor;
}

export function each(items, key, render) {
  if (typeof render !== "function") throw new Error("each requires a render function");
  const itemsFn = typeof items === "function" ? items : () => items;
  const keyFn = typeof key === "function"
    ? key
    : typeof key === "string"
      ? (item) => item[key]
      : (_item, index) => index;
  const parentHandle = parent();
  const anchor = createAnchor();
  const groups = new Map();
  const renderList = () => {
    const list = itemsFn() ?? [];
    const next = [];
    for (let index = 0; index < list.length; index += 1) {
      const item = list[index];
      next.push({ item, key: keyFn(item, index) });
    }
    const seen = new Set(next.map((entry) => entry.key));
    for (const [groupKey, handles] of groups) {
      if (!seen.has(groupKey)) {
        for (const handle of handles) disposeHandle(handle);
        groups.delete(groupKey);
      }
    }
    let index = anchorIndex(parentHandle, anchor);
    for (const entry of next) {
      let handles = groups.get(entry.key);
      if (handles === undefined) {
        handles = renderAt(parentHandle, index, () => render(entry.item, entry.key));
        groups.set(entry.key, handles);
        index += handles.length;
      } else {
        for (const handle of handles) {
          moveHandle(handle, parentHandle, index);
          index += 1;
        }
      }
    }
  };
  runBinding(addBinding(renderList, anchor));
  return anchor;
}

export function flush() {
  for (let guard = 0; guard < 8; guard += 1) {
    const ids = api.takeDirty();
    if (ids.length === 0) break;
    for (const id of ids) runBinding(id);
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

export function press(handler, x, y) {
  const fn = handlers.get(handler);
  if (!fn) return false;
  const event = x && typeof x === "object"
    ? x
    : { type: "tap", x: x ?? 0, y: y ?? 0 };
  fn(event);
  flush();
  return true;
}

globalThis.__bubbleSetApi = setApi;
globalThis.__bubblePress = press;
