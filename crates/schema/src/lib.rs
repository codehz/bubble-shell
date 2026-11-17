//! Schema for Bubble Shell props, gestures, and the author-facing API.
//!
//! [`bubble_shell_schema`] is the only definition. [`codegen`] turns it into
//! the `Prop` enum, the compiler's `PrimSpec` tables, and the generated
//! `.d.ts`.

mod schema;

pub mod codegen;

pub use schema::bubble_shell_schema;

/// Shell property: the string `Prop::parse` accepts and the enum variant it
/// maps to. Variant names are not PascalCase of the key (`overflowX` is
/// `ScrollX`, `flexDirection` is `Row`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShellProp {
    pub key: &'static str,
    pub variant: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PropDef {
    pub name: &'static str,
    pub ty: PropType,
    pub required: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrimitiveSpec {
    pub name: &'static str,
    pub props: Vec<PropDef>,
}

impl PrimitiveSpec {
    pub fn required_props(&self) -> Vec<&'static str> {
        self.props
            .iter()
            .filter(|prop| prop.required)
            .map(|prop| prop.name)
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventHandler {
    pub name: &'static str,
    pub slot: u8,
    pub event: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GestureSpec {
    pub name: &'static str,
    pub kind: u8,
    pub props: Vec<PropDef>,
    pub events: Vec<EventHandler>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventField {
    pub name: &'static str,
    pub ty: PropType,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventSpec {
    pub name: &'static str,
    pub fields: Vec<EventField>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Param {
    pub name: &'static str,
    pub ty: PropType,
    pub optional: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    pub name: &'static str,
    pub ty: PropType,
    pub optional: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FnType {
    pub params: Vec<Param>,
    pub ret: Box<PropType>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Method {
    pub name: &'static str,
    pub params: Vec<Param>,
    pub ret: PropType,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlatformApi {
    Object {
        name: &'static str,
        methods: Vec<Method>,
    },
    Function {
        name: &'static str,
        params: Vec<Param>,
        ret: PropType,
    },
}

/// Property type stored in the schema.
///
/// JSX props that reach the compiler use the simple variants (`Color`,
/// `Len`, `Union` of string literals, `Edges`, `EventFn`, `Ref`). Platform
/// signatures also use `Promise`, `Func`, `ObjectLit`, and `Null`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PropType {
    Color,
    Len,
    Number,
    String,
    Boolean,
    Null,
    Unknown,
    Object,
    Void,
    /// Unparameterized `Ref`. Not a dynamic prop.
    Ref,
    StrLit(&'static str),
    Union(Vec<PropType>),
    Array(Box<PropType>),
    Promise(Box<PropType>),
    /// `number | number[]`, the padding/margin/inset shorthand.
    Edges,
    /// `"a" | "b" | ("a" | "b")[]`.
    OneOrArray(&'static [&'static str]),
    /// `(event: Name) => void`. Not a dynamic prop.
    EventFn(&'static str),
    Func(FnType),
    ObjectLit(Vec<Field>),
}

impl PropType {
    /// Props a binding or shared value may drive. Handlers and refs are not
    /// dynamic: handlers are plain functions, and `ref` is the ref itself.
    pub fn is_dynamic_eligible(&self) -> bool {
        !matches!(
            self,
            PropType::Ref | PropType::EventFn(_) | PropType::Func(_)
        )
    }
}

#[derive(Clone, Debug, Default)]
pub struct Schema {
    shell_props: Vec<ShellProp>,
    primitives: Vec<PrimitiveSpec>,
    gestures: Vec<GestureSpec>,
    events: Vec<EventSpec>,
    platform: Vec<PlatformApi>,
}

impl Schema {
    pub fn shell_props(&self) -> &[ShellProp] {
        &self.shell_props
    }

    pub fn primitives(&self) -> &[PrimitiveSpec] {
        &self.primitives
    }

    pub fn gestures(&self) -> &[GestureSpec] {
        &self.gestures
    }

    pub fn events(&self) -> &[EventSpec] {
        &self.events
    }

    pub fn platform(&self) -> &[PlatformApi] {
        &self.platform
    }
}
