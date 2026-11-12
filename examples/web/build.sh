#!/bin/sh
set -eu
cd "$(dirname "$0")/../.."

cargo build -p bubble-wasm --target wasm32-unknown-unknown --release
cp target/wasm32-unknown-unknown/release/bubble_wasm.wasm examples/web/bubble.wasm
sh examples/build-fixtures.sh
