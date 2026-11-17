fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let wasm = std::path::Path::new(&manifest_dir)
        .join("../../target/wasm32-unknown-unknown/release/bubble_wasm.wasm");
    let wasm = wasm.canonicalize().unwrap_or(wasm);
    println!("cargo:rerun-if-changed={}", wasm.display());
    if !wasm.is_file() {
        panic!(
            "wasm not found; run: cargo build -p bubble-wasm --target wasm32-unknown-unknown --release"
        );
    }
}
