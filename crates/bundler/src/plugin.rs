//! Rolldown plugin: resolve and load Bubble modules, native specifiers, and assets.

use std::borrow::Cow;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use arcstr::ArcStr;
use bubble_compiler::{compile_module, CompileOptions};
use rolldown_common::{ModuleType, ResolvedExternal};
use rolldown_plugin::{
    HookLoadArgs, HookLoadOutput, HookLoadReturn, HookResolveIdArgs, HookResolveIdOutput,
    HookResolveIdReturn, HookUsage, Plugin, PluginContext, SharedLoadPluginContext,
};
use rolldown_sourcemap::SourceMap;

use crate::assets::{AssetConflict, AssetRegistry};

/// Modules the host provides. They stay external specifiers in the bundle.
pub const NATIVE_MODULES: &[&str] = &["bubble-shell", "bubble:rt", "bubble:hot"];

const SCRIPT_EXTENSIONS: &[&str] = &["tsx", "ts", "jsx", "js"];

/// Shared plugin state. The bundler keeps a handle to read assets after the build.
#[derive(Debug, Default)]
pub struct BubbleState {
    pub assets: Mutex<AssetRegistry>,
    pub error: Mutex<Option<String>>,
}

impl BubbleState {
    fn record(&self, error: &str) {
        let mut slot = self.error.lock().unwrap();
        if slot.is_none() {
            *slot = Some(error.to_string());
        }
    }

    pub fn take_error(&self) -> Option<String> {
        self.error.lock().unwrap().take()
    }
}

#[derive(Debug)]
pub struct BubblePlugin {
    source_map: bool,
    state: Arc<BubbleState>,
}

impl BubblePlugin {
    pub fn new(source_map: bool, state: Arc<BubbleState>) -> Self {
        Self { source_map, state }
    }

    fn resolve(
        &self,
        specifier: &str,
        importer: Option<&str>,
    ) -> Result<Option<HookResolveIdOutput>, String> {
        if NATIVE_MODULES.contains(&specifier) {
            return Ok(Some(HookResolveIdOutput {
                id: ArcStr::from(specifier),
                external: Some(ResolvedExternal::Bool(true)),
                ..HookResolveIdOutput::default()
            }));
        }
        if !specifier.starts_with('.') && !specifier.starts_with('/') {
            return Ok(None);
        }
        let base = match importer {
            Some(importer) if Path::new(importer).is_absolute() => Path::new(importer)
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_default(),
            _ => PathBuf::from("."),
        };
        let path = base.join(specifier);
        match resolve_script_or_asset(&path) {
            Some(path) => Ok(Some(HookResolveIdOutput::from_id(
                path.to_string_lossy().into_owned(),
            ))),
            None => Err(format!(
                "{}: cannot resolve {specifier}{}",
                importer.unwrap_or("<entry>"),
                ""
            )),
        }
    }

    fn load_module(&self, id: &str) -> Result<Option<HookLoadOutput>, String> {
        if NATIVE_MODULES.contains(&id) {
            return Err(format!(
                "{id} is provided by the host and must stay external"
            ));
        }
        let path = Path::new(id);
        let extension = path.extension().and_then(|ext| ext.to_str()).unwrap_or("");
        if crate::assets::ASSET_EXTENSIONS.contains(&extension) {
            let bytes =
                std::fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
            let mut registry = self.state.assets.lock().unwrap();
            let exported = registry
                .add(path, bytes)
                .map_err(|conflict| conflict_message(&conflict))?;
            drop(registry);
            let code = format!("export default {};\n", js_string(&exported));
            return Ok(Some(HookLoadOutput {
                code: ArcStr::from(code),
                module_type: Some(ModuleType::Js),
                ..HookLoadOutput::default()
            }));
        }
        if !SCRIPT_EXTENSIONS.contains(&extension) {
            return Err(format!(
                "{}: unsupported import; expected a .tsx, .ts, .jsx, .js, or image file",
                path.display()
            ));
        }
        let source = std::fs::read_to_string(path)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        let compiled = compile_module(
            id,
            &source,
            &CompileOptions {
                source_map: self.source_map,
            },
        )?;
        let map = compiled.map.and_then(|json| {
            serde_json::from_str::<rolldown_sourcemap::JSONSourceMap>(&json)
                .ok()
                .and_then(|value| SourceMap::from_json(value).ok())
        });
        Ok(Some(HookLoadOutput {
            code: ArcStr::from(compiled.code),
            map,
            module_type: Some(ModuleType::Js),
            ..HookLoadOutput::default()
        }))
    }
}

impl Plugin for BubblePlugin {
    fn name(&self) -> Cow<'static, str> {
        Cow::Borrowed("bubble")
    }

    fn register_hook_usage(&self) -> HookUsage {
        HookUsage::ResolveId | HookUsage::Load
    }

    fn resolve_id(
        &self,
        _ctx: &PluginContext,
        args: &HookResolveIdArgs<'_>,
    ) -> impl Future<Output = HookResolveIdReturn> + Send {
        let resolution = self.resolve(args.specifier, args.importer);
        if let Err(error) = &resolution {
            self.state.record(error);
        }
        async move { resolution.map_err(|error| anyhow::anyhow!(error)) }
    }

    fn load(
        &self,
        _ctx: SharedLoadPluginContext,
        args: &HookLoadArgs<'_>,
    ) -> impl Future<Output = HookLoadReturn> + Send {
        let loaded = self.load_module(args.id);
        if let Err(error) = &loaded {
            self.state.record(error);
        }
        async move { loaded.map_err(|error| anyhow::anyhow!(error)) }
    }
}

fn conflict_message(conflict: &AssetConflict) -> String {
    format!(
        "asset name {} is produced by both {} and {} with different contents",
        conflict.path,
        conflict.first.display(),
        conflict.second.display()
    )
}

fn resolve_script_or_asset(path: &Path) -> Option<PathBuf> {
    if path.is_file() {
        return std::fs::canonicalize(path)
            .ok()
            .or_else(|| Some(path.to_path_buf()));
    }
    let extension = path.extension().and_then(|ext| ext.to_str());
    if extension.is_none() {
        for candidate_extension in SCRIPT_EXTENSIONS {
            let candidate = path.with_extension(candidate_extension);
            if candidate.is_file() {
                return std::fs::canonicalize(&candidate).ok().or(Some(candidate));
            }
        }
    }
    None
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
