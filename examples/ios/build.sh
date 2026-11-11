#!/bin/sh
set -eu
cd "$(dirname "$0")/../.."
cargo build -p bubble-ffi --release --target aarch64-apple-ios-sim
SDK="$(xcrun --sdk iphonesimulator --show-sdk-path)"
OUT="examples/ios/BubbleShell.app"
rm -rf "$OUT"
mkdir -p "$OUT"
cp examples/ios/Info.plist "$OUT/Info.plist"
cp examples/web/bundle/manifest.json "$OUT/manifest.json"
cp examples/web/bundle/main.js "$OUT/main.js"
FONT="/System/Library/Fonts/Supplemental/Arial.ttf"
if [ -f "$FONT" ]; then
  cp "$FONT" "$OUT/HostFont.ttf"
fi
swiftc -sdk "$SDK" -target arm64-apple-ios17.0-simulator \
  -O -framework UIKit -framework CoreText -framework CoreGraphics \
  -L target/aarch64-apple-ios-sim/release -lbubble_ffi \
  -import-objc-header /dev/null \
  -o "$OUT/BubbleShell" \
  examples/ios/Sources/main.swift
echo "$OUT"
