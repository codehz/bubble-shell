//! Retained tree. Taffy owns flex; this type owns scroll offsets, hit testing, and paint order.

use std::cell::{Cell, Ref, RefCell};
use std::collections::{HashMap, HashSet};

use crate::color::Color;
use crate::editing::{
    display_text, utf16_len, DocRange, EditCommand, EditConfig, EditError, EditEvent, EditOp,
    EditOutcome, EditSession, FieldState, Selection, SessionId,
};
use crate::gesture::{GestureEvent, GestureKind, PointerAction, PointerInput, Recognizer};
use crate::ir::{Kind, NodeDesc};
use crate::layout::{snap_unit, wrapping_layout, LayoutKey, LayoutRequest, LayoutSnapshot};
use crate::ops::{decode_ops, Op, Value};
use crate::props::Prop;
use crate::semantics::{
    Role, SemanticsNode, SemanticsSnapshot, ACTION_ACTIVATE, ACTION_FOCUS, ACTION_SCROLL_BACKWARD,
    ACTION_SCROLL_DOWN, ACTION_SCROLL_FORWARD, ACTION_SCROLL_LEFT, ACTION_SCROLL_RIGHT,
    ACTION_SCROLL_UP, ACTION_SET_VALUE,
};
use taffy::prelude::*;
use taffy::style::Overflow;
use taffy::{compute_leaf_layout, NodeId, TaffyTree};

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

#[derive(Clone, Copy)]
struct NodeStyle {
    row: bool,
    overflow_x: Overflow,
    overflow_y: Overflow,
    display: Display,
    flex_grow: f32,
    flex_shrink: f32,
    width: Option<f32>,
    height: Option<f32>,
    padding: [f32; 4],
    margin: [f32; 4],
    gap: f32,
    align_items: u8,
    justify_content: u8,
    position: u8,
    inset: [f32; 4],
}

impl NodeStyle {
    fn from_node(node: &Node) -> Self {
        Self {
            row: node.row,
            overflow_x: node.overflow_x,
            overflow_y: node.overflow_y,
            display: node.taffy_display(),
            flex_grow: node.flex_grow,
            flex_shrink: node.flex_shrink,
            width: node.width,
            height: node.height,
            padding: node.padding,
            margin: node.margin,
            gap: node.gap,
            align_items: node.align_items,
            justify_content: node.justify_content,
            position: node.position,
            inset: node.inset,
        }
    }
}

impl Default for NodeStyle {
    fn default() -> Self {
        Self {
            row: false,
            overflow_x: Overflow::Visible,
            overflow_y: Overflow::Visible,
            display: Display::Flex,
            flex_grow: 0.0,
            flex_shrink: 1.0,
            width: None,
            height: None,
            padding: [0.0; 4],
            margin: [0.0; 4],
            gap: 0.0,
            align_items: 0,
            justify_content: 0,
            position: 0,
            inset: [f32::NAN; 4],
        }
    }
}

#[allow(dead_code)]
struct Node {
    parent: u32,
    children: Vec<u32>,
    kind: Kind,
    overflow_x: Overflow,
    overflow_y: Overflow,
    background: Color,
    color: Color,
    font_size: f32,
    radius: f32,
    opacity: f32,
    handler: u32,
    width: Option<f32>,
    height: Option<f32>,
    flex_grow: f32,
    flex_shrink: f32,
    row: bool,
    padding: [f32; 4],
    margin: [f32; 4],
    gap: f32,
    align_items: u8,
    justify_content: u8,
    position: u8,
    inset: [f32; 4],
    line_height: f32,
    text_align: u8,
    font_weight: u16,
    src: String,
    fit: u8,
    text: String,
    taffy: NodeId,
    scroll_left: f32,
    scroll_top: f32,
    chain_x: bool,
    chain_y: bool,
    multiline: bool,
    secure: bool,
    max_length: u32,
    keyboard: String,
    autofill: String,
    role: Option<Role>,
    label: String,
    disabled: bool,
    a11y_hidden: bool,
    /// `display: none`. The subtree is out of layout, paint, hit, focus, and semantics.
    display_none: bool,
    /// `visibility: hidden`. Layout stays; the subtree cannot paint, hit, or take actions.
    visibility_hidden: bool,
    focusable: bool,
    modal: bool,
    checked: Option<bool>,
    selected: bool,
    expanded: Option<bool>,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}
impl Node {
    fn scrolls_x(&self) -> bool {
        self.overflow_x == Overflow::Scroll
    }

    fn scrolls_y(&self) -> bool {
        self.overflow_y == Overflow::Scroll
    }

    fn taffy_display(&self) -> Display {
        if self.display_none {
            Display::None
        } else {
            Display::Flex
        }
    }

    /// Painted and hit-tested only when both layout and visibility include it.
    fn painted(&self) -> bool {
        !self.display_none && !self.visibility_hidden
    }
}

pub struct Shell {
    nodes: Vec<Option<Node>>,
    retired: HashSet<u32>,
    high_water: u32,
    taffy: TaffyTree<MeasureCtx>,
    viewport: Size<f32>,
    paint_cache: RefCell<Vec<PaintCmd>>,
    paint_dirty: Cell<bool>,
    recognizer: Recognizer,
    fields: HashMap<u32, EditSession>,
    focused: u32,
    selection_handles: Option<[(f32, f32, f32, f32); 2]>,
    edit_events: Vec<EditEvent>,
    edit_conflicts: Vec<EditConflict>,
    edit_batch: Option<EditBatch>,
    caret_handle: u32,
    caret_box: Option<(f32, f32, f32, f32)>,
    selection_rects: Vec<(f32, f32, f32, f32)>,
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

#[derive(Clone, Debug, PartialEq)]
pub enum PaintCmd {
    Fill {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        radius: f32,
        color: Color,
    },
    Text {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        font_size: f32,
        color: Color,
        line_height: f32,
        weight: u16,
        align: u8,
        text: String,
    },
    Image {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        radius: f32,
        fit: u8,
        src: String,
    },
    PushClip {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    },
    PopClip,
    PushOpacity {
        opacity: f32,
    },
    PopOpacity,
}

struct FieldGeom {
    text: String,
    origin_x: f32,
    origin_y: f32,
    key: LayoutKey,
}

impl Shell {
    /// Empty viewport root. Function components append children with `spawn`.
    pub fn blank() -> Result<Self, String> {
        let mut taffy: TaffyTree<MeasureCtx> = TaffyTree::new();
        let style = style_for(NodeStyle::default());
        let taffy_id = taffy.new_leaf(style).map_err(|err| err.to_string())?;
        Ok(Self {
            nodes: vec![
                None,
                Some(Node {
                    parent: 0,
                    children: Vec::new(),
                    kind: Kind::View,
                    overflow_x: Overflow::Visible,
                    overflow_y: Overflow::Visible,
                    background: Color::TRANSPARENT,
                    color: Color::WHITE,
                    font_size: 16.0,
                    radius: 0.0,
                    opacity: 1.0,
                    handler: 0,
                    width: None,
                    height: None,
                    flex_grow: 0.0,
                    flex_shrink: 1.0,
                    row: false,
                    padding: [0.0; 4],
                    margin: [0.0; 4],
                    gap: 0.0,
                    align_items: 0,
                    justify_content: 0,
                    position: 0,
                    inset: [f32::NAN; 4],
                    line_height: 0.0,
                    text_align: 0,
                    font_weight: 400,
                    src: String::new(),
                    fit: 0,
                    text: String::new(),
                    taffy: taffy_id,
                    scroll_left: 0.0,
                    scroll_top: 0.0,
                    chain_x: false,
                    chain_y: false,
                    multiline: false,
                    secure: false,
                    max_length: 0,
                    keyboard: String::new(),
                    autofill: String::new(),
                    role: None,
                    label: String::new(),
                    disabled: false,
                    a11y_hidden: false,
                    display_none: false,
                    visibility_hidden: false,
                    focusable: false,
                    modal: false,
                    checked: None,
                    selected: false,
                    expanded: None,
                    x: 0.0,
                    y: 0.0,
                    w: 0.0,
                    h: 0.0,
                }),
            ],
            retired: HashSet::new(),
            high_water: 1,
            taffy,
            viewport: Size::ZERO,
            paint_cache: RefCell::new(Vec::new()),
            paint_dirty: Cell::new(true),
            recognizer: Recognizer::new(),
            fields: HashMap::new(),
            focused: 0,
            selection_handles: None,
            edit_events: Vec::new(),
            edit_conflicts: Vec::new(),
            edit_batch: None,
            caret_handle: 0,
            caret_box: None,
            selection_rects: Vec::new(),
        })
    }

    /// Append `desc` under `desc.parent`. `desc.id` is the caller-allocated handle.
    pub fn spawn(&mut self, desc: NodeDesc) -> Result<(), String> {
        self.spawn_desc(desc)
    }

    /// Append a dynamic node from a `Spawn` op.
    pub fn spawn_desc(&mut self, desc: NodeDesc) -> Result<(), String> {
        let parent = desc.parent;
        if parent == 0 {
            return Err("spawn requires a parent".into());
        }
        let index = self.node(parent)?.children.len() as u32;
        self.insert_node(desc, parent, index)
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
                Op::Spawn { desc } => self.spawn_desc(desc)?,
                Op::SpawnAt { desc, index } => {
                    let parent = desc.parent;
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
            }
        }
        if self.focused != 0
            && (!self.is_focusable(self.focused)
                || !self.interaction_available(self.focused, self.active_modal()))
        {
            self.focus(0)?;
        }
        self.paint_dirty.set(true);
        Ok(seq)
    }

    pub fn layout(&mut self, measure: Measure<'_>) -> Result<(), String> {
        self.compute_layout(measure)?;
        self.refresh_caret(&mut |req| {
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

    /// Lay out, then take editing geometry from `host`. The host snapshot must
    /// match the field key; this is the path platform editors use.
    pub fn layout_with(
        &mut self,
        measure: Measure<'_>,
        host: &mut dyn FnMut(&LayoutRequest<'_>) -> Result<LayoutSnapshot, String>,
    ) -> Result<(), String> {
        self.compute_layout(measure)?;
        self.refresh_caret(host)
    }

    pub fn compute_layout(&mut self, measure: Measure<'_>) -> Result<(), String> {
        if self.viewport.width <= 0.0 || self.viewport.height <= 0.0 {
            return Err("viewport is empty".into());
        }
        let root = self.node(1)?.taffy;
        let root_style = {
            let root_node = self.node(1)?;
            let mut style = NodeStyle::from_node(root_node);
            style.width = Some(self.viewport.width);
            style.height = Some(self.viewport.height);
            style_for(style)
        };
        self.taffy
            .set_style(root, root_style)
            .map_err(|err| err.to_string())?;
        self.taffy
            .compute_layout_with_measure(
                root,
                Size {
                    width: AvailableSpace::Definite(self.viewport.width),
                    height: AvailableSpace::Definite(self.viewport.height),
                },
                |inputs, _node, ctx, style| {
                    let measured = ctx.map(|ctx| {
                        (
                            ctx.text.clone(),
                            ctx.font_size,
                            ctx.line_height,
                            ctx.font_weight,
                        )
                    });
                    compute_leaf_layout(
                        inputs,
                        style,
                        |_, _| 0.0,
                        |known, available| {
                            let Some((text, font_size, line_height, font_weight)) =
                                measured.as_ref()
                            else {
                                return Size::ZERO;
                            };
                            if let (Some(width), Some(height)) = (known.width, known.height) {
                                return Size { width, height };
                            }
                            let max_width = match available.width {
                                AvailableSpace::Definite(width) => Some(width),
                                _ => known.width,
                            };
                            let (width, height) =
                                measure(text, *font_size, *line_height, *font_weight, max_width);
                            Size {
                                width: known.width.unwrap_or(width),
                                height: known.height.unwrap_or(height),
                            }
                        },
                    )
                },
            )
            .map_err(|err| err.to_string())?;
        self.place(1, 0.0, 0.0)?;
        let limits: Vec<(usize, f32, f32)> = self
            .nodes
            .iter()
            .enumerate()
            .filter_map(|(index, node)| {
                let node = node.as_ref()?;
                Some((
                    index,
                    if node.scrolls_x() {
                        self.max_scroll_x(node)
                    } else {
                        0.0
                    },
                    if node.scrolls_y() {
                        self.max_scroll_y(node)
                    } else {
                        0.0
                    },
                ))
            })
            .collect();
        for (index, limit_x, limit_y) in limits {
            if let Some(node) = &mut self.nodes[index] {
                node.scroll_left = if node.scrolls_x() {
                    node.scroll_left.clamp(0.0, limit_x)
                } else {
                    0.0
                };
                node.scroll_top = if node.scrolls_y() {
                    node.scroll_top.clamp(0.0, limit_y)
                } else {
                    0.0
                };
            }
        }
        self.paint_dirty.set(true);
        Ok(())
    }

    pub fn paint(&self) -> Ref<'_, Vec<PaintCmd>> {
        if self.paint_dirty.get() {
            let mut out = Vec::new();
            if self.viewport.width > 0.0 {
                self.paint_node(1, &mut out);
            }
            *self.paint_cache.borrow_mut() = out;
            self.paint_dirty.set(false);
        }
        self.paint_cache.borrow()
    }

    /// Handler on the hit node, or the nearest ancestor that has one.
    /// A node without a handler still occludes nodes behind it.
    pub fn tap(&self, x: f32, y: f32) -> Option<u32> {
        let mut path = Vec::new();
        if !self.hit_path(1, x, y, &mut path) {
            return None;
        }
        path.iter().find_map(|id| {
            let handler = self.node(*id).ok()?.handler;
            if handler == 0 {
                None
            } else {
                Some(handler)
            }
        })
    }

    /// Feed one normalized pointer sample. `Down` fixes the real hit path;
    /// move/up/cancel reuse it. Scroll deltas produced by the recognizer are
    /// applied to the frozen chain immediately, before the host reads paint.
    pub fn pointer(&mut self, mut input: PointerInput) {
        if input.action == PointerAction::Down {
            input.path.clear();
            self.hit_path(1, input.x, input.y, &mut input.path);
        }
        self.recognizer.pointer(input);
        self.flush_gesture_scroll();
    }

    /// UI-thread timer tick for long press and the double-tap window.
    pub fn advance_gestures(&mut self, now_ms: f64) {
        self.recognizer.advance(now_ms);
        self.flush_gesture_scroll();
    }

    /// Drain recognition events in order. The host dispatches them to the
    /// script thread; nothing here calls JS.
    /// Host pause/focus loss: drop touches, timers, reserves, and ownership.
    pub fn cancel_gestures(&mut self, reason: u8) -> Vec<GestureEvent> {
        self.recognizer.cancel(reason);
        self.flush_gesture_scroll();
        self.take_gesture_events()
    }

    pub fn set_gesture_trace(&mut self, enabled: bool) {
        self.recognizer.set_trace(enabled);
    }

    pub fn take_gesture_trace(&mut self) -> Vec<String> {
        self.recognizer.take_trace()
    }

    /// Whether timers still need `advance_gestures` calls.
    pub fn gestures_pending(&self) -> bool {
        self.recognizer.pending()
    }

    pub fn take_gesture_events(&mut self) -> Vec<GestureEvent> {
        self.recognizer.drain_events()
    }

    /// Focus a semantic node. `0` blurs. Text fields and nodes marked
    /// `focusable` (or carrying an interactive role) accept focus.
    pub fn focus(&mut self, handle: u32) -> Result<(), String> {
        if handle != 0 && !self.interaction_available(handle, self.active_modal()) {
            return Err(format!("handle {handle} is unavailable"));
        }
        if handle == 0 {
            self.abort_edit_batch();
            self.focused = 0;
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

    fn interaction_available(&self, handle: u32, modal: u32) -> bool {
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

    /// Semantic snapshot in viewport coordinates. Nodes are in pre-order;
    /// `parent` points at the nearest semantic ancestor.
    pub fn semantics(&self) -> SemanticsSnapshot {
        let modal = self.active_modal();
        let mut nodes = Vec::new();
        self.collect_semantics(1, 0, 0, false, modal, &mut nodes);
        SemanticsSnapshot {
            version: 1,
            focused: self.focused,
            modal,
            nodes,
        }
    }

    pub fn semantics_json(&self) -> String {
        self.semantics().to_json()
    }

    fn collect_semantics(
        &self,
        id: u32,
        parent: u32,
        depth: u32,
        hidden: bool,
        modal: u32,
        out: &mut Vec<SemanticsNode>,
    ) {
        let Ok(node) = self.node(id) else {
            return;
        };
        if node.display_none {
            return;
        }
        let outside = modal != 0 && !self.contains(modal, id);
        let inherited_hidden = hidden || node.a11y_hidden || node.visibility_hidden;
        let hidden = inherited_hidden || outside;
        // A modal must not remain under an aria-hidden/background ancestor.
        let (parent, depth) = if id == modal { (0, 0) } else { (parent, depth) };
        let role = self.node_role(node);
        let semantic = node.kind != Kind::View
            || node.role.is_some()
            || !node.label.is_empty()
            || node.scrolls_x()
            || node.scrolls_y()
            || self.is_focusable(id)
            || node.modal
            || node.disabled;
        let mut next_parent = parent;
        let mut next_depth = depth;
        if semantic {
            let mut actions = 0;
            if self.is_focusable(id) {
                actions |= ACTION_FOCUS;
            }
            if self.recognizer.has_tap(id) {
                actions |= ACTION_ACTIVATE;
            }
            if node.kind == Kind::Field {
                actions |= ACTION_SET_VALUE;
            }
            if node.scrolls_x() {
                actions |= ACTION_SCROLL_FORWARD
                    | ACTION_SCROLL_BACKWARD
                    | ACTION_SCROLL_LEFT
                    | ACTION_SCROLL_RIGHT;
            }
            if node.scrolls_y() {
                actions |= ACTION_SCROLL_FORWARD
                    | ACTION_SCROLL_BACKWARD
                    | ACTION_SCROLL_UP
                    | ACTION_SCROLL_DOWN;
            }
            if hidden || node.disabled {
                actions = 0;
            }
            let editable = node.kind == Kind::Field;
            let secure = editable && node.secure;
            let (label, value) = match node.kind {
                Kind::Text => (
                    if node.label.is_empty() {
                        node.text.clone()
                    } else {
                        node.label.clone()
                    },
                    String::new(),
                ),
                Kind::Field => (
                    node.label.clone(),
                    if secure {
                        String::new()
                    } else {
                        node.text.clone()
                    },
                ),
                _ => (node.label.clone(), String::new()),
            };
            out.push(SemanticsNode {
                handle: id,
                parent,
                depth,
                role,
                label,
                value,
                bounds: [node.x, node.y, node.w, node.h],
                actions,
                focusable: !hidden && !node.disabled && self.is_focusable(id),
                focused: id == self.focused,
                disabled: node.disabled,
                hidden,
                checked: node.checked,
                selected: node.selected,
                expanded: node.expanded,
                secure,
                editable,
                scrollable: node.scrolls_x() || node.scrolls_y(),
            });
            next_parent = id;
            next_depth = depth + 1;
        }
        let inherited = inherited_hidden;
        let children = node.children.clone();
        for child in children {
            self.collect_semantics(child, next_parent, next_depth, inherited, modal, out);
        }
    }

    fn node_role(&self, node: &Node) -> Role {
        node.role.unwrap_or(match node.kind {
            Kind::Text => Role::Text,
            Kind::Field => Role::Textbox,
            Kind::Image => Role::Image,
            Kind::View => Role::Generic,
        })
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
            Key::Left => self.move_caret(false, false),
            Key::Right => self.move_caret(true, false),
            Key::Home => self.move_caret(false, true),
            Key::End => self.move_caret(true, true),
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

    fn move_caret(&mut self, forward: bool, line: bool) -> bool {
        let handle = self.focused;
        if handle == 0 || !self.node(handle).is_ok_and(|node| node.kind == Kind::Field) {
            return false;
        }
        let Ok((value, state)) = self.field(handle) else {
            return false;
        };
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

    /// Route a screen-reader action to a semantic node.
    pub fn a11y_action(&mut self, handle: u32, action: A11yAction) -> Result<(), String> {
        self.node(handle)?;
        if !self.interaction_available(handle, self.active_modal()) {
            return Err(format!("handle {handle} is unavailable"));
        }
        match action {
            A11yAction::Focus => self.focus(handle),
            A11yAction::Activate => {
                if self.node(handle)?.kind == Kind::Field {
                    self.focus(handle)
                } else {
                    let (cx, cy) = {
                        let node = self.node(handle)?;
                        (node.x + node.w / 2.0, node.y + node.h / 2.0)
                    };
                    self.recognizer.activate(handle, cx, cy, 0.0);
                    Ok(())
                }
            }
            A11yAction::SetValue(text) => self.set_field_text(handle, &text),
            action @ (A11yAction::ScrollForward
            | A11yAction::ScrollBackward
            | A11yAction::ScrollLeft
            | A11yAction::ScrollRight
            | A11yAction::ScrollUp
            | A11yAction::ScrollDown) => self.a11y_scroll(handle, action),
        }
    }

    /// Scroll the nearest ancestor that accepts the requested axis. The step
    /// is a page of the visible extent, so platform bridges never invent their
    /// own pixel deltas.
    fn a11y_scroll(&mut self, handle: u32, action: A11yAction) -> Result<(), String> {
        let (target, axis) = match action {
            A11yAction::ScrollForward | A11yAction::ScrollBackward => {
                let target = self.scroll_target(handle, ScrollAxis::Primary)?;
                let node = self.node(target)?;
                (
                    target,
                    if node.scrolls_y() {
                        ScrollAxis::Y
                    } else {
                        ScrollAxis::X
                    },
                )
            }
            A11yAction::ScrollLeft | A11yAction::ScrollRight => {
                (self.scroll_target(handle, ScrollAxis::X)?, ScrollAxis::X)
            }
            A11yAction::ScrollUp | A11yAction::ScrollDown => {
                (self.scroll_target(handle, ScrollAxis::Y)?, ScrollAxis::Y)
            }
            _ => unreachable!("a11y_scroll only handles scroll actions"),
        };
        let (w, h) = {
            let node = self.node(target)?;
            (node.w, node.h)
        };
        let step_x = page_step(w);
        let step_y = page_step(h);
        let (dx, dy) = match action {
            A11yAction::ScrollForward => {
                if axis == ScrollAxis::Y {
                    (0.0, step_y)
                } else {
                    (step_x, 0.0)
                }
            }
            A11yAction::ScrollBackward => {
                if axis == ScrollAxis::Y {
                    (0.0, -step_y)
                } else {
                    (-step_x, 0.0)
                }
            }
            A11yAction::ScrollLeft => (-step_x, 0.0),
            A11yAction::ScrollRight => (step_x, 0.0),
            A11yAction::ScrollUp => (0.0, -step_y),
            A11yAction::ScrollDown => (0.0, step_y),
            _ => unreachable!("a11y_scroll only handles scroll actions"),
        };
        self.scroll_path(&[target], dx, dy);
        Ok(())
    }

    fn scroll_target(&self, handle: u32, axis: ScrollAxis) -> Result<u32, String> {
        let mut id = handle;
        let modal = self.active_modal();
        while id != 0 {
            if !self.interaction_available(id, modal) {
                break;
            }
            let node = self.node(id)?;
            let accepts = match axis {
                ScrollAxis::X => node.scrolls_x(),
                ScrollAxis::Y => node.scrolls_y(),
                ScrollAxis::Primary => node.scrolls_x() || node.scrolls_y(),
            };
            if accepts {
                return Ok(id);
            }
            id = node.parent;
        }
        Err(format!("handle {handle} has no scrollable ancestor"))
    }

    fn set_field_text(&mut self, handle: u32, text: &str) -> Result<(), String> {
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

    fn set_editing_value(
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

    fn flush_gesture_scroll(&mut self) {
        let requests = self.recognizer.drain_scroll();
        for request in requests {
            self.scroll_path(&request.path, request.dx, request.dy);
        }
    }

    /// Nodes under the point from deepest to root. Hidden and `display: none`
    /// subtrees miss. A clipped axis stops at the padding box (the outer box);
    /// a visible axis stays open inside the viewport and ancestor clips.
    fn hit_path(&self, id: u32, x: f32, y: f32, out: &mut Vec<u32>) -> bool {
        self.hit_clipped(id, x, y, self.viewport_bounds(), out)
    }

    fn viewport_bounds(&self) -> Bounds {
        Bounds {
            x0: 0.0,
            y0: 0.0,
            x1: self.viewport.width,
            y1: self.viewport.height,
        }
    }

    fn hit_clipped(&self, id: u32, x: f32, y: f32, clip: Bounds, out: &mut Vec<u32>) -> bool {
        let Some(node) = self.nodes.get(id as usize).and_then(Option::as_ref) else {
            return false;
        };
        if !node.painted() || !clip.contains(x, y) {
            return false;
        }
        let next = clip.intersect(axis_bounds(
            node.overflow_x,
            node.overflow_y,
            node.x,
            node.y,
            node.w,
            node.h,
            self.viewport.width,
            self.viewport.height,
        ));
        let (nx, ny, nw, nh) = (node.x, node.y, node.w, node.h);
        let count = node.children.len();
        if next.contains(x, y) {
            for index in (0..count).rev() {
                let child = self.nodes[id as usize]
                    .as_ref()
                    .expect("hit child")
                    .children[index];
                if self.hit_clipped(child, x, y, next, out) {
                    out.push(id);
                    return true;
                }
            }
        }
        if x >= nx && y >= ny && x < nx + nw && y < ny + nh {
            out.push(id);
            true
        } else {
            false
        }
    }

    /// Wheel/trackpad input: route each axis through the hit path from the
    /// deepest scroll container outward. Returns whether an offset changed.
    pub fn scroll_wheel(&mut self, x: f32, y: f32, dx: f32, dy: f32) -> bool {
        self.scroll_chain(x, y, dx, dy).changed()
    }

    /// Route a normalized delta through the real hit path. x and y are
    /// independent; each container consumes only its own enabled axis and a
    /// `contain` boundary stops that axis after its own consumption.
    pub fn scroll_chain(&mut self, x: f32, y: f32, dx: f32, dy: f32) -> ScrollOutcome {
        let mut path = Vec::new();
        if !self.hit_path(1, x, y, &mut path) {
            return ScrollOutcome {
                remaining_x: dx,
                remaining_y: dy,
                ..ScrollOutcome::default()
            };
        }
        self.scroll_path(&path, dx, dy)
    }

    /// Apply an already-frozen path. Used by touch scrolling after the down
    /// sample fixed the target chain.
    pub fn scroll_path(&mut self, path: &[u32], dx: f32, dy: f32) -> ScrollOutcome {
        let mut outcome = ScrollOutcome {
            remaining_x: dx,
            remaining_y: dy,
            ..ScrollOutcome::default()
        };
        for id in path {
            let Ok(node) = self.node(*id) else {
                continue;
            };
            let scroll_x = node.scrolls_x();
            let scroll_y = node.scrolls_y();
            let chain_x = node.chain_x;
            let chain_y = node.chain_y;
            let limit_x = if scroll_x {
                self.max_scroll_x(node)
            } else {
                0.0
            };
            let limit_y = if scroll_y {
                self.max_scroll_y(node)
            } else {
                0.0
            };
            if scroll_x && outcome.remaining_x != 0.0 {
                let node = self.node_mut(*id).expect("scroll node");
                let before = node.scroll_left;
                let next = (before + outcome.remaining_x).clamp(0.0, limit_x);
                node.scroll_left = next;
                let consumed = next - before;
                outcome.consumed_x += consumed;
                outcome.remaining_x -= consumed;
            }
            if scroll_y && outcome.remaining_y != 0.0 {
                let node = self.node_mut(*id).expect("scroll node");
                let before = node.scroll_top;
                let next = (before + outcome.remaining_y).clamp(0.0, limit_y);
                node.scroll_top = next;
                let consumed = next - before;
                outcome.consumed_y += consumed;
                outcome.remaining_y -= consumed;
            }
            if chain_x && scroll_x {
                outcome.truncated_x = true;
                outcome.remaining_x = 0.0;
            }
            if chain_y && scroll_y {
                outcome.truncated_y = true;
                outcome.remaining_y = 0.0;
            }
        }
        if outcome.changed() {
            self.paint_dirty.set(true);
        }
        if self.recognizer.is_tracing() {
            let trace = outcome;
            self.recognizer.trace(|| {
                format!(
                    "scroll consumed x={} y={} remaining x={} y={} truncated x={} y={}",
                    trace.consumed_x,
                    trace.consumed_y,
                    trace.remaining_x,
                    trace.remaining_y,
                    trace.truncated_x,
                    trace.truncated_y
                )
            });
        }
        outcome
    }

    fn scroll_to(&mut self, id: u32, x: f32, y: f32) -> Result<(), String> {
        let (scroll_x, scroll_y) = {
            let node = self.node(id)?;
            (node.scrolls_x(), node.scrolls_y())
        };
        if !scroll_x && !scroll_y {
            return Err(format!("handle {id} is not a scroll container"));
        }
        let limit_x = if scroll_x {
            self.max_scroll_x(self.node(id)?)
        } else {
            0.0
        };
        let limit_y = if scroll_y {
            self.max_scroll_y(self.node(id)?)
        } else {
            0.0
        };
        let node = self.node_mut(id)?;
        node.scroll_left = x.clamp(0.0, limit_x);
        node.scroll_top = y.clamp(0.0, limit_y);
        self.paint_dirty.set(true);
        Ok(())
    }

    fn set_prop(&mut self, id: u32, key: &str, value: &Value) -> Result<(), String> {
        let Some(prop) = Prop::parse(key) else {
            return Err(format!("unsupported prop {key}"));
        };
        match prop {
            Prop::Color => self.node_mut(id)?.color = value.as_color()?,
            Prop::BackgroundColor => self.node_mut(id)?.background = value.as_color()?,
            Prop::Radius => self.node_mut(id)?.radius = value.as_f32()?,
            Prop::Opacity => {
                self.node_mut(id)?.opacity = value.as_f32()?.clamp(0.0, 1.0);
            }
            Prop::FontSize => {
                self.node_mut(id)?.font_size = value.as_f32()?;
                self.refresh_measure(id)?;
            }
            Prop::PaddingLeft => self.node_mut(id)?.padding[0] = value.as_f32()?,
            Prop::PaddingRight => self.node_mut(id)?.padding[1] = value.as_f32()?,
            Prop::PaddingTop => self.node_mut(id)?.padding[2] = value.as_f32()?,
            Prop::PaddingBottom => self.node_mut(id)?.padding[3] = value.as_f32()?,
            Prop::Width => self.node_mut(id)?.width = value.optional_f32()?,
            Prop::Height => self.node_mut(id)?.height = value.optional_f32()?,
            Prop::FlexGrow => self.node_mut(id)?.flex_grow = value.as_f32()?,
            Prop::FlexShrink => self.node_mut(id)?.flex_shrink = value.as_f32()?,
            Prop::ScrollX => self.assign_overflow(id, true, parse_overflow(value)?)?,
            Prop::ScrollY => self.assign_overflow(id, false, parse_overflow(value)?)?,
            Prop::Overflow => {
                let mode = parse_overflow(value)?;
                self.assign_overflow(id, true, mode)?;
                self.assign_overflow(id, false, mode)?;
            }
            Prop::Display => {
                let hide = parse_display_none(value)?;
                let changed = self.node(id)?.display_none != hide;
                self.node_mut(id)?.display_none = hide;
                if hide && changed {
                    self.conceal_subtree(id);
                }
            }
            Prop::Visibility => {
                let hide = parse_visibility_hidden(value)?;
                let changed = self.node(id)?.visibility_hidden != hide;
                self.node_mut(id)?.visibility_hidden = hide;
                if hide && changed {
                    self.conceal_subtree(id);
                }
            }
            Prop::Row => self.node_mut(id)?.row = flex_direction_row(value)?,
            Prop::LineHeight => {
                self.node_mut(id)?.line_height = value.as_f32()?.max(0.0);
                self.refresh_measure(id)?;
            }
            Prop::TextAlign => self.node_mut(id)?.text_align = text_align(value)?,
            Prop::FontWeight => {
                self.node_mut(id)?.font_weight = value.as_u32()?.clamp(1, 1000) as u16;
                self.refresh_measure(id)?;
            }
            Prop::MarginLeft => self.node_mut(id)?.margin[0] = value.as_f32()?,
            Prop::MarginRight => self.node_mut(id)?.margin[1] = value.as_f32()?,
            Prop::MarginTop => self.node_mut(id)?.margin[2] = value.as_f32()?,
            Prop::MarginBottom => self.node_mut(id)?.margin[3] = value.as_f32()?,
            Prop::Gap => self.node_mut(id)?.gap = value.as_f32()?.max(0.0),
            Prop::AlignItems => self.node_mut(id)?.align_items = align_items(value)?,
            Prop::JustifyContent => self.node_mut(id)?.justify_content = justify_content(value)?,
            Prop::Position => self.node_mut(id)?.position = position(value)?,
            Prop::Top => self.node_mut(id)?.inset[0] = value.optional_f32()?.unwrap_or(f32::NAN),
            Prop::Right => self.node_mut(id)?.inset[1] = value.optional_f32()?.unwrap_or(f32::NAN),
            Prop::Bottom => self.node_mut(id)?.inset[2] = value.optional_f32()?.unwrap_or(f32::NAN),
            Prop::Left => self.node_mut(id)?.inset[3] = value.optional_f32()?.unwrap_or(f32::NAN),
            Prop::ScrollChainX => self.node_mut(id)?.chain_x = !overscroll_chains(value)?,
            Prop::ScrollChainY => self.node_mut(id)?.chain_y = !overscroll_chains(value)?,
            Prop::Src => {
                let Value::String(source) = value else {
                    return Err("src must be a string".into());
                };
                self.node_mut(id)?.src = source.clone();
            }
            Prop::Fit => self.node_mut(id)?.fit = image_fit(value)?,
            Prop::Multiline => self.node_mut(id)?.multiline = value.as_bool()?,
            Prop::Secure => {
                self.node_mut(id)?.secure = value.as_bool()?;
                self.refresh_measure(id)?;
            }
            Prop::MaxLength => self.node_mut(id)?.max_length = value.as_u32()?,
            Prop::Keyboard => {
                let Value::String(name) = value else {
                    return Err("keyboard must be a string".into());
                };
                self.node_mut(id)?.keyboard = name.clone();
            }
            Prop::Autofill => {
                let Value::String(name) = value else {
                    return Err("autofill must be a string".into());
                };
                self.node_mut(id)?.autofill = name.clone();
            }
            Prop::Role => {
                let Value::String(name) = value else {
                    return Err("role must be a string".into());
                };
                let role = Role::parse(name).ok_or_else(|| format!("unknown role {name}"))?;
                self.node_mut(id)?.role = Some(role);
            }
            Prop::Label => {
                let Value::String(text) = value else {
                    return Err("label must be a string".into());
                };
                self.node_mut(id)?.label = text.clone();
            }
            Prop::Disabled => self.node_mut(id)?.disabled = value.as_bool()?,
            Prop::Hidden => self.node_mut(id)?.a11y_hidden = value.as_bool()?,
            Prop::Focusable => self.node_mut(id)?.focusable = value.as_bool()?,
            Prop::Modal => self.node_mut(id)?.modal = value.as_bool()?,
            Prop::Checked => self.node_mut(id)?.checked = Some(value.as_bool()?),
            Prop::Selected => self.node_mut(id)?.selected = value.as_bool()?,
            Prop::Expanded => self.node_mut(id)?.expanded = Some(value.as_bool()?),
        }
        self.restyle(id)
    }

    fn restyle(&mut self, id: u32) -> Result<(), String> {
        let (taffy, style) = {
            let node = self.node(id)?;
            (node.taffy, style_for(NodeStyle::from_node(node)))
        };
        self.taffy
            .set_style(taffy, style)
            .map_err(|err| err.to_string())
    }

    fn refresh_measure(&mut self, id: u32) -> Result<(), String> {
        let (taffy, text, font_size, line_height, font_weight) = {
            let node = self.node(id)?;
            if !matches!(node.kind, Kind::Text | Kind::Field) {
                return Ok(());
            }
            let text = if node.kind == Kind::Field {
                display_text(&node.text, node.secure)
            } else {
                node.text.clone()
            };
            (
                node.taffy,
                text,
                node.font_size,
                node.line_height,
                node.font_weight,
            )
        };
        self.taffy
            .set_node_context(
                taffy,
                Some(MeasureCtx {
                    text,
                    font_size,
                    line_height,
                    font_weight,
                }),
            )
            .map_err(|err| err.to_string())
    }

    pub fn node_box(&self, id: u32) -> Result<(f32, f32, f32, f32), String> {
        let node = self.node(id)?;
        Ok((node.x, node.y, node.w, node.h))
    }

    pub fn text(&self, id: u32) -> Result<&str, String> {
        Ok(&self.node(id)?.text)
    }

    /// `(left, top)` offsets of a scroll container.
    pub fn scroll_of(&self, id: u32) -> Result<(f32, f32), String> {
        let node = self.node(id)?;
        Ok((node.scroll_left, node.scroll_top))
    }

    pub fn next_handle(&self) -> u32 {
        self.high_water + 1
    }

    fn node(&self, id: u32) -> Result<&Node, String> {
        self.nodes
            .get(id as usize)
            .and_then(Option::as_ref)
            .ok_or_else(|| format!("bad handle {id}"))
    }

    fn node_mut(&mut self, id: u32) -> Result<&mut Node, String> {
        self.nodes
            .get_mut(id as usize)
            .and_then(Option::as_mut)
            .ok_or_else(|| format!("bad handle {id}"))
    }
    fn insert_node(&mut self, desc: NodeDesc, parent: u32, index: u32) -> Result<(), String> {
        let handle = desc.id;
        if handle == 0 || self.retired.contains(&handle) || self.node(handle).is_ok() {
            return Err(format!("handle {handle} is not free"));
        }
        let parent_len = self.node(parent)?.children.len() as u32;
        if index > parent_len {
            return Err(format!("index {index} past {parent_len} children"));
        }
        let style = style_for(NodeStyle {
            row: desc.row,
            overflow_x: if desc.scroll_x {
                Overflow::Scroll
            } else {
                Overflow::Visible
            },
            overflow_y: if desc.scroll_y {
                Overflow::Scroll
            } else {
                Overflow::Visible
            },
            flex_grow: desc.flex_grow,
            flex_shrink: desc.flex_shrink,
            width: desc.width,
            height: desc.height,
            padding: desc.padding,
            ..NodeStyle::default()
        });
        let taffy_id = if matches!(desc.kind, Kind::Text | Kind::Field) {
            self.taffy
                .new_leaf_with_context(
                    style,
                    MeasureCtx {
                        text: desc.text.clone(),
                        font_size: desc.font_size,
                        line_height: 0.0,
                        font_weight: 400,
                    },
                )
                .map_err(|err| err.to_string())?
        } else {
            self.taffy.new_leaf(style).map_err(|err| err.to_string())?
        };
        if self.nodes.len() <= handle as usize {
            self.nodes.resize_with(handle as usize + 1, || None);
        }
        self.nodes[handle as usize] = Some(Node {
            parent,
            children: Vec::new(),
            kind: desc.kind,
            overflow_x: if desc.scroll_x {
                Overflow::Scroll
            } else {
                Overflow::Visible
            },
            overflow_y: if desc.scroll_y {
                Overflow::Scroll
            } else {
                Overflow::Visible
            },
            background: Color::from_argb(desc.background),
            color: Color::from_argb(desc.color),
            font_size: desc.font_size,
            radius: desc.radius,
            opacity: desc.opacity,
            handler: desc.handler,
            width: desc.width,
            height: desc.height,
            flex_grow: desc.flex_grow,
            flex_shrink: desc.flex_shrink,
            row: desc.row,
            padding: desc.padding,
            margin: [0.0; 4],
            gap: 0.0,
            align_items: 0,
            justify_content: 0,
            position: 0,
            inset: [f32::NAN; 4],
            line_height: 0.0,
            text_align: 0,
            font_weight: 400,
            src: String::new(),
            fit: 0,
            text: desc.text.clone(),
            taffy: taffy_id,
            scroll_left: 0.0,
            scroll_top: 0.0,
            chain_x: false,
            chain_y: false,
            multiline: false,
            secure: false,
            max_length: 0,
            keyboard: String::new(),
            autofill: String::new(),
            role: None,
            label: String::new(),
            disabled: false,
            a11y_hidden: false,
            display_none: false,
            visibility_hidden: false,
            focusable: false,
            modal: false,
            checked: None,
            selected: false,
            expanded: None,
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
        });
        self.node_mut(parent)?
            .children
            .insert(index as usize, handle);
        self.sync_children(parent)?;
        self.high_water = self.high_water.max(handle);
        if desc.kind == Kind::Field {
            self.fields.insert(
                handle,
                EditSession::new(
                    &desc.text,
                    EditConfig {
                        multiline: false,
                        max_length: 0,
                    },
                ),
            );
        }
        Ok(())
    }

    fn dispose_node(&mut self, handle: u32) -> Result<(), String> {
        if handle <= 1 {
            return Err("cannot dispose the root".into());
        }
        let parent = self.node(handle)?.parent;
        let children = self.node_mut(parent)?.children.clone();
        let Some(pos) = children.iter().position(|id| *id == handle) else {
            return Err(format!("handle {handle} is not a child"));
        };
        self.node_mut(parent)?.children.remove(pos);
        self.sync_children(parent)?;
        self.cancel_gestures_in_subtree(handle);
        self.destroy_subtree(handle)
    }
    fn move_node(&mut self, handle: u32, parent: u32, index: u32) -> Result<(), String> {
        if handle <= 1 {
            return Err("cannot move the root".into());
        }
        if self.contains(handle, parent) {
            return Err("cannot move a node under itself".into());
        }
        let old_parent = self.node(handle)?.parent;
        let old_children = self.node(old_parent)?.children.clone();
        let Some(pos) = old_children.iter().position(|id| *id == handle) else {
            return Err(format!("handle {handle} is not a child"));
        };
        let same = old_parent == parent;
        let dest_len = self.node(parent)?.children.len() as u32 - u32::from(same);
        if index > dest_len {
            return Err(format!("index {index} past {dest_len} children"));
        }
        self.node_mut(old_parent)?.children.remove(pos);
        self.node_mut(parent)?
            .children
            .insert(index as usize, handle);
        self.node_mut(handle)?.parent = parent;
        if same {
            self.sync_children(parent)?;
        } else {
            self.sync_children(old_parent)?;
            self.sync_children(parent)?;
        }
        Ok(())
    }

    fn contains(&self, ancestor: u32, mut id: u32) -> bool {
        while id != 0 {
            if id == ancestor {
                return true;
            }
            id = self.node(id).map(|node| node.parent).unwrap_or(0);
        }
        false
    }

    fn assign_overflow(&mut self, id: u32, horizontal: bool, mode: Overflow) -> Result<(), String> {
        let node = self.node_mut(id)?;
        if horizontal {
            node.overflow_x = mode;
            if mode != Overflow::Scroll {
                node.scroll_left = 0.0;
            }
        } else {
            node.overflow_y = mode;
            if mode != Overflow::Scroll {
                node.scroll_top = 0.0;
            }
        }
        Ok(())
    }

    /// Cancel active and pending gestures in this subtree without disposing them.
    fn conceal_subtree(&mut self, id: u32) {
        self.recognizer.conceal_target(id);
        let count = self
            .nodes
            .get(id as usize)
            .and_then(Option::as_ref)
            .map(|node| node.children.len())
            .unwrap_or(0);
        for index in 0..count {
            let child = self.nodes[id as usize]
                .as_ref()
                .expect("conceal child")
                .children[index];
            self.conceal_subtree(child);
        }
    }

    fn cancel_gestures_in_subtree(&mut self, handle: u32) {
        self.recognizer.target_disposed(handle);
        let Some(node) = self.nodes.get(handle as usize).and_then(Option::as_ref) else {
            return;
        };
        let children = node.children.clone();
        for child in children {
            self.cancel_gestures_in_subtree(child);
        }
    }

    fn destroy_subtree(&mut self, handle: u32) -> Result<(), String> {
        let children = self.node(handle)?.children.clone();
        for child in children {
            self.destroy_subtree(child)?;
        }
        let taffy_id = self.node(handle)?.taffy;
        if self
            .edit_batch
            .as_ref()
            .is_some_and(|batch| batch.handle == handle)
        {
            self.edit_batch = None;
        }
        self.taffy.remove(taffy_id).map_err(|err| err.to_string())?;
        self.nodes[handle as usize] = None;
        if let Some(mut session) = self.fields.remove(&handle) {
            session.close();
        }
        if self.focused == handle {
            self.focused = 0;
        }
        self.retired.insert(handle);
        Ok(())
    }

    fn sync_children(&mut self, parent: u32) -> Result<(), String> {
        let child_ids = self.node(parent)?.children.clone();
        let mut taffy_children = Vec::with_capacity(child_ids.len());
        for id in child_ids {
            taffy_children.push(self.node(id)?.taffy);
        }
        let parent_id = self.node(parent)?.taffy;
        self.taffy
            .set_children(parent_id, &taffy_children)
            .map_err(|err| err.to_string())
    }

    fn place(&mut self, id: u32, origin_x: f32, origin_y: f32) -> Result<(), String> {
        let taffy_id = self.node(id)?.taffy;
        let layout = *self.taffy.layout(taffy_id).map_err(|err| err.to_string())?;
        let x = origin_x + layout.location.x;
        let y = origin_y + layout.location.y;
        let scroll_left = self.node(id)?.scroll_left;
        let scroll_top = self.node(id)?.scroll_top;
        let children = self.node(id)?.children.clone();
        {
            let node = self.node_mut(id)?;
            node.x = x;
            node.y = y;
            node.w = layout.size.width;
            node.h = layout.size.height;
        }
        for child in children {
            self.place(child, x - scroll_left, y - scroll_top)?;
        }
        Ok(())
    }

    fn max_scroll_x(&self, node: &Node) -> f32 {
        let Ok(layout) = self.taffy.layout(node.taffy) else {
            return 0.0;
        };
        let visible = (layout.size.width - layout.padding.left - layout.padding.right).max(0.0);
        let content = layout.scrollable_overflow_rect.right.max(0.0);
        (content - visible).max(0.0)
    }

    fn max_scroll_y(&self, node: &Node) -> f32 {
        let Ok(layout) = self.taffy.layout(node.taffy) else {
            return 0.0;
        };
        let visible = (layout.size.height - layout.padding.top - layout.padding.bottom).max(0.0);
        let content = layout.scrollable_overflow_rect.bottom.max(0.0);
        (content - visible).max(0.0)
    }

    /// Recompute caret and selection from the current platform snapshot.
    /// An old revision or a mismatched width/font/align/density is an error.
    pub fn refresh_caret(
        &mut self,
        host: &mut dyn FnMut(&LayoutRequest<'_>) -> Result<LayoutSnapshot, String>,
    ) -> Result<(), String> {
        self.caret_handle = 0;
        self.caret_box = None;
        self.selection_rects.clear();
        self.selection_handles = None;
        let handle = self.focused;
        if handle == 0 {
            return Ok(());
        }
        let Some(state) = self.fields.get(&handle).cloned() else {
            return Ok(());
        };
        if self
            .node(handle)
            .ok()
            .filter(|node| node.kind == Kind::Field)
            .is_none()
        {
            return Ok(());
        }
        let (geom, snapshot) = self.field_snapshot(handle, host)?;
        let active = snap_unit(&geom.text, state.sel_end()).min(snapshot.units);
        let caret = snapshot.caret(active).ok_or("layout has no caret")?;
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
        Ok(FieldGeom {
            text: display_text(&node.text, node.secure),
            origin_x: node.x + pads[0],
            origin_y: node.y + pads[2],
            key: LayoutKey {
                revision,
                width: (node.w - pads[0] - pads[1]).max(0.0),
                font_size,
                font_weight: node.font_weight,
                line_height,
                align: node.text_align,
                density: 1.0,
            },
        })
    }

    fn paint_node(&self, id: u32, out: &mut Vec<PaintCmd>) {
        let node = self.node(id).expect("paint handle");
        if !node.painted() {
            return;
        }
        let x = node.x;
        let y = node.y;
        let w = node.w;
        let h = node.h;
        let radius = node.radius;
        let background = node.background;
        let kind = node.kind;
        let text = node.text.clone();
        let secure = node.secure;
        let color = node.color;
        let font_size = node.font_size;
        let opacity = node.opacity.clamp(0.0, 1.0);
        let line_height = node.line_height;
        let weight = node.font_weight;
        let align = node.text_align;
        let fit = node.fit;
        let src = node.src.clone();
        let overflow_x = node.overflow_x;
        let overflow_y = node.overflow_y;
        let child_count = node.children.len();
        let pads = self.pads(id);
        let clip = paint_clip(
            overflow_x,
            overflow_y,
            x,
            y,
            w,
            h,
            self.viewport.width,
            self.viewport.height,
        );
        if opacity < 1.0 {
            out.push(PaintCmd::PushOpacity { opacity });
        }
        if background.is_visible() {
            out.push(PaintCmd::Fill {
                x,
                y,
                w,
                h,
                radius,
                color: background,
            });
        }
        if let Some((cx, cy, cw, ch)) = clip {
            out.push(PaintCmd::PushClip {
                x: cx,
                y: cy,
                w: cw,
                h: ch,
            });
        }
        if kind == Kind::Field && id == self.caret_handle {
            for (sx, sy, sw, sh) in &self.selection_rects {
                out.push(PaintCmd::Fill {
                    x: *sx,
                    y: *sy,
                    w: *sw,
                    h: *sh,
                    radius: 0.0,
                    color: Color::new(0.2, 0.45, 1.0, 0.35),
                });
            }
        }
        let shown = if kind == Kind::Field {
            display_text(&text, secure)
        } else {
            text
        };
        if matches!(kind, Kind::Text | Kind::Field) && !shown.is_empty() {
            let (text_x, text_y, text_w, text_h) = if kind == Kind::Field {
                (
                    x + pads[0],
                    y + pads[2],
                    (w - pads[0] - pads[1]).max(0.0),
                    (h - pads[2] - pads[3]).max(0.0),
                )
            } else {
                (x, y, w, h)
            };
            out.push(PaintCmd::Text {
                x: text_x,
                y: text_y,
                w: text_w,
                h: text_h,
                font_size,
                color,
                line_height,
                weight,
                align,
                text: shown,
            });
        } else if kind == Kind::Image && !src.is_empty() {
            out.push(PaintCmd::Image {
                x,
                y,
                w,
                h,
                radius,
                fit,
                src,
            });
        }
        if kind == Kind::Field && id == self.caret_handle {
            if let Some((cx, cy, cw, ch)) = self.caret_box {
                out.push(PaintCmd::Fill {
                    x: cx,
                    y: cy,
                    w: cw,
                    h: ch,
                    radius: 0.0,
                    color,
                });
            }
        }
        for index in 0..child_count {
            let child = self.node(id).expect("paint child").children[index];
            self.paint_node(child, out);
        }
        if clip.is_some() {
            out.push(PaintCmd::PopClip);
        }
        if id == self.focused && kind != Kind::Field && self.is_focusable(id) {
            let color = Color::new(0.30, 0.64, 1.0, 0.95);
            let thickness = 2.0;
            out.push(PaintCmd::Fill {
                x,
                y,
                w,
                h: thickness,
                radius: 0.0,
                color,
            });
            out.push(PaintCmd::Fill {
                x,
                y: y + h - thickness,
                w,
                h: thickness,
                radius: 0.0,
                color,
            });
            out.push(PaintCmd::Fill {
                x,
                y,
                w: thickness,
                h,
                radius: 0.0,
                color,
            });
            out.push(PaintCmd::Fill {
                x: x + w - thickness,
                y,
                w: thickness,
                h,
                radius: 0.0,
                color,
            });
        }
        if opacity < 1.0 {
            out.push(PaintCmd::PopOpacity);
        }
    }

    fn pads(&self, id: u32) -> [f32; 4] {
        let node = self.node(id).expect("pad handle");
        match self.taffy.layout(node.taffy) {
            Ok(layout) => [
                layout.padding.left,
                layout.padding.right,
                layout.padding.top,
                layout.padding.bottom,
            ],
            Err(_) => node.padding,
        }
    }

}


#[derive(Clone, Copy)]
struct Bounds {
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
}

impl Bounds {
    fn contains(self, x: f32, y: f32) -> bool {
        x >= self.x0 && y >= self.y0 && x < self.x1 && y < self.y1
    }

    fn intersect(self, other: Self) -> Self {
        Self {
            x0: self.x0.max(other.x0),
            y0: self.y0.max(other.y0),
            x1: self.x1.min(other.x1),
            y1: self.y1.min(other.y1),
        }
    }
}

/// Clip rect for one node. A visible axis uses the viewport; a hidden or
/// scroll axis uses the outer box, which is the padding box when there is no border.
fn axis_bounds(
    overflow_x: Overflow,
    overflow_y: Overflow,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    viewport_w: f32,
    viewport_h: f32,
) -> Bounds {
    Bounds {
        x0: if overflow_x == Overflow::Visible { 0.0 } else { x },
        y0: if overflow_y == Overflow::Visible { 0.0 } else { y },
        x1: if overflow_x == Overflow::Visible {
            viewport_w
        } else {
            x + w
        },
        y1: if overflow_y == Overflow::Visible {
            viewport_h
        } else {
            y + h
        },
    }
}

fn paint_clip(
    overflow_x: Overflow,
    overflow_y: Overflow,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    viewport_w: f32,
    viewport_h: f32,
) -> Option<(f32, f32, f32, f32)> {
    if overflow_x == Overflow::Visible && overflow_y == Overflow::Visible {
        return None;
    }
    let bounds = axis_bounds(
        overflow_x,
        overflow_y,
        x,
        y,
        w,
        h,
        viewport_w,
        viewport_h,
    );
    Some((
        bounds.x0,
        bounds.y0,
        (bounds.x1 - bounds.x0).max(0.0),
        (bounds.y1 - bounds.y0).max(0.0),
    ))
}

fn style_for(node: NodeStyle) -> Style {
    let mut style = Style {
        display: node.display,
        flex_direction: if node.row {
            FlexDirection::Row
        } else {
            FlexDirection::Column
        },
        flex_grow: node.flex_grow,
        flex_shrink: node.flex_shrink,
        size: Size {
            width: dim(node.width),
            height: dim(node.height),
        },
        padding: Rect {
            left: length(node.padding[0]),
            right: length(node.padding[1]),
            top: length(node.padding[2]),
            bottom: length(node.padding[3]),
        },
        margin: Rect {
            left: length(node.margin[0]),
            right: length(node.margin[1]),
            top: length(node.margin[2]),
            bottom: length(node.margin[3]),
        },
        gap: Size {
            width: length(node.gap),
            height: length(node.gap),
        },
        align_items: match node.align_items {
            1 => AlignItems::START,
            2 => AlignItems::CENTER,
            3 => AlignItems::END,
            4 => AlignItems::BASELINE,
            _ => AlignItems::STRETCH,
        },
        justify_content: match node.justify_content {
            1 => JustifyContent::CENTER,
            2 => JustifyContent::END,
            3 => JustifyContent::SPACE_BETWEEN,
            4 => JustifyContent::SPACE_AROUND,
            5 => JustifyContent::SPACE_EVENLY,
            _ => JustifyContent::START,
        },
        position: if node.position == 1 {
            Position::Absolute
        } else {
            Position::Relative
        },
        inset: Rect {
            top: inset(node.inset[0]),
            right: inset(node.inset[1]),
            bottom: inset(node.inset[2]),
            left: inset(node.inset[3]),
        },
        ..Style::default()
    };
    if node.overflow_x == Overflow::Scroll || node.flex_grow > 0.0 {
        style.min_size.width = length(0.0);
    }
    if node.overflow_y == Overflow::Scroll || node.flex_grow > 0.0 {
        style.min_size.height = length(0.0);
    }
    style.overflow.x = node.overflow_x;
    style.overflow.y = node.overflow_y;
    if node.overflow_x == Overflow::Scroll || node.overflow_y == Overflow::Scroll {
        style.scrollbar_width = 0.0;
    }
    style
}

fn inset(value: f32) -> LengthPercentageAuto {
    if value.is_finite() {
        length(value)
    } else {
        auto()
    }
}

fn text_align(value: &Value) -> Result<u8, String> {
    let Value::String(text) = value else {
        return Err("textAlign must be a string".into());
    };
    match text.as_str() {
        "left" => Ok(0),
        "center" => Ok(1),
        "right" => Ok(2),
        other => Err(format!("unsupported textAlign {other}")),
    }
}

fn align_items(value: &Value) -> Result<u8, String> {
    let Value::String(text) = value else {
        return Err("alignItems must be a string".into());
    };
    match text.as_str() {
        "stretch" => Ok(0),
        "start" | "flex-start" => Ok(1),
        "center" => Ok(2),
        "end" | "flex-end" => Ok(3),
        "baseline" => Ok(4),
        other => Err(format!("unsupported alignItems {other}")),
    }
}

fn justify_content(value: &Value) -> Result<u8, String> {
    let Value::String(text) = value else {
        return Err("justifyContent must be a string".into());
    };
    match text.as_str() {
        "start" | "flex-start" => Ok(0),
        "center" => Ok(1),
        "end" | "flex-end" => Ok(2),
        "space-between" => Ok(3),
        "space-around" => Ok(4),
        "space-evenly" => Ok(5),
        other => Err(format!("unsupported justifyContent {other}")),
    }
}

fn position(value: &Value) -> Result<u8, String> {
    let Value::String(text) = value else {
        return Err("position must be a string".into());
    };
    match text.as_str() {
        "relative" => Ok(0),
        "absolute" => Ok(1),
        other => Err(format!("unsupported position {other}")),
    }
}

fn image_fit(value: &Value) -> Result<u8, String> {
    let Value::String(text) = value else {
        return Err("fit must be a string".into());
    };
    match text.as_str() {
        "contain" => Ok(0),
        "cover" => Ok(1),
        "fill" => Ok(2),
        other => Err(format!("unsupported fit {other}")),
    }
}

fn parse_overflow(value: &Value) -> Result<Overflow, String> {
    let Value::String(text) = value else {
        return Err("overflow must be a string".into());
    };
    match text.as_str() {
        "visible" => Ok(Overflow::Visible),
        "hidden" => Ok(Overflow::Hidden),
        "scroll" => Ok(Overflow::Scroll),
        other => Err(format!("unsupported overflow {other}")),
    }
}

fn parse_display_none(value: &Value) -> Result<bool, String> {
    let Value::String(text) = value else {
        return Err("display must be a string".into());
    };
    match text.as_str() {
        "flex" => Ok(false),
        "none" => Ok(true),
        other => Err(format!("unsupported display {other}")),
    }
}

fn parse_visibility_hidden(value: &Value) -> Result<bool, String> {
    let Value::String(text) = value else {
        return Err("visibility must be a string".into());
    };
    match text.as_str() {
        "visible" => Ok(false),
        "hidden" => Ok(true),
        other => Err(format!("unsupported visibility {other}")),
    }
}

fn flex_direction_row(value: &Value) -> Result<bool, String> {
    let Value::String(text) = value else {
        return Err("flexDirection must be a string".into());
    };
    match text.as_str() {
        "row" => Ok(true),
        "column" => Ok(false),
        other => Err(format!("unsupported flexDirection {other}")),
    }
}

fn overscroll_chains(value: &Value) -> Result<bool, String> {
    value
        .as_bool()
        .map_err(|_| "overscroll must be a bool".into())
}

fn dim(value: Option<f32>) -> Dimension {
    match value {
        Some(value) => length(value),
        None => auto(),
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

fn utf16_slice(value: &str, start: u32, end: u32) -> String {
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
