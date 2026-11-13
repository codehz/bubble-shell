#!/bin/sh
set -eu
cd "$(dirname "$0")/../.."
cargo build -p bubble-ffi --release --target aarch64-apple-ios-sim
OUT="dist/BubbleShell.xcframework"
rm -rf "$OUT"
xcodebuild -create-xcframework \
  -library target/aarch64-apple-ios-sim/release/libbubble_ffi.a \
  -headers crates/native/include \
  -output "$OUT"
echo "$OUT"
