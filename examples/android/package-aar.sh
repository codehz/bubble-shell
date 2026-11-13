#!/bin/sh
set -eu
cd "$(dirname "$0")/../.."

NDK="${ANDROID_HOME:?}/ndk/27.2.12479018"
PRE="$NDK/toolchains/llvm/prebuilt/darwin-arm64"
if [ ! -d "$PRE" ]; then
  PRE="$NDK/toolchains/llvm/prebuilt/darwin-x86_64"
fi
export CC_aarch64_linux_android="$PRE/bin/aarch64-linux-android24-clang"
export AR_aarch64_linux_android="$PRE/bin/llvm-ar"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$CC_aarch64_linux_android"
export BINDGEN_EXTRA_CLANG_ARGS_aarch64_linux_android="--sysroot=$PRE/sysroot"
cargo build -p bubble-ffi --release --target aarch64-linux-android

KOTLIN_HOME="${KOTLIN_HOME:-$HOME/.cache/bubble-kotlinc}"
SDK="${ANDROID_HOME:?}"
ANDROID_JAR="$SDK/platforms/android-35/android.jar"
OUT="dist/android"
rm -rf "$OUT"
mkdir -p "$OUT/jni/arm64-v8a"
cp target/aarch64-linux-android/release/libbubble_ffi.so "$OUT/jni/arm64-v8a/libbubble_ffi.so"
cat > "$OUT/AndroidManifest.xml" <<'EOF'
<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android" package="dev.bubble.shell">
  <uses-sdk android:minSdkVersion="24" />
</manifest>
EOF
"$KOTLIN_HOME/bin/kotlinc" -jvm-target 17 -classpath "$ANDROID_JAR" -d "$OUT/classes.jar" \
  examples/android/app/src/main/java/dev/bubble/shell/BubbleNative.kt \
  examples/android/app/src/main/java/dev/bubble/shell/BubbleView.kt \
  examples/android/app/src/main/java/dev/bubble/shell/HostCalls.kt \
  examples/android/app/src/main/java/dev/bubble/shell/FieldLayout.kt
(
  cd "$OUT"
  zip -qr ../bubble-shell.aar AndroidManifest.xml classes.jar jni
)
echo "dist/bubble-shell.aar"
