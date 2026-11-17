//! Props checks for built-in JSX tags.
//!
//! Specs follow `bubble_shell_dts` and the values `Prop::parse` / the JS
//! runtime actually accept. Colors are CSS strings or `0xAARRGGBB`. Lengths
//! are numbers, percent strings, or `"auto"`. Expressions that are not
//! literals stay unchecked.

use std::collections::HashMap;
use std::sync::LazyLock;

use oxc::ast::ast::*;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum PropType {
    Unknown,
    Number,
    String,
    Boolean,
    /// CSS color string or numeric `0xAARRGGBB`.
    Color,
    /// Number, percent string, or `"auto"`.
    Len,
    Function,
    Array(Box<PropType>),
    /// String literal, kept so enum props can reject unknown spellings.
    StrLit(String),
    Union(Vec<&'static str>),
}

impl PropType {
    pub(crate) fn display(&self) -> String {
        match self {
            PropType::Unknown => "unknown".into(),
            PropType::Number => "number".into(),
            PropType::String | PropType::StrLit(_) => "string".into(),
            PropType::Boolean => "boolean".into(),
            PropType::Color => "color".into(),
            PropType::Len => "length".into(),
            PropType::Function => "function".into(),
            PropType::Array(inner) => format!("{}[]", inner.display()),
            PropType::Union(variants) => variants.join(" | "),
        }
    }
}

pub(crate) struct PrimSpec {
    pub(crate) allowed: HashMap<&'static str, PropType>,
    pub(crate) required: &'static [&'static str],
}

pub(crate) fn prim_spec(tag: &str) -> Option<&'static PrimSpec> {
    static VIEW: LazyLock<PrimSpec> = LazyLock::new(|| PrimSpec {
        allowed: view_props(),
        required: &[],
    });
    static TEXT: LazyLock<PrimSpec> = LazyLock::new(|| PrimSpec {
        allowed: text_props(),
        required: &[],
    });
    static IMAGE: LazyLock<PrimSpec> = LazyLock::new(|| PrimSpec {
        allowed: HashMap::from([
            ("src", PropType::String),
            ("fit", PropType::Union(vec!["contain", "cover", "fill"])),
            ("testID", PropType::String),
            ("width", PropType::Len),
            ("height", PropType::Len),
            ("label", PropType::String),
            ("ref", PropType::Unknown),
        ]),
        required: &["src"],
    });
    static TEXT_FIELD: LazyLock<PrimSpec> = LazyLock::new(|| PrimSpec {
        allowed: text_field_props(),
        required: &[],
    });
    static SCROLL_BAR: LazyLock<PrimSpec> = LazyLock::new(|| PrimSpec {
        allowed: HashMap::from([
            ("axis", PropType::Union(vec!["vertical", "horizontal"])),
            ("scrollbarWidth", PropType::Number),
            ("scrollbarMargin", PropType::Number),
            ("scrollbarColor", PropType::Color),
            ("scrollbarRadius", PropType::Number),
            ("scrollbarOpacity", PropType::Number),
            ("scrollbarMinLength", PropType::Number),
            ("autoHide", PropType::Boolean),
            ("interactive", PropType::Boolean),
            ("ref", PropType::Unknown),
        ]),
        required: &[],
    });

    match tag {
        "View" => Some(&VIEW),
        "Text" => Some(&TEXT),
        "Image" => Some(&IMAGE),
        "TextField" => Some(&TEXT_FIELD),
        "ScrollBar" => Some(&SCROLL_BAR),
        _ => None,
    }
}

fn edges() -> PropType {
    PropType::Union(vec!["number", "number[]"])
}

fn overflow() -> PropType {
    PropType::Union(vec!["visible", "hidden", "scroll"])
}

fn align() -> PropType {
    PropType::Union(vec![
        "flex-start",
        "flex-end",
        "center",
        "stretch",
        "start",
        "end",
        "baseline",
    ])
}

fn view_props() -> HashMap<&'static str, PropType> {
    HashMap::from([
        ("ref", PropType::Unknown),
        ("backgroundColor", PropType::Color),
        ("flexGrow", PropType::Number),
        ("flexShrink", PropType::Number),
        ("flexBasis", PropType::Len),
        (
            "flexDirection",
            PropType::Union(vec!["row", "column", "row-reverse", "column-reverse"]),
        ),
        (
            "flexWrap",
            PropType::Union(vec!["wrap", "nowrap", "wrap-reverse"]),
        ),
        ("alignItems", align()),
        (
            "alignSelf",
            PropType::Union(vec![
                "auto",
                "flex-start",
                "flex-end",
                "center",
                "stretch",
                "start",
                "end",
                "baseline",
            ]),
        ),
        ("alignContent", align()),
        (
            "justifyContent",
            PropType::Union(vec![
                "flex-start",
                "flex-end",
                "center",
                "space-between",
                "space-around",
                "space-evenly",
                "start",
                "end",
            ]),
        ),
        ("padding", edges()),
        ("margin", edges()),
        ("inset", edges()),
        ("gap", PropType::Number),
        ("rowGap", PropType::Number),
        ("columnGap", PropType::Number),
        ("width", PropType::Len),
        ("height", PropType::Len),
        ("minWidth", PropType::Len),
        ("minHeight", PropType::Len),
        ("maxWidth", PropType::Len),
        ("maxHeight", PropType::Len),
        ("aspectRatio", PropType::Number),
        ("position", PropType::Union(vec!["relative", "absolute"])),
        ("top", PropType::Len),
        ("bottom", PropType::Len),
        ("left", PropType::Len),
        ("right", PropType::Len),
        ("start", PropType::Len),
        ("end", PropType::Len),
        ("radius", PropType::Number),
        ("opacity", PropType::Number),
        ("overflow", overflow()),
        ("overflowX", overflow()),
        ("overflowY", overflow()),
        ("overscrollX", PropType::Len),
        ("overscrollY", PropType::Len),
        ("display", PropType::Union(vec!["flex", "none"])),
        ("visibility", PropType::Union(vec!["visible", "hidden"])),
        ("fadeEdges", PropType::Number),
        ("testID", PropType::String),
        (
            "role",
            PropType::Union(vec![
                "generic",
                "none",
                "text",
                "paragraph",
                "button",
                "link",
                "image",
                "img",
                "textbox",
                "textfield",
                "searchbox",
                "checkbox",
                "radio",
                "switch",
                "heading",
                "list",
                "listitem",
                "group",
                "dialog",
                "tab",
                "menuitem",
                "progressbar",
                "slider",
            ]),
        ),
        ("label", PropType::String),
        ("disabled", PropType::Boolean),
        ("hidden", PropType::Boolean),
        ("focusable", PropType::Boolean),
        ("modal", PropType::Boolean),
        ("checked", PropType::Boolean),
        ("selected", PropType::Boolean),
        ("expanded", PropType::Boolean),
    ])
}

fn text_props() -> HashMap<&'static str, PropType> {
    HashMap::from([
        ("ref", PropType::Unknown),
        ("color", PropType::Color),
        ("fontSize", PropType::Number),
        ("fontWeight", PropType::Number),
        ("fontStyle", PropType::Union(vec!["normal", "italic"])),
        ("lineHeight", PropType::Number),
        ("letterSpacing", PropType::Number),
        (
            "textAlign",
            PropType::Union(vec!["left", "center", "right"]),
        ),
        ("textOverflow", PropType::Union(vec!["clip", "ellipsis"])),
        ("lineClamp", PropType::Number),
        ("testID", PropType::String),
    ])
}

fn text_field_props() -> HashMap<&'static str, PropType> {
    let mut props = HashMap::from([
        ("value", PropType::String),
        ("placeholder", PropType::String),
        ("secure", PropType::Boolean),
        ("multiline", PropType::Boolean),
        ("autofill", PropType::String),
        ("keyboard", PropType::String),
        ("maxLength", PropType::Number),
        ("onChange", PropType::Function),
        ("fontSize", PropType::Number),
        ("color", PropType::Color),
    ]);
    for (key, value) in view_props() {
        props.entry(key).or_insert(value);
    }
    props
}

pub(crate) fn infer_prop_type(value: &JSXAttributeValue<'_>) -> PropType {
    match value {
        JSXAttributeValue::StringLiteral(literal) => {
            PropType::StrLit(literal.value.as_str().unwrap_or("").to_string())
        }
        JSXAttributeValue::ExpressionContainer(container) => {
            let Some(expr) = container.expression.as_expression() else {
                return PropType::Unknown;
            };
            match expr {
                Expression::NumericLiteral(_) => PropType::Number,
                Expression::BooleanLiteral(_) => PropType::Boolean,
                Expression::StringLiteral(literal) => {
                    PropType::StrLit(literal.value.as_str().unwrap_or("").to_string())
                }
                Expression::ArrayExpression(_) => PropType::Array(Box::new(PropType::Unknown)),
                Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_) => {
                    PropType::Function
                }
                _ => PropType::Unknown,
            }
        }
        _ => PropType::Unknown,
    }
}

pub(crate) fn prop_types_compatible(actual: &PropType, expected: &PropType) -> bool {
    match (actual, expected) {
        (PropType::Unknown, _) | (_, PropType::Unknown) => true,
        (a, b) if a == b => true,
        (PropType::StrLit(_), PropType::String | PropType::Color | PropType::Len) => true,
        (PropType::StrLit(value), PropType::Union(variants)) => variants.contains(&value.as_str()),
        (PropType::String, PropType::Union(variants)) => {
            variants.iter().any(|&v| v != "number" && v != "boolean")
        }
        (PropType::Number, PropType::Color | PropType::Len) => true,
        (PropType::Number, PropType::Union(variants)) => {
            variants.contains(&"number") || variants.contains(&"number[]")
        }
        (PropType::Array(_), PropType::Union(variants)) => {
            variants.iter().any(|variant| variant.contains("[]"))
        }
        (PropType::Function, PropType::Function) => true,
        _ => false,
    }
}
