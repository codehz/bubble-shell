//! M0 web slice: static template IR, op batches, Taffy layout, hit-test, paint list.
//!
//! Assumptions locked for this slice:
//! - handle == template node id (dense, starting at 1)
//! - no dynamic Create/Insert/Move/Dispose
//! - signature must be non-empty but is not cryptographically verified

mod ir;
mod manifest;
mod ops;
mod shell;

pub use ir::{Kind, NodeDesc, Template};
pub use manifest::validate_bundle_hashes;
pub use ops::{decode_ops, encode_set_prop_color, encode_set_text, Op};
pub use shell::{PaintCmd, Shell};
