# dusrelimage — Android (Kotlin/UniFFI)

Android library for dusrelimage (v0.1.0): the same Rust core as the
JS/Python/CLI bindings, exposed to Kotlin via UniFFI 0.29. Offline —
no network, no uploads. All image work happens in Rust; the Kotlin
side is a thin wrapper plus generated JNA bindings.

## Layout

| Path | What it is |
|---|---|
| `lib/src/main/kotlin/com/dusrel/image/dusrelimage.kt` | **Generated** UniFFI Kotlin bindings (4,026 lines, JNA-based). Do NOT edit by hand — regenerate (below). |
| `lib/src/main/kotlin/com/dusrel/image/DusrelImageWrapper.kt` | Thin idiomatic wrapper: `ByteArray` in/out, Kotlin-friendly defaults, one `DusrelError` exception. This is the public API — use it, not the generated file directly. |
| `lib/src/test/kotlin/com/dusrel/image/VectorsTest.kt` | The 82 shared test vectors from `shared/test-vectors.json` (same cases + expectations as the JS runner). |
| `lib/src/test/kotlin/com/dusrel/image/DusrelImageTest.kt` | Focused wrapper tests (colours, error mapping, defaults, bulk partial failure). |
| `scripts/build-jni.sh` | Builds the JNI `.so` for arm64-v8a + armeabi-v7a + x86_64 with cargo-ndk. |
| `settings.gradle.kts`, `build.gradle.kts`, `lib/build.gradle.kts` | Gradle module: namespace `com.dusrel.image`, compileSdk 34, minSdk 24, Kotlin 2.0.21, AGP 8.5.2, Gradle 8.11.1, JDK 17. |

## Usage

```kotlin
import com.dusrel.image.DusrelImage
import com.dusrel.image.CompressOptions
import com.dusrel.image.CommonOptions
import com.dusrel.image.ImageFormat

val input: ByteArray = /* JPEG/PNG/WebP/GIF/BMP/TIFF bytes */

// Tool 1 — compress to JPEG quality 80 (defaults: quality 80, keep format).
val jpeg: ByteArray = DusrelImage.compressImage(input, CompressOptions(CommonOptions(), format = ImageFormat.JPEG))

// Tool 2 — resize to width 800, aspect kept.
val small: ByteArray = DusrelImage.resizeImage(
    input,
    ResizeOptions(CommonOptions(), width = 800u, background = DusrelImage.white()),
)

// Tool 7 — colour of one pixel.
val colour = DusrelImage.pickColour(input, 10u, 20u) // Colour(hex="#0A141E", r, g, b, a, hsl)

// Info + errors.
val info = DusrelImage.getImageInfo(input) // ImageInfo(format, width, height, hasAlpha, colourType, fileSize)
try {
    DusrelImage.compressImage(ByteArray(0))
} catch (e: DusrelError) {
    println("${e.code}: ${e.messageEn} / ${e.messageHi}") // e.g. EMPTY_INPUT
}

// Bulk: one bad input fails only its own item.
val results: List<BulkResult> = DusrelImage.bulkProcess(
    listOf(input, byteArrayOf(0, 1, 2, 3)),
    BulkOp.Pixelate(PixelateOptions(CommonOptions())),
)
// results[0] is BulkResult.Image(bytes=...), results[1] is BulkResult.Failed(error=...)
```

The generated top-level functions (`compressImage`, `pickColour`, …)
also exist in the same package, but they use `List<UByte>` and throw
the generated `DusrelException` (one subclass per error code) — the
wrapper exists precisely so callers don't have to deal with that.

### Colours

The generated `Colour` record has no defaults, so build colours via
the wrapper: `DusrelImage.white()`, `DusrelImage.black()`,
`DusrelImage.colourFromRgb(r, g, b, a = 255)`,
`DusrelImage.colourFromHex("#RRGGBB")` (also `#RGB`, `#RRGGBBAA`).
`hex` is always `"#RRGGBB"` and `hsl` is
`[h_degrees_0..360, s_0..1, l_0..1]` — both derived from RGB, exactly
as the Rust core does (the Rust side re-derives them from RGBA
anyway).

### Options defaults

Every `*Options` parameter defaults to a fully-defaulted object:
`CommonOptions()` (autoOrient=true, maxPixels=120,000,000,
maxDimension=16000), white `#FFFFFF` background where one is
required, `CollageLayout.Grid(rows=2, cols=2)` for collage. The UDL
cannot give a default to a field that is itself a dictionary, so the
generated `common` / `background` / `layout` fields are required
there — the wrapper fills them. Defaults reproduce the Rust `Default`
values (SPEC section 2) exactly.

## Build

### 1. Host cdylib + JVM unit tests (no Android SDK needed)

```sh
cargo build -p dusrelimage-uniffi --release   # host lib for the JVM tests
cd packages/android
gradle :lib:testDebugUnitTest
```

The tests load the host library through the
`uniffi.component.dusrelimage.libraryOverride` system property, set
in `lib/build.gradle.kts` from `DUSRELIMAGE_UNIFFI_LIB` (default:
`<repo>/target/release/libdusrelimage_uniffi.so` on Linux; on macOS
set `DUSRELIMAGE_UNIFFI_LIB` to the `.dylib` path). The tests run the
82 shared vectors plus the wrapper tests — they need the cdylib, not
an emulator or device.

### 2. JNI `.so` for Android (needs SDK + NDK)

```sh
rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android
cargo install cargo-ndk --version 4.1.2 --locked
export ANDROID_NDK_HOME=/path/to/android-sdk/ndk/26.3.11579264
bash packages/android/scripts/build-jni.sh
```

Exact cargo-ndk command (also in the script):

```sh
cargo ndk -t arm64-v8a -t armeabi-v7a -t x86_64 --platform 24 \
  -o packages/android/lib/src/main/jniLibs \
  build --release -p dusrelimage-uniffi
```

`--platform 24` matches minSdk 24. Output:
`lib/src/main/jniLibs/<abi>/libdusrelimage_uniffi.so` — built in CI,
**never committed** (`*.so` is gitignored). The library AAR then
includes the `.so` for the three ABIs.

### 3. Regenerate the Kotlin bindings

After any change to `crates/dusrelimage-uniffi/src/dusrelimage.udl`
(or `uniffi.toml`):

```sh
cargo run -p dusrelimage-uniffi --bin uniffi-bindgen -- generate \
  --language kotlin --no-format \
  --out-dir packages/android/lib/src/main/kotlin \
  crates/dusrelimage-uniffi/src/dusrelimage.udl
```

Notes:

- The bindgen is the **in-crate binary** (`crates/dusrelimage-uniffi/
  src/bin/uniffi-bindgen.rs`, `uniffi::uniffi_bindgen_main()`),
  so it always matches the pinned uniffi **0.29.5** — never install a
  separate `uniffi-bindgen` from crates.io for this crate (the
  `uniffi_bindgen` 0.29.5 crate ships no binary).
- `--no-format`: ktlint is not installed in CI/sandbox, so generation
  skips the auto-format step (bindgen warns about the missing ktlint
  otherwise). The checked-in file is the raw `--no-format` output;
  CI diffs a fresh regeneration against it and fails if it differs.
- The Kotlin package is `com.dusrel.image` (set in
  `crates/dusrelimage-uniffi/uniffi.toml`, `[bindings.kotlin]
  package_name`), so the generated file lives at
  `lib/src/main/kotlin/com/dusrel/image/dusrelimage.kt` next to the
  wrapper. The Swift worker generates from the same `.udl` with
  `--language swift` — the UDL is the shared contract; see its header
  for the syntax rules (notably `[Throws=...]` attributes and the
  `[Enum] interface` / `[Error] interface` forms).

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
