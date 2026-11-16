use crate::editing::{
    display_text, utf16_len, DocRange, EditCommand, EditConfig, EditError, EditEvent, EditOp,
    EditOutcome, EditSession, FieldState, Selection, SessionId,
};
use crate::ir::Kind;
use crate::layout::{snap_unit, wrapping_layout, LayoutKey, LayoutRequest, LayoutSnapshot};
use crate::semantics::Role;

use super::{EditBatch, EditConflict, EditMenuAction, Key, Measure, MenuResult, Shell};

struct FieldGeom {
    text: String,
    origin_x: f32,
    origin_y: f32,
    key: LayoutKey,
}

impl Shell {
    /// Focus a semantic node. `0` blurs. Text fields and nodes marked
    /// `focusable` (or carrying an interactive role) accept focus.
    pub fn focus(&mut self, handle: u32) -> Result<(), String> {
        if handle != 0 && !self.interaction_available(handle, self.active_modal()) {
            return Err(format!("handle {handle} is unavailable"));
        }
        if handle == 0 {
            self.abort_edit_batch();
            self.focused = 0;
            self.sample_sources();
            self.paint_dirty.set(true);
            return Ok(());
        }
        let node = self.node(handle)?;
        if !self.is_focusable(handle) {
            return Err(format!("handle {handle} is not focusable"));
        }
        if node.kind == Kind::Field && !self.fields.contains_key(&handle) {
            return Err(format!("handle {handle} has no editing session"));
        }
        if self.focused != handle {
            self.abort_edit_batch();
        }
        self.focused = handle;
        self.sample_sources();
        self.paint_dirty.set(true);
        Ok(())
    }

    pub fn focused(&self) -> u32 {
        self.focused
    }

    /// Whether the node participates in Tab order and screen-reader focus.
    pub fn is_focusable(&self, handle: u32) -> bool {
        let Ok(node) = self.node(handle) else {
            return false;
        };
        node.kind == Kind::Field || node.focusable || node.role.is_some_and(Role::interactive)
    }

    /// Move focus through the current Tab order. Modal containers trap the
    /// order to their subtree. Returns `false` when no focusable node exists.
    pub fn focus_next(&mut self, forward: bool) -> bool {
        let order = self.focus_order();
        if order.is_empty() {
            return false;
        }
        let current = order.iter().position(|handle| *handle == self.focused);
        let next = match current {
            Some(index) if forward => (index + 1) % order.len(),
            Some(index) => (index + order.len() - 1) % order.len(),
            None if forward => 0,
            None => order.len() - 1,
        };
        self.focus(order[next]).is_ok()
    }

    /// Deepest visible modal container, or `0`. Later siblings win ties.
    pub fn active_modal(&self) -> u32 {
        let mut found = (0, 0);
        self.walk_modal(1, 0, &mut found);
        found.0
    }

    fn walk_modal(&self, id: u32, depth: u32, found: &mut (u32, u32)) {
        let Ok(node) = self.node(id) else {
            return;
        };
        if node.a11y_hidden || node.display_none || node.visibility_hidden {
            return;
        }
        if node.modal && depth >= found.1 {
            *found = (id, depth);
        }
        for &child in &node.children {
            self.walk_modal(child, depth + 1, found);
        }
    }

    pub(super) fn interaction_available(&self, handle: u32, modal: u32) -> bool {
        let Ok(node) = self.node(handle) else {
            return false;
        };
        if node.disabled || (modal != 0 && !self.contains(modal, handle)) {
            return false;
        }
        let mut id = handle;
        while id != 0 {
            let Ok(node) = self.node(id) else {
                return false;
            };
            if node.a11y_hidden || node.display_none || node.visibility_hidden {
                return false;
            }
            id = node.parent;
        }
        true
    }

    fn focus_order(&self) -> Vec<u32> {
        let modal = self.active_modal();
        let mut out = Vec::new();
        self.collect_focusable(1, modal, false, &mut out);
        out
    }

    fn collect_focusable(&self, id: u32, modal: u32, hidden: bool, out: &mut Vec<u32>) {
        let Ok(node) = self.node(id) else {
            return;
        };
        if node.display_none {
            return;
        }
        let outside = modal != 0 && !self.contains(modal, id);
        let hidden = hidden || node.a11y_hidden || node.visibility_hidden;
        if !hidden && !outside && !node.disabled && self.is_focusable(id) {
            out.push(id);
        }
        let children = node.children.clone();
        for child in children {
            self.collect_focusable(child, modal, hidden, out);
        }
    }

    /// Handle one UI-thread keyboard intent. Returns `true` when consumed;
    /// text-producing keys stay with the platform input method.
    pub fn key(&mut self, key: Key, time_ms: f64) -> bool {
        match key {
            Key::Tab => self.focus_next(true),
            Key::TabBack => self.focus_next(false),
            Key::Escape => {
                if self.focused == 0 {
                    false
                } else {
                    self.focus(0).is_ok()
                }
            }
            Key::Enter => {
                if self.focused == 0 {
                    return false;
                }
                let multiline = self.node(self.focused).is_ok_and(|node| node.multiline);
                if multiline
                    && self
                        .node(self.focused)
                        .is_ok_and(|node| node.kind == Kind::Field)
                {
                    self.commit_text("\n", 1).is_ok()
                } else {
                    self.activate_focused(time_ms)
                }
            }
            Key::Space => {
                if self.focused == 0 {
                    return false;
                }
                if self
                    .node(self.focused)
                    .is_ok_and(|node| node.kind == Kind::Field)
                {
                    false
                } else {
                    self.activate_focused(time_ms)
                }
            }
            Key::Left => self.move_caret(false, false, false),
            Key::Right => self.move_caret(true, false, false),
            Key::Home => self.move_caret(false, true, false),
            Key::End => self.move_caret(true, true, false),
            Key::ShiftLeft => self.move_caret(false, false, true),
            Key::ShiftRight => self.move_caret(true, false, true),
            Key::ShiftHome => self.move_caret(false, true, true),
            Key::ShiftEnd => self.move_caret(true, true, true),
            Key::SelectAll => {
                if self.focused == 0
                    || !self
                        .node(self.focused)
                        .is_ok_and(|node| node.kind == Kind::Field)
                {
                    return false;
                }
                self.edit_menu(EditMenuAction::SelectAll, "").is_ok()
            }
        }
    }

    fn activate_focused(&mut self, time_ms: f64) -> bool {
        let handle = self.focused;
        if handle == 0 {
            return false;
        }
        if self.node(handle).is_ok_and(|node| node.kind == Kind::Field) {
            return self.focus(handle).is_ok();
        }
        let (cx, cy) = self
            .node(handle)
            .map(|node| (node.x + node.w / 2.0, node.y + node.h / 2.0))
            .unwrap_or((0.0, 0.0));
        self.recognizer.activate(handle, cx, cy, time_ms)
    }

    fn move_caret(&mut self, forward: bool, line: bool, extend: bool) -> bool {
        let handle = self.focused;
        if handle == 0 || !self.node(handle).is_ok_and(|node| node.kind == Kind::Field) {
            return false;
        }
        let Ok((value, state)) = self.field(handle) else {
            return false;
        };
        if extend {
            let (anchor, active) = (state.anchor(), state.active());
            let target = if line {
                line_edge(&value, active, forward)
            } else {
                caret_step(&value, active, forward)
            };
            if target == active {
                return false;
            }
            return self
                .edit(EditCommand::SetSelection {
                    start: anchor,
                    end: target,
                })
                .is_ok();
        }
        let (start, end) = (state.sel_start(), state.sel_end());
        let target = if line {
            line_edge(&value, state.active(), forward)
        } else if start != end {
            if forward {
                end
            } else {
                start
            }
        } else {
            caret_step(&value, state.active(), forward)
        };
        self.edit(EditCommand::SetSelection {
            start: target,
            end: target,
        })
        .is_ok()
    }

    pub(super) fn set_field_text(&mut self, handle: u32, text: &str) -> Result<(), String> {
        let (handler, config) = {
            let node = self.node(handle)?;
            if node.kind != Kind::Field {
                return Err(format!("handle {handle} is not a text field"));
            }
            (
                node.handler,
                EditConfig {
                    multiline: node.multiline,
                    max_length: node.max_length,
                },
            )
        };
        let session = self
            .fields
            .get_mut(&handle)
            .ok_or_else(|| format!("handle {handle} has no editing session"))?;
        session.set_config(config);
        if session.field_state().composing {
            let _ = session.apply(EditOp::CancelComposition, None);
        }
        let len = utf16_len(session.value());
        let selection = Selection::collapsed(utf16_len(text));
        let outcome = session
            .apply(
                EditOp::Replace {
                    range: DocRange::new(0, len),
                    text: text.to_owned(),
                    selection: Some(selection),
                    composition: crate::editing::CompositionChange::Clear,
                },
                None,
            )
            .map_err(|err| err.to_string())?;
        self.finish_edit(handle, handler, outcome)
    }

    /// Cut, copy, paste, or select all in the focused field. The host owns
    /// clipboard storage and passes the text in for paste.
    pub fn edit_menu(
        &mut self,
        action: EditMenuAction,
        clipboard: &str,
    ) -> Result<MenuResult, String> {
        let handle = self.focused;
        if handle == 0 || !self.node(handle).is_ok_and(|node| node.kind == Kind::Field) {
            return Err("no focused field".into());
        }
        let (value, state) = self.field(handle)?;
        let (start, end) = (state.sel_start(), state.sel_end());
        let secure = self.node(handle).is_ok_and(|node| node.secure);
        let mut result = MenuResult::default();
        match action {
            EditMenuAction::Copy => {
                if !secure && start != end {
                    result.clipboard = Some(utf16_slice(&value, start, end));
                }
            }
            EditMenuAction::Cut => {
                if !secure && start != end {
                    result.clipboard = Some(utf16_slice(&value, start, end));
                    self.commit_text("", 1)?;
                    result.changed = true;
                }
            }
            EditMenuAction::Paste => {
                if !clipboard.is_empty() {
                    self.commit_text(clipboard, 1)?;
                    result.changed = true;
                }
            }
            EditMenuAction::SelectAll => {
                let len = utf16_len(&value);
                if len > 0 && (start, end) != (0, len) {
                    self.edit(EditCommand::SetSelection { start: 0, end: len })?;
                    result.changed = true;
                }
            }
        }
        Ok(result)
    }

    /// `bit0` multiline, `bit1` secure, bits 8..15 keyboard kind
    /// (`1` number, `2` email, `3` url, `4` phone), bits 16..23 autofill
    /// (`1` username, `2` password, `3` newPassword, `4` email, `5` phone, `6` oneTimeCode).
    pub fn field_flags(&self, handle: u32) -> Result<u32, String> {
        let node = self.node(handle)?;
        if node.kind != Kind::Field {
            return Err(format!("handle {handle} is not a text field"));
        }
        let mut bits = 0u32;
        if node.multiline {
            bits |= 1;
        }
        if node.secure {
            bits |= 2;
        }
        let kind = match node.keyboard.as_str() {
            "number" | "numeric" => 1,
            "email" => 2,
            "url" => 3,
            "phone" | "tel" => 4,
            _ => 0,
        };
        let autofill = match node.autofill.as_str() {
            "username" => 1,
            "password" | "current-password" => 2,
            "newPassword" | "new-password" => 3,
            "email" => 4,
            "phone" | "tel" => 5,
            "oneTimeCode" | "one-time-code" => 6,
            _ => 0,
        };
        Ok(bits | (kind << 8) | (autofill << 16))
    }

    /// Focused caret in viewport coordinates: x, y, width, height.
    pub fn caret_box(&self) -> Option<(f32, f32, f32, f32)> {
        self.caret_box
    }

    /// Start and end selection handle boxes (viewport coordinates), ordered
    /// as document start and document end. `None` for a collapsed selection.
    pub fn selection_handles(&self) -> Option<[(f32, f32, f32, f32); 2]> {
        self.selection_handles
    }

    /// UTF-16 offset in the focused field nearest the point. Does not move the caret.
    pub fn unit_at(&self, x: f32, y: f32, measure: Measure<'_>) -> Result<Option<u32>, String> {
        self.unit_at_with(x, y, &mut |req| {
            Ok(wrapping_layout(req, |line| {
                measure(
                    line,
                    req.key.font_size,
                    req.key.line_height,
                    req.key.font_weight,
                    None,
                )
                .0
            }))
        })
    }

    pub fn unit_at_with(
        &self,
        x: f32,
        y: f32,
        host: &mut dyn FnMut(&LayoutRequest<'_>) -> Result<LayoutSnapshot, String>,
    ) -> Result<Option<u32>, String> {
        let handle = self.focused;
        if handle == 0 {
            return Ok(None);
        }
        let (geom, snapshot) = self.field_snapshot(handle, host)?;
        Ok(Some(snap_unit(
            &geom.text,
            snapshot.hit(x - geom.origin_x, y - geom.origin_y),
        )))
    }

    /// Caret rect for a UTF-16 offset in the focused field, viewport coordinates.
    pub fn caret_rect_at(
        &self,
        unit: u32,
        measure: Measure<'_>,
    ) -> Result<Option<(f32, f32, f32, f32)>, String> {
        self.caret_rect_at_with(unit, &mut |req| {
            Ok(wrapping_layout(req, |line| {
                measure(
                    line,
                    req.key.font_size,
                    req.key.line_height,
                    req.key.font_weight,
                    None,
                )
                .0
            }))
        })
    }

    pub fn caret_rect_at_with(
        &self,
        unit: u32,
        host: &mut dyn FnMut(&LayoutRequest<'_>) -> Result<LayoutSnapshot, String>,
    ) -> Result<Option<(f32, f32, f32, f32)>, String> {
        let handle = self.focused;
        if handle == 0 {
            return Ok(None);
        }
        let (geom, snapshot) = self.field_snapshot(handle, host)?;
        let unit = snap_unit(&geom.text, unit).min(snapshot.units);
        let caret = snapshot.caret(unit).ok_or("layout has no caret")?;
        Ok(Some((
            geom.origin_x + caret.x,
            geom.origin_y + caret.y,
            1.0,
            caret.h,
        )))
    }

    pub fn selection_rects(&self) -> &[(f32, f32, f32, f32)] {
        &self.selection_rects
    }

    /// Focus the field under the point and collapse the caret to the nearest
    /// UTF-16 offset. Returns `0` when the point misses every field.
    pub fn focus_point(&mut self, x: f32, y: f32, measure: Measure<'_>) -> Result<u32, String> {
        self.focus_point_with(x, y, &mut |req| {
            Ok(wrapping_layout(req, |line| {
                measure(
                    line,
                    req.key.font_size,
                    req.key.line_height,
                    req.key.font_weight,
                    None,
                )
                .0
            }))
        })
    }

    pub fn focus_point_with(
        &mut self,
        x: f32,
        y: f32,
        host: &mut dyn FnMut(&LayoutRequest<'_>) -> Result<LayoutSnapshot, String>,
    ) -> Result<u32, String> {
        let handle = self.field_at(x, y);
        if handle == 0 {
            return Ok(0);
        }
        self.focus(handle)?;
        let (geom, snapshot) = self.field_snapshot(handle, host)?;
        let unit = snap_unit(
            &geom.text,
            snapshot.hit(x - geom.origin_x, y - geom.origin_y),
        );
        self.edit(EditCommand::SetSelection {
            start: unit,
            end: unit,
        })?;
        Ok(handle)
    }

    /// Deepest text field containing the point, or `0`.
    /// Occlusion, clipping, and hiding match `hit_path`.
    pub fn field_at(&self, x: f32, y: f32) -> u32 {
        let mut path = Vec::new();
        if !self.hit_path(1, x, y, &mut path) {
            return 0;
        }
        path.into_iter()
            .find(|id| self.node(*id).is_ok_and(|node| node.kind == Kind::Field))
            .filter(|handle| self.interaction_available(*handle, self.active_modal()))
            .unwrap_or(0)
    }

    pub fn field(&self, handle: u32) -> Result<(String, FieldState), String> {
        let node = self.node(handle)?;
        if node.kind != Kind::Field {
            return Err(format!("handle {handle} is not a text field"));
        }
        let session = self
            .fields
            .get(&handle)
            .ok_or_else(|| format!("handle {handle} has no editing session"))?;
        Ok((session.value().to_owned(), session.field_state()))
    }

    /// Session identity for a field, used by platform input connections.
    pub fn session_id(&self, handle: u32) -> Result<SessionId, String> {
        self.fields
            .get(&handle)
            .map(EditSession::id)
            .ok_or_else(|| format!("handle {handle} has no editing session"))
    }

    /// Drain writes rejected for a stale revision, an open composition, or a
    /// closed session. Diagnostics never include user text.
    pub fn take_edit_conflicts(&mut self) -> Vec<EditConflict> {
        std::mem::take(&mut self.edit_conflicts)
    }

    /// Open a nested edit batch on the focused field. Commands accumulate
    /// deltas and commit one revision and one script event at the outermost
    /// `end_edit_batch`.
    pub fn begin_edit_batch(&mut self) -> Result<(), String> {
        let handle = self.focused;
        if handle == 0 {
            return Err("no focused field".into());
        }
        if let Some(batch) = self.edit_batch.as_mut() {
            if batch.handle != handle {
                return Err("edit batch targets another field".into());
            }
            let session = self
                .fields
                .get_mut(&handle)
                .ok_or_else(|| format!("handle {handle} has no editing session"))?;
            session.begin_batch().map_err(|err| err.to_string())?;
            batch.depth += 1;
            return Ok(());
        }
        let handler = self.node(handle)?.handler;
        let session = self
            .fields
            .get_mut(&handle)
            .ok_or_else(|| format!("handle {handle} has no editing session"))?;
        session.begin_batch().map_err(|err| err.to_string())?;
        self.edit_batch = Some(EditBatch {
            handle,
            handler,
            depth: 1,
        });
        Ok(())
    }

    /// Close one batch level. The outermost level publishes one `BSED` event
    /// with ordered deltas and the final selection/composition snapshot.
    pub fn end_edit_batch(&mut self) -> Result<(), String> {
        let Some(batch) = self.edit_batch.as_mut() else {
            return Err("no edit batch".into());
        };
        let handle = batch.handle;
        let handler = batch.handler;
        batch.depth -= 1;
        if batch.depth > 0 {
            let session = self
                .fields
                .get_mut(&handle)
                .ok_or_else(|| format!("handle {handle} has no editing session"))?;
            session.end_batch().map_err(|err| err.to_string())?;
            return Ok(());
        }
        self.edit_batch = None;
        let outcome = {
            let session = self
                .fields
                .get_mut(&handle)
                .ok_or_else(|| format!("handle {handle} has no editing session"))?;
            session.end_batch().map_err(|err| err.to_string())?
        };
        if !outcome.changed {
            return Ok(());
        }
        let value = self
            .fields
            .get(&handle)
            .ok_or_else(|| format!("handle {handle} has no editing session"))?
            .value()
            .to_owned();
        if !outcome.deltas.is_empty() {
            self.node_mut(handle)?.text = value.clone();
            self.refresh_measure(handle)?;
        }
        self.push_edit_event(handle, handler, value, outcome);
        self.paint_dirty.set(true);
        Ok(())
    }

    pub fn edit_batch_active(&self) -> bool {
        self.edit_batch.is_some()
    }

    pub fn abort_edit_batch(&mut self) {
        if let Some(batch) = self.edit_batch.take() {
            let restored = self.fields.get_mut(&batch.handle).map(|session| {
                session.abort_batch();
                session.value().to_owned()
            });
            if let Some(value) = restored {
                if let Ok(node) = self.node_mut(batch.handle) {
                    node.text = value;
                }
                let _ = self.refresh_measure(batch.handle);
                self.paint_dirty.set(true);
            }
        }
    }

    /// Mutate the focused field. No-ops that do not change state emit nothing.
    pub fn edit(&mut self, command: EditCommand) -> Result<(), String> {
        let handle = self.focused;
        if handle == 0 {
            return Err("no focused field".into());
        }
        self.edit_handle(handle, command)
    }

    pub fn take_edit_events(&mut self) -> Vec<EditEvent> {
        std::mem::take(&mut self.edit_events)
    }

    pub fn focused_state(&self) -> Result<FieldState, String> {
        let handle = self.focused;
        if handle == 0 {
            return Err("no focused field".into());
        }
        self.fields
            .get(&handle)
            .map(EditSession::field_state)
            .ok_or_else(|| format!("handle {handle} has no editing session"))
    }

    pub fn commit_text(&mut self, text: &str, new_cursor_position: i32) -> Result<(), String> {
        self.apply_focused_session(|session| session.commit_text(text, new_cursor_position))
    }

    pub fn compose_text(&mut self, text: &str, new_cursor_position: i32) -> Result<(), String> {
        self.apply_focused_session(|session| session.compose_text(text, new_cursor_position))
    }

    /// Replace a focused-field range with IME text and a relative cursor.
    pub fn replace_with_cursor(
        &mut self,
        start: u32,
        end: u32,
        text: &str,
        new_cursor_position: i32,
        composing: bool,
    ) -> Result<(), String> {
        self.apply_focused_session(|session| {
            session.replace_with_cursor(
                DocRange::new(start, end),
                text,
                new_cursor_position,
                composing,
            )
        })
    }

    /// Delete units around the focused selection, including an open
    /// composition range.
    pub fn delete_surrounding(&mut self, before: u32, after: u32) -> Result<(), String> {
        self.apply_focused_session(|session| session.delete_surrounding(before, after))
    }

    pub fn delete_surrounding_code_points(
        &mut self,
        before: u32,
        after: u32,
    ) -> Result<(), String> {
        self.apply_focused_session(|session| session.delete_surrounding_code_points(before, after))
    }

    pub fn set_composition_region(&mut self, start: u32, end: u32) -> Result<(), String> {
        self.apply_focused_session(|session| session.set_composition(DocRange::new(start, end)))
    }

    pub fn finish_composition(&mut self) -> Result<(), String> {
        self.apply_focused_session(|session| session.end_composition(None, None))
    }

    fn apply_focused_session(
        &mut self,
        op: impl FnOnce(&mut EditSession) -> Result<EditOutcome, EditError>,
    ) -> Result<(), String> {
        let handle = self.focused;
        if handle == 0 {
            return Err("no focused field".into());
        }
        let (handler, config) = {
            let node = self.node(handle)?;
            if node.kind != Kind::Field {
                return Err(format!("handle {handle} is not a text field"));
            }
            (
                node.handler,
                EditConfig {
                    multiline: node.multiline,
                    max_length: node.max_length,
                },
            )
        };
        let result = {
            let session = self
                .fields
                .get_mut(&handle)
                .ok_or_else(|| format!("handle {handle} has no editing session"))?;
            session.set_config(config);
            op(session)
        };
        let outcome = match result {
            Ok(outcome) => outcome,
            Err(err) => {
                self.rollback_failed_batch(handle);
                return Err(err.to_string());
            }
        };
        self.finish_edit(handle, handler, outcome)
    }

    fn rollback_failed_batch(&mut self, handle: u32) {
        if self.edit_batch.is_none() {
            return;
        }
        let restored = self.fields.get_mut(&handle).map(|session| {
            session.abort_batch();
            session.value().to_owned()
        });
        if let Some(value) = restored {
            if let Ok(node) = self.node_mut(handle) {
                node.text = value;
            }
            let _ = self.refresh_measure(handle);
        }
        self.edit_batch = None;
    }

    fn edit_handle(&mut self, handle: u32, command: EditCommand) -> Result<(), String> {
        let (handler, config) = {
            let node = self.node(handle)?;
            if node.kind != Kind::Field {
                return Err(format!("handle {handle} is not a text field"));
            }
            (
                node.handler,
                EditConfig {
                    multiline: node.multiline,
                    max_length: node.max_length,
                },
            )
        };
        let result = {
            let session = self
                .fields
                .get_mut(&handle)
                .ok_or_else(|| format!("handle {handle} has no editing session"))?;
            session.set_config(config);
            match command {
                EditCommand::Insert(text) => session.insert(&text),
                EditCommand::Backspace => session.apply(EditOp::Backspace, None),
                EditCommand::DeleteForward => session.apply(EditOp::DeleteForward, None),
                EditCommand::SetSelection { start, end } => {
                    session.apply(EditOp::Select(Selection::new(start, end)), None)
                }
                EditCommand::CompositionStart => session.begin_composition(),
                EditCommand::CompositionUpdate(text) => session.composition_update(&text),
                EditCommand::CompositionEnd(text) => session.end_composition(Some(&text), None),
                EditCommand::Undo => session.apply(EditOp::Undo, None),
                EditCommand::Redo => session.apply(EditOp::Redo, None),
            }
        };
        let outcome = match result {
            Ok(outcome) => outcome,
            Err(err) => {
                self.rollback_failed_batch(handle);
                return Err(err.to_string());
            }
        };
        self.finish_edit(handle, handler, outcome)
    }

    fn finish_edit(
        &mut self,
        handle: u32,
        handler: u32,
        outcome: EditOutcome,
    ) -> Result<(), String> {
        if !outcome.changed {
            return Ok(());
        }
        let value = self
            .fields
            .get(&handle)
            .ok_or_else(|| format!("handle {handle} has no editing session"))?
            .value()
            .to_owned();
        if !outcome.deltas.is_empty() || self.edit_batch.is_some() {
            self.node_mut(handle)?.text = value.clone();
        }
        if self.edit_batch.is_some() {
            // Reads see the live document; layout and the script event wait
            // for the outermost end_edit_batch.
            self.paint_dirty.set(true);
            return Ok(());
        }
        if !outcome.deltas.is_empty() {
            self.refresh_measure(handle)?;
        }
        self.push_edit_event(handle, handler, value, outcome);
        self.paint_dirty.set(true);
        Ok(())
    }

    fn push_edit_event(&mut self, handle: u32, handler: u32, value: String, outcome: EditOutcome) {
        let state = outcome.snapshot;
        self.edit_events.push(EditEvent {
            handle,
            handler,
            session: state.session,
            revision: state.revision,
            deltas: outcome.deltas,
            selection: state.selection,
            composition: state.composition,
            composing: state.composing,
            value,
        });
    }

    pub(super) fn set_editing_value(
        &mut self,
        handle: u32,
        epoch: u64,
        next: &str,
        sel_start: u32,
        sel_end: u32,
        cancel: bool,
    ) -> Result<(), String> {
        let (handler, config) = {
            let node = self.node(handle)?;
            if node.kind != Kind::Field {
                return Err(format!("handle {handle} is not a text field"));
            }
            (
                node.handler,
                EditConfig {
                    multiline: node.multiline,
                    max_length: node.max_length,
                },
            )
        };
        let session = self
            .fields
            .get_mut(&handle)
            .ok_or_else(|| format!("handle {handle} has no editing session"))?;
        let session_id = session.id();
        session.set_config(config);
        let outcome =
            match session.script_reset(epoch, next, Selection::new(sel_start, sel_end), cancel) {
                Ok(outcome) => outcome,
                Err(error) => {
                    self.edit_conflicts.push(EditConflict {
                        handle,
                        session: session_id,
                        revision: session.revision(),
                        error,
                    });
                    return Ok(());
                }
            };
        self.finish_edit(handle, handler, outcome)
    }

    /// Recompute caret and selection from the current platform snapshot.
    /// An old revision or a mismatched width/font/align/density is an error.
    pub fn refresh_caret(
        &mut self,
        host: &mut dyn FnMut(&LayoutRequest<'_>) -> Result<LayoutSnapshot, String>,
    ) -> Result<(), String> {
        self.resolve_field_long_press(host);
        self.caret_handle = 0;
        self.caret_box = None;
        self.selection_rects.clear();
        self.selection_handles = None;
        let handle = self.focused;
        if handle == 0 {
            self.caret_reveal = None;
            return Ok(());
        }
        let Some(state) = self.fields.get(&handle).cloned() else {
            self.caret_reveal = None;
            return Ok(());
        };
        if self
            .node(handle)
            .ok()
            .filter(|node| node.kind == Kind::Field)
            .is_none()
        {
            self.caret_reveal = None;
            return Ok(());
        }
        let (geom, snapshot) = self.field_snapshot(handle, host)?;
        let active = snap_unit(&geom.text, state.sel_end()).min(snapshot.units);
        let caret = snapshot.caret(active).ok_or("layout has no caret")?;
        // Auto-scroll follows the caret only when it appears or moves; a
        // scrolled-away caret must not be snapped back on every layout.
        let reveal = match self.caret_reveal {
            Some((last_handle, last_offset, last_x, last_y)) => {
                last_handle != handle
                    || last_offset != active
                    || (last_x - caret.x).abs() > 0.5
                    || (last_y - caret.y).abs() > 0.5
            }
            None => true,
        };
        if reveal {
            self.reveal_field_caret(handle, caret.x, caret.y, caret.h)?;
            self.caret_reveal = Some((handle, active, caret.x, caret.y));
        }
        let geom = self.field_geom(handle)?;
        self.caret_handle = handle;
        self.selection_rects = snapshot
            .selection_rects(state.sel_start(), state.sel_end())
            .into_iter()
            .map(|[x, y, w, h]| (geom.origin_x + x, geom.origin_y + y, w, h))
            .collect();
        self.caret_box = Some((
            geom.origin_x + caret.x,
            geom.origin_y + caret.y,
            1.0,
            caret.h,
        ));
        let (start, end) = (state.sel_start(), state.sel_end());
        if start != end {
            let start = snap_unit(&geom.text, start).min(snapshot.units);
            let end = snap_unit(&geom.text, end).min(snapshot.units);
            if let (Some(a), Some(b)) = (snapshot.caret(start), snapshot.caret(end)) {
                self.selection_handles = Some([
                    (
                        geom.origin_x + a.x - 8.0,
                        geom.origin_y + a.y + a.h - 4.0,
                        16.0,
                        16.0,
                    ),
                    (
                        geom.origin_x + b.x - 8.0,
                        geom.origin_y + b.y + b.h - 4.0,
                        16.0,
                        16.0,
                    ),
                ]);
            }
        }
        Ok(())
    }

    fn field_snapshot(
        &self,
        handle: u32,
        host: &mut dyn FnMut(&LayoutRequest<'_>) -> Result<LayoutSnapshot, String>,
    ) -> Result<(FieldGeom, LayoutSnapshot), String> {
        let geom = self.field_geom(handle)?;
        let req = LayoutRequest {
            text: &geom.text,
            key: geom.key,
        };
        let snapshot = host(&req)?;
        if snapshot.units != utf16_len(&geom.text)
            || snapshot.carets.len() != snapshot.units as usize + 1
        {
            return Err("layout snapshot does not cover the field text".into());
        }
        if !snapshot.matches(&geom.key) {
            return Err("stale layout snapshot".into());
        }
        Ok((geom, snapshot))
    }

    fn field_geom(&self, handle: u32) -> Result<FieldGeom, String> {
        let node = self.node(handle)?;
        if node.kind != Kind::Field {
            return Err(format!("handle {handle} is not a text field"));
        }
        let pads = self.pads(handle);
        let font_size = node.font_size;
        let line_height = if node.line_height > 0.0 {
            node.line_height
        } else {
            font_size * 1.2
        };
        let revision = self
            .fields
            .get(&handle)
            .map(|state| state.revision())
            .unwrap_or(0);
        let viewport_w = (node.w - pads[0] - pads[1]).max(0.0);
        let width = if node.multiline {
            viewport_w
        } else {
            node.content_w.max(viewport_w)
        };
        Ok(FieldGeom {
            text: display_text(&node.text, node.secure),
            origin_x: node.x + pads[0] - node.scroll_left,
            origin_y: node.y + pads[2] - node.scroll_top,
            key: LayoutKey {
                revision,
                width,
                font_size,
                font_weight: node.font_weight,
                line_height,
                align: self.effective_paint_style(handle, node).text_align,
                density: 1.0,
            },
        })
    }
    /// Bring the caret into the field viewport and clamp to the measured text.
    fn reveal_field_caret(
        &mut self,
        handle: u32,
        caret_x: f32,
        caret_y: f32,
        caret_h: f32,
    ) -> Result<(), String> {
        let pads = self.pads(handle);
        let node = self.node_mut(handle)?;
        if node.kind != Kind::Field {
            return Ok(());
        }
        let viewport_w = (node.w - pads[0] - pads[1]).max(0.0);
        let viewport_h = (node.h - pads[2] - pads[3]).max(0.0);
        node.content_w = node.content_w.max(caret_x + 1.0);
        node.content_h = node.content_h.max(caret_y + caret_h);
        if !node.multiline && node.scrolls_x() {
            let visible_end = node.scroll_left + viewport_w;
            if caret_x < node.scroll_left {
                node.scroll_left = (caret_x - 20.0).max(0.0);
            } else if caret_x > visible_end {
                node.scroll_left = caret_x - viewport_w + 20.0;
            }
        } else if node.multiline && node.scrolls_y() {
            let caret_bottom = caret_y + caret_h;
            let visible_end = node.scroll_top + viewport_h;
            if caret_y < node.scroll_top {
                node.scroll_top = (caret_y - 4.0).max(0.0);
            } else if caret_bottom > visible_end {
                node.scroll_top = caret_bottom - viewport_h + 4.0;
            }
        }
        if node.scrolls_x() {
            let max_scroll = (node.content_w - viewport_w).max(0.0);
            node.scroll_left = node.scroll_left.clamp(0.0, max_scroll);
        }
        if node.scrolls_y() {
            let max_scroll = (node.content_h - viewport_h).max(0.0);
            node.scroll_top = node.scroll_top.clamp(0.0, max_scroll);
        }
        Ok(())
    }
}

fn caret_step(value: &str, offset: u32, forward: bool) -> u32 {
    let mut seen = 0u32;
    if forward {
        for ch in value.chars() {
            let width = ch.len_utf16() as u32;
            if seen + width > offset {
                return seen + width;
            }
            seen += width;
        }
        utf16_len(value)
    } else {
        let mut previous = 0u32;
        for ch in value.chars() {
            let width = ch.len_utf16() as u32;
            if seen >= offset {
                return previous;
            }
            previous = seen;
            seen += width;
        }
        previous
    }
}

fn line_edge(value: &str, offset: u32, forward: bool) -> u32 {
    let mut seen = 0u32;
    if forward {
        for ch in value.chars() {
            if seen >= offset && ch == '\n' {
                return seen;
            }
            seen += ch.len_utf16() as u32;
        }
        utf16_len(value)
    } else {
        let mut line_start = 0u32;
        for ch in value.chars() {
            if seen >= offset {
                return line_start;
            }
            seen += ch.len_utf16() as u32;
            if ch == '\n' {
                line_start = seen;
            }
        }
        line_start
    }
}

pub(super) fn utf16_slice(value: &str, start: u32, end: u32) -> String {
    let mut out = String::new();
    let mut seen = 0u32;
    for ch in value.chars() {
        let width = ch.len_utf16() as u32;
        if seen >= end {
            break;
        }
        if seen >= start {
            out.push(ch);
        }
        seen += width;
    }
    out
}
