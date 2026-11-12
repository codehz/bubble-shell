//! Retained tree, signal graph, op batches, Taffy layout, hit-test, and paint list.
//!
//! Apps are function components. `Turn` owns the signal graph and produces op
//! batches; the host owns `Shell`, applies batches on its UI/frame thread, and
//! paints. Nodes are created only by `Spawn` / `SpawnAt` ops. Handles are
//! unique and never reused. A bundle signature must be non-empty; it is not
//! verified cryptographically yet.

mod color;
mod editing;
mod gesture;
mod ir;
mod layout;
mod manifest;
mod ops;
mod paint;
mod props;
mod reactive;
mod semantics;
mod shell;
mod turn;

pub use color::{parse as parse_color, Color};
pub use editing::{
    display_text, encode_edit_events, utf16_len, Affinity, CompositionChange, DocPosition,
    DocRange, EditCommand, EditConfig, EditDelta, EditError, EditEvent, EditOp, EditOutcome,
    EditSession, Expect, FieldState, Revision, Selection, SessionId,
};
pub use gesture::{
    encode_events, GestureConfig, GestureDecl, GestureEvent, GestureKind, PointerAction,
    PointerInput, Recognizer, ScrollRequest, REASON_HOST_CANCEL,
};
pub use ir::{Extension, Kind, NodeDesc, NODE_VERSION};
pub use layout::{
    decode_layout, encode_layout, snap_unit, wrapping_layout, CaretEdge, LayoutKey, LayoutRequest,
    LayoutRun, LayoutSnapshot, LAYOUT_MAGIC,
};
pub use manifest::validate_bundle_hashes;
pub use ops::{decode_ops, encode, encode_set_prop_color, encode_set_text, Op, Value};
pub use paint::encode_paint;
pub use props::{Prop, DEFERRED_PROPS, SET_PROPS, SPAWN_PROPS};
pub use reactive::{Kind as ComputationKind, Sched};
pub use semantics::{
    Role, SemanticsNode, SemanticsSnapshot, ACTION_ACTIVATE, ACTION_FOCUS, ACTION_SET_VALUE,
};
pub use shell::{A11yAction, EditMenuAction, Key, MenuResult, PaintCmd, ScrollOutcome, Shell};
pub use turn::Turn;
