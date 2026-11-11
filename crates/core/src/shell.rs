//! Retained tree. Taffy owns flex; this type owns scroll offsets, hit testing, and paint order.

use std::cell::{Cell, Ref, RefCell};
use std::collections::HashSet;

use crate::ir::{Kind, NodeDesc};
use crate::ops::{decode_ops, Op, Value};
use taffy::prelude::*;
use taffy::style::Overflow;
use taffy::{compute_leaf_layout, NodeId, TaffyTree};

#[derive(Clone, Debug)]
struct MeasureCtx {
    text: String,
    font_size: f32,
}

#[allow(dead_code)]
struct Node {
    parent: u32,
    children: Vec<u32>,
    kind: Kind,
    scroll_y: bool,
    background: u32,
    color: u32,
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
    text: String,
    taffy: NodeId,
    scroll: f32,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

pub struct Shell {
    nodes: Vec<Option<Node>>,
    retired: HashSet<u32>,
    high_water: u32,
    taffy: TaffyTree<MeasureCtx>,
    viewport: Size<f32>,
    paint_cache: RefCell<Vec<PaintCmd>>,
    paint_dirty: Cell<bool>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PaintCmd {
    Fill {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        radius: f32,
        color: u32,
    },
    Text {
        x: f32,
        y: f32,
        w: f32,
        font_size: f32,
        color: u32,
        text: String,
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

impl Shell {
    /// Empty viewport root. Function components append children with `spawn`.
    pub fn blank() -> Result<Self, String> {
        let mut taffy: TaffyTree<MeasureCtx> = TaffyTree::new();
        let style = style_for(false, false, 0.0, 1.0, None, None, [0.0; 4]);
        let taffy_id = taffy.new_leaf(style).map_err(|err| err.to_string())?;
        Ok(Self {
            nodes: vec![
                None,
                Some(Node {
                    parent: 0,
                    children: Vec::new(),
                    kind: Kind::View,
                    scroll_y: false,
                    background: 0,
                    color: 0xffff_ffff,
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
                    text: String::new(),
                    taffy: taffy_id,
                    scroll: 0.0,
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
                    let taffy_id = node.taffy;
                    self.taffy
                        .set_node_context(taffy_id, Some(MeasureCtx { text, font_size }))
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
                Op::ScrollTo { handle, y } => self.scroll_to(handle, y)?,
                Op::Spawn { desc } => self.spawn_desc(desc)?,
                Op::SpawnAt { desc, index } => {
                    let parent = desc.parent;
                    self.insert_node(desc, parent, index)?
                }
            }
        }
        self.paint_dirty.set(true);
        Ok(seq)
    }

    pub fn layout(
        &mut self,
        measure: &mut dyn FnMut(&str, f32, Option<f32>) -> (f32, f32),
    ) -> Result<(), String> {
        if self.viewport.width <= 0.0 || self.viewport.height <= 0.0 {
            return Err("viewport is empty".into());
        }
        let root = self.node(1)?.taffy;
        let root_style = {
            let root_node = self.node(1)?;
            style_for(
                root_node.row,
                root_node.scroll_y,
                root_node.flex_grow,
                root_node.flex_shrink,
                Some(self.viewport.width),
                Some(self.viewport.height),
                root_node.padding,
            )
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
                    let measured = ctx.map(|ctx| (ctx.text.clone(), ctx.font_size));
                    compute_leaf_layout(
                        inputs,
                        style,
                        |_, _| 0.0,
                        |known, available| {
                            let Some((text, font_size)) = measured.as_ref() else {
                                return Size::ZERO;
                            };
                            if let (Some(width), Some(height)) = (known.width, known.height) {
                                return Size { width, height };
                            }
                            let max_width = match available.width {
                                AvailableSpace::Definite(width) => Some(width),
                                _ => known.width,
                            };
                            let (width, height) = measure(text, *font_size, max_width);
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
        let limits: Vec<(usize, f32)> = self
            .nodes
            .iter()
            .enumerate()
            .filter_map(|(index, node)| {
                let node = node.as_ref()?;
                Some((
                    index,
                    if node.scroll_y {
                        self.max_scroll(node)
                    } else {
                        0.0
                    },
                ))
            })
            .collect();
        for (index, limit) in limits {
            if let Some(node) = &mut self.nodes[index] {
                if node.scroll_y {
                    node.scroll = node.scroll.clamp(0.0, limit);
                }
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
        match self.hit(1, x, y) {
            Hit::Miss => None,
            Hit::Inside { handler } => handler,
        }
    }

    pub fn scroll_wheel(&mut self, x: f32, y: f32, dy: f32) -> bool {
        let Some(id) = self.scroll_target(1, x, y) else {
            return false;
        };
        let limit = self.max_scroll(self.node(id).expect("scroll target"));
        let node = self.node_mut(id).expect("scroll target");
        let next = (node.scroll + dy).clamp(0.0, limit);
        if (next - node.scroll).abs() < f32::EPSILON {
            return false;
        }
        node.scroll = next;
        self.paint_dirty.set(true);
        true
    }

    fn scroll_to(&mut self, id: u32, y: f32) -> Result<(), String> {
        if !self.node(id)?.scroll_y {
            return Err(format!("handle {id} is not a scroll container"));
        }
        let limit = self.max_scroll(self.node(id)?);
        self.node_mut(id)?.scroll = y.clamp(0.0, limit);
        self.paint_dirty.set(true);
        Ok(())
    }

    fn set_prop(&mut self, id: u32, key: &str, value: &Value) -> Result<(), String> {
        match key {
            "color" => self.node_mut(id)?.color = value.as_u32()?,
            "backgroundColor" => self.node_mut(id)?.background = value.as_u32()?,
            "radius" => self.node_mut(id)?.radius = value.as_f32()?,
            "opacity" => {
                self.node_mut(id)?.opacity = value.as_f32()?.clamp(0.0, 1.0);
            }
            "fontSize" => {
                let (text, font_size, taffy) = {
                    let node = self.node_mut(id)?;
                    node.font_size = value.as_f32()?;
                    (node.text.clone(), node.font_size, node.taffy)
                };
                self.taffy
                    .set_node_context(taffy, Some(MeasureCtx { text, font_size }))
                    .map_err(|err| err.to_string())?;
            }
            "width" => self.node_mut(id)?.width = value.optional_f32()?,
            "height" => self.node_mut(id)?.height = value.optional_f32()?,
            "flexGrow" => self.node_mut(id)?.flex_grow = value.as_f32()?,
            "flexShrink" => self.node_mut(id)?.flex_shrink = value.as_f32()?,
            "scrollY" => self.node_mut(id)?.scroll_y = value.as_bool()?,
            "row" => self.node_mut(id)?.row = value.as_bool()?,
            other => return Err(format!("unsupported prop {other}")),
        }
        self.restyle(id)
    }

    fn restyle(&mut self, id: u32) -> Result<(), String> {
        let (taffy, style) = {
            let node = self.node(id)?;
            (
                node.taffy,
                style_for(
                    node.row,
                    node.scroll_y,
                    node.flex_grow,
                    node.flex_shrink,
                    node.width,
                    node.height,
                    node.padding,
                ),
            )
        };
        self.taffy
            .set_style(taffy, style)
            .map_err(|err| err.to_string())
    }

    pub fn node_box(&self, id: u32) -> Result<(f32, f32, f32, f32), String> {
        let node = self.node(id)?;
        Ok((node.x, node.y, node.w, node.h))
    }

    pub fn text(&self, id: u32) -> Result<&str, String> {
        Ok(&self.node(id)?.text)
    }

    pub fn scroll_of(&self, id: u32) -> Result<f32, String> {
        Ok(self.node(id)?.scroll)
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
        let style = style_for(
            desc.row,
            desc.scroll_y,
            desc.flex_grow,
            desc.flex_shrink,
            desc.width,
            desc.height,
            desc.padding,
        );
        let taffy_id = if desc.kind == Kind::Text {
            self.taffy
                .new_leaf_with_context(
                    style,
                    MeasureCtx {
                        text: desc.text.clone(),
                        font_size: desc.font_size,
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
            scroll_y: desc.scroll_y,
            background: desc.background,
            color: desc.color,
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
            text: desc.text.clone(),
            taffy: taffy_id,
            scroll: 0.0,
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

    fn destroy_subtree(&mut self, handle: u32) -> Result<(), String> {
        let children = self.node(handle)?.children.clone();
        for child in children {
            self.destroy_subtree(child)?;
        }
        let taffy_id = self.node(handle)?.taffy;
        self.taffy.remove(taffy_id).map_err(|err| err.to_string())?;
        self.nodes[handle as usize] = None;
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
        let scroll = self.node(id)?.scroll;
        let children = self.node(id)?.children.clone();
        {
            let node = self.node_mut(id)?;
            node.x = x;
            node.y = y;
            node.w = layout.size.width;
            node.h = layout.size.height;
        }
        for child in children {
            self.place(child, x, y - scroll)?;
        }
        Ok(())
    }

    fn max_scroll(&self, node: &Node) -> f32 {
        let Ok(layout) = self.taffy.layout(node.taffy) else {
            return 0.0;
        };
        let visible = (layout.size.height - layout.padding.top - layout.padding.bottom).max(0.0);
        let content = layout.scrollable_overflow_rect.bottom.max(0.0);
        (content - visible).max(0.0)
    }

    fn paint_node(&self, id: u32, out: &mut Vec<PaintCmd>) {
        let node = self.node(id).expect("paint handle");
        let x = node.x;
        let y = node.y;
        let w = node.w;
        let h = node.h;
        let radius = node.radius;
        let background = node.background;
        let kind = node.kind;
        let text = node.text.clone();
        let color = node.color;
        let font_size = node.font_size;
        let opacity = node.opacity.clamp(0.0, 1.0);
        let scroll_y = node.scroll_y;
        let children = node.children.clone();
        let pads = self.pads(id);
        if opacity < 1.0 {
            out.push(PaintCmd::PushOpacity { opacity });
        }
        if background != 0 {
            out.push(PaintCmd::Fill {
                x,
                y,
                w,
                h,
                radius,
                color: background,
            });
        }
        if kind == Kind::Text && !text.is_empty() {
            out.push(PaintCmd::Text {
                x,
                y,
                w,
                font_size,
                color,
                text,
            });
        }
        if scroll_y {
            out.push(PaintCmd::PushClip {
                x: x + pads[0],
                y: y + pads[2],
                w: (w - pads[0] - pads[1]).max(0.0),
                h: (h - pads[2] - pads[3]).max(0.0),
            });
        }
        for child in children {
            self.paint_node(child, out);
        }
        if scroll_y {
            out.push(PaintCmd::PopClip);
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

    fn hit(&self, id: u32, x: f32, y: f32) -> Hit {
        let node = self.node(id).expect("hit handle");
        let nx = node.x;
        let ny = node.y;
        let nw = node.w;
        let nh = node.h;
        let scroll_y = node.scroll_y;
        let handler = node.handler;
        let children = node.children.clone();
        if x < nx || y < ny || x >= nx + nw || y >= ny + nh {
            return Hit::Miss;
        }
        if scroll_y {
            let pads = self.pads(id);
            let left = nx + pads[0];
            let top = ny + pads[2];
            let right = nx + nw - pads[1];
            let bottom = ny + nh - pads[3];
            if x < left || y < top || x >= right || y >= bottom {
                return Hit::Inside {
                    handler: nonzero(handler),
                };
            }
        }
        for child in children.iter().rev() {
            match self.hit(*child, x, y) {
                Hit::Miss => {}
                Hit::Inside {
                    handler: Some(found),
                } => {
                    return Hit::Inside {
                        handler: Some(found),
                    };
                }
                Hit::Inside { handler: None } => {
                    return Hit::Inside {
                        handler: nonzero(handler),
                    };
                }
            }
        }
        Hit::Inside {
            handler: nonzero(handler),
        }
    }

    fn scroll_target(&self, id: u32, x: f32, y: f32) -> Option<u32> {
        let node = self.node(id).expect("scroll handle");
        if x < node.x || y < node.y || x >= node.x + node.w || y >= node.y + node.h {
            return None;
        }
        let children = node.children.clone();
        for child in children.iter().rev() {
            if let Some(found) = self.scroll_target(*child, x, y) {
                return Some(found);
            }
        }
        let node = self.node(id).expect("scroll handle");
        if node.scroll_y && self.max_scroll(node) > 0.0 {
            Some(id)
        } else {
            None
        }
    }
}

enum Hit {
    Miss,
    Inside { handler: Option<u32> },
}

fn nonzero(handler: u32) -> Option<u32> {
    if handler == 0 {
        None
    } else {
        Some(handler)
    }
}

fn style_for(
    row: bool,
    scroll_y: bool,
    flex_grow: f32,
    flex_shrink: f32,
    width: Option<f32>,
    height: Option<f32>,
    padding: [f32; 4],
) -> Style {
    let mut style = Style::default();
    style.display = Display::Flex;
    style.flex_direction = if row {
        FlexDirection::Row
    } else {
        FlexDirection::Column
    };
    style.flex_grow = flex_grow;
    style.flex_shrink = flex_shrink;
    style.size = Size {
        width: dim(width),
        height: dim(height),
    };
    style.padding = Rect {
        left: length(padding[0]),
        right: length(padding[1]),
        top: length(padding[2]),
        bottom: length(padding[3]),
    };
    if scroll_y || flex_grow > 0.0 {
        style.min_size.height = length(0.0);
    }
    if scroll_y {
        style.overflow.y = Overflow::Scroll;
        style.scrollbar_width = 0.0;
    }
    style
}

fn dim(value: Option<f32>) -> Dimension {
    match value {
        Some(value) => length(value),
        None => auto(),
    }
}
