#!/bin/sh
set -eu
cd "$(dirname "$0")/../.."

SDK="${ANDROID_HOME:?}"
BT="$SDK/build-tools/36.1.0"
ANDROID_JAR="$SDK/platforms/android-35/android.jar"
OUT="examples/android/build"
CLASSES="$OUT/classes.jar"
SRC="examples/android/app/src/androidTest/java/dev/bubble/shell/test/ShellInstrumentation.java"
if [ ! -f "$CLASSES" ]; then
  echo "missing $CLASSES; package the app before the test apk" >&2
  exit 1
fi
if [ ! -x "$BT/d8" ]; then
  echo "missing build-tools d8 at $BT" >&2
  exit 1
fi

rm -rf "$OUT/test-classes" "$OUT/test-dex" "$OUT/test-apk"
mkdir -p "$OUT/test-classes" "$OUT/test-dex" "$OUT/test-apk"

javac --release 8 -classpath "$ANDROID_JAR:$CLASSES" -d "$OUT/test-classes" "$SRC"
jar cf "$OUT/test-classes.jar" -C "$OUT/test-classes" .

"$BT/d8" --min-api 24 --lib "$ANDROID_JAR" --output "$OUT/test-dex" "$OUT/test-classes.jar"
"$BT/aapt2" link -o "$OUT/test-base.apk" -I "$ANDROID_JAR" \
  --manifest examples/android/app/src/androidTest/AndroidManifest.xml \
  --min-sdk-version 24 --target-sdk-version 35

rm -rf "$OUT/test-apk"
mkdir -p "$OUT/test-apk"
unzip -q -o "$OUT/test-base.apk" -d "$OUT/test-apk"
cp "$OUT/test-dex/classes.dex" "$OUT/test-apk/classes.dex"
rm -f "$OUT/BubbleShellTests-unsigned.apk" "$OUT/BubbleShellTests-aligned.apk"
(
  cd "$OUT/test-apk"
  zip -qr -n arsc ../BubbleShellTests-unsigned.apk .
)
"$BT/zipalign" -p -f 4 "$OUT/BubbleShellTests-unsigned.apk" "$OUT/BubbleShellTests-aligned.apk"

KEYSTORE="examples/android/debug.keystore"
if [ ! -f "$KEYSTORE" ]; then
  keytool -genkeypair -keystore "$KEYSTORE" -storepass android -keypass android \
    -alias androiddebugkey -keyalg RSA -keysize 2048 -validity 10000 -dname "CN=BubbleShell"
fi
"$BT/apksigner" sign --ks "$KEYSTORE" --ks-pass pass:android --key-pass pass:android \
  --out "$OUT/BubbleShellTests.apk" "$OUT/BubbleShellTests-aligned.apk"
echo "$OUT/BubbleShellTests.apk"
