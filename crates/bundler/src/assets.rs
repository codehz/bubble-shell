//! Content-addressed asset imports.
//!
//! `import logo from "./logo.png"` becomes a module that exports
//! `"assets/<name>.<hash8>.png"` relative to the bundle root. The file bytes
//! travel with the build result so the CLI can copy them next to the bundle.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// Raster extensions accepted by the bundler.
pub const ASSET_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp"];

/// One asset copied into the output directory.
#[derive(Debug, Clone)]
pub struct BundledAsset {
    /// Path relative to the bundle root, e.g. `assets/logo.1a2b3c4d.png`.
    pub path: String,
    /// SHA-256 of the contents as lowercase hex.
    pub sha256: String,
    /// Byte length.
    pub size: u64,
    /// File contents.
    pub bytes: Vec<u8>,
}

#[derive(Debug, Default)]
pub struct AssetRegistry {
    entries: Vec<Entry>,
}

#[derive(Debug)]
struct Entry {
    path: String,
    source: PathBuf,
    sha256: String,
    bytes: Vec<u8>,
}

#[derive(Debug)]
pub struct AssetConflict {
    pub path: String,
    pub first: PathBuf,
    pub second: PathBuf,
}

impl AssetRegistry {
    /// Register an asset and return its bundle-relative path.
    pub fn add(&mut self, source: &Path, bytes: Vec<u8>) -> Result<String, AssetConflict> {
        let sha256 = hex(&Sha256::digest(&bytes));
        let name = asset_file_name(source, &sha256).map_err(|_| AssetConflict {
            path: String::new(),
            first: source.to_path_buf(),
            second: source.to_path_buf(),
        })?;
        if let Some(existing) = self.entries.iter().find(|entry| entry.path == name) {
            if existing.sha256 == sha256 {
                return Ok(name);
            }
            return Err(AssetConflict {
                path: name,
                first: existing.source.clone(),
                second: source.to_path_buf(),
            });
        }
        self.entries.push(Entry {
            path: name.clone(),
            source: source.to_path_buf(),
            sha256,
            bytes,
        });
        Ok(name)
    }

    pub fn into_assets(self) -> Vec<BundledAsset> {
        self.entries
            .into_iter()
            .map(|entry| BundledAsset {
                size: entry.bytes.len() as u64,
                path: entry.path,
                sha256: entry.sha256,
                bytes: entry.bytes,
            })
            .collect()
    }
}

fn asset_file_name(source: &Path, sha256: &str) -> Result<String, String> {
    let extension = source
        .extension()
        .and_then(|ext| ext.to_str())
        .ok_or_else(|| format!("{}: asset has no extension", source.display()))?;
    if !ASSET_EXTENSIONS.contains(&extension) {
        return Err(format!(
            "{}: unsupported asset type .{extension}; expected one of {}",
            source.display(),
            ASSET_EXTENSIONS.join(", ")
        ));
    }
    let stem = source
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or_else(|| format!("{}: asset has no file stem", source.display()))?;
    Ok(format!("assets/{stem}.{}.{extension}", &sha256[..8]))
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(DIGITS[(byte >> 4) as usize] as char);
        out.push(DIGITS[(byte & 0xf) as usize] as char);
    }
    out
}

/// Full SHA-256 hex digest of a file's contents.
pub fn content_hash(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}
