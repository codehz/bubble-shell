//! Accessibility semantics projected from the retained tree.
//!
//! Semantics are derived from node kind, role, label, state props, and the
//! gesture declarations targeting the node. The snapshot is a flat pre-order
//! list so platform bridges can build virtual accessibility trees without
//! walking Rust state. Bounds are viewport coordinates.

use serde::{Deserialize, Serialize};

/// Action bits carried by [`SemanticsNode::actions`].
pub const ACTION_FOCUS: u32 = 1;
pub const ACTION_ACTIVATE: u32 = 2;
pub const ACTION_SET_VALUE: u32 = 4;
/// Primary-axis page scroll: vertical when the node scrolls vertically.
pub const ACTION_SCROLL_FORWARD: u32 = 8;
pub const ACTION_SCROLL_BACKWARD: u32 = 16;
pub const ACTION_SCROLL_LEFT: u32 = 32;
pub const ACTION_SCROLL_RIGHT: u32 = 64;
pub const ACTION_SCROLL_UP: u32 = 128;
pub const ACTION_SCROLL_DOWN: u32 = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Generic,
    Text,
    Button,
    Link,
    Image,
    Textbox,
    Searchbox,
    Checkbox,
    Radio,
    Switch,
    Heading,
    List,
    ListItem,
    Group,
    Dialog,
    Tab,
    MenuItem,
    ProgressBar,
    Slider,
}

impl Role {
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "generic" | "none" => Self::Generic,
            "text" | "paragraph" => Self::Text,
            "button" => Self::Button,
            "link" => Self::Link,
            "image" | "img" => Self::Image,
            "textbox" | "textfield" => Self::Textbox,
            "searchbox" => Self::Searchbox,
            "checkbox" => Self::Checkbox,
            "radio" => Self::Radio,
            "switch" => Self::Switch,
            "heading" => Self::Heading,
            "list" => Self::List,
            "listitem" => Self::ListItem,
            "group" => Self::Group,
            "dialog" => Self::Dialog,
            "tab" => Self::Tab,
            "menuitem" => Self::MenuItem,
            "progressbar" => Self::ProgressBar,
            "slider" => Self::Slider,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Generic => "generic",
            Self::Text => "text",
            Self::Button => "button",
            Self::Link => "link",
            Self::Image => "image",
            Self::Textbox => "textbox",
            Self::Searchbox => "searchbox",
            Self::Checkbox => "checkbox",
            Self::Radio => "radio",
            Self::Switch => "switch",
            Self::Heading => "heading",
            Self::List => "list",
            Self::ListItem => "listitem",
            Self::Group => "group",
            Self::Dialog => "dialog",
            Self::Tab => "tab",
            Self::MenuItem => "menuitem",
            Self::ProgressBar => "progressbar",
            Self::Slider => "slider",
        }
    }

    /// Roles that take keyboard focus when the author marks them focusable.
    pub fn interactive(self) -> bool {
        matches!(
            self,
            Self::Button
                | Self::Link
                | Self::Checkbox
                | Self::Radio
                | Self::Switch
                | Self::Tab
                | Self::MenuItem
                | Self::Slider
                | Self::Textbox
                | Self::Searchbox
        )
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SemanticsNode {
    pub handle: u32,
    /// Nearest semantic ancestor, or `0` for the root.
    pub parent: u32,
    pub depth: u32,
    pub role: Role,
    pub label: String,
    pub value: String,
    pub bounds: [f32; 4],
    pub actions: u32,
    pub focusable: bool,
    pub focused: bool,
    pub disabled: bool,
    pub hidden: bool,
    pub checked: Option<bool>,
    pub selected: bool,
    pub expanded: Option<bool>,
    pub secure: bool,
    pub editable: bool,
    pub scrollable: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SemanticsSnapshot {
    pub version: u32,
    pub focused: u32,
    /// Deepest modal container currently trapping focus, or `0`.
    pub modal: u32,
    pub nodes: Vec<SemanticsNode>,
}

impl SemanticsSnapshot {
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("semantics snapshot serializes")
    }
}

impl Default for SemanticsSnapshot {
    fn default() -> Self {
        Self {
            version: 1,
            focused: 0,
            modal: 0,
            nodes: Vec::new(),
        }
    }
}
