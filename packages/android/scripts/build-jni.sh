#!/usr/bin/env bash
# Build the JNI shared libraries for Android with cargo-ndk.
#
# Exact command (documented here and in packages/android/README.md):
#   cargo ndk -t arm64-v8a -t armeabi-v7a -t x86_64 --platform 24 \
#     -o packages/android/lib/src/main/jniLibs \
#     build --release -p dusrelimage-uniffi
#
# Requirements (CI installs all of them; see .github/workflows/mobile.yml):
# - cargo-ndk 4.1.2 (`cargo install cargo-ndk --version 4.1.2 --locked`)
# - Rust targets aarch64-linux-android, armv7-linux-androideabi,
#   x86_64-linux-android (`rustup target add ...`)
# - ANDROID_NDK_HOME pointing at the Android NDK (r26 or later)
# - `--platform 24` matches the library minSdk (24)
#
# Output: lib/src/main/jniLibs/<abi>/libdusrelimage_uniffi.so — built
# in CI, NEVER committed (see packages/android/.gitignore rules in the
# repo root .gitignore; the .so files are build artefacts).
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
OUT_DIR="$REPO_ROOT/packages/android/lib/src/main/jniLibs"

: "${ANDROID_NDK_HOME:?ANDROID_NDK_HOME must point at the Android NDK}"

rm -rf "$OUT_DIR"
cargo ndk -t arm64-v8a -t armeabi-v7a -t x86_64 --platform 24 \
    -o "$OUT_DIR" \
    build --release -p dusrelimage-uniffi

echo "JNI libs built in $OUT_DIR:"
find "$OUT_DIR" -name '*.so' | sort
