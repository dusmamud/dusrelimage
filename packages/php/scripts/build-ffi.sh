#!/usr/bin/env bash
# Build the dusrelimage PHP FFI shared library (and the CLI binary the
# PHPUnit tests use to generate fixtures) and put the .so where the
# PHP class expects it.
#
# Usage (from anywhere):
#   packages/php/scripts/build-ffi.sh
#
# What it does:
#   1. cargo build -p dusrelimage-ffi --release
#      (the C API crate; produces target/release/libdusrelimage_ffi.so
#      on Linux, .dylib on macOS, dusrelimage_ffi.dll on Windows)
#   2. cargo build -p dusrelimage-cli --release
#      (the `dusrelimage` binary; the tests call
#      `dusrelimage mock --output … --kind … --width … --height …
#      --seed …` to generate deterministic fixtures — see
#      tests/Fixtures.php)
#   3. Copies the shared library to packages/php/lib/
#      (the first location Dusrelimage::resolveLibPath() looks for,
#      after an explicit path and $DUSRELIMAGE_FFI_LIB).
#
# The .so is a build artifact: it is gitignored and never committed.
# To point the PHP class at a library elsewhere, set:
#   DUSRELIMAGE_FFI_LIB=/path/to/libdusrelimage_ffi.so
#
# cargo is expected on PATH; if it is only at ~/.cargo/bin/cargo
# (the usual rustup install), this script adds that directory.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
PHP_DIR="$ROOT/packages/php"

if ! command -v cargo >/dev/null 2>&1; then
    export PATH="$HOME/.cargo/bin:$PATH"
fi
command -v cargo >/dev/null 2>&1 || {
    echo "error: cargo not found (looked on PATH and in \$HOME/.cargo/bin)" >&2
    exit 1
}

cd "$ROOT"
echo "building dusrelimage-ffi (release)…"
cargo build -p dusrelimage-ffi --release
echo "building dusrelimage-cli (release, for test fixtures)…"
cargo build -p dusrelimage-cli --release

LIB_SRC=""
for candidate in \
    "$ROOT/target/release/libdusrelimage_ffi.so" \
    "$ROOT/target/release/libdusrelimage_ffi.dylib" \
    "$ROOT/target/release/dusrelimage_ffi.dll"; do
    if [ -f "$candidate" ]; then
        LIB_SRC="$candidate"
        break
    fi
done
if [ -z "$LIB_SRC" ]; then
    echo "error: no built dusrelimage-ffi library found under $ROOT/target/release/" >&2
    exit 1
fi

mkdir -p "$PHP_DIR/lib"
cp "$LIB_SRC" "$PHP_DIR/lib/$(basename "$LIB_SRC")"
echo "copied $LIB_SRC -> $PHP_DIR/lib/$(basename "$LIB_SRC")"
echo "CLI binary: $ROOT/target/release/dusrelimage"
echo "done. Run the tests with: cd $PHP_DIR && composer install && vendor/bin/phpunit"
