//! Retained tree. Taffy owns flex; this type owns scroll offsets, hit testing, and paint order.

mod field_gesture;
mod focus_edit;
mod hit_scroll;
mod paint;
mod semantics;
mod tree;
mod vars;

pub use paint::PaintCmd;

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};

use crate::editing::{EditError, EditEvent, EditSession, SessionId};
use crate::gesture::{GestureEvent, GestureKind, Recognizer};
use crate::ir::{Kind, NodeDesc};
use crate::ops::{decode_ops, Op, Value};
use crate::state_style::{StyleDecls, VarEvent, VarTable};
use crate::ui::{UiCall, UiProgram, UiValue};
use std::collections::VecDeque;
use taffy::prelude::*;
use taffy::style::Overflow;
use taffy::{NodeId, TaffyTree};

use self::tree::{style_for, Node};

#[derive(Clone, Debug)]
struct MeasureCtx {
    text: String,
    font_size: f32,
    line_height: f32,
    font_weight: u16,
}

/// Host text measurement: `(text, font size, line height, weight, max width)`
/// to a box size.
type Measure<'a> = &'a mut dyn FnMut(&str, f32, f32, u16, Option<f32>) -> (f32, f32);

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Len {
    Auto,
    Pt(f32),
    /// Fraction of the percentage base. `0.5` is 50%.
    Pct(f32),
}

fn zero_len() -> Len {
    Len::Pt(0.0)
}

fn zero_box() -> [Len; 4] {
    [zero_len(); 4]
}

fn auto_box() -> [Len; 4] {
    [Len::Auto; 4]
}

#[derive(Clone, Copy)]
struct NodeStyle {
    direction: u8,
    overflow_x: Overflow,
    overflow_y: Overflow,
    display: Display,
    flex_grow: f32,
    flex_shrink: f32,
    flex_basis: Len,
    width: Len,
    height: Len,
    min_width: Len,
    min_height: Len,
    max_width: Len,
    max_height: Len,
    padding: [Len; 4],
    margin: [Len; 4],
    gap_column: Len,
    gap_row: Len,
    align_items: u8,
    align_self: u8,
    justify_content: u8,
    align_content: u8,
    flex_wrap: u8,
    position: u8,
    inset: [Len; 4],
    aspect_ratio: Option<f32>,
}

impl NodeStyle {
    fn from_node(node: &Node) -> Self {
        Self {
            direction: node.direction,
            overflow_x: node.overflow_x,
            overflow_y: node.overflow_y,
            display: node.taffy_display(),
            flex_grow: node.flex_grow,
            flex_shrink: node.flex_shrink,
            flex_basis: node.flex_basis,
            width: node.width,
            height: node.height,
            min_width: node.min_width,
            min_height: node.min_height,
            max_width: node.max_width,
            max_height: node.max_height,
            padding: node.padding,
            margin: node.margin,
            gap_column: node.gap_column,
            gap_row: node.gap_row,
            align_items: node.align_items,
            align_self: node.align_self,
            justify_content: node.justify_content,
            align_content: node.align_content,
            flex_wrap: node.flex_wrap,
            position: node.position,
            inset: node.inset,
            aspect_ratio: node.aspect_ratio,
        }
    }
}

impl Default for NodeStyle {
    fn default() -> Self {
        Self::from_node(&Node::new_root(NodeId::new(0)))
    }
}

/// Pressed feedback for the current touch. The shell owns it because an edit
/// bypass reaches paint without a recognizer sequence.
#[derive(Default)]
struct VisualPress {
    /// Eligible pointer ids still down, first first.
    pointers: Vec<u32>,
    /// First eligible pointer of the current sequence; `0` when idle.
    owner: u32,
    /// Node drawing pressed feedback; `0` when idle or cancelled.
    target: u32,
    start_x: f32,
    start_y: f32,
    /// The sequence also runs through the recognizer, so recognizer
    /// cancellation applies to it.
    gesture: bool,
}

pub struct Shell {
    nodes: Vec<Option<Node>>,
    retired: HashSet<u32>,
    high_water: u32,
    taffy: TaffyTree<MeasureCtx>,
    viewport: Size<f32>,
    paint_cache: RefCell<Vec<PaintCmd>>,
    pub(crate) paint_dirty: Cell<bool>,
    pub(crate) recognizer: Recognizer,
    pub(crate) vars: VarTable,
    var_events: Vec<VarEvent>,
    pending_settle: std::collections::HashSet<u32>,
    pub(crate) now_ms: f64,
    state_styles: StyleDecls,
    visual: VisualPress,
    field_pointer: Option<field_gesture::FieldPointerState>,
    /// Pointer kind of the last press sample (0 touch, 1 mouse, 2 pen).
    /// Mouse presses never draw the selection handles.
    last_pointer_kind: u8,
    fields: HashMap<u32, EditSession>,
    focused: u32,
    selection_handles: Option<[(f32, f32, f32, f32); 2]>,
    edit_events: Vec<EditEvent>,
    edit_conflicts: Vec<EditConflict>,
    edit_batch: Option<EditBatch>,
    caret_handle: u32,
    caret_box: Option<(f32, f32, f32, f32)>,
    /// Viewport rect of the content-drag insertion caret.
    drop_caret_box: Option<(f32, f32, f32, f32)>,
    /// Field that owns `drop_caret_box`. `0` when there is no drop indicator.
    drop_caret_field: u32,
    /// Masked selection shown while dragging field content.
    drag_ghost_text: String,
    /// Viewport point the drag ghost is centered on.
    drag_ghost_pos: Option<(f32, f32)>,
    /// Measured ghost text size before padding. `(0, 0)` until measured.
    drag_ghost_size: (f32, f32),
    /// Last auto-revealed caret: handle, offset, content-space x/y. A fresh
    /// frame only scrolls to the caret again when this changes, so free
    /// scrolling is never snapped back.
    caret_reveal: Option<(u32, u32, f32, f32)>,
    selection_rects: Vec<(f32, f32, f32, f32)>,
    /// UI-thread programs ("worklets"), by id. Shared with the running
    /// frame so a call does not copy the program.
    pub(crate) ui_programs: Vec<Option<std::sync::Arc<UiProgram>>>,
    /// Variables written in the current UI batch, re-evaluated once at the end.
    pub(crate) ui_dirty: HashSet<u32>,
    /// JS→UI calls waiting for the next `drain_ui_work`.
    pub(crate) ui_calls: VecDeque<UiCall>,
    /// UI→JS `scheduleOnJS` notifications, drained by the host.
    pub(crate) js_calls: VecDeque<UiCall>,
    /// Runtime faults of the current UI batch.
    pub(crate) ui_faults: u32,
    pub(crate) ui_last_error: String,
    /// Shared value stack across one `drain_ui_work` batch.
    pub(crate) ui_stack: Vec<UiValue>,
    /// The gesture event the current UI call answers, if any.
    pub(crate) ui_event: Option<GestureEvent>,
}

/// A rejected script or IME write. Never contains user text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditConflict {
    pub handle: u32,
    pub session: SessionId,
    pub revision: u64,
    pub error: EditError,
}

/// UI-thread keyboard intent. Text-producing keys stay with the host/IME;
/// this surface covers focus traversal, activation, dismissal, and caret
/// movement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Tab,
    TabBack,
    Enter,
    Space,
    Escape,
    Left,
    Right,
    Home,
    End,
    ShiftLeft,
    ShiftRight,
    ShiftHome,
    ShiftEnd,
    SelectAll,
}

impl Key {
    pub fn parse(value: u32) -> Option<Self> {
        Some(match value {
            0 => Self::Tab,
            1 => Self::TabBack,
            2 => Self::Enter,
            3 => Self::Space,
            4 => Self::Escape,
            5 => Self::Left,
            6 => Self::Right,
            7 => Self::Home,
            8 => Self::End,
            9 => Self::ShiftLeft,
            10 => Self::ShiftRight,
            11 => Self::ShiftHome,
            12 => Self::ShiftEnd,
            13 => Self::SelectAll,
            _ => return None,
        })
    }
}

/// Screen-reader action routed to one semantic node. Scroll actions resolve
/// the nearest scrollable ancestor that accepts the requested axis.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum A11yAction {
    Focus,
    Activate,
    SetValue(String),
    ScrollForward,
    ScrollBackward,
    ScrollLeft,
    ScrollRight,
    ScrollUp,
    ScrollDown,
}

impl A11yAction {
    pub fn parse(value: u32) -> Option<Self> {
        Some(match value {
            0 => Self::Focus,
            1 => Self::Activate,
            2 => Self::SetValue(String::new()),
            3 => Self::ScrollForward,
            4 => Self::ScrollBackward,
            5 => Self::ScrollLeft,
            6 => Self::ScrollRight,
            7 => Self::ScrollUp,
            8 => Self::ScrollDown,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScrollAxis {
    X,
    Y,
    Primary,
}

/// Screen-reader scroll distance: most of the visible page, but never zero.
fn page_step(extent: f32) -> f32 {
    (extent * 0.8).max(1.0)
}

/// Native or custom edit menu command. The host owns the clipboard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditMenuAction {
    Cut,
    Copy,
    Paste,
    SelectAll,
}

impl EditMenuAction {
    pub fn parse(value: u32) -> Option<Self> {
        Some(match value {
            0 => Self::Cut,
            1 => Self::Copy,
            2 => Self::Paste,
            3 => Self::SelectAll,
            _ => return None,
        })
    }
}

/// Result of an edit menu command. `clipboard` is `Some` when the caller must
/// write the returned text to the host clipboard.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MenuResult {
    pub clipboard: Option<String>,
    pub changed: bool,
}

struct EditBatch {
    handle: u32,
    handler: u32,
    depth: u32,
}

/// Per-axis result of routing a wheel or touch delta through the hit path.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ScrollOutcome {
    pub consumed_x: f32,
    pub consumed_y: f32,
    pub remaining_x: f32,
    pub remaining_y: f32,
    /// A `contain` boundary stopped the axis.
    pub truncated_x: bool,
    pub truncated_y: bool,
}

impl ScrollOutcome {
    pub fn changed(&self) -> bool {
        self.consumed_x != 0.0 || self.consumed_y != 0.0
    }
}

impl Shell {
    /// Empty viewport root. Function components append children with `spawn`.
    pub fn blank() -> Result<Self, String> {
        let mut taffy: TaffyTree<MeasureCtx> = TaffyTree::new();
        // Text measure and paint wrap at the node box width. Taffy's pixel-grid
        // rounding can shave a box below its intrinsic text width (e.g. a
        // 79.74 px label rounds to 79), which then re-wraps painted text the
        // measure pass had already sized for one line. Boxes stay fractional so
        // measure, layout, and paint agree.
        taffy.disable_rounding();
        let style = style_for(NodeStyle::default());
        let taffy_id = taffy.new_leaf(style).map_err(|err| err.to_string())?;
        Ok(Self {
            nodes: vec![None, Some(Node::new_root(taffy_id))],
            retired: HashSet::new(),
            high_water: 1,
            taffy,
            viewport: Size::ZERO,
            paint_cache: RefCell::new(Vec::new()),
            paint_dirty: Cell::new(true),
            recognizer: Recognizer::new(),
            vars: VarTable::new(),
            var_events: Vec::new(),
            pending_settle: std::collections::HashSet::new(),
            now_ms: 0.0,
            state_styles: StyleDecls::new(),
            visual: VisualPress::default(),
            field_pointer: None,
            last_pointer_kind: 0,
            fields: HashMap::new(),
            focused: 0,
            selection_handles: None,
            edit_events: Vec::new(),
            edit_conflicts: Vec::new(),
            edit_batch: None,
            caret_handle: 0,
            caret_box: None,
            drop_caret_box: None,
            drop_caret_field: 0,
            drag_ghost_text: String::new(),
            drag_ghost_pos: None,
            drag_ghost_size: (0.0, 0.0),
            caret_reveal: None,
            selection_rects: Vec::new(),
            ui_programs: Vec::new(),
            ui_dirty: HashSet::new(),
            ui_calls: VecDeque::new(),
            js_calls: VecDeque::new(),
            ui_faults: 0,
            ui_last_error: String::new(),
            ui_stack: Vec::new(),
            ui_event: None,
        })
    }

    pub fn set_viewport(&mut self, width: f32, height: f32) {
        if self.viewport.width != width || self.viewport.height != height {
            self.viewport = Size { width, height };
            self.paint_dirty.set(true);
        }
    }

    pub fn apply_ops(&mut self, bytes: &[u8]) -> Result<u32, String> {
        let (seq, ops) = decode_ops(bytes)?;
        for op in ops {
            match op {
                Op::SetText { handle, text } => {
                    let node = self.node_mut(handle)?;
                    if node.kind != Kind::Text {
                        return Err(format!("handle {handle} is not text"));
                    }
                    node.text = text.clone();
                    let font_size = node.font_size;
                    let line_height = node.line_height;
                    let font_weight = node.font_weight;
                    let taffy_id = node.taffy;
                    self.taffy
                        .set_node_context(
                            taffy_id,
                            Some(MeasureCtx {
                                text,
                                font_size,
                                line_height,
                                font_weight,
                            }),
                        )
                        .map_err(|err| err.to_string())?;
                }
                Op::SetPropColor { handle, key, rgba } => {
                    self.set_prop(handle, &key, &Value::Int(rgba as i64))?
                }
                Op::SetProp { handle, key, value } => self.set_prop(handle, &key, &value)?,
                Op::DisposeNode { handle } => self.dispose_node(handle)?,
                Op::MoveNode {
                    handle,
                    parent,
                    index,
                } => self.move_node(handle, parent, index)?,
                Op::BindEvent { handle, handler } => self.node_mut(handle)?.handler = handler,
                Op::ScrollTo { handle, x, y } => self.scroll_to(handle, x, y)?,
                Op::Spawn {
                    handle,
                    parent,
                    kind,
                    text,
                } => {
                    let mut desc = NodeDesc::view(handle, parent);
                    desc.kind = kind;
                    desc.text = text;
                    self.spawn_desc(desc)?;
                }
                Op::SpawnAt {
                    handle,
                    parent,
                    kind,
                    text,
                    index,
                } => {
                    let mut desc = NodeDesc::view(handle, parent);
                    desc.kind = kind;
                    desc.text = text;
                    self.insert_node(desc, parent, index)?
                }
                Op::SpawnGesture {
                    gesture,
                    target,
                    kind,
                    index,
                } => {
                    self.node(target)?;
                    let kind = GestureKind::parse(kind)
                        .ok_or_else(|| format!("bad gesture kind {kind}"))?;
                    self.recognizer.spawn(gesture, target, kind, index)?;
                }
                Op::SetGestureProp {
                    gesture,
                    key,
                    value,
                } => self.recognizer.set_prop(gesture, &key, &value)?,
                Op::MoveGesture {
                    gesture,
                    target,
                    index,
                } => {
                    self.node(target)?;
                    self.recognizer.move_decl(gesture, target, index)?;
                }
                Op::DisposeGesture { gesture } => self.recognizer.dispose(gesture)?,
                Op::BindGestureEvent {
                    gesture,
                    slot,
                    handler,
                } => self.recognizer.bind(gesture, slot, handler)?,
                Op::Focus { handle } => self.focus(handle)?,
                Op::SetEditingValue {
                    handle,
                    epoch,
                    value,
                    sel_start,
                    sel_end,
                    cancel,
                } => self.set_editing_value(handle, epoch, &value, sel_start, sel_end, cancel)?,
                Op::CreateVar {
                    var,
                    source,
                    initial,
                } => self.create_var(var, source, &initial)?,
                Op::BindVar { var, target } => self.bind_var(var, target)?,
                Op::DisposeVar { var } => self.dispose_var(var)?,
                Op::SetVar { var, value, anim } => self.set_var(var, &value, anim)?,
                Op::SettleVar { var } => self.settle_var(var)?,
                Op::DefineUiFunction {
                    id,
                    params,
                    captures,
                    consts,
                    code,
                } => self.define_ui_function(id, params, captures, consts, code)?,
                Op::BindGestureProgram {
                    gesture,
                    slot,
                    function,
                } => self.recognizer.bind_program(gesture, slot, function)?,
                Op::RunUiFunction { function, args } => {
                    let args = args
                        .iter()
                        .map(UiValue::try_from_value)
                        .collect::<Result<Vec<_>, _>>()?;
                    self.queue_ui_call(UiCall { function, args });
                }
                Op::CancelVarAnim { var } => self.cancel_var_anim(var)?,
                Op::SpawnStyleDecl {
                    decl,
                    target,
                    component,
                    index,
                } => {
                    self.node(target)?;
                    self.state_styles.spawn(decl, target, component, index)?;
                }
                Op::SetStyleDeclProp { decl, key, value } => {
                    self.set_style_decl_prop(decl, &key, &value)?
                }
                Op::MoveStyleDecl {
                    decl,
                    target,
                    index,
                } => {
                    self.node(target)?;
                    self.state_styles.move_decl(decl, target, index)?;
                }
                Op::DisposeStyleDecl { decl } => self.state_styles.dispose(decl)?,
            }
        }
        if self.focused != 0
            && (!self.is_focusable(self.focused)
                || !self.interaction_available(self.focused, self.active_modal()))
        {
            self.focus(0)?;
        }
        self.sync_press_target();
        self.reval_dirty();
        self.paint_dirty.set(true);
        Ok(seq)
    }
}
