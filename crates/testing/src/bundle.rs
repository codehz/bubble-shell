//! One bundled test fixture app, built in-process once and cached.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock, Mutex};

use bubble_bundler::{bundle_entry, BundleOptions};

use crate::manifest::manifest_json;

/// A built fixture: the manifest JSON and entry bytes `bubble_load` consumes.
pub struct App {
    pub name: String,
    pub manifest: String,
    pub entry: Vec<u8>,
}

fn apps_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("apps")
}

static APPS: LazyLock<Mutex<HashMap<String, Arc<App>>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Build (once per process) and return the fixture named `apps/<name>.tsx`.
pub fn app(name: &str) -> Result<Arc<App>, String> {
    let mut cache = APPS.lock().expect("app cache locks");
    if let Some(built) = cache.get(name) {
        return Ok(Arc::clone(built));
    }
    let entry = apps_dir().join(format!("{name}.tsx"));
    if !entry.is_file() {
        return Err(format!("no test app {name} at {}", entry.display()));
    }
    let bundled = bundle_entry(&BundleOptions {
        entry,
        source_map: false,
    })?;
    let built = Arc::new(App {
        name: name.to_string(),
        manifest: manifest_json(&bundled.name, &bundled.code, &bundled.assets),
        entry: bundled.code.into_bytes(),
    });
    cache.insert(name.to_string(), Arc::clone(&built));
    Ok(built)
}
