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

Go, PHP, Android and Swift build on the same core too — see
[Go](#go), [PHP](#php), [Android](#android) and [Swift](#swift)
below. The C API (`crates/dusrelimage-ffi`) and the UniFFI
interface (`crates/dusrelimage-uniffi`) are the shared
foundations; every binding ships in v0.1.0.

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

## Go

Module `github.com/dusmamud/dusrelimage-go` (cgo over the C API).
Build the native library first — the package downloads no prebuilt
binary:

```sh
bash packages/go/scripts/build-ffi.sh   # cargo build -p dusrelimage-ffi --release
```

```go
import dusrelimage "github.com/dusmamud/dusrelimage-go"

small, err := dusrelimage.CompressToSize(input, 50*1024, nil)
opts := dusrelimage.DefaultResizeOptions()
width := uint32(800)
opts.Width = &width
thumb, err := dusrelimage.Resize(input, &opts)
```

Full API and options: [packages/go/README.md](packages/go/README.md).

## PHP

PHP 8.1+ with the `ffi` and `json` extensions. **Composer-direct
install is NOT possible** — the wrapper needs the native shared
library, built from the Rust source first:

```sh
composer install
bash packages/php/scripts/build-ffi.sh   # builds libdusrelimage_ffi.so into packages/php/lib/
```

```php
use Dusrelimage\Dusrelimage;

$di = new Dusrelimage(); // finds lib/libdusrelimage_ffi.so, or $DUSRELIMAGE_FFI_LIB
$small = $di->compressToSize($bytes, 50 * 1024, ['format' => 'Jpeg', 'quality' => 90]);
$thumb = $di->resize($bytes, ['width' => 800]);
```

Full API and options: [packages/php/README.md](packages/php/README.md).

## Android

Kotlin library, namespace `com.dusrel.image`, compileSdk 34,
minSdk 24 (UniFFI 0.29.5 over the Rust core). The JNI `.so`
(arm64-v8a + armeabi-v7a + x86_64) is built with cargo-ndk and is
never committed:

```sh
bash packages/android/scripts/build-jni.sh
```

```kotlin
import com.dusrel.image.DusrelImage
import com.dusrel.image.CompressOptions

val small: ByteArray = DusrelImage.compressToSize(input, 50 * 1024)
val thumb: ByteArray = DusrelImage.resizeImage(
    input, ResizeOptions(CommonOptions(), width = 800u)
)
```

Full API and build steps: [packages/android/README.md](packages/android/README.md).

## Swift

Swift package `DusrelImage`, macOS 13+ / iOS 16+ (UniFFI 0.29.5
over the Rust core). Build the native library first — it is never
committed:

```sh
cargo build -p dusrelimage-uniffi --release
cd packages/swift
swift build -Xlinker -L"$PWD/../../target/release"
```

```swift
import DusrelImage

let small = try DusrelImage.compressToSize(input, targetBytes: 50 * 1024)
let thumb = try DusrelImage.resizeImage(
    input, options: ResizeOptions(common: CommonOptions(), width: 800)
)
```

Full API and linking: [packages/swift/README.md](packages/swift/README.md).

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
  (`crates/dusrelimage`); JS (WASM), Python (PyO3), Go and PHP
  (over the C API in `crates/dusrelimage-ffi`), Android/Kotlin
  and Swift (over the UniFFI interface in
  `crates/dusrelimage-uniffi`) and the CLI all call the same
  code, so behaviour never drifts between languages.
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
- EXIF is always stripped on output — `keep_exif`, `dpi` and
  `progressive` are accepted but **not honoured** in v0.1.0 (the
  underlying `image` 0.25 encoders expose no such switches); only
  orientation is read for auto-rotate.
- AVIF input does not decode (`UNSUPPORTED_FORMAT`); AVIF output is
  lossy-only / 8-bit. WebP output is always lossless.
  `compress_to_size` searches quality only for JPEG/AVIF. TIFF input
  decodes but falls back to PNG output. The SPEC §2 PNG `palette`
  quantise flag is not implemented.

See [SPEC.md](SPEC.md) for the full contract.

## Layout

```
crates/dusrelimage/       Rust core (the only place with image logic)
crates/dusrelimage-cli/   `dusrelimage` binary (publish = false)
crates/dusrelimage-py/    Python bindings, PyO3 (publish = false)
crates/dusrelimage-wasm/  WASM bindings (publish = false)
crates/dusrelimage-ffi/   C API over the core (publish = false)
crates/dusrelimage-uniffi/ UniFFI 0.29.5 bindings (publish = false)
packages/js/              npm package `dusrelimage` (WASM wrapper)
packages/python/          PyPI package `dusrelimage` (thin wrapper)
packages/go/              Go module `dusrelimage-go` (cgo over the FFI lib)
packages/php/             Composer package `dusmamud/dusrelimage` (PHP FFI)
packages/android/         Android library `com.dusrel.image` (UniFFI Kotlin)
packages/swift/           Swift package `DusrelImage` (UniFFI Swift)
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
