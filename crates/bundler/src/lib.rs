//! Release bundling for Bubble apps.
//!
//! One entry in, one ES module out. The output only contains application code
//! and imports of the host modules (`bubble-shell`, `bubble:rt`); the runtime
//! itself is never bundled.

mod assets;
mod bundle;
mod dev;
mod plugin;

pub use assets::{content_hash, BundledAsset, ASSET_EXTENSIONS};
pub use bundle::{bundle_entry, BundleOptions, BundledEntry};
pub use dev::{build_dev, render_dev_bundle, DevBuild, DevChunk};
