let api;
const parents = [1];
const handlers = new Map();
const handlersOfNode = new Map();

const bindingHandles = new Map();
const childrenOf = new Map([[1, []]]);
const parentOf = new Map();
const cursors = [];
const collectors = [];
let nextHandler = 1;
const gestureHandlers = new Map();
const fieldHandlers = new Map();
const gesturesOfTarget = new Map();
const gestureOrderOfTarget = new Map();
const gestureParentOf = new Map();
const gestureBindings = new Map();
const gestureHandlerIds = new Map();
const retiredGestureHandlers = new Map();
const gestureOwners = new Map();
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

const signalValues = [undefined];
const computations = new Map();
const cleanups = [];
let bodyFn = null;
let bodyError = null;

function signalChanged(equals, prev, next) {
  if (equals === false) return true;
  if (typeof equals === "function") return !equals(prev, next);
  return prev !== next;
}

function writeSignal(id, value, equals) {
  const prev = signalValues[id];
  if (!signalChanged(equals, prev, value)) return;
  signalValues[id] = value;
  const code = api.signal_notify(id);
  if (code !== 0) throw new Error(api.error() || "signal write failed");
}

export function signal(initial, options) {
  const id = api.signal_alloc();
  if (!id) throw new Error(api.error() || "signal alloc failed");
  signalValues[id] = initial;
  const equals = options && options.equals;
  const read = () => {
    const code = api.signal_read(id);
    if (code !== 0) throw new Error(api.error() || "signal read failed");
    return signalValues[id];
  };
  read.id = id;
  read.set = (value) => writeSignal(id, value, equals);
  read.update = (fn) => writeSignal(id, fn(signalValues[id]), equals);
  return read;
}

function createComputation(kind, run, equals) {
  const id = api.alloc_node(kind);
  if (!id) throw new Error(api.error() || "alloc_node failed");
  computations.set(id, { kind, run, equals, value: undefined, ready: false });
  if (api.mount_node(id) !== 0) throw new Error(api.error() || "mount_node failed");
  return id;
}

export function effect(run) {
  const id = createComputation(3, run);
  return () => {
    if (api.dispose_node(id) !== 0) throw new Error(api.error() || "dispose_node failed");
  };
}

export function computed(run, options) {
  const id = createComputation(1, run, options && options.equals);
  const read = () => {
    if (api.signal_read(id) !== 0) throw new Error(api.error() || "computed read failed");
    return computations.get(id).value;
  };
  read.id = id;
  return read;
}

export function cleanup(fn) {
  const id = cleanups.length;
  cleanups.push(fn);
  if (api.on_cleanup(id) !== 0) fn();
}

export function batch(fn) {
  const previous = bodyFn;
  bodyFn = fn;
  try {
    if (api.run_updates() !== 0) throw new Error(api.error() || "batch failed");
    if (bodyError) {
      const error = bodyError;
      bodyError = null;
      throw error;
    }
  } finally {
    bodyFn = previous;
  }
}

globalThis.__bubbleBody = () => {
  try {
    bodyFn?.();
  } catch (error) {
    bodyError = error;
  }
};

globalThis.__bubbleRun = (id) => {
  const node = computations.get(id);
  if (!node) return 0;
  try {
    if (node.kind === 1) {
      const next = node.run(node.ready ? node.value : undefined);
      const changed = node.ready && signalChanged(node.equals, node.value, next);
      node.value = next;
      node.ready = true;
      return changed ? 1 : 0;
    }
    node.run();
    return 0;
  } catch (error) {
    bodyError = error;
    return -1;
  }
};

globalThis.__bubbleCleanup = (id) => {
  const fn = cleanups[id];
  if (fn) fn();
};

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

// BNOD version 2: header (magic + u16 version) + the `NodeDesc` body + TLV.
// `id`, `parent`, and `handler` are zero; Rust assigns the handle and parent.
const SPEC_PROPS = new Set([
  "overflow",
  "overflowX",
  "overflowY",
  "fontSize",
  "radius",
  "opacity",
  "width",
  "height",
  "flexGrow",
  "flexShrink",
  "flexDirection",
  "padding",
]);

function specBytes(kind, props, text) {
  const bytes = utf8(text);
  const buffer = new Uint8Array(6 + 71 + 4 + bytes.length);
  buffer[0] = 0x42;
  buffer[1] = 0x4e;
  buffer[2] = 0x4f;
  buffer[3] = 0x44;
  const view = new DataView(buffer.buffer);
  view.setUint16(4, 2, true);
  let offset = 6;
  view.setUint32(offset, 0, true);
  offset += 4;
  view.setUint32(offset, 0, true);
  offset += 4;
  view.setUint8(offset, kind);
  offset += 1;
  view.setUint8(offset, scrollFlags(props));
  offset += 1;
  // Color props are deferred so Rust parses numeric ARGB and CSS strings
  // through one implementation; the descriptor keeps its default slots.
  view.setUint32(offset, 0, true);
  offset += 4;
  view.setUint32(offset, 0xffffffff, true);
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
  view.setUint8(offset, rowFlag(props.flexDirection));
  offset += 1;
  const pad = props.padding == null ? [0, 0, 0, 0] : cssBox(props.padding, "padding");
  for (let index = 0; index < 4; index += 1) {
    view.setFloat32(offset, pad[index] ?? 0, true);
    offset += 4;
  }
  view.setUint32(offset, bytes.length, true);
  offset += 4;
  buffer.set(bytes, offset);
  offset += bytes.length;
  view.setUint32(offset, 0, true);
  return buffer;
}

function finite(value) {
  return typeof value === "number" && Number.isFinite(value) ? value : NaN;
}
function scrollFlags(props) {
  let x = "visible";
  let y = "visible";
  if (props.overflow != null) {
    x = props.overflow;
    y = props.overflow;
  }
  if (props.overflowX != null) x = props.overflowX;
  if (props.overflowY != null) y = props.overflowY;
  return (overflowBit(y) ? 1 : 0) | (overflowBit(x) ? 2 : 0);
}

function overflowBit(value) {
  if (value === "visible") return false;
  if (value === "scroll") return true;
  if (value === "hidden") throw new Error("overflow hidden is not implemented");
  throw new Error(`unsupported overflow ${value}`);
}

function rowFlag(value) {
  if (value == null || value === "column") return 0;
  if (value === "row") return 1;
  throw new Error(`unsupported flexDirection ${value}`);
}

function cssSides(value, key) {
  if (!Array.isArray(value) || (value.length !== 1 && value.length !== 2 && value.length !== 4)) {
    throw new Error(`${key} must be an array of 1, 2, or 4 numbers`);
  }
  const top = value[0] ?? 0;
  const right = value.length === 1 ? top : (value[1] ?? 0);
  const bottom = value.length === 4 ? (value[2] ?? 0) : top;
  const left = value.length === 4 ? (value[3] ?? 0) : right;
  return { top, right, bottom, left };
}

function cssBox(value, key) {
  const sides = cssSides(value, key);
  return [sides.left, sides.right, sides.top, sides.bottom];
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

export function image(props) {
  return pack(3, props, "");
}

export function textField(props) {
  props = props ?? {};
  const onChange = props.onChange;
  const value = props.value;
  if (value instanceof Dynamic) {
    throw new Error("TextField value is set with setEditingValue, not a signal binding");
  }
  const rest = {};
  for (const key of Object.keys(props)) {
    if (key !== "onChange" && key !== "value") rest[key] = props[key];
  }
  const handle = pack(4, rest, value == null ? "" : String(value));
  if (onChange != null) {
    if (typeof onChange !== "function") throw new Error("onChange must be a function");
    const handler = nextHandler++;
    fieldHandlers.set(handler, onChange);
    if (api.bind_event(handle, handler) !== 0) {
      throw new Error(api.error() || "bind_event failed");
    }
  }
  return handle;
}

export function setEditingValue(handle, epoch, value, start, end, cancel) {
  if (api.set_editing_value(handle, epoch, String(value ?? ""), start ?? 0, end ?? 0, cancel ? 1 : 0) !== 0) {
    throw new Error(api.error() || "set_editing_value failed");
  }
}

export function focusField(handle) {
  if (api.focus(handle ?? 0) !== 0) throw new Error(api.error() || "focus failed");
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

const GESTURE_KINDS = {
  TapGesture: 1,
  DoubleTapGesture: 2,
  LongPressGesture: 3,
  PanGesture: 4,
  TransformGesture: 5,
};

const GESTURE_EVENTS = {
  1: { onTapStart: 1, onTapEnd: 2, onTapCancel: 3, onTap: 4, enabled: 0 },
  2: { onDoubleTapStart: 1, onDoubleTapEnd: 2, onDoubleTapCancel: 3, onDoubleTap: 5, enabled: 0 },
  3: { onLongPress: 6, onLongPressEnd: 8, onLongPressCancel: 9, duration: 0, maxDistance: 0, captureAxis: 0, enabled: 0 },
  4: {
    onPanStart: 6,
    onPanUpdate: 7,
    onPanEnd: 8,
    onPanCancel: 9,
    axis: 0,
    direction: 0,
    minDistance: 0,
    failCrossDistance: 0,
    captureAxis: 0,
    enabled: 0,
  },
  5: {
    onTransformStart: 6,
    onTransformUpdate: 7,
    onTransformEnd: 8,
    onTransformCancel: 9,
    mode: 0,
    minDistance: 0,
    captureAxis: 0,
    enabled: 0,
  },
};

function gesturePropValue(value) {
  if (value == null) return [0, 0, ""];
  if (typeof value === "boolean") return [1, value ? 1 : 0, ""];
  if (typeof value === "number") return [Number.isInteger(value) ? 2 : 3, value, ""];
  if (typeof value === "string") return [4, 0, value];
  throw new Error(`unsupported gesture property value ${typeof value}`);
}

function setGestureProp(handle, key, value) {
  if (key === "mode") {
    const parts = Array.isArray(value) ? value : String(value).split(",");
    value = parts.map((part) => String(part).trim()).filter(Boolean).join(",");
  } else if (key === "direction" && Array.isArray(value)) {
    value = value.join(",");
  }
  const [tag, number, text] = gesturePropValue(value);
  if (api.set_gesture_prop(handle, key, tag, number, text) !== 0) {
    throw new Error(api.error() || "set_gesture_prop failed");
  }
}

function bindGestureProp(handle, key, fn) {
  let set = gestureBindings.get(handle);
  if (!set) {
    set = new Set();
    gestureBindings.set(handle, set);
  }
  const id = addBinding(() => setGestureProp(handle, key, fn()), null);
  set.add(id);

}

function registerGesture(target, kind) {
  if (typeof api.spawn_gesture !== "function") {
    throw new Error("host does not expose gesture declarations");
  }
  let order = gestureOrderOfTarget.get(target);
  if (!order) {
    order = [];
    gestureOrderOfTarget.set(target, order);
  }
  const index = order.length;
  const id = api.spawn_gesture(target, kind, index);
  if (!id) throw new Error(api.error() || "spawn_gesture failed");
  order.push(id);
  let list = gesturesOfTarget.get(target);
  if (!list) {
    list = [];
    gesturesOfTarget.set(target, list);
  }
  list.push(id);
  gestureParentOf.set(id, target);
  gestureOwners.set(id, collectors[collectors.length - 1] ?? null);
  // Nested If/Each branches can target the same surviving ancestor; every
  // such scope must be able to dispose the declaration it created.
  for (const collector of collectors) {
    if (collector.parent === target) collector.gestures.push(id);
  }
  return { id };
}

function rememberGestureHandler(gesture, handler) {
  let ids = gestureHandlerIds.get(gesture);
  if (!ids) {
    ids = [];
    gestureHandlerIds.set(gesture, ids);
  }
  ids.push(handler);
}

function addGestureHandlers(id, props, allowed) {
  for (const [name, slot] of Object.entries(GESTURE_EVENTS[allowed])) {
    if (!slot) continue;
    const fn = props[name];
    if (fn == null) continue;
    if (typeof fn !== "function") throw new Error(`${name} must be a function`);
    const handler = nextHandler++;
    gestureHandlers.set(handler, fn);
    rememberGestureHandler(id, handler);
    if (api.bind_gesture_event(id, slot, handler) !== 0) {
      throw new Error(api.error() || "bind_gesture_event failed");
    }
  }
}

function packGesture(type, props) {
  const kind = GESTURE_KINDS[type];
  if (!kind) throw new Error(`unknown gesture ${type}`);
  props = props ?? {};
  const allowed = GESTURE_EVENTS[kind];
  for (const key of Object.keys(props)) {
    if (!(key in allowed)) throw new Error(`${type} has no property ${key}`);
  }
  const target = parent();
  const { id } = registerGesture(target, kind);
  for (const key of Object.keys(props)) {
    const value = props[key];
    if (key in GESTURE_EVENTS[kind] && GESTURE_EVENTS[kind][key]) continue;
    if (value instanceof Dynamic) bindGestureProp(id, key, value.run);
    else setGestureProp(id, key, value);
  }
  addGestureHandlers(id, props, kind);
  return id;
}


export function tapGesture(props) {
  return packGesture("TapGesture", props);
}
export function doubleTapGesture(props) {
  return packGesture("DoubleTapGesture", props);
}
export function longPressGesture(props) {
  return packGesture("LongPressGesture", props);
}
export function panGesture(props) {
  return packGesture("PanGesture", props);
}
export function transformGesture(props) {
  return packGesture("TransformGesture", props);
}

registerPrimitive("View", view);
registerPrimitive("Text", text);
registerPrimitive("Image", image);
registerPrimitive("TextField", textField);
registerPrimitive("TapGesture", tapGesture);
registerPrimitive("DoubleTapGesture", doubleTapGesture);
registerPrimitive("LongPressGesture", longPressGesture);
registerPrimitive("PanGesture", panGesture);
registerPrimitive("TransformGesture", transformGesture);

function propParts(value) {
  if (value == null) return [0, 0, ""];
  if (typeof value === "boolean") return [1, value ? 1 : 0, ""];
  if (typeof value === "number") return [Number.isInteger(value) ? 2 : 3, value, ""];
  if (typeof value === "string") return [4, 0, value];
  throw new Error(`unsupported property value ${typeof value}`);
}

export function setProp(handle, key, value) {
  if (key === "padding" || key === "margin" || key === "inset") {
    const sides = cssSides(value, key);
    if (key === "inset") {
      setProp(handle, "top", sides.top);
      setProp(handle, "right", sides.right);
      setProp(handle, "bottom", sides.bottom);
      setProp(handle, "left", sides.left);
    } else {
      const prefix = key === "padding" ? "padding" : "margin";
      setProp(handle, `${prefix}Left`, sides.left);
      setProp(handle, `${prefix}Right`, sides.right);
      setProp(handle, `${prefix}Top`, sides.top);
      setProp(handle, `${prefix}Bottom`, sides.bottom);
    }
    return;
  }
  const [tag, number, text] = propParts(value);
  if (api.set_prop(handle, key, tag, number, text) !== 0) {
    throw new Error(api.error() || "set_prop failed");
  }
}

export function bindProp(handle, key, fn) {
  addBinding(() => setProp(handle, key, fn()), handle);
}

function addBinding(run, handle) {
  const id = createComputation(2, run);
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

function deactivateHandleBindings(handle) {
  const set = bindingHandles.get(handle);
  if (!set) return;
  for (const id of set) {
    if (api.dispose_node(id) !== 0) throw new Error(api.error() || "dispose_node failed");
  }
  bindingHandles.delete(handle);
}

function bind(handle, fn) {
  addBinding(() => writeText(handle, String(fn())), handle);
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

function deactivateGestureBindings(id) {
  const set = gestureBindings.get(id);
  if (!set) return;
  for (const binding of set) {
    if (api.dispose_node(binding) !== 0) throw new Error(api.error() || "dispose_node failed");
  }
  gestureBindings.delete(id);
}

function disposeGesture(id) {
  if (!gestureParentOf.has(id)) return;
  const target = gestureParentOf.get(id);
  gestureParentOf.delete(id);
  gestureOwners.delete(id);
  deactivateGestureBindings(id);
  const handlers = gestureHandlerIds.get(id);
  if (handlers) {
    // The shell acknowledges disposal after queued terminal events. Keeping
    // handlers until that barrier also covers disposal from an event callback.
    retiredGestureHandlers.set(id, handlers);
    gestureHandlerIds.delete(id);
  }
  const order = gestureOrderOfTarget.get(target);
  if (order) {
    const index = order.indexOf(id);
    if (index >= 0) order.splice(index, 1);
  }
  const list = gesturesOfTarget.get(target);
  if (list) {
    const index = list.indexOf(id);
    if (index >= 0) list.splice(index, 1);
  }
  if (api.dispose_gesture(id) !== 0) throw new Error(api.error() || "dispose gesture failed");
}

function disposeTargetGestures(handle) {
  const list = gesturesOfTarget.get(handle);
  if (!list) return;
  for (const id of [...list]) disposeGesture(id);
  gesturesOfTarget.delete(handle);
  gestureOrderOfTarget.delete(handle);
}

function disposeHandle(handle) {
  if (handle === 1) return;
  disposeTargetGestures(handle);
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
  const collector = { parent: parentHandle, handles: [], gestures: [] };
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
  return collector;
}

function createAnchor() {
  return pack(1, { width: 0, height: 0, flexGrow: 0, flexShrink: 0, position: "absolute" }, "");
}

function replaceBranch(parentHandle, anchor, branch, build) {
  const anchors = new Map();
  for (const gesture of branch.gestures) {
    const target = gestureParentOf.get(gesture);
    const list = gestureOrderOfTarget.get(target);
    const index = list ? list.indexOf(gesture) : -1;
    if (index >= 0) anchors.set(target, Math.min(anchors.get(target) ?? index, index));
  }
  for (const handle of branch.handles) disposeHandle(handle);
  for (const gesture of branch.gestures) disposeGesture(gesture);
  const next = { parent: parentHandle, handles: [], gestures: [] };
  if (typeof build === "function") {
    const created = renderAt(parentHandle, anchorIndex(parentHandle, anchor), build);
    next.handles = created.handles;
    next.gestures = created.gestures;
  }
  for (const [target, at] of anchors) {
    const list = gestureOrderOfTarget.get(target);
    if (!list) continue;
    const created = [...new Set(next.gestures)].filter(
      (gesture) => gestureParentOf.get(gesture) === target,
    );
    for (let offset = 0; offset < created.length; offset += 1) {
      const gesture = created[offset];
      const from = list.indexOf(gesture);
      if (from < 0) continue;
      const to = Math.min(at + offset, list.length - 1);
      if (from === to) continue;
      list.splice(from, 1);
      list.splice(to, 0, gesture);
      if (api.move_gesture(gesture, target, to) !== 0) {
        throw new Error(api.error() || "move gesture failed");
      }
    }
  }
  return next;
}

export function show(whenFn, fallback, render) {
  if (typeof whenFn !== "function") throw new Error("Show requires a when accessor");
  if (typeof render !== "function") throw new Error("Show requires a render function");
  const parentHandle = parent();
  const anchor = createAnchor();
  let branch = { parent: parentHandle, handles: [], gestures: [] };
  let open = false;
  const read = () => whenFn();
  addBinding(() => {
    const now = !!whenFn();
    if (now && open) return;
    open = now;
    branch = replaceBranch(
      parentHandle,
      anchor,
      branch,
      now ? () => render(read) : (typeof fallback === "function" ? fallback : null),
    );
  }, anchor);
  return anchor;
}

export function forEach(items, by, render) {
  if (typeof by !== "function") throw new Error("For requires a by function");
  if (typeof render !== "function") throw new Error("For requires a render function");
  const itemsFn = typeof items === "function" ? items : () => items;
  const parentHandle = parent();
  const anchor = createAnchor();
  const groups = new Map();
  const live = new Map();
  const disposeGroup = (entry) => {
    for (const handle of entry.handles) disposeHandle(handle);
    for (const gesture of entry.gestures) disposeGesture(gesture);
  };
  addBinding(() => {
    const list = itemsFn() ?? [];
    const next = [];
    for (let index = 0; index < list.length; index += 1) {
      next.push({ item: list[index], key: by(list[index], index), index });
    }
    const seen = new Set(next.map((entry) => entry.key));
    for (const [groupKey, entry] of groups) {
      if (!seen.has(groupKey)) {
        disposeGroup(entry);
        groups.delete(groupKey);
        live.delete(groupKey);
      }
    }
    let cursor = anchorIndex(parentHandle, anchor);
    const rank = new Map();
    for (const entry of next) {
      live.set(entry.key, entry.item);
      let group = groups.get(entry.key);
      if (group === undefined) {
        const key = entry.key;
        const read = () => {
          itemsFn();
          return live.get(key);
        };
        group = renderAt(parentHandle, cursor, () => render(read, entry.index));
        groups.set(key, group);
        cursor += group.handles.length;
      } else {
        for (const handle of group.handles) {
          moveHandle(handle, parentHandle, cursor);
          cursor += 1;
        }
      }
      rank.set(group, rank.size);
    }
    reorderScopedGestures(rank);
  }, anchor);
  return anchor;
}

export function indexEach(items, render) {
  if (typeof render !== "function") throw new Error("Index requires a render function");
  const itemsFn = typeof items === "function" ? items : () => items;
  const parentHandle = parent();
  const anchor = createAnchor();
  const groups = new Map();
  const disposeGroup = (entry) => {
    for (const handle of entry.handles) disposeHandle(handle);
    for (const gesture of entry.gestures) disposeGesture(gesture);
  };
  addBinding(() => {
    const list = itemsFn() ?? [];
    for (const [index, entry] of groups) {
      if (index >= list.length) {
        disposeGroup(entry);
        groups.delete(index);
      }
    }
    let cursor = anchorIndex(parentHandle, anchor);
    const rank = new Map();
    for (let index = 0; index < list.length; index += 1) {
      let group = groups.get(index);
      if (group === undefined) {
        const at = index;
        const read = () => (itemsFn() ?? [])[at];
        group = renderAt(parentHandle, cursor, () => render(read, at));
        groups.set(index, group);
        cursor += group.handles.length;
      } else {
        for (const handle of group.handles) {
          moveHandle(handle, parentHandle, cursor);
          cursor += 1;
        }
      }
      rank.set(group, rank.size);
    }
    reorderScopedGestures(rank);
  }, anchor);
  return anchor;
}

export function switchFlow(fallback, arms) {
  if (!Array.isArray(arms)) throw new Error("Switch requires matches");
  const parentHandle = parent();
  const anchor = createAnchor();
  let branch = { parent: parentHandle, handles: [], gestures: [] };
  let active = -2;
  addBinding(() => {
    let chosen = -1;
    for (let index = 0; index < arms.length; index += 1) {
      if (arms[index][0]()) {
        chosen = index;
        break;
      }
    }
    if (chosen === active) return;
    active = chosen;
    const selected = chosen;
    branch = replaceBranch(
      parentHandle,
      anchor,
      branch,
      chosen < 0
        ? (typeof fallback === "function" ? fallback : null)
        : () => arms[selected][1](() => arms[selected][0]()),
    );
  }, anchor);
  return anchor;
}

// A reordered Each keeps each group's declarations together but places the
// groups in virtual child order for the next touch sequence.
function reorderScopedGestures(rank) {
  if (rank.size < 2) return;
  for (const [target, list] of gestureOrderOfTarget) {
    const slots = [];
    for (let index = 0; index < list.length; index += 1) {
      const owner = gestureOwners.get(list[index]);
      if (owner && rank.has(owner)) slots.push(index);
    }
    if (slots.length < 2) continue;
    const ids = slots.map((slot) => list[slot]);
    ids.sort((a, b) => {
      const ownerA = gestureOwners.get(a);
      const ownerB = gestureOwners.get(b);
      const rankA = rank.get(ownerA);
      const rankB = rank.get(ownerB);
      if (rankA !== rankB) return rankA - rankB;
      return ownerA.gestures.indexOf(a) - ownerB.gestures.indexOf(b);
    });
    for (let index = 0; index < slots.length; index += 1) {
      const gesture = ids[index];
      if (list[slots[index]] !== gesture) {
        if (api.move_gesture(gesture, target, slots[index]) !== 0) {
          throw new Error(api.error() || "move gesture failed");
        }
      }
      list[slots[index]] = gesture;
    }
  }
}

export function flush() {
  if (bodyError) {
    const error = bodyError;
    bodyError = null;
    throw error;
  }
  if (api.commit() === 0 && api.error()) throw new Error(api.error());
  if (typeof api.layout === "function" && api.layout() !== 0) throw new Error(api.error() || "layout failed");
}

export function mount(component) {
  computations.clear();
  cleanups.length = 0;
  if (api.boot() !== 0) throw new Error(api.error() || "boot failed");
  api.prepare?.();
  bodyFn = () => {
    parents.length = 0;
    parents.push(1);
    component();
  };
  if (api.run_init() !== 0) throw new Error(api.error() || "run_init failed");
  flush();
}

export function press(handler, x, y) {
  const fn = handlers.get(handler);
  if (!fn) return false;
  const event = x && typeof x === "object"
    ? x
    : { type: "tap", x: x ?? 0, y: y ?? 0 };
  batch(() => fn(event));
  flush();
  return true;
}

const GESTURE_SLOTS = {
  0: "disposed",
  1: "pressStart",
  2: "pressEnd",
  3: "pressCancel",
  4: "tap",
  5: "doubleTap",
  6: "start",
  7: "update",
  8: "end",
  9: "cancel",
};

function decodeGestureEvents(bytes) {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  if (bytes.byteLength < 10 || view.getUint8(0) !== 0x42 || view.getUint8(1) !== 0x53 ||
      view.getUint8(2) !== 0x47 || view.getUint8(3) !== 0x45) {
    throw new Error("bad gesture event batch");
  }
  const version = view.getUint16(4, true);
  if (version !== 1) throw new Error(`unsupported gesture event version ${version}`);
  const count = view.getUint32(6, true);
  const events = [];
  let offset = 10;
  for (let index = 0; index < count; index += 1) {
    if (offset + 66 > bytes.byteLength) throw new Error("truncated gesture event");
    const slot = view.getUint8(offset);
    const gesture = view.getUint32(offset + 1, true);
    const sequence = view.getUint32(offset + 5, true);
    const handler = view.getUint32(offset + 9, true);
    const target = view.getUint32(offset + 13, true);
    const reason = view.getUint8(offset + 17);
    const time = view.getFloat64(offset + 18, true);
    const f = (at) => view.getFloat32(offset + 26 + at * 4, true);
    events.push({
      type: GESTURE_SLOTS[slot] ?? "unknown",
      gesture,
      sequence,
      handler,
      target,
      reason,
      time,
      x: f(0),
      y: f(1),
      translationX: f(2),
      translationY: f(3),
      deltaX: f(4),
      deltaY: f(5),
      scale: f(6),
      rotation: f(7),
      velocityX: f(8),
      velocityY: f(9),
    });
    offset += 66;
  }
  return events;
}

export function dispatchGestureEvents(bytes) {
  const events = bytes instanceof Uint8Array ? decodeGestureEvents(bytes) : bytes;
  let ran = false;
  let dispatched = 0;
  batch(() => {
    for (const event of events) {
      if (event.type === "disposed") {
        const ids = retiredGestureHandlers.get(event.gesture);
        if (ids) {
          for (const handler of ids) gestureHandlers.delete(handler);
          retiredGestureHandlers.delete(event.gesture);
        }
        continue;
      }
      dispatched += 1;
      const fn = gestureHandlers.get(event.handler);
      if (!fn) continue;
      ran = true;
      fn(event);
    }
  });
  if (ran) flush();
  return dispatched;
}

globalThis.__bubbleSetApi = setApi;
globalThis.__bubblePress = press;
globalThis.__bubbleGesture = dispatchGestureEvents;
globalThis.__bubbleEdit = dispatchEditEvents;

export function dispatchEditEvents(bytes) {
  const events = decodeEditEvents(bytes);
  let ran = false;
  batch(() => {
    for (const event of events) {
      const fn = fieldHandlers.get(event.handler);
      if (!fn) continue;
      ran = true;
      fn(event);
    }
  });
  if (ran) flush();
  return events.length;
}

function decodeEditEvents(bytes) {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  if (bytes.byteLength < 10 || view.getUint8(0) !== 0x42 || view.getUint8(1) !== 0x53 ||
      view.getUint8(2) !== 0x45 || view.getUint8(3) !== 0x44) {
    throw new Error("bad edit event batch");
  }
  const version = view.getUint16(4, true);
  if (version !== 1) throw new Error(`unsupported edit event version ${version}`);
  const count = view.getUint32(6, true);
  const events = [];
  let offset = 10;
  for (let index = 0; index < count; index += 1) {
    if (offset + 36 > bytes.byteLength) throw new Error("truncated edit event");
    const handler = view.getUint32(offset, true);
    const handle = view.getUint32(offset + 4, true);
    const revision = Number(view.getBigUint64(offset + 8, true));
    const selStart = view.getUint32(offset + 16, true);
    const selEnd = view.getUint32(offset + 20, true);
    const compStart = view.getUint32(offset + 24, true);
    const compEnd = view.getUint32(offset + 28, true);
    const len = view.getUint32(offset + 32, true);
    offset += 36;
    if (offset + len > bytes.byteLength) throw new Error("truncated edit value");
    const value = decodeUtf8(bytes, offset, offset + len);
    offset += len;
    events.push({
      type: "input",
      handler,
      handle,
      revision,
      value,
      selection: [selStart, selEnd],
      composition: compStart === 0xffffffff ? null : [compStart, compEnd],
    });
  }
  return events;
}

function decodeUtf8(bytes, start, end) {
  let out = "";
  let i = start;
  while (i < end) {
    const b = bytes[i++];
    if (b < 0x80) out += String.fromCharCode(b);
    else if (b < 0xe0) {
      out += String.fromCharCode(((b & 0x1f) << 6) | (bytes[i++] & 0x3f));
    } else if (b < 0xf0) {
      const b2 = bytes[i++];
      const b3 = bytes[i++];
      out += String.fromCharCode(((b & 0x0f) << 12) | ((b2 & 0x3f) << 6) | (b3 & 0x3f));
    } else {
      const b2 = bytes[i++];
      const b3 = bytes[i++];
      const b4 = bytes[i++];
      let cp = ((b & 7) << 18) | ((b2 & 0x3f) << 12) | ((b3 & 0x3f) << 6) | (b4 & 0x3f);
      cp -= 0x10000;
      out += String.fromCharCode(0xd800 + (cp >> 10), 0xdc00 + (cp & 0x3ff));
    }
  }
  return out;
}
