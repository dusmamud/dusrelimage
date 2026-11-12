# dusrelimage — Swift (UniFFI)

Swift package for dusrelimage (v0.1.0): the same Rust core as the
JS/Python/CLI/Kotlin bindings, exposed to Swift via UniFFI 0.29.5.
Offline — no network, no uploads. All image work happens in Rust;
the Swift side is a thin wrapper plus generated bindings. macOS 13+
and iOS 16+ (Swift 5.9+, Swift Package Manager).

## Layout

| Path | What it is |
|---|---|
| `Sources/DusrelImage/dusrelimage.swift` | **Generated** UniFFI Swift bindings (4,066 lines). Do NOT edit by hand — regenerate (below). |
| `Sources/DusrelImage/DusrelImageWrapper.swift` | Thin idiomatic wrapper: `Data` in/out, Swift-friendly defaults, one `DusrelError` type, `BulkResult`. This is the public API — use it, not the generated file directly. |
| `Sources/dusrelimageFFI/dusrelimageFFI.h` | **Generated** C header for the UniFFI cdylib. Do NOT edit by hand — regenerate (below). |
| `Sources/dusrelimageFFI/module.modulemap` | **Generated** modulemap content (renamed from the bindgen output file `dusrelimageFFI.modulemap` — SPM system-library targets require the name `module.modulemap`). Do NOT edit by hand. |
| `Tests/DusrelImageTests/VectorsTests.swift` | The 82 shared test vectors from `shared/test-vectors.json` (same cases + expectations as the JS/Kotlin runners). Reads the shared file at test time — no copy is committed, so the vectors cannot drift between languages. |
| `Tests/DusrelImageTests/DusrelImageTests.swift` | Focused wrapper tests (colours, error mapping, defaults, bulk partial failure). |
| `Tests/DusrelImageTests/TestSupport.swift` | Test-only helpers: JSON loading, a minimal PNG/TIFF encoder (stored-deflate, no zlib dependency), fixture builders, pixel access via `pickColour`. |
| `Package.swift` | Package `DusrelImage`: library product `DusrelImage`, system-library target `dusrelimageFFI`, Swift target `DusrelImage`, test target `DusrelImageTests`. |

## Usage

```swift
import DusrelImage

let input: Data = /* JPEG/PNG/WebP/GIF/BMP/TIFF bytes */

// Tool 1 — compress to JPEG quality 80 (defaults: quality 80, keep format).
let jpeg = try DusrelImage.compressImage(
    input,
    options: CompressOptions(common: CommonOptions(), format: .jpeg)
)

// Tool 2 — resize to width 800, aspect kept.
let small = try DusrelImage.resizeImage(
    input,
    options: ResizeOptions(common: CommonOptions(), width: 800, background: DusrelImage.white())
)

// Tool 7 — colour of one pixel.
let colour = try DusrelImage.pickColour(input, x: 10, y: 20) // Colour(hex: "#0A141E", r, g, b, a, hsl)

// Info + errors.
let info = try DusrelImage.getImageInfo(input) // ImageInfo(format, width, height, hasAlpha, colourType, fileSize)
do {
    _ = try DusrelImage.compressImage(Data())
} catch let error as DusrelError {
    print("\(error.code): \(error.messageEn) / \(error.messageHi)") // e.g. EMPTY_INPUT
}

// Bulk: one bad input fails only its own item.
let results: [BulkResult] = DusrelImage.bulkProcess(
    [input, Data([0, 1, 2, 3])],
    op: .pixelate(options: PixelateOptions(common: CommonOptions()))
)
// results[0] is .image(index:bytes:), results[1] is .failed(index:error:)
```

The generated top-level functions (`compressImage`, `pickColour`, …)
also exist in the same module, but they use `[UInt8]` and have no
defaults — the wrapper exists precisely so callers don't have to deal
with that. (Implementation note: the wrapper's static methods share
names with the generated functions, so `DusrelImageWrapper.swift` binds each
generated function once at file scope under an `ffi`-prefixed
forwarder and the wrapper methods call those; calling the generated
function from inside the same-named static method would recurse.)

### Colours

The generated `Colour` record has no defaults, so build colours via
the wrapper: `DusrelImage.white()`, `DusrelImage.black()`,
`DusrelImage.colourFromRgb(_:_:_:_:)`,
`DusrelImage.colourFromHex(_:)` (also `#RGB`, `#RRGGBBAA`).
`hex` is always `"#RRGGBB"` and `hsl` is
`[h_degrees_0..360, s_0..1, l_0..1]` — both derived from RGB, exactly
as the Rust core does (the Rust side re-derives them from RGBA
anyway).

### Options defaults

Every `*Options` parameter defaults to a fully-defaulted object:
`CommonOptions()` (autoOrient=true, maxPixels=120,000,000,
maxDimension=16000), white `#FFFFFF` background where one is
required, `CollageLayout.grid(rows: 2, cols: 2)` for collage. The UDL
cannot give a default to a field that is itself a dictionary, so the
generated `common` / `background` / `layout` fields are required
there — the wrapper fills them. Defaults reproduce the Rust `Default`
values (SPEC section 2) exactly.

### Errors

Unlike Kotlin, where UniFFI generates a `DusrelException` class
hierarchy, UniFFI's Swift generator emits the error enum directly as
`DusrelError`, with one case per stable code
(`.InvalidImage`, `.UnsupportedFormat`, …), each carrying `code`,
`messageEn` and `messageHi`. There is deliberately NO separate
wrapper error struct — a second type named `DusrelError` would
collide with the generated enum in this module. `DusrelImageWrapper.swift`
extends the generated enum with uniform `code` / `messageEn` /
`messageHi` computed properties (plus the `reasonEn` / `reasonHi`
aliases the JS/Python wrappers expose), so callers read the same
shape from every case.

## Build

### 1. Build the native library (required — it is never committed)

The Swift code calls into the Rust cdylib built from
`crates/dusrelimage-uniffi`. Build it for the host first:

```sh
cargo build -p dusrelimage-uniffi --release
# -> target/release/libdusrelimage_uniffi.dylib (macOS)
#    target/release/libdusrelimage_uniffi.so   (Linux)
```

### 2. Linking — how the package finds that library

`Package.swift` declares the generated header as a **system-library
target** (`dusrelimageFFI`, header + modulemap only — it builds
nothing) and the Swift target links the cdylib with
`.linkedLibrary("dusrelimage_uniffi")`. The library search path is
supplied at build time; the library itself is never committed
(`*.dylib` / `*.so` are gitignored) and no prebuilt binary is
checked in. From the repo root:

```sh
cd packages/swift
export LIBRARY_PATH="$PWD/../../target/release"       # link time
export DYLD_LIBRARY_PATH="$PWD/../../target/release"  # run time (macOS; LD_LIBRARY_PATH on Linux)
swift build
swift test
```

or pass the search path explicitly (what CI does):

```sh
swift build -Xlinker -L"$PWD/../../target/release"
swift test  -Xlinker -L"$PWD/../../target/release"
```

`DYLD_LIBRARY_PATH` (or `LD_LIBRARY_PATH` on Linux) must still be set
when running the tests, so the test executable finds the `.dylib` at
runtime. If you embed this package in an app / XCFramework instead,
bundle `libdusrelimage_uniffi.dylib` (or the iOS static library built
with `cargo lipo`) with the app and link it there; the Swift sources
are unchanged.

### 3. Regenerate the Swift bindings

After any change to `crates/dusrelimage-uniffi/src/dusrelimage.udl`
(or `uniffi.toml`):

```sh
cargo run -p dusrelimage-uniffi --bin uniffi-bindgen -- generate \
  --language swift --no-format \
  --out-dir packages/swift/Sources/DusrelImage \
  crates/dusrelimage-uniffi/src/dusrelimage.udl
```

That writes `dusrelimage.swift`, `dusrelimageFFI.h` and
`dusrelimageFFI.modulemap` into `Sources/DusrelImage/` — move the
header and modulemap to `Sources/dusrelimageFFI/` (renaming the
modulemap to `module.modulemap`), leaving only `dusrelimage.swift`
in `Sources/DusrelImage/`.

Notes:

- The bindgen is the **in-crate binary** (`crates/dusrelimage-uniffi/
  src/bin/uniffi-bindgen.rs`, `uniffi::uniffi_bindgen_main()`),
  so it always matches the pinned uniffi **0.29.5** — never install a
  separate `uniffi-bindgen` from crates.io for this crate (the
  `uniffi_bindgen` 0.29.5 crate ships no binary).
- `--no-format`: no Swift formatter is installed in CI/sandbox, so
  generation skips the auto-format step. The checked-in files are
  the raw `--no-format` output; CI diffs a fresh regeneration
  against them and fails if it differs.
- The Swift package is `DusrelImage` (module `DusrelImage`); the
  generated file is `dusrelimage.swift` (lowercase, as bindgen
  emits it). The UDL is the shared contract with Kotlin; see its
  header for the syntax rules (notably `[Throws=...]` attributes
  and the `[Enum] interface` / `[Error] interface` forms).

### 4. Tests

```sh
# from packages/swift, after step 1 (native library built):
swift test -Xlinker -L"$PWD/../../target/release"
```

The tests run the 82 shared vectors plus the wrapper tests — they
need the cdylib, not a simulator or device. Fixtures use
`makeTestImage` (same Rust builders as the CLI `mock` command) plus
the in-test PNG/TIFF encoders in `TestSupport.swift`; pixel
spot-checks read pixels back through `pickColour` (the core decodes),
so no image decoder lives in the tests. The vectors file is located
via the test source path (`#filePath`), with `DUSRELIMAGE_VECTORS`
as an override — run from the repo root or `packages/swift`.

## Honest limits (v0.1.0, same as the Rust core)

- AVIF input does **not** decode (`UNSUPPORTED_FORMAT`); AVIF output works, lossy 8-bit only.
- WebP output is always lossless — quality/effort are ignored for WebP.
- `keep_exif` / `dpi` / `progressive` are accepted but **not honoured** (output is always stripped, no DPI embedded, no progressive/interlaced output).
- `compress_to_size` quality search is JPEG/AVIF-only; other formats take one best-effort encode (and can still fail with `TARGET_SIZE_UNREACHABLE`).
- TIFF input decodes but falls back to PNG output.
- Collage output is always PNG. PNG `palette` quantise flag is not implemented.
- GIF output is the first frame only (animated input is not animated output).
- No HEIC. Whole image in memory, no streaming.
- Error codes (stable, EN + HI messages): `INVALID_IMAGE`,
  `UNSUPPORTED_FORMAT`, `IMAGE_TOO_LARGE`, `TARGET_SIZE_UNREACHABLE`,
  `CROP_OUT_OF_BOUNDS`, `PIXEL_OUT_OF_BOUNDS`, `INVALID_QUALITY`,
  `INVALID_DIMENSIONS`, `INVALID_ARGUMENT`, `EMPTY_INPUT`, `INVALID_TYPE`.
