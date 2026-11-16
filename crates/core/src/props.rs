//! Canonical node property names.
//!
//! `Prop::parse` is the single authority for the property names `Shell` accepts.
//! Node creation sends only identity and text; every authored property is a
//! `SetProp` value and goes through the same parser whether it is static or
//! dynamic. `padding`, `margin`, and `inset` arrays are split into edge
//! properties by the JS runtime. The 4-number form is top, end, bottom, start.
//! `gap` is applied before `rowGap` / `columnGap`, and `overflow` before its
//! axes, independent of object key order. A dynamic shorthand shares one
//! binding with the authored axes and reapplies them after every shorthand
//! change. A later explicit `setProp` keeps call order until that binding runs
//! again.
//!
//! `VIEW_STYLE_PROPS` / `TEXT_STYLE_PROPS` are the paint-only whitelists a
//! `ViewStyle` / `TextStyle` declaration may patch. Layout properties stay on
//! the node.

use crate::ops::Value;
use crate::shell::Len;
use taffy::style::Overflow;

/// Paint properties a `ViewStyle` declaration may patch.
pub const VIEW_STYLE_PROPS: &[&str] = &[
    "backgroundColor",
    "radius",
    "opacity",
    "fit",
    "translateX",
    "translateY",
    "scale",
    "rotate",
];

/// Paint properties a `TextStyle` declaration may patch.
pub const TEXT_STYLE_PROPS: &[&str] = &["color", "textAlign"];

/// `0` left, `1` center, `2` right.
pub(crate) fn text_align(value: &Value) -> Result<u8, String> {
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

/// `0` clip, `1` ellipsis.
pub(crate) fn parse_text_overflow(value: &Value) -> Result<u8, String> {
    let Value::String(text) = value else {
        return Err("textOverflow must be a string".into());
    };
    match text.as_str() {
        "clip" => Ok(0),
        "ellipsis" => Ok(1),
        other => Err(format!("unsupported textOverflow {other}")),
    }
}

/// `0` contain, `1` cover, `2` fill.
pub(crate) fn image_fit(value: &Value) -> Result<u8, String> {
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Prop {
    Color,
    BackgroundColor,
    Radius,
    Opacity,
    FontSize,
    PaddingStart,
    PaddingEnd,
    PaddingTop,
    PaddingBottom,
    Width,
    Height,
    MinWidth,
    MinHeight,
    MaxWidth,
    MaxHeight,
    FlexBasis,
    FlexGrow,
    FlexShrink,
    ScrollX,
    ScrollY,
    Overflow,
    Display,
    Visibility,
    Row,
    LineHeight,
    TextAlign,
    TextOverflow,
    LineClamp,
    FontWeight,
    MarginStart,
    MarginEnd,
    MarginTop,
    MarginBottom,
    Gap,
    RowGap,
    ColumnGap,
    AlignItems,
    AlignSelf,
    AlignContent,
    JustifyContent,
    FlexWrap,
    Position,
    Top,
    End,
    Bottom,
    Start,
    AspectRatio,
    Src,
    Fit,
    ScrollChainX,
    ScrollChainY,
    Multiline,
    Secure,
    MaxLength,
    Keyboard,
    Autofill,
    Role,
    Label,
    TestID,
    Disabled,
    Hidden,
    Focusable,
    Modal,
    Checked,
    Selected,
    Expanded,
}

impl Prop {
    pub fn parse(key: &str) -> Option<Self> {
        Some(match key {
            "color" => Self::Color,
            "backgroundColor" => Self::BackgroundColor,
            "radius" => Self::Radius,
            "opacity" => Self::Opacity,
            "fontSize" => Self::FontSize,
            "paddingStart" => Self::PaddingStart,
            "paddingEnd" => Self::PaddingEnd,
            "paddingTop" => Self::PaddingTop,
            "paddingBottom" => Self::PaddingBottom,
            "width" => Self::Width,
            "height" => Self::Height,
            "minWidth" => Self::MinWidth,
            "minHeight" => Self::MinHeight,
            "maxWidth" => Self::MaxWidth,
            "maxHeight" => Self::MaxHeight,
            "flexBasis" => Self::FlexBasis,
            "flexGrow" => Self::FlexGrow,
            "flexShrink" => Self::FlexShrink,
            "overflowX" => Self::ScrollX,
            "overflowY" => Self::ScrollY,
            "overflow" => Self::Overflow,
            "display" => Self::Display,
            "visibility" => Self::Visibility,
            "flexDirection" => Self::Row,
            "lineHeight" => Self::LineHeight,
            "textAlign" => Self::TextAlign,
            "textOverflow" => Self::TextOverflow,
            "lineClamp" => Self::LineClamp,
            "fontWeight" => Self::FontWeight,
            "marginStart" => Self::MarginStart,
            "marginEnd" => Self::MarginEnd,
            "marginTop" => Self::MarginTop,
            "marginBottom" => Self::MarginBottom,
            "gap" => Self::Gap,
            "rowGap" => Self::RowGap,
            "columnGap" => Self::ColumnGap,
            "alignItems" => Self::AlignItems,
            "alignSelf" => Self::AlignSelf,
            "alignContent" => Self::AlignContent,
            "justifyContent" => Self::JustifyContent,
            "flexWrap" => Self::FlexWrap,
            "position" => Self::Position,
            "top" => Self::Top,
            "end" => Self::End,
            "bottom" => Self::Bottom,
            "start" => Self::Start,
            "aspectRatio" => Self::AspectRatio,
            "src" => Self::Src,
            "fit" => Self::Fit,
            "overscrollX" => Self::ScrollChainX,
            "overscrollY" => Self::ScrollChainY,
            "multiline" => Self::Multiline,
            "secure" => Self::Secure,
            "maxLength" => Self::MaxLength,
            "keyboard" => Self::Keyboard,
            "autofill" => Self::Autofill,
            "role" => Self::Role,
            "label" => Self::Label,
            "testID" => Self::TestID,
            "disabled" => Self::Disabled,
            "hidden" => Self::Hidden,
            "focusable" => Self::Focusable,
            "modal" => Self::Modal,
            "checked" => Self::Checked,
            "selected" => Self::Selected,
            "expanded" => Self::Expanded,
            _ => return None,
        })
    }
}

pub(crate) fn align_items(value: &Value) -> Result<u8, String> {
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

pub(crate) fn justify_content(value: &Value) -> Result<u8, String> {
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

pub(crate) fn position(value: &Value) -> Result<u8, String> {
    let Value::String(text) = value else {
        return Err("position must be a string".into());
    };
    match text.as_str() {
        "relative" => Ok(0),
        "absolute" => Ok(1),
        other => Err(format!("unsupported position {other}")),
    }
}

pub(crate) fn parse_overflow(value: &Value) -> Result<Overflow, String> {
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

pub(crate) fn parse_display_none(value: &Value) -> Result<bool, String> {
    let Value::String(text) = value else {
        return Err("display must be a string".into());
    };
    match text.as_str() {
        "flex" => Ok(false),
        "none" => Ok(true),
        other => Err(format!("unsupported display {other}")),
    }
}

pub(crate) fn parse_visibility_hidden(value: &Value) -> Result<bool, String> {
    let Value::String(text) = value else {
        return Err("visibility must be a string".into());
    };
    match text.as_str() {
        "visible" => Ok(false),
        "hidden" => Ok(true),
        other => Err(format!("unsupported visibility {other}")),
    }
}

pub(crate) fn flex_direction(value: &Value) -> Result<u8, String> {
    let Value::String(text) = value else {
        return Err("flexDirection must be a string".into());
    };
    match text.as_str() {
        "column" => Ok(0),
        "row" => Ok(1),
        "column-reverse" => Ok(2),
        "row-reverse" => Ok(3),
        other => Err(format!("unsupported flexDirection {other}")),
    }
}

pub(crate) fn align_self(value: &Value) -> Result<u8, String> {
    let Value::String(text) = value else {
        return Err("alignSelf must be a string".into());
    };
    if text == "auto" {
        return Ok(0);
    }
    Ok(align_items(&Value::String(text.clone()))? + 1)
}

pub(crate) fn flex_wrap(value: &Value) -> Result<u8, String> {
    let Value::String(text) = value else {
        return Err("flexWrap must be a string".into());
    };
    match text.as_str() {
        "nowrap" => Ok(0),
        "wrap" => Ok(1),
        "wrap-reverse" => Ok(2),
        other => Err(format!("unsupported flexWrap {other}")),
    }
}

pub(crate) fn parse_len(value: &Value, allow_auto: bool) -> Result<Len, String> {
    match value {
        Value::Null if allow_auto => Ok(Len::Auto),
        Value::Int(value) => Ok(Len::Pt(*value as f32)),
        Value::Float(value) => Ok(Len::Pt(*value as f32)),
        Value::String(text) if allow_auto && text == "auto" => Ok(Len::Auto),
        Value::String(text) => {
            let Some(number) = text.strip_suffix('%') else {
                return Err(format!("unsupported length {text}"));
            };
            let number: f32 = number
                .trim()
                .parse()
                .map_err(|_| format!("unsupported length {text}"))?;
            Ok(Len::Pct(number / 100.0))
        }
        _ => Err("unsupported length".into()),
    }
}
