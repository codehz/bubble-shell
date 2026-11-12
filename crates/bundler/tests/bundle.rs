use std::path::{Path, PathBuf};

use bubble_bundler::{build_dev, bundle_entry, render_dev_bundle, BundleOptions};

struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("bubble-bundler-{name}-{}", std::process::id()));
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

fn bundle(entry: &Path) -> bubble_bundler::BundledEntry {
    bundle_entry(&BundleOptions {
        entry: entry.to_path_buf(),
        source_map: false,
    })
    .unwrap_or_else(|error| panic!("bundle failed:\n{error}"))
}

#[test]
fn bundles_modules_and_assets_into_one_module() {
    let dir = TempDir::new("single");
    dir.write(
        "helper.ts",
        r#"export function helper(value: number): string {
  return `n=${value}`;
}
"#,
    );
    dir.write("logo.png", b"\x89PNG\r\n\x1a\nfake");
    let entry = dir.write(
        "app.tsx",
        r#"import { mount, signal, Text, View } from "bubble-shell";
import { helper } from "./helper";
import logo from "./logo.png";

function App() {
  const count = signal(1);
  return (
    <View>
      <Text>{helper(count())}</Text>
      <Image src={logo} width={64} height={64} />
    </View>
  );
}

mount(App);
"#,
    );

    let bundled = bundle(&entry);
    let code = &bundled.code;
    assert_eq!(bundled.name, "app");
    assert!(code.contains("from \"bubble:rt\""), "{code}");
    assert!(code.contains("from \"bubble-shell\""), "{code}");
    assert!(
        !code.contains("./helper"),
        "module graph is linked:\n{code}"
    );
    assert!(code.contains("function helper"), "{code}");
    assert!(
        !code.contains("registerPrimitive") && !code.contains("class Dynamic"),
        "runtime source must not be bundled:\n{code}"
    );
    assert!(bundled.assets.len() == 1, "{:?}", bundled.assets);
    let asset = &bundled.assets[0];
    assert!(asset.path.starts_with("assets/logo."), "{}", asset.path);
    assert!(asset.path.ends_with(".png"), "{}", asset.path);
    assert_eq!(asset.size, 12);
    assert!(
        code.contains(&asset.path),
        "bundle references the asset:\n{code}"
    );
}

#[test]
fn keeps_native_module_imports_as_specifiers() {
    let dir = TempDir::new("native");
    let entry = dir.write(
        "plain.js",
        r#"import { mount, signal } from "bubble-shell";
const count = signal(0);
mount(count);
"#,
    );
    let bundled = bundle(&entry);
    assert!(
        bundled.code.contains("from \"bubble-shell\""),
        "{}",
        bundled.code
    );
    assert!(!bundled.code.contains("bubble:rt"), "{}", bundled.code);
}

#[test]
fn deduplicates_identical_assets() {
    let dir = TempDir::new("assets");
    let entry = dir.write(
        "app.tsx",
        r#"import { mount, Text } from "bubble-shell";
import one from "./a/logo.png";
import two from "./b/logo.png";
function App() {
  return <Text>{one + two}</Text>;
}
mount(App);
"#,
    );
    dir.write("a/logo.png", b"same");
    dir.write("b/logo.png", b"same");
    let bundled = bundle(&entry);
    assert_eq!(bundled.assets.len(), 1);
    assert!(bundled.code.contains(&bundled.assets[0].path));
    assert!(bundled.code.contains("logo_default$1"), "{}", bundled.code);
}

#[test]
fn reports_compiler_errors_with_the_file_name() {
    let dir = TempDir::new("errors");
    let entry = dir.write(
        "bad.tsx",
        "import { View } from \"bubble-shell\";\nfunction App() { return <View {...{}} />; }\n",
    );
    let error = bundle_entry(&BundleOptions {
        entry,
        source_map: false,
    })
    .unwrap_err();
    assert!(error.contains("bad.tsx"), "{error}");
    assert!(error.contains("spread"), "{error}");
}

#[test]
fn emits_source_maps_only_when_requested() {
    let dir = TempDir::new("maps");
    let entry = dir.write(
        "app.tsx",
        "import { mount, Text } from \"bubble-shell\";\nfunction App() { return <Text>hi</Text>; }\nmount(App);\n",
    );
    let plain = bundle(&entry);
    assert!(plain.source_map.is_none());
    let mapped = bundle_entry(&BundleOptions {
        entry,
        source_map: true,
    })
    .unwrap();
    let map = mapped.source_map.expect("map");
    assert!(map.contains("app.tsx"), "{map}");
    assert!(mapped.code.contains("sourceMappingURL"), "{}", mapped.code);
}

#[test]
fn dev_build_keeps_one_factory_per_module() {
    let dir = TempDir::new("dev");
    dir.write("helper.ts", "export const label = \"hi\";\n");
    let entry = dir.write(
        "app.tsx",
        r#"import { mount, signal, Text, View } from "bubble-shell";
import { label } from "./helper";
const count = signal(1);
function App() {
  return (
    <View>
      <Text>{label}</Text>
      <Text>{count()}</Text>
    </View>
  );
}
mount(App);
"#,
    );
    let build = build_dev(&entry).expect("dev build");
    assert_eq!(build.entry_id, "app.js");
    let ids: Vec<_> = build.chunks.iter().map(|chunk| chunk.id.as_str()).collect();
    assert!(ids.contains(&"app.js"), "{ids:?}");
    assert!(ids.contains(&"helper.js"), "{ids:?}");

    let bundle = render_dev_bundle(&build);
    assert!(bundle.contains("from \"bubble:hot\""), "{bundle}");
    assert!(
        bundle.contains("__bubble_hot.define(\"app.js\""),
        "{bundle}"
    );
    assert!(
        bundle.contains("__bubble_hot.define(\"helper.js\""),
        "{bundle}"
    );
    assert!(bundle.contains("__bubble_hot.run(\"app.js\")"), "{bundle}");
    assert!(
        bundle.contains("require(\"bubble:rt\")"),
        "host modules stay requires:\n{bundle}"
    );
    assert!(bundle.contains("bubble_hot.keep"), "{bundle}");
    assert!(bundle.contains("bubble_hot.component"), "{bundle}");
    assert!(bundle.contains("bubble_hot.mount"), "{bundle}");
    assert!(
        !bundle.contains("registerPrimitive"),
        "runtime source stays out of the dev bundle"
    );
}
