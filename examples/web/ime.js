// DOM input events to editing transactions. Composition text is owned by the
// composition events; `beforeinput` insertCompositionText is ignored so a
// key is not applied twice.

export function commandsForDom(event) {
  if (event.type === "compositionstart") return [];
  if (event.type === "compositionupdate") {
    return [{ op: "compose", text: event.data ?? "" }];
  }
  if (event.type === "compositionend") {
    return [{ op: "commit", text: event.data ?? "" }];
  }
  if (event.type !== "beforeinput" || event.inputType === "insertCompositionText") return [];
  if (event.inputType === "insertText" || event.inputType === "insertFromPaste") {
    return [{ op: "commit", text: event.data ?? "" }];
  }
  if (event.inputType === "deleteContentBackward") return [{ op: "delete", before: 1, after: 0 }];
  if (event.inputType === "deleteContentForward") return [{ op: "delete", before: 0, after: 1 }];
  if (event.inputType === "historyUndo") return [{ op: "undo" }];
  if (event.inputType === "historyRedo") return [{ op: "redo" }];
  return [];
}

function utf16Len(value) {
  let count = 0;
  for (const ch of value) count += ch.codePointAt(0) > 0xffff ? 2 : 1;
  return count;
}

function utf16Slice(value, start, end) {
  let out = "";
  let unit = 0;
  for (const ch of value) {
    const width = ch.codePointAt(0) > 0xffff ? 2 : 1;
    if (unit >= end) break;
    if (unit + width > start && unit >= start) out += ch;
    unit += width;
  }
  return out;
}

/** Cut, copy, paste, and select-all for the focused field. */
export function menuActions(field, name, clipboard = "") {
  const [rawStart, rawEnd] = field.selection;
  const start = Math.min(rawStart, rawEnd);
  const end = Math.max(rawStart, rawEnd);
  const selected = utf16Slice(field.value, start, end);
  if (name === "copy") {
    if (field.secure || start === end) return { clipboard: "", commands: [] };
    return { clipboard: selected, commands: [] };
  }
  if (name === "cut") {
    if (field.secure || start === end) return { clipboard: "", commands: [] };
    return {
      clipboard: selected,
      commands: [{ op: "replace", start, end, text: "" }],
    };
  }
  if (name === "paste") {
    if (!clipboard) return { clipboard: "", commands: [] };
    return { clipboard, commands: [{ op: "commit", text: clipboard }] };
  }
  if (name === "selectAll") {
    const len = utf16Len(field.value);
    if (len === 0 || (start === 0 && end === len)) return { clipboard: "", commands: [] };
    return {
      clipboard: "",
      commands: [{ op: "select", anchor: 0, active: len }],
    };
  }
  return null;
}

/** Selection covering the pointer-down offset through the current hit. */
export function dragSelection(anchor, unit) {
  if (!Number.isInteger(anchor) || !Number.isInteger(unit) || anchor < 0 || unit < 0) return null;
  return { op: "select", anchor, active: unit };
}
