# Changelog

All notable changes to dusrelimage are documented here. The format is
based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.1.0] - 2026-11-12

### Added

- Rust core (`crates/dusrelimage`, edition 2024, Rust 1.85+): one
  offline core, bytes in / bytes out, no network, no uploads. All 10
  tools — Compressor (`compress_image`, `compress_to_size` with
  iterative quality search, max 10 steps), Resizer (width / height /
  percent, aspect lock, `Fit | Fill | Stretch | Crop`, filters
  `Nearest | Triangle | CatmullRom | Gaussian | Lanczos3`), Cropper
  (explicit rect + anchor presets), Converter
  (`Jpeg | Png | WebP | Avif | Gif | Bmp`, background flatten for
  alpha), Watermark (text with font size / colour / opacity /
  position incl. tiled / rotation, or logo image with scale /
  opacity / position / margin), Annotator (Arrow, Rect, Ellipse,
  Line, Text — raster drawing, absolute pixel coordinates), Colour
  picker (`pick_colour` -> hex + RGBA + HSL), Palette extractor
  (median-cut quantisation, most frequent first, count 1-16),
  Collage (grid layouts `1x1`-`4x4` + `row` / `column`, cell gap,
  background, per-cell fit), Pixelator (rect + block size 2-100,
  downscale-Nearest + upscale-Nearest of the rect only).
- Common helpers: `get_image_info` (format, dimensions, alpha,
  colour type, file size) and `bulk_process` (one op over many
  inputs, one result per input, same order, partial failures
  reported per item, never aborting the batch).
- Text drawing with `ab_glyph` + bundled DejaVu Sans (permissive
  Bitstream Vera licence, font + licence shipped under
  `crates/dusrelimage/assets/`).
- CLI (`crates/dusrelimage-cli`, binary `dusrelimage`):
  `compress, resize, crop, convert, watermark, annotate,
  pick-colour, palette, collage, pixelate, info, mock` (+
  `--version`, `--help`). `mock` generates deterministic test
  images (solid / gradient / photo-like noise, seeded).
- Python bindings (`crates/dusrelimage-py`, PyO3 + maturin) +
  thin wrapper (`packages/python`) with Pydantic v2 models
  (optional import).
- JS/WASM bindings (`crates/dusrelimage-wasm`, wasm-bindgen) + TS
  wrapper (`packages/js`) with Zod schemas and vitest tests.
- C API (`crates/dusrelimage-ffi`, publish = false): 16
  `dusrelimage_`-prefixed exports declared in
  `crates/dusrelimage-ffi/dusrelimage.h`, building the cdylib
  `libdusrelimage_ffi.so` (`.dylib` / `.dll` on macOS / Windows)
  plus a staticlib. Status ints 0 `OK` / 1 `INVALID_IMAGE` / 2
  `UNSUPPORTED_FORMAT` / 3 `IMAGE_TOO_LARGE` / 4
  `TARGET_SIZE_UNREACHABLE` / 5 `CROP_OUT_OF_BOUNDS` / 6
  `PIXEL_OUT_OF_BOUNDS` / 7 `INVALID_QUALITY` / 8
  `INVALID_DIMENSIONS` / 9 `INVALID_ARGUMENT` / 10 `EMPTY_INPUT` /
  11 `INVALID_TYPE` / 99 `UNKNOWN`; thread-local last error as
  JSON (`dusrelimage_last_error()`); options JSON deep-merged over
  the Rust defaults (same convention as the WASM layer, incl. hex
  colours, collage layout strings and `{"type": ...}`
  annotations). `dusrelimage_get_image_info` is the 5-arg form
  WITH `options_json` (fixed during v0.1.0 — the earlier 4-arg
  form dropped `max_pixels` / `max_dimension`); the pixelate
  export is `dusrelimage_pixelate`.
- Go bindings (`packages/go`, module
  `github.com/dusmamud/dusrelimage-go`): cgo over the FFI lib
  (build it with `packages/go/scripts/build-ffi.sh`), typed API
  mirroring the Rust `*Options` structs with
  `Default*Options()` constructors, `*DusrelError` with stable
  codes + `MessageEn` / `MessageHi`, `Version()` returning
  `"0.1.0"`. `go.mod` carries no version — releases are tagged
  `vX.Y.Z` on the repo.
- PHP bindings (`packages/php`, `dusmamud/dusrelimage`): PHP 8.1+
  FFI wrapper, class `Dusrelimage\Dusrelimage`, snake_case
  options arrays, `DusrelError` with `code` / `messageEn` /
  `messageHi`. Composer-direct install is NOT possible —
  `libdusrelimage_ffi.so` must be built first
  (`packages/php/scripts/build-ffi.sh`); until it exists every
  call fails with a clear "native library not found" error.
- UniFFI bindings (`crates/dusrelimage-uniffi`, uniffi 0.29.5):
  the `.udl` at `crates/dusrelimage-uniffi/src/dusrelimage.udl`
  (13 functions, all `*Options` records, 11 error codes) is the
  shared contract for Kotlin and Swift; generated bindings are
  checked in and CI diffs a fresh regeneration against them.
- Android bindings (`packages/android`, namespace
  `com.dusrel.image`, compileSdk 34, minSdk 24): generated UniFFI
  Kotlin bindings (JNA) + thin wrapper `DusrelImage`
  (`ByteArray` in/out, one `DusrelError` exception). JNI `.so`
  via cargo-ndk (arm64-v8a + armeabi-v7a + x86_64, `--platform
  24`); no versionName / versionCode — the Rust core is the
  version of record.
- Swift bindings (`packages/swift`, package `DusrelImage`,
  macOS 13+ / iOS 16+, Swift 5.9+): generated UniFFI Swift
  bindings + thin wrapper `DusrelImage` (`Data` in/out, the
  generated `DusrelError` enum extended by the wrapper).
  System-library target `dusrelimageFFI` linking
  `libdusrelimage_uniffi`; no version in `Package.swift` —
  releases are tagged `vX.Y.Z` on the repo.
- Shared contract (`shared/`): `test-vectors.json` — 82 cases
  covering all 10 tools + `get_image_info` + `bulk_process` and
  34 error cases, with fixtures generated by `dusrelimage mock`
  (deterministic seed); `spec.md` points at the API section of
  `SPEC.md`. Rust, Python, JS, Go, PHP, Kotlin and Swift all run
  the same vectors and must agree.
- CI (`.github/workflows/`): `ci.yml` (fmt / clippy `-D warnings` /
  tests on ubuntu + macOS + Windows, CLI smoke test, TS wrapper,
  Python wrapper), `python.yml` (maturin wheels + pytest),
  `wasm.yml` (wasm-pack web + bundler builds, export checks),
  `go.yml` (FFI build + `go vet` + `go test`), `php.yml`
  (FFI build + PHPUnit), `mobile.yml` (Android JVM unit tests +
  Swift tests against the host UniFFI cdylib, plus JNI `.so`
  build and binding-regeneration diff checks),
  `website.yml` (Astro build AND deploy to Cloudflare Pages),
  `release.yml` + `release-pr.yml` (release-plz), `npm.yml`
  (Trusted Publishing OIDC, no token), `pypi.yml` (Trusted
  Publishing OIDC, no token).
- Tests verified at v0.1.0: Rust 131 pass locally (68 core + CLI,
  42 FFI, 21 UniFFI); Go 93 pass locally (82 shared vectors +
  11 focused). PHPUnit, Kotlin unit tests and Swift tests are
  CI-only — no PHP / JDK / Swift toolchain in the sandbox (PHP
  could not even be installed there).
- Stable error codes with English + Hindi messages
  (`INVALID_IMAGE`, `UNSUPPORTED_FORMAT`, `IMAGE_TOO_LARGE`,
  `TARGET_SIZE_UNREACHABLE`, `CROP_OUT_OF_BOUNDS`,
  `PIXEL_OUT_OF_BOUNDS`, `INVALID_QUALITY`, `INVALID_DIMENSIONS`,
  `INVALID_ARGUMENT`, `EMPTY_INPUT`, `INVALID_TYPE`).

### Known limits (v0.1.0, honest)

- AVIF input does NOT decode (`UNSUPPORTED_FORMAT`); AVIF output
  works, lossy 8-bit only.
- SPEC deviation: the SPEC §2 PNG `palette` quantise flag is NOT
  implemented in v0.1.0 — PNG `quality` maps to a compression level
  only (≥67 `Best`, 34–66 `Default`, else `Fast`).
- WebP output is always lossless; `quality` / `effort` are ignored
  for WebP.
- `keep_exif` / `dpi` / `progressive` are accepted but not honoured
  (the `image` 0.25 encoders expose no such switches).
- `compress_to_size` quality search is JPEG / AVIF only (PNG is
  best-effort).
- TIFF input decodes but has no `ImageFormat` variant and falls back
  to PNG output.
- `bulk_process` signature is `bulk_process(inputs, &BulkOp)` (op +
  options merged into one enum), and `BulkOp::Collage` returns
  `INVALID_ARGUMENT` per item (collage must be called via
  `make_collage`).
- Collage output is always PNG. Pixelate rect out-of-bounds ->
  `INVALID_ARGUMENT`. Palette returns up to `count` entries.
- GIF encode is first-frame only for animated input. No HEIC
  (libheif + licence). Whole image in memory, no streaming.
- Collage `masonry` layout is out of scope for v0.1.0.
