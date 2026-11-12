# dusrelimage — SPEC (v0.1.0 contract)

Pattern: same as `dusrelpdf` / `indycheck` (read-only reference in
`~/workspace/dusrelpdf` and `~/workspace/indycheck` — learn the layout,
do NOT copy their code). One Rust core; JS (WASM), Python (PyO3) and
CLI call the same code. All packages named `dusrelimage`, version
`0.1.0` in lockstep. MIT licence. Offline, no network, no uploads.

## 1. The 10 tools (Rust core, bytes in / bytes out)

Core crate `crates/dusrelimage`, edition 2024, Rust 1.85+.
Base decode/encode on the `image` crate (0.25.x). Text drawing with
`ab_glyph` + bundled DejaVu Sans (permissive Bitstream Vera licence,
ship the font file under `crates/dusrelimage/assets/` with its licence).
Palette via median-cut quantisation over sampled pixels.

| # | Tool | Core function (sketch — names must match across bindings) |
|---|---|---|
| 1 | Compressor | `compress_image(input, &CompressOptions) -> Vec<u8>`; `compress_to_size(input, target_bytes, &CompressOptions) -> Vec<u8>` (iterative quality search, binary search on quality, max 10 steps; fails with `TARGET_SIZE_UNREACHABLE` if even lowest quality exceeds target) |
| 2 | Resizer | `resize_image(input, &ResizeOptions) -> Vec<u8>` — width and/or height, percent, aspect lock, fit modes `Fit \| Fill \| Stretch \| Crop`, filter enum `Nearest \| Triangle \| CatmullRom \| Gaussian \| Lanczos3` (default Lanczos3) |
| 3 | Cropper | `crop_image(input, &CropOptions) -> Vec<u8>` — x, y, width, height; out-of-bounds → `CROP_OUT_OF_BOUNDS`; anchor presets `Center \| TopLeft \| TopRight \| BottomLeft \| BottomRight` |
| 4 | Converter | `convert_image(input, &ConvertOptions) -> Vec<u8>` — `ImageFormat::{Jpeg,Png,WebP,Avif,Gif,Bmp}`; quality; `background` colour used to flatten alpha when target has none (default white) |
| 5 | Watermark | `add_watermark(input, &WatermarkOptions) -> Vec<u8>` — text (string, font size, colour, opacity 0-1, position incl. tiled, rotation degrees) OR logo image bytes (scale, opacity, position, margin) |
| 6 | Annotator | `annotate_image(input, &[Annotation]) -> Vec<u8>` — ops: `Arrow { from, to, colour, width }`, `Rect { x,y,w,h, colour, width, fill? }`, `Ellipse { … }`, `Line { … }`, `Text { x,y, text, size, colour }`; coordinates are absolute pixels; drawing is raster (imageproc-style, implemented in core, no external GUI) |
| 7 | Colour picker | `pick_colour(input, x, y) -> Colour` — `Colour { hex: "#RRGGBB", r, g, b, a, hsl: (f32,f32,f32) }`; out-of-bounds → `PIXEL_OUT_OF_BOUNDS` |
| 8 | Palette | `extract_palette(input, count: u8) -> Vec<Colour>` — dominant colours, most frequent first; count 1-16; returns hex + share (0-1) per colour |
| 9 | Collage | `make_collage(inputs: &[Vec<u8>], &CollageOptions) -> Vec<u8>` — grid layouts `1x1…4x4` + named layouts (`grid`, `row`, `column`, `masonry` is out of scope v0.1.0), cell gap px, background colour, output width/height, per-cell fit mode |
| 10 | Pixelator | `pixelate_image(input, &PixelateOptions) -> Vec<u8>` — rect (x,y,w,h) + block size px (2-100); implemented as downscale-Nearest + upscale-Nearest of the rect only |

Common helpers (all tools): `get_image_info(input, &CommonOptions) -> ImageInfo`
(`format, width, height, has_alpha, colour_type, file_size`), and
`bulk_process(inputs, op, options)` — one call, one result per input,
same order, partial failures reported per item, never aborting the batch.

## 2. Options (pro control layer — every op takes its Options struct)

- `CommonOptions`: `auto_orient: bool` (default true — apply EXIF
  orientation on load), `keep_exif: bool` (default false — strip EXIF /
  ICC / XMP unless set), `dpi: Option<(u32,u32)>` (set on JPEG/PNG
  output where the encoder supports it), `max_pixels: u64`
  (default 120_000_000 — refuse larger with `IMAGE_TOO_LARGE`),
  `max_dimension: u32` (default 16000).
- `CompressOptions`: `quality: u8` (1-100, default 80 JPEG/WebP,
  PNG uses `compression_level` 0-9 + `palette` quantise flag),
  `format: Option<ImageFormat>` (default: keep input format),
  `progressive: bool` (JPEG progressive / PNG interlaced),
  `lossless: bool` (WebP/PNG only), `effort: u8` (WebP 0-6).
- `ResizeOptions`: `width: Option<u32>`, `height: Option<u32>`,
  `percent: Option<f32>`, `fit: FitMode`, `filter: Filter`,
  `background: Colour` (used by Fill).
- `ConvertOptions`: `format: ImageFormat`, `quality: u8`,
  `background: Colour`, `progressive: bool`.
- `WatermarkOptions`: `text: Option<TextWatermark>`,
  `logo: Option<ImageWatermark>`, `opacity: f32`, `position: Position`
  (9 anchors + `Tiled`), `rotation_deg: f32`, `margin: u32`.
- `CollageOptions`: `layout: CollageLayout`, `gap: u32`,
  `background: Colour`, `width: u32`, `height: u32`,
  `cell_fit: FitMode`.
- `PixelateOptions`: `x, y, width, height, block_size: u32`.

## 3. Formats

- In: JPEG, PNG, WebP, GIF, BMP, TIFF, AVIF (decode via `image` crate).
- Out: JPEG, PNG, WebP, AVIF, GIF, BMP.
- v0.1.0 limits: NO HEIC (libheif + licence); AVIF encode lossy-only /
  8-bit (underlying crate limit); GIF encode is first-frame only for
  animated input (document it); whole image in memory, no streaming.

## 4. Errors — stable codes, English + Hindi

`INVALID_IMAGE`, `UNSUPPORTED_FORMAT`, `IMAGE_TOO_LARGE`,
`TARGET_SIZE_UNREACHABLE`, `CROP_OUT_OF_BOUNDS`,
`PIXEL_OUT_OF_BOUNDS`, `INVALID_QUALITY`, `INVALID_DIMENSIONS`,
`INVALID_ARGUMENT`, `EMPTY_INPUT`, `INVALID_TYPE`.
`DusrelError { code, message_en, message_hi }`. Every validator /
binding returns the result struct, never panics on bad input.

## 5. Bindings (v0.1.0)

- `crates/dusrelimage` — core only (publish = true, name `dusrelimage`).
- `crates/dusrelimage-cli` — binary `dusrelimage` (publish = false):
  `compress, resize, crop, convert, watermark, annotate, pick-colour,
  palette, collage, pixelate, info, mock` (+ `--version`, `--help`).
  `mock` generates test images (solid / gradient / photo-like noise).
- `crates/dusrelimage-py` — PyO3 + maturin (publish = false);
  `packages/python` thin wrapper + Pydantic v2 models (optional import).
- `crates/dusrelimage-wasm` — wasm-bindgen (publish = false);
  `packages/js` TS wrapper + Zod schemas, `vitest` tests.
- `crates/dusrelimage-ffi` — C ABI over the core (publish = false):
  16 `dusrelimage_`-prefixed exports declared in
  `crates/dusrelimage-ffi/dusrelimage.h` (see §5.1). Builds the
  cdylib `libdusrelimage_ffi.so` (`.dylib` / `.dll` on macOS /
  Windows) plus a staticlib. The shared foundation the Go (cgo)
  and PHP (FFI) bindings build on.
- `crates/dusrelimage-uniffi` — UniFFI 0.29.5 bindings
  (publish = false): the `.udl` at
  `crates/dusrelimage-uniffi/src/dusrelimage.udl` is the shared
  contract for the Kotlin (Android) and Swift (iOS) bindings
  (see §5.2). Generated Kotlin + Swift bindings are checked in.
- `packages/go` — Go module `github.com/dusmamud/dusrelimage-go`
  (cgo over the FFI lib; build it with
  `packages/go/scripts/build-ffi.sh`; `go.mod` carries no version —
  releases are tagged `vX.Y.Z` on the repo). Typed API mirroring
  the Rust `*Options` structs field for field,
  `Default*Options()` constructors, `*DusrelError` with the
  stable codes + `MessageEn` / `MessageHi`.
- `packages/php` — PHP 8.1+ FFI wrapper (`dusmamud/dusrelimage`,
  `packages/php/composer.json`): class
  `Dusrelimage\Dusrelimage`, snake_case options arrays,
  `DusrelError` with `code` / `messageEn` / `messageHi`.
  Composer-direct install is NOT possible —
  `libdusrelimage_ffi.so` must be built first
  (`packages/php/scripts/build-ffi.sh`).
- `packages/android` — Android library, namespace
  `com.dusrel.image`, compileSdk 34, minSdk 24: generated UniFFI
  Kotlin bindings (JNA) + thin wrapper `DusrelImage`
  (`ByteArray` in/out, one `DusrelError` exception). JNI `.so`
  via cargo-ndk (arm64-v8a + armeabi-v7a + x86_64,
  `--platform 24`); no versionName / versionCode — the Rust core
  is the version of record.
- `packages/swift` — Swift package `DusrelImage` (macOS 13+ /
  iOS 16+, Swift 5.9+): generated UniFFI Swift bindings + thin
  wrapper `DusrelImage` (`Data` in/out, `DusrelError` enum
  extended by the wrapper). System-library target
  `dusrelimageFFI` linking `libdusrelimage_uniffi`; no version in
  `Package.swift` — releases are tagged `vX.Y.Z` on the repo.
- `website/` — Astro 5 + Starlight docs (one page per tool + options,
  error-codes, comparison, quickstart, one page per language) +
  WASM playground page + landing.
  `astro.config.mjs` site: `https://dusrelimage.pages.dev`.
- `shared/` — `test-vectors.json` (the contract every language must
  pass: input fixture name + op + options + expected output hash /
  dimensions / pixel spot-checks) + `spec.md` (this file's API section).
  Fixtures are generated by `dusrelimage mock` (deterministic seed),
  never committed as binary blobs except 2 tiny PNGs (<50 KB total).

Phase 2 and Phase 3 are IN v0.1.0, before its first publish: the
Go (C API), PHP (FFI) and UniFFI Kotlin/Swift (Android/iOS)
bindings listed above all ship in v0.1.0. Still NOT in v0.1.0:
the napi-rs native Node package (planned v0.2.x).

### 5.1 FFI contract (C API, `crates/dusrelimage-ffi`)

The header `crates/dusrelimage-ffi/dusrelimage.h` declares all 16
exports. Every image function returns an `int32_t` status and
writes its output to a caller-owned heap buffer (`*out` /
`*out_json`, `*out_len` bytes), freed exactly once with
`dusrelimage_free_buffer(ptr, len)`. JSON outputs are UTF-8 and
NOT NUL-terminated — use the length. `dusrelimage_last_error()`
and `dusrelimage_version()` return NUL-terminated C strings owned
by the library; do NOT free them. The last error is thread-local
and is `NULL` after a successful call.

| Function | Shape |
|---|---|
| `dusrelimage_compress` | `(input, len, options_json, out, out_len)` |
| `dusrelimage_compress_to_size` | `(input, len, target_bytes, options_json, out, out_len)` |
| `dusrelimage_resize` | `(input, len, options_json, out, out_len)` |
| `dusrelimage_crop` | `(input, len, options_json, out, out_len)` |
| `dusrelimage_convert` | `(input, len, options_json, out, out_len)` |
| `dusrelimage_add_watermark` | `(input, len, options_json, out, out_len)` |
| `dusrelimage_annotate` | `(input, len, annotations_json, out, out_len)` |
| `dusrelimage_make_collage` | `(inputs, input_lens, count, options_json, out, out_len)` |
| `dusrelimage_pixelate` | `(input, len, options_json, out, out_len)` |
| `dusrelimage_pick_colour` | `(input, len, x, y, out_json, out_len)` |
| `dusrelimage_extract_palette` | `(input, len, count, out_json, out_len)` |
| `dusrelimage_get_image_info` | `(input, len, options_json, out_json, out_len)` |
| `dusrelimage_bulk_process` | `(inputs, input_lens, count, op, options_json, out_json, out_len)` |
| `dusrelimage_last_error` | `() -> const char *` (JSON `{"code","message_en","message_hi"}`) |
| `dusrelimage_free_buffer` | `(ptr, len)` |
| `dusrelimage_version` | `() -> const char *` (e.g. `"0.1.0"`) |

Status ints: 0 `OK`, 1 `INVALID_IMAGE`, 2 `UNSUPPORTED_FORMAT`,
3 `IMAGE_TOO_LARGE`, 4 `TARGET_SIZE_UNREACHABLE`,
5 `CROP_OUT_OF_BOUNDS`, 6 `PIXEL_OUT_OF_BOUNDS`,
7 `INVALID_QUALITY`, 8 `INVALID_DIMENSIONS`, 9 `INVALID_ARGUMENT`,
10 `EMPTY_INPUT`, 11 `INVALID_TYPE`, 99 `UNKNOWN`.

Options JSON: every `options_json` is a JSON object using the Rust
Options structs' serde snake_case field names (§2); `NULL` means
all defaults. Partial objects are deep-merged over the defaults,
so `{"quality": 70}` is a complete `CompressOptions`. Every
options struct embeds `common: CommonOptions`; its fields may sit
at the top level (`{"max_pixels": 100}`) or nested under
`"common"` — exactly like the WASM layer. Accepted shorthands:
hex-string colours (`"#ff0000"`) in `colour` / `background` /
`fill` fields; collage layout strings (`"2x2"`, `"grid"`,
`"row"`, `"column"`); a base64 string (or byte array) for a logo
watermark's `image`; `{"type": "Rect", ...}` annotation form.
Invalid JSON or wrong-typed values return `INVALID_ARGUMENT`
(status 9). `NULL` input / output pointers return
`INVALID_ARGUMENT`, never a crash. Empty input (`len == 0`) is
passed to the core, which reports `EMPTY_INPUT` (status 10).

`dusrelimage_get_image_info` is the 5-arg form WITH `options_json`
(input, len, options_json, out_json, out_len). The earlier 4-arg
form (no options) dropped `CommonOptions` (`max_pixels` /
`max_dimension`) and failed the shared vectors
`info-too-large-max-dimension` / `info-too-large-max-pixels`; it
was fixed during v0.1.0 for parity with the WASM binding, which
takes options for `get_image_info` too. The pixelate export is
`dusrelimage_pixelate` (not `dusrelimage_pixelate_image`).

### 5.2 UniFFI (Kotlin / Swift, `crates/dusrelimage-uniffi`)

UniFFI **0.29.5** (pinned in the workspace `Cargo.lock`). The
interface definition is
`crates/dusrelimage-uniffi/src/dusrelimage.udl` — the single
shared contract both generated bindings are built from, so it
must stay in lockstep with the core (`options.rs`, `types.rs`,
`error.rs`). It declares **15 functions** — the 13 core functions
(11 tool functions + `get_image_info` + `bulk_process`) plus the
extras `make_test_image` (test helper) and `version` — all `*Options`
records, and the **11 error codes** of §4 as the `DusrelError`
error interface (every variant carries `code` + `message_en` +
`message_hi`). Generated Kotlin
(`packages/android/lib/src/main/kotlin/com/dusrel/image/dusrelimage.kt`)
and Swift (`packages/swift/Sources/DusrelImage/dusrelimage.swift`)
bindings are **checked in** — regenerate after any `.udl` change
with the in-crate `uniffi-bindgen` binary
(`cargo run -p dusrelimage-uniffi --bin uniffi-bindgen`), never a
separately installed bindgen; CI diffs a fresh regeneration
against the checked-in files. Kotlin error type is the generated
`DusrelException` (one subclass per code; the wrapper exposes one
`DusrelError` exception instead); Swift error type is the
generated `DusrelError` enum (one case per code), extended by the
wrapper with uniform `code` / `messageEn` / `messageHi`
properties. UDL divergences from the Rust types (documented in
the `.udl` header): `Colour.hsl` is `sequence<f32>` (no tuples),
`CommonOptions.dpi` is a `Dpi?` record, annotation `from`/`to`
are `Point` records, `common` / `background` / `layout` fields
are required (UDL cannot default a dictionary — wrappers fill
them), and `BulkOp.CompressToSize` exists only in the UDL (the
core `BulkOp` has no such variant).

## 6. Tests — everything verified by GitHub Actions (Rust, Go also run locally)

- Rust: `cargo test --workspace` — unit + integration tests per tool
  (round-trip: decode → op → encode → decode; dimensions, pixel
  spot-checks, error codes, target-size guarantee). 131 tests pass
  locally: 68 core + CLI, 42 FFI, 21 UniFFI.
- Go: `go test ./...` in `packages/go` (after
  `packages/go/scripts/build-ffi.sh`) — 93 tests pass locally: the
  82 shared vectors (as subtests) + 11 top-level tests (9 focused +
  2 vector runners).
- Python: `pytest` on the maturin wheel (all 10 tools + pydantic).
- JS: `vitest` on the WASM wrapper (all 10 tools + zod).
- PHP: `vendor/bin/phpunit` in `packages/php` (after
  `packages/php/scripts/build-ffi.sh`) — OptionsTest (pure PHP) +
  ApiTest + VectorsTest (82 shared vectors). CI-only: no PHP in
  the sandbox (it could not even be installed there).
- Android (Kotlin): `gradle :lib:testDebugUnitTest` in
  `packages/android` against the host UniFFI cdylib — the 82 shared
  vectors + wrapper tests. CI-only: no JDK in the sandbox.
- Swift: `swift test` in `packages/swift` against the host UniFFI
  cdylib — the 82 shared vectors + wrapper tests. CI-only: no
  Swift toolchain in the sandbox.
- Shared vectors: Rust, Python, JS, Go, PHP, Kotlin and Swift all
  run `shared/test-vectors.json` and must agree.
- `cargo clippy --workspace --all-targets -- -D warnings` (0 warnings),
  `cargo fmt --all -- --check`.

## 7. CI workflows (`.github/workflows/`, mirror dusrelpdf/indycheck)

Phase 1: `ci.yml`, `python.yml`, `wasm.yml`, `website.yml` (build AND
deploy to Cloudflare Pages — dusrelpdf had build-only, do not repeat
that), `release.yml` + `release-pr.yml` (release-plz), `npm.yml`
(Trusted Publishing OIDC, like indycheck), `pypi.yml`
(Trusted Publishing). The Go, PHP and mobile bindings ship in
v0.1.0 too, so `go.yml`, `php.yml` and `mobile.yml` (Android +
Swift) are part of v0.1.0 as well. Trusted Publishing setup itself
is the owner's dashboard work — workflows must only assume it exists.

## 8. Competitor gaps this must keep (research 2026-11-12, 10/10)

No package covers all 10 tools fully; only ImageMagick covers all 10
at least partially (CLI workarounds). Palette extractor dedicated
nowhere; collage dedicated nowhere; annotator full only in Intervention
Image; pixelator only in Jimp / Photon / Intervention; colour picker
only in Intervention. Sharp = Node-only, Pillow = Python-only,
Intervention = PHP-only — dusrelimage wins by being one core in every
language, offline, with the same answer everywhere.
