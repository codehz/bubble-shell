// Script-thread runtime. Effects record ops; the worker flushes one BSOP batch
// after each turn. Repeated writes to the same slot collapse to the last value.
(() => {
  const encoder = new TextEncoder();
  const pending = new Map();
  let seq = 1;

  function signal(initial) {
    let value = initial;
    const subs = new Set();
    const read = () => {
      if (signal.tracking) signal.tracking.add(subs);
      return value;
    };
    const write = (next) => {
      if (Object.is(next, value)) return;
      value = next;
      for (const sub of [...subs]) sub();
    };
    return [read, write];
  }
  signal.tracking = null;

  function effect(fn) {
    let stop = () => {};
    const run = () => {
      stop();
      const subs = new Set();
      const previous = signal.tracking;
      signal.tracking = subs;
      try {
        fn();
      } finally {
        signal.tracking = previous;
      }
      const removers = [];
      for (const sub of subs) {
        sub.add(run);
        removers.push(() => sub.delete(run));
      }
      stop = () => {
        for (const remove of removers) remove();
      };
    };
    run();
  }

  function setText(handle, text) {
    pending.set(`t:${handle}`, { kind: "text", handle, text: String(text) });
  }

  function setColor(handle, key, rgba) {
    pending.set(`c:${handle}:${key}`, { kind: "color", handle, key, rgba: rgba >>> 0 });
  }

  function u32(value) {
    const bytes = new Uint8Array(4);
    new DataView(bytes.buffer).setUint32(0, value, true);
    return bytes;
  }

  function flush() {
    if (pending.size === 0) return null;
    const ops = [...pending.values()];
    pending.clear();
    const strings = [];
    const ids = new Map();
    const intern = (value) => {
      const existing = ids.get(value);
      if (existing !== undefined) return existing;
      const id = strings.length;
      strings.push(value);
      ids.set(value, id);
      return id;
    };
    const payload = [];
    const pushU32 = (value) => {
      for (const byte of u32(value)) payload.push(byte);
    };
    for (const op of ops) {
      if (op.kind === "color") {
        payload.push(1);
        pushU32(op.handle);
        pushU32(intern(op.key));
        payload.push(5);
        pushU32(op.rgba);
      } else {
        payload.push(2);
        pushU32(op.handle);
        pushU32(intern(op.text));
      }
    }
    const table = [];
    for (const value of strings) {
      const bytes = encoder.encode(value);
      for (const byte of u32(bytes.length)) table.push(byte);
      for (const byte of bytes) table.push(byte);
    }
    const header = new Uint8Array(22);
    const view = new DataView(header.buffer);
    header.set([0x42, 0x53, 0x4f, 0x50]);
    view.setUint16(4, 1, true);
    view.setUint32(6, seq, true);
    seq += 1;
    view.setUint32(10, ops.length, true);
    view.setUint32(14, table.length, true);
    view.setUint32(18, payload.length, true);
    const out = new Uint8Array(header.length + table.length + payload.length);
    out.set(header, 0);
    out.set(table, header.length);
    out.set(payload, header.length + table.length);
    return out;
  }

  self.bubble = { signal, effect, setText, setColor, flush };
})();
