#!/bin/sh
set -eu
cd "$(dirname "$0")/.."

OUT="examples/web/fixtures"
rm -rf "$OUT"
mkdir -p "$OUT"

for source in examples/app/fixtures/*.tsx; do
  name="$(basename "$source" .tsx)"
  cargo run -q -p bubble-compiler -- "$source" "$OUT/$name.js"
done

cp examples/app/gestures.js "$OUT/gestures.js"

python3 - "$OUT" <<'PY'
import hashlib
import json
import sys
from pathlib import Path

root = Path(sys.argv[1])
for entry in sorted(root.glob("*.js")):
    name = entry.stem
    data = entry.read_bytes()
    manifest = {
        "schema": 1,
        "id": f"com.bubble.{name}",
        "version": 1,
        "minRuntime": "0.1.0",
        "platforms": ["web", "ios", "android"],
        "entry": entry.name,
        "files": [{
            "path": entry.name,
            "sha256": hashlib.sha256(data).hexdigest(),
            "size": len(data),
        }],
        "keyId": "m0-placeholder",
        "signature": "m0-placeholder",
    }
    (root / f"{name}.json").write_text(json.dumps(manifest, indent=2) + "\n")
PY
