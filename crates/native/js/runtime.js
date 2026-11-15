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
const gestureOrderOfTarget = new Map();
const gestureBindings = new Map();
const gestureHandlerIds = new Map();
const retiredGestureHandlers = new Map();
const styleOrderOfTarget = new Map();
const virtualTargetOf = new Map();
const virtualScopeOf = new Map();
const virtualRoots = new Map();
let currentVirtualScope = null;
let leafStyles = false;
const encoder = typeof TextEncoder === "function" ? new TextEncoder() : null;

export function setApi(next) {
  api = next;
}

// Internal wrapper the compiler emits around primitive prop expressions that
// read signals; `pack` / `packGesture` split these out and `bindProp` /
// `bindGestureProp` own their lifecycle. Component props are getters instead,
// so Dynamic is not part of the author-facing API.
class Dynamic {
  constructor(run) {
    this.run = run;
  }
}

// Compiler-facing wrapper for primitive props that read signals. The compiler
// emits these calls from `bubble:rt`; author code never imports it.
export function dynamic(run) {
  if (typeof run !== "function") throw new Error("dynamic expects a function");
  return new Dynamic(run);
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
  scheduleFlush();
}

// Writes that happen outside an event batch (async completions, timers) still
// queue ops in the shell; commit them on the next microtask or they never
// paint. Batch-driven entry points flush themselves and skip the schedule.
let flushScheduled = false;
function scheduleFlush() {
  if (flushScheduled || batchDepth > 0) return;
  flushScheduled = true;
  queueMicrotask(() => {
    flushScheduled = false;
    if (batchDepth > 0) return;
    try {
      flush();
    } catch (error) {
      globalThis.__bubbleLastError = String(error && error.message || error);
      globalThis.__bubbleLastErrorStack = String(error && error.stack || error).slice(0, 1200);
      console.error(error);
    }
  });
}

// `bubble dev` seeds these values so a remount keeps signal state. Signals
// are re-allocated in the same order, so ids line up with the snapshot.
let pendingSignalValues = null;

export function snapshotSignals() {
  return signalValues.slice();
}

function resetRenderState() {
  parents.length = 1;
  parents[0] = 1;
  handlers.clear();
  handlersOfNode.clear();
  bindingHandles.clear();
  childrenOf.clear();
  childrenOf.set(1, []);
  parentOf.clear();
  cursors.length = 0;
  collectors.length = 0;
  nextHandler = 1;
  gestureHandlers.clear();
  fieldHandlers.clear();
  gestureOrderOfTarget.clear();
  gestureBindings.clear();
  gestureHandlerIds.clear();
  retiredGestureHandlers.clear();
  styleOrderOfTarget.clear();
  resetUiPrograms();
  virtualTargetOf.clear();
  virtualScopeOf.clear();
  virtualRoots.clear();
  leafStyles = false;
  currentVirtualScope = newVirtualScope(1, null);
  virtualRoots.set(1, currentVirtualScope);
  bodyError = null;
}

export function signal(initial, options) {
  const id = api.signal_alloc();
  if (!id) throw new Error(api.error() || "signal alloc failed");
  signalValues[id] =
    pendingSignalValues && id < pendingSignalValues.length
      ? pendingSignalValues[id]
      : initial;
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

let batchDepth = 0;

export function batch(fn) {
  const previous = bodyFn;
  batchDepth += 1;
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
    batchDepth -= 1;
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
    globalThis.__bubbleLastError = String(error && error.message || error);
    globalThis.__bubbleLastErrorStack = String(error && error.stack || error).slice(0, 1200);
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

function cssSides(value, key) {
  if (!Array.isArray(value) || (value.length !== 1 && value.length !== 2 && value.length !== 4)) {
    throw new Error(`${key} must be an array of 1, 2, or 4 lengths`);
  }
  const top = value[0] ?? 0;
  const end = value.length === 1 ? top : (value[1] ?? 0);
  const bottom = value.length === 4 ? (value[2] ?? 0) : top;
  const start = value.length === 4 ? (value[3] ?? 0) : end;
  return { top, end, bottom, start };
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

// Authored shorthands are applied before axis overrides, regardless of object
// key order. A dynamic shorthand shares one binding with the authored axes.
// Explicit `setProp` calls keep call order until that authored binding runs.
function shorthandRank(key) {
  if (key === "overflow") return [0, 0];
  if (key === "overflowX") return [0, 1];
  if (key === "overflowY") return [0, 2];
  if (key === "gap") return [1, 0];
  if (key === "rowGap") return [1, 1];
  if (key === "columnGap") return [1, 2];
  return null;
}

function applyAuthored(handle, prop) {
  setProp(handle, prop.key, prop.run ? prop.run() : prop.value);
}

function applyShorthandGroup(handle, group) {
  const shorthand = group[0];
  const axes = group.slice(1).filter(Boolean);
  if (shorthand && shorthand.run) {
    const authored = [shorthand, ...axes];
    addBinding(() => {
      for (const prop of authored) applyAuthored(handle, prop);
    }, handle);
  } else {
    if (shorthand) applyAuthored(handle, shorthand);
    for (const axis of axes) {
      if (axis.run) bindProp(handle, axis.key, axis.run);
      else applyAuthored(handle, axis);
    }
  }
}

function applyNodeProps(handle, values, dynamics) {
  const staticRest = [];
  const groups = [
    [null, null, null],
    [null, null, null],
  ];
  for (const [key, value] of values) {
    const rank = shorthandRank(key);
    if (!rank) staticRest.push([key, value]);
    else groups[rank[0]][rank[1]] = { key, value };
  }
  const dynamicRest = [];
  for (const [key, run] of dynamics) {
    const rank = shorthandRank(key);
    if (!rank) dynamicRest.push([key, run]);
    else groups[rank[0]][rank[1]] = { key, run };
  }
  for (const [key, value] of staticRest) setProp(handle, key, value);
  for (const group of groups) applyShorthandGroup(handle, group);
  for (const [key, run] of dynamicRest) bindProp(handle, key, run);
}

function refValue(value) {
  if (typeof value !== "object" || value === null || typeof value.current !== "number") {
    throw new Error("ref must come from ref()");
  }
  return value;
}

// Point a ref at the declaration it just spawned and flush the shared values
// created before it existed. View refs resolve to node handles; gesture refs
// resolve to gesture declaration ids, a separate id space.
function resolveRef(binding, handle) {
  binding.current = handle;
  const pending = binding.pending;
  if (!pending) return;
  binding.pending = null;
  for (const varId of pending) {
    if (api.bind_var(varId, handle) !== 0) throw new Error(api.error() || "bind_var failed");
  }
}

function pack(kind, props, text) {
  props = props ?? {};
  const parentHandle = parent();
  const cursor = cursors[cursors.length - 1];
  const values = [];
  const dynamics = [];
  let refBinding = null;
  for (const key of Object.keys(props)) {
    const value = props[key];
    if (key === "ref") {
      refBinding = refValue(value);
      continue;
    }
    if (value instanceof Dynamic) dynamics.push([key, value.run]);
    else values.push([key, value]);
  }
  let handle;
  if (cursor && cursor.parent === parentHandle) {
    handle = api.spawn_at(parentHandle, cursor.index, kind, text);
    if (!handle) throw new Error(api.error() || "spawn_at failed");
    registerChild(parentHandle, handle, cursor.index);
    cursor.index += 1;
  } else {
    handle = api.spawn(parentHandle, kind, text);
    if (!handle) throw new Error(api.error() || "spawn failed");
    registerChild(parentHandle, handle);
  }
  if (refBinding) resolveRef(refBinding, handle);
  applyNodeProps(handle, values, dynamics);
  return handle;
}

export function view(props, build) {
  const handle = pack(1, props, "");
  const scope = currentVirtualScope;
  currentVirtualScope = rootVirtualScope(handle);
  parents.push(handle);
  try {
    build();
  } finally {
    parents.pop();
    currentVirtualScope = scope;
  }
  return handle;
}

export function text(props, content, buildStyles) {
  let handle;
  if (typeof content === "function") {
    handle = pack(2, props, "");
    bind(handle, content);
  } else {
    handle = pack(2, props, content == null ? "" : String(content));
  }
  buildLeafStyles(handle, buildStyles);
  return handle;
}

export function image(props, buildStyles) {
  const handle = pack(3, props, "");
  buildLeafStyles(handle, buildStyles);
  return handle;
}

export function textField(props, buildStyles) {
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
  buildLeafStyles(handle, buildStyles);
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

// Place a render thunk (component children or a named slot) at this position.
// The thunk runs once per invocation and emits into the current parent; the
// branch machinery of Show / For / Switch owns any later replacement of the
// emitted nodes. A missing thunk renders the fallback.
export function slot(thunk, fallback) {
  if (thunk == null) {
    if (typeof fallback === "function") fallback();
    return;
  }
  if (typeof thunk !== "function") throw new Error("slot expects a render function");
  thunk();
}

// Children-position element expression (`{pad}`, `{cond ? <A/> : <B/>}`): run
// the thunk once now and again whenever the signals it reads change, replacing
// the nodes it mounted (the same replaceBranch path Show uses). The thunk must
// mount nodes into the current parent; a thunk that yields no nodes renders
// nothing.
export function content(thunk) {
  if (typeof thunk !== "function") throw new Error("content expects a render function");
  const parentHandle = parent();
  const anchor = createAnchor();
  const scope = newVirtualScope(parentHandle, currentVirtualScope);
  currentVirtualScope.entries.push(scope);
  let branch = { parent: parentHandle, handles: [] };
  addBinding(() => {
    branch = replaceBranch(parentHandle, anchor, branch, thunk, scope);
  }, anchor);
  return anchor;
}

const primitives = new Map();

export function registerPrimitive(name, factory) {
  if (typeof factory !== "function") throw new Error("primitive factory must be a function");
  primitives.set(name, factory);
}

export function primitive(name, props, children, buildStyles) {
  const factory = primitives.get(name);
  if (!factory) throw new Error(`unknown primitive ${name}`);
  const STYLE_COMPONENTS = new Set(["ViewStyle", "TextStyle"]);
  if (leafStyles && !STYLE_COMPONENTS.has(name)) {
    throw new Error(`${name} cannot be built inside style declaration children`);
  }
  return factory(props, children, buildStyles);
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
  registerVirtual("gesture", id, target);
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
    // A worklet takes over the slot: the shell drops the JS handler and
    // events flow to the UI program instead.
    if (fn && typeof fn === "object" && typeof fn.__uiFunction === "number") {
      if (api.bind_gesture_program(id, slot, fn.__uiFunction) !== 0) {
        throw new Error(api.error() || "bind_gesture_program failed");
      }
      continue;
    }
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
  let refBinding = null;
  for (const key of Object.keys(props)) {
    if (key === "ref") {
      refBinding = refValue(props[key]);
      continue;
    }
    if (!(key in allowed)) throw new Error(`${type} has no property ${key}`);
  }
  const target = parent();
  const { id } = registerGesture(target, kind);
  if (refBinding) resolveRef(refBinding, id);
  for (const key of Object.keys(props)) {
    if (key === "ref") continue;
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
registerPrimitive("ViewStyle", viewStyle);
registerPrimitive("TextStyle", textStyle);

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
      setProp(handle, "end", sides.end);
      setProp(handle, "bottom", sides.bottom);
      setProp(handle, "start", sides.start);
    } else {
      const prefix = key === "padding" ? "padding" : "margin";
      setProp(handle, `${prefix}Start`, sides.start);
      setProp(handle, `${prefix}End`, sides.end);
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
  if (api.queue_text(handle, value) !== 0) throw new Error(api.error() || "text failed");
}

function deactivateGestureBindings(id) {
  const set = gestureBindings.get(id);
  if (!set) return;
  for (const binding of set) {
    if (api.dispose_node(binding) !== 0) throw new Error(api.error() || "dispose_node failed");
  }
  gestureBindings.delete(id);
}

// Gesture ids and state style ids are independent spaces that both start at 1,
// so the unified bookkeeping keys them by kind.
function virtualKey(kind, id) {
  return `${kind}:${id}`;
}

function virtualId(key) {
  return Number(key.slice(key.indexOf(":") + 1));
}

function disposeGesture(id) {
  const key = virtualKey("gesture", id);
  if (!virtualTargetOf.has(key)) return;
  const target = virtualTargetOf.get(key);
  unlinkVirtual(key);
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
    if (order.length === 0) gestureOrderOfTarget.delete(target);
  }
  if (api.dispose_gesture(id) !== 0) throw new Error(api.error() || "dispose gesture failed");
}

// ---------------------------------------------------------------------------
// Shared variables and style declarations
// ---------------------------------------------------------------------------

const SOURCE_CODES = {
  none: 0,
  pressed: 1,
  focused: 2,
  disabled: 3,
  editing: 4,
  // Pan translation from the target gesture declaration, logical px.
  "gesture.translationX": 5,
  "gesture.translationY": 6,
  // Pinch values from the target transform gesture declaration.
  "gesture.scale": 7,
  "gesture.rotation": 8,
  // Scroll offsets from the target scroll container, logical px.
  "scroll.x": 9,
  "scroll.y": 10,
};

// Settle requests per variable id; resolved when the shell reports the
// variable converged.
const settleWaiters = new Map();

/// Wire encoding of an animation spec. `anim` 0 immediate, 2 tween
/// (a=duration ms, c=easing code), 3 spring (a=stiffness, b=damping).
function animParts(anim) {
  if (anim == null) return [0, 0, 0, 0];
  if (typeof anim !== "object") throw new Error("anim must be a spec object");
  if (anim.type === "tween") {
    if (typeof anim.duration !== "number" || !(anim.duration > 0)) {
      throw new Error("tween requires a positive duration");
    }
    const easing = anim.easing == null || anim.easing === "linear" ? 0 : anim.easing === "easeInOut" ? 1 : null;
    if (easing == null) throw new Error(`unsupported tween easing ${anim.easing}`);
    return [2, anim.duration, 0, easing];
  }
  if (anim.type === "spring") {
    if (typeof anim.stiffness !== "number" || !(anim.stiffness > 0)) {
      throw new Error("spring requires a positive stiffness");
    }
    if (typeof anim.damping !== "number" || !(anim.damping >= 0)) {
      throw new Error("spring requires a non-negative damping");
    }
    return [3, anim.stiffness, anim.damping, 0];
  }
  throw new Error(`unsupported anim type ${anim.type}`);
}

/// Value the write converges to, unwrapped from an animation marker.
function animTarget(value) {
  if (value == null) throw new Error("animation target must be a number");
  return value.target;
}

// Opcode table for the style expression program; must match crates/core/src/expr.rs.
const EXPR_OPS = {
  add: 2,
  sub: 3,
  mul: 4,
  div: 5,
  neg: 6,
  lt: 7,
  le: 8,
  gt: 9,
  ge: 10,
  eq: 11,
  ne: 12,
  and: 13,
  or: 14,
  not: 15,
  select: 16,
  mix: 17,
  lerp: 18,
  clamp: 19,
  min: 20,
  max: 21,
  abs: 22,
};

// Enum name tables; the engine carries numeric codes only.
const ENUM_CODES = {
  textAlign: { left: 0, center: 1, right: 2 },
  fit: { contain: 0, cover: 1, fill: 2 },
};

// Per-component paint whitelists; must match crates/core/src/props.rs.
const VIEW_STYLE_PROPS = ["backgroundColor", "radius", "opacity", "fit", "translateX", "translateY", "scale", "rotate"];
const TEXT_STYLE_PROPS = ["color", "textAlign"];

export function ref(initial) {
  return { current: initial ?? 0 };
}

function resolveTarget(target) {
  if (target == null) return 0;
  if (typeof target === "number") return target;
  if (typeof target === "object" && typeof target.current === "number") return target.current;
  throw new Error("target must be a ref or a node handle");
}

/// One UI-thread shared value. `createSharedValue({ source, target })` binds
/// a state source the shell drives; sources `gesture.translationX` /
/// `gesture.translationY` / `gesture.scale` / `gesture.rotation` read the
/// target gesture declaration's live values (`scale` stays `1.0` and
/// translations `0` while idle), `scroll.x` / `scroll.y` read the target
/// container's scroll offsets. `createSharedValue(0)` makes a script-driven
/// variable written from JS with `value = ...`, or from a worklet with
/// `.value` reads and writes. Either form accepts `{ value }` for the
/// starting value.
export function createSharedValue(valueOrOptions, options) {
  if (typeof api.create_var !== "function") {
    throw new Error("host does not expose shared variables");
  }
  let opts = valueOrOptions;
  let initial;
  if (valueOrOptions !== null && typeof valueOrOptions === "object" && !("id" in valueOrOptions)) {
    initial = valueOrOptions.value;
  } else {
    initial = valueOrOptions;
    opts = options ?? {};
  }
  if (initial == null) initial = 0;
  const source = (opts && opts.source) ?? "none";
  if (!(source in SOURCE_CODES)) throw new Error(`unknown var source ${source}`);
  const [tag, number] = propParts(initial);
  const id = api.create_var(SOURCE_CODES[source], tag, number);
  if (!id) throw new Error(api.error() || "create_var failed");
  const targetRef = opts && opts.target;
  const target = resolveTarget(targetRef);
  if (target != 0) {
    if (api.bind_var(id, target) !== 0) throw new Error(api.error() || "bind_var failed");
  } else if (typeof targetRef === "object" && targetRef !== null) {
    // The target node does not exist yet: flush the binding when its ref
    // resolves during spawn.
    (targetRef.pending ??= []).push(id);
  }
  const self = { id, current: target, source };
  if (source !== "none") {
    // The shell owns the value; writes would fight the source driver.
    self.set = () => {
      throw new Error("source-driven variables are written by the UI thread");
    };
    Object.defineProperty(self, "value", {
      get() { return undefined; },
      set() { throw new Error("source-driven variables are written by the UI thread"); },
    });
  } else {
    self.set = (value) => {
      const [valueTag, valueNumber] = propParts(animTarget(value) ?? value);
      const [animCode, animA, animB, animC] = animParts(value && value.__anim ? value.spec : null);
      if (api.set_var(id, valueTag, valueNumber, animCode, animA, animB, animC) !== 0) {
        throw new Error(api.error() || "set_var failed");
      }
    };
    Object.defineProperty(self, "value", {
      get() {
        throw new Error("shared values cannot be read on the JS thread; use scheduleOnUI");
      },
      set(value) { self.set(value); },
    });
    self.bind = (run) => {
      if (typeof run !== "function") throw new Error("bind requires a function");
      addBinding(() => self.set(run()));
    };
  }
  self.settle = () => {
    if (api.settle_var(id) !== 0) throw new Error(api.error() || "settle_var failed");
    return new Promise((resolve) => {
      const waiters = settleWaiters.get(id);
      if (waiters) waiters.push(resolve);
      else settleWaiters.set(id, [resolve]);
    });
  };
  return self;
}

/// Start a tween to `target` when the marker is assigned to a shared value.
export function withTiming(target, options) {
  const duration = options && options.duration;
  const easing = options ? options.easing : undefined;
  const spec = { type: "tween", duration, easing };
  animParts(spec); // validate eagerly so author mistakes throw at the call.
  return { __anim: true, target, spec };
}

/// Start a spring to `target` when the marker is assigned to a shared value.
export function withSpring(target, options) {
  const spec = {
    type: "spring",
    stiffness: options ? options.stiffness : undefined,
    damping: options ? options.damping : undefined,
  };
  animParts(spec); // validate eagerly so author mistakes throw at the call.
  return { __anim: true, target, spec };
}

/// Stop a shared value's in-flight animation; the displayed value stays.
export function cancelAnimation(sv) {
  if (sv == null || typeof sv.id !== "number") {
    throw new Error("cancelAnimation expects a shared value");
  }
  if (api.cancel_anim(sv.id) !== 0) throw new Error(api.error() || "cancel_anim failed");
}

// ---------------------------------------------------------------------------
// UI worklets
// ---------------------------------------------------------------------------

// Handler ids for JS functions captured by worklets and called through
// scheduleOnJS. Each UI program retains the handlers it captures, so a
// redefinition keeps shared callbacks alive and frees the rest.
const uiCallbacks = new Map();
const uiCallbackRefs = new Map();
let uiCallbackIds = new WeakMap();
/// Compiler site key → the UI program that definition owns. A re-render
/// redefines the same site in place, so the program table is bounded by
/// source sites instead of growing once per render.
const uiSites = new Map();

function resetUiPrograms() {
  uiCallbacks.clear();
  uiCallbackRefs.clear();
  uiCallbackIds = new WeakMap();
  uiSites.clear();
}

function uiCallbackId(fn) {
  let id = uiCallbackIds.get(fn);
  if (id === undefined) {
    id = nextHandler++;
    uiCallbackIds.set(fn, id);
  }
  return id;
}

function retainUiCallbacks(fns) {
  for (const fn of fns) {
    const id = uiCallbackId(fn);
    const refs = uiCallbackRefs.get(id) ?? 0;
    uiCallbackRefs.set(id, refs + 1);
    if (refs === 0) uiCallbacks.set(id, fn);
  }
}

function releaseUiCallbacks(fns) {
  for (const fn of fns) {
    const id = uiCallbackId(fn);
    const refs = (uiCallbackRefs.get(id) ?? 0) - 1;
    if (refs > 0) {
      uiCallbackRefs.set(id, refs);
    } else {
      uiCallbackRefs.delete(id);
      uiCallbacks.delete(id);
    }
  }
}

function sameUiPayload(a, b) {
  if (a.length !== b.length) return false;
  for (let index = 0; index < a.length; index += 1) {
    if (a[index] !== b[index]) return false;
  }
  return true;
}

/// Compiler-facing: define one UI program. `params` is an array of parameter
/// name strings, `captures` an array of captured VALUES (shared values,
/// worklets, JS functions, or primitives), `body` the statement tree, and
/// `site` the compiler's stable per-definition key.
export function uiFunction(params, captures, body, site) {
  if (typeof api.define_ui_function !== "function") {
    throw new Error("host does not expose UI worklets");
  }
  if (!Array.isArray(params) || params.some((name) => typeof name !== "string")) {
    throw new Error("worklet parameters must be identifiers");
  }
  if (captures == null || typeof body !== "object" || body == null) {
    throw new Error("worklet body must be an encoded tree");
  }
  const callbacks = [];
  const entries = captures.map((value) => {
    if (value == null) throw new Error("worklet captured null");
    if (typeof value === "number") {
      if (!Number.isFinite(value)) throw new Error("worklet captured a non-finite number");
      return { tag: 1, number: value };
    }
    if (typeof value === "boolean") return { tag: 3, number: value ? 1 : 0 };
    if (typeof value === "string") return { tag: 7, text: value };
    if (typeof value === "object" && typeof value.id === "number") return { tag: 4, number: value.id };
    if (typeof value === "object" && typeof value.__uiFunction === "number") {
      return { tag: 5, number: value.__uiFunction };
    }
    if (typeof value === "function") {
      callbacks.push(value);
      return { tag: 6, number: uiCallbackId(value) };
    }
    throw new Error(`worklet captured an unsupported value ${typeof value}`);
  });
  const payload = encodeUiFunction(params.length, entries, body);
  const cached = site == null ? undefined : uiSites.get(site);
  if (cached && sameUiPayload(cached.payload, payload)) {
    // Same captures and code: keep the existing program.
    return { __uiFunction: cached.id };
  }
  const reuse = cached ? cached.id : 0;
  const id = api.define_ui_function(params.length, Array.from(payload), reuse);
  if (!id) throw new Error(api.error() || "define_ui_function failed");
  retainUiCallbacks(callbacks);
  if (cached) releaseUiCallbacks(cached.callbacks);
  if (site != null) uiSites.set(site, { id, payload, callbacks });
  return { __uiFunction: id };
}

/// Compiler-facing: queue one UI call with primitive arguments.
export function scheduleOnUI(fn, ...args) {
  if (fn == null || typeof fn.__uiFunction !== "number") {
    throw new Error("scheduleOnUI requires a worklet");
  }
  if (typeof api.run_ui_function !== "function") {
    throw new Error("host does not expose UI worklets");
  }
  const bytes = encodeUiArgs(args);
  if (api.run_ui_function(fn.__uiFunction, Array.from(bytes)) !== 0) {
    throw new Error(api.error() || "run_ui_function failed");
  }
}

/// Decode the `BSUE` batch and dispatch each queued call to its JS handler.
export function dispatchUiCalls(bytes) {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  if (bytes.byteLength < 10 || view.getUint8(0) !== 0x42 || view.getUint8(1) !== 0x53 ||
      view.getUint8(2) !== 0x55 || view.getUint8(3) !== 0x45) {
    throw new Error("bad ui call batch");
  }
  const version = view.getUint16(4, true);
  if (version !== 1) throw new Error(`unsupported ui call version ${version}`);
  const count = view.getUint32(6, true);
  let offset = 10;
  const calls = [];
  for (let index = 0; index < count; index += 1) {
    const callback = view.getUint32(offset, true);
    offset += 4;
    const argc = view.getUint8(offset);
    offset += 1;
    const args = [];
    for (let i = 0; i < argc; i += 1) {
      const tag = view.getUint8(offset);
      offset += 1;
      if (tag === 1) {
        args.push(view.getUint8(offset) !== 0);
        offset += 1;
      } else if (tag === 2) {
        args.push(view.getFloat32(offset, true));
        offset += 4;
      } else if (tag === 3) {
        const len = view.getUint32(offset, true);
        offset += 4;
        args.push(decodeUtf8(bytes, offset, offset + len));
        offset += len;
      } else if (tag === 4) {
        args.push(view.getUint32(offset, true));
        offset += 4;
      } else {
        throw new Error(`unknown ui call arg tag ${tag}`);
      }
    }
    calls.push({ callback, args });
  }
  let ran = false;
  batch(() => {
    for (const call of calls) {
      const fn = uiCallbacks.get(call.callback);
      if (!fn) continue;
      ran = true;
      fn(...call.args);
    }
  });
  if (ran) flush();
  return calls.length;
}

// UI program opcodes; must match crates/core/src/ui.rs.
const UI_OPS = {
  add: 2,
  sub: 3,
  mul: 4,
  div: 5,
  neg: 6,
  lt: 7,
  le: 8,
  gt: 9,
  ge: 10,
  eq: 11,
  ne: 12,
  and: 13,
  or: 14,
  not: 15,
  select: 16,
  mix: 17,
  lerp: 18,
  clamp: 19,
  min: 20,
  max: 21,
  abs: 22,
  floor: 45,
  ceil: 46,
  round: 47,
  sqrt: 48,
  pow: 49,
};
const UNARY_UI_OPS = new Set(["neg", "not", "abs", "floor", "ceil", "round", "sqrt"]);
const BINARY_UI_OPS = new Set([
  "add", "sub", "mul", "div", "lt", "le", "gt", "ge", "eq", "ne",
  "and", "or", "min", "max", "pow",
]);

/// Encode one worklet body tree into captures + consts + code bytes, in the
/// `encode_ui_function_payload` layout.
function encodeUiFunction(params, captures, body) {
  const capturesBytes = [];
  const u16 = (value) => [value & 0xff, (value >>> 8) & 0xff];
  const u32 = (value) => [
    value & 0xff,
    (value >>> 8) & 0xff,
    (value >>> 16) & 0xff,
    (value >>> 24) & 0xff,
  ];
  const f32 = (value) => {
    const buffer = new ArrayBuffer(4);
    new Float32Array(buffer)[0] = value;
    return [...new Uint8Array(buffer)];
  };
  for (const capture of captures) {
    capturesBytes.push(capture.tag);
    if (capture.tag === 1) capturesBytes.push(...f32(capture.number));
    else if (capture.tag === 2) capturesBytes.push(...u32(capture.number >>> 0));
    else if (capture.tag === 3) capturesBytes.push(capture.number & 0xff);
    else if (capture.tag >= 4 && capture.tag <= 6) capturesBytes.push(...u32(capture.number));
    else if (capture.tag === 7) {
      const bytes = utf8(capture.text);
      capturesBytes.push(...u32(bytes.length), ...bytes);
    }
  }
  const consts = [];
  const constIndex = (tag, number) => {
    const existing = consts.findIndex((c) => c[0] === tag && c[1] === number);
    if (existing >= 0) return existing;
    consts.push([tag, number]);
    return consts.length - 1;
  };
  const code = [];
  const byte = (value) => code.push(value & 0xff);
  const patch16 = (at, value) => {
    code[at] = value & 0xff;
    code[at + 1] = (value >>> 8) & 0xff;
  };

  function pushConst(tag, number) {
    byte(0);
    byte(constIndex(tag, number));
  }

  function emitExpr(node) {
    if (!Array.isArray(node)) {
      // Non-array leaves: numbers and bools fold into constants here.
      if (typeof node === "number") {
        if (!Number.isFinite(node)) throw new Error("worklets only accept finite numbers");
        pushConst(1, node);
        return;
      }
      if (typeof node === "boolean") {
        pushConst(3, node ? 1 : 0);
        return;
      }
      throw new Error(`unsupported worklet value ${typeof node}`);
    }
    const [op, ...args] = node;
    if (op === "const") {
      pushConst(1, args[0]);
      return;
    }
    if (op === "param") {
      byte(1);
      byte(args[0]);
      return;
    }
    if (op === "cap") {
      byte(32);
      code.push(...u16(args[0]));
      return;
    }
    if (op === "event") {
      byte(33);
      byte(args[0]);
      return;
    }
    if (op === "local") {
      byte(38);
      byte(args[0]);
      return;
    }
    if (op === "select") {
      emitExpr(args[0]);
      emitExpr(args[1]);
      emitExpr(args[2]);
      byte(16);
      return;
    }
    if (op === "mix" || op === "lerp" || op === "clamp") {
      // Push order matches core pops: t/value operands below the pair.
      emitExpr(args[0]);
      emitExpr(args[1]);
      emitExpr(args[2]);
      byte(UI_OPS[op]);
      return;
    }
    if (UNARY_UI_OPS.has(op)) {
      emitExpr(args[0]);
      byte(UI_OPS[op]);
      return;
    }
    if (BINARY_UI_OPS.has(op)) {
      emitExpr(args[0]);
      emitExpr(args[1]);
      byte(UI_OPS[op]);
      return;
    }
    if (op === "call") {
      for (const arg of args.slice(1)) emitExpr(arg);
      byte(39);
      code.push(...u16(args[0]));
      byte(args.length - 1);
      return;
    }
    if (op === "scheduleJs") {
      for (const arg of args.slice(1)) emitExpr(arg);
      byte(41);
      code.push(...u16(args[0]));
      byte(args.length - 1);
      return;
    }
    if (op === "cancelAnim") {
      byte(36);
      code.push(...u16(args[0]));
      return;
    }
    throw new Error(`unsupported worklet expression ${op}`);
  }

  function emitStatements(statements) {
    for (const statement of statements ?? []) emitStatement(statement);
  }

  function emitStatement(node) {
    if (!Array.isArray(node)) throw new Error("worklet statements must be encoded trees");
    const [op, ...args] = node;
    if (op === "let") {
      emitExpr(args[1]);
      byte(37);
      byte(args[0]);
      return;
    }
    if (op === "store") {
      emitExpr(args[1]);
      byte(34);
      code.push(...u16(args[0]));
      return;
    }
    if (op === "storeAnim") {
      // Stack layout, top first: value, spec code, spec params.
      // `args[1]` is the spec tree: ["tween", duration, easing] or
      // ["spring", stiffness, damping] — numbers the compiler baked in.
      const spec = args[1];
      const kind = spec?.[0];
      if (kind === "tween") {
        emitExpr(spec[1]);
        emitExpr(spec[2]);
        pushConst(1, 0);
      } else if (kind === "spring") {
        emitExpr(spec[1]);
        emitExpr(spec[2]);
        pushConst(1, 1);
      } else {
        throw new Error(`unsupported worklet animation ${kind}`);
      }
      emitExpr(args[2]);
      byte(35);
      code.push(...u16(args[0]));
      return;
    }
    if (op === "pop") {
      emitExpr(args[0]);
      byte(44);
      return;
    }
    if (op === "scheduleJs" || op === "cancelAnim") {
      // Statement-position calls that leave no stack value.
      emitExpr(node);
      return;
    }
    if (op === "return") {
      emitExpr(args[0]);
      byte(40);
      return;
    }
    if (op === "if") {
      emitExpr(args[0]);
      byte(43);
      code.push(...u16(0));
      const jumpAt = code.length - 2;
      emitStatements(args[1]);
      if (args[2] && args[2].length > 0) {
        byte(42);
        code.push(...u16(0));
        const elseJumpAt = code.length - 2;
        patch16(jumpAt, code.length);
        emitStatements(args[2]);
        patch16(elseJumpAt, code.length);
      } else {
        patch16(jumpAt, code.length);
      }
      return;
    }
    throw new Error(`unsupported worklet statement ${op}`);
  }

  emitStatements(body.statements ?? body);
  // A trailing explicit return keeps the validator's reachability simple.
  pushConst(1, 0);
  byte(40);

  const payload = [];
  payload.push(...u16(captures.length));
  payload.push(...capturesBytes);
  payload.push(consts.length & 0xff);
  for (const [tag, number] of consts) {
    payload.push(tag);
    if (tag === 1) payload.push(...f32(number));
    else payload.push(number & 0xff);
  }
  payload.push(...u32(code.length), ...code);
  return Uint8Array.from(payload);
}

/// Encode `scheduleOnUI` arguments: argc byte, then tag/payload pairs.
function encodeUiArgs(args) {
  const out = [Math.min(args.length, 255)];
  const u32 = (value) => out.push(
    value & 0xff,
    (value >>> 8) & 0xff,
    (value >>> 16) & 0xff,
    (value >>> 24) & 0xff,
  );
  for (const arg of args) {
    if (typeof arg === "boolean") {
      out.push(1, arg ? 1 : 0);
    } else if (typeof arg === "number") {
      if (!Number.isFinite(arg)) throw new Error("scheduleOnUI arguments must be finite");
      const buffer = new ArrayBuffer(4);
      new Float32Array(buffer)[0] = arg;
      out.push(2, ...new Uint8Array(buffer));
    } else if (typeof arg === "string") {
      const bytes = utf8(arg);
      out.push(3);
      u32(bytes.length);
      out.push(...bytes);
    } else {
      throw new Error(`scheduleOnUI argument must be a number, bool, or string, got ${typeof arg}`);
    }
  }
  return Uint8Array.from(out);
}

/// Marker wrapper the compiler emits around style declaration expressions.
export function styleExpr(tree) {
  return { __bubbleStyleExpr: tree };
}

/// Encode one expression tree node into program bytes. `enums` maps enum
/// names for the slot; `colorSlot` makes number leaves numeric colors.
/// Returns `{ program: Uint8Array }`, or `{ static }` when the tree folded to
/// a plain value.
function encodeStyleTree(tree, enums, colorSlot) {
  const consts = [];
  const intern = (value) => {
    const index = consts.findIndex((existing) => {
      if (existing.tag !== value.tag) return false;
      if (existing.tag === 4) return existing.text === value.text;
      return existing.number === value.number;
    });
    if (index >= 0) return index;
    consts.push(value);
    return consts.length - 1;
  };
  const hasVar = { value: false };
  const code = walk(tree);

  function leaf(value, enumKey, numberAsColor) {
    if (value == null) throw new Error("style expressions do not accept null");
    if (typeof value === "boolean") return [["c", intern({ tag: 3, number: value ? 1 : 0 })]];
    if (typeof value === "number") {
      if (!Number.isFinite(value)) throw new Error("style expressions only accept finite numbers");
      if (numberAsColor) {
        if (!Number.isInteger(value) || value < 0 || value > 0xffffffff) {
          throw new Error(`numeric color ${value} is not 0xAARRGGBB`);
        }
        return [["c", intern({ tag: 2, number: value })]];
      }
      return [["c", intern({ tag: 1, number: value })]];
    }
    if (typeof value === "string") {
      if (enumKey) {
        const table = ENUM_CODES[enumKey];
        if (!(value in table)) throw new Error(`unsupported ${enumKey} ${value}`);
        return [["c", intern({ tag: 1, number: table[value] })]];
      }
      return [["c", intern({ tag: 4, text: value })]];
    }
    if (typeof value === "object" && typeof value.id === "number") {
      hasVar.value = true;
      return [["l", value.id]];
    }
    throw new Error(`unsupported style expression value ${typeof value}`);
  }

  function walk(node) {
    if (!Array.isArray(node)) return leaf(node, null, colorSlot);
    const [op, ...args] = node;
    if (!(op in EXPR_OPS)) throw new Error(`unknown style expression ${op}`);
    const opcode = EXPR_OPS[op];
    if (op === "neg" || op === "not" || op === "abs") {
      const inner = walk1(args[0], null, false);
      return [...inner, ["o", opcode]];
    }
    const binary = op === "add" || op === "sub" || op === "mul" || op === "div"
      || op === "lt" || op === "le" || op === "gt" || op === "ge"
      || op === "eq" || op === "ne" || op === "and" || op === "or"
      || op === "min" || op === "max";
    if (binary) {
      const a = walk1(args[0], null, false);
      const b = walk1(args[1], null, false);
      return [...a, ...b, ["o", opcode]];
    }
    if (op === "select") {
      const cond = walk1(args[0], null, false);
      const a = walk1(args[1], enums, false);
      const b = walk1(args[2], enums, false);
      return [...cond, ...a, ...b, ["o", opcode]];
    }
    const ternary = op === "mix" || op === "lerp" || op === "clamp";
    if (ternary) {
      const asColor = colorSlot && op === "mix";
      const a = walk1(args[0], asColor, asColor);
      const b = walk1(args[1], asColor, asColor);
      const c = walk1(args[2], null, false);
      return [...a, ...b, ...c, ["o", opcode]];
    }
    throw new Error(`unknown style expression ${op}`);
  }

  function walk1(node, enumKey, numberAsColor) {
    if (!Array.isArray(node) && typeof node === "string" && enumKey) {
      return leaf(node, enumKey, numberAsColor);
    }
    return Array.isArray(node) ? walk(node) : leaf(node, enumKey, numberAsColor);
  }

  // A tree with no variables and no operations folds to the plain value.
  if (!hasVar.value && code.length === 1 && code[0][0] === "c") {
    return { static: constToValue(consts[code[0][1]], enums) };
  }

  function constToValue(value, enumKey) {
    if (value.tag === 3) return value.number !== 0;
    if (value.tag === 4) return value.text;
    return value.number;
  }

  function constBytes(value) {
    const out = [value.tag];
    if (value.tag === 4) {
      const bytes = utf8(value.text);
      out.push(bytes.length & 0xff, (bytes.length >>> 8) & 0xff, (bytes.length >>> 16) & 0xff, (bytes.length >>> 24) & 0xff);
      out.push(...bytes);
    } else {
      // Num consts are f32, Color consts are u32; both little-endian.
      if (value.tag === 2) {
        out.push(
          value.number & 0xff,
          (value.number >>> 8) & 0xff,
          (value.number >>> 16) & 0xff,
          (value.number >>> 24) & 0xff,
        );
      } else {
        const buffer = new ArrayBuffer(4);
        new Float32Array(buffer)[0] = value.number;
        out.push(...new Uint8Array(buffer));
      }
    }
    return out;
  }

  const bytes = [consts.length & 0xff];
  for (const value of consts) bytes.push(...constBytes(value));
  bytes.push(code.length & 0xff, (code.length >>> 8) & 0xff);
  for (const item of code) {
    if (item[0] === "c") {
      bytes.push(0, item[1]);
    } else if (item[0] === "l") {
      const id = item[1];
      bytes.push(1, id & 0xff, (id >>> 8) & 0xff, (id >>> 16) & 0xff, (id >>> 24) & 0xff);
    } else {
      bytes.push(item[1]);
    }
  }
  return { program: Uint8Array.from(bytes) };
}

function styleDecl(props, component) {
  if (typeof api.spawn_style_decl !== "function") {
    throw new Error("host does not expose style declarations");
  }
  props = props ?? {};
  if (props.target == null) throw new Error("style declarations require a target");
  const target = resolveTarget(props.target);
  if (!target) throw new Error("style declarations require a resolved target");
  const allowed = component === 1 ? VIEW_STYLE_PROPS : TEXT_STYLE_PROPS;
  for (const key of Object.keys(props)) {
    if (key === "target" || key === "when") continue;
    if (!allowed.includes(key)) throw new Error(`this style component has no property ${key}`);
  }
  let order = styleOrderOfTarget.get(target);
  if (!order) {
    order = [];
    styleOrderOfTarget.set(target, order);
  }
  const id = api.spawn_style_decl(target, component, order.length);
  if (!id) throw new Error(api.error() || "spawn_style_decl failed");
  order.push(id);
  registerVirtual("style", id, target);
  for (const key of Object.keys(props)) {
    if (key === "target") continue;
    const value = props[key];
    const tree = value && value.__bubbleStyleExpr ? value.__bubbleStyleExpr : value;
    const enums = key === "textAlign" || key === "fit" ? key : null;
    const colorSlot =
      (component === 1 && key === "backgroundColor") || (component === 2 && key === "color");
    const encoded = encodeStyleTree(tree, enums, colorSlot);
    if (encoded.static !== undefined) {
      const [tag, number, text] = propParts(encoded.static);
      if (api.set_style_prop(id, key, tag, number, text) !== 0) {
        throw new Error(api.error() || "set_style_prop failed");
      }
    } else {
      // Both hosts take the program bytes as a plain number array; programs
      // are small after the compiler folds constants.
      if (api.set_style_prop_program(id, key, Array.from(encoded.program)) !== 0) {
        throw new Error(api.error() || "set_style_prop_program failed");
      }
    }
  }
  return id;
}

function viewStyle(props) {
  return styleDecl(props, 1);
}

function textStyle(props) {
  return styleDecl(props, 2);
}

// One scope per real UI target: declarations created while building that
// target land in its root scope, and every control-flow node keeps a stable
// child scope so authored order survives an empty branch. Gestures and state
// styles share the tree; each kind keeps its own per-target order for the
// wire.
function newVirtualScope(target, parentScope) {
  return { target, parent: parentScope, entries: [] };
}

function rootVirtualScope(target) {
  let scope = virtualRoots.get(target);
  if (!scope) {
    scope = newVirtualScope(target, null);
    virtualRoots.set(target, scope);
  }
  return scope;
}

function flattenVirtualScope(scope, gestures, styles) {
  for (const entry of scope.entries) {
    if (typeof entry === "string") {
      const id = virtualId(entry);
      if (entry.startsWith("gesture:")) gestures.push(id);
      else styles.push(id);
    } else {
      flattenVirtualScope(entry, gestures, styles);
    }
  }
}

function registerVirtual(kind, id, target) {
  const scope = currentVirtualScope;
  const key = virtualKey(kind, id);
  virtualTargetOf.set(key, target);
  virtualScopeOf.set(key, scope);
  scope.entries.push(key);
}

function unlinkVirtual(key) {
  const scope = virtualScopeOf.get(key);
  virtualScopeOf.delete(key);
  if (!scope) return;
  const index = scope.entries.indexOf(key);
  if (index >= 0) scope.entries.splice(index, 1);
}


function disposeStyleDecl(id) {
  const key = virtualKey("style", id);
  if (!virtualTargetOf.has(key)) return;
  const target = virtualTargetOf.get(key);
  unlinkVirtual(key);
  const order = styleOrderOfTarget.get(target);
  if (order) {
    const index = order.indexOf(id);
    if (index >= 0) order.splice(index, 1);
    if (order.length === 0) styleOrderOfTarget.delete(target);
  }
  if (api.dispose_style_decl(id) !== 0) {
    throw new Error(api.error() || "dispose style declaration failed");
  }
}

function disposeVirtual(key) {
  const id = virtualId(key);
  if (key.startsWith("gesture:")) disposeGesture(id);
  else disposeStyleDecl(id);
}

// Drop a scope's declarations but keep the scope, so a reopened branch lands
// back in its authored slot.
function resetVirtualScope(scope) {
  for (const entry of [...scope.entries]) {
    if (typeof entry === "string") disposeVirtual(entry);
    else resetVirtualScope(entry);
  }
  scope.entries.length = 0;
}

function disposeVirtualScope(scope) {
  resetVirtualScope(scope);
  const parent = scope.parent;
  scope.parent = null;
  if (!parent) return;
  const index = parent.entries.indexOf(scope);
  if (index >= 0) parent.entries.splice(index, 1);
}

function disposeTargetVirtuals(handle) {
  const gestures = gestureOrderOfTarget.get(handle);
  if (gestures) for (const id of [...gestures]) disposeGesture(id);
  gestureOrderOfTarget.delete(handle);
  const styles = styleOrderOfTarget.get(handle);
  if (styles) for (const id of [...styles]) disposeStyleDecl(id);
  styleOrderOfTarget.delete(handle);
  const root = virtualRoots.get(handle);
  if (root) {
    root.entries.length = 0;
    virtualRoots.delete(handle);
  }
}

// Flatten every target's virtual order and move only the declarations that
// disagree with it. `index` is the final position after removal.
function syncVirtualOrders() {
  for (const [target, root] of virtualRoots) {
    const gestures = [];
    const styles = [];
    flattenVirtualScope(root, gestures, styles);
    syncOrder(target, gestures, gestureOrderOfTarget, "gesture");
    syncOrder(target, styles, styleOrderOfTarget, "state style");
  }
}

function syncOrder(target, desired, orders, label) {
  const order = orders.get(target);
  if (!order || order.length === 0) return;
  for (let index = 0; index < desired.length; index += 1) {
    if (order[index] === desired[index]) continue;
    const from = order.indexOf(desired[index]);
    if (from < 0) continue;
    order.splice(from, 1);
    order.splice(index, 0, desired[index]);
    const code =
      label === "gesture"
        ? api.move_gesture(desired[index], target, index)
        : api.move_style_decl(desired[index], target, index);
    if (code !== 0) throw new Error(api.error() || `move ${label} failed`);
  }
}

// A leaf primitive's children are style declarations only, and they target
// the leaf itself.
function buildLeafStyles(handle, buildStyles) {
  if (typeof buildStyles !== "function") return;
  const scope = currentVirtualScope;
  const leaf = leafStyles;
  currentVirtualScope = rootVirtualScope(handle);
  leafStyles = true;
  parents.push(handle);
  try {
    buildStyles();
  } finally {
    parents.pop();
    leafStyles = leaf;
    currentVirtualScope = scope;
  }
}

function disposeHandle(handle) {
  if (handle === 1) return;
  disposeTargetVirtuals(handle);
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
  const old = oldParent !== undefined ? childrenOf.get(oldParent) : undefined;
  const position = old ? old.indexOf(handle) : -1;
  if (oldParent === parentHandle && position === index) return;
  if (oldParent !== undefined) {
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

function renderAt(parentHandle, index, build, scope) {
  const collector = { parent: parentHandle, handles: [] };
  const outer = currentVirtualScope;
  collectors.push(collector);
  cursors.push({ parent: parentHandle, index });
  parents.push(parentHandle);
  if (scope) currentVirtualScope = scope;
  try {
    build();
  } finally {
    parents.pop();
    cursors.pop();
    collectors.pop();
    currentVirtualScope = outer;
  }
  return collector;
}

function createAnchor() {
  return pack(1, { width: 0, height: 0, flexGrow: 0, flexShrink: 0, position: "absolute" }, "");
}

// Reopening a branch leaves its declarations in the scope slot they occupied
// before, so authored order does not depend on when the branch was rebuilt.
function replaceBranch(parentHandle, anchor, branch, build, scope) {
  for (const handle of branch.handles) disposeHandle(handle);
  resetVirtualScope(scope);
  const next = { parent: parentHandle, handles: [] };
  if (typeof build === "function") {
    const created = renderAt(parentHandle, anchorIndex(parentHandle, anchor), build, scope);
    next.handles = created.handles;
  }
  return next;
}

export function show(whenFn, fallback, render) {
  if (typeof whenFn !== "function") throw new Error("Show requires a when accessor");
  if (typeof render !== "function") throw new Error("Show requires a render function");
  const parentHandle = parent();
  const anchor = createAnchor();
  const scope = newVirtualScope(parentHandle, currentVirtualScope);
  currentVirtualScope.entries.push(scope);
  let branch = { parent: parentHandle, handles: [] };
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
      scope,
    );
  }, anchor);
  return anchor;
}

// Shared For/Index placement. Existing groups sit directly before the anchor,
// so their move target is a final position: the anchor's index minus every
// handle still to be placed. New groups render at the running cursor end.
// `groupOf` returns the existing group for an entry, or undefined; `spawn`
// renders a missing group at `cursor` and returns its handle count.
function placeGroups(parentHandle, anchor, entries, existingHandles, groupOf, spawn) {
  let cursor = anchorIndex(parentHandle, anchor) - existingHandles;
  for (const entry of entries) {
    const group = groupOf(entry);
    if (group === undefined) {
      cursor += spawn(entry, cursor);
    } else {
      for (const handle of group.handles) {
        moveHandle(handle, parentHandle, cursor);
        cursor += 1;
      }
    }
  }
}

export function forEach(items, by, render) {
  if (typeof by !== "function") throw new Error("For requires a by function");
  if (typeof render !== "function") throw new Error("For requires a render function");
  const itemsFn = typeof items === "function" ? items : () => items;
  const parentHandle = parent();
  const anchor = createAnchor();
  const scope = newVirtualScope(parentHandle, currentVirtualScope);
  currentVirtualScope.entries.push(scope);
  const groups = new Map();
  const live = new Map();
  const disposeGroup = (entry) => {
    for (const handle of entry.handles) disposeHandle(handle);
    if (entry.scope) disposeVirtualScope(entry.scope);
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
    // Existing groups sit directly before the anchor; placeGroups owns the
    // final-position cursor math.
    for (const entry of next) live.set(entry.key, entry.item);
    placeGroups(
      parentHandle,
      anchor,
      next,
      next.reduce(
        (handles, entry) => handles + (groups.get(entry.key)?.handles.length ?? 0),
        0,
      ),
      (entry) => groups.get(entry.key),
      (entry, cursor) => {
        live.set(entry.key, entry.item);
        const key = entry.key;
        const read = () => {
          itemsFn();
          return live.get(key);
        };
        const groupScope = newVirtualScope(parentHandle, scope);
        scope.entries.push(groupScope);
        const group = renderAt(parentHandle, cursor, () => render(read, entry.index), groupScope);
        group.scope = groupScope;
        groups.set(key, group);
        return group.handles.length;
      },
    );
    scope.entries.length = 0;
    for (const entry of next) {
      const group = groups.get(entry.key);
      if (group && group.scope) scope.entries.push(group.scope);
    }
  }, anchor);
  return anchor;
}

export function indexEach(items, render) {
  if (typeof render !== "function") throw new Error("Index requires a render function");
  const itemsFn = typeof items === "function" ? items : () => items;
  const parentHandle = parent();
  const anchor = createAnchor();
  const scope = newVirtualScope(parentHandle, currentVirtualScope);
  currentVirtualScope.entries.push(scope);
  const groups = new Map();
  const disposeGroup = (entry) => {
    for (const handle of entry.handles) disposeHandle(handle);
    if (entry.scope) disposeVirtualScope(entry.scope);
  };
  addBinding(() => {
    const list = itemsFn() ?? [];
    for (const [index, entry] of groups) {
      if (index >= list.length) {
        disposeGroup(entry);
        groups.delete(index);
      }
    }
    // Same final-position cursor math as forEach, shared in placeGroups.
    // Groups are keyed by position, so placeGroups walks `list.keys()`.
    placeGroups(
      parentHandle,
      anchor,
      list.keys(),
      [...groups.values()].reduce(
        (handles, group) => handles + (group.scope ? group.handles.length : 0),
        0,
      ),
      (index) => groups.get(index),
      (index, cursor) => {
        const at = index;
        const read = () => (itemsFn() ?? [])[at];
        const groupScope = newVirtualScope(parentHandle, scope);
        scope.entries.push(groupScope);
        const group = renderAt(parentHandle, cursor, () => render(read, at), groupScope);
        group.scope = groupScope;
        groups.set(index, group);
        return group.handles.length;
      },
    );
    scope.entries.length = 0;
    for (const group of groups.values()) {
      if (group.scope) scope.entries.push(group.scope);
    }
  }, anchor);
  return anchor;
}

export function switchFlow(fallback, arms) {
  if (!Array.isArray(arms)) throw new Error("Switch requires matches");
  const parentHandle = parent();
  const anchor = createAnchor();
  const scope = newVirtualScope(parentHandle, currentVirtualScope);
  currentVirtualScope.entries.push(scope);
  let branch = { parent: parentHandle, handles: [] };
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
      scope,
    );
  }, anchor);
  return anchor;
}

export function flush() {
  if (bodyError) {
    const error = bodyError;
    bodyError = null;
    throw error;
  }
  syncVirtualOrders();
  if (api.commit() === 0 && api.error()) throw new Error(api.error());
  if (typeof api.layout === "function" && api.layout() !== 0) throw new Error(api.error() || "layout failed");
}

export function mount(component, retained) {
  resetRenderState();
  computations.clear();
  cleanups.length = 0;
  if (api.boot() !== 0) throw new Error(api.error() || "boot failed");
  pendingSignalValues = retained ? retained.slice() : null;
  api.prepare?.();
  bodyFn = () => {
    parents.length = 0;
    parents.push(1);
    component();
  };
  if (api.run_init() !== 0) throw new Error(api.error() || "run_init failed");
  pendingSignalValues = null;
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
globalThis.__bubbleVar = dispatchVarEvents;
globalThis.__bubbleUiCalls = dispatchUiCalls;

export function dispatchVarEvents(bytes) {
  const events = decodeVarEvents(bytes);
  for (const event of events) {
    const waiters = settleWaiters.get(event.var);
    settleWaiters.delete(event.var);
    if (waiters) for (const resolve of waiters) resolve();
  }
  return events.length;
}

function decodeVarEvents(bytes) {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  if (bytes.byteLength < 10 || view.getUint8(0) !== 0x42 || view.getUint8(1) !== 0x53 ||
      view.getUint8(2) !== 0x56 || view.getUint8(3) !== 0x45) {
    throw new Error("bad var event batch");
  }
  const version = view.getUint16(4, true);
  if (version !== 1) throw new Error(`unsupported var event version ${version}`);
  const count = view.getUint32(6, true);
  const events = [];
  let offset = 10;
  for (let index = 0; index < count; index += 1) {
    events.push({ var: view.getUint32(offset, true) });
    offset += 4;
  }
  return events;
}

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
  if (version !== 2) throw new Error(`unsupported edit event version ${version}`);
  const count = view.getUint32(6, true);
  const events = [];
  let offset = 10;
  for (let index = 0; index < count; index += 1) {
    if (offset + 48 > bytes.byteLength) throw new Error("truncated edit event");
    const handler = view.getUint32(offset, true);
    const handle = view.getUint32(offset + 4, true);
    const session = Number(view.getBigUint64(offset + 8, true));
    const revision = Number(view.getBigUint64(offset + 16, true));
    const anchor = view.getUint32(offset + 24, true);
    const active = view.getUint32(offset + 28, true);
    const compStart = view.getUint32(offset + 32, true);
    const compEnd = view.getUint32(offset + 36, true);
    const flags = view.getUint32(offset + 40, true);
    const deltaCount = view.getUint32(offset + 44, true);
    offset += 48;
    const deltas = [];
    for (let d = 0; d < deltaCount; d += 1) {
      if (offset + 12 > bytes.byteLength) throw new Error("truncated edit delta");
      const start = view.getUint32(offset, true);
      const end = view.getUint32(offset + 4, true);
      const len = view.getUint32(offset + 8, true);
      offset += 12;
      if (offset + len > bytes.byteLength) throw new Error("truncated edit delta text");
      deltas.push({ start, end, insert: decodeUtf8(bytes, offset, offset + len) });
      offset += len;
    }
    if (offset + 4 > bytes.byteLength) throw new Error("truncated edit value");
    const len = view.getUint32(offset, true);
    offset += 4;
    if (offset + len > bytes.byteLength) throw new Error("truncated edit value");
    const value = decodeUtf8(bytes, offset, offset + len);
    offset += len;
    events.push({
      type: "input",
      handler,
      handle,
      session,
      revision,
      deltas,
      value,
      selection: [anchor, active],
      composition: compStart === 0xffffffff ? null : [compStart, compEnd],
      composing: (flags & 1) !== 0,
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

export const storage = {
  get(key) { return globalThis.bubbleStorage.get(key); },
  set(key, value) { globalThis.bubbleStorage.set(key, value); },
  remove(key) { globalThis.bubbleStorage.remove(key); },
  keys() { return globalThis.bubbleStorage.keys(); },
};

// host.call returns a Promise on every backend. Delivery differs: on wasm
// bubbleHost.call runs on the same thread and usually settles within the
// tick; on native the request queues in Shared.requests and settles when the
// host answers via bubble_poll_request / bubble_finish_request. Callers must
// await; never rely on synchronous timing.
export const host = {
  get params() { return globalThis.bubbleHost.params; },
  call(name, data) { return globalThis.bubbleHost.call(name, data); },
  post(name, data) { globalThis.bubbleHost.post(name, data); },
  on(name, fn) { globalThis.bubbleHost.on(name, fn); },
  onBack(fn) { return globalThis.bubbleHost.onBack(fn); },
  offBack() { return globalThis.bubbleHost.offBack(); },
};

export function toast(message) {
  return host.call("toast", { message: String(message ?? "") });
}

export function openURL(url) {
  return host.call("openURL", { url: String(url) });
}

export function navigate(intent) {
  const body = intent && typeof intent === "object" ? intent : { intent: String(intent ?? "") };
  return host.call("navigate", body);
}

export const clipboard = {
  readText() { return host.call("clipboard.readText", null); },
  writeText(text) { return host.call("clipboard.writeText", { text: String(text ?? "") }); },
};

export function fetch(url, init) {
  return host.call("fetch", { url: String(url), method: init && init.method, body: init && init.body });
}
