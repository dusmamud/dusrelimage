// Package dusrelimage is the Go binding for dusrelimage — the offline
// image toolkit (compress, resize, crop, convert, watermark, annotate,
// pick colours, extract palettes, build collages and pixelate). One Rust
// core, wrapped here over its C API (crates/dusrelimage-ffi) via cgo,
// so results agree with the CLI, Python and JS bindings.
//
// Every operation takes bytes in and returns bytes out. Every failure
// is a *DusrelError with a stable machine-readable Code
// ("INVALID_IMAGE", "CROP_OUT_OF_BOUNDS", ...) plus human-readable
// messages in English and Hindi — never a raw panic, never a bare
// error string.
//
// Options: every operation takes a pointer to its options struct.
// Pass nil for all defaults, or start from the matching
// Default*Options() constructor and change the fields you need. A
// zero-value options struct is NOT the defaults (for example a zero
// CompressOptions has Quality 0, which is INVALID_QUALITY); the
// constructors exist so callers never have to remember the Rust
// defaults field by field.
//
// Native library: the C library is NOT downloaded and NOT vendored.
// Build it first with scripts/build-ffi.sh (cargo build -p
// dusrelimage-ffi --release) and cgo links the static library out of
// the repo's target/release directory. See README.md for the exact
// build and link steps.
//
// Honest limits (same as the Rust core, v0.1.0): AVIF input does NOT
// decode (UNSUPPORTED_FORMAT; AVIF output works, lossy 8-bit). WebP
// output is always lossless — quality/effort are ignored for WebP.
// keep_exif / dpi / progressive are accepted but not honoured.
// compress_to_size quality search is JPEG/AVIF-only (other formats
// take one best-effort encode). TIFF input decodes but falls back to
// PNG output. GIF output is the first frame only. Collage output is
// always PNG. No HEIC, no streaming.
package dusrelimage
