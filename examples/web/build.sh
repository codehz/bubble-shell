#!/bin/sh
set -eu
cd "$(dirname "$0")/../.."

cargo build -p bubble-wasm --target wasm32-unknown-unknown --release
cp target/wasm32-unknown-unknown/release/bubble_wasm.wasm examples/web/bubble.wasm

mkdir -p examples/web/bundle
cargo run -q -p bubble-compiler -- examples/app/counter.tsx examples/web/bundle/main.js

python3 - <<'PY'
import hashlib, json
from pathlib import Path
root = Path("examples/web/bundle")
data = (root / "main.js").read_bytes()
manifest = {
    "schema": 1,
    "id": "com.bubble.counter",
    "version": 1,
    "minRuntime": "0.1.0",
    "platforms": ["web"],
    "entry": "main.js",
    "files": [{
        "path": "main.js",
        "sha256": hashlib.sha256(data).hexdigest(),
        "size": len(data),
    }],
    "keyId": "m0-placeholder",
    "signature": "m0-placeholder",
}
(root / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
PY
