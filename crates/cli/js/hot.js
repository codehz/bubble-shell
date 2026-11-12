// Development runtime for `bubble dev`.
//
// Modules arrive as CommonJS factories from the dev bundle. Components and
// top-level signals are registered under stable keys so re-running a module
// swaps implementations while state survives.

import * as natives from "bubble-shell";

const modules = new Map();
const components = new Map();
const keeps = new Map();
let mounted = null;
let started = false;
let updating = false;
let socket = null;

export function define(id, factory) {
  let entry = modules.get(id);
  if (!entry) {
    entry = { factory, exports: {} };
    modules.set(id, entry);
  } else {
    entry.factory = factory;
  }
  return entry;
}

export function reset() {
  modules.clear();
}

function resolveId(from, request) {
  if (!request.startsWith(".")) return request;
  const parts = from.split("/").slice(0, -1);
  for (const part of request.split("/")) {
    if (part === "" || part === ".") continue;
    if (part === "..") parts.pop();
    else parts.push(part);
  }
  return parts.join("/");
}

function execute(id, entry = modules.get(id)) {
  if (!entry) throw new Error(`hot module ${id} is not defined`);
  if (entry.running) return entry.exports;
  entry.running = true;
  try {
    entry.factory(
      (request) => requireFrom(id, request),
      { exports: entry.exports },
      entry.exports,
    );
  } finally {
    entry.running = false;
  }
  return entry.exports;
}

const hot = {
  component,
  keep,
  mount,
  define,
  run,
  update,
  reset,
  connect,
  remount,
};

function requireFrom(from, request) {
  if (!request.startsWith(".")) {
    if (request === "bubble-shell" || request === "bubble:rt") return natives;
    if (request === "bubble:hot") return hot;
    throw new Error(`unknown host module ${request} required from ${from}`);
  }
  const id = resolveId(from, request);
  return execute(id);
}

export function component(key, impl) {
  let entry = components.get(key);
  if (!entry) {
    entry = { impl };
    entry.proxy = (...args) => entry.impl(...args);
    components.set(key, entry);
  } else {
    entry.impl = impl;
  }
  return entry.proxy;
}

export function keep(key, create) {
  if (!keeps.has(key)) keeps.set(key, create());
  return keeps.get(key);
}

export function remount() {
  if (!mounted) return;
  const retained =
    typeof natives.snapshotSignals === "function" ? natives.snapshotSignals() : null;
  natives.mount(mounted.component, retained);
  globalThis.__bubbleRepaint?.();
}

export function mount(key, component) {
  mounted = { key, component };
  if (started && !updating) remount();
}

export function run(id) {
  execute(id);
  started = true;
  remount();
}

export function update(changes) {
  updating = true;
  try {
    for (const change of changes) {
      define(change.id, new Function("require", "module", "exports", change.code));
    }
    for (const change of changes) execute(change.id);

  } finally {
    updating = false;
  }
  remount();
}

function overlay(message) {
  let element = document.getElementById("__bubble_hot_error");
  if (!element) {
    element = document.createElement("pre");
    element.id = "__bubble_hot_error";
    Object.assign(element.style, {
      position: "fixed",
      left: "0",
      right: "0",
      bottom: "0",
      maxHeight: "50%",
      overflow: "auto",
      margin: "0",
      padding: "12px",
      background: "#3b0d0d",
      color: "#fecaca",
      font: "12px/1.4 ui-monospace, monospace",
      whiteSpace: "pre-wrap",
      zIndex: "10",
    });
    document.body.append(element);
  }
  element.textContent = message;
}

function clearOverlay() {
  document.getElementById("__bubble_hot_error")?.remove();
}

export function connect(url) {
  socket = new WebSocket(url);
  socket.addEventListener("message", (event) => {
    let message;
    try {
      message = JSON.parse(event.data);
    } catch (error) {
      return;
    }
    if (message.type === "update") {
      try {
        update(message.modules);
        clearOverlay();
      } catch (error) {
        console.error("hot update failed", error);
        location.reload();
      }
    } else if (message.type === "error") {
      overlay(message.message);
    } else if (message.type === "clear") {
      clearOverlay();
    } else if (message.type === "reload") {
      location.reload();
    }
  });
}

globalThis.__bubble_hot = { component, keep, mount, define, run, update, reset, connect, remount };
