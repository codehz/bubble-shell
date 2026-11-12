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
if [ ! -x "$KOTLIN_HOME/bin/kotlinc" ]; then
  mkdir -p "$(dirname "$KOTLIN_HOME")"
  curl -fsSL -o /tmp/kotlin-compiler.zip \
    https://github.com/JetBrains/kotlin/releases/download/v2.0.21/kotlin-compiler-2.0.21.zip
  rm -rf /tmp/kotlinc
  unzip -q /tmp/kotlin-compiler.zip -d /tmp
  rm -rf "$KOTLIN_HOME"
  mv /tmp/kotlinc "$KOTLIN_HOME"
fi

SDK="$ANDROID_HOME"
BT="$SDK/build-tools/36.1.0"
ANDROID_JAR="$SDK/platforms/android-35/android.jar"
sh examples/build-fixtures.sh

OUT="examples/android/build"
rm -rf "$OUT"
mkdir -p "$OUT/classes" "$OUT/apk/assets" "$OUT/apk/lib/arm64-v8a"

cp -R examples/web/fixtures "$OUT/apk/assets/fixtures"
FONT="/System/Library/Fonts/Supplemental/Arial.ttf"
if [ -f "$FONT" ]; then
  cp "$FONT" "$OUT/apk/assets/HostFont.ttf"
fi
cp target/aarch64-linux-android/release/libbubble_ffi.so "$OUT/apk/lib/arm64-v8a/libbubble_ffi.so"

"$KOTLIN_HOME/bin/kotlinc" -jvm-target 17 -classpath "$ANDROID_JAR" -d "$OUT/classes.jar" \
  examples/android/app/src/main/java/dev/bubble/shell/*.kt
"$BT/d8" --min-api 24 --lib "$ANDROID_JAR" --output "$OUT/apk" "$OUT/classes.jar" \
  "$KOTLIN_HOME/lib/kotlin-stdlib.jar"

"$BT/aapt2" link -o "$OUT/base.apk" -I "$ANDROID_JAR" \
  --manifest examples/android/app/src/main/AndroidManifest.xml \
  --min-sdk-version 24 --target-sdk-version 35
unzip -q -o "$OUT/base.apk" -d "$OUT/apk"
rm -f "$OUT/BubbleShell-unsigned.apk"
(
  cd "$OUT/apk"
  zip -qr -n arsc ../BubbleShell-unsigned.apk .
)
"$BT/zipalign" -p -f 4 "$OUT/BubbleShell-unsigned.apk" "$OUT/BubbleShell-aligned.apk"
KEYSTORE="examples/android/debug.keystore"
if [ ! -f "$KEYSTORE" ]; then
  keytool -genkeypair -keystore "$KEYSTORE" -storepass android -keypass android \
    -alias androiddebugkey -keyalg RSA -keysize 2048 -validity 10000 -dname "CN=BubbleShell"
fi
 "$BT/apksigner" sign --ks "$KEYSTORE" --ks-pass pass:android --key-pass pass:android \
  --out "$OUT/BubbleShell.apk" "$OUT/BubbleShell-aligned.apk"
echo "$OUT/BubbleShell.apk"
