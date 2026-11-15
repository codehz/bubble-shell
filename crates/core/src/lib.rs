//! Retained tree, signal graph, op batches, Taffy layout, hit-test, and paint list.
//!
//! Apps are function components. `Turn` owns the signal graph and produces op
//! batches; the host owns `Shell`, applies batches on its UI/frame thread, and
//! paints. Nodes are created only by `Spawn` / `SpawnAt` ops. Handles are
//! unique and never reused. A bundle signature must be non-empty; it is not
//! verified cryptographically yet.

mod color;
mod editing;
mod expr;
mod gesture;
mod ir;
mod layout;
mod manifest;
mod ops;
mod ordered;
mod paint;
mod props;
mod reactive;
mod semantics;
mod shell;
mod state_style;
mod turn;

pub use color::{parse as parse_color, Color};
pub use editing::{
    display_text, encode_edit_events, utf16_len, Affinity, CompositionChange, DocPosition,
    DocRange, EditCommand, EditConfig, EditDelta, EditError, EditEvent, EditOp, EditOutcome,
    EditSession, Expect, FieldState, Revision, Selection, SessionId,
};
pub use expr::{
    decode_program, Ease, Program, ProgramBuilder, ProgramError, OP_ABS, OP_ADD, OP_AND, OP_CLAMP,
    OP_CONST, OP_DIV, OP_EASE, OP_EQ, OP_GT, OP_LE, OP_LERP, OP_LOAD, OP_LT, OP_MAX, OP_MIN,
    OP_MIX, OP_MUL, OP_NE, OP_NEG, OP_NOT, OP_OR, OP_SELECT, OP_SUB,
};
pub use gesture::{
    encode_events, GestureConfig, GestureDecl, GestureEvent, GestureKind, PointerAction,
    PointerInput, Recognizer, ScrollRequest, REASON_HOST_CANCEL,
};
pub use ir::{Kind, NodeDesc};
pub use layout::{
    decode_layout, encode_layout, snap_unit, wrapping_layout, CaretEdge, LayoutKey, LayoutRequest,
    LayoutRun, LayoutSnapshot, LAYOUT_MAGIC,
};
pub use manifest::validate_bundle_hashes;
pub use ops::{
    decode_ops, encode, encode_set_prop_color, encode_set_text, spawn_at_ops, spawn_ops, Op, Value,
};
pub use paint::encode_paint;
pub use props::{Prop, TEXT_STYLE_PROPS, VIEW_STYLE_PROPS};
pub use reactive::{Kind as ComputationKind, Sched};
pub use semantics::{
    Role, SemanticsNode, SemanticsSnapshot, ACTION_ACTIVATE, ACTION_FOCUS, ACTION_SCROLL_BACKWARD,
    ACTION_SCROLL_DOWN, ACTION_SCROLL_FORWARD, ACTION_SCROLL_LEFT, ACTION_SCROLL_RIGHT,
    ACTION_SCROLL_UP, ACTION_SET_VALUE,
};
pub use shell::{A11yAction, EditMenuAction, Key, MenuResult, PaintCmd, ScrollOutcome, Shell};
pub use state_style::{
    encode_var_events, AnimChoice, AnimSpec, VarEvent, EASE_LINEAR, SOURCE_DISABLED,
    SOURCE_EDITING, SOURCE_FOCUSED, SOURCE_NONE, SOURCE_PRESSED,
};
pub use turn::Turn;
