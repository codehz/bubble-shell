//! Retained tree, op batches, Taffy layout, hit-test, and paint list.
//!
//! `Template` is the node list `Shell::mount` reads. It is not an authoring format:
//! apps are function components, and the signal graph is not implemented here.
//! Handles of nodes created at mount match their template ids. Dynamic handles are
//! caller-allocated, unique, and never reused. A bundle signature must be non-empty;
//! it is not verified cryptographically yet.

mod ir;
mod manifest;
mod ops;
mod shell;

pub use ir::{Kind, NodeDesc, Template};
pub use manifest::validate_bundle_hashes;
pub use ops::{decode_ops, encode, encode_set_prop_color, encode_set_text, Op};
pub use shell::{PaintCmd, Shell};
