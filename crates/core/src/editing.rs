//! UI-thread text editing. Offsets are UTF-16 code units so IME agents on
//! iOS, Android, and the web share one selection space. A surrogate pair is
//! one scalar: a boundary inside the pair snaps backward, and backspace or
//! delete removes the whole scalar.
//!
//! [`EditSession`] is the authoritative document and edit state for one text
//! field. Platform adapters read [`FieldState`] snapshots and submit
//! [`EditOp`] transactions. Every committed change bumps a monotonic
//! [`Revision`]; batches commit several operations atomically and publish one
//! revision.

use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

pub type Revision = u64;

static NEXT_SESSION: AtomicU64 = AtomicU64::new(1);

/// Identity of one field editing session. A closed session never accepts
/// another transaction, even if a handle is reused by a later node.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SessionId(u64);

impl SessionId {
    fn allocate() -> Self {
        Self(NEXT_SESSION.fetch_add(1, Ordering::Relaxed))
    }

    pub fn get(self) -> u64 {
        self.0
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Which side of an insertion boundary a document position sticks to.
/// Reserved for future multi-paragraph, styled, or atomic content; plain text
/// currently treats both sides the same.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Affinity {
    Upstream,
    #[default]
    Downstream,
}

/// A document position in UTF-16 code units plus its boundary affinity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DocPosition {
    pub offset: u32,
    pub affinity: Affinity,
}

impl DocPosition {
    pub const fn new(offset: u32) -> Self {
        Self {
            offset,
            affinity: Affinity::Downstream,
        }
    }

    pub const fn with_affinity(offset: u32, affinity: Affinity) -> Self {
        Self { offset, affinity }
    }
}

/// A normalized document range: `start.offset <= end.offset`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DocRange {
    pub start: DocPosition,
    pub end: DocPosition,
}

impl DocRange {
    pub fn new(start: u32, end: u32) -> Self {
        Self::from_positions(DocPosition::new(start), DocPosition::new(end))
    }

    pub fn from_positions(a: DocPosition, b: DocPosition) -> Self {
        if a.offset <= b.offset {
            Self { start: a, end: b }
        } else {
            Self { start: b, end: a }
        }
    }

    pub fn offsets(self) -> (u32, u32) {
        (self.start.offset, self.end.offset)
    }
}

/// A directional selection. `anchor` is the fixed end; `active` is the caret
/// end. A caret is a collapsed selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selection {
    pub anchor: DocPosition,
    pub active: DocPosition,
}

impl Selection {
    pub fn new(anchor: u32, active: u32) -> Self {
        Self {
            anchor: DocPosition::new(anchor),
            active: DocPosition::new(active),
        }
    }

    pub fn from_positions(anchor: DocPosition, active: DocPosition) -> Self {
        Self { anchor, active }
    }

    pub fn collapsed(offset: u32) -> Self {
        Self {
            anchor: DocPosition::new(offset),
            active: DocPosition::new(offset),
        }
    }

    /// Ordered selection offsets, independent of direction.
    pub fn range(self) -> (u32, u32) {
        if self.anchor.offset <= self.active.offset {
            (self.anchor.offset, self.active.offset)
        } else {
            (self.active.offset, self.anchor.offset)
        }
    }

    pub fn is_collapsed(self) -> bool {
        self.anchor.offset == self.active.offset
    }
}

/// Synchronous edit rules for a field. `max_length` is UTF-16 code units;
/// `0` means unbounded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EditConfig {
    pub multiline: bool,
    pub max_length: u32,
}

impl Default for EditConfig {
    fn default() -> Self {
        Self {
            multiline: false,
            max_length: 0,
        }
    }
}

impl EditConfig {
    fn limit(self) -> u32 {
        if self.max_length == 0 {
            u32::MAX
        } else {
            self.max_length
        }
    }
}

/// A rejected transaction. Conflict results never carry user text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EditError {
    /// The session was closed by focus loss, disposal, or host pause.
    Closed,
    /// The command was addressed to another session generation.
    SessionMismatch {
        expected: SessionId,
        actual: SessionId,
    },
    /// The caller observed an older or different revision.
    StaleRevision {
        expected: Revision,
        actual: Revision,
    },
    /// An application write arrived while an IME composition is open.
    CompositionConflict,
    /// The operation is not allowed inside an open batch.
    BatchActive,
    /// `end_batch` without a matching `begin_batch`.
    BatchUnderflow,
}

impl fmt::Display for EditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Closed => write!(f, "editing session is closed"),
            Self::SessionMismatch { expected, actual } => {
                write!(f, "editing session {expected} is not {actual}")
            }
            Self::StaleRevision { expected, actual } => {
                write!(f, "revision {expected} is stale; current is {actual}")
            }
            Self::CompositionConflict => write!(f, "composition is open"),
            Self::BatchActive => write!(f, "edit batch is open"),
            Self::BatchUnderflow => write!(f, "end_batch without begin_batch"),
        }
    }
}

impl std::error::Error for EditError {}

/// Caller identity and revision expectation for a transaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Expect {
    /// Only the session identity is checked.
    Session(SessionId),
    /// Both session identity and exact revision are checked.
    Revision {
        session: SessionId,
        revision: Revision,
    },
}

/// One replacement in the ordered edit stream. `start..end` are old document
/// offsets; `insert` is the replacement text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditDelta {
    pub start: u32,
    pub end: u32,
    pub insert: String,
}

/// Result of a transaction or batch commit. Deltas replay `before` to the
/// final snapshot; the snapshot is authoritative for selection and
/// composition. Inside an open batch, deltas stay in the batch and are
/// returned by `end_batch`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditOutcome {
    pub changed: bool,
    pub revision: Revision,
    pub snapshot: FieldState,
    pub deltas: Vec<EditDelta>,
}

/// Read-only state of a field session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldState {
    pub session: SessionId,
    pub revision: Revision,
    pub selection: Selection,
    /// Normalized UTF-16 range of the active composition.
    pub composition: Option<(u32, u32)>,
    pub composing: bool,
}

impl FieldState {
    pub fn sel_start(&self) -> u32 {
        self.selection.range().0
    }

    pub fn sel_end(&self) -> u32 {
        self.selection.range().1
    }

    pub fn anchor(&self) -> u32 {
        self.selection.anchor.offset
    }

    pub fn active(&self) -> u32 {
        self.selection.active.offset
    }
}

/// How a replacement affects the active composition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompositionChange {
    /// Leave the composition untouched.
    Keep,
    /// Clear the composition range; composition activity is unchanged.
    Clear,
    /// Set the composition range and activate composition if needed.
    Set(DocRange),
}

/// A transaction primitive. Range replacements carry their resulting
/// selection explicitly, so callers can apply IME `newCursorPosition`
/// without leaking platform indices into the core.
#[derive(Clone, Debug, PartialEq)]
pub enum EditOp {
    Insert(String),
    Backspace,
    DeleteForward,
    Replace {
        range: DocRange,
        text: String,
        selection: Option<Selection>,
        composition: CompositionChange,
    },
    DeleteRange(DocRange),
    Select(Selection),
    BeginComposition,
    SetComposition(DocRange),
    EndComposition {
        text: Option<String>,
        selection: Option<Selection>,
    },
    CancelComposition,
    Undo,
    Redo,
    Reset {
        value: String,
        selection: Selection,
    },
}

/// Compatibility command surface used by the current platform bridges. New
/// bridges should submit [`EditOp`] transactions instead.
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

#[derive(Clone, Debug, PartialEq, Eq)]
struct Snapshot {
    value: String,
    selection: Selection,
    composition: Option<(u32, u32)>,
    composing: bool,
}

#[derive(Clone, Debug)]
struct HistoryEntry {
    value: String,
    selection: Selection,
}

#[derive(Clone, Debug, Default)]
struct EditHistory {
    undo: Vec<HistoryEntry>,
    redo: Vec<HistoryEntry>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HistoryMode {
    Push,
    None,
}

#[derive(Clone, Debug, Default)]
struct BatchState {
    depth: u32,
    base: Option<Snapshot>,
    base_revision: Revision,
    changed: bool,
    deltas: Vec<EditDelta>,
}

/// Authoritative document plus selection, composition, revision, and undo
/// history for one text field.
#[derive(Clone, Debug)]
pub struct EditSession {
    id: SessionId,
    value: String,
    selection: Selection,
    composition: Option<(u32, u32)>,
    composing: bool,
    revision: Revision,
    config: EditConfig,
    history: EditHistory,
    composition_base: Option<Snapshot>,
    batch: BatchState,
    closed: bool,
}

impl EditSession {
    pub fn new(value: &str, config: EditConfig) -> Self {
        let end = utf16_len(value);
        Self {
            id: SessionId::allocate(),
            value: value.to_owned(),
            selection: Selection::collapsed(end),
            composition: None,
            composing: false,
            revision: 0,
            config,
            history: EditHistory::default(),
            composition_base: None,
            batch: BatchState::default(),
            closed: false,
        }
    }

    pub fn id(&self) -> SessionId {
        self.id
    }

    pub fn revision(&self) -> Revision {
        self.revision
    }

    pub fn value(&self) -> &str {
        &self.value
    }

    pub fn config(&self) -> EditConfig {
        self.config
    }

    pub fn set_config(&mut self, config: EditConfig) {
        self.config = config;
    }

    pub fn field_state(&self) -> FieldState {
        FieldState {
            session: self.id,
            revision: self.revision,
            selection: self.selection,
            composition: self.composition,
            composing: self.composing,
        }
    }

    pub fn sel_start(&self) -> u32 {
        self.selection.range().0
    }

    pub fn sel_end(&self) -> u32 {
        self.selection.range().1
    }

    pub fn is_closed(&self) -> bool {
        self.closed
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            value: self.value.clone(),
            selection: self.selection,
            composition: self.composition,
            composing: self.composing,
        }
    }

    /// Invalidate the session. Any open batch is discarded without a commit.
    pub fn close(&mut self) {
        if self.batch.depth > 0 {
            self.rollback_batch();
        }
        self.composition_base = None;
        self.composing = false;
        self.composition = None;
        self.closed = true;
    }

    pub fn begin_batch(&mut self) -> Result<(), EditError> {
        if self.closed {
            return Err(EditError::Closed);
        }
        if self.batch.depth == 0 {
            self.batch.base = Some(self.snapshot());
            self.batch.base_revision = self.revision;
            self.batch.changed = false;
            self.batch.deltas.clear();
        }
        self.batch.depth += 1;
        Ok(())
    }

    /// Commit the outermost batch. Nested ends return an unchanged outcome
    /// until the outermost batch closes.
    pub fn end_batch(&mut self) -> Result<EditOutcome, EditError> {
        if self.closed {
            return Err(EditError::Closed);
        }
        if self.batch.depth == 0 {
            return Err(EditError::BatchUnderflow);
        }
        self.batch.depth -= 1;
        if self.batch.depth > 0 {
            return Ok(self.unchanged_outcome());
        }
        let base = self.batch.base.take();
        let changed = self.batch.changed;
        let deltas = std::mem::take(&mut self.batch.deltas);
        if changed {
            self.revision += 1;
            if let Some(base) = base {
                self.push_undo(base);
            }
        }
        Ok(EditOutcome {
            changed,
            revision: self.revision,
            snapshot: self.field_state(),
            deltas: if changed { deltas } else { Vec::new() },
        })
    }

    /// Discard an open batch and restore its starting state.
    pub fn abort_batch(&mut self) {
        self.rollback_batch();
    }

    fn rollback_batch(&mut self) {
        if let Some(base) = self.batch.base.take() {
            self.value = base.value;
            self.selection = base.selection;
            self.composition = base.composition;
            self.composing = base.composing;
            self.revision = self.batch.base_revision;
        }
        self.batch.depth = 0;
        self.batch.changed = false;
        self.batch.deltas.clear();
    }

    /// Submit one transaction. An error inside a batch discards the entire
    /// uncommitted batch, leaving no partial edit.
    pub fn apply(&mut self, op: EditOp, expect: Option<Expect>) -> Result<EditOutcome, EditError> {
        if self.closed {
            return Err(EditError::Closed);
        }
        let result = self
            .check_expect(expect)
            .and_then(|()| self.apply_inner(op));
        if result.is_err() && self.batch.depth > 0 {
            self.rollback_batch();
        }
        result
    }

    fn apply_inner(&mut self, op: EditOp) -> Result<EditOutcome, EditError> {
        match op {
            EditOp::Insert(text) => self.insert(&text),
            EditOp::Backspace => self.delete_unit(false),
            EditOp::DeleteForward => self.delete_unit(true),
            EditOp::Replace {
                range,
                text,
                selection,
                composition,
            } => self.replace(range, &text, selection, composition),
            EditOp::DeleteRange(range) => self.replace(range, "", None, CompositionChange::Keep),
            EditOp::Select(selection) => self.select(selection),
            EditOp::BeginComposition => self.begin_composition(),
            EditOp::SetComposition(range) => self.set_composition(range),
            EditOp::EndComposition { text, selection } => {
                self.end_composition(text.as_deref(), selection)
            }
            EditOp::CancelComposition => self.cancel_composition(),
            EditOp::Undo => self.undo(),
            EditOp::Redo => self.redo(),
            EditOp::Reset { value, selection } => self.reset(&value, selection),
        }
    }

    fn check_expect(&self, expect: Option<Expect>) -> Result<(), EditError> {
        match expect {
            None => Ok(()),
            Some(Expect::Session(id)) => {
                if id != self.id {
                    return Err(EditError::SessionMismatch {
                        expected: id,
                        actual: self.id,
                    });
                }
                Ok(())
            }
            Some(Expect::Revision { session, revision }) => {
                if session != self.id {
                    return Err(EditError::SessionMismatch {
                        expected: session,
                        actual: self.id,
                    });
                }
                if revision != self.revision {
                    return Err(EditError::StaleRevision {
                        expected: revision,
                        actual: self.revision,
                    });
                }
                Ok(())
            }
        }
    }

    pub fn insert(&mut self, text: &str) -> Result<EditOutcome, EditError> {
        if self.composing {
            return Err(EditError::CompositionConflict);
        }
        let (start, end) = self.selection.range();
        let text = sanitize(text, self.config.multiline);
        let before = self.snapshot();
        let (_, inserted_end) = self.splice(start, end, &text);
        self.selection = Selection::collapsed(inserted_end);
        Ok(self.commit(before, HistoryMode::Push))
    }

    fn delete_unit(&mut self, forward: bool) -> Result<EditOutcome, EditError> {
        if self.composing {
            return Err(EditError::CompositionConflict);
        }
        let (start, end) = self.selection.range();
        let (start, end) = if start != end {
            (start, end)
        } else if forward {
            let next = next_scalar_boundary(&self.value, start);
            if next == start {
                return Ok(self.unchanged_outcome());
            }
            (start, next)
        } else {
            let prev = prev_scalar_boundary(&self.value, start);
            if prev == start {
                return Ok(self.unchanged_outcome());
            }
            (prev, start)
        };
        let before = self.snapshot();
        let (deleted_at, _) = self.splice(start, end, "");
        self.selection = Selection::collapsed(deleted_at);
        Ok(self.commit(before, HistoryMode::Push))
    }

    fn replace(
        &mut self,
        range: DocRange,
        text: &str,
        selection: Option<Selection>,
        composition: CompositionChange,
    ) -> Result<EditOutcome, EditError> {
        let (start, end) = self.normalize_range(range);
        let text = sanitize(text, self.config.multiline);
        let before = self.snapshot();
        if let CompositionChange::Set(_) = composition {
            if !self.composing {
                self.composition_base = Some(before.clone());
                self.composing = true;
            }
        }
        let (_, inserted_end) = self.splice(start, end, &text);
        self.selection = match selection {
            Some(selection) => self.normalize_selection(selection),
            None => Selection::collapsed(inserted_end),
        };
        match composition {
            CompositionChange::Keep => {
                if let Some((comp_start, comp_end)) = self.composition {
                    self.composition = Some(self.normalize_offsets(comp_start, comp_end));
                }
            }
            CompositionChange::Clear => self.composition = None,
            CompositionChange::Set(range) => {
                self.composition = Some(self.normalize_range(range));
            }
        }
        Ok(self.commit(before, HistoryMode::Push))
    }

    fn select(&mut self, selection: Selection) -> Result<EditOutcome, EditError> {
        let selection = self.normalize_selection(selection);
        let before = self.snapshot();
        self.selection = selection;
        let mode = if self.composing {
            HistoryMode::None
        } else {
            HistoryMode::Push
        };
        Ok(self.commit(before, mode))
    }

    pub fn begin_composition(&mut self) -> Result<EditOutcome, EditError> {
        let before = self.snapshot();
        if !self.composing {
            self.composition_base = Some(before.clone());
            self.composing = true;
            self.composition = None;
            return Ok(self.unchanged_outcome());
        }
        let had_range = self.composition.take().is_some();
        if had_range {
            Ok(self.commit(before, HistoryMode::None))
        } else {
            Ok(self.unchanged_outcome())
        }
    }

    pub fn set_composition(&mut self, range: DocRange) -> Result<EditOutcome, EditError> {
        let before = self.snapshot();
        if !self.composing {
            self.composition_base = Some(before.clone());
            self.composing = true;
        }
        self.composition = Some(self.normalize_range(range));
        Ok(self.commit(before, HistoryMode::None))
    }

    /// Commit IME text over the active composition, or the selection when no
    /// composition range exists. The composition becomes one history step.
    pub fn commit_text(
        &mut self,
        text: &str,
        new_cursor_position: i32,
    ) -> Result<EditOutcome, EditError> {
        let range = self
            .composition
            .map(|(start, end)| DocRange::new(start, end))
            .unwrap_or_else(|| {
                let (start, end) = self.selection.range();
                DocRange::new(start, end)
            });
        self.replace_with_cursor(range, text, new_cursor_position, false)
    }

    /// Replace the active composition (or selection) with new composing text.
    pub fn compose_text(
        &mut self,
        text: &str,
        new_cursor_position: i32,
    ) -> Result<EditOutcome, EditError> {
        let range = self
            .composition
            .map(|(start, end)| DocRange::new(start, end))
            .unwrap_or_else(|| {
                let (start, end) = self.selection.range();
                DocRange::new(start, end)
            });
        self.replace_with_cursor(range, text, new_cursor_position, true)
    }

    /// Replace a range with IME text and apply Android's `newCursorPosition`
    /// relative to the fitted insertion. `composing` keeps or starts a
    /// composition over the inserted span; otherwise any open composition is
    /// committed and the replacement becomes one history step.
    pub fn replace_with_cursor(
        &mut self,
        range: DocRange,
        text: &str,
        new_cursor_position: i32,
        composing: bool,
    ) -> Result<EditOutcome, EditError> {
        if self.closed {
            return Err(EditError::Closed);
        }
        let (start, end) = self.normalize_range(range);
        let text = sanitize(text, self.config.multiline);
        let before = self.snapshot();
        let was_composing = self.composing;
        let base = self.composition_base.clone();
        if composing && !self.composing {
            self.composition_base = Some(before.clone());
            self.composing = true;
        }
        let (insert_start, insert_end) = self.splice(start, end, &text);
        let len = utf16_len(&self.value);
        let cursor = cursor_from_position(insert_start, insert_end, new_cursor_position, len);
        self.selection = Selection::collapsed(cursor);
        if composing {
            self.composition = Some((insert_start, insert_end));
            return Ok(self.commit(before, HistoryMode::None));
        }
        self.composition = None;
        self.composing = false;
        self.composition_base = None;
        let outcome = self.commit(
            before,
            if was_composing {
                HistoryMode::None
            } else {
                HistoryMode::Push
            },
        );
        if was_composing {
            if let Some(base) = base {
                if base.value != self.value || base.selection != self.selection {
                    self.push_undo(base);
                }
            }
        }
        Ok(outcome)
    }

    /// Delete `before_units` and `after_units` around the selection, expanding
    /// the range to cover an open composition. The caret collapses at the
    /// start of the deleted range.
    pub fn delete_surrounding(
        &mut self,
        before_units: u32,
        after_units: u32,
    ) -> Result<EditOutcome, EditError> {
        if self.closed {
            return Err(EditError::Closed);
        }
        let (start, end) = self.surrounding_range();
        let len = utf16_len(&self.value);
        let start = normalize_offset(&self.value, start.saturating_sub(before_units));
        let end = normalize_offset(
            &self.value,
            end.saturating_add(after_units).min(len),
        );
        self.delete_units(start, end)
    }

    /// Delete code points around the selection. Unlike UTF-16 unit counts,
    /// this never splits a surrogate pair and counts each scalar once.
    pub fn delete_surrounding_code_points(
        &mut self,
        before: u32,
        after: u32,
    ) -> Result<EditOutcome, EditError> {
        if self.closed {
            return Err(EditError::Closed);
        }
        let (start, end) = self.surrounding_range();
        let start = walk_scalars_back(&self.value, start, before);
        let end = walk_scalars_forward(&self.value, end, after);
        self.delete_units(start, end)
    }

    fn surrounding_range(&self) -> (u32, u32) {
        let (mut start, mut end) = self.selection.range();
        if let Some((comp_start, comp_end)) = self.composition {
            start = start.min(comp_start);
            end = end.max(comp_end);
        }
        (start, end)
    }

    fn delete_units(&mut self, start: u32, end: u32) -> Result<EditOutcome, EditError> {
        let (start, end) = self.normalize_offsets(start, end);
        let before = self.snapshot();
        let was_composing = self.composing;
        let base = self.composition_base.clone();
        let (deleted_at, _) = self.splice(start, end, "");
        self.selection = Selection::collapsed(deleted_at);
        self.composition = None;
        self.composing = false;
        self.composition_base = None;
        let outcome = self.commit(
            before,
            if was_composing {
                HistoryMode::None
            } else {
                HistoryMode::Push
            },
        );
        if was_composing {
            if let Some(base) = base {
                if base.value != self.value || base.selection != self.selection {
                    self.push_undo(base);
                }
            }
        }
        Ok(outcome)
    }

    /// Replace the current composition range with `text` and collapse the
    /// caret after it. Used by the legacy `CompositionUpdate` command; the
    /// transaction API can set selection and composition independently.
    pub fn composition_update(&mut self, text: &str) -> Result<EditOutcome, EditError> {
        if !self.composing {
            return Err(EditError::CompositionConflict);
        }
        let (start, end) = self.composition.unwrap_or_else(|| self.selection.range());
        let text = sanitize(text, self.config.multiline);
        let before = self.snapshot();
        let (inserted_start, inserted_end) = self.splice(start, end, &text);
        self.selection = Selection::collapsed(inserted_end);
        self.composition = Some((inserted_start, inserted_end));
        Ok(self.commit(before, HistoryMode::None))
    }

    pub fn end_composition(
        &mut self,
        text: Option<&str>,
        selection: Option<Selection>,
    ) -> Result<EditOutcome, EditError> {
        if !self.composing {
            return Err(EditError::CompositionConflict);
        }
        let before = self.snapshot();
        let base = self.composition_base.take();
        if let Some(text) = text {
            let text = sanitize(text, self.config.multiline);
            let (start, end) = self.composition.unwrap_or_else(|| self.selection.range());
            let (_, inserted_end) = self.splice(start, end, &text);
            self.selection = match selection {
                Some(selection) => self.normalize_selection(selection),
                None => Selection::collapsed(inserted_end),
            };
        } else if let Some(selection) = selection {
            self.selection = self.normalize_selection(selection);
        }
        self.composing = false;
        self.composition = None;
        let outcome = self.commit(before, HistoryMode::None);
        if self.batch.depth == 0 {
            if let Some(base) = base {
                if base.value != self.value || base.selection != self.selection {
                    self.push_undo(base);
                }
            }
        }
        Ok(outcome)
    }

    fn cancel_composition(&mut self) -> Result<EditOutcome, EditError> {
        if !self.composing {
            return Ok(self.unchanged_outcome());
        }
        let before = self.snapshot();
        if let Some(base) = self.composition_base.take() {
            self.value = base.value;
            self.selection = base.selection;
        }
        self.composing = false;
        self.composition = None;
        if before.value != self.value
            || before.selection != self.selection
            || before.composition != self.composition
        {
            Ok(self.commit(before, HistoryMode::None))
        } else {
            Ok(self.unchanged_outcome())
        }
    }

    fn undo(&mut self) -> Result<EditOutcome, EditError> {
        if self.batch.depth > 0 {
            return Err(EditError::BatchActive);
        }
        if self.composing {
            return self.cancel_composition();
        }
        let Some(entry) = self.history.undo.pop() else {
            return Ok(self.unchanged_outcome());
        };
        let before = self.snapshot();
        self.history.redo.push(HistoryEntry {
            value: self.value.clone(),
            selection: self.selection,
        });
        self.value = entry.value;
        self.selection = entry.selection;
        self.composition = None;
        self.composing = false;
        self.composition_base = None;
        Ok(self.commit(before, HistoryMode::None))
    }

    fn redo(&mut self) -> Result<EditOutcome, EditError> {
        if self.batch.depth > 0 {
            return Err(EditError::BatchActive);
        }
        if self.composing {
            let _ = self.cancel_composition();
        }
        let Some(entry) = self.history.redo.pop() else {
            return Ok(self.unchanged_outcome());
        };
        let before = self.snapshot();
        self.history.undo.push(HistoryEntry {
            value: self.value.clone(),
            selection: self.selection,
        });
        self.value = entry.value;
        self.selection = entry.selection;
        self.composition = None;
        self.composing = false;
        self.composition_base = None;
        Ok(self.commit(before, HistoryMode::None))
    }

    /// Replace the whole document, clear composition, and clear undo history.
    pub fn reset(&mut self, value: &str, selection: Selection) -> Result<EditOutcome, EditError> {
        if self.batch.depth > 0 {
            return Err(EditError::BatchActive);
        }
        let before = self.snapshot();
        self.history.undo.clear();
        self.history.redo.clear();
        self.composition_base = None;
        self.composition = None;
        self.composing = false;
        let value = sanitize(value, self.config.multiline);
        self.value = fit(&value, self.config.limit());
        self.selection = self.normalize_selection(selection);
        let mut outcome = self.commit(before, HistoryMode::None);
        if !outcome.changed {
            // An explicit reset invalidates IME snapshots even when the text
            // happens to match the current document.
            self.revision += 1;
            outcome.changed = true;
            outcome.revision = self.revision;
            outcome.snapshot = self.field_state();
        }
        Ok(outcome)
    }

    /// Apply a script-provided value. `epoch` is the revision the script
    /// observed. A stale epoch or an open composition rejects the write with
    /// an explicit conflict. `cancel` first cancels the composition, then
    /// applies the value.
    pub fn script_reset(
        &mut self,
        epoch: Revision,
        value: &str,
        selection: Selection,
        cancel: bool,
    ) -> Result<EditOutcome, EditError> {
        if self.closed {
            return Err(EditError::Closed);
        }
        if self.batch.depth > 0 {
            return Err(EditError::BatchActive);
        }
        if epoch != self.revision {
            return Err(EditError::StaleRevision {
                expected: epoch,
                actual: self.revision,
            });
        }
        if self.composing {
            if !cancel {
                return Err(EditError::CompositionConflict);
            }
            let _ = self.cancel_composition();
        }
        self.reset(value, selection)
    }

    fn unchanged_outcome(&self) -> EditOutcome {
        EditOutcome {
            changed: false,
            revision: self.revision,
            snapshot: self.field_state(),
            deltas: Vec::new(),
        }
    }

    fn commit(&mut self, before: Snapshot, history: HistoryMode) -> EditOutcome {
        let after = self.snapshot();
        let changed = before != after;
        let delta = if before.value != after.value {
            Some(diff(&before.value, &after.value))
        } else {
            None
        };
        if changed {
            if self.batch.depth > 0 {
                self.batch.changed = true;
                if let Some(delta) = &delta {
                    self.batch.deltas.push(delta.clone());
                }
            } else {
                self.revision += 1;
            }
        }
        if changed && history == HistoryMode::Push && self.batch.depth == 0 && !self.composing {
            self.push_undo(before);
        }
        let deltas = if self.batch.depth == 0 {
            delta.into_iter().collect()
        } else {
            Vec::new()
        };
        EditOutcome {
            changed,
            revision: self.revision,
            snapshot: self.field_state(),
            deltas,
        }
    }

    fn push_undo(&mut self, snapshot: Snapshot) {
        self.history.undo.push(HistoryEntry {
            value: snapshot.value,
            selection: snapshot.selection,
        });
        if self.history.undo.len() > HISTORY_LIMIT {
            self.history.undo.remove(0);
        }
        self.history.redo.clear();
    }

    /// Replace `start..end` with already-sanitized `text`, enforcing the
    /// length limit. Returns the inserted range after fitting.
    fn splice(&mut self, start: u32, end: u32, text: &str) -> (u32, u32) {
        let (start, end) = self.normalize_offsets(start, end);
        let start_byte = boundary_index(&self.value, start);
        let end_byte = boundary_index(&self.value, end);
        let outside = utf16_len(&self.value[..start_byte]) + utf16_len(&self.value[end_byte..]);
        let room = self.config.limit().saturating_sub(outside);
        let insert = fit(text, room);
        let mut next = String::with_capacity(self.value.len() + insert.len());
        next.push_str(&self.value[..start_byte]);
        next.push_str(&insert);
        next.push_str(&self.value[end_byte..]);
        let inserted_end = start + utf16_len(&insert);
        self.value = next;
        (start, inserted_end)
    }

    fn normalize_offsets(&self, start: u32, end: u32) -> (u32, u32) {
        let len = utf16_len(&self.value);
        let start = normalize_offset(&self.value, start.min(len));
        let end = normalize_offset(&self.value, end.min(len));
        if start <= end {
            (start, end)
        } else {
            (end, start)
        }
    }

    fn normalize_range(&self, range: DocRange) -> (u32, u32) {
        self.normalize_offsets(range.start.offset, range.end.offset)
    }

    fn normalize_selection(&self, selection: Selection) -> Selection {
        let len = utf16_len(&self.value);
        Selection::from_positions(
            DocPosition::with_affinity(
                normalize_offset(&self.value, selection.anchor.offset.min(len)),
                selection.anchor.affinity,
            ),
            DocPosition::with_affinity(
                normalize_offset(&self.value, selection.active.offset.min(len)),
                selection.active.affinity,
            ),
        )
    }
}

/// Script-driven value compatibility helper. Returns the transaction result
/// or an explicit conflict. Prefer [`EditSession::script_reset`].
pub fn display_text(value: &str, secure: bool) -> String {
    if !secure {
        return value.to_owned();
    }
    "•".repeat(utf16_len(value) as usize)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditEvent {
    pub handle: u32,
    pub handler: u32,
    pub session: SessionId,
    pub revision: u64,
    /// Ordered text replacements from the previous event value.
    pub deltas: Vec<EditDelta>,
    /// Final directional selection.
    pub selection: Selection,
    /// Final composition range, if any.
    pub composition: Option<(u32, u32)>,
    /// Whether a composition is open, including before it has a range.
    pub composing: bool,
    /// Final document value. Sent once per batch for controlled fields.
    pub value: String,
}

pub fn encode_edit_events(events: &[EditEvent]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"BSED");
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&(events.len() as u32).to_le_bytes());
    for event in events {
        out.extend_from_slice(&event.handler.to_le_bytes());
        out.extend_from_slice(&event.handle.to_le_bytes());
        out.extend_from_slice(&event.session.get().to_le_bytes());
        out.extend_from_slice(&event.revision.to_le_bytes());
        out.extend_from_slice(&event.selection.anchor.offset.to_le_bytes());
        out.extend_from_slice(&event.selection.active.offset.to_le_bytes());
        let (comp_start, comp_end) = event.composition.unwrap_or((u32::MAX, u32::MAX));
        out.extend_from_slice(&comp_start.to_le_bytes());
        out.extend_from_slice(&comp_end.to_le_bytes());
        let flags = u32::from(event.composing);
        out.extend_from_slice(&flags.to_le_bytes());
        out.extend_from_slice(&(event.deltas.len() as u32).to_le_bytes());
        for delta in &event.deltas {
            out.extend_from_slice(&delta.start.to_le_bytes());
            out.extend_from_slice(&delta.end.to_le_bytes());
            let bytes = delta.insert.as_bytes();
            out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
            out.extend_from_slice(bytes);
        }
        let bytes = event.value.as_bytes();
        out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        out.extend_from_slice(bytes);
    }
    out
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

fn normalize_offset(value: &str, units: u32) -> u32 {
    utf16_len(&value[..boundary_index(value, units)])
}

fn boundary_index(value: &str, units: u32) -> usize {
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

fn walk_scalars_back(value: &str, units: u32, count: u32) -> u32 {
    let mut position = units;
    for _ in 0..count {
        let previous = prev_scalar_boundary(value, position);
        if previous == position {
            break;
        }
        position = previous;
    }
    position
}

fn walk_scalars_forward(value: &str, units: u32, count: u32) -> u32 {
    let mut position = units;
    for _ in 0..count {
        let next = next_scalar_boundary(value, position);
        if next == position {
            break;
        }
        position = next;
    }
    position
}

fn cursor_from_position(
    insert_start: u32,
    insert_end: u32,
    new_cursor_position: i32,
    len: u32,
) -> u32 {
    let position = if new_cursor_position > 0 {
        insert_end.saturating_add((new_cursor_position - 1) as u32)
    } else {
        insert_start.saturating_sub(new_cursor_position.unsigned_abs())
    };
    position.min(len)
}

fn next_scalar_boundary(value: &str, units: u32) -> u32 {
    let mut seen = 0u32;
    for ch in value.chars() {
        let width = ch.len_utf16() as u32;
        if seen >= units {
            return seen;
        }
        if seen + width > units {
            return seen + width;
        }
        seen += width;
    }
    seen
}

fn prev_scalar_boundary(value: &str, units: u32) -> u32 {
    let mut prev = 0u32;
    for ch in value.chars() {
        let next = prev + ch.len_utf16() as u32;
        if next >= units {
            return prev;
        }
        prev = next;
    }
    prev
}

fn diff(old: &str, new: &str) -> EditDelta {
    let mut prefix_bytes = 0usize;
    let mut prefix_units = 0u32;
    for (a, b) in old.chars().zip(new.chars()) {
        if a != b {
            break;
        }
        prefix_bytes += a.len_utf8();
        prefix_units += a.len_utf16() as u32;
    }
    let old_rest = &old[prefix_bytes..];
    let new_rest = &new[prefix_bytes..];
    let mut suffix_bytes_new = 0usize;
    let mut suffix_units = 0u32;
    for (a, b) in old_rest.chars().rev().zip(new_rest.chars().rev()) {
        if a != b {
            break;
        }
        suffix_bytes_new += b.len_utf8();
        suffix_units += a.len_utf16() as u32;
    }
    let old_end = utf16_len(old) - suffix_units;
    let new_end = new.len() - suffix_bytes_new;
    EditDelta {
        start: prefix_units,
        end: old_end,
        insert: new[prefix_bytes..new_end].to_owned(),
    }
}


pub fn utf16_len(value: &str) -> u32 {
    value.chars().map(|ch| ch.len_utf16() as u32).sum()
}
