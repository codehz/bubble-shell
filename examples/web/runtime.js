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
const gesturesOfTarget = new Map();
const gestureOrderOfTarget = new Map();
const gestureParentOf = new Map();
const gestureBindings = new Map();
const gestureHandlerIds = new Map();
const retiredGestureHandlers = new Map();
const gestureOwners = new Map();
const sugarTargets = new Set();
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
  "scrollX",
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
  view.setUint8(offset, (props.scrollY ? 1 : 0) | (props.scrollX ? 2 : 0));
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
  offset += bytes.length;
  view.setUint32(offset, 0, true);
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
    if (typeof props.onPress !== "function") throw new Error("onPress must be a function");
    createSugarTap(handle, props.onPress);
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
  1: { onPressStart: 1, onPressEnd: 2, onPressCancel: 3, onTap: 4, enabled: 0 },
  2: { onPressStart: 1, onPressEnd: 2, onPressCancel: 3, onDoubleTap: 5, enabled: 0 },
  3: { onStart: 6, onEnd: 8, onCancel: 9, duration: 0, maxDistance: 0, reserveScroll: 0, enabled: 0 },
  4: {
    onStart: 6,
    onUpdate: 7,
    onEnd: 8,
    onCancel: 9,
    axis: 0,
    direction: 0,
    minDistance: 0,
    failCrossDistance: 0,
    reserveScroll: 0,
    enabled: 0,
  },
  5: {
    onStart: 6,
    onUpdate: 7,
    onEnd: 8,
    onCancel: 9,
    pan: 0,
    pinch: 0,
    rotate: 0,
    minDistance: 0,
    reserveScroll: 0,
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
    if (key === "onPressStart" && sugarTargets.has(parent())) {
      throw new Error("View onPress conflicts with TapGesture on the same target");
    }
  }
  if (kind === 1 && sugarTargets.has(parent())) {
    throw new Error("View onPress conflicts with TapGesture on the same target");
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

function createSugarTap(target, handler) {
  sugarTargets.add(target);
  const { id } = registerGesture(target, 1);
  const handlerId = nextHandler++;
  gestureHandlers.set(handlerId, handler);
  rememberGestureHandler(id, handlerId);
  if (api.bind_gesture_event(id, 4, handlerId) !== 0) {
    throw new Error(api.error() || "bind_gesture_event failed");
  }
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
    // Keep the handlers alive until the cancel batch from the shell has been
    // dispatched; otherwise a terminal notification would be dropped.
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
  sugarTargets.delete(handle);
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
  return collector.handles;
}

function createAnchor() {
  return pack(1, { width: 0, height: 0, flexGrow: 0, flexShrink: 0, position: "absolute" }, "");
}

export function when(cond, thenBranch, elseBranch) {
  const condFn = typeof cond === "function" ? cond : () => cond;
  const parentHandle = parent();
  const anchor = createAnchor();
  let branch = { parent: parentHandle, handles: [], gestures: [] };
  const disposeBranch = () => {
    for (const handle of branch.handles) disposeHandle(handle);
    for (const gesture of branch.gestures) disposeGesture(gesture);
    branch = { parent: parentHandle, handles: [], gestures: [] };
  };
  const render = () => {
    const anchors = new Map();
    for (const gesture of branch.gestures) {
      const target = gestureParentOf.get(gesture);
      const list = gestureOrderOfTarget.get(target);
      const index = list ? list.indexOf(gesture) : -1;
      if (index >= 0) anchors.set(target, Math.min(anchors.get(target) ?? index, index));
    }
    disposeBranch();
    const next = condFn() ? thenBranch : elseBranch;
    if (typeof next !== "function") return;
    branch = renderAt(parentHandle, anchorIndex(parentHandle, anchor), next);
    for (const [target, at] of anchors) {
      const list = gestureOrderOfTarget.get(target);
      if (!list) continue;
      const created = [...new Set(branch.gestures)].filter(
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
  };
  addBinding(render, anchor);
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
  const disposeGroup = (entry) => {
    for (const handle of entry.handles) disposeHandle(handle);
    for (const gesture of entry.gestures) disposeGesture(gesture);
  };
  const renderList = () => {
    const list = itemsFn() ?? [];
    const next = [];
    for (let index = 0; index < list.length; index += 1) {
      const item = list[index];
      next.push({ item, key: keyFn(item, index) });
    }
    const seen = new Set(next.map((entry) => entry.key));
    for (const [groupKey, entry] of groups) {
      if (!seen.has(groupKey)) {
        disposeGroup(entry);
        groups.delete(groupKey);
      }
    }
    let index = anchorIndex(parentHandle, anchor);
    const rank = new Map();
    for (const entry of next) {
      let group = groups.get(entry.key);
      if (group === undefined) {
        group = renderAt(parentHandle, index, () => render(entry.item, entry.key));
        groups.set(entry.key, group);
        index += group.handles.length;
      } else {
        for (const handle of group.handles) {
          moveHandle(handle, parentHandle, index);
          index += 1;
        }
      }
      rank.set(group, rank.size);
    }
    reorderScopedGestures(rank);
  };
  addBinding(renderList, anchor);
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
  batch(() => {
    for (const event of events) {
      const fn = gestureHandlers.get(event.handler);
      if (!fn) continue;
      ran = true;
      fn(event);
    }
  });
  if (ran) flush();
  if (retiredGestureHandlers.size > 0) {
    for (const ids of retiredGestureHandlers.values()) {
      for (const handler of ids) gestureHandlers.delete(handler);
    }
    retiredGestureHandlers.clear();
  }
  return events.length;
}

globalThis.__bubbleSetApi = setApi;
globalThis.__bubblePress = press;
globalThis.__bubbleGesture = dispatchGestureEvents;
