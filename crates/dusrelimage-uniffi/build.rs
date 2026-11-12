//! Build script for the UniFFI scaffolding.
//!
//! The UDL interface (`src/dusrelimage.udl`) is compiled into Rust
//! scaffolding by `uniffi::generate_scaffolding` and included by
//! `uniffi::include_scaffolding!("dusrelimage")` in `src/lib.rs`.
//! Without this file `$OUT_DIR` is never set and the crate does not
//! compile — that was the state the interrupted worker left behind.

fn main() {
    uniffi::generate_scaffolding("src/dusrelimage.udl").unwrap();
}
