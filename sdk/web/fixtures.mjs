// Inputs the wasm-shell tests need: the built `bubble_wasm.wasm` and one
// bundled test app. Both come from build outputs, so nothing generated is
// committed.
//
// - wasm: `cargo build -p bubble-wasm --target wasm32-unknown-unknown --release`
// - app bundle: `bubble bundle crates/testing/apps/<name>.tsx`, the same
//   in-process bundling `cargo test` uses, cached under `target/`.
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";

const root = new URL("../..", import.meta.url);

export function wasmBytes() {
  return readFileSync(new URL("target/wasm32-unknown-unknown/release/bubble_wasm.wasm", root));
}

export function fixture(name) {
  execFileSync("cargo", [
    "run",
    "-q",
    "-p",
    "bubble-cli",
    "--",
    "bundle",
    `crates/testing/apps/${name}.tsx`,
    "--out",
    "target/bubble-web-fixtures",
  ], { cwd: root, stdio: "inherit" });
  return {
    manifest: readFileSync(new URL(`target/bubble-web-fixtures/${name}.json`, root)),
    entry: readFileSync(new URL(`target/bubble-web-fixtures/${name}.js`, root)),
  };
}

export function entryUrl(name) {
  return new URL(`target/bubble-web-fixtures/${name}.js`, root).href;
}
