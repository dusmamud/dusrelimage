// `uniffi-bindgen` binary for this crate (UniFFI 0.29).
//
// The `uniffi_bindgen` crate ships no binary of its own in 0.29, and
// there is no matching `uniffi-bindgen` binary crate on crates.io —
// so this workspace provides one via `uniffi::uniffi_bindgen_main()`
// (feature `cli`). It always matches the `uniffi` version this crate
// is built against; never install a separate `uniffi-bindgen` for
// this crate.
//
// Regenerate the Kotlin bindings:
//   cargo run -p dusrelimage-uniffi --bin uniffi-bindgen -- generate \
//     --language kotlin --out-dir packages/android/lib/src/main/kotlin \
//     crates/dusrelimage-uniffi/src/dusrelimage.udl
// (the Swift worker runs the same command with `--language swift`).
// The config in `uniffi.toml` next to this crate sets the Kotlin
// package (`com.dusrel.image`) and cdylib name.

fn main() {
    uniffi::uniffi_bindgen_main()
}
