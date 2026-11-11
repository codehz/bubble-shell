let wasm;

function memory() {
  return wasm.memory;
}

self.memory = memory;
self.wasm = null;
self.pass = (bytes) => {
  const ptr = wasm.alloc(bytes.length);
  new Uint8Array(wasm.memory.buffer, ptr, bytes.length).set(bytes);
  return ptr;
};

self.onmessage = async (event) => {
  try {
    if (event.data.type === "boot") {
      const instantiated = await WebAssembly.instantiate(event.data.wasm, {
        env: {
          host_measure() {},
        },
      });
      wasm = instantiated.instance.exports;
      self.wasm = wasm;
      importScripts(event.data.runtime);
      self.bubble.configure(event.data.nextHandle);
      importScripts(event.data.entry);
      postBatch();
      return;
    }
    if (event.data.type === "tap" && typeof self.onTap === "function") {
      self.onTap(event.data.handlerId, event.data.handle);
      postBatch();
    }
  } catch (err) {
    postMessage({ type: "error", message: err && err.stack ? err.stack : String(err) });
  }
};

function postBatch() {
  const batch = self.bubble.flush();
  if (!batch) return;
  postMessage({ type: "ops", buffer: batch.buffer }, [batch.buffer]);
}
