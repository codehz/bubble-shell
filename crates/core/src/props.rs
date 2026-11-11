//! Canonical node property names.
//!
//! `SPAWN_PROPS` is the set a `BNOD` descriptor may carry; the JS runtime
//! fetches it from the host so creation-time validation cannot drift from the
//! Rust decoder. `SET_PROPS` is the set `Shell::set_prop` accepts at runtime.
//! `padding` is creation-only in the core: the runtime splits a dynamic
//! padding array into the four edge properties.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Prop {
    Color,
    BackgroundColor,
    Radius,
    Opacity,
    FontSize,
    PaddingLeft,
    PaddingRight,
    PaddingTop,
    PaddingBottom,
    Width,
    Height,
    FlexGrow,
    FlexShrink,
    ScrollY,
    Row,
}

impl Prop {
    pub fn parse(key: &str) -> Option<Self> {
        Some(match key {
            "color" => Self::Color,
            "backgroundColor" => Self::BackgroundColor,
            "radius" => Self::Radius,
            "opacity" => Self::Opacity,
            "fontSize" => Self::FontSize,
            "paddingLeft" => Self::PaddingLeft,
            "paddingRight" => Self::PaddingRight,
            "paddingTop" => Self::PaddingTop,
            "paddingBottom" => Self::PaddingBottom,
            "width" => Self::Width,
            "height" => Self::Height,
            "flexGrow" => Self::FlexGrow,
            "flexShrink" => Self::FlexShrink,
            "scrollY" => Self::ScrollY,
            "row" => Self::Row,
            _ => return None,
        })
    }
}

pub const SPAWN_PROPS: &[&str] = &[
    "scrollY",
    "background",
    "color",
    "fontSize",
    "radius",
    "opacity",
    "width",
    "height",
    "flexGrow",
    "flexShrink",
    "row",
    "padding",
    "onPress",
];

pub const SET_PROPS: &[&str] = &[
    "color",
    "backgroundColor",
    "radius",
    "opacity",
    "fontSize",
    "paddingLeft",
    "paddingRight",
    "paddingTop",
    "paddingBottom",
    "width",
    "height",
    "flexGrow",
    "flexShrink",
    "scrollY",
    "row",
];
