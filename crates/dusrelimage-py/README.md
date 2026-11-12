# dusrelimage (Python)

Offline image toolkit — compress, resize, crop, convert, watermark,
annotate, colour picker, palette, collage and pixelate. One Rust
core, no network, no uploads: every image is processed on your own
machine.

```sh
pip install dusrelimage
```

```python
import dusrelimage

small = dusrelimage.compress_to_size(png_bytes, 50 * 1024)
thumb = dusrelimage.resize_image(png_bytes, width=800)
info = dusrelimage.get_image_info(png_bytes)  # {"format": "png", "width": 64, ...}
colour = dusrelimage.pick_colour(png_bytes, 10, 20)  # {"hex": "#FF8800", ...}
```

## Operations (v0.1.0)

| Function | What it does |
|---|---|
| `compress_image(image, ...)` | Re-encode at a quality (1-100, default 80) |
| `compress_to_size(image, target_bytes, ...)` | Highest quality that fits the target size |
| `resize_image(image, ...)` | Width / height / percent, fit modes, filters |
| `crop_image(image, ...)` | Rectangle or anchor preset |
| `convert_image(image, *, format=..., ...)` | JPEG / PNG / WebP / AVIF / GIF / BMP |
| `add_watermark(image, ...)` | Text and/or logo watermark |
| `annotate_image(image, annotations)` | Arrows, rects, ellipses, lines, text |
| `pick_colour(image, x, y)` | Pixel colour: hex + RGBA + HSL |
| `extract_palette(image, count=5)` | Dominant colours with share (0-1) |
| `make_collage(images, ...)` | Grid / row / column layouts |
| `pixelate_image(image, ...)` | Pixelate a rectangle (default: whole image) |
| `get_image_info(image, ...)` | Format, dimensions, alpha, colour type, size |
| `bulk_process(images, op, ...)` | One op over many images; failures per item |
| `make_test_image(kind=..., ...)` | Deterministic test PNG (solid / gradient / noise) |

Every function takes bytes in and returns bytes out. Every operation
accepts keyword arguments mirroring the Rust option fields, plus an
`options` dict override (kwargs win). Option dataclasses
(`CompressOptions`, `ResizeOptions`, `CropOptions`,
`ConvertOptions`, `WatermarkOptions`, `CollageOptions`,
`PixelateOptions`) can be passed as `options` too:

```python
from dusrelimage import ResizeOptions

out = dusrelimage.resize_image(png, options=ResizeOptions(width=800))
```

Pydantic v2 models for every options struct live in
`dusrelimage.pydantic` (requires `pip install dusrelimage[pydantic]`):

```python
from dusrelimage.pydantic import CompressOptions

opts = CompressOptions(quality=90, format="webp")
out = dusrelimage.compress_image(png, options=opts)
```

Colours accept `"#RRGGBB"`, `"#RGB"`, `[r, g, b]` / `[r, g, b, a]`,
a dict, or a `Colour`. Enums accept lowercase names (`"jpeg"`,
`"fit"`, `"lanczos3"`, `"bottom-right"`). Layouts accept `"grid"`,
`"row"`, `"column"`, `"2x2"` or `{"rows": 2, "cols": 2}`. Common
options (`auto_orient`, `keep_exif`, `dpi`, `max_pixels`,
`max_dimension`) can be passed flat on any op, or nested under
`"common"`.

## Errors

Every failure raises `dusrelimage.DusrelError` with stable attributes —
never a bare `TypeError` or `ValueError`:

```python
try:
    dusrelimage.crop_image(png, x=90, y=90, width=20, height=20)
except dusrelimage.DusrelError as exc:
    print(exc.code)       # "CROP_OUT_OF_BOUNDS"
    print(exc.reason_en)  # "Crop rectangle (90, 90, 20x20) is outside ..."
    print(exc.reason_hi)  # Roman-script Hindi reason
```

Error codes: `INVALID_IMAGE`, `UNSUPPORTED_FORMAT`,
`IMAGE_TOO_LARGE`, `TARGET_SIZE_UNREACHABLE`, `CROP_OUT_OF_BOUNDS`,
`PIXEL_OUT_OF_BOUNDS`, `INVALID_QUALITY`, `INVALID_DIMENSIONS`,
`INVALID_ARGUMENT`, `EMPTY_INPUT`, `INVALID_TYPE`,
`OPERATION_FAILED`.

## Honest limits (v0.1.0)

- AVIF input does not decode (`UNSUPPORTED_FORMAT`); AVIF output
  works (lossy, 8-bit).
- WebP output is always lossless — `quality` / `effort` are ignored
  for WebP.
- `keep_exif` / `dpi` / `progressive` are accepted but not honoured
  yet (output is always stripped of metadata).
- `compress_to_size` quality search is JPEG/AVIF-only; PNG/WebP/GIF/
  BMP take one best-effort encode instead.
- TIFF input decodes but falls back to PNG output.
- GIF output is the first frame only for animated input.
- Whole images live in memory; there is no streaming mode.
- `bulk_process` delegates to the core's `bulk_process(inputs,
  &BulkOp)` (same per-item contract, including `pick_colour` /
  `palette` outputs); the core has no `BulkOp` for
  `compress_to_size`, so that one op's batch loop lives in this
  binding. `collage` answers every bulk item with
  `INVALID_ARGUMENT` (call `make_collage` once instead);
  `get_image_info` is not batchable.
