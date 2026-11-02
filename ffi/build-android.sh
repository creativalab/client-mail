#!/usr/bin/env bash
# Builds the engine for Android and generates its Kotlin bindings.
#
#   ffi/build-android.sh [debug|release] [OUT_DIR]
#
# Output (default OUT_DIR = android/core/src/main):
#   jniLibs/<abi>/libferrowl_ffi.so   for arm64-v8a, armeabi-v7a, x86_64
#   kotlin/com/ferrowl/core/ferrowl_ffi.kt
#
# Needs: the Android NDK (ANDROID_NDK_HOME, or the newest one under
# $ANDROID_HOME/ndk), the Rust Android targets and cargo-ndk:
#   rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android
#   cargo install cargo-ndk
set -euo pipefail
cd "$(dirname "$0")/.."

PROFILE=${1:-release}
OUT=${2:-android/core/src/main}

if [ -z "${ANDROID_NDK_HOME:-}" ]; then
  ndk_root="${ANDROID_HOME:-$HOME/Android/Sdk}/ndk"
  ANDROID_NDK_HOME=$(ls -d "$ndk_root"/* 2>/dev/null | sort -V | tail -1 || true)
  export ANDROID_NDK_HOME
fi
[ -d "${ANDROID_NDK_HOME:-}" ] || { echo "Android NDK not found: install it from Android Studio (SDK Manager > SDK Tools > NDK)." >&2; exit 1; }

flag=()
[ "$PROFILE" = release ] && flag=(--release)

# minSdk of the app (Android 8.0).
cargo ndk --platform 26 \
  -t arm64-v8a -t armeabi-v7a -t x86_64 \
  -o "$OUT/jniLibs" \
  build -p ferrowl-ffi --locked "${flag[@]}"

# Bindings come from the library's metadata (any ABI will do);
# ffi/uniffi.toml (package com.ferrowl.core) is read automatically.
cargo run -q -p ferrowl-ffi --features bindgen --bin uniffi-bindgen -- \
  generate --library "$OUT/jniLibs/arm64-v8a/libferrowl_ffi.so" \
  --language kotlin --no-format \
  --out-dir "$OUT/kotlin"
echo "Android engine ready in $OUT"
