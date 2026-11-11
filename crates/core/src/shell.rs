//! Retained tree. Taffy owns flex; this type owns scroll offsets, hit testing, and paint order.

use std::cell::{Cell, Ref, RefCell};
use std::collections::HashSet;

use crate::ir::{Kind, NodeDesc};
use crate::ops::{decode_ops, Op, Value};
use crate::props::Prop;
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

#[derive(Clone, Copy)]
struct NodeStyle {
    row: bool,
    scroll_x: bool,
    scroll_y: bool,
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
            scroll_x: node.scroll_x,
            scroll_y: node.scroll_y,
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
            scroll_x: false,
            scroll_y: false,
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
    scroll_x: bool,
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
        h: f32,
        font_size: f32,
        color: u32,
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
                    scroll_x: false,
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
            }
        }
        self.paint_dirty.set(true);
        Ok(seq)
    }

    pub fn layout(
        &mut self,
        measure: &mut dyn FnMut(&str, f32, f32, u16, Option<f32>) -> (f32, f32),
    ) -> Result<(), String> {
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
                            let (width, height) = measure(
                                text,
                                *font_size,
                                *line_height,
                                *font_weight,
                                max_width,
                            );
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
                    if node.scroll_x {
                        self.max_scroll_x(node)
                    } else {
                        0.0
                    },
                    if node.scroll_y {
                        self.max_scroll_y(node)
                    } else {
                        0.0
                    },
                ))
            })
            .collect();
        for (index, limit_x, limit_y) in limits {
            if let Some(node) = &mut self.nodes[index] {
                node.scroll_left = if node.scroll_x {
                    node.scroll_left.clamp(0.0, limit_x)
                } else {
                    0.0
                };
                node.scroll_top = if node.scroll_y {
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
        match self.hit(1, x, y) {
            Hit::Miss => None,
            Hit::Inside { handler } => handler,
        }
    }

    pub fn scroll_wheel(&mut self, x: f32, y: f32, dx: f32, dy: f32) -> bool {
        let Some(id) = self.scroll_target(1, x, y, dx, dy) else {
            return false;
        };
        let node = self.node(id).expect("scroll target");
        let limit_x = if node.scroll_x {
            self.max_scroll_x(node)
        } else {
            0.0
        };
        let limit_y = if node.scroll_y {
            self.max_scroll_y(node)
        } else {
            0.0
        };
        let node = self.node_mut(id).expect("scroll target");
        let next_x = (node.scroll_left + dx).clamp(0.0, limit_x);
        let next_y = (node.scroll_top + dy).clamp(0.0, limit_y);
        if (next_x - node.scroll_left).abs() < f32::EPSILON
            && (next_y - node.scroll_top).abs() < f32::EPSILON
        {
            return false;
        }
        node.scroll_left = next_x;
        node.scroll_top = next_y;
        self.paint_dirty.set(true);
        true
    }

    fn scroll_to(&mut self, id: u32, x: f32, y: f32) -> Result<(), String> {
        let (scroll_x, scroll_y) = {
            let node = self.node(id)?;
            (node.scroll_x, node.scroll_y)
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
            Prop::Color => self.node_mut(id)?.color = value.as_u32()?,
            Prop::BackgroundColor => self.node_mut(id)?.background = value.as_u32()?,
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
            Prop::ScrollX => self.node_mut(id)?.scroll_x = value.as_bool()?,
            Prop::ScrollY => self.node_mut(id)?.scroll_y = value.as_bool()?,
            Prop::Row => self.node_mut(id)?.row = value.as_bool()?,
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
            Prop::Bottom => {
                self.node_mut(id)?.inset[2] = value.optional_f32()?.unwrap_or(f32::NAN)
            }
            Prop::Left => self.node_mut(id)?.inset[3] = value.optional_f32()?.unwrap_or(f32::NAN),
            Prop::Src => {
                let Value::String(source) = value else {
                    return Err("src must be a string".into());
                };
                self.node_mut(id)?.src = source.clone();
            }
            Prop::Fit => self.node_mut(id)?.fit = image_fit(value)?,
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
            if node.kind != Kind::Text {
                return Ok(());
            }
            (
                node.taffy,
                node.text.clone(),
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
            scroll_x: desc.scroll_x,
            scroll_y: desc.scroll_y,
            flex_grow: desc.flex_grow,
            flex_shrink: desc.flex_shrink,
            width: desc.width,
            height: desc.height,
            padding: desc.padding,
            ..NodeStyle::default()
        });
        let taffy_id = if desc.kind == Kind::Text {
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
            scroll_x: desc.scroll_x,
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
        let scrolls = node.scroll_x || node.scroll_y;
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
                h,
                font_size,
                color,
                line_height: node.line_height,
                weight: node.font_weight,
                align: node.text_align,
                text,
            });
        } else if kind == Kind::Image && !node.src.is_empty() {
            out.push(PaintCmd::Image {
                x,
                y,
                w,
                h,
                radius,
                fit: node.fit,
                src: node.src.clone(),
            });
        }
        if scrolls {
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
        if scrolls {
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
        let scrolls = node.scroll_x || node.scroll_y;
        let handler = node.handler;
        let children = node.children.clone();
        if x < nx || y < ny || x >= nx + nw || y >= ny + nh {
            return Hit::Miss;
        }
        if scrolls {
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

    fn scroll_target(&self, id: u32, x: f32, y: f32, dx: f32, dy: f32) -> Option<u32> {
        let node = self.node(id).expect("scroll handle");
        if x < node.x || y < node.y || x >= node.x + node.w || y >= node.y + node.h {
            return None;
        }
        let children = node.children.clone();
        for child in children.iter().rev() {
            if let Some(found) = self.scroll_target(*child, x, y, dx, dy) {
                return Some(found);
            }
        }
        let node = self.node(id).expect("scroll handle");
        let can_x = dx != 0.0 && node.scroll_x && self.max_scroll_x(node) > 0.0;
        let can_y = dy != 0.0 && node.scroll_y && self.max_scroll_y(node) > 0.0;
        if can_x || can_y {
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

fn style_for(node: NodeStyle) -> Style {
    let mut style = Style::default();
    style.display = Display::Flex;
    style.flex_direction = if node.row {
        FlexDirection::Row
    } else {
        FlexDirection::Column
    };
    style.flex_grow = node.flex_grow;
    style.flex_shrink = node.flex_shrink;
    style.size = Size {
        width: dim(node.width),
        height: dim(node.height),
    };
    style.padding = Rect {
        left: length(node.padding[0]),
        right: length(node.padding[1]),
        top: length(node.padding[2]),
        bottom: length(node.padding[3]),
    };
    style.margin = Rect {
        left: length(node.margin[0]),
        right: length(node.margin[1]),
        top: length(node.margin[2]),
        bottom: length(node.margin[3]),
    };
    style.gap = Size {
        width: length(node.gap),
        height: length(node.gap),
    };
    style.align_items = match node.align_items {
        1 => AlignItems::START,
        2 => AlignItems::CENTER,
        3 => AlignItems::END,
        4 => AlignItems::BASELINE,
        _ => AlignItems::STRETCH,
    };
    style.justify_content = match node.justify_content {
        1 => JustifyContent::CENTER,
        2 => JustifyContent::END,
        3 => JustifyContent::SPACE_BETWEEN,
        4 => JustifyContent::SPACE_AROUND,
        5 => JustifyContent::SPACE_EVENLY,
        _ => JustifyContent::START,
    };
    style.position = if node.position == 1 {
        Position::Absolute
    } else {
        Position::Relative
    };
    style.inset = Rect {
        top: inset(node.inset[0]),
        right: inset(node.inset[1]),
        bottom: inset(node.inset[2]),
        left: inset(node.inset[3]),
    };
    if node.scroll_x || node.flex_grow > 0.0 {
        style.min_size.width = length(0.0);
    }
    if node.scroll_y || node.flex_grow > 0.0 {
        style.min_size.height = length(0.0);
    }
    if node.scroll_x {
        style.overflow.x = Overflow::Scroll;
    }
    if node.scroll_y {
        style.overflow.y = Overflow::Scroll;
    }
    if node.scroll_x || node.scroll_y {
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

fn dim(value: Option<f32>) -> Dimension {
    match value {
        Some(value) => length(value),
        None => auto(),
    }
}
