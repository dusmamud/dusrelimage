# dusrelimage (JavaScript / TypeScript)

Offline image toolkit — compress, resize, crop, convert, watermark,
annotate, colour picker, palette, collage and pixelate. One Rust core,
wrapped as WASM; no network, no uploads. Version `0.1.0`, in lockstep
with the Rust crate, the Python package and the CLI.

```js
import { compressImage, getImageInfo, pickColour } from 'dusrelimage';

const info = getImageInfo(bytes); // { format, width, height, hasAlpha, colourType, fileSize }
const small = compressImage(bytes, { quality: 70, format: 'Jpeg' });
const colour = pickColour(bytes, 10, 20); // { hex: '#FF0000', r, g, b, a, hsl }
```

All functions are **synchronous** (the WASM calls are synchronous)
and throw a `DusrelError` on failure — never a raw `TypeError`, never
a raw JSON string. Inputs are `Uint8Array` only (Node `Buffer` works:
it extends `Uint8Array`).

## Install / build

```bash
npm install
npm run build   # wasm-pack build --target nodejs + tsc
npm test        # vitest run (builds the wasm first if needed)
npm run typecheck
```

`npm run build` writes the wasm-pack output to `wasm-pkg/` and the
compiled TypeScript to `dist/` — both are gitignored build artifacts.

## API

| Function | Returns | Notes |
|---|---|---|
| `version()` | `string` | `"0.1.0"` |
| `makeTestImage(kind, width, height, seed?)` | `Uint8Array` (PNG) | `kind`: `"solid" \| "gradient" \| "noise"`. Test data only. |
| `compressImage(input, options?)` | `Uint8Array` | Tool 1 |
| `compressToSize(input, targetBytes, options?)` | `Uint8Array` | Tool 1; `TARGET_SIZE_UNREACHABLE` when impossible |
| `resizeImage(input, options?)` | `Uint8Array` | Tool 2 |
| `cropImage(input, options)` | `Uint8Array` | Tool 3; `options` requires `width` + `height` |
| `convertImage(input, options)` | `Uint8Array` | Tool 4; `options.format` required |
| `addWatermark(input, options?)` | `Uint8Array` | Tool 5; exactly one of `text` / `logo` |
| `annotateImage(input, annotations)` | `Uint8Array` | Tool 6 |
| `pickColour(input, x, y)` | `Colour` | Tool 7 |
| `extractPalette(input, count = 5)` | `PaletteColour[]` | Tool 8; count 1–16 |
| `makeCollage(inputs, options?)` | `Uint8Array` | Tool 9 |
| `pixelateImage(input, options?)` | `Uint8Array` | Tool 10 |
| `getImageInfo(input, options?)` | `ImageInfo` | `{ format, width, height, hasAlpha, colourType, fileSize }` |
| `bulkProcess(inputs, op, options?)` | `BulkResult[]` | One result per input, same order; partial failure tolerated |

### `BulkResult`

```ts
type BulkResult = {
  index: number;
  ok: boolean;
  output?: Uint8Array;                       // present when ok (image ops)
  colour?: Colour;                           // present when ok (op "pick_colour")
  palette?: PaletteColour[];                 // present when ok (op "palette")
  error?: { code: string; messageEn: string; messageHi: string }; // present when !ok
};
```

`op` is a short name (`"compress"`, `"resize"`, `"crop"`,
`"convert"`, `"watermark"`, `"annotate"`, `"pick_colour"`,
`"palette"`, `"pixelate"`) or the full snake_case export name
(`"compress_image"`, …), plus `"compress_to_size"` (pass
`{ targetBytes, … }`) — see `docs/reference/info-and-bulk`.
`"annotate"` takes `{ annotations }`, `"pick_colour"` takes
`{ x, y }`, `"palette"` takes `{ count }`. `make_collage` is not a
per-item op and is rejected with `INVALID_ARGUMENT` — the wasm layer
delegates bulk to the core's `bulk_process` (`BulkOp` enum), which
reports collage per item as `INVALID_ARGUMENT` for the same reason;
the one op the core's `BulkOp` does NOT carry is
`"compress_to_size"`, which the wasm layer dispatches per item itself.

## Options (camelCase → Rust snake_case)

The TS API uses camelCase keys; the wrapper translates them to the
Rust snake_case JSON keys before calling wasm. Every field is
optional unless noted — omitted fields use the Rust defaults.

| TS key | Rust key | Default |
|---|---|---|
| `autoOrient` | `auto_orient` | `true` |
| `keepExif` | `keep_exif` | `false` (v0.1.0: accepted, not honoured) |
| `dpi` | `dpi` | `null` (v0.1.0: accepted, not embedded) |
| `maxPixels` | `max_pixels` | `120_000_000` |
| `maxDimension` | `max_dimension` | `16000` |
| `quality` | `quality` | `80` (1–100) |
| `format` | `format` | `null` = keep input format (`"Jpeg" \| "Png" \| "WebP" \| "Avif" \| "Gif" \| "Bmp"`) |
| `progressive` | `progressive` | `false` (v0.1.0: accepted, not honoured) |
| `lossless` | `lossless` | `false` (WebP/PNG only) |
| `effort` | `effort` | `4` (0–6) |
| `percent` | `percent` | `null` |
| `fit` | `fit` | `"Fit"` (`"Fit" \| "Fill" \| "Stretch" \| "Crop"`) |
| `filter` | `filter` | `"Lanczos3"` |
| `background` | `background` | white `"#FFFFFF"` |
| `anchor` | `anchor` | `null` (crop preset; `"Center"` etc.) |
| `opacity` | `opacity` | `0.5` (0–1) |
| `position` | `position` | `"BottomRight"` (watermark) |
| `rotationDeg` | `rotation_deg` | `0` |
| `margin` | `margin` | `16` |
| `layout` | `layout` | 2×2 grid (`"2x2"`, `"grid"`, `"row"`, `"column"`, `{ rows, cols }`) |
| `gap` | `gap` | `8` |
| `cellFit` | `cell_fit` | `"Fit"` |
| `blockSize` | `block_size` | `10` (2–100) |
| `targetBytes` | `target_bytes` | — (`compressToSize` / bulk only) |

Colours are `"#RRGGBB"` hex strings (also `"#RGB"` / `"#RRGGBBAA"`)
or full `Colour` objects (`{ hex, r, g, b, a, hsl }`); the wrapper
expands hex strings, computing HSL itself. Annotations use the
`{ type: "Arrow", from, to, colour, width }` form (see `Annotation`
in `src/index.ts`); the wrapper translates them to serde's tagged
form for wasm.

## Errors

```ts
import { DusrelError } from 'dusrelimage';
try {
  compressImage(bytes, { quality: 0 });
} catch (e) {
  if (e instanceof DusrelError) console.log(e.code, e.messageEn, e.messageHi);
}
```

Stable codes (verbatim from the Rust core, SPEC §4):
`INVALID_IMAGE`, `UNSUPPORTED_FORMAT`, `IMAGE_TOO_LARGE`,
`TARGET_SIZE_UNREACHABLE`, `CROP_OUT_OF_BOUNDS`,
`PIXEL_OUT_OF_BOUNDS`, `INVALID_QUALITY`, `INVALID_DIMENSIONS`,
`INVALID_ARGUMENT`, `EMPTY_INPUT`, `INVALID_TYPE`, plus
`OPERATION_FAILED` for non-core failures. Every `DusrelError` carries
`code`, `messageEn` and `messageHi` (Roman-script Hinglish).

## Zod schemas

`dusrelimage/zod` exports schemas mirroring the TS option interfaces
(`compressOptionsSchema`, `resizeOptionsSchema`, …,
`annotationSchema`, `imageBytesSchema`, `bulkOpSchema`). Shape-only:
value ranges and image validity are enforced by the core and reported
as `DusrelError`s.

## WASM contract (for the website playground)

The playground (`website/src/pages/playground.astro`) calls the wasm
exports directly, snake_case, with partial option objects
(`{ quality: 70 }`, `{ filter: "Lanczos3" }`):

- Export names: `compress_image`, `compress_to_size`, `resize_image`,
  `crop_image`, `convert_image`, `add_watermark`, `annotate_image`,
  `pick_colour`, `extract_palette`, `make_collage`,
  `pixelate_image`, `get_image_info`, `bulk_process`, `version`,
  `make_test_image`. Binary results are `Uint8Array`s.
- `get_image_info` / `pick_colour` / `extract_palette` return JSON
  strings — the playground does `JSON.parse()` on them.
- `extract_palette` JSON entries are FLATTENED:
  `{ hex, r, g, b, a, hsl, share }` (the playground reads `c.hex` /
  `c.share` at the top level, not `c.colour.hex`).
- The wasm layer accepts partial options (missing fields are filled
  from the Rust defaults), hex-string colours in any `colour` /
  `background` / `fill` field, collage layouts as `"2x2"` / `"row"` /
  `"column"` strings, and annotations in `{ "type": "Arrow", … }`
  form. `compress_to_size` takes `target_bytes` as a plain JS number
  (wasm-bindgen would demand a `BigInt` for a Rust `u64`).
- Errors are thrown as JSON strings:
  `{"ok":false,"code":"…","message_en":"…","message_hi":"…",
  "reason_en":"…","reason_hi":"…"}`.

## Honest limits (v0.1.0, same as the Rust core)

- AVIF input does NOT decode (`UNSUPPORTED_FORMAT`); AVIF output
  works (lossy, 8-bit).
- WebP output is always lossless; `quality` / `effort` are ignored
  for WebP.
- `keep_exif` / `dpi` / `progressive` are accepted but not honoured.
- `compress_to_size` quality search is JPEG/AVIF-only; other formats
  take one best-effort encode.
- TIFF input decodes but falls back to PNG output.
- GIF output is the first frame only for animated input.
- Whole images live in memory; no streaming mode.
