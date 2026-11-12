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

## 5. Bindings (Phase 1 = v0.1.0)

- `crates/dusrelimage` — core only (publish = true, name `dusrelimage`).
- `crates/dusrelimage-cli` — binary `dusrelimage` (publish = false):
  `compress, resize, crop, convert, watermark, annotate, pick-colour,
  palette, collage, pixelate, info, mock` (+ `--version`, `--help`).
  `mock` generates test images (solid / gradient / photo-like noise).
- `crates/dusrelimage-py` — PyO3 + maturin (publish = false);
  `packages/python` thin wrapper + Pydantic v2 models (optional import).
- `crates/dusrelimage-wasm` — wasm-bindgen (publish = false);
  `packages/js` TS wrapper + Zod schemas, `vitest` tests.
- `website/` — Astro 5 + Starlight docs (one page per tool + options,
  error-codes, comparison, quickstart) + WASM playground page +
  landing. `astro.config.mjs` site: `https://dusrelimage.pages.dev`.
- `shared/` — `test-vectors.json` (the contract every language must
  pass: input fixture name + op + options + expected output hash /
  dimensions / pixel spot-checks) + `spec.md` (this file's API section).
  Fixtures are generated by `dusrelimage mock` (deterministic seed),
  never committed as binary blobs except 2 tiny PNGs (<50 KB total).

Phase 2 (v0.2.x, NOT v0.1.0): Go via C API (`crates/dusrelimage-ffi`),
napi-rs native Node package, PHP FFI wrapper over the C API.
Phase 3 (v0.3.x, NOT v0.1.0): UniFFI Kotlin/Swift (Android/iOS).

## 6. Tests — everything verified by GitHub Actions (no local cargo here)

- Rust: `cargo test --workspace` — unit + integration tests per tool
  (round-trip: decode → op → encode → decode; dimensions, pixel
  spot-checks, error codes, target-size guarantee).
- Python: `pytest` on the maturin wheel (all 10 tools + pydantic).
- JS: `vitest` on the WASM wrapper (all 10 tools + zod).
- Shared vectors: Rust, Python and JS all run `shared/test-vectors.json`
  and must agree.
- `cargo clippy --workspace --all-targets -- -D warnings` (0 warnings),
  `cargo fmt --all -- --check`.

## 7. CI workflows (`.github/workflows/`, mirror dusrelpdf/indycheck)

Phase 1: `ci.yml`, `python.yml`, `wasm.yml`, `website.yml` (build AND
deploy to Cloudflare Pages — dusrelpdf had build-only, do not repeat
that), `release.yml` + `release-pr.yml` (release-plz), `npm.yml`
(Trusted Publishing OIDC, like indycheck), `pypi.yml`
(Trusted Publishing). Phase 2 adds `go.yml` + `php.yml`; Phase 3 adds
`mobile.yml`. Trusted Publishing setup itself is the owner's dashboard
work — workflows must only assume it exists.

## 8. Competitor gaps this must keep (research 2026-11-12, 10/10)

No package covers all 10 tools fully; only ImageMagick covers all 10
at least partially (CLI workarounds). Palette extractor dedicated
nowhere; collage dedicated nowhere; annotator full only in Intervention
Image; pixelator only in Jimp / Photon / Intervention; colour picker
only in Intervention. Sharp = Node-only, Pillow = Python-only,
Intervention = PHP-only — dusrelimage wins by being one core in every
language, offline, with the same answer everywhere.
