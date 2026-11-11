//! Retained tree, signal graph, op batches, Taffy layout, hit-test, and paint list.
//!
//! Apps are function components. `Turn` owns the signal graph and produces op
//! batches; the host owns `Shell`, applies batches on its UI/frame thread, and
//! paints. Nodes are created only by `Spawn` / `SpawnAt` ops. Handles are
//! unique and never reused. A bundle signature must be non-empty; it is not
//! verified cryptographically yet.

mod ir;
mod manifest;
mod ops;
mod paint;
mod props;
mod shell;
mod turn;

pub use ir::{Kind, NodeDesc};
pub use manifest::validate_bundle_hashes;
pub use ops::{decode_ops, encode, encode_set_prop_color, encode_set_text, Op, Value};
pub use paint::encode_paint;
pub use props::{Prop, SET_PROPS, SPAWN_PROPS};
pub use shell::{PaintCmd, Shell};
pub use turn::Turn;
