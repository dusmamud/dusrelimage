# dusrelimage-go — Go binding for dusrelimage

Go binding for **dusrelimage**, the offline image toolkit (compress,
resize, crop, convert, watermark, annotate, pick colours, extract
palettes, build collages and pixelate). One Rust core, wrapped here
over its C API (`crates/dusrelimage-ffi`) via cgo, so results agree
with the CLI, Python and JS bindings. No network, no uploads.

Module: `github.com/dusmamud/dusrelimage-go` — version `0.1.0`, in
lockstep with the core (releases are tagged `vX.Y.Z` on the repo; the
`go.mod` carries no version of its own).

## Build the native library first

The image work lives in Rust. The Go package does **not** download a
prebuilt binary and does not vendor one — you build the C library
from the Rust sources in this repo:

```sh
# From the repo root:
bash packages/go/scripts/build-ffi.sh
# Runs: cargo build -p dusrelimage-ffi --release
# Leaves: target/release/libdusrelimage_ffi.a   (Linux/macOS)
#         target/release/dusrelimage_ffi.lib    (Windows)
```

Requires a Rust toolchain (`cargo` on `PATH`, or `~/.cargo/bin/cargo`).
The script fails loudly if `cargo` is missing or the library is not
produced.

### How cgo links it

`ffi.go` (the only file with cgo declarations) links the static
library by relative path from this package:

```go
#cgo linux LDFLAGS: ${SRCDIR}/../../target/release/libdusrelimage_ffi.a -ldl -lpthread -lm
#cgo darwin LDFLAGS: ${SRCDIR}/../../target/release/libdusrelimage_ffi.a -lpthread -lm
#cgo windows LDFLAGS: ${SRCDIR}/../../target/release/dusrelimage_ffi.lib -lws2_32 -luserenv -lbcrypt
```

`${SRCDIR}` is this package's directory, so the path always resolves
to the repo's `target/release` — no install step, no `CGO_LDFLAGS`
environment fiddling. The declarations in `ffi.go` are copied from
`crates/dusrelimage-ffi/dusrelimage.h`; if the C API changes, `ffi.go`
is the one file to fix.

## Test

```sh
# Build the FFI library (above) and the CLI used for test fixtures:
cargo build -p dusrelimage-cli --release

# Then, from packages/go:
go vet ./...
go test ./...
```

`go test` runs the shared vectors in `shared/test-vectors.json` (82
cases: all 10 tools + info + bulk + error cases), the same contract
the Rust, Python and JS runners check. Fixtures are generated with
the `dusrelimage mock` CLI command (solid/gradient/noise + seed), the
way the other bindings generate theirs; the test locates the CLI at
`$DUSRELIMAGE_CLI`, else `../../target/release/dusrelimage`, else
`PATH`, and fails loudly if it cannot find it. Set
`DUSRELIMAGE_VECTORS` to point at a different vectors file.

## Usage

```go
import dusrelimage "github.com/dusmamud/dusrelimage-go"

// Bytes in / bytes out. Nil options = all defaults.
out, err := dusrelimage.Compress(input, nil)
if err != nil {
    var de *dusrelimage.DusrelError
    errors.As(err, &de)
    fmt.Println(de.Code, de.MessageEn, de.MessageHi) // e.g. INVALID_IMAGE
}

// Options start from a Default*Options() constructor:
resizeOpts := dusrelimage.DefaultResizeOptions()
width := uint32(800)
resizeOpts.Width = &width
out, err = dusrelimage.Resize(input, &resizeOpts)

// Convert to another format:
convertOpts := dusrelimage.DefaultConvertOptions()
convertOpts.Format = dusrelimage.FormatWebP
out, err = dusrelimage.Convert(input, &convertOpts)

// One pixel, a palette, a collage:
colour, err := dusrelimage.PickColour(input, 5, 5)   // colour.Hex == "#0A141E"
palette, err := dusrelimage.ExtractPalette(input, 5) // []PaletteColour, most frequent first
collage, err := dusrelimage.MakeCollage([][]byte{a, b}, nil)

// One op over many inputs; one bad input fails only its own item:
results, err := dusrelimage.BulkProcess(
    [][]byte{good, bad, good},
    dusrelimage.BulkResize(&resizeOpts),
)
for _, r := range results {
    if !r.OK {
        fmt.Println(r.Index, r.Error.Code) // e.g. 1 INVALID_IMAGE
    }
}

// Image summary and library version:
info, err := dusrelimage.GetImageInfo(input, nil) // info.Format == "png", info.Width, ...
fmt.Println(dusrelimage.Version())                // "0.1.0"
```

### API

| Function | Description |
|---|---|
| `Compress(input, opts)` | Re-encode at a fixed quality (and `Format`, or the input format) |
| `CompressToSize(input, targetBytes, opts)` | Re-encode to at most `targetBytes` bytes |
| `Resize(input, opts)` | Width and/or height, percent, fit, filter |
| `Crop(input, opts)` | Rectangle or anchor preset |
| `Convert(input, opts)` | Convert to another format |
| `AddWatermark(input, opts)` | Text and/or logo watermark |
| `Annotate(input, annotations)` | Arrows, rects, ellipses, lines, text |
| `PickColour(input, x, y)` | Colour of one pixel |
| `ExtractPalette(input, count)` | Dominant colours, most frequent first |
| `MakeCollage(inputs, opts)` | Grid / row / column collage (always PNG) |
| `Pixelate(input, opts)` | Pixelate a rectangle (whole image by default) |
| `GetImageInfo(input, opts)` | Image summary without modifying it |
| `BulkProcess(inputs, op)` | One op over many inputs; partial failures per item |
| `Version()` | Native library version, e.g. `"0.1.0"` |

Options structs mirror the Rust `*Options` structs field for field
(Go field names in CamelCase, JSON tags in the Rust serde snake_case
names), with a `Default*Options()` constructor per op filling the
Rust defaults. Pass `nil` for all defaults, or start from the
constructor and change the fields you need — a zero-value options
struct is **not** the defaults (e.g. a zero `CompressOptions` has
`Quality` 0, which is `INVALID_QUALITY`).

Colours are full objects (`{Hex, R, G, B, A, HSL}`); use
`ColourFromHex("#FF0000")` (also accepts `"#RGB"` / `"#RRGGBBAA"`,
with or without `#`) or `ColourFromRGB` / `ColourWhite` /
`ColourBlack`. Collage layouts are `GridLayout(rows, cols)`,
`RowLayout`, `ColumnLayout`. Annotations are concrete types
(`ArrowAnnotation`, `RectAnnotation`, `EllipseAnnotation`,
`LineAnnotation`, `TextAnnotation`).

### Errors

Every failure is a `*DusrelError` with a stable `Code`
(`INVALID_IMAGE`, `UNSUPPORTED_FORMAT`, `IMAGE_TOO_LARGE`,
`TARGET_SIZE_UNREACHABLE`, `CROP_OUT_OF_BOUNDS`,
`PIXEL_OUT_OF_BOUNDS`, `INVALID_QUALITY`, `INVALID_DIMENSIONS`,
`INVALID_ARGUMENT`, `EMPTY_INPUT`, `INVALID_TYPE`), plus
`MessageEn` / `MessageHi` from the Rust core — never a raw panic or
a bare error string. `Error()` formats as `"CODE: english message"`.

## Honest limits (v0.1.0, same as the Rust core)

- AVIF input does **not** decode (`UNSUPPORTED_FORMAT`); AVIF output
  works, lossy 8-bit.
- WebP output is always lossless — quality/effort are ignored for WebP.
- `keep_exif` / `dpi` / `progressive` are accepted but not honoured
  (output is always stripped of EXIF/ICC/XMP; no DPI is embedded).
- `compress_to_size` quality search is JPEG/AVIF-only; other formats
  take one best-effort encode.
- TIFF input decodes but falls back to PNG output (TIFF has no output
  format).
- GIF output is the first frame only for animated input.
- Collage output is always PNG.
- PNG `palette` quantise flag is not implemented.
- No HEIC. Whole image in memory, no streaming.

## FFI corrections applied (v0.1.0)

Two deviations from the original task brief were found against the
real C API (`dusrelimage.h`, verified with `nm -D`) and fixed in
this binding:

1. **Pixelate is `dusrelimage_pixelate`, not
   `dusrelimage_pixelate_image`.** The task brief named the latter;
   the header and `lib.rs` export the former, so `ffi.go` declares
   and calls `dusrelimage_pixelate`. The bulk op alias string
   `"pixelate_image"` is still accepted by the FFI layer, so the
   bulk op map keeps it.
2. **`dusrelimage_get_image_info` is the 5-arg form
   `(input, len, options_json, out_json, out_len)`.** The FFI worker
   first shipped a 4-arg form with no options, which dropped
   `CommonOptions` (`max_pixels` / `max_dimension`) and would have
   failed 2 shared vectors; the coordinator updated the FFI crate to
   the 5-arg form (options deep-merged over defaults like every
   other op), and `GetImageInfo` / `ffi.go` are coded against it.
   Rebuild the library fresh (`bash packages/go/scripts/build-ffi.sh`)
   before running `go test`, so the tests run against the updated lib.

Threading note: the FFI last error is thread-local, so every FFI
call and its error read run pinned to one OS thread
(`runtime.LockOSThread` in `ffi.go`). The image functions themselves
are safe to call concurrently from multiple goroutines.
