#!/bin/sh
# Print the Android tools found on this machine. Override with ANDROID_NDK_HOME or KOTLIN_HOME.
set -eu
SDK="${ANDROID_HOME:?ANDROID_HOME is unset}"

if [ -n "${ANDROID_NDK_HOME:-}" ]; then
  NDK="$ANDROID_NDK_HOME"
else
  version=$(find "$SDK/ndk" -mindepth 1 -maxdepth 1 -type d -exec basename {} \; 2>/dev/null | sort -t. -k1,1n -k2,2n -k3,3n | tail -n 1)
  NDK="$SDK/ndk/$version"
fi
if [ ! -d "$NDK" ]; then
  echo "Android NDK not found. Set ANDROID_NDK_HOME or install an NDK under \$ANDROID_HOME/ndk." >&2
  exit 1
fi

case "$(uname -s)-$(uname -m)" in
  Darwin-arm64) host=darwin-arm64; alt=darwin-x86_64 ;;
  Darwin-x86_64) host=darwin-x86_64; alt=darwin-arm64 ;;
  Linux-x86_64) host=linux-x86_64; alt=linux-aarch64 ;;
  Linux-aarch64|Linux-arm64) host=linux-aarch64; alt=linux-x86_64 ;;
  *)
    echo "unsupported NDK host $(uname -s)-$(uname -m)" >&2
    exit 1
    ;;
esac
PRE="$NDK/toolchains/llvm/prebuilt/$host"
if [ ! -d "$PRE" ]; then
  PRE="$NDK/toolchains/llvm/prebuilt/$alt"
fi
CC=$(
  find "$PRE/bin" -name 'aarch64-linux-android[0-9]*-clang' ! -name '*++*' | while read -r file; do
    api=$(basename "$file" | sed -n 's/^aarch64-linux-android\([0-9]*\)-clang$/\1/p')
    printf '%s %s\n' "$api" "$file"
  done | sort -n | tail -n 1 | awk '{print $2}'
)
if [ -z "$CC" ]; then
  echo "aarch64 Android clang not found in $PRE/bin" >&2
  exit 1
fi

tools=$(find "$SDK/build-tools" -mindepth 1 -maxdepth 1 -type d -exec basename {} \; | sort -t. -k1,1n -k2,2n -k3,3n | tail -n 1)
BT="$SDK/build-tools/$tools"
platform=$(find "$SDK/platforms" -mindepth 1 -maxdepth 1 -type d -name 'android-*' -exec basename {} \; | sed 's/^android-//' | sort -n | tail -n 1)
JAR="$SDK/platforms/android-$platform/android.jar"
if [ ! -d "$BT" ] || [ ! -f "$JAR" ]; then
  echo "Android build-tools or platforms are not installed under $SDK" >&2
  exit 1
fi

if [ -n "${KOTLIN_HOME:-}" ]; then
  KOTLINC="$KOTLIN_HOME/bin/kotlinc"
elif command -v kotlinc >/dev/null 2>&1; then
  KOTLINC=$(command -v kotlinc)
elif [ -x "$HOME/.cache/bubble-kotlinc/bin/kotlinc" ]; then
  KOTLINC="$HOME/.cache/bubble-kotlinc/bin/kotlinc"
else
  echo "kotlinc not found. Install Kotlin or set KOTLIN_HOME." >&2
  exit 1
fi
KOTLIN_LIB=$(CDPATH= cd "$(dirname "$KOTLINC")/../lib" && pwd)
if [ ! -f "$KOTLIN_LIB/kotlin-stdlib.jar" ]; then
  echo "kotlin-stdlib.jar not found next to $KOTLINC" >&2
  exit 1
fi

printf 'NDK_PRE=%s\n' "$PRE"
printf 'CC=%s\n' "$CC"
printf 'AR=%s\n' "$PRE/bin/llvm-ar"
printf 'SYSROOT=%s\n' "$PRE/sysroot"
printf 'BUILD_TOOLS=%s\n' "$BT"
printf 'ANDROID_JAR=%s\n' "$JAR"
printf 'ANDROID_PLATFORM=%s\n' "$platform"
printf 'KOTLINC=%s\n' "$KOTLINC"
printf 'KOTLIN_LIB=%s\n' "$KOTLIN_LIB"
