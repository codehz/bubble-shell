// Match the core action bits; pixel deltas and scroll chaining stay in Rust.
const SCROLL_KEYS = {
  PageDown: 3,
  PageUp: 4,
  ArrowLeft: 5,
  ArrowRight: 6,
  ArrowUp: 7,
  ArrowDown: 8,
};

/** Resolve a page-scroll intent from the accessibility focus's semantic path. */
export function scrollActionForKey(event, node, nodes) {
  if (event.altKey || event.ctrlKey || event.metaKey || event.shiftKey || event.isComposing) return;
  if (!node || node.editable) return;
  const action = SCROLL_KEYS[event.key];
  if (action === undefined) return;
  for (let current = node; current; current = nodes.get(current.parent)) {
    if (current.hidden || current.disabled) return;
    if ((current.actions & (1 << action)) !== 0) return action;
  }
}
