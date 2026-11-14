//! Programmatic release manifests for test fixtures.
//!
//! Same fields as the `bubble bundle` output (`counter.json`): schema, id,
//! version, minRuntime, platforms, entry, file hashes, assets, and the
//! placeholder signature the native loader accepts.

use bubble_bundler::BundledAsset;

const MIN_RUNTIME: &str = "0.1.0";
const PLATFORMS: &[&str] = &["web", "ios", "android"];

pub fn manifest_json(name: &str, code: &str, assets: &[BundledAsset]) -> String {
    let files = vec![serde_json::json!({
        "path": format!("{name}.js"),
        "sha256": bubble_bundler::content_hash(code.as_bytes()),
        "size": code.len() as u64,
    })];
    let assets: Vec<_> = assets
        .iter()
        .map(|asset| {
            serde_json::json!({
                "path": asset.path,
                "sha256": asset.sha256,
                "size": asset.size,
            })
        })
        .collect();
    serde_json::json!({
        "schema": 1,
        "id": format!("com.bubble.{name}"),
        "version": 1,
        "minRuntime": MIN_RUNTIME,
        "platforms": PLATFORMS,
        "entry": format!("{name}.js"),
        "files": files,
        "assets": assets,
        "keyId": "m0-placeholder",
        "signature": "m0-placeholder",
    })
    .to_string()
}
