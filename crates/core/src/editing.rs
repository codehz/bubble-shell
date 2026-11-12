//! UI-thread text editing. Offsets are UTF-16 code units so IME agents on
//! iOS, Android, and the web share one selection space. A surrogate pair is
//! one scalar: a boundary inside the pair snaps backward, and backspace or
//! delete removes the whole scalar.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldState {
    pub sel_start: u32,
    pub sel_end: u32,
    pub composition: Option<(u32, u32)>,
    pub revision: u64,
    pub composing: bool,
}

impl FieldState {
    pub fn new(value: &str) -> Self {
        let end = utf16_len(value);
        Self {
            sel_start: end,
            sel_end: end,
            composition: None,
            revision: 0,
            composing: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EditCommand {
    Insert(String),
    Backspace,
    DeleteForward,
    SetSelection { start: u32, end: u32 },
    CompositionStart,
    CompositionUpdate(String),
    CompositionEnd(String),
    Undo,
    Redo,
}

const HISTORY_LIMIT: usize = 100;

#[derive(Clone, Debug)]
pub(crate) struct EditSnap {
    value: String,
    sel_start: u32,
    sel_end: u32,
}

#[derive(Clone, Debug, Default)]
pub struct EditHistory {
    undo: Vec<EditSnap>,
    redo: Vec<EditSnap>,
    composition_base: Option<EditSnap>,
}

impl EditHistory {

    /// Snapshot to commit if the command changes the field. Composition updates
    /// stay on the snapshot taken at `CompositionStart`.
    pub(crate) fn checkpoint(&mut self, command: &EditCommand, value: &str, sel_start: u32, sel_end: u32) -> Option<EditSnap> {
        let snap = EditSnap {
            value: value.to_owned(),
            sel_start,
            sel_end,
        };
        match command {
            EditCommand::Undo | EditCommand::Redo | EditCommand::CompositionUpdate(_) => None,
            EditCommand::CompositionStart => {
                self.composition_base = Some(snap);
                None
            }
            EditCommand::CompositionEnd(_) => self.composition_base.take(),
            _ => Some(snap),
        }
    }

    pub(crate) fn commit(&mut self, checkpoint: Option<EditSnap>, changed: bool) {
        let Some(snap) = checkpoint else {
            return;
        };
        if !changed {
            return;
        }
        self.redo.clear();
        self.undo.push(snap);
        if self.undo.len() > HISTORY_LIMIT {
            self.undo.remove(0);
        }
    }

    pub fn undo(&mut self, value: &str, sel_start: u32, sel_end: u32) -> Option<(String, u32, u32)> {
        self.composition_base = None;
        let snap = self.undo.pop()?;
        self.redo.push(EditSnap {
            value: value.to_owned(),
            sel_start,
            sel_end,
        });
        Some((snap.value, snap.sel_start, snap.sel_end))
    }

    pub fn redo(&mut self, value: &str, sel_start: u32, sel_end: u32) -> Option<(String, u32, u32)> {
        self.composition_base = None;
        let snap = self.redo.pop()?;
        self.undo.push(EditSnap {
            value: value.to_owned(),
            sel_start,
            sel_end,
        });
        Some((snap.value, snap.sel_start, snap.sel_end))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditEvent {
    pub handle: u32,
    pub handler: u32,
    pub revision: u64,
    pub value: String,
    pub sel_start: u32,
    pub sel_end: u32,
    pub composition: Option<(u32, u32)>,
}

/// `true` when `value` or the selection changed.
pub fn apply_command(
    state: &mut FieldState,
    value: &mut String,
    command: EditCommand,
    multiline: bool,
    max_length: u32,
) -> Result<bool, String> {
    match command {
        EditCommand::Insert(text) => {
            if state.composing {
                return Err("insert during composition; update the composition".into());
            }
            let text = sanitize(&text, multiline);
            Ok(replace_selection(state, value, &text, max_length, None))
        }
        EditCommand::Backspace => {
            if state.composing {
                return Err("backspace during composition; update the composition".into());
            }
            Ok(delete_selection(state, value, true))
        }
        EditCommand::DeleteForward => {
            if state.composing {
                return Err("delete during composition; update the composition".into());
            }
            Ok(delete_selection(state, value, false))
        }
        EditCommand::SetSelection { start, end } => {
            if state.composing {
                return Err("selection is owned by the composition".into());
            }
            let start = clamp_units(value, start);
            let end = clamp_units(value, end);
            if state.sel_start == start && state.sel_end == end {
                return Ok(false);
            }
            state.sel_start = start;
            state.sel_end = end;
            state.revision += 1;
            Ok(true)
        }
        EditCommand::CompositionStart => {
            state.composing = true;
            state.composition = None;
            Ok(false)
        }
        EditCommand::CompositionUpdate(text) => {
            if !state.composing {
                return Err("composition update without start".into());
            }
            let text = sanitize(&text, multiline);
            Ok(replace_selection(
                state,
                value,
                &text,
                max_length,
                Some(true),
            ))
        }
        EditCommand::CompositionEnd(text) => {
            if !state.composing {
                return Err("composition end without start".into());
            }
            let text = sanitize(&text, multiline);
            let before = state.revision;
            replace_selection(state, value, &text, max_length, Some(false));
            state.composing = false;
            state.composition = None;
            if state.revision == before {
                state.revision += 1;
            }
            Ok(true)
        }
        EditCommand::Undo | EditCommand::Redo => Err("undo history is applied by the shell".into()),
    }
}

/// Script-driven value. Returns whether it was applied.
///
/// `epoch` is the revision the script observed. An older epoch is ignored.
/// An open composition is ignored unless `cancel` is set.
pub fn apply_script(
    state: &mut FieldState,
    value: &mut String,
    epoch: u64,
    next: &str,
    sel_start: u32,
    sel_end: u32,
    cancel: bool,
    multiline: bool,
    max_length: u32,
) -> bool {
    if state.composing && !cancel {
        return false;
    }
    if epoch < state.revision {
        return false;
    }
    let limit = if max_length == 0 { u32::MAX } else { max_length };
    let next = sanitize(next, multiline);
    let next = fit(&next, limit);
    *value = next;
    state.composing = false;
    state.composition = None;
    state.sel_start = clamp_units(value, sel_start);
    state.sel_end = clamp_units(value, sel_end);
    state.revision += 1;
    true
}

pub fn display_text(value: &str, secure: bool) -> String {
    if !secure {
        return value.to_owned();
    }
    "•".repeat(utf16_len(value) as usize)
}

pub fn encode_edit_events(events: &[EditEvent]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"BSED");
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&(events.len() as u32).to_le_bytes());
    for event in events {
        out.extend_from_slice(&event.handler.to_le_bytes());
        out.extend_from_slice(&event.handle.to_le_bytes());
        out.extend_from_slice(&event.revision.to_le_bytes());
        out.extend_from_slice(&event.sel_start.to_le_bytes());
        out.extend_from_slice(&event.sel_end.to_le_bytes());
        let (comp_start, comp_end) = event.composition.unwrap_or((u32::MAX, u32::MAX));
        out.extend_from_slice(&comp_start.to_le_bytes());
        out.extend_from_slice(&comp_end.to_le_bytes());
        let bytes = event.value.as_bytes();
        out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        out.extend_from_slice(bytes);
    }
    out
}

fn replace_selection(
    state: &mut FieldState,
    value: &mut String,
    insert: &str,
    max_length: u32,
    composition: Option<bool>,
) -> bool {
    let (start, end) = if let Some(range) = state.composition {
        range
    } else {
        ordered(state.sel_start, state.sel_end)
    };
    let start_byte = boundary(value, start);
    let end_byte = boundary(value, end);
    let outside = utf16_len(&value[..start_byte]) + utf16_len(&value[end_byte..]);
    let room = if max_length == 0 {
        u32::MAX
    } else {
        max_length.saturating_sub(outside)
    };
    let insert = fit(insert, room);
    if insert.is_empty() && start_byte == end_byte && composition.is_none() {
        return false;
    }
    let mut next = String::new();
    next.push_str(&value[..start_byte]);
    next.push_str(&insert);
    next.push_str(&value[end_byte..]);
    let cursor = utf16_len(&value[..start_byte]) + utf16_len(&insert);
    let comp = match composition {
        Some(true) => Some((utf16_len(&value[..start_byte]), cursor)),
        Some(false) | None => None,
    };
    if &next == value && state.sel_start == cursor && state.sel_end == cursor && state.composition == comp
    {
        return false;
    }
    *value = next;
    state.sel_start = cursor;
    state.sel_end = cursor;
    state.composition = comp;
    state.revision += 1;
    true
}

fn delete_selection(state: &mut FieldState, value: &mut String, backspace: bool) -> bool {
    let (start, end) = ordered(state.sel_start, state.sel_end);
    if start != end {
        return replace_selection(state, value, "", 0, None);
    }
    if backspace {
        if start == 0 {
            return false;
        }
        let (next, cursor) = delete_unit(value, start, true);
        if next == *value {
            return false;
        }
        *value = next;
        state.sel_start = cursor;
        state.sel_end = cursor;
        state.revision += 1;
        true
    } else {
        if start >= utf16_len(value) {
            return false;
        }
        let (next, cursor) = delete_unit(value, start, false);
        if next == *value {
            return false;
        }
        *value = next;
        state.sel_start = cursor;
        state.sel_end = cursor;
        state.revision += 1;
        true
    }
}

fn delete_unit(value: &str, units: u32, backspace: bool) -> (String, u32) {
    let mut acc = 0u32;
    for (index, ch) in value.char_indices() {
        let width = ch.len_utf16() as u32;
        let owns = if backspace {
            acc < units && acc + width >= units
        } else {
            acc == units || (acc < units && acc + width > units)
        };
        if owns {
            let mut next = String::new();
            next.push_str(&value[..index]);
            next.push_str(&value[index + ch.len_utf8()..]);
            return (next, acc);
        }
        acc += width;
    }
    (value.to_owned(), units)
}

fn ordered(start: u32, end: u32) -> (u32, u32) {
    if start <= end {
        (start, end)
    } else {
        (end, start)
    }
}

fn sanitize(text: &str, multiline: bool) -> String {
    if multiline {
        text.replace('\r', "")
    } else {
        text.chars()
            .filter(|ch| *ch != '\n' && *ch != '\r')
            .collect()
    }
}

fn fit(text: &str, max_units: u32) -> String {
    if max_units == u32::MAX {
        return text.to_owned();
    }
    let mut out = String::new();
    let mut used = 0u32;
    for ch in text.chars() {
        let width = ch.len_utf16() as u32;
        if used + width > max_units {
            break;
        }
        out.push(ch);
        used += width;
    }
    out
}

fn clamp_units(value: &str, units: u32) -> u32 {
    utf16_len(&value[..boundary(value, units)])
}

fn boundary(value: &str, units: u32) -> usize {
    let mut seen = 0u32;
    for (index, ch) in value.char_indices() {
        if seen == units {
            return index;
        }
        let width = ch.len_utf16() as u32;
        if seen + width > units {
            return index;
        }
        seen += width;
    }
    value.len()
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextMetrics {
    /// Selection rectangles relative to the text origin: x, y, w, h.
    pub selection: Vec<[f32; 4]>,
    pub caret_x: f32,
    pub caret_y: f32,
    pub caret_h: f32,
}

/// `advance` is the width of one line, measured without wrapping.
pub fn text_metrics(
    text: &str,
    sel_start: u32,
    sel_end: u32,
    line_height: f32,
    mut advance: impl FnMut(&str) -> f32,
) -> TextMetrics {
    let lh = if line_height.is_finite() && line_height > 0.0 {
        line_height
    } else {
        0.0
    };
    let lines: Vec<&str> = text.split('\n').collect();
    let mut spans = Vec::new();
    let mut unit = 0u32;
    for (index, line) in lines.iter().enumerate() {
        let len = utf16_len(line);
        let has_nl = index + 1 != lines.len();
        spans.push((unit, unit + len, has_nl, *line));
        unit += len + u32::from(has_nl);
    }
    if spans.is_empty() {
        spans.push((0, 0, false, ""));
    }
    let locate = |units: u32| -> (usize, u32) {
        for (index, &(start, end, _, _)) in spans.iter().enumerate() {
            if units <= end {
                return (index, units.saturating_sub(start));
            }
        }
        let last = spans.len() - 1;
        let (start, end, _, _) = spans[last];
        (last, end.saturating_sub(start))
    };
    let (lo, hi) = if sel_start <= sel_end {
        (sel_start, sel_end)
    } else {
        (sel_end, sel_start)
    };
    let mut selection = Vec::new();
    for (index, &(start, end, has_nl, line)) in spans.iter().enumerate() {
        let line_hi = end + u32::from(has_nl);
        let from = lo.max(start);
        let to = hi.min(line_hi);
        if from >= to {
            continue;
        }
        let col0 = from.saturating_sub(start).min(end.saturating_sub(start));
        let col1 = if to > end {
            end.saturating_sub(start)
        } else {
            to.saturating_sub(start)
        };
        let x0 = advance(&line[..boundary(line, col0)]);
        let x1 = advance(&line[..boundary(line, col1)]);
        if x1 > x0 {
            selection.push([x0, index as f32 * lh, x1 - x0, lh]);
        }
    }
    let (line, col) = locate(sel_end);
    let caret_line = spans[line].3;
    TextMetrics {
        selection,
        caret_x: advance(&caret_line[..boundary(caret_line, col)]),
        caret_y: line as f32 * lh,
        caret_h: lh,
    }
}

/// UTF-16 offset nearest to a point relative to the text origin.
/// `advance` is the width of one line, measured without wrapping.
pub fn hit_unit(
    text: &str,
    x: f32,
    y: f32,
    line_height: f32,
    mut advance: impl FnMut(&str) -> f32,
) -> u32 {
    let lh = if line_height.is_finite() && line_height > 0.0 {
        line_height
    } else {
        0.0
    };
    let lines: Vec<&str> = text.split('\n').collect();
    let lines: Vec<&str> = if lines.is_empty() { vec![""] } else { lines };
    let index = if lh == 0.0 || y < lh {
        0
    } else {
        ((y / lh).floor() as usize).min(lines.len() - 1)
    };
    let mut base = 0u32;
    for line in lines.iter().take(index) {
        base += utf16_len(line) + 1;
    }
    let line = lines[index];
    let total = utf16_len(line);
    let mut best = 0u32;
    let mut best_dist = f32::MAX;
    for col in 0..=total {
        let width = advance(&line[..boundary(line, col)]);
        let dist = (width - x).abs();
        if dist < best_dist {
            best_dist = dist;
            best = col;
        }
    }
    base + best
}

pub fn utf16_len(value: &str) -> u32 {
    value.chars().map(|ch| ch.len_utf16() as u32).sum()
}
