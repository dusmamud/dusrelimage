#!/usr/bin/env bash
# Build the dusrelimage C FFI library for the Go binding.
#
#   bash packages/go/scripts/build-ffi.sh
#
# Runs `cargo build -p dusrelimage-ffi --release` from the repo root
# and leaves the static library at
#   target/release/libdusrelimage_ffi.a   (Linux/macOS)
#   target/release/dusrelimage_ffi.lib    (Windows)
# (the cdylib is libdusrelimage_ffi.so/.dylib/.dll). The lib is named
# `dusrelimage_ffi`, not `dusrelimage`, so it does not collide with
# the core crate of the same name. The Go package links the static
# library with `#cgo LDFLAGS` (see ffi.go).
#
# No prebuilt binary is ever downloaded: the library is always built
# from the Rust sources in this repo. Requires a Rust toolchain
# (cargo on PATH, or ~/.cargo/bin/cargo).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"

if ! command -v cargo >/dev/null 2>&1; then
    if [ -x "$HOME/.cargo/bin/cargo" ]; then
        export PATH="$HOME/.cargo/bin:$PATH"
    else
        echo "build-ffi.sh: cargo not found (install Rust: https://rustup.rs)" >&2
        exit 1
    fi
fi

cargo build -p dusrelimage-ffi --release --manifest-path "$ROOT/Cargo.toml"

case "$(uname -s)" in
    MINGW*|MSYS*|CYGWIN*)
        LIB="$ROOT/target/release/dusrelimage_ffi.lib"
        ;;
    *)
        LIB="$ROOT/target/release/libdusrelimage_ffi.a"
        ;;
esac

if [ ! -f "$LIB" ]; then
    echo "build-ffi.sh: expected library not found: $LIB" >&2
    exit 1
fi

echo "build-ffi.sh: built $LIB"
