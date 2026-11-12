import assert from "node:assert/strict";
import test from "node:test";
import { commandsForDom, dragSelection, menuActions } from "./ime.js";

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

test("menu actions cut, copy, paste, and select all", () => {
  const field = { value: "a😀b", selection: [1, 3], secure: false };
  assert.deepEqual(menuActions(field, "copy"), { clipboard: "😀", commands: [] });
  assert.deepEqual(menuActions(field, "cut"), {
    clipboard: "😀",
    commands: [{ op: "replace", start: 1, end: 3, text: "" }],
  });
  assert.deepEqual(menuActions(field, "paste", "x"), {
    clipboard: "x",
    commands: [{ op: "commit", text: "x" }],
  });
  assert.deepEqual(menuActions({ ...field, selection: [0, 0] }, "selectAll"), {
    clipboard: "",
    commands: [{ op: "select", anchor: 0, active: 4 }],
  });
  assert.deepEqual(menuActions({ ...field, secure: true }, "copy"), { clipboard: "", commands: [] });
  assert.equal(menuActions(field, "bold"), null);
});

test("drag selection keeps the pointer-down offset", () => {
  assert.deepEqual(dragSelection(1, 4), { op: "select", anchor: 1, active: 4 });
  assert.equal(dragSelection(1, -1), null);
});
