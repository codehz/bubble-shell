import assert from "node:assert/strict";
import test from "node:test";
import { scrollActionForKey } from "../../sdk/web/a11y.js";

function tree(leaf = {}, scroller = {}) {
  const nodes = new Map([
    [1, { handle: 1, parent: 0, actions: 8 | 16 | 128 | 256, ...scroller }],
    [2, { handle: 2, parent: 1, actions: 0, ...leaf }],
  ]);
  return { nodes, leaf: nodes.get(2) };
}

function intent(key, state, modifiers = {}) {
  return scrollActionForKey({ key, ...modifiers }, state.leaf, state.nodes);
}

test("static descendants can page a scrollable semantic ancestor", () => {
  const state = tree();
  assert.equal(intent("PageDown", state), 3);
  assert.equal(intent("PageUp", state), 4);
  assert.equal(intent("ArrowDown", state), 8);
  assert.equal(intent("ArrowLeft", state), undefined);
  assert.equal(intent("Home", state), undefined);
});

test("each axis resolves independently through nested scrollers", () => {
  const state = tree({}, { actions: 8 | 16 | 32 | 64, parent: 3 });
  state.nodes.set(3, { handle: 3, parent: 0, actions: 128 | 256 });
  assert.equal(intent("ArrowRight", state), 6);
  assert.equal(intent("ArrowUp", state), 7);
  state.nodes.delete(3);
  assert.equal(intent("ArrowUp", state), undefined);
});

test("editable nodes retain native caret and page navigation", () => {
  const state = tree({ editable: true });
  for (const key of ["PageDown", "PageUp", "ArrowUp", "ArrowDown"]) {
    assert.equal(intent(key, state), undefined);
  }
});

test("hidden and disabled nodes cannot scroll through an unavailable ancestor", () => {
  for (const flag of ["hidden", "disabled"]) {
    assert.equal(intent("PageDown", tree({ [flag]: true })), undefined);
    assert.equal(intent("PageDown", tree({}, { [flag]: true })), undefined);
  }
});

test("a modal subtree can scroll despite its hidden background ancestor", () => {
  const state = tree({}, { parent: 3 });
  state.nodes.set(3, { parent: 0, hidden: true });
  assert.equal(intent("PageDown", state), 3);
});

test("screen-reader shortcuts and composition keep their key events", () => {
  const state = tree();
  for (const modifier of ["altKey", "ctrlKey", "metaKey", "shiftKey", "isComposing"]) {
    assert.equal(intent("PageDown", state, { [modifier]: true }), undefined);
  }
});
