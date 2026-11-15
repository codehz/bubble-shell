use std::cell::Ref;

use crate::color::Color;
use crate::editing::display_text;
use crate::ir::Kind;
use crate::layout::{wrapping_layout, LayoutRequest, LayoutSnapshot};
use taffy::prelude::*;
use taffy::style::Overflow;
use taffy::compute_leaf_layout;

use super::tree::style_for;
use super::{Len, Measure, NodeStyle, Shell};

/// The six paint values a frame draws for one node: its base value with the
/// matching state style patches applied. Layout never reads this.
#[derive(Clone, Copy)]
pub(super) struct EffectivePaintStyle {
    pub(super) background: Color,
    pub(super) color: Color,
    pub(super) opacity: f32,
    pub(super) radius: f32,
    pub(super) text_align: u8,
    pub(super) fit: u8,
    pub(super) translate_x: f32,
    pub(super) translate_y: f32,
    pub(super) scale: f32,
    pub(super) rotate: f32,
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
    /// Transform around a center point: translate (logical px), scale
    /// multiplier, rotate in radians. Applies to the node and its subtree
    /// until the matching `PopTransform`.
    PushTransform {
        cx: f32,
        cy: f32,
        translate_x: f32,
        translate_y: f32,
        scale: f32,
        rotate: f32,
    },
    PopTransform,
}

impl Shell {
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
            style.width = Len::Pt(self.viewport.width);
            style.height = Len::Pt(self.viewport.height);
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

    fn paint_node(&self, id: u32, out: &mut Vec<PaintCmd>) {
        let node = self.node(id).expect("paint handle");
        if !node.painted() {
            return;
        }
        let x = node.x;
        let y = node.y;
        let w = node.w;
        let h = node.h;
        let paint = self.effective_paint_style(id, node);
        let radius = paint.radius;
        let background = paint.background;
        let kind = node.kind;
        let text = node.text.clone();
        let secure = node.secure;
        let color = paint.color;
        let font_size = node.font_size;
        let opacity = paint.opacity.clamp(0.0, 1.0);
        let line_height = node.line_height;
        let weight = node.font_weight;
        let align = paint.text_align;
        let fit = paint.fit;
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
        if paint.translate_x != 0.0
            || paint.translate_y != 0.0
            || paint.scale != 1.0
            || paint.rotate != 0.0
        {
            out.push(PaintCmd::PushTransform {
                cx: x + w / 2.0,
                cy: y + h / 2.0,
                translate_x: paint.translate_x,
                translate_y: paint.translate_y,
                scale: paint.scale,
                rotate: paint.rotate,
            });
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
        if id == self.focused
            && kind != Kind::Field
            && self.is_focusable(id)
            && !self.focus_style_taken(id)
        {
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
        if paint.translate_x != 0.0
            || paint.translate_y != 0.0
            || paint.scale != 1.0
            || paint.rotate != 0.0
        {
            out.push(PaintCmd::PopTransform);
        }
        if opacity < 1.0 {
            out.push(PaintCmd::PopOpacity);
        }
    }

    pub(super) fn pads(&self, id: u32) -> [f32; 4] {
        let node = self.node(id).expect("pad handle");
        match self.taffy.layout(node.taffy) {
            Ok(layout) => [
                layout.padding.left,
                layout.padding.right,
                layout.padding.top,
                layout.padding.bottom,
            ],
            Err(_) => [0.0; 4],
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct Bounds {
    pub(super) x0: f32,
    pub(super) y0: f32,
    pub(super) x1: f32,
    pub(super) y1: f32,
}

impl Bounds {
    pub(super) fn contains(self, x: f32, y: f32) -> bool {
        x >= self.x0 && y >= self.y0 && x < self.x1 && y < self.y1
    }

    pub(super) fn intersect(self, other: Self) -> Self {
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
pub(super) fn axis_bounds(
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
        x0: if overflow_x == Overflow::Visible {
            0.0
        } else {
            x
        },
        y0: if overflow_y == Overflow::Visible {
            0.0
        } else {
            y
        },
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
    let bounds = axis_bounds(overflow_x, overflow_y, x, y, w, h, viewport_w, viewport_h);
    Some((
        bounds.x0,
        bounds.y0,
        (bounds.x1 - bounds.x0).max(0.0),
        (bounds.y1 - bounds.y0).max(0.0),
    ))
}
