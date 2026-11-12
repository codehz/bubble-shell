//! Run one rolldown build for a single entry.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use rolldown::{Bundler, BundlerOptions, InputItem, OutputFormat, Platform, SourceMapType};
use rolldown_common::Output;
use rolldown_plugin::Plugin;

use crate::assets::BundledAsset;
use crate::plugin::{BubblePlugin, BubbleState};

/// One entry to bundle.
#[derive(Debug, Clone)]
pub struct BundleOptions {
    /// Entry module path.
    pub entry: PathBuf,
    /// Emit a source map next to the bundle.
    pub source_map: bool,
}

/// Result of bundling one entry.
#[derive(Debug)]
pub struct BundledEntry {
    /// Entry name, derived from the file stem.
    pub name: String,
    /// The single ES module.
    pub code: String,
    /// Source map JSON when requested.
    pub source_map: Option<String>,
    /// Assets referenced by the bundle.
    pub assets: Vec<BundledAsset>,
}

pub fn bundle_entry(options: &BundleOptions) -> Result<BundledEntry, String> {
    let entry = std::fs::canonicalize(&options.entry)
        .map_err(|error| format!("{}: {error}", options.entry.display()))?;
    let name = entry
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or_else(|| format!("{}: entry has no file name", entry.display()))?
        .to_string();
    let cwd = entry
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));

    let state = Arc::new(BubbleState::default());
    let plugin = BubblePlugin::new(options.source_map, state.clone());
    let bundler_options = BundlerOptions {
        input: Some(vec![InputItem {
            name: Some(name.clone()),
            import: entry.to_string_lossy().into_owned(),
        }]),
        cwd: Some(cwd),
        format: Some(OutputFormat::Esm),
        platform: Some(Platform::Neutral),
        sourcemap: options.source_map.then_some(SourceMapType::File),
        ..BundlerOptions::default()
    };

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("tokio runtime: {error}"))?;
    let mut bundler =
        Bundler::with_plugins(bundler_options, vec![BubblePlugin::new_shared(plugin)])
            .map_err(|error| format!("{error:?}"))?;
    let generated = match runtime.block_on(bundler.generate()) {
        Ok(generated) => generated,
        Err(error) => {
            return Err(state.take_error().unwrap_or_else(|| format!("{error:?}")));
        }
    };

    let mut code = None;
    let mut source_map = None;
    for output in generated.assets {
        if let Output::Chunk(chunk) = output {
            if chunk.is_entry {
                code = Some(chunk.code.clone());
                source_map = chunk.map.as_ref().map(|map| map.to_json_string());
            }
        }
    }
    let code =
        code.ok_or_else(|| format!("{}: bundler produced no entry chunk", entry.display()))?;

    let assets = std::mem::take(&mut *state.assets.lock().unwrap()).into_assets();
    Ok(BundledEntry {
        name,
        code,
        source_map,
        assets,
    })
}
