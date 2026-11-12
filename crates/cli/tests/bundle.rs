use std::path::{Path, PathBuf};
use std::process::Command;

struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("bubble-cli-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn write(&self, name: &str, contents: impl AsRef<[u8]>) -> PathBuf {
        let path = self.0.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, contents).unwrap();
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn bundle(entry: &Path, out: &Path) {
    let status = Command::new(env!("CARGO_BIN_EXE_bubble"))
        .arg("bundle")
        .arg(entry)
        .arg("--out")
        .arg(out)
        .status()
        .unwrap();
    assert!(status.success());
}

#[test]
fn writes_bundle_manifest_and_assets() {
    let dir = TempDir::new("manifest");
    dir.write("logo.png", b"\x89PNG\r\n\x1a\nasset");
    let entry = dir.write(
        "app.tsx",
        r#"import { mount, Text, View } from "bubble-shell";
import logo from "./logo.png";
function App() {
  return <View><Text>{logo}</Text></View>;
}
mount(App);
"#,
    );
    let out = dir.0.join("out");
    bundle(&entry, &out);

    let code = std::fs::read(out.join("app.js")).unwrap();
    assert!(
        String::from_utf8_lossy(&code).contains("from \"bubble-shell\""),
        "bundle keeps host imports"
    );
    assert!(
        !out.join("app.js.map").exists(),
        "release writes no source map"
    );

    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(out.join("app.json")).unwrap()).unwrap();
    assert_eq!(manifest["schema"], 1);
    assert_eq!(manifest["id"], "com.bubble.app");
    assert_eq!(manifest["version"], 1);
    assert_eq!(manifest["minRuntime"], "0.1.0");
    assert_eq!(
        manifest["platforms"],
        serde_json::json!(["web", "ios", "android"])
    );
    assert_eq!(manifest["entry"], "app.js");
    assert_eq!(manifest["files"][0]["path"], "app.js");
    assert_eq!(manifest["files"][0]["size"], code.len() as u64);
    assert_eq!(manifest["files"][0]["sha256"], hex(&code));
    assert!(manifest["signature"]
        .as_str()
        .is_some_and(|s| !s.is_empty()));

    let asset = manifest["assets"][0]["path"].as_str().unwrap();
    assert!(asset.starts_with("assets/logo."), "{asset}");
    assert!(asset.ends_with(".png"), "{asset}");
    let bytes = std::fs::read(out.join(asset)).unwrap();
    assert_eq!(manifest["assets"][0]["size"], bytes.len() as u64);
    assert_eq!(manifest["assets"][0]["sha256"], hex(&bytes));
}
