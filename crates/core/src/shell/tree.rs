use crate::color::Color;
use crate::editing::{display_text, EditConfig, EditSession};
use crate::expr::ExprValue;
use crate::ir::{Kind, NodeDesc};
use crate::ops::Value;
use crate::props::{
    align_items, align_self, flex_direction, flex_wrap, image_fit, justify_content,
    parse_display_none, parse_len, parse_overflow, parse_visibility_hidden, position, text_align,
    Prop,
};
use crate::semantics::Role;
use crate::state_style::{slot_allowed, slot_of, slot_type, DeclExpr, SlotType};
use taffy::prelude::*;
use taffy::style::Overflow;
use taffy::NodeId;

use super::vars::parse_expr_value;
use super::{auto_box, zero_box, zero_len, Len, MeasureCtx, NodeStyle, Shell};

#[allow(dead_code)]
pub(super) struct Node {
    pub(super) parent: u32,
    pub(super) children: Vec<u32>,
    pub(super) kind: Kind,
    pub(super) overflow_x: Overflow,
    pub(super) overflow_y: Overflow,
    pub(super) background: Color,
    pub(super) color: Color,
    pub(super) font_size: f32,
    pub(super) radius: f32,
    pub(super) opacity: f32,
    pub(super) handler: u32,
    pub(super) width: Len,
    pub(super) height: Len,
    pub(super) min_width: Len,
    pub(super) min_height: Len,
    pub(super) max_width: Len,
    pub(super) max_height: Len,
    pub(super) flex_basis: Len,
    pub(super) flex_grow: f32,
    pub(super) flex_shrink: f32,
    pub(super) direction: u8,
    pub(super) padding: [Len; 4],
    pub(super) margin: [Len; 4],
    pub(super) gap_column: Len,
    pub(super) gap_row: Len,
    pub(super) align_items: u8,
    pub(super) align_self: u8,
    pub(super) justify_content: u8,
    pub(super) align_content: u8,
    pub(super) flex_wrap: u8,
    pub(super) position: u8,
    pub(super) inset: [Len; 4],
    pub(super) aspect_ratio: Option<f32>,
    pub(super) line_height: f32,
    pub(super) text_align: u8,
    pub(super) font_weight: u16,
    pub(super) src: String,
    pub(super) fit: u8,
    pub(super) text: String,
    pub(super) taffy: NodeId,
    pub(super) scroll_left: f32,
    pub(super) scroll_top: f32,
    pub(super) chain_x: bool,
    pub(super) chain_y: bool,
    pub(super) multiline: bool,
    pub(super) secure: bool,
    pub(super) max_length: u32,
    pub(super) keyboard: String,
    pub(super) autofill: String,
    pub(super) role: Option<Role>,
    pub(super) label: String,
    pub(super) test_id: String,
    pub(super) disabled: bool,
    pub(super) a11y_hidden: bool,
    /// `display: none`. The subtree is out of layout, paint, hit, focus, and semantics.
    pub(super) display_none: bool,
    /// `visibility: hidden`. Layout stays; the subtree cannot paint, hit, or take actions.
    pub(super) visibility_hidden: bool,
    pub(super) focusable: bool,
    pub(super) modal: bool,
    pub(super) checked: Option<bool>,
    pub(super) selected: bool,
    pub(super) expanded: Option<bool>,
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) w: f32,
    pub(super) h: f32,
}

impl Node {
    /// Single source of truth for a fresh node's defaults.
    pub(super) fn new_root(taffy_id: NodeId) -> Self {
        Self {
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
            width: Len::Auto,
            height: Len::Auto,
            min_width: Len::Auto,
            min_height: Len::Auto,
            max_width: Len::Auto,
            max_height: Len::Auto,
            flex_basis: Len::Auto,
            flex_grow: 0.0,
            flex_shrink: 0.0,
            direction: 0,
            padding: zero_box(),
            margin: zero_box(),
            gap_column: zero_len(),
            gap_row: zero_len(),
            align_items: 0,
            align_self: 0,
            justify_content: 0,
            align_content: 0,
            flex_wrap: 0,
            position: 0,
            inset: auto_box(),
            aspect_ratio: None,
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
            test_id: String::new(),
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
        }
    }

    /// Node from a descriptor: descriptor fields or the new_root defaults.
    fn from_desc(desc: NodeDesc, taffy_id: NodeId) -> Self {
        Self {
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
            flex_grow: desc.flex_grow,
            flex_shrink: desc.flex_shrink,
            direction: desc.direction,
            width: desc.width.map(Len::Pt).unwrap_or(Len::Auto),
            height: desc.height.map(Len::Pt).unwrap_or(Len::Auto),
            padding: desc.padding.map(Len::Pt),
            text: desc.text,
            taffy: taffy_id,
            ..Self::new_root(NodeId::new(0))
        }
    }

    pub(super) fn scrolls_x(&self) -> bool {
        self.overflow_x == Overflow::Scroll
    }

    pub(super) fn scrolls_y(&self) -> bool {
        self.overflow_y == Overflow::Scroll
    }

    pub(super) fn taffy_display(&self) -> Display {
        if self.display_none {
            Display::None
        } else {
            Display::Flex
        }
    }

    /// Painted and hit-tested only when both layout and visibility include it.
    pub(super) fn painted(&self) -> bool {
        !self.display_none && !self.visibility_hidden
    }
}

impl Shell {
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

    /// Apply one declaration value: `when` or a paint slot. Programs are
    /// validated and evaluated once here, so a type or range error fails the
    /// batch instead of painting a wrong value.
    pub(super) fn set_style_decl_prop(&mut self, decl: u32, key: &str, value: &Value) -> Result<(), String> {
        let slot = if key == "when" {
            None
        } else {
            let slot =
                slot_of(key).ok_or_else(|| format!("style declaration has no property {key}"))?;
            let component = self
                .state_styles
                .decl(decl)
                .ok_or_else(|| format!("unknown style declaration {decl}"))?
                .component;
            if !slot_allowed(component, slot) {
                return Err(format!("this style component has no property {key}"));
            }
            Some(slot)
        };
        let expr = match (slot, value) {
            (None, Value::Bool(constant)) => Some(DeclExpr::Static(ExprValue::Bool(*constant))),
            (None, Value::Program(program)) => {
                program
                    .result_type(&|id| self.vars.get(id).map(|var| var.value_type()))
                    .map_err(|error| format!("when: {error}"))?;
                Some(DeclExpr::Program(Box::new(program.clone())))
            }
            (Some(_), Value::Null) => None,
            (Some(_), Value::Program(program)) => {
                program
                    .result_type(&|id| self.vars.get(id).map(|var| var.value_type()))
                    .map_err(|error| format!("{key}: {error}"))?;
                Some(DeclExpr::Program(Box::new(program.clone())))
            }
            (Some(slot), static_value) => {
                let parsed = match slot_type(slot) {
                    SlotType::Color => ExprValue::Color(
                        static_value
                            .as_color()
                            .map_err(|error| format!("{key}: {error}"))?,
                    ),
                    SlotType::Enum if matches!(static_value, Value::String(_)) => {
                        // Enum names are static values too: map them to codes.
                        let code = if slot == crate::state_style::SLOT_TEXT_ALIGN {
                            crate::props::text_align(static_value)? as f32
                        } else {
                            crate::props::image_fit(static_value)? as f32
                        };
                        ExprValue::Num(code)
                    }
                    _ => parse_expr_value(static_value)
                        .ok_or_else(|| format!("{key} value must be a number, color, or bool"))?,
                };
                Some(DeclExpr::Static(parsed))
            }
            (None, other) => {
                return Err(format!("when must be a bool or a program, got {other:?}"))
            }
        };
        // Store the slot, recompute interests, and evaluate once.
        self.state_styles.set_slot(decl, slot, expr)?;
        self.state_styles.register_deps(decl, &self.vars);
        self.eval_decl(decl);
        self.paint_dirty.set(true);
        Ok(())
    }

    pub(super) fn set_prop(&mut self, id: u32, key: &str, value: &Value) -> Result<(), String> {
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
            Prop::PaddingStart => self.node_mut(id)?.padding[0] = parse_len(value, false)?,
            Prop::PaddingEnd => self.node_mut(id)?.padding[1] = parse_len(value, false)?,
            Prop::PaddingTop => self.node_mut(id)?.padding[2] = parse_len(value, false)?,
            Prop::PaddingBottom => self.node_mut(id)?.padding[3] = parse_len(value, false)?,
            Prop::Width => self.node_mut(id)?.width = parse_len(value, true)?,
            Prop::Height => self.node_mut(id)?.height = parse_len(value, true)?,
            Prop::MinWidth => self.node_mut(id)?.min_width = parse_len(value, true)?,
            Prop::MinHeight => self.node_mut(id)?.min_height = parse_len(value, true)?,
            Prop::MaxWidth => self.node_mut(id)?.max_width = parse_len(value, true)?,
            Prop::MaxHeight => self.node_mut(id)?.max_height = parse_len(value, true)?,
            Prop::FlexBasis => self.node_mut(id)?.flex_basis = parse_len(value, true)?,
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
            Prop::Row => self.node_mut(id)?.direction = flex_direction(value)?,
            Prop::LineHeight => {
                self.node_mut(id)?.line_height = value.as_f32()?.max(0.0);
                self.refresh_measure(id)?;
            }
            Prop::TextAlign => self.node_mut(id)?.text_align = text_align(value)?,
            Prop::FontWeight => {
                self.node_mut(id)?.font_weight = value.as_u32()?.clamp(1, 1000) as u16;
                self.refresh_measure(id)?;
            }
            Prop::MarginStart => self.node_mut(id)?.margin[0] = parse_len(value, false)?,
            Prop::MarginEnd => self.node_mut(id)?.margin[1] = parse_len(value, false)?,
            Prop::MarginTop => self.node_mut(id)?.margin[2] = parse_len(value, false)?,
            Prop::MarginBottom => self.node_mut(id)?.margin[3] = parse_len(value, false)?,
            Prop::Gap => {
                let gap = parse_len(value, false)?;
                let node = self.node_mut(id)?;
                node.gap_column = gap;
                node.gap_row = gap;
            }
            Prop::RowGap => self.node_mut(id)?.gap_row = parse_len(value, false)?,
            Prop::ColumnGap => self.node_mut(id)?.gap_column = parse_len(value, false)?,
            Prop::AlignItems => self.node_mut(id)?.align_items = align_items(value)?,
            Prop::AlignSelf => self.node_mut(id)?.align_self = align_self(value)?,
            Prop::AlignContent => self.node_mut(id)?.align_content = justify_content(value)?,
            Prop::JustifyContent => self.node_mut(id)?.justify_content = justify_content(value)?,
            Prop::FlexWrap => self.node_mut(id)?.flex_wrap = flex_wrap(value)?,
            Prop::Position => self.node_mut(id)?.position = position(value)?,
            Prop::Top => self.node_mut(id)?.inset[0] = parse_len(value, true)?,
            Prop::End => self.node_mut(id)?.inset[1] = parse_len(value, true)?,
            Prop::Bottom => self.node_mut(id)?.inset[2] = parse_len(value, true)?,
            Prop::Start => self.node_mut(id)?.inset[3] = parse_len(value, true)?,
            Prop::AspectRatio => {
                self.node_mut(id)?.aspect_ratio = match value {
                    Value::Null => None,
                    other => Some(other.as_f32()?),
                };
            }
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
            Prop::TestID => {
                let Value::String(text) = value else {
                    return Err("testID must be a string".into());
                };
                self.node_mut(id)?.test_id = text.clone();
            }
            Prop::Disabled => {
                self.node_mut(id)?.disabled = value.as_bool()?;
                self.sample_sources();
            }
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

    pub(super) fn refresh_measure(&mut self, id: u32) -> Result<(), String> {
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

    pub(super) fn node(&self, id: u32) -> Result<&Node, String> {
        self.nodes
            .get(id as usize)
            .and_then(Option::as_ref)
            .ok_or_else(|| format!("bad handle {id}"))
    }

    pub(super) fn node_mut(&mut self, id: u32) -> Result<&mut Node, String> {
        self.nodes
            .get_mut(id as usize)
            .and_then(Option::as_mut)
            .ok_or_else(|| format!("bad handle {id}"))
    }

    pub(super) fn insert_node(&mut self, desc: NodeDesc, parent: u32, index: u32) -> Result<(), String> {
        let handle = desc.id;
        if handle == 0 || self.retired.contains(&handle) || self.node(handle).is_ok() {
            return Err(format!("handle {handle} is not free"));
        }
        let parent_len = self.node(parent)?.children.len() as u32;
        if index > parent_len {
            return Err(format!("index {index} past {parent_len} children"));
        }
        let style = style_for(NodeStyle {
            direction: desc.direction,
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
            width: desc.width.map(Len::Pt).unwrap_or(Len::Auto),
            height: desc.height.map(Len::Pt).unwrap_or(Len::Auto),
            padding: desc.padding.map(Len::Pt),
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
        let mut node = Node::from_desc(desc.clone(), taffy_id);
        node.parent = parent;
        self.nodes[handle as usize] = Some(node);
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

    pub(super) fn dispose_node(&mut self, handle: u32) -> Result<(), String> {
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

    pub(super) fn move_node(&mut self, handle: u32, parent: u32, index: u32) -> Result<(), String> {
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

    pub(super) fn contains(&self, ancestor: u32, mut id: u32) -> bool {
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
        if self.visual.target == id {
            self.visual.target = 0;
            self.sample_sources();
        }
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
        self.state_styles.target_disposed(handle);
        // Variables sourced at the destroyed node retire; declarations that
        // read them stop matching.
        let retired_vars = self.vars.retire_target(handle);
        for var in retired_vars {
            let dependents = self.vars.dispose(var).unwrap_or_default();
            for decl in dependents {
                self.mark_decl_unmatched(decl);
            }
        }
        if self.visual.target == handle {
            self.visual.target = 0;
        }
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
}

pub(super) fn style_for(node: NodeStyle) -> Style {
    let mut style = Style {
        display: node.display,
        flex_direction: match node.direction {
            1 => FlexDirection::Row,
            2 => FlexDirection::ColumnReverse,
            3 => FlexDirection::RowReverse,
            _ => FlexDirection::Column,
        },
        flex_wrap: match node.flex_wrap {
            1 => FlexWrap::Wrap,
            2 => FlexWrap::WrapReverse,
            _ => FlexWrap::NoWrap,
        },
        flex_grow: node.flex_grow,
        flex_shrink: node.flex_shrink,
        flex_basis: dim_of(node.flex_basis),
        aspect_ratio: node.aspect_ratio,
        size: Size {
            width: dim_of(node.width),
            height: dim_of(node.height),
        },
        min_size: Size {
            width: min_dim(
                node.min_width,
                node.overflow_x == Overflow::Scroll || node.flex_grow > 0.0,
            ),
            height: min_dim(
                node.min_height,
                node.overflow_y == Overflow::Scroll || node.flex_grow > 0.0,
            ),
        },
        max_size: Size {
            width: lpa_of(node.max_width),
            height: lpa_of(node.max_height),
        },
        padding: Rect {
            left: lp_of(node.padding[0]),
            right: lp_of(node.padding[1]),
            top: lp_of(node.padding[2]),
            bottom: lp_of(node.padding[3]),
        },
        margin: Rect {
            left: lpa_of(node.margin[0]),
            right: lpa_of(node.margin[1]),
            top: lpa_of(node.margin[2]),
            bottom: lpa_of(node.margin[3]),
        },
        gap: Size {
            width: lp_of(node.gap_column),
            height: lp_of(node.gap_row),
        },
        align_items: map_align(node.align_items),
        align_self: match node.align_self {
            0 => None,
            other => Some(map_align(other - 1)),
        },
        align_content: map_content(node.align_content),
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
            top: lpa_of(node.inset[0]),
            right: lpa_of(node.inset[1]),
            bottom: lpa_of(node.inset[2]),
            left: lpa_of(node.inset[3]),
        },
        ..Style::default()
    };
    style.overflow.x = node.overflow_x;
    style.overflow.y = node.overflow_y;
    if node.overflow_x == Overflow::Scroll || node.overflow_y == Overflow::Scroll {
        style.scrollbar_width = 0.0;
    }
    style
}

fn map_align(value: u8) -> AlignItems {
    match value {
        1 => AlignItems::START,
        2 => AlignItems::CENTER,
        3 => AlignItems::END,
        4 => AlignItems::BASELINE,
        _ => AlignItems::STRETCH,
    }
}

fn map_content(value: u8) -> AlignContent {
    match value {
        1 => AlignContent::CENTER,
        2 => AlignContent::END,
        3 => AlignContent::SPACE_BETWEEN,
        4 => AlignContent::SPACE_AROUND,
        5 => AlignContent::SPACE_EVENLY,
        _ => AlignContent::START,
    }
}

fn dim_of(value: Len) -> Dimension {
    match value {
        Len::Auto => auto(),
        Len::Pt(value) => length(value),
        Len::Pct(value) => Dimension::percent(value),
    }
}

fn min_dim(value: Len, implicit_zero: bool) -> LengthPercentageAuto {
    match value {
        Len::Auto if implicit_zero => length(0.0),
        other => lpa_of(other),
    }
}

fn lp_of(value: Len) -> LengthPercentage {
    match value {
        Len::Pct(value) => LengthPercentage::percent(value),
        Len::Pt(value) => LengthPercentage::length(value),
        Len::Auto => LengthPercentage::length(0.0),
    }
}

fn lpa_of(value: Len) -> LengthPercentageAuto {
    match value {
        Len::Auto => auto(),
        Len::Pt(value) => length(value),
        Len::Pct(value) => LengthPercentageAuto::percent(value),
    }
}

fn overscroll_chains(value: &Value) -> Result<bool, String> {
    value
        .as_bool()
        .map_err(|_| "overscroll must be a bool".into())
}
