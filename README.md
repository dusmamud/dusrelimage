# dusrelimage

Offline image toolkit — compress, resize, crop, convert, watermark,
annotate, colour picker, palette, collage, pixelate. One Rust core,
one package, no network, no uploads: every image is processed on
your own machine.

```sh
npm install dusrelimage   # JavaScript / TypeScript (WASM)
pip install dusrelimage   # Python (native wheel)
cargo add dusrelimage     # Rust
```

```rust
use dusrelimage::{compress_to_size, resize_image, ResizeOptions};

fn main() -> Result<(), dusrelimage::DusrelError> {
    let png = std::fs::read("photo.png")?;
    let small = compress_to_size(&png, 50 * 1024, &Default::default())?;
    assert!(small.len() <= 50 * 1024);
    let thumb = resize_image(&png, &ResizeOptions { width: Some(800), ..Default::default() })?;
    std::fs::write("thumb.jpg", thumb)?;
    Ok(())
}
```

```js
import { compressToSize, resizeImage } from "dusrelimage";

const small = await compressToSize(pngBytes, 50 * 1024);
const thumb = await resizeImage(pngBytes, { width: 800 });
```

```python
import dusrelimage

small = dusrelimage.compress_to_size(png_bytes, 50 * 1024)
thumb = dusrelimage.resize_image(png_bytes, width=800)
```

```sh
dusrelimage compress photo.png --max-size 50KB -o small.jpg
dusrelimage resize photo.png --width 800 -o thumb.jpg
dusrelimage palette photo.png --count 5
```

## Tools (v0.1.0)

| Tool | What it does |
|---|---|
| `compress` | Shrink file size — quality, or a target size in bytes |
| `resize` | Change width / height, aspect lock, fit modes |
| `crop` | Cut a selected rectangle out of the image |
| `convert` | Change format (JPG / PNG / WebP / AVIF / GIF / BMP) |
| `watermark` | Text or logo image, opacity + position + rotation |
| `annotate` | Arrows, boxes, ellipses, lines and text on a screenshot |
| `pick_colour` | Colour code (hex / RGB / HSL) of one pixel |
| `palette` | Dominant colour palette of the image |
| `collage` | Multiple photos arranged in one image (grid layouts) |
| `pixelate` | Pixelate a selected rectangle |

Plus the pro control layer: every operation takes bytes in and bytes
out, `Options` everywhere (quality, resize filter, EXIF, metadata,
DPI, background, progressive), stable error codes in English and Hindi,
bulk operations, Zod schemas (JS) and Pydantic models (Python),
and a full CLI.

## Why dusrelimage

- **One core, every language.** The logic lives once in Rust
  (`crates/dusrelimage`); JS (WASM), Python (PyO3) and the CLI all call
  the same code, so behaviour never drifts between languages.
- **10 tools, first-class.** No other package covers all ten —
  see [SPEC.md](SPEC.md) for the competitor gaps this fills.
- **Private by design.** Nothing is uploaded anywhere. The website
  playground runs the WASM build entirely in your browser.

## Honest limits (v0.1.0)

- **No HEIC input/output** (libheif + licence). AVIF encode is
  lossy-only / 8-bit in the underlying `image` crate.
- **No streaming mode.** Whole images live in memory.
- **Annotate / watermark text** uses one bundled open font
  (DejaVu Sans) — no system-font lookup.
- EXIF is stripped by default unless `keep_exif` is set; only
  orientation is read for auto-rotate.

See [SPEC.md](SPEC.md) for the full contract.

## Layout

```
crates/dusrelimage/       Rust core (the only place with image logic)
crates/dusrelimage-cli/   `dusrelimage` binary (publish = false)
crates/dusrelimage-py/    Python bindings, PyO3 (publish = false)
crates/dusrelimage-wasm/  WASM bindings (publish = false)
packages/js/              npm package `dusrelimage` (WASM wrapper)
website/                  Astro Starlight docs + WASM playground
shared/                   test vectors every language must pass
```

## Development

```sh
cargo test --workspace          # Rust core + CLI
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check

cd packages/js && npm ci && npm run build && npm test
cd crates/dusrelimage-py && maturin develop && pytest

dusrelimage --version
```

## Licence

MIT — see [LICENSE](LICENSE).
