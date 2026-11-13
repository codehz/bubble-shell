#!/bin/sh
set -eu
cd "$(dirname "$0")/.."

OUT="examples/web/fixtures"
rm -rf "$OUT"
mkdir -p "$OUT"

cargo run -q -p bubble-cli -- bundle \
  examples/app/fixtures/counter.tsx \
  examples/app/fixtures/editing.tsx \
  examples/app/fixtures/focus.tsx \
  examples/app/fixtures/media.tsx \
  examples/app/gestures.js \
  --out "$OUT"

# The runtime has one source of truth; Web serves the same file the native
# loader embeds.
cp crates/native/js/runtime.js examples/web/runtime.js
cp crates/native/js/runtime.js sdk/web/runtime.js
