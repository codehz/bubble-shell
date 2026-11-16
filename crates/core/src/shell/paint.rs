use std::cell::Ref;

use crate::color::Color;
use crate::editing::display_text;
use crate::ir::Kind;
use crate::layout::{
    min_content_width, wrapped_lines, wrapping_layout, LayoutRequest, LayoutSnapshot,
};
use taffy::compute_leaf_layout;
use taffy::prelude::*;
use taffy::style::Overflow;

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
        /// Corner radius in logical px. `0` keeps a rectangular clip.
        /// Already clamped to half the shorter side of this rect.
        radius: f32,
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
    /// Scrollport edge fade in logical px. A side of `0` is not drawn.
    /// Hosts erase already painted pixels with destination-out.
    FadeEdges {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        top: f32,
        right: f32,
        bottom: f32,
        left: f32,
    },
    /// Offscreen group. Scroll-edge fade erases inside it so pixels drawn
    /// before `PushLayer` (the container background) stay in place.
    PushLayer,
    PopLayer,
}

/// Longest prefix of `text` whose width with a trailing `…` fits. The original
/// string is returned when it already fits. Empty when even `…` does not fit.
fn truncate_with_ellipsis(text: &str, width: f32, measure: &mut dyn FnMut(&str) -> f32) -> String {
    if !width.is_finite() || width <= 0.0 {
        return String::new();
    }
    if measure(text) <= width {
        return text.to_owned();
    }
    prefix_with_ellipsis(text, width, measure)
}

/// `text` shortened so `prefix + "…"` fits. Always includes the ellipsis when
/// the mark itself fits, including when `text` was already short enough.
fn prefix_with_ellipsis(text: &str, width: f32, measure: &mut dyn FnMut(&str) -> f32) -> String {
    if !width.is_finite() || width <= 0.0 || measure("…") > width {
        return String::new();
    }
    let ends: Vec<usize> = std::iter::once(0)
        .chain(text.char_indices().map(|(index, ch)| index + ch.len_utf8()))
        .collect();
    let mut buf = String::with_capacity(text.len() + "…".len());
    let mut lo = 0usize;
    let mut hi = ends.len() - 1;
    while lo < hi {
        let mid = (lo + hi + 1) / 2;
        buf.clear();
        buf.push_str(&text[..ends[mid]]);
        buf.push('…');
        if measure(&buf) <= width {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    buf.clear();
    buf.push_str(&text[..ends[lo]]);
    buf.push('…');
    buf
}

fn clamp_lines(
    text: &str,
    clamp: u32,
    width: f32,
    advance: &mut dyn FnMut(&str) -> f32,
) -> Option<String> {
    let lines = wrapped_lines(text, width, &mut *advance);
    let clamp = clamp as usize;
    if clamp == 0 || lines.len() <= clamp {
        return None;
    }
    let mut out = String::new();
    for (index, line) in lines.iter().take(clamp - 1).enumerate() {
        if index > 0 {
            out.push('\n');
        }
        out.push_str(line);
    }
    if clamp > 1 {
        out.push('\n');
    }
    out.push_str(&prefix_with_ellipsis(&lines[clamp - 1], width, advance));
    Some(out)
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
        })?;
        self.apply_text_ellipsis(measure)
    }

    /// Lay out, then take editing geometry from `host`. The host snapshot must
    /// match the field key; this is the path platform editors use.
    pub fn layout_with(
        &mut self,
        measure: Measure<'_>,
        host: &mut dyn FnMut(&LayoutRequest<'_>) -> Result<LayoutSnapshot, String>,
    ) -> Result<(), String> {
        self.compute_layout(measure)?;
        self.refresh_caret(host)?;
        self.apply_text_ellipsis(measure)
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
                            ctx.max_lines,
                        )
                    });
                    compute_leaf_layout(
                        inputs,
                        style,
                        |_, _| 0.0,
                        |known, available| {
                            let Some((text, font_size, line_height, font_weight, max_lines)) =
                                measured.as_ref()
                            else {
                                return Size::ZERO;
                            };
                            if let (Some(width), Some(height)) = (known.width, known.height) {
                                return Size { width, height };
                            }
                            // Min-content is the longest word. Max-content and an
                            // indefinite width are the unwrapped string. A definite
                            // width wraps, which is what keeps a paragraph inside
                            // a sized parent instead of overflowing it.
                            let limit = match available.width {
                                AvailableSpace::Definite(width) => Some(width),
                                AvailableSpace::MinContent => {
                                    let mut advance = |sample: &str| {
                                        measure(
                                            sample,
                                            *font_size,
                                            *line_height,
                                            *font_weight,
                                            None,
                                        )
                                        .0
                                    };
                                    let min_width = min_content_width(text, &mut advance);
                                    (min_width > 0.0).then_some(min_width)
                                }
                                AvailableSpace::MaxContent => known
                                    .width
                                    .filter(|width| width.is_finite() && *width > 0.0),
                            };
                            let (width, mut height) =
                                measure(text, *font_size, *line_height, *font_weight, limit);
                            if *max_lines > 0 {
                                if let Some(limit) =
                                    limit.filter(|limit| limit.is_finite() && *limit > 0.0)
                                {
                                    let line_count = {
                                        let mut advance = |sample: &str| {
                                            measure(
                                                sample,
                                                *font_size,
                                                *line_height,
                                                *font_weight,
                                                None,
                                            )
                                            .0
                                        };
                                        wrapped_lines(text, limit, &mut advance)
                                            .len()
                                            .min(*max_lines as usize)
                                    };
                                    // `line_height` 0 is the host's natural line box.
                                    let line = if *line_height > 0.0 {
                                        *line_height
                                    } else {
                                        measure("x", *font_size, *line_height, *font_weight, None).1
                                    };
                                    if line_count > 0 && line > 0.0 {
                                        height = line_count as f32 * line;
                                    }
                                }
                            }
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
        self.sync_field_content(measure)?;
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
                self.paint_drag_ghost(&mut out);
            }
            *self.paint_cache.borrow_mut() = out;
            self.paint_dirty.set(false);
        }
        self.paint_cache.borrow()
    }

    fn paint_drag_ghost(&self, out: &mut Vec<PaintCmd>) {
        let Some((gx, gy)) = self.drag_ghost_pos else {
            return;
        };
        if self.drag_ghost_text.is_empty() {
            return;
        }
        let Ok(node) = self.node(self.focused) else {
            return;
        };
        let font_size = node.font_size;
        let line_height = if node.line_height > 0.0 {
            node.line_height
        } else {
            font_size * 1.2
        };
        let weight = node.font_weight;
        let color = self.effective_paint_style(self.focused, node).color;
        // Light text needs a dark plate. Faint alpha has no reliable luminance.
        const LUMINANCE_SPLIT: f32 = 0.5;
        const FAINT_ALPHA: f32 = 0.1;
        let bg_color = if color.a <= FAINT_ALPHA || color.relative_luminance() < LUMINANCE_SPLIT {
            Color::new(0.95, 0.95, 0.95, 1.0)
        } else {
            Color::new(0.1, 0.1, 0.1, 1.0)
        };
        let text_w = self.drag_ghost_size.0;
        let text_h = if self.drag_ghost_size.1 > 0.0 {
            self.drag_ghost_size.1
        } else {
            line_height
        };
        let pad_x = 8.0;
        let pad_y = 4.0;
        let box_w = text_w + pad_x * 2.0;
        let box_h = text_h + pad_y * 2.0;
        let x = gx - box_w * 0.5;
        let y = gy - box_h * 0.5;
        out.push(PaintCmd::PushOpacity { opacity: 0.65 });
        out.push(PaintCmd::PushTransform {
            cx: gx,
            cy: gy,
            translate_x: 0.0,
            translate_y: 0.0,
            scale: 0.85,
            rotate: 0.0,
        });
        out.push(PaintCmd::Fill {
            x,
            y,
            w: box_w,
            h: box_h,
            radius: 4.0,
            color: bg_color,
        });
        out.push(PaintCmd::Text {
            x: x + pad_x,
            y: y + pad_y,
            w: text_w,
            h: text_h,
            font_size,
            color,
            line_height,
            weight,
            align: 0,
            text: self.drag_ghost_text.clone(),
        });
        out.push(PaintCmd::PopTransform);
        out.push(PaintCmd::PopOpacity);
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
        let multiline = node.multiline;
        let scroll_left = node.scroll_left;
        let scroll_top = node.scroll_top;
        let fade_edges = node.fade_edges;
        let scrolls_x = node.scrolls_x();
        let scrolls_y = node.scrolls_y();
        let content_w = node.content_w;
        let content_h = node.content_h;
        let color = paint.color;
        let font_size = node.font_size;
        let opacity = paint.opacity.clamp(0.0, 1.0);
        let line_height = node.line_height;
        let weight = node.font_weight;
        let align = paint.text_align;
        let ellipsis_text = node.ellipsis_text.clone();
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
            radius,
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
        if let Some((cx, cy, cw, ch, cr)) = clip {
            out.push(PaintCmd::PushClip {
                x: cx,
                y: cy,
                w: cw,
                h: ch,
                radius: cr,
            });
        }
        let fade = content_fade(
            self,
            id,
            fade_edges,
            scrolls_x,
            scrolls_y,
            scroll_left,
            scroll_top,
        );
        if fade.is_some() {
            out.push(PaintCmd::PushLayer);
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
        let shown = if let Some(text) = ellipsis_text {
            text
        } else if kind == Kind::Field {
            display_text(&text, secure)
        } else {
            text
        };
        if matches!(kind, Kind::Text | Kind::Field) && !shown.is_empty() {
            let (text_x, text_y, text_w, text_h) = if kind == Kind::Field {
                let viewport_w = (w - pads[0] - pads[1]).max(0.0);
                let viewport_h = (h - pads[2] - pads[3]).max(0.0);
                let (layout_w, layout_h) = if multiline {
                    (viewport_w, content_h.max(viewport_h))
                } else {
                    (content_w.max(viewport_w), viewport_h.max(content_h))
                };
                (
                    x + pads[0] - scroll_left,
                    y + pads[2] - scroll_top,
                    layout_w,
                    layout_h,
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
        if kind == Kind::Field && id == self.drop_caret_field {
            if let Some((dx, dy, dw, dh)) = self.drop_caret_box {
                if self
                    .field_pointer
                    .as_ref()
                    .is_some_and(|state| state.content_drop_valid())
                {
                    out.push(PaintCmd::Fill {
                        x: dx,
                        y: dy,
                        w: dw,
                        h: dh,
                        radius: 0.0,
                        color: Color::new(0.0, 0.8, 0.4, 0.85),
                    });
                }
            }
        }
        for index in 0..child_count {
            let child = self.node(id).expect("paint child").children[index];
            self.paint_node(child, out);
        }
        if let Some((top, right, bottom, left)) = fade {
            out.push(PaintCmd::FadeEdges {
                x,
                y,
                w,
                h,
                top,
                right,
                bottom,
                left,
            });
            out.push(PaintCmd::PopLayer);
        }
        self.paint_scrollbars(
            id,
            x,
            y,
            w,
            h,
            scroll_left,
            scroll_top,
            scrolls_x,
            scrolls_y,
            out,
        );
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

    fn paint_scrollbars(
        &self,
        id: u32,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        scroll_left: f32,
        scroll_top: f32,
        scrolls_x: bool,
        scrolls_y: bool,
        out: &mut Vec<PaintCmd>,
    ) {
        if !scrolls_x && !scrolls_y {
            return;
        }
        let count = self.scrollbars.ids(id).map(|ids| ids.len()).unwrap_or(0);
        if count == 0 {
            return;
        }
        let max_x = if scrolls_x {
            self.max_scroll_x(self.node(id).expect("scrollbar parent"))
        } else {
            0.0
        };
        let max_y = if scrolls_y {
            self.max_scroll_y(self.node(id).expect("scrollbar parent"))
        } else {
            0.0
        };
        for index in 0..count {
            let bar_id = self.scrollbars.ids(id).expect("scrollbar ids")[index];
            let Some((vertical, auto_hide, thickness, margin, min_length, color, opacity, radius)) =
                self.scrollbars.scrollbar(bar_id).map(|bar| {
                    (
                        bar.axis == 0,
                        bar.auto_hide,
                        bar.width,
                        bar.margin,
                        bar.min_length,
                        bar.color,
                        bar.opacity,
                        bar.radius,
                    )
                })
            else {
                continue;
            };
            let scrollable = if vertical {
                scrolls_y && max_y > 0.0
            } else {
                scrolls_x && max_x > 0.0
            };
            if !scrollable || !self.scrollbar_visible(auto_hide, id) {
                continue;
            }
            let Some(thumb) = super::scrollbar_thumb(
                vertical,
                x,
                y,
                w,
                h,
                if vertical { scroll_top } else { scroll_left },
                if vertical { max_y } else { max_x },
                thickness,
                margin,
                min_length,
            ) else {
                continue;
            };
            let radius = if radius < 0.0 {
                thickness / 2.0
            } else {
                radius
            };
            if opacity < 1.0 {
                out.push(PaintCmd::PushOpacity { opacity });
            }
            out.push(PaintCmd::Fill {
                x: thumb.x,
                y: thumb.y,
                w: thumb.w,
                h: thumb.h,
                radius,
                color,
            });
            if opacity < 1.0 {
                out.push(PaintCmd::PopOpacity);
            }
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
    /// Text extent used as the field's scrollable overflow. Taffy only sees a leaf.
    fn sync_field_content(&mut self, measure: Measure<'_>) -> Result<(), String> {
        let ids: Vec<u32> = self
            .nodes
            .iter()
            .enumerate()
            .filter_map(|(index, node)| {
                node.as_ref()
                    .filter(|node| node.kind == Kind::Field)
                    .map(|_| index as u32)
            })
            .collect();
        for id in ids {
            let (text, font_size, line_height, font_weight, multiline, viewport_w) = {
                let node = self.node(id)?;
                let pads = self.pads(id);
                (
                    display_text(&node.text, node.secure),
                    node.font_size,
                    node.line_height,
                    node.font_weight,
                    node.multiline,
                    (node.w - pads[0] - pads[1]).max(0.0),
                )
            };
            let max_width = if multiline { Some(viewport_w) } else { None };
            let (width, height) = measure(&text, font_size, line_height, font_weight, max_width);
            let node = self.node_mut(id)?;
            node.content_w = width;
            node.content_h = height;
        }
        Ok(())
    }

    fn apply_text_ellipsis(&mut self, measure: Measure<'_>) -> Result<(), String> {
        let ids: Vec<u32> = self
            .nodes
            .iter()
            .enumerate()
            .filter_map(|(index, node)| {
                node.as_ref()
                    .filter(|node| matches!(node.kind, Kind::Text | Kind::Field))
                    .map(|_| index as u32)
            })
            .collect();
        for id in ids {
            let next = self.ellipsis_text_for(id, measure);
            self.node_mut(id)?.ellipsis_text = next;
        }
        self.paint_dirty.set(true);
        Ok(())
    }

    fn ellipsis_text_for(&self, id: u32, measure: Measure<'_>) -> Option<String> {
        let node = self.node(id).ok()?;
        if node.kind != Kind::Text || node.text_overflow != 1 {
            return None;
        }
        let shown = node.text.clone();
        if shown.is_empty() {
            return None;
        }
        let width = node.w.max(0.0);
        let font_size = node.font_size;
        let line_height = node.line_height;
        let weight = node.font_weight;
        let clamp = node.line_clamp;
        let mut advance = |text: &str| measure(text, font_size, line_height, weight, None).0;
        if clamp >= 2 {
            clamp_lines(&shown, clamp, width, &mut advance)
        } else {
            let truncated = truncate_with_ellipsis(&shown, width, &mut advance);
            (truncated != shown).then_some(truncated)
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct Bounds {
    pub(super) x0: f32,
    pub(super) y0: f32,
    pub(super) x1: f32,
    pub(super) y1: f32,
    /// Corner radius. `0` is a rectangle. Kept only while this rect is
    /// exactly one source clip; see [`Bounds::intersect`].
    pub(super) radius: f32,
}

impl Bounds {
    pub(super) fn contains(self, x: f32, y: f32) -> bool {
        if self.radius > 0.0 {
            super::hit_scroll::contains_rounded(
                x,
                y,
                self.x0,
                self.y0,
                self.x1 - self.x0,
                self.y1 - self.y0,
                self.radius,
            )
        } else {
            x >= self.x0 && y >= self.y0 && x < self.x1 && y < self.y1
        }
    }

    /// Axis-aligned intersection. A rounded corner belongs to the clip whose
    /// rect survives unchanged: the viewport's square clip must not wipe a
    /// smaller rounded box, and a partial overlap is not itself a rounded
    /// rect (dropping the radius over-accepts rather than rounding the wrong
    /// box). When both rects are the same, the larger radius is the tighter cut.
    pub(super) fn intersect(self, other: Self) -> Self {
        let x0 = self.x0.max(other.x0);
        let y0 = self.y0.max(other.y0);
        let x1 = self.x1.min(other.x1);
        let y1 = self.y1.min(other.y1);
        let same_self = x0 == self.x0 && y0 == self.y0 && x1 == self.x1 && y1 == self.y1;
        let same_other = x0 == other.x0 && y0 == other.y0 && x1 == other.x1 && y1 == other.y1;
        let radius = if same_self && same_other {
            self.radius.max(other.radius)
        } else if same_self {
            self.radius
        } else if same_other {
            other.radius
        } else {
            0.0
        };
        Self {
            x0,
            y0,
            x1,
            y1,
            radius,
        }
    }
}

/// Clip rect for one node. A visible axis uses the viewport; a hidden or
/// scroll axis uses the outer box, which is the padding box when there is no border.
/// `radius` is the node's corner radius. It is clamped to half the shorter
/// side and ignored when it is not positive.
pub(super) fn axis_bounds(
    overflow_x: Overflow,
    overflow_y: Overflow,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    viewport_w: f32,
    viewport_h: f32,
    radius: f32,
) -> Bounds {
    let x0 = if overflow_x == Overflow::Visible {
        0.0
    } else {
        x
    };
    let y0 = if overflow_y == Overflow::Visible {
        0.0
    } else {
        y
    };
    let x1 = if overflow_x == Overflow::Visible {
        viewport_w
    } else {
        x + w
    };
    let y1 = if overflow_y == Overflow::Visible {
        viewport_h
    } else {
        y + h
    };
    let rw = (x1 - x0).max(0.0);
    let rh = (y1 - y0).max(0.0);
    let radius = if radius > 0.0 && rw > 0.0 && rh > 0.0 {
        radius.min(rw.min(rh) * 0.5)
    } else {
        0.0
    };
    Bounds {
        x0,
        y0,
        x1,
        y1,
        radius,
    }
}

/// Edge lengths for a scroll fade, or `None` when nothing would be drawn.
/// Background stays outside the content layer; only these insets are erased.
fn content_fade(
    shell: &Shell,
    id: u32,
    fade_edges: f32,
    scrolls_x: bool,
    scrolls_y: bool,
    scroll_left: f32,
    scroll_top: f32,
) -> Option<(f32, f32, f32, f32)> {
    if fade_edges <= 0.0 || (!scrolls_x && !scrolls_y) {
        return None;
    }
    let node = shell.node(id).expect("paint handle");
    let mut top = 0.0;
    let mut right = 0.0;
    let mut bottom = 0.0;
    let mut left = 0.0;
    if scrolls_x {
        let max_scroll = shell.max_scroll_x(node);
        if max_scroll > 0.0 {
            left = fade_edges.min(scroll_left).max(0.0);
            right = fade_edges.min((max_scroll - scroll_left).max(0.0));
        }
    }
    if scrolls_y {
        let max_scroll = shell.max_scroll_y(node);
        if max_scroll > 0.0 {
            top = fade_edges.min(scroll_top).max(0.0);
            bottom = fade_edges.min((max_scroll - scroll_top).max(0.0));
        }
    }
    if top > 0.0 || right > 0.0 || bottom > 0.0 || left > 0.0 {
        Some((top, right, bottom, left))
    } else {
        None
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
    radius: f32,
) -> Option<(f32, f32, f32, f32, f32)> {
    if overflow_x == Overflow::Visible && overflow_y == Overflow::Visible {
        return None;
    }
    let bounds = axis_bounds(
        overflow_x, overflow_y, x, y, w, h, viewport_w, viewport_h, radius,
    );
    Some((
        bounds.x0,
        bounds.y0,
        (bounds.x1 - bounds.x0).max(0.0),
        (bounds.y1 - bounds.y0).max(0.0),
        bounds.radius,
    ))
}
