use std::collections::HashSet;

use crate::{
    EventField, EventHandler, EventSpec, Field, FnType, GestureSpec, Method, Param, PlatformApi,
    PrimitiveSpec, PropDef, PropType, Schema, ShellProp,
};

const SHELL_PROPS: &[(&str, &str)] = &[
    ("color", "Color"),
    ("backgroundColor", "BackgroundColor"),
    ("radius", "Radius"),
    ("opacity", "Opacity"),
    ("fontSize", "FontSize"),
    ("paddingStart", "PaddingStart"),
    ("paddingEnd", "PaddingEnd"),
    ("paddingTop", "PaddingTop"),
    ("paddingBottom", "PaddingBottom"),
    ("width", "Width"),
    ("height", "Height"),
    ("minWidth", "MinWidth"),
    ("minHeight", "MinHeight"),
    ("maxWidth", "MaxWidth"),
    ("maxHeight", "MaxHeight"),
    ("flexBasis", "FlexBasis"),
    ("flexGrow", "FlexGrow"),
    ("flexShrink", "FlexShrink"),
    ("overflowX", "ScrollX"),
    ("overflowY", "ScrollY"),
    ("overflow", "Overflow"),
    ("display", "Display"),
    ("visibility", "Visibility"),
    ("flexDirection", "Row"),
    ("lineHeight", "LineHeight"),
    ("textAlign", "TextAlign"),
    ("textOverflow", "TextOverflow"),
    ("lineClamp", "LineClamp"),
    ("fontWeight", "FontWeight"),
    ("marginStart", "MarginStart"),
    ("marginEnd", "MarginEnd"),
    ("marginTop", "MarginTop"),
    ("marginBottom", "MarginBottom"),
    ("gap", "Gap"),
    ("rowGap", "RowGap"),
    ("columnGap", "ColumnGap"),
    ("alignItems", "AlignItems"),
    ("alignSelf", "AlignSelf"),
    ("alignContent", "AlignContent"),
    ("justifyContent", "JustifyContent"),
    ("flexWrap", "FlexWrap"),
    ("position", "Position"),
    ("top", "Top"),
    ("end", "End"),
    ("bottom", "Bottom"),
    ("start", "Start"),
    ("aspectRatio", "AspectRatio"),
    ("src", "Src"),
    ("fit", "Fit"),
    ("overscrollX", "ScrollChainX"),
    ("overscrollY", "ScrollChainY"),
    ("multiline", "Multiline"),
    ("secure", "Secure"),
    ("maxLength", "MaxLength"),
    ("keyboard", "Keyboard"),
    ("autofill", "Autofill"),
    ("role", "Role"),
    ("label", "Label"),
    ("testID", "TestID"),
    ("disabled", "Disabled"),
    ("hidden", "Hidden"),
    ("focusable", "Focusable"),
    ("modal", "Modal"),
    ("checked", "Checked"),
    ("selected", "Selected"),
    ("expanded", "Expanded"),
    ("fadeEdges", "FadeEdges"),
];

const ALIGN: &[&str] = &[
    "flex-start",
    "flex-end",
    "center",
    "stretch",
    "start",
    "end",
    "baseline",
];

const ALIGN_SELF: &[&str] = &[
    "auto",
    "flex-start",
    "flex-end",
    "center",
    "stretch",
    "start",
    "end",
    "baseline",
];

const JUSTIFY: &[&str] = &[
    "flex-start",
    "flex-end",
    "center",
    "space-between",
    "space-around",
    "space-evenly",
    "start",
    "end",
];

const OVERFLOW: &[&str] = &["visible", "hidden", "scroll"];

const ROLE: &[&str] = &[
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
];

const CAPTURE_AXIS: &[&str] = &["none", "x", "y", "both"];
const TRANSFORM_MODE: &[&str] = &["pan", "pinch", "rotate"];

const DECORATION_LAYER: &[&str] = &["background", "underlay", "overlay"];
const BORDER_ALIGN: &[&str] = &["inside", "center", "outside"];
/// The single schema. Shell props are listed explicitly: JSX shorthands
/// (`padding`, `margin`, `inset`) are expanded by the runtime before
/// `Prop::parse`, and several keys do not match their variant names.
pub fn bubble_shell_schema() -> Schema {
    let view = view_props();
    let mut schema = Schema::default();
    schema.shell_props = SHELL_PROPS
        .iter()
        .map(|(key, variant)| ShellProp { key, variant })
        .collect();

    schema.primitives = vec![
        PrimitiveSpec {
            name: "View",
            props: view.clone(),
        },
        PrimitiveSpec {
            name: "Text",
            props: text_props(),
        },
        PrimitiveSpec {
            name: "Image",
            props: image_props(),
        },
        PrimitiveSpec {
            name: "TextField",
            props: text_field_props(&view),
        },
        PrimitiveSpec {
            name: "ScrollBar",
            props: scroll_bar_props(),
        },
        PrimitiveSpec {
            name: "Border",
            props: border_props(),
        },
        PrimitiveSpec {
            name: "Outline",
            props: outline_props(),
        },
        PrimitiveSpec {
            name: "Shadow",
            props: shadow_props(),
        },
    ];

    schema.events = vec![
        event(
            "TextFieldChangeEvent",
            vec![field("value", PropType::String)],
        ),
        event(
            "TapEvent",
            vec![
                field("type", PropType::StrLit("tap")),
                field("x", PropType::Number),
                field("y", PropType::Number),
            ],
        ),
        event(
            "LongPressEvent",
            vec![
                field("type", PropType::StrLit("longPress")),
                field("x", PropType::Number),
                field("y", PropType::Number),
                field("translationX", PropType::Number),
                field("translationY", PropType::Number),
            ],
        ),
        event(
            "PanEvent",
            vec![
                field("type", PropType::StrLit("pan")),
                field("x", PropType::Number),
                field("y", PropType::Number),
                field("translationX", PropType::Number),
                field("translationY", PropType::Number),
                field("deltaX", PropType::Number),
                field("deltaY", PropType::Number),
                field("velocityX", PropType::Number),
                field("velocityY", PropType::Number),
            ],
        ),
        event(
            "TransformEvent",
            vec![
                field("type", PropType::StrLit("transform")),
                field("x", PropType::Number),
                field("y", PropType::Number),
                field("translationX", PropType::Number),
                field("translationY", PropType::Number),
                field("deltaX", PropType::Number),
                field("deltaY", PropType::Number),
                field("scale", PropType::Number),
                field("rotation", PropType::Number),
                field("velocityX", PropType::Number),
                field("velocityY", PropType::Number),
            ],
        ),
    ];

    schema.gestures = vec![
        GestureSpec {
            name: "TapGesture",
            kind: 1,
            props: vec![opt("enabled", PropType::Boolean)],
            events: vec![
                on("onTapStart", 1, "TapEvent"),
                on("onTapEnd", 2, "TapEvent"),
                on("onTapCancel", 3, "TapEvent"),
                on("onTap", 4, "TapEvent"),
            ],
        },
        GestureSpec {
            name: "DoubleTapGesture",
            kind: 2,
            props: vec![opt("enabled", PropType::Boolean)],
            events: vec![
                on("onDoubleTapStart", 1, "TapEvent"),
                on("onDoubleTapEnd", 2, "TapEvent"),
                on("onDoubleTapCancel", 3, "TapEvent"),
                on("onDoubleTap", 5, "TapEvent"),
            ],
        },
        GestureSpec {
            name: "LongPressGesture",
            kind: 3,
            props: vec![
                opt("duration", PropType::Number),
                opt("maxDistance", PropType::Number),
                opt("captureAxis", union(CAPTURE_AXIS)),
                opt("enabled", PropType::Boolean),
            ],
            events: vec![
                on("onLongPress", 6, "LongPressEvent"),
                on("onLongPressEnd", 8, "LongPressEvent"),
                on("onLongPressCancel", 9, "LongPressEvent"),
            ],
        },
        GestureSpec {
            name: "PanGesture",
            kind: 4,
            props: vec![
                opt("axis", union(&["x", "y", "both"])),
                opt("direction", array(PropType::String)),
                opt("minDistance", PropType::Number),
                opt("failCrossDistance", PropType::Number),
                opt("captureAxis", union(CAPTURE_AXIS)),
                opt("enabled", PropType::Boolean),
            ],
            events: vec![
                on("onPanStart", 6, "PanEvent"),
                on("onPanUpdate", 7, "PanEvent"),
                on("onPanEnd", 8, "PanEvent"),
                on("onPanCancel", 9, "PanEvent"),
            ],
        },
        GestureSpec {
            name: "TransformGesture",
            kind: 5,
            props: vec![
                opt("mode", PropType::OneOrArray(TRANSFORM_MODE)),
                opt("minDistance", PropType::Number),
                opt("captureAxis", union(CAPTURE_AXIS)),
                opt("enabled", PropType::Boolean),
            ],
            events: vec![
                on("onTransformStart", 6, "TransformEvent"),
                on("onTransformUpdate", 7, "TransformEvent"),
                on("onTransformEnd", 8, "TransformEvent"),
                on("onTransformCancel", 9, "TransformEvent"),
            ],
        },
    ];

    schema.platform = vec![
        PlatformApi::Object {
            name: "storage",
            methods: vec![
                method(
                    "get",
                    vec![param("key", PropType::String)],
                    promise(PropType::Union(vec![PropType::String, PropType::Null])),
                ),
                method(
                    "set",
                    vec![
                        param("key", PropType::String),
                        param("value", PropType::String),
                    ],
                    promise(PropType::Void),
                ),
                method(
                    "remove",
                    vec![param("key", PropType::String)],
                    promise(PropType::Void),
                ),
                method("keys", vec![], promise(array(PropType::String))),
            ],
        },
        PlatformApi::Object {
            name: "host",
            methods: vec![
                method(
                    "call",
                    vec![
                        param("method", PropType::String),
                        opt_param("params", PropType::Object),
                    ],
                    promise(PropType::Unknown),
                ),
                method(
                    "post",
                    vec![
                        param("method", PropType::String),
                        opt_param("params", PropType::Object),
                    ],
                    PropType::Void,
                ),
                method(
                    "on",
                    vec![
                        param("event", PropType::String),
                        param(
                            "handler",
                            func(vec![param("payload", PropType::Unknown)], PropType::Void),
                        ),
                    ],
                    func(vec![], PropType::Void),
                ),
                method(
                    "onBack",
                    vec![param("handler", func(vec![], PropType::Boolean))],
                    func(vec![], PropType::Void),
                ),
            ],
        },
        PlatformApi::Object {
            name: "clipboard",
            methods: vec![
                method("readText", vec![], promise(PropType::String)),
                method(
                    "writeText",
                    vec![param("text", PropType::String)],
                    promise(PropType::Void),
                ),
            ],
        },
        PlatformApi::Function {
            name: "toast",
            params: vec![param("message", PropType::String)],
            ret: promise(PropType::Void),
        },
        PlatformApi::Function {
            name: "openURL",
            params: vec![param("url", PropType::String)],
            ret: promise(PropType::Void),
        },
        PlatformApi::Function {
            name: "navigate",
            params: vec![param("intent", PropType::Object)],
            ret: PropType::Void,
        },
        PlatformApi::Function {
            name: "fetch",
            params: vec![
                param("url", PropType::String),
                opt_param(
                    "init",
                    PropType::ObjectLit(vec![
                        Field {
                            name: "method",
                            ty: PropType::String,
                            optional: true,
                        },
                        Field {
                            name: "body",
                            ty: PropType::String,
                            optional: true,
                        },
                    ]),
                ),
            ],
            ret: promise(PropType::Unknown),
        },
    ];

    validate(&schema);
    schema
}

fn view_props() -> Vec<PropDef> {
    let mut props = vec![
        opt("ref", PropType::Ref),
        opt("backgroundColor", PropType::Color),
        opt("flexGrow", PropType::Number),
        opt("flexShrink", PropType::Number),
        opt("flexBasis", PropType::Len),
        opt(
            "flexDirection",
            union(&["row", "column", "row-reverse", "column-reverse"]),
        ),
        opt("flexWrap", union(&["wrap", "nowrap", "wrap-reverse"])),
        opt("alignItems", union(ALIGN)),
        opt("alignSelf", union(ALIGN_SELF)),
        opt("alignContent", union(ALIGN)),
        opt("justifyContent", union(JUSTIFY)),
        opt("padding", PropType::Edges),
        opt("margin", PropType::Edges),
        opt("inset", PropType::Edges),
        opt("gap", PropType::Number),
        opt("rowGap", PropType::Number),
        opt("columnGap", PropType::Number),
        opt("width", PropType::Len),
        opt("height", PropType::Len),
        opt("minWidth", PropType::Len),
        opt("minHeight", PropType::Len),
        opt("maxWidth", PropType::Len),
        opt("maxHeight", PropType::Len),
        opt("aspectRatio", PropType::Number),
        opt("position", union(&["relative", "absolute"])),
        opt("top", PropType::Len),
        opt("bottom", PropType::Len),
        opt("left", PropType::Len),
        opt("right", PropType::Len),
        opt("start", PropType::Len),
        opt("end", PropType::Len),
        opt("radius", PropType::Number),
        opt("opacity", PropType::Number),
        opt("overflow", union(OVERFLOW)),
        opt("overflowX", union(OVERFLOW)),
        opt("overflowY", union(OVERFLOW)),
        opt("overscrollX", PropType::Len),
        opt("overscrollY", PropType::Len),
        opt("display", union(&["flex", "none"])),
        opt("visibility", union(&["visible", "hidden"])),
        opt("fadeEdges", PropType::Number),
        opt("testID", PropType::String),
        opt("role", union(ROLE)),
        opt("label", PropType::String),
        opt("disabled", PropType::Boolean),
        opt("hidden", PropType::Boolean),
        opt("focusable", PropType::Boolean),
        opt("modal", PropType::Boolean),
        opt("checked", PropType::Boolean),
        opt("selected", PropType::Boolean),
        opt("expanded", PropType::Boolean),
    ];
    push_transition(&mut props, VIEW_TRANSITIONS);
    props
}

fn text_props() -> Vec<PropDef> {
    let mut props = vec![
        opt("ref", PropType::Ref),
        opt("color", PropType::Color),
        opt("fontSize", PropType::Number),
        opt("fontWeight", PropType::Number),
        opt("fontStyle", union(&["normal", "italic"])),
        opt("lineHeight", PropType::Number),
        opt("letterSpacing", PropType::Number),
        opt("textAlign", union(&["left", "center", "right"])),
        opt("textOverflow", union(&["clip", "ellipsis"])),
        opt("lineClamp", PropType::Number),
        opt("width", PropType::Len),
        opt("testID", PropType::String),
    ];
    push_transition(&mut props, &["transition:color"]);
    props
}

fn image_props() -> Vec<PropDef> {
    let mut props = vec![
        opt("ref", PropType::Ref),
        req("src", PropType::String),
        opt("fit", union(&["contain", "cover", "fill"])),
        opt("width", PropType::Len),
        opt("height", PropType::Len),
        opt("label", PropType::String),
        opt("testID", PropType::String),
    ];
    push_transition(&mut props, VIEW_TRANSITIONS);
    props
}

/// TextField's own props win; every View prop is then inherited, matching
/// `text_field_props` in the old compiler table.
fn text_field_props(view: &[PropDef]) -> Vec<PropDef> {
    let mut props = vec![
        opt("value", PropType::String),
        opt("placeholder", PropType::String),
        opt("secure", PropType::Boolean),
        opt("multiline", PropType::Boolean),
        opt("autofill", PropType::String),
        opt("keyboard", PropType::String),
        opt("maxLength", PropType::Number),
        opt("onChange", PropType::EventFn("TextFieldChangeEvent")),
        opt("fontSize", PropType::Number),
        opt("color", PropType::Color),
        opt("transition:color", PropType::Transition),
    ];
    for prop in view {
        if props.iter().any(|existing| existing.name == prop.name) {
            continue;
        }
        props.push(prop.clone());
    }
    props
}

fn scroll_bar_props() -> Vec<PropDef> {
    vec![
        opt("axis", union(&["vertical", "horizontal"])),
        opt("scrollbarWidth", PropType::Number),
        opt("scrollbarMargin", PropType::Number),
        opt("scrollbarColor", PropType::Color),
        opt("scrollbarRadius", PropType::Number),
        opt("scrollbarOpacity", PropType::Number),
        opt("scrollbarMinLength", PropType::Number),
        opt("autoHide", PropType::Boolean),
        opt("interactive", PropType::Boolean),
        opt("ref", PropType::Ref),
    ]
}
fn decoration_base() -> Vec<PropDef> {
    vec![
        req("color", PropType::Color),
        opt("when", PropType::Boolean),
        opt("radius", PropType::Number),
        opt("layer", union(DECORATION_LAYER)),
    ]
}

fn border_props() -> Vec<PropDef> {
    let mut props = decoration_base();
    props.push(opt("width", PropType::Number));
    props.push(opt("align", union(BORDER_ALIGN)));
    props
}

fn outline_props() -> Vec<PropDef> {
    let mut props = decoration_base();
    props.push(opt("width", PropType::Number));
    props.push(opt("offset", PropType::Number));
    props
}

fn shadow_props() -> Vec<PropDef> {
    let mut props = decoration_base();
    props.push(opt("offsetX", PropType::Number));
    props.push(opt("offsetY", PropType::Number));
    props.push(opt("blur", PropType::Number));
    props.push(opt("spread", PropType::Number));
    props
}

fn validate(schema: &Schema) {
    let mut keys = HashSet::new();
    let mut variants = HashSet::new();
    for prop in &schema.shell_props {
        if !keys.insert(prop.key) {
            panic!("duplicate shell prop key {}", prop.key);
        }
        if !variants.insert(prop.variant) {
            panic!("duplicate shell prop variant {}", prop.variant);
        }
        if prop.variant.is_empty()
            || !prop
                .variant
                .chars()
                .next()
                .is_some_and(|ch| ch.is_ascii_alphabetic())
        {
            panic!("shell prop {} has a bad variant name", prop.key);
        }
    }

    let mut prim_names = HashSet::new();
    for prim in &schema.primitives {
        if !prim_names.insert(prim.name) {
            panic!("duplicate primitive {}", prim.name);
        }
        let mut names = HashSet::new();
        for prop in &prim.props {
            if !names.insert(prop.name) {
                panic!("duplicate prop {} on {}", prop.name, prim.name);
            }
            if let PropType::EventFn(event) = prop.ty {
                require_event(schema, prim.name, prop.name, event);
            }
        }
    }

    let mut event_names = HashSet::new();
    for event in &schema.events {
        if !event_names.insert(event.name) {
            panic!("duplicate event {}", event.name);
        }
    }

    let mut gesture_names = HashSet::new();
    for gesture in &schema.gestures {
        if !gesture_names.insert(gesture.name) {
            panic!("duplicate gesture {}", gesture.name);
        }
        for handler in &gesture.events {
            require_event(schema, gesture.name, handler.name, handler.event);
        }
    }
}
const VIEW_TRANSITIONS: &[&str] = &[
    "transition:backgroundColor",
    "transition:opacity",
    "transition:radius",
    "transition:translateX",
    "transition:translateY",
    "transition:scale",
    "transition:rotate",
];

fn push_transition(props: &mut Vec<PropDef>, names: &[&'static str]) {
    props.push(opt("transition", PropType::Transition));
    for name in names {
        props.push(opt(name, PropType::Transition));
    }
}


fn require_event(schema: &Schema, owner: &str, prop: &str, event: &str) {
    if !schema.events.iter().any(|spec| spec.name == event) {
        panic!("{owner} prop {prop} references unknown event {event}");
    }
}

fn opt(name: &'static str, ty: PropType) -> PropDef {
    PropDef {
        name,
        ty,
        required: false,
    }
}

fn req(name: &'static str, ty: PropType) -> PropDef {
    PropDef {
        name,
        ty,
        required: true,
    }
}

fn union(variants: &'static [&'static str]) -> PropType {
    PropType::Union(variants.iter().copied().map(PropType::StrLit).collect())
}

fn array(ty: PropType) -> PropType {
    PropType::Array(Box::new(ty))
}

fn promise(ty: PropType) -> PropType {
    PropType::Promise(Box::new(ty))
}

fn func(params: Vec<Param>, ret: PropType) -> PropType {
    PropType::Func(FnType {
        params,
        ret: Box::new(ret),
    })
}

fn field(name: &'static str, ty: PropType) -> EventField {
    EventField { name, ty }
}

fn event(name: &'static str, fields: Vec<EventField>) -> EventSpec {
    EventSpec { name, fields }
}

fn on(name: &'static str, slot: u8, event: &'static str) -> EventHandler {
    EventHandler { name, slot, event }
}

fn param(name: &'static str, ty: PropType) -> Param {
    Param {
        name,
        ty,
        optional: false,
    }
}

fn opt_param(name: &'static str, ty: PropType) -> Param {
    Param {
        name,
        ty,
        optional: true,
    }
}

fn method(name: &'static str, params: Vec<Param>, ret: PropType) -> Method {
    Method { name, params, ret }
}
