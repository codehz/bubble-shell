//! Touch and pen field gestures. Mouse selects on the down sample; touch and
//! pen wait for a long press so a drag can scroll. On touch and pen the
//! selection handles and the caret handle claim the pointer immediately; a
//! mouse press never drags them — it re-creates the selection, or long-presses
//! the selection body to move its content.

use crate::editing::{display_text, utf16_len, EditCommand};
use crate::gesture::{PointerAction, PointerInput, LONG_PRESS_DIST, LONG_PRESS_MS};
use crate::layout::{LayoutKey, LayoutRequest, LayoutSnapshot};

use super::focus_edit::utf16_slice;
use super::Shell;

const HANDLE_MARGIN: f32 = 22.0;

type LayoutHost<'a> = &'a mut dyn FnMut(&LayoutRequest<'_>) -> Result<LayoutSnapshot, String>;

#[derive(Clone, Copy, Debug, PartialEq)]
enum SelectionHandleId {
    Start,
    End,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum FieldPointerMode {
    WaitingLongPress,
    /// Timer fired. The next layout host places the caret or starts a content drag.
    PendingLongPressActivation,
    Scrolling,
    SelectingText {
        anchor: u32,
    },
    DraggingCursor,
    DraggingSelectionStart {
        fixed_end: u32,
    },
    DraggingSelectionEnd {
        fixed_start: u32,
    },
    DraggingContent {
        original_start: u32,
        original_end: u32,
    },
}

#[derive(Debug)]
pub(super) struct FieldPointerState {
    pointer_id: u32,
    field_handle: u32,
    mode: FieldPointerMode,
    start_x: f32,
    start_y: f32,
    last_x: f32,
    last_y: f32,
    timer_deadline: f64,
    /// Frozen down-sample hit path. Touch scrolling reuses it so a dragged
    /// finger keeps moving the containers it landed on.
    hit_path: Vec<u32>,
    /// Set when the down sample landed in an existing selection body.
    content_drag: Option<(u32, u32)>,
    /// UTF-16 offset under the pointer while dragging field content.
    drop_unit: Option<u32>,
    /// False when the pointer is inside the original selection.
    drop_valid: bool,
}

impl FieldPointerState {
    pub(super) fn content_drop_valid(&self) -> bool {
        matches!(self.mode, FieldPointerMode::DraggingContent { .. }) && self.drop_valid
    }
}

impl Shell {
    pub(super) fn field_owns(&self, id: u32) -> bool {
        self.field_pointer
            .as_ref()
            .is_some_and(|state| state.pointer_id == id)
    }

    /// Claim a pointer-down that lands on a field, selection handle, or caret.
    /// `false` leaves the sample for the gesture recognizer.
    pub(super) fn try_field_pointer(&mut self, input: &PointerInput, host: LayoutHost<'_>) -> bool {
        if self
            .field_pointer
            .as_ref()
            .is_some_and(|state| state.pointer_id != input.id)
        {
            return false;
        }
        let direct = input.pointer_kind == 1;
        if direct && !input.eligible() {
            return false;
        }

        // Mouse never drags the caret or the selection handles. A press on
        // the selection body arms the same long-press content drag touch
        // uses; anywhere else it re-creates the selection from the anchor.
        if direct {
            if self.hit_selection_body(input.x, input.y) {
                if let Ok(field) = self.focused_state() {
                    let (start, end) = (field.sel_start(), field.sel_end());
                    if start != end {
                        self.begin_field_pointer(
                            input,
                            self.focused,
                            FieldPointerMode::WaitingLongPress,
                            Some((start, end)),
                        );
                        return true;
                    }
                }
            }
        } else if let Some(which) = self.hit_selection_handle(input.x, input.y) {
            let Ok(field) = self.focused_state() else {
                return false;
            };
            let mode = match which {
                SelectionHandleId::Start => FieldPointerMode::DraggingSelectionStart {
                    fixed_end: field.sel_end(),
                },
                SelectionHandleId::End => FieldPointerMode::DraggingSelectionEnd {
                    fixed_start: field.sel_start(),
                },
            };
            self.begin_field_pointer(input, self.focused, mode, None);
            return true;
        } else if self.hit_cursor_handle(input.x, input.y) {
            self.begin_field_pointer(input, self.focused, FieldPointerMode::DraggingCursor, None);
            return true;
        } else if self.hit_selection_body(input.x, input.y) {
            if let Ok(field) = self.focused_state() {
                let (start, end) = (field.sel_start(), field.sel_end());
                if start != end {
                    self.begin_field_pointer(
                        input,
                        self.focused,
                        FieldPointerMode::WaitingLongPress,
                        Some((start, end)),
                    );
                    return true;
                }
            }
        }

        let handle = self.field_at(input.x, input.y);
        if handle == 0 {
            return false;
        }
        if direct {
            let claimed = self.focus_point_with(input.x, input.y, host).unwrap_or(0);
            if claimed == 0 {
                return false;
            }
            let anchor = self
                .focused_state()
                .map(|field| field.active())
                .unwrap_or(0);
            self.begin_field_pointer(
                input,
                claimed,
                FieldPointerMode::SelectingText { anchor },
                None,
            );
            return true;
        }
        self.begin_field_pointer(input, handle, FieldPointerMode::WaitingLongPress, None);
        true
    }

    pub(super) fn handle_field_pointer(&mut self, input: &PointerInput, host: LayoutHost<'_>) {
        if input.action == PointerAction::Move && self.past_long_press_slop(input) {
            if let Some(state) = self.field_pointer.as_mut() {
                state.mode = FieldPointerMode::Scrolling;
            }
        } else {
            self.promote_field_long_press(input.time_ms);
            self.resolve_field_long_press(host);
        }
        match input.action {
            PointerAction::Move => self.move_field_pointer(input, Some(host)),
            PointerAction::Up => self.up_field_pointer(input, Some(host)),
            PointerAction::Cancel => self.cancel_field_pointer(),
            PointerAction::Down => {}
        }
    }

    pub(super) fn advance_field_pointer(&mut self, now_ms: f64) {
        let Some(state) = self.field_pointer.as_mut() else {
            return;
        };
        if state.mode == FieldPointerMode::WaitingLongPress && now_ms >= state.timer_deadline {
            state.mode = FieldPointerMode::PendingLongPressActivation;
        }
    }

    /// Place the caret (or arm a content drag) once a layout host exists.
    pub(super) fn resolve_field_long_press(&mut self, host: LayoutHost<'_>) {
        let Some(state) = self.field_pointer.as_ref() else {
            return;
        };
        if state.mode != FieldPointerMode::PendingLongPressActivation {
            return;
        }
        let (x, y) = (state.start_x, state.start_y);
        let content = state.content_drag;
        if let Some((start, end)) = content {
            if let Some(state) = self.field_pointer.as_mut() {
                state.mode = FieldPointerMode::DraggingContent {
                    original_start: start,
                    original_end: end,
                };
                state.drop_unit = None;
                state.drop_valid = false;
            }
            self.arm_drag_ghost(start, end, host);
            return;
        }
        let claimed = self.focus_point_with(x, y, host).unwrap_or(0);
        if claimed == 0 {
            self.end_field_pointer();
            return;
        }
        let anchor = self
            .focused_state()
            .map(|field| field.active())
            .unwrap_or(0);
        if let Some(state) = self.field_pointer.as_mut() {
            state.field_handle = claimed;
            state.mode = FieldPointerMode::SelectingText { anchor };
        }
    }

    pub(super) fn cancel_field_pointer(&mut self) {
        let content = self
            .field_pointer
            .as_ref()
            .and_then(|state| match state.mode {
                FieldPointerMode::DraggingContent {
                    original_start,
                    original_end,
                } => Some((original_start, original_end)),
                _ => None,
            });
        if let Some((start, end)) = content {
            let _ = self.edit(EditCommand::SetSelection { start, end });
            if self.edit_batch_active() {
                self.abort_edit_batch();
            }
        }
        self.end_field_pointer();
    }

    fn begin_field_pointer(
        &mut self,
        input: &PointerInput,
        field_handle: u32,
        mode: FieldPointerMode,
        content_drag: Option<(u32, u32)>,
    ) {
        self.clear_drag_visual();
        self.field_pointer = Some(FieldPointerState {
            pointer_id: input.id,
            field_handle,
            mode,
            start_x: input.x,
            start_y: input.y,
            last_x: input.x,
            last_y: input.y,
            timer_deadline: input.time_ms + LONG_PRESS_MS,
            hit_path: input.path.clone(),
            content_drag,
            drop_unit: None,
            drop_valid: false,
        });
    }

    fn promote_field_long_press(&mut self, now_ms: f64) {
        let Some(state) = self.field_pointer.as_mut() else {
            return;
        };
        if state.mode == FieldPointerMode::WaitingLongPress && now_ms >= state.timer_deadline {
            state.mode = FieldPointerMode::PendingLongPressActivation;
        }
    }

    fn past_long_press_slop(&self, input: &PointerInput) -> bool {
        let Some(state) = self.field_pointer.as_ref() else {
            return false;
        };
        if state.mode != FieldPointerMode::WaitingLongPress {
            return false;
        }
        (input.x - state.start_x).hypot(input.y - state.start_y) > LONG_PRESS_DIST
    }

    fn move_field_pointer(&mut self, input: &PointerInput, mut host: Option<LayoutHost<'_>>) {
        let Some(state) = self.field_pointer.as_ref() else {
            return;
        };
        let mode = state.mode;
        let last_x = state.last_x;
        let last_y = state.last_y;
        match mode {
            FieldPointerMode::WaitingLongPress | FieldPointerMode::PendingLongPressActivation => {
                self.note_field_point(input);
            }
            FieldPointerMode::Scrolling => {
                self.note_field_point(input);
                // Recognizer scroll negates the pointer delta so content follows
                // the finger. Field drags use the same sign.
                self.field_scroll(-(input.x - last_x), -(input.y - last_y));
            }
            FieldPointerMode::SelectingText { anchor } => {
                self.note_field_point(input);
                if let Some(unit) = self.field_unit(input.x, input.y, host) {
                    self.select_field(anchor, unit);
                }
            }
            FieldPointerMode::DraggingCursor => {
                self.note_field_point(input);
                if let Some(unit) = self.field_unit(input.x, input.y, host) {
                    self.select_field(unit, unit);
                }
            }
            FieldPointerMode::DraggingSelectionStart { fixed_end } => {
                self.note_field_point(input);
                let Some(unit) = self.field_unit(input.x, input.y, host) else {
                    return;
                };
                if unit > fixed_end {
                    if let Some(state) = self.field_pointer.as_mut() {
                        state.mode = FieldPointerMode::DraggingSelectionEnd {
                            fixed_start: fixed_end,
                        };
                    }
                }
                self.select_field(fixed_end, unit);
            }
            FieldPointerMode::DraggingSelectionEnd { fixed_start } => {
                self.note_field_point(input);
                let Some(unit) = self.field_unit(input.x, input.y, host) else {
                    return;
                };
                if unit < fixed_start {
                    if let Some(state) = self.field_pointer.as_mut() {
                        state.mode = FieldPointerMode::DraggingSelectionStart {
                            fixed_end: fixed_start,
                        };
                    }
                }
                self.select_field(fixed_start, unit);
            }
            FieldPointerMode::DraggingContent {
                original_start,
                original_end,
            } => {
                self.note_field_point(input);
                self.drag_ghost_pos = Some((input.x, input.y));
                if self.drag_ghost_size == (0.0, 0.0) && !self.drag_ghost_text.is_empty() {
                    if let Some(measure_host) = host.as_mut() {
                        self.drag_ghost_size = self.measure_drag_ghost(&mut **measure_host);
                    }
                }
                let unit = host
                    .as_mut()
                    .and_then(|hit_host| self.field_unit(input.x, input.y, Some(&mut **hit_host)));
                let valid = unit.is_some_and(|at| !(at >= original_start && at <= original_end));
                let previous = self
                    .field_pointer
                    .as_ref()
                    .map(|state| (state.drop_unit, state.drop_valid));
                if let Some(state) = self.field_pointer.as_mut() {
                    state.drop_unit = unit;
                    state.drop_valid = valid;
                }
                if previous != Some((unit, valid)) {
                    self.drop_caret_box = match (unit.filter(|_| valid), host) {
                        (Some(unit), Some(caret_host)) => self
                            .caret_rect_at_with(unit, caret_host)
                            .ok()
                            .flatten()
                            .map(|(x, y, _, h)| (x, y, 2.0, h)),
                        _ => None,
                    };
                }
                self.paint_dirty.set(true);
            }
        }
    }

    fn up_field_pointer(&mut self, input: &PointerInput, host: Option<LayoutHost<'_>>) {
        let Some(state) = self.field_pointer.as_ref() else {
            return;
        };
        let mode = state.mode;
        let (start_x, start_y) = (state.start_x, state.start_y);
        match mode {
            FieldPointerMode::WaitingLongPress | FieldPointerMode::PendingLongPressActivation => {
                if let Some(host) = host {
                    let _ = self.focus_point_with(start_x, start_y, host);
                }
                self.end_field_pointer();
            }
            FieldPointerMode::DraggingContent {
                original_start,
                original_end,
            } => {
                self.finish_content_drag(input.x, input.y, original_start, original_end, host);
                self.end_field_pointer();
            }
            _ => self.end_field_pointer(),
        }
    }

    fn finish_content_drag(
        &mut self,
        x: f32,
        y: f32,
        original_start: u32,
        original_end: u32,
        host: Option<LayoutHost<'_>>,
    ) {
        let restore = EditCommand::SetSelection {
            start: original_start,
            end: original_end,
        };
        let Some(host) = host else {
            let _ = self.edit(restore);
            return;
        };
        let Some(drop_unit) = self.unit_at_with(x, y, host).ok().flatten() else {
            let _ = self.edit(restore);
            return;
        };
        if drop_unit >= original_start && drop_unit <= original_end {
            let _ = self.edit(restore);
            return;
        }
        let text = match self.field(self.focused) {
            Ok((value, _)) => utf16_slice(&value, original_start, original_end),
            Err(_) => return,
        };
        if text.is_empty() {
            return;
        }
        if self.begin_edit_batch().is_err() {
            return;
        }
        if self.commit_text("", 1).is_err() {
            self.abort_edit_batch();
            return;
        }
        let removed = original_end.saturating_sub(original_start);
        let adjusted = if drop_unit > original_end {
            drop_unit.saturating_sub(removed)
        } else {
            drop_unit
        };
        if self
            .edit(EditCommand::SetSelection {
                start: adjusted,
                end: adjusted,
            })
            .is_err()
            || self.commit_text(&text, 1).is_err()
        {
            self.abort_edit_batch();
            return;
        }
        if self.end_edit_batch().is_err() {
            self.abort_edit_batch();
        }
    }

    fn end_field_pointer(&mut self) {
        self.clear_drag_visual();
        self.field_pointer = None;
    }

    fn clear_drag_visual(&mut self) {
        let showing = self.drop_caret_box.is_some()
            || self.drag_ghost_pos.is_some()
            || !self.drag_ghost_text.is_empty();
        self.drop_caret_box = None;
        self.drag_ghost_text.clear();
        self.drag_ghost_pos = None;
        self.drag_ghost_size = (0.0, 0.0);
        if showing {
            self.paint_dirty.set(true);
        }
    }

    fn arm_drag_ghost(&mut self, start: u32, end: u32, host: LayoutHost<'_>) {
        let text = self
            .field(self.focused)
            .ok()
            .map(|(value, _)| utf16_slice(&value, start, end))
            .unwrap_or_default();
        let secure = self.node(self.focused).is_ok_and(|node| node.secure);
        self.drag_ghost_text = display_text(&text, secure);
        self.drag_ghost_pos = self
            .field_pointer
            .as_ref()
            .map(|state| (state.start_x, state.start_y));
        self.drag_ghost_size = self.measure_drag_ghost(host);
        self.drop_caret_box = None;
        self.paint_dirty.set(true);
    }

    fn measure_drag_ghost(&mut self, host: LayoutHost<'_>) -> (f32, f32) {
        if self.drag_ghost_text.is_empty() {
            return (0.0, 0.0);
        }
        let Some(key) = self.ghost_layout_key() else {
            return (0.0, 0.0);
        };
        let req = LayoutRequest {
            text: &self.drag_ghost_text,
            key,
        };
        let measured = host(&req).ok().map(|snapshot| ghost_extent(&snapshot));
        match measured {
            Some((width, height)) if width > 0.0 || height > 0.0 => (width, height),
            _ => {
                let units = utf16_len(&self.drag_ghost_text) as f32;
                (units * key.font_size * 0.6, key.line_height)
            }
        }
    }

    fn ghost_layout_key(&self) -> Option<LayoutKey> {
        let node = self.node(self.focused).ok()?;
        let font_size = node.font_size;
        let line_height = if node.line_height > 0.0 {
            node.line_height
        } else {
            font_size * 1.2
        };
        Some(LayoutKey {
            revision: 0,
            width: 1.0e6,
            font_size,
            font_weight: node.font_weight,
            line_height,
            align: 0,
            density: 1.0,
        })
    }

    fn note_field_point(&mut self, input: &PointerInput) {
        if let Some(state) = self.field_pointer.as_mut() {
            state.last_x = input.x;
            state.last_y = input.y;
        }
    }

    fn field_scroll(&mut self, dx: f32, dy: f32) {
        let path = match self.field_pointer.as_mut() {
            Some(state) => std::mem::take(&mut state.hit_path),
            None => return,
        };
        self.scroll_path(&path, dx, dy);
        if let Some(state) = self.field_pointer.as_mut() {
            state.hit_path = path;
        }
    }

    fn field_unit(&mut self, x: f32, y: f32, host: Option<LayoutHost<'_>>) -> Option<u32> {
        let host = host?;
        let handle = self.field_pointer.as_ref()?.field_handle;
        if self.focused != handle {
            return None;
        }
        self.unit_at_with(x, y, host).ok().flatten()
    }

    fn select_field(&mut self, anchor: u32, active: u32) {
        let _ = self.edit(EditCommand::SetSelection {
            start: anchor,
            end: active,
        });
    }

    /// True when `(x, y)` is within the caret's touch margin. A live selection
    /// hides this target so it cannot steal the selection handles.
    fn hit_cursor_handle(&self, x: f32, y: f32) -> bool {
        if self.selection_handles().is_some() {
            return false;
        }
        let Some((caret_x, caret_y, _caret_w, caret_h)) = self.caret_box() else {
            return false;
        };
        x >= caret_x - HANDLE_MARGIN
            && x < caret_x + HANDLE_MARGIN
            && y >= caret_y
            && y < caret_y + caret_h
    }

    /// Which selection handle was hit, preferring the nearer caret when the
    /// touch margins overlap on a short range.
    fn hit_selection_handle(&self, x: f32, y: f32) -> Option<SelectionHandleId> {
        let handles = self.selection_handles()?;
        let rects = self.selection_rects();
        let mut best: Option<(SelectionHandleId, f32)> = None;
        for (index, (hx, hy, _hw, _hh)) in handles.iter().enumerate() {
            let caret_x = hx + 8.0;
            let caret_bottom_y = hy + 4.0;
            let Some((_, rect_y, _, rect_h)) = rects.iter().find(|(_, rect_y, _, rect_h)| {
                caret_bottom_y >= *rect_y && caret_bottom_y <= rect_y + rect_h
            }) else {
                continue;
            };
            if x < caret_x - HANDLE_MARGIN
                || x >= caret_x + HANDLE_MARGIN
                || y < *rect_y
                || y >= rect_y + rect_h
            {
                continue;
            }
            let distance = (x - caret_x).abs();
            let id = if index == 0 {
                SelectionHandleId::Start
            } else {
                SelectionHandleId::End
            };
            if best.is_none_or(|(_, previous)| distance < previous) {
                best = Some((id, distance));
            }
        }
        best.map(|(id, _)| id)
    }

    /// True when `(x, y)` is inside a selection highlight and, when the
    /// handles are drawn (touch and pen), outside both handle touch margins
    /// on that line. The mouse draws no handles, so its whole highlight is
    /// body.
    fn hit_selection_body(&self, x: f32, y: f32) -> bool {
        let rects = self.selection_rects();
        let Some(index) = rects.iter().position(|(rect_x, rect_y, rect_w, rect_h)| {
            x >= *rect_x && y >= *rect_y && x < rect_x + rect_w && y < rect_y + rect_h
        }) else {
            return false;
        };
        let rect = rects[index];
        let Some(handles) = self.selection_handles() else {
            return true;
        };
        for (hx, hy, _hw, _hh) in &handles {
            let caret_x = hx + 8.0;
            let caret_bottom_y = hy + 4.0;
            if caret_bottom_y >= rect.1
                && caret_bottom_y <= rect.1 + rect.3
                && x >= caret_x - HANDLE_MARGIN
                && x < caret_x + HANDLE_MARGIN
            {
                return false;
            }
        }
        true
    }
}

fn ghost_extent(snapshot: &LayoutSnapshot) -> (f32, f32) {
    if snapshot.runs.is_empty() {
        let height = snapshot
            .carets
            .last()
            .map(|caret| caret.y + caret.h)
            .unwrap_or(0.0);
        return (0.0, height.max(0.0));
    }
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = 0.0f32;
    let mut max_y = 0.0f32;
    for run in &snapshot.runs {
        min_x = min_x.min(run.x);
        min_y = min_y.min(run.y);
        max_x = max_x.max(run.x + run.w);
        max_y = max_y.max(run.y + run.h);
    }
    ((max_x - min_x).max(0.0), (max_y - min_y).max(0.0))
}
