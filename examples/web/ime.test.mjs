import assert from "node:assert/strict";
import test from "node:test";
import { commandsForDom, menuActions } from "./ime.js";

test("insert, paste, and delete map to edit actions", () => {
  assert.deepEqual(commandsForDom({ type: "beforeinput", inputType: "insertText", data: "a" }), [
    { action: 1, text: "a" },
  ]);
  assert.deepEqual(commandsForDom({ type: "beforeinput", inputType: "insertFromPaste", data: "hi" }), [
    { action: 1, text: "hi" },
  ]);
  assert.deepEqual(commandsForDom({ type: "beforeinput", inputType: "deleteContentBackward" }), [
    { action: 2, text: "" },
  ]);
  assert.deepEqual(commandsForDom({ type: "beforeinput", inputType: "deleteContentForward" }), [
    { action: 3, text: "" },
  ]);
  assert.deepEqual(commandsForDom({ type: "beforeinput", inputType: "historyUndo" }), [
    { action: 8, text: "" },
  ]);
  assert.deepEqual(commandsForDom({ type: "beforeinput", inputType: "historyRedo" }), [
    { action: 9, text: "" },
  ]);
});

test("composition is one path and beforeinput composition text is dropped", () => {
  assert.deepEqual(commandsForDom({ type: "compositionstart" }), [{ action: 5, text: "" }]);
  assert.deepEqual(commandsForDom({ type: "compositionupdate", data: "中" }), [{ action: 6, text: "中" }]);
  assert.deepEqual(commandsForDom({ type: "compositionend", data: "中" }), [{ action: 7, text: "中" }]);
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
    commands: [{ action: 2, text: "", a: 0, b: 0 }],
  });
  assert.deepEqual(menuActions(field, "paste", "x"), {
    clipboard: "x",
    commands: [{ action: 1, text: "x", a: 0, b: 0 }],
  });
  assert.deepEqual(menuActions({ ...field, selection: [0, 0] }, "selectAll"), {
    clipboard: "",
    commands: [{ action: 4, text: "", a: 0, b: 4 }],
  });
  assert.deepEqual(menuActions({ ...field, secure: true }, "copy"), { clipboard: "", commands: [] });
  assert.equal(menuActions(field, "bold"), null);
});
