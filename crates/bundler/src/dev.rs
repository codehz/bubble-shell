//! Development builds: one CommonJS factory per module, wrapped for hot updates.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use rolldown::{Bundler, BundlerOptions, InputItem, OutputFormat, Platform};
use rolldown_common::Output;
use rolldown_plugin::Plugin;

use crate::assets::BundledAsset;
use crate::plugin::{BubblePlugin, BubbleState};

/// One module in a development build.
#[derive(Debug, Clone)]
pub struct DevChunk {
    /// Module id: the emitted chunk filename, e.g. `app.js` or `card/card.js`.
    pub id: String,
    /// Factory body that runs inside `(require, module, exports)`.
    pub code: String,
}

/// Result of a development build.
#[derive(Debug)]
pub struct DevBuild {
    /// Id of the entry module.
    pub entry_id: String,
    /// Every module, ordered by rolldown's emission.
    pub chunks: Vec<DevChunk>,
    /// Assets referenced by the modules.
    pub assets: Vec<BundledAsset>,
    /// Source files the build read, for the watcher.
    pub sources: Vec<PathBuf>,
}

/// Build every module of `entry` as a CommonJS factory.
pub fn build_dev(entry: &Path) -> Result<DevBuild, String> {
    let entry =
        std::fs::canonicalize(entry).map_err(|error| format!("{}: {error}", entry.display()))?;
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
    let plugin = BubblePlugin::new(false, state.clone()).with_hot(true);
    let options = BundlerOptions {
        input: Some(vec![InputItem {
            name: Some(name),
            import: entry.to_string_lossy().into_owned(),
        }]),
        cwd: Some(cwd),
        format: Some(OutputFormat::Cjs),
        platform: Some(Platform::Neutral),
        preserve_modules: Some(true),
        ..BundlerOptions::default()
    };

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("tokio runtime: {error}"))?;
    let mut bundler = Bundler::with_plugins(options, vec![BubblePlugin::new_shared(plugin)])
        .map_err(|error| format!("{error:?}"))?;
    let generated = match runtime.block_on(bundler.generate()) {
        Ok(generated) => generated,
        Err(error) => {
            return Err(state.take_error().unwrap_or_else(|| format!("{error:?}")));
        }
    };

    let mut chunks = Vec::new();
    let mut entry_id = None;
    for output in generated.assets {
        if let Output::Chunk(chunk) = output {
            if chunk.is_entry {
                entry_id = Some(chunk.filename.to_string());
            }
            chunks.push(DevChunk {
                id: chunk.filename.to_string(),
                code: chunk.code.clone(),
            });
        }
    }
    let entry_id =
        entry_id.ok_or_else(|| format!("{}: bundler produced no entry chunk", entry.display()))?;
    let assets = std::mem::take(&mut *state.assets.lock().unwrap()).into_assets();
    let sources = state.sources.lock().unwrap().clone();
    Ok(DevBuild {
        entry_id,
        chunks,
        assets,
        sources,
    })
}

/// Render the module table as an ES module that installs itself in `bubble:hot`.
pub fn render_dev_bundle(build: &DevBuild) -> String {
    let mut out = String::from(
        "import * as __bubble_hot from \"bubble:hot\";\n\
         __bubble_hot.reset?.();\n",
    );
    for chunk in &build.chunks {
        out.push_str(&format!(
            "__bubble_hot.define({}, function (require, module, exports) {{\n{}}});\n",
            js_string(&chunk.id),
            chunk.code
        ));
    }
    out.push_str(&format!(
        "__bubble_hot.run({});\n\
         if (globalThis.__bubbleDev && globalThis.__bubbleDev.ws) __bubble_hot.connect(globalThis.__bubbleDev.ws);\n",
        js_string(&build.entry_id)
    ));
    out
}

fn js_string(value: &str) -> String {
    let mut out = String::from("\"");
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}
