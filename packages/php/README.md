# dusrelimage (PHP)

Offline image toolkit — compress, resize, crop, convert, watermark,
annotate, colour picker, palette, collage and pixelate. One Rust core,
wrapped for PHP via FFI; no network, no uploads. Version `0.1.0`, in
lockstep with the Rust crate, the JS package and the Python package.

> **Composer-direct install is NOT possible.** `composer require
> dusmamud/dusrelimage` alone does **not** give you a working
> library: this package is a thin PHP wrapper around a native shared
> library (`libdusrelimage_ffi.so`) that must be built from the Rust
> source first. Until that `.so` exists on your machine, every call
> fails with a clear "native library not found" error. Build it with
> `scripts/build-ffi.sh` (see below) — there is no pure-PHP fallback
> and no prebuilt binary shipped with the package.

```php
use Dusrelimage\Dusrelimage;

$di = new Dusrelimage(); // finds lib/libdusrelimage_ffi.so, or $DUSRELIMAGE_FFI_LIB
$bytes = file_get_contents('photo.png');

$small = $di->compressToSize($bytes, 50 * 1024, ['format' => 'Jpeg', 'quality' => 90]);
$thumb = $di->resize($bytes, ['width' => 800]);
$info  = $di->getImageInfo($bytes); // ['format', 'width', 'height', 'has_alpha', 'colour_type', 'file_size']
$colour = $di->pickColour($bytes, 10, 20); // ['hex' => '#FF0000', 'r', 'g', 'b', 'a', 'hsl']
```

All methods are **synchronous** and throw a `DusrelError` on failure
— never a raw FFI error, never a raw JSON string. Inputs are PHP
strings (bytes) only; any non-string input throws `DusrelError`
with code `INVALID_TYPE`, and empty strings throw `EMPTY_INPUT`.

## Install / build

Requirements: PHP >= 8.1 with the `ffi` and `json` extensions,
Composer, and a Rust toolchain (for building the native library).

```bash
composer install
bash scripts/build-ffi.sh   # cargo build -p dusrelimage-ffi --release
                            # + cargo build -p dusrelimage-cli --release
                            # copies target/release/libdusrelimage_ffi.so
                            # to packages/php/lib/libdusrelimage_ffi.so
vendor/bin/phpunit          # runs the shared test vectors + wrapper tests
```

How the `.so` is built, exactly:

1. `scripts/build-ffi.sh` runs `cargo build -p dusrelimage-ffi
   --release` from the repo root. That crate (`crates/dusrelimage-ffi`)
   is the C API over the Rust core; the release build produces
   `target/release/libdusrelimage_ffi.so` on Linux
   (`.dylib` on macOS, `dusrelimage_ffi.dll` on Windows).
2. The script copies that file to `packages/php/lib/` — the first
   place `Dusrelimage` looks for it. The copy is a build artifact:
   it is gitignored and never committed.
3. The same script also builds `dusrelimage-cli` (release), because
   the PHPUnit tests generate their fixtures with
   `dusrelimage mock` (see "Tests").

Where the library must live — `Dusrelimage` resolves it in this
order:

1. The path passed to `new Dusrelimage($path)`;
2. the `DUSRELIMAGE_FFI_LIB` environment variable;
3. `packages/php/lib/libdusrelimage_ffi.so` (where the build script puts it);
4. `target/release/libdusrelimage_ffi.so` in a repo checkout.

If none of those exists, the constructor throws a `RuntimeException`
telling you to run the build script. PHP's FFI also needs to be
enabled: under the CLI SAPI the default `ffi.enable=preload` is
enough; under a web SAPI set `ffi.enable=true` (or preload the
library) or FFI calls are refused.

## API

| Method | Returns | Notes |
|---|---|---|
| `version()` | `string` | `"0.1.0"` |
| `compress(string $input, array $options = [])` | `string` (bytes) | Tool 1 |
| `compressToSize(string $input, int $targetBytes, array $options = [])` | `string` (bytes) | Tool 1; `TARGET_SIZE_UNREACHABLE` when impossible |
| `resize(string $input, array $options = [])` | `string` (bytes) | Tool 2 |
| `crop(string $input, array $options = [])` | `string` (bytes) | Tool 3; `options` requires `width` + `height` |
| `convert(string $input, array $options = [])` | `string` (bytes) | Tool 4; `options['format']` required |
| `addWatermark(string $input, array $options = [])` | `string` (bytes) | Tool 5; `text` and/or `logo` |
| `annotate(string $input, array $annotations)` | `string` (bytes) | Tool 6 |
| `pickColour(string $input, int $x, int $y)` | `array` | Tool 7; `['hex','r','g','b','a','hsl']` |
| `extractPalette(string $input, int $count = 5)` | `array` | Tool 8; count 1–16; entries are `['hex','r','g','b','a','hsl','share']` |
| `makeCollage(array $inputs, array $options = [])` | `string` (bytes) | Tool 9; always PNG |
| `pixelate(string $input, array $options = [])` | `string` (bytes) | Tool 10 |
| `getImageInfo(string $input, array $options = [])` | `array` | `['format','width','height','has_alpha','colour_type','file_size']` |
| `bulkProcess(array $inputs, string $op, array $options = [])` | `array` | One result per input, same order; partial failure tolerated |

`DusrelError` carries `->code` (stable, machine-readable),
`->messageEn` and `->messageHi` (human-readable, Roman-script
Hinglish), plus `->reasonEn` / `->reasonHi` aliases. Error codes:
`INVALID_IMAGE`, `UNSUPPORTED_FORMAT`, `IMAGE_TOO_LARGE`,
`TARGET_SIZE_UNREACHABLE`, `CROP_OUT_OF_BOUNDS`,
`PIXEL_OUT_OF_BOUNDS`, `INVALID_QUALITY`, `INVALID_DIMENSIONS`,
`INVALID_ARGUMENT`, `EMPTY_INPUT`, `INVALID_TYPE`, `UNKNOWN`
(unclassified FFI failure).

### `bulkProcess` result

```php
[
  ['index' => 0, 'ok' => true, 'output' => '...bytes...'],                 // image ops
  ['index' => 1, 'ok' => true, 'colour' => ['hex' => '#090807', ...]],     // op "pick_colour"
  ['index' => 2, 'ok' => true, 'palette' => [['hex' => '...', 'share' => 1.0], ...]], // op "palette"
  ['index' => 3, 'ok' => false, 'error' => ['code' => 'INVALID_IMAGE', 'messageEn' => '...', 'messageHi' => '...']],
]
```

`$op` is the short name (`"compress"`, `"resize"`, `"crop"`,
`"convert"`, `"watermark"`, `"annotate"`, `"pick_colour"`,
`"palette"`, `"collage"`, `"pixelate"`, `"compress_to_size"`) or the
full snake_case name (`"compress_image"`, …). For
`"compress_to_size"` pass `target_bytes` in `$options`; for
`"annotate"` pass `annotations`; for `"pick_colour"` pass `x` / `y`;
for `"palette"` pass `count`. An empty input list returns `[]`.
`"collage"` is not a per-item op: it returns one `INVALID_ARGUMENT`
result per input — call `makeCollage` once with the full list
instead.

## Options (snake_case → Rust)

Options are PHP arrays with snake_case keys matching the Rust
`*Options` serde names (see `SPEC.md` section 2 and
`crates/dusrelimage/src/options.rs`). The wrapper ALWAYS sends
complete objects: every field is filled with the Rust default, so
callers only pass what they want to change. `common` fields may be
passed nested under `"common"` or flat at the top level (nested
wins).

```php
$di->compress($bytes, ['quality' => 70, 'format' => 'Jpeg']);           // quality 1-100, default 80
$di->resize($bytes, ['percent' => 50.0]);                               // or width / height, fit, filter
$di->resize($bytes, ['width' => 40, 'height' => 40, 'fit' => 'Fill', 'background' => '#FFFFFF']);
$di->crop($bytes, ['x' => 10, 'y' => 10, 'width' => 20, 'height' => 20]); // or ['anchor' => 'Center', ...]
$di->convert($bytes, ['format' => 'WebP', 'quality' => 80]);
$di->addWatermark($bytes, ['text' => ['text' => 'Hi', 'size' => 48.0, 'colour' => '#000000'], 'opacity' => 1.0, 'position' => 'BottomRight']);
$di->addWatermark($bytes, ['logo' => ['image' => $logoBytes, 'scale' => 0.5], 'position' => 'Center']);
$di->annotate($bytes, [
  ['type' => 'Rect', 'x' => 10, 'y' => 10, 'w' => 40, 'h' => 30, 'colour' => '#FF0000', 'width' => 2, 'fill' => '#FF0000'],
  ['type' => 'Line', 'from' => [0, 0], 'to' => [99, 99], 'colour' => '#000000', 'width' => 1],
]);
$di->makeCollage([$a, $b], ['layout' => '2x2', 'gap' => 10, 'width' => 210, 'height' => 100]); // or 'row' / 'column' / ['rows'=>2,'cols'=>2]
$di->pixelate($bytes, ['x' => 0, 'y' => 0, 'width' => 50, 'height' => 50, 'block_size' => 10]);
```

Defaults (from the Rust core): `quality` 80, `effort` 4,
`progressive` / `lossless` false, `fit` `Fit`, `filter` `Lanczos3`,
background white, watermark `opacity` 0.5 / `position`
`BottomRight` / `rotation_deg` 0 / `margin` 16, collage `gap` 8 /
`width` / `height` 1200 / `cell_fit` `Fit`, `block_size` 10,
`auto_orient` true, `keep_exif` false, `max_pixels` 120,000,000,
`max_dimension` 16000.

Colours are `"#RRGGBB"` hex strings (or `"#RGB"` / `"#RRGGBBAA"`)
or full `Colour` arrays; enum values accept the Rust serde
spelling (`"Jpeg"`, `"Fit"`, `"Lanczos3"`, `"BottomRight"`) or a
lowercase spelling (`"jpeg"`, `"fit"`, `"lanczos3"`,
`"bottom-right"`). Annotation `type` is case-insensitive
(`"rect"` works, like the Python wrapper). A watermark `text` may
be a plain string and a `logo` may be raw bytes (the Python
spellings).

## Tests

```bash
composer install
bash scripts/build-ffi.sh   # builds the .so AND the CLI binary
vendor/bin/phpunit          # OptionsTest (pure PHP) + ApiTest + VectorsTest (82 shared vectors)
```

`VectorsTest` runs all 82 cases in `shared/test-vectors.json` — the
same contract the Rust, Python and JS runners check. Fixtures are
generated the same way the JS/Python runners do: `solid` /
`gradient` / `noise` images come from `dusrelimage mock --kind …
--width … --height … --seed …` (the CLI binary built by the build
script, found via `$DUSRELIMAGE_CLI` or
`target/release/dusrelimage`); the derived fixtures (`solid_rgb`,
`transparent_rgba`, `two_colour`, `tiff`, `jpeg`, `avif`,
`garbage`, `empty`) are built in-test with a minimal pure-PHP
PNG/TIFF encoder (stdlib `zlib` only — no GD), and the `jpeg` /
`avif` fixtures are derived through the public PHP wrapper, exactly
like the JS/Python runners. No binary blobs are committed.

## FFI contract (for the `dusrelimage-ffi` implementer)

The whole C declaration lives in the single `Dusrelimage::CDEF`
constant in `src/Dusrelimage.php`, so a name fix is a one-line
change there. In short, every image function is

```c
int32_t fn(const uint8_t *input, size_t input_len, const char *options_json, uint8_t **out, size_t *out_len);
```

with these exceptions: `compress_to_size` adds
`uint64_t target_bytes` before `options_json`; `annotate` takes
`const char *annotations_json`; `pick_colour` takes
`uint32_t x, uint32_t y` and returns the colour as JSON;
`extract_palette` takes `uint8_t count` and returns the palette as
JSON (nested `{"colour": {...}, "share": ...}` entries — the
wrapper flattens them); `make_collage` and `bulk_process` take
pointer arrays (`const uint8_t **inputs, const size_t *input_lens,
size_t input_count`), and `bulk_process` additionally takes
`const char *op, const char *options_json` and returns the per-item
results as JSON with image outputs base64-encoded. Two names
differ from the first draft of this contract: the pixelate export
is `dusrelimage_pixelate` (not `dusrelimage_pixelate_image`), and
`get_image_info` takes a `CommonOptions` JSON `options_json`
(5-arg shape: input, input_len, options_json, out, out_len) — the
first draft had it as 4-arg with no options, which dropped
`max_pixels` / `max_dimension` and failed the shared vectors
`info-too-large-max-dimension` / `info-too-large-max-pixels`;
that is fixed. Status ints: 0
OK, 1 `INVALID_IMAGE`, 2 `UNSUPPORTED_FORMAT`, 3
`IMAGE_TOO_LARGE`, 4 `TARGET_SIZE_UNREACHABLE`, 5
`CROP_OUT_OF_BOUNDS`, 6 `PIXEL_OUT_OF_BOUNDS`, 7
`INVALID_QUALITY`, 8 `INVALID_DIMENSIONS`, 9 `INVALID_ARGUMENT`,
10 `EMPTY_INPUT`, 11 `INVALID_TYPE`, 99 `UNKNOWN`. On a non-zero
status, `dusrelimage_last_error()` returns a JSON string
`{"code","message_en","message_hi"}` (or NULL when the last call
succeeded); output buffers are always
freed with `dusrelimage_free_buffer(ptr, len)`. Options JSON is the
complete Rust `*Options` object (snake_case, nested `common`); the
bulk options object additionally carries the op extras at the top
level (`target_bytes` for `compress_to_size`, `annotations` for
`annotate`, `x`/`y` for `pick_colour`, `count` for `palette`).
The bulk `op` string is the short snake_case name (`"compress"`,
`"resize"`, `"pick_colour"`, …).

## Honest limits (same as the Rust core, v0.1.0)

- AVIF input does NOT decode (`UNSUPPORTED_FORMAT`); AVIF output
  works (lossy, 8-bit).
- WebP output is always lossless — quality/effort are ignored for WebP.
- `keep_exif` / `dpi` / `progressive` are accepted but NOT honoured
  (output is always stripped; no DPI is embedded; no progressive /
  interlaced encoding).
- `compress_to_size` quality search is JPEG/AVIF-only; other formats
  take one best-effort encode.
- TIFF input decodes but falls back to PNG output (TIFF is not an
  output format).
- Collage output is always PNG.
- The PNG `palette` quantise flag is not implemented.
- GIF output is the first frame only for animated input.
- No HEIC support, no streaming — the whole image is in memory.
