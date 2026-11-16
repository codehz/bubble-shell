//! Node descriptors.
//!
//! `NodeDesc` is the in-process form used by `Shell::spawn` and by Rust callers
//! that create nodes directly. It never appears on the wire: `Spawn` carries
//! identity and text, and every authored property arrives as `SetProp`.
//! `Turn::spawn` lowers a descriptor into that op sequence.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    View = 1,
    Text = 2,
    Image = 3,
    Field = 4,
    ScrollBar = 5,
}

impl Kind {
    pub fn from_u8(value: u8) -> Result<Self, String> {
        match value {
            1 => Ok(Kind::View),
            2 => Ok(Kind::Text),
            3 => Ok(Kind::Image),
            4 => Ok(Kind::Field),
            5 => Ok(Kind::ScrollBar),
            other => Err(format!("bad node kind {other}")),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct NodeDesc {
    pub id: u32,
    pub parent: u32,
    pub kind: Kind,
    /// Vertical scrolling for direct Rust callers. Authored values use
    /// `overflowY`.
    pub scroll_y: bool,
    /// Horizontal scrolling for direct Rust callers. Authored values use
    /// `overflowX`.
    pub scroll_x: bool,
    /// `0xAARRGGBB`.
    pub background: u32,
    /// `0xAARRGGBB`.
    pub color: u32,
    pub font_size: f32,
    pub radius: f32,
    pub opacity: f32,
    pub handler: u32,
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub padding: [f32; 4],
    pub flex_grow: f32,
    pub flex_shrink: f32,
    /// 0 column, 1 row, 2 column-reverse, 3 row-reverse.
    pub direction: u8,
    pub text: String,
    /// Scroll-edge fade in logical px. `0` disables it.
    pub fade_edges: f32,
}

impl NodeDesc {
    pub fn view(id: u32, parent: u32) -> Self {
        Self {
            id,
            parent,
            kind: Kind::View,
            scroll_y: false,
            scroll_x: false,
            background: 0,
            color: 0xffff_ffff,
            font_size: 16.0,
            radius: 0.0,
            opacity: 1.0,
            handler: 0,
            width: None,
            height: None,
            padding: [0.0; 4],
            flex_grow: 0.0,
            flex_shrink: 0.0,
            direction: 0,
            text: String::new(),
            fade_edges: 0.0,
        }
    }

    pub fn text(id: u32, parent: u32, text: impl Into<String>) -> Self {
        let mut node = Self::view(id, parent);
        node.kind = Kind::Text;
        node.text = text.into();
        node
    }
}
