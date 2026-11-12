# dusrelimage-uniffi — UniFFI bindings (Kotlin/Swift)

Thin translators over the core `dusrelimage` crate — no image logic
lives here. The interface is `src/dusrelimage.udl`; it is the shared
contract for the Kotlin (Android) and Swift (iOS) wrappers, so the
UDL — not this file — is the source of truth for the cross-language
API. `src/lib.rs` converts between the UDL shapes and the core types
where the UDL cannot express a Rust type (see the UDL header).

## What is in this crate

| File | What it is |
|---|---|
| `src/dusrelimage.udl` | The UniFFI interface: 13 functions + all records/enums/error. Read the header comment first — it documents the UDL syntax rules and every divergence from the Rust types. |
| `src/lib.rs` | Rust side of the UDL: type conversions + the 13 functions + `make_test_image` / `version` extras + 21 unit tests. |
| `build.rs` | Runs `uniffi::generate_scaffolding` on the UDL (required — without it `$OUT_DIR` is never set and the crate does not compile). |
| `src/bin/uniffi-bindgen.rs` | The `uniffi-bindgen` binary (see below). |
| `uniffi.toml` | Bindgen config: Kotlin `package_name = "com.dusrel.image"`, `cdylib_name = "dusrelimage_uniffi"`. |

## UDL syntax rules (UniFFI 0.29 / weedle2 — this is WebIDL, not Rust)

The previous version of this UDL did not compile. These are the rules
that fixed it — do not "simplify" the UDL back:

1. **Throwing functions use the `[Throws=DusrelError]` attribute** on
   the line above the function. The Rust-style `throws X` suffix after
   the signature does **not** parse (weedle2 has no `throws` keyword).
2. **Enums with fields use `[Enum] interface`** with operation-style
   variants (`Grid(u32 rows, u32 cols);`, `Row();`), and the error
   enum uses **`[Error] interface`** the same way. The
   `"Variant" { ... };` struct-variant form does **not** parse.
   Flat enums (`ImageFormat`, `FitMode`, `Filter`, `Position`) use the
   plain `enum X { "A", "B" };` form.
3. UDL dictionary fields **cannot** carry a default that is itself a
   dictionary — so `common: CommonOptions`, `background: Colour` and
   `layout: CollageLayout` are required fields with no default. The
   Kotlin/Swift wrappers fill them (see the UDL header).

## Tests

```sh
cargo test -p dusrelimage-uniffi    # 21 tests: all 13 functions, error shapes, bulk, options round-trip
cargo clippy -p dusrelimage-uniffi --all-targets -- -D warnings
cargo fmt -p dusrelimage-uniffi -- --check
```

## Regenerating bindings

The `uniffi_bindgen` 0.29.5 crate ships **no binary**, and there is no
matching `uniffi-bindgen` binary crate on crates.io — so this crate
provides one via `uniffi::uniffi_bindgen_main()` (feature `cli`).
It always matches the pinned uniffi **0.29.5**; never install a
separate `uniffi-bindgen` for this crate.

Kotlin (checked in under `packages/android/lib/src/main/kotlin/`):

```sh
cargo run -p dusrelimage-uniffi --bin uniffi-bindgen -- generate \
  --language kotlin --no-format \
  --out-dir packages/android/lib/src/main/kotlin \
  crates/dusrelimage-uniffi/src/dusrelimage.udl
```

Swift (the Swift worker's directory; same command, `--language swift`):

```sh
cargo run -p dusrelimage-uniffi --bin uniffi-bindgen -- generate \
  --language swift --out-dir <swift-dir> \
  crates/dusrelimage-uniffi/src/dusrelimage.udl
```

`--no-format` for Kotlin: ktlint is not installed in CI/sandbox, so
generation skips the auto-format step (bindgen warns about the missing
ktlint otherwise). The checked-in Kotlin is the raw `--no-format`
output; CI diffs a fresh regeneration against it.

## Building the shared library

```sh
cargo build -p dusrelimage-uniffi --release
# -> target/release/libdusrelimage_uniffi.so (Linux) / .dylib (macOS)
#    Android .so per ABI: bash packages/android/scripts/build-jni.sh
```

The JVM unit tests load the host library via the
`uniffi.component.dusrelimage.libraryOverride` system property (see
`packages/android/lib/build.gradle.kts`).
