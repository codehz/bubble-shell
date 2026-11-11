//! M0 web slice: static template IR, op batches, Taffy layout, hit-test, paint list.
//!
//! Assumptions locked for this slice:
//! - static handles equal template ids (dense, starting at 1)
//! - dynamic handles are script-allocated, checked for uniqueness, and never reused
//! - signature must be non-empty but is not cryptographically verified

mod ir;
mod manifest;
mod ops;
mod shell;

pub use ir::{Kind, NodeDesc, Template};
pub use manifest::validate_bundle_hashes;
pub use ops::{decode_ops, encode, encode_set_prop_color, encode_set_text, Op};
pub use shell::{PaintCmd, Shell};
