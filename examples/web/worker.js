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
  if (event.data.type === "boot") {
    const instantiated = await WebAssembly.instantiate(event.data.wasm, {
      env: {
        host_measure() {},
      },
    });
    wasm = instantiated.instance.exports;
    self.wasm = wasm;
    importScripts(event.data.entry);
    return;
  }
  if (event.data.type === "tap" && typeof self.onTap === "function") {
    self.onTap(event.data.handlerId, event.data.handle);
  }
};
