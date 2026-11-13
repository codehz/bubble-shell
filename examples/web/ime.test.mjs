import assert from "node:assert/strict";
import test from "node:test";
import { commandsForDom, dragSelection } from "../../sdk/web/ime.js";

test("insert, paste, and delete map to edit transactions", () => {
  assert.deepEqual(commandsForDom({ type: "beforeinput", inputType: "insertText", data: "a" }), [
    { op: "commit", text: "a" },
  ]);
  assert.deepEqual(commandsForDom({ type: "beforeinput", inputType: "insertFromPaste", data: "hi" }), [
    { op: "commit", text: "hi" },
  ]);
  assert.deepEqual(commandsForDom({ type: "beforeinput", inputType: "deleteContentBackward" }), [
    { op: "delete", before: 1, after: 0 },
  ]);
  assert.deepEqual(commandsForDom({ type: "beforeinput", inputType: "deleteContentForward" }), [
    { op: "delete", before: 0, after: 1 },
  ]);
  assert.deepEqual(commandsForDom({ type: "beforeinput", inputType: "historyUndo" }), [
    { op: "undo" },
  ]);
  assert.deepEqual(commandsForDom({ type: "beforeinput", inputType: "historyRedo" }), [
    { op: "redo" },
  ]);
});

test("composition is one path and beforeinput composition text is dropped", () => {
  assert.deepEqual(commandsForDom({ type: "compositionstart" }), []);
  assert.deepEqual(commandsForDom({ type: "compositionupdate", data: "中" }), [
    { op: "compose", text: "中" },
  ]);
  assert.deepEqual(commandsForDom({ type: "compositionend", data: "中" }), [
    { op: "commit", text: "中" },
  ]);
  assert.deepEqual(
    commandsForDom({ type: "beforeinput", inputType: "insertCompositionText", data: "中" }),
    [],
  );
});

test("drag selection keeps the pointer-down offset", () => {
  assert.deepEqual(dragSelection(1, 4), { op: "select", anchor: 1, active: 4 });
  assert.equal(dragSelection(1, -1), null);
});
