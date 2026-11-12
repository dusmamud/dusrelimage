# dusrelimage-ffi — C ABI bindings for dusrelimage

C bindings for the `dusrelimage` offline image toolkit (v0.1.0).
This crate is the shared foundation the Go (cgo) and PHP (FFI)
bindings build on: every function is a thin wrapper over the same
Rust core the CLI / Python / WASM bindings use, so results agree
across languages.

The C API is declared in [`dusrelimage.h`](dusrelimage.h) (16
functions, all `dusrelimage_`-prefixed). The Rust side lives in
[`src/lib.rs`](src/lib.rs).

## Building

```sh
# Shared library (Go cgo / PHP FFI link against this):
cargo build -p dusrelimage-ffi
# -> target/debug/libdusrelimage_ffi.so
#    (release: target/release/libdusrelimage_ffi.so)

# Static library:
# -> target/debug/libdusrelimage_ffi.a  (built by the same command)
```

Artifacts produced (`crate-type = ["cdylib", "staticlib", "rlib"]`):

| Artifact | Use |
|---|---|
| `libdusrelimage_ffi.so` / `.dll` / `.dylib` | Go (cgo), PHP (FFI), any C caller |
| `libdusrelimage_ffi.a` | static linking |
| `libdusrelimage_ffi.rlib` | Rust integration tests only (see below) |

The `rlib` is included — same precedent as `dusrelimage-wasm`
(`["cdylib", "rlib"]`) — because without it cargo cannot link the
integration tests in `tests/` on a bare `cargo test` run: cargo
only builds cdylib/staticlib on `cargo build`, never for tests.
It changes nothing about the C API or the cdylib/staticlib
artifacts. The lib is named `dusrelimage_ffi` (not `dusrelimage`)
so it does not collide with the core crate of the same name when a
test binary links both.

Requirements: Rust 1.85+ (workspace MSRV), gcc for the C smoke test.

## Status codes

Every function except `dusrelimage_last_error`,
`dusrelimage_free_buffer` and `dusrelimage_version` returns an
`int32_t` status:

| Code | Name | Meaning |
|---:|---|---|
| 0 | `OK` | success |
| 1 | `INVALID_IMAGE` | input bytes are not a recognisable image |
| 2 | `UNSUPPORTED_FORMAT` | format known but not supported by this build (e.g. AVIF input) |
| 3 | `IMAGE_TOO_LARGE` | exceeds `max_pixels` / `max_dimension` |
| 4 | `TARGET_SIZE_UNREACHABLE` | even the lowest quality exceeds the target size |
| 5 | `CROP_OUT_OF_BOUNDS` | crop rectangle sticks out of the image |
| 6 | `PIXEL_OUT_OF_BOUNDS` | pixel coordinate is outside the image |
| 7 | `INVALID_QUALITY` | quality outside 1-100 |
| 8 | `INVALID_DIMENSIONS` | a dimension is zero, missing or unusable |
| 9 | `INVALID_ARGUMENT` | bad JSON, bad value, NULL pointer, … |
| 10 | `EMPTY_INPUT` | empty input where input is required |
| 11 | `INVALID_TYPE` | wrong-typed value reached a typed API |
| 99 | `UNKNOWN` | anything else (including an internal error) |

On failure, `dusrelimage_last_error()` returns the error as JSON:

```json
{"code": "INVALID_IMAGE", "message_en": "...", "message_hi": "..."}
```

It returns `NULL` when the last call on the same thread succeeded.
The last error is thread-local. A successful call clears it.

## Memory ownership

On success, `*out` / `*out_json` points at a heap buffer of
`*out_len` bytes owned by the caller. Free it exactly once with:

```c
dusrelimage_free_buffer(ptr, len);
```

JSON outputs are UTF-8, `*out_len` bytes long, and **not
NUL-terminated** — use the length (copy and NUL-terminate before
passing them to C string functions). `dusrelimage_last_error()` and
`dusrelimage_version()` return NUL-terminated C strings owned by
the library; do **not** free them. The last-error pointer stays
valid until the next FFI call on the same thread.

## Functions

### Bytes in / bytes out

```c
int32_t dusrelimage_compress(const uint8_t *input, size_t input_len,
                             const char *options_json,
                             uint8_t **out, size_t *out_len);
int32_t dusrelimage_compress_to_size(const uint8_t *input, size_t input_len,
                                     uint64_t target_bytes,
                                     const char *options_json,
                                     uint8_t **out, size_t *out_len);
int32_t dusrelimage_resize(const uint8_t *input, size_t input_len,
                           const char *options_json,
                           uint8_t **out, size_t *out_len);
int32_t dusrelimage_crop(const uint8_t *input, size_t input_len,
                         const char *options_json,
                         uint8_t **out, size_t *out_len);
int32_t dusrelimage_convert(const uint8_t *input, size_t input_len,
                            const char *options_json,
                            uint8_t **out, size_t *out_len);
int32_t dusrelimage_add_watermark(const uint8_t *input, size_t input_len,
                                  const char *options_json,
                                  uint8_t **out, size_t *out_len);
int32_t dusrelimage_annotate(const uint8_t *input, size_t input_len,
                             const char *annotations_json,
                             uint8_t **out, size_t *out_len);
int32_t dusrelimage_make_collage(const uint8_t *const *inputs,
                                 const size_t *input_lens, size_t count,
                                 const char *options_json,
                                 uint8_t **out, size_t *out_len);
int32_t dusrelimage_pixelate(const uint8_t *input, size_t input_len,
                             const char *options_json,
                             uint8_t **out, size_t *out_len);
```

### JSON out

```c
int32_t dusrelimage_pick_colour(const uint8_t *input, size_t input_len,
                                uint32_t x, uint32_t y,
                                uint8_t **out_json, size_t *out_len);
int32_t dusrelimage_extract_palette(const uint8_t *input, size_t input_len,
                                    uint8_t count,
                                    uint8_t **out_json, size_t *out_len);
int32_t dusrelimage_get_image_info(const uint8_t *input, size_t input_len,
                                   const char *options_json,
                                   uint8_t **out_json, size_t *out_len);
int32_t dusrelimage_bulk_process(const uint8_t *const *inputs,
                                 const size_t *input_lens, size_t count,
                                 const char *op, const char *options_json,
                                 uint8_t **out_json, size_t *out_len);
```

### Diagnostics

```c
const char *dusrelimage_last_error(void);
void dusrelimage_free_buffer(uint8_t *ptr, size_t len);
const char *dusrelimage_version(void);
```

## Options JSON

Every `options_json` parameter is a JSON object using the Rust
Options structs' serde snake_case field names (see
`crates/dusrelimage/src/options.rs`). `NULL` means "all defaults".
Partial objects are deep-merged over the defaults, so
`{"quality": 70}` is a complete `CompressOptions` — plain serde
would reject it, because the structs' non-`Option` fields have no
`#[serde(default)]`. (This mirrors the WASM layer, which does the
same deep-merge.)

| Op | Options struct | Main fields |
|---|---|---|
| `compress` / `compress_to_size` | `CompressOptions` | `quality` (1-100, default 80), `format` (`"Jpeg"`/`"Png"`/`"WebP"`/`"Avif"`/`"Gif"`/`"Bmp"` or `null` = keep input), `progressive`, `lossless`, `effort` (0-6) |
| `resize` | `ResizeOptions` | `width`, `height`, `percent`, `fit` (`"Fit"`/`"Fill"`/`"Stretch"`/`"Crop"`), `filter` (`"Nearest"`/`"Triangle"`/`"CatmullRom"`/`"Gaussian"`/`"Lanczos3"`), `background` |
| `crop` | `CropOptions` | `x`, `y`, `width`, `height`, `anchor` (`"Center"`, `"TopLeft"`, … or `null`) |
| `convert` | `ConvertOptions` | `format` (required-ish; default `"Jpeg"`), `quality`, `background`, `progressive` |
| `add_watermark` | `WatermarkOptions` | `text` (`{"text", "size", "colour"}`), `logo` (`{"image", "scale"}`), `opacity` (0-1), `position` (9 anchors + `"Tiled"`), `rotation_deg`, `margin` |
| `make_collage` | `CollageOptions` | `layout`, `gap`, `background`, `width`, `height`, `cell_fit` |
| `pixelate` | `PixelateOptions` | `x`, `y`, `width`, `height` (0 = to the edge), `block_size` (2-100) |

Every options struct embeds `common: CommonOptions`
(`auto_orient` default true, `keep_exif` default false,
`dpi`, `max_pixels` default 120,000,000, `max_dimension` default
16000). Those fields may sit at the top level of the options
object (`{"max_pixels": 100}`) or nested under `"common"`
(`{"common": {"max_pixels": 100}}`) — exactly like the WASM layer.

Accepted shorthands (normalised before deserialising, also like
the WASM layer):

- Hex-string colours (`"#ff0000"`, `"#fff"`, `"#RRGGBBAA"`) in any
  `colour` / `background` / `fill` field become full Colour objects.
- Collage layout strings: `"2x2"` → 2×2 grid, `"grid"` → 2×2 grid,
  `"row"` → `Row`, `"column"` → `Column`. The serde forms
  (`"Row"`, `{"Grid": {"rows": 2, "cols": 2}}`) work too.
- A logo watermark's `image` may be a base64 string or a JSON
  array of byte values.
- Annotations (see below) may use `{"type": "Rect", ...}` form.

Invalid JSON or wrong-typed JSON values return `INVALID_ARGUMENT`
(status 9). `NULL` input / output pointers return
`INVALID_ARGUMENT`, never a crash. Empty input (`len == 0`) is
passed to the core, which reports `EMPTY_INPUT` (status 10).

## Annotations JSON (`dusrelimage_annotate`)

A JSON array, in serde externally-tagged form or `{"type": ...}`
form. Coordinates are absolute pixels. Colour fields accept hex
strings.

```json
[
  {"Rect": {"x": 10, "y": 10, "w": 40, "h": 30,
            "colour": "#FF0000", "width": 2, "fill": "#FF0000"}},
  {"type": "Line", "from": [0, 0], "to": [99, 99],
   "colour": "#000000", "width": 1}
]
```

Variants: `Arrow {from, to, colour, width}`,
`Rect {x, y, w, h, colour, width, fill}`,
`Ellipse {x, y, w, h, colour, width, fill}`,
`Line {from, to, colour, width}`,
`Text {x, y, text, size, colour}`. `fill` is `null` for
outline-only. An empty array is `INVALID_ARGUMENT`.

## JSON output shapes

`pick_colour` — a Colour object:

```json
{"hex": "#0A141E", "r": 10, "g": 20, "b": 30, "a": 255, "hsl": [210.0, 0.5, 0.04]}
```

`extract_palette` — a JSON array of `PaletteColour`, most frequent
first (nested shape, `{"colour": {...}, "share": ...}`):

```json
[{"colour": {"hex": "#010203", "r": 1, "g": 2, "b": 3, "a": 255,
             "hsl": [210.0, 0.5, 0.01]}, "share": 1.0}]
```

`get_image_info` — an `ImageInfo` object:

```json
{"format": "png", "width": 64, "height": 48, "has_alpha": false,
 "colour_type": "rgb8", "file_size": 641}
```

`bulk_process` — a JSON array of per-item results, mirroring the
WASM bulk shape (one result per input, same order; a failing item
never aborts the batch):

```json
[
  {"index": 0, "ok": true, "output": "<base64 image bytes>"},
  {"index": 1, "ok": false,
   "error": {"code": "INVALID_IMAGE", "message_en": "...", "message_hi": "..."}},
  {"index": 2, "ok": true, "colour": {"hex": "#090807", "...": "..."}},
  {"index": 3, "ok": true, "palette": [{"hex": "#090807", "share": 1.0}]}
]
```

Image ops return `"output"` (base64), `pick_colour` returns
`"colour"`, `palette` returns `"palette"` with **flattened**
entries (`hex`/`share` at the top level, as in the WASM bulk
shape — unlike the standalone `extract_palette`, which returns
the nested `PaletteColour` shape).

`op` names for `bulk_process`: `"compress"` / `"compress_image"`,
`"compress_to_size"` (reads `target_bytes` from options),
`"resize"` / `"resize_image"`, `"crop"` / `"crop_image"`,
`"convert"` / `"convert_image"`, `"watermark"` /
`"add_watermark"`, `"annotate"` / `"annotate_image"` (reads
`annotations` from options), `"pick_colour"` (reads `x`/`y`),
`"palette"` (reads `count`, default 5), `"pixelate"` /
`"pixelate_image"`, `"collage"` / `"make_collage"`. A zero input
count returns `"[]"` with status 0. `"collage"` is accepted but
reported per item as `INVALID_ARGUMENT` — a collage combines all
inputs, so call `dusrelimage_make_collage` once instead. An
unknown `op` is `INVALID_ARGUMENT` for the whole call.

## Example (C)

```c
#include <stdio.h>
#include <stdlib.h>
#include "dusrelimage.h"

int main(void) {
    /* ... read input.png into `in`, `in_len` ... */
    uint8_t *out = NULL;
    size_t out_len = 0;
    int32_t status = dusrelimage_compress(
        in, in_len, "{\"format\": \"Jpeg\", \"quality\": 70}",
        &out, &out_len);
    if (status != DUSRELIMAGE_OK) {
        fprintf(stderr, "compress failed: %s\n", dusrelimage_last_error());
        return 1;
    }
    /* ... write `out[0..out_len]` ... */
    dusrelimage_free_buffer(out, out_len);
    printf("version: %s\n", dusrelimage_version());
    return 0;
}
```

Compile against the built cdylib:

```sh
gcc example.c -I crates/dusrelimage-ffi \
    -L target/debug -ldusrelimage_ffi \
    -Wl,-rpath,$PWD/target/debug
```

## Tests

```sh
# Rust integration tests (42 tests, all 13 ops + error paths):
cargo test -p dusrelimage-ffi
cargo clippy -p dusrelimage-ffi --all-targets -- -D warnings
cargo fmt -p dusrelimage-ffi -- --check

# C smoke test (gcc against the built cdylib + dusrelimage.h):
bash crates/dusrelimage-ffi/tests/c_smoke/run_c_smoke.sh
```

The Rust tests call the exported `extern "C"` functions directly
and cover: all 13 ops on the happy path (fixtures from the core
crate), partial-options deep-merge, hex-colour / layout-shorthand /
`{"type": ...}` annotation normalisation, and the error paths
(garbage bytes → `INVALID_IMAGE`, empty input → `EMPTY_INPUT`,
crop out of bounds, bad JSON → `INVALID_ARGUMENT`, wrong-typed
JSON → `INVALID_ARGUMENT`, NULL pointers → `INVALID_ARGUMENT`,
`free_buffer` round-trip, `last_error` content and clearing,
`version`). The C smoke test calls `compress` + `get_image_info`
+ `version` + `free_buffer` through the real `libdusrelimage_ffi.so`,
plus the garbage-bytes error path.

## Honest limits (inherited from the v0.1.0 core)

- AVIF input does not decode (`UNSUPPORTED_FORMAT`); AVIF output
  works (lossy, 8-bit).
- WebP output is always lossless in v0.1.0; GIF output is the
  first frame only; `keep_exif` / `dpi` / `progressive` are accepted
  but not honoured yet.
- Whole images live in memory; there is no streaming mode.
