#!/bin/sh
set -eu
cd "$(dirname "$0")/../.."

sh sdk/android/package-aar.sh
eval "$(sh sdk/android/env.sh)"

sh examples/build-fixtures.sh

OUT="examples/android/build"
SDK_DIR="dist/android"
rm -rf "$OUT"
mkdir -p "$OUT/classes" "$OUT/app" "$OUT/apk/assets" "$OUT/apk/lib/arm64-v8a"

cp -R examples/web/fixtures "$OUT/apk/assets/fixtures"
FONT="/System/Library/Fonts/Supplemental/Arial.ttf"
if [ -f "$FONT" ]; then
  cp "$FONT" "$OUT/apk/assets/HostFont.ttf"
fi
cp "$SDK_DIR/jni/arm64-v8a/libbubble_ffi.so" "$OUT/apk/lib/arm64-v8a/libbubble_ffi.so"

"$KOTLINC" -jvm-target 17 -classpath "$ANDROID_JAR:$SDK_DIR/classes.jar" \
  -d "$OUT/app" \
  examples/android/app/src/main/java/dev/bubble/shell/MainActivity.kt
(
  cd "$OUT/classes"
  jar xf "../../../$SDK_DIR/classes.jar"
  cp -R ../app/* .
  rm -rf META-INF
  jar cf ../classes.jar .
)
"$BUILD_TOOLS/d8" --min-api 24 --lib "$ANDROID_JAR" --output "$OUT/apk" "$OUT/classes.jar" \
  "$KOTLIN_LIB/kotlin-stdlib.jar"

"$BUILD_TOOLS/aapt2" link -o "$OUT/base.apk" -I "$ANDROID_JAR" \
  --manifest examples/android/app/src/main/AndroidManifest.xml \
  --min-sdk-version 24 --target-sdk-version "$ANDROID_PLATFORM"
unzip -q -o "$OUT/base.apk" -d "$OUT/apk"
rm -f "$OUT/BubbleShell-unsigned.apk"
(
  cd "$OUT/apk"
  zip -qr -n arsc ../BubbleShell-unsigned.apk .
)
"$BUILD_TOOLS/zipalign" -p -f 4 "$OUT/BubbleShell-unsigned.apk" "$OUT/BubbleShell-aligned.apk"
KEYSTORE="examples/android/debug.keystore"
if [ ! -f "$KEYSTORE" ]; then
  keytool -genkeypair -keystore "$KEYSTORE" -storepass android -keypass android \
    -alias androiddebugkey -keyalg RSA -keysize 2048 -validity 10000 -dname "CN=BubbleShell"
fi
"$BUILD_TOOLS/apksigner" sign --ks "$KEYSTORE" --ks-pass pass:android --key-pass pass:android \
  --out "$OUT/BubbleShell.apk" "$OUT/BubbleShell-aligned.apk"
echo "$OUT/BubbleShell.apk"
