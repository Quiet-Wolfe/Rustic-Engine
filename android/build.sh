#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
MANIFEST="$SCRIPT_DIR/AndroidManifest.xml"
PLATFORM="/opt/android-sdk/platforms/android-37.0/android.jar"
OUTPUT_DIR="$PROJECT_DIR/target/android-apk"
LIB_SO="$PROJECT_DIR/target/aarch64-linux-android/release/librustic_app.so"

export ANDROID_NDK_HOME="${ANDROID_NDK_HOME:-/opt/android-ndk}"

# Step 1: Build native library
echo "=== Building native library (aarch64) ==="
cd "$PROJECT_DIR"
cargo ndk -t arm64-v8a build -p rustic-app --release --lib

if [ ! -f "$LIB_SO" ]; then
    echo "ERROR: $LIB_SO not found"
    exit 1
fi

echo "=== Library size: $(du -h "$LIB_SO" | cut -f1) ==="

# Step 2: Create APK staging directory
echo "=== Packaging APK ==="
rm -rf "$OUTPUT_DIR/staging"
mkdir -p "$OUTPUT_DIR/staging/lib/arm64-v8a"
cp "$LIB_SO" "$OUTPUT_DIR/staging/lib/arm64-v8a/"

# Step 3: Create base APK (manifest only) with aapt2, then add .so via zip
aapt2 link \
    -o "$OUTPUT_DIR/rustic.unsigned.apk" \
    --manifest "$MANIFEST" \
    -I "$PLATFORM" \
    --min-sdk-version 28 \
    --target-sdk-version 35

# Add native library to APK
cd "$OUTPUT_DIR/staging"
zip -r "$OUTPUT_DIR/rustic.unsigned.apk" lib/
cd "$PROJECT_DIR"

# Step 4: Align
zipalign -f 4 "$OUTPUT_DIR/rustic.unsigned.apk" "$OUTPUT_DIR/rustic.aligned.apk"

# Step 5: Sign
apksigner sign \
    --ks ~/.android/debug.keystore \
    --ks-pass pass:android \
    --key-pass pass:android \
    --out "$OUTPUT_DIR/rustic.apk" \
    "$OUTPUT_DIR/rustic.aligned.apk"

echo "=== APK ready: $OUTPUT_DIR/rustic.apk ==="
echo "=== APK size: $(du -h "$OUTPUT_DIR/rustic.apk" | cut -f1) ==="

# Optional: install if --install flag passed
if [ "$1" = "--install" ]; then
    echo "=== Installing via ADB ==="
    adb install -r "$OUTPUT_DIR/rustic.apk"
    echo "=== Launching ==="
    adb shell am start -n com.rustic.engine/android.app.NativeActivity
fi
