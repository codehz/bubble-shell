//! Retained tree. Taffy owns flex; this type owns scroll offsets, hit testing, and paint order.

use crate::ir::{Kind, Template};
use crate::ops::{decode_ops, Op};
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
    nodes: Vec<Node>,
    taffy: TaffyTree<MeasureCtx>,
    viewport: Size<f32>,
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
}

impl Shell {
    pub fn mount(bytes: &[u8]) -> Result<Self, String> {
        let template = Template::decode(bytes)?;
        if template.nodes.is_empty() {
            return Err("template is empty".into());
        }
        let mut taffy: TaffyTree<MeasureCtx> = TaffyTree::new();
        let mut nodes = Vec::with_capacity(template.nodes.len());
        for desc in &template.nodes {
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
                taffy
                    .new_leaf_with_context(
                        style,
                        MeasureCtx {
                            text: desc.text.clone(),
                            font_size: desc.font_size,
                        },
                    )
                    .map_err(|err| err.to_string())?
            } else {
                taffy.new_leaf(style).map_err(|err| err.to_string())?
            };
            nodes.push(Node {
                parent: desc.parent,
                children: Vec::new(),
                kind: desc.kind,
                scroll_y: desc.scroll_y,
                background: desc.background,
                color: desc.color,
                font_size: desc.font_size,
                radius: desc.radius,
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
        }
        for index in 0..nodes.len() {
            let parent = nodes[index].parent;
            if parent != 0 {
                nodes[(parent - 1) as usize].children.push((index as u32) + 1);
            }
        }
        for node in &nodes {
            if node.children.is_empty() {
                continue;
            }
            let children: Vec<NodeId> = node
                .children
                .iter()
                .map(|id| nodes[(id - 1) as usize].taffy)
                .collect();
            taffy
                .set_children(node.taffy, &children)
                .map_err(|err| err.to_string())?;
        }
        Ok(Self {
            nodes,
            taffy,
            viewport: Size::ZERO,
        })
    }

    pub fn set_viewport(&mut self, width: f32, height: f32) {
        self.viewport = Size { width, height };
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
                    let node = self.node_mut(handle)?;
                    match key.as_str() {
                        "color" => node.color = rgba,
                        "backgroundColor" => node.background = rgba,
                        other => return Err(format!("unsupported prop {other}")),
                    }
                }
            }
        }
        Ok(seq)
    }

    pub fn layout(
        &mut self,
        measure: &mut dyn FnMut(&str, f32, Option<f32>) -> (f32, f32),
    ) -> Result<(), String> {
        if self.viewport.width <= 0.0 || self.viewport.height <= 0.0 {
            return Err("viewport is empty".into());
        }
        let root = self.nodes[0].taffy;
        let root_style = style_for(
            self.nodes[0].row,
            self.nodes[0].scroll_y,
            self.nodes[0].flex_grow,
            self.nodes[0].flex_shrink,
            Some(self.viewport.width),
            Some(self.viewport.height),
            self.nodes[0].padding,
        );
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
                    compute_leaf_layout(inputs, style, |_, _| 0.0, |known, available| {
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
                    })
                },
            )
            .map_err(|err| err.to_string())?;
        self.place(1, 0.0, 0.0)?;
        let limits: Vec<f32> = self
            .nodes
            .iter()
            .map(|node| if node.scroll_y { self.max_scroll(node) } else { 0.0 })
            .collect();
        for (node, limit) in self.nodes.iter_mut().zip(limits) {
            if node.scroll_y {
                node.scroll = node.scroll.clamp(0.0, limit);
            }
        }
        Ok(())
    }

    pub fn paint(&self) -> Vec<PaintCmd> {
        let mut out = Vec::new();
        if self.viewport.width > 0.0 {
            self.paint_node(1, &mut out);
        }
        out
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
        true
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

    fn node(&self, id: u32) -> Result<&Node, String> {
        self.nodes
            .get((id as usize).wrapping_sub(1))
            .ok_or_else(|| format!("bad handle {id}"))
    }

    fn node_mut(&mut self, id: u32) -> Result<&mut Node, String> {
        self.nodes
            .get_mut((id as usize).wrapping_sub(1))
            .ok_or_else(|| format!("bad handle {id}"))
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
        let node = &self.nodes[(id - 1) as usize];
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
        let scroll_y = node.scroll_y;
        let children = node.children.clone();
        let pads = self.pads(id);
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
    }

    fn pads(&self, id: u32) -> [f32; 4] {
        let node = &self.nodes[(id - 1) as usize];
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
        let node = &self.nodes[(id - 1) as usize];
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
                Hit::Inside { handler: Some(found) } => {
                    return Hit::Inside { handler: Some(found) };
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
        let node = &self.nodes[(id - 1) as usize];
        if x < node.x || y < node.y || x >= node.x + node.w || y >= node.y + node.h {
            return None;
        }
        let children = node.children.clone();
        for child in children.iter().rev() {
            if let Some(found) = self.scroll_target(*child, x, y) {
                return Some(found);
            }
        }
        let node = &self.nodes[(id - 1) as usize];
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
    style.flex_direction = if row { FlexDirection::Row } else { FlexDirection::Column };
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
