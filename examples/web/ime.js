// DOM input events to `edit` action codes. Composition text is owned by the
// composition events; `beforeinput` insertCompositionText is ignored so a
// key is not applied twice.

export function commandsForDom(event) {
  if (event.type === "compositionstart") return [{ action: 5, text: "" }];
  if (event.type === "compositionupdate") return [{ action: 6, text: event.data ?? "" }];
  if (event.type === "compositionend") return [{ action: 7, text: event.data ?? "" }];
  if (event.type !== "beforeinput" || event.inputType === "insertCompositionText") return [];
  if (event.inputType === "insertText" || event.inputType === "insertFromPaste") {
    return [{ action: 1, text: event.data ?? "" }];
  }
  if (event.inputType === "deleteContentBackward") return [{ action: 2, text: "" }];
  if (event.inputType === "deleteContentForward") return [{ action: 3, text: "" }];
  if (event.inputType === "historyUndo") return [{ action: 8, text: "" }];
  if (event.inputType === "historyRedo") return [{ action: 9, text: "" }];
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
    return { clipboard: selected, commands: [{ action: 2, text: "", a: 0, b: 0 }] };
  }
  if (name === "paste") {
    if (!clipboard) return { clipboard: "", commands: [] };
    return { clipboard, commands: [{ action: 1, text: clipboard, a: 0, b: 0 }] };
  }
  if (name === "selectAll") {
    const len = utf16Len(field.value);
    if (len === 0 || (start === 0 && end === len)) return { clipboard: "", commands: [] };
    return { clipboard: "", commands: [{ action: 4, text: "", a: 0, b: len }] };
  }
  return null;
}
