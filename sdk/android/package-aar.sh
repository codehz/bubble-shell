#!/bin/sh
set -eu
cd "$(dirname "$0")/../.."
eval "$(sh sdk/android/env.sh)"

export CC_aarch64_linux_android="$CC"
export AR_aarch64_linux_android="$AR"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$CC"
export BINDGEN_EXTRA_CLANG_ARGS_aarch64_linux_android="--sysroot=$SYSROOT"
cargo build -p bubble-ffi --release --target aarch64-linux-android

OUT="dist/android"
rm -rf "$OUT"
mkdir -p "$OUT/jni/arm64-v8a"
cp target/aarch64-linux-android/release/libbubble_ffi.so "$OUT/jni/arm64-v8a/libbubble_ffi.so"
cat > "$OUT/AndroidManifest.xml" <<EOF
<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android" package="dev.bubble.shell">
  <uses-sdk android:minSdkVersion="24" />
</manifest>
EOF
"$KOTLINC" -jvm-target 17 -classpath "$ANDROID_JAR" -d "$OUT/classes.jar" \
  sdk/android/src/dev/bubble/shell/*.kt
(
  cd "$OUT"
  zip -qr ../bubble-shell.aar AndroidManifest.xml classes.jar jni
)
echo "dist/bubble-shell.aar"
