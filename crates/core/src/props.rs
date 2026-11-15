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
