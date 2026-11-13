// Drop-in page player. One player per document: the wasm shell is still one
// module. Paint, IME, and the accessibility overlay stay in host.js.

function installHost(options) {
  globalThis.__bubbleHostParams = options.params ?? null;
  const handlers = options.handle ?? {};
  const storageKey = `bubble.storage.${String(options.id ?? "bubble")}`;
  const storage = new Map();
  try {
    const raw = localStorage.getItem(storageKey);
    if (raw) {
      for (const [key, value] of Object.entries(JSON.parse(raw))) storage.set(key, String(value));
    }
  } catch {
    /* private mode or a non-browser test */
  }
  const save = () => {
    try {
      localStorage.setItem(storageKey, JSON.stringify(Object.fromEntries(storage)));
    } catch {
      /* ignore quota and private mode */
    }
  };
  globalThis.bubbleStorage = {
    get(key) {
      return storage.has(String(key)) ? storage.get(String(key)) : null;
    },
    set(key, value) {
      storage.set(String(key), String(value));
      save();
    },
    remove(key) {
      storage.delete(String(key));
      save();
    },
    keys() {
      return [...storage.keys()];
    },
  };
  globalThis.bubbleHost = {
    get params() {
      return globalThis.__bubbleHostParams;
    },
    async call(method, params) {
      if (method === "toast") {
        const node = document.createElement("div");
        node.textContent = String(params?.message ?? "");
        Object.assign(node.style, {
          position: "fixed",
          left: "50%",
          bottom: "24px",
          transform: "translateX(-50%)",
          background: "#111",
          color: "#fff",
          padding: "8px 12px",
          zIndex: "3",
        });
        document.body.append(node);
        setTimeout(() => node.remove(), 1600);
        return null;
      }
      if (method === "openURL") {
        window.open(String(params?.url ?? ""), "_blank", "noopener");
        return null;
      }
      if (method === "clipboard.readText") {
        return navigator.clipboard.readText();
      }
      if (method === "clipboard.writeText") {
        await navigator.clipboard.writeText(String(params?.text ?? ""));
        return null;
      }
      if (method === "fetch") {
        const response = await window.fetch(String(params?.url ?? ""), {
          method: params?.method,
          body: params?.body ?? undefined,
        });
        return { status: response.status, text: await response.text() };
      }
      const handler = handlers[method];
      if (!handler) throw new Error(`no host handler for ${method}`);
      return handler(params ?? null);
    },
    post(method, params) {
      void this.call(method, params).catch((error) => options.onError?.(String(error)));
    },
    on(name, fn) {
      let list = listeners.get(name);
      if (!list) {
        list = [];
        listeners.set(name, list);
      }
      list.push(fn);
      for (let i = queued.length - 1; i >= 0; i -= 1) {
        if (queued[i].name !== name) continue;
        const [event] = queued.splice(i, 1);
        fn(event.payload);
      }
    },
  };
  globalThis.__bubbleHostEmit = (name, payload) => {
    let value = payload;
    try {
      value = JSON.parse(payload);
    } catch {
      /* already a value */
    }
    const list = listeners.get(name);
    if (!list || list.length === 0) {
      queued.push({ name, payload: value });
      return;
    }
    for (const fn of list) fn(value);
  };
}

const listeners = new Map();
const queued = [];

export async function createPlayer(root, options = {}) {
  installHost(options);
  const canvas = document.createElement("canvas");
  canvas.tabIndex = 0;
  Object.assign(canvas.style, { width: "100%", height: "100%", display: "block", touchAction: "none" });
  root.append(canvas);
  try {
    await import("./host.js");
  } catch (error) {
    options.onError?.(String(error));
    throw error;
  }
  return {
    send(name, data) {
      globalThis.__bubbleHostEmit(name, JSON.stringify(data ?? null));
    },
    destroy() {
      canvas.remove();
    },
  };
}
