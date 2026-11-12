#!/usr/bin/env bash
# C smoke test runner for the dusrelimage FFI library.
#
# Builds the cdylib, generates a deterministic fixture PNG with the
# CLI `mock` command, compiles `smoke.c` against the built cdylib +
# `dusrelimage.h` with gcc, and runs it. No network, no installs.
#
# Usage: ./run_c_smoke.sh   (from anywhere; paths resolve to the repo root)
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
export PATH="$HOME/.cargo/bin:$PATH"

cd "$ROOT"
cargo build -p dusrelimage-ffi
cargo build -p dusrelimage-cli --offline 2>/dev/null || cargo build -p dusrelimage-cli

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

./target/debug/dusrelimage mock --output "$TMP/fixture.png" \
    --kind gradient --width 100 --height 80 --seed 3

gcc -std=c11 -Wall -Wextra -O2 \
    -I crates/dusrelimage-ffi \
    crates/dusrelimage-ffi/tests/c_smoke/smoke.c \
    -o "$TMP/smoke" \
    -L target/debug -ldusrelimage_ffi \
    -Wl,-rpath,"$ROOT/target/debug"

LD_LIBRARY_PATH="$ROOT/target/debug" "$TMP/smoke" "$TMP/fixture.png"
