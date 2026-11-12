//! `bubble` command line: build fixture bundles.

use std::path::PathBuf;
use std::process::ExitCode;

use bubble_bundler::{bundle_entry, content_hash, BundleOptions};

const DEFAULT_MIN_RUNTIME: &str = "0.1.0";
const PLATFORMS: &[&str] = &["web", "ios", "android"];

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("bundle") => bundle_command(&args[1..]),
        Some(other) => Err(format!(
            "unknown command {other}; expected `bubble bundle <entries...> --out <dir>`"
        )),
        None => Err(usage()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("bubble: {error}");
            ExitCode::FAILURE
        }
    }
}

fn usage() -> String {
    "usage: bubble bundle <entries...> --out <dir> [--id <id>] [--dev]".into()
}

#[derive(Default)]
struct BundleArgs {
    entries: Vec<PathBuf>,
    out: Option<PathBuf>,
    id: Option<String>,
    dev: bool,
}

fn parse_bundle_args(args: &[String]) -> Result<BundleArgs, String> {
    let mut parsed = BundleArgs::default();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--out" => {
                index += 1;
                let value = args.get(index).ok_or("--out needs a directory")?;
                parsed.out = Some(PathBuf::from(value));
            }
            "--id" => {
                index += 1;
                let value = args.get(index).ok_or("--id needs a value")?;
                parsed.id = Some(value.clone());
            }
            "--dev" => parsed.dev = true,
            other if other.starts_with("--") => {
                return Err(format!("unknown option {other}"));
            }
            entry => parsed.entries.push(PathBuf::from(entry)),
        }
        index += 1;
    }
    if parsed.entries.is_empty() {
        return Err(usage());
    }
    if parsed.id.is_some() && parsed.entries.len() > 1 {
        return Err("--id only applies to a single entry".into());
    }
    Ok(parsed)
}

fn bundle_command(args: &[String]) -> Result<(), String> {
    let parsed = parse_bundle_args(args)?;
    let out = parsed.out.ok_or_else(usage)?;
    std::fs::create_dir_all(&out).map_err(|error| format!("{}: {error}", out.display()))?;

    let mut names = std::collections::HashSet::new();
    for entry in &parsed.entries {
        let bundled = bundle_entry(&BundleOptions {
            entry: entry.clone(),
            source_map: parsed.dev,
        })?;
        if !names.insert(bundled.name.clone()) {
            return Err(format!(
                "{}: duplicate entry name {}",
                entry.display(),
                bundled.name
            ));
        }
        let bundle_path = out.join(format!("{}.js", bundled.name));
        std::fs::write(&bundle_path, &bundled.code)
            .map_err(|error| format!("{}: {error}", bundle_path.display()))?;
        if let Some(map) = &bundled.source_map {
            let map_path = out.join(format!("{}.js.map", bundled.name));
            std::fs::write(&map_path, map)
                .map_err(|error| format!("{}: {error}", map_path.display()))?;
        }
        for asset in &bundled.assets {
            let path = out.join(&asset.path);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| format!("{}: {error}", parent.display()))?;
            }
            let unchanged = std::fs::read(&path).is_ok_and(|existing| existing == asset.bytes);
            if !unchanged {
                std::fs::write(&path, &asset.bytes)
                    .map_err(|error| format!("{}: {error}", path.display()))?;
            }
        }
        if !parsed.dev {
            let manifest = manifest_json(
                parsed.id.as_deref(),
                &bundled.name,
                &bundled.code,
                &bundled.assets,
            );
            let manifest_path = out.join(format!("{}.json", bundled.name));
            std::fs::write(&manifest_path, manifest)
                .map_err(|error| format!("{}: {error}", manifest_path.display()))?;
        }
        println!("{}.js ({} bytes)", bundled.name, bundled.code.len());
    }
    Ok(())
}

fn manifest_json(
    id: Option<&str>,
    name: &str,
    code: &str,
    assets: &[bubble_bundler::BundledAsset],
) -> String {
    let id = id
        .map(str::to_string)
        .unwrap_or_else(|| format!("com.bubble.{name}"));
    let files = vec![serde_json::json!({
        "path": format!("{name}.js"),
        "sha256": content_hash(code.as_bytes()),
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
    let manifest = serde_json::json!({
        "schema": 1,
        "id": id,
        "version": 1,
        "minRuntime": DEFAULT_MIN_RUNTIME,
        "platforms": PLATFORMS,
        "entry": format!("{name}.js"),
        "files": files,
        "assets": assets,
        "keyId": "m0-placeholder",
        "signature": "m0-placeholder",
    });
    format!(
        "{}\n",
        serde_json::to_string_pretty(&manifest).expect("manifest serializes")
    )
}
