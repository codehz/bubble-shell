//! M0 loader checks. Ed25519 is intentionally not implemented; `signature` must
//! be present and non-empty so the later verifier has a field to fill.

use serde::Deserialize;
use sha2::{Digest, Sha256};

#[derive(Debug, Deserialize)]
struct Manifest {
    schema: u32,
    signature: Option<String>,
    files: Vec<FileEntry>,
}

#[derive(Debug, Deserialize)]
struct FileEntry {
    path: String,
    sha256: String,
    size: u64,
}

pub fn validate_bundle_hashes(manifest_json: &str, files: &[(&str, &[u8])]) -> Result<(), String> {
    let manifest: Manifest = serde_json::from_str(manifest_json).map_err(|err| err.to_string())?;
    if manifest.schema != 1 {
        return Err(format!("unsupported manifest schema {}", manifest.schema));
    }
    match manifest.signature.as_deref().map(str::trim) {
        Some(signature) if !signature.is_empty() => {}
        _ => return Err("signature placeholder is missing".into()),
    }
    if manifest.files.is_empty() {
        return Err("manifest has no files".into());
    }
    for entry in &manifest.files {
        if entry.path.is_empty()
            || entry.path.starts_with('/')
            || entry
                .path
                .split('/')
                .any(|part| part.is_empty() || part == "." || part == "..")
        {
            return Err(format!("illegal bundle path {}", entry.path));
        }
        let bytes = files
            .iter()
            .find(|(path, _)| *path == entry.path)
            .map(|(_, bytes)| *bytes)
            .ok_or_else(|| format!("missing bundle file {}", entry.path))?;
        if bytes.len() as u64 != entry.size {
            return Err(format!(
                "{} size is {} bytes, manifest says {}",
                entry.path,
                bytes.len(),
                entry.size
            ));
        }
        let digest = Sha256::digest(bytes);
        let encoded = hex_encode(&digest);
        if !encoded.eq_ignore_ascii_case(&entry.sha256) {
            return Err(format!("{} sha256 mismatch", entry.path));
        }
    }
    Ok(())
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0xf) as usize] as char);
    }
    out
}
