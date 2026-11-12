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
