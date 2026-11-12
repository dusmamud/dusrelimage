import Foundation

// ---------------------------------------------------------------------------
// File-scope forwarding to the generated top-level functions.
//
// The generated functions in `dusrelimage.swift` (`compressImage`,
// `version`, …) and the wrapper's static methods below deliberately
// share names — the wrapper IS the public API, with `Data` in/out and
// defaults. Inside `enum DusrelImage`, an unqualified call such as
// `compressImage(...)` would resolve to the wrapper's own static
// method (infinite recursion), and `DusrelImage.compressImage(...)`
// resolves to the same static method, not the generated function.
// So each generated function is bound once here, at file scope —
// where no type member shadows it — under an `ffi`-prefixed name,
// and the wrapper methods below call these forwarders. (The Kotlin
// wrapper does the same thing with a package qualifier;
// `com.dusrel.image.compressImage(...)`. Swift has no equivalent
// module qualifier when the module and the type share a name.)
// ---------------------------------------------------------------------------

private func ffiCompressImage(_ input: [UInt8], _ options: CompressOptions) throws -> [UInt8] {
    try compressImage(input: input, options: options)
}

private func ffiCompressToSize(_ input: [UInt8], _ targetBytes: UInt64, _ options: CompressOptions) throws -> [UInt8] {
    try compressToSize(input: input, targetBytes: targetBytes, options: options)
}

private func ffiResizeImage(_ input: [UInt8], _ options: ResizeOptions) throws -> [UInt8] {
    try resizeImage(input: input, options: options)
}

private func ffiCropImage(_ input: [UInt8], _ options: CropOptions) throws -> [UInt8] {
    try cropImage(input: input, options: options)
}

private func ffiConvertImage(_ input: [UInt8], _ options: ConvertOptions) throws -> [UInt8] {
    try convertImage(input: input, options: options)
}

private func ffiAddWatermark(_ input: [UInt8], _ options: WatermarkOptions) throws -> [UInt8] {
    try addWatermark(input: input, options: options)
}

private func ffiAnnotateImage(_ input: [UInt8], _ annotations: [Annotation]) throws -> [UInt8] {
    try annotateImage(input: input, annotations: annotations)
}

private func ffiPickColour(_ input: [UInt8], _ x: UInt32, _ y: UInt32) throws -> Colour {
    try pickColour(input: input, x: x, y: y)
}

private func ffiExtractPalette(_ input: [UInt8], _ count: UInt8) throws -> [PaletteColour] {
    try extractPalette(input: input, count: count)
}

private func ffiMakeCollage(_ inputs: [[UInt8]], _ options: CollageOptions) throws -> [UInt8] {
    try makeCollage(inputs: inputs, options: options)
}

private func ffiPixelateImage(_ input: [UInt8], _ options: PixelateOptions) throws -> [UInt8] {
    try pixelateImage(input: input, options: options)
}

private func ffiGetImageInfo(_ input: [UInt8], _ options: CommonOptions) throws -> ImageInfo {
    try getImageInfo(input: input, options: options)
}

private func ffiBulkProcess(_ inputs: [[UInt8]], _ op: BulkOp) -> [BulkItemResult] {
    bulkProcess(inputs: inputs, op: op)
}

private func ffiMakeTestImage(_ kind: String, _ width: UInt32, _ height: UInt32, _ seed: UInt64) throws -> [UInt8] {
    try makeTestImage(kind: kind, width: width, height: height, seed: seed)
}

private func ffiVersion() -> String {
    version()
}

/// DusrelImage — thin idiomatic Swift wrapper over the UniFFI-generated
/// bindings in `dusrelimage.swift` (same module, generated from
/// `crates/dusrelimage-uniffi/src/dusrelimage.udl` with uniffi 0.29.5).
///
/// What this wrapper adds over the generated API:
/// - `Data` in / out (the generated functions use `[UInt8]`).
/// - Swift-friendly defaults: every `*Options` parameter defaults to a
///   fully-defaulted options object. The UDL cannot give a default to a
///   field that is itself a dictionary, so the generated `common`,
///   `background` and `layout` fields are REQUIRED there — this wrapper
///   fills them (`CommonOptions()` with all UDL defaults, white
///   `#FFFFFF`, `CollageLayout.grid(rows: 2, cols: 2)`) where the caller
///   does not pass them. The defaults reproduce the Rust `Default`
///   values (SPEC section 2) exactly.
/// - One error type: the generated `DusrelError` enum itself (see the
///   extension below). Unlike Kotlin, where UniFFI generates a
///   `DusrelException` class hierarchy, UniFFI's Swift generator emits
///   the error enum directly as `DusrelError`, with one case per stable
///   code, each carrying `code`, `messageEn` and `messageHi`. There is
///   deliberately NO separate wrapper error struct — a second type
///   named `DusrelError` would collide with the generated enum in this
///   module. The extension below adds uniform `code` / `messageEn` /
///   `messageHi` accessors (plus the `reasonEn` / `reasonHi` aliases
///   the JS/Python wrappers expose) so callers read the same shape
///   from every case.
/// - Colour helpers (`white()`, `black()`, `colourFromRgb(_:_:_:_:)`,
///   `colourFromHex(_:)`): the generated `Colour` record has no
///   defaults, so callers build colours through these (hex + HSL are
///   derived from RGB, exactly as the Rust core does — the Rust side
///   re-derives them from RGBA anyway, so a caller-supplied hex/HSL
///   that disagrees is normalised).
///
/// All image work still happens in the Rust core via UniFFI/FFI; this
/// file contains no image logic. Honest v0.1.0 limits (same as the
/// core): AVIF input does not decode, WebP output is always lossless,
/// `keepExif` / `dpi` / `progressive` are accepted but not honoured,
/// `compress_to_size` quality search is JPEG/AVIF-only, TIFF input
/// falls back to PNG, collage output is always PNG, PNG `palette`
/// quantise flag is not implemented, GIF output is the first frame
/// only, no HEIC, no streaming.
public enum DusrelImage {

    // ------------------------------------------------------------------
    // Colours
    // ------------------------------------------------------------------

    /// White `#FFFFFF` — the background default for convert / resize-fill / collage.
    public static func white() -> Colour {
        colourFromRgb(255, 255, 255)
    }

    /// Black `#000000`.
    public static func black() -> Colour {
        colourFromRgb(0, 0, 0)
    }

    /// Build a `Colour` from 8-bit channels. `hex` is always `"#RRGGBB"`
    /// (alpha is NOT in the hex string) and `hsl` is
    /// `[h_degrees_0..360, s_0..1, l_0..1]` — both derived from RGB,
    /// mirroring `Colour::from_rgba` in the Rust core.
    public static func colourFromRgb(_ r: UInt8, _ g: UInt8, _ b: UInt8, _ a: UInt8 = 255) -> Colour {
        Colour(
            hex: String(format: "#%02X%02X%02X", r, g, b),
            r: r,
            g: g,
            b: b,
            a: a,
            hsl: rgbToHsl(r, g, b)
        )
    }

    /// Parse `"#RRGGBB"`, `"RRGGBB"`, `"#RGB"` or `"#RRGGBBAA"` — the
    /// same forms as `Colour::from_hex` in the Rust core. Anything else
    /// throws `DusrelError.InvalidArgument` with code `INVALID_ARGUMENT`.
    public static func colourFromHex(_ hex: String) throws -> Colour {
        let h = hex.hasPrefix("#") ? String(hex.dropFirst()) : hex
        func bad() throws -> Never {
            throw DusrelError.InvalidArgument(
                code: "INVALID_ARGUMENT",
                messageEn: "Invalid hex colour \"\(hex)\". Use \"#RRGGBB\".",
                messageHi: "Galat hex colour \"\(hex)\" hai. \"#RRGGBB\" use karein."
            )
        }
        func nibble(_ index: String.Index) throws -> UInt8 {
            guard let v = UInt8(String(h[index...index]), radix: 16) else { try bad() }
            return v &* 16 &+ v
        }
        func pair(_ start: Int) throws -> UInt8 {
            let from = h.index(h.startIndex, offsetBy: start)
            let to = h.index(h.startIndex, offsetBy: start + 2)
            guard let v = UInt8(String(h[from..<to]), radix: 16) else { try bad() }
            return v
        }
        let r: UInt8
        let g: UInt8
        let b: UInt8
        var a: UInt8 = 255
        switch h.count {
        case 3:
            r = try nibble(h.startIndex)
            g = try nibble(h.index(h.startIndex, offsetBy: 1))
            b = try nibble(h.index(h.startIndex, offsetBy: 2))
        case 6:
            r = try pair(0)
            g = try pair(2)
            b = try pair(4)
        case 8:
            r = try pair(0)
            g = try pair(2)
            b = try pair(4)
            a = try pair(6)
        default:
            try bad()
        }
        return colourFromRgb(r, g, b, a)
    }

    /// RGB (0-255) -> HSL (h degrees 0..360, s/l 0..1); mirrors the core `rgb_to_hsl`.
    internal static func rgbToHsl(_ r: UInt8, _ g: UInt8, _ b: UInt8) -> [Float] {
        let rf = Float(r) / 255
        let gf = Float(g) / 255
        let bf = Float(b) / 255
        let max = Swift.max(rf, gf, bf)
        let min = Swift.min(rf, gf, bf)
        let l = (max + min) / 2
        if max - min < 1e-6 { return [0, 0, l] }
        let d = max - min
        let s: Float = l > 0.5 ? d / (2 - max - min) : d / (max + min)
        let h: Float
        if max == rf {
            h = 60 * remEuclid((gf - bf) / d, 6)
        } else if max == gf {
            h = 60 * ((bf - rf) / d + 2)
        } else {
            h = 60 * ((rf - gf) / d + 4)
        }
        return [h, s, l]
    }

    /// Non-negative remainder, like Rust's `f32::rem_euclid`.
    private static func remEuclid(_ x: Float, _ n: Float) -> Float {
        let r = x.truncatingRemainder(dividingBy: n)
        return r >= 0 ? r : r + n
    }

    // ------------------------------------------------------------------
    // Small option builders
    // ------------------------------------------------------------------

    /// Text watermark: string, size in px, colour (default white, like the Rust `Default`).
    public static func textWatermark(_ text: String, size: Float = 48, colour: Colour? = nil) -> TextWatermark {
        TextWatermark(text: text, size: size, colour: colour ?? white())
    }

    /// Logo watermark: raw image bytes + scale (`1.0` = natural size).
    public static func imageWatermark(_ image: Data, scale: Float = 1) -> ImageWatermark {
        ImageWatermark(image: [UInt8](image), scale: scale)
    }

    // ------------------------------------------------------------------
    // Tools
    // ------------------------------------------------------------------

    /// Crate version, e.g. `"0.1.0"` (lockstep with npm / PyPI / crates.io).
    public static func version() -> String {
        ffiVersion()
    }

    /// Tool 1 — re-encode an image at a fixed quality (and `format`, or the input format when unset).
    public static func compressImage(
        _ input: Data,
        options: CompressOptions = CompressOptions(common: CommonOptions())
    ) throws -> Data {
        Data(try ffiCompressImage([UInt8](input), options))
    }

    /// Tool 1b — re-encode so the result is at most `targetBytes` bytes, at the highest quality that fits.
    public static func compressToSize(
        _ input: Data,
        targetBytes: UInt64,
        options: CompressOptions = CompressOptions(common: CommonOptions())
    ) throws -> Data {
        Data(try ffiCompressToSize([UInt8](input), targetBytes, options))
    }

    /// Tool 2 — resize (width and/or height, percent, fit, filter).
    public static func resizeImage(
        _ input: Data,
        options: ResizeOptions = ResizeOptions(common: CommonOptions(), background: white())
    ) throws -> Data {
        Data(try ffiResizeImage([UInt8](input), options))
    }

    /// Tool 3 — crop to a rectangle (or an anchor preset).
    public static func cropImage(_ input: Data, options: CropOptions) throws -> Data {
        Data(try ffiCropImage([UInt8](input), options))
    }

    /// Tool 3 convenience — explicit rectangle; `anchor` places the rect and ignores `x`/`y`.
    public static func cropImage(
        _ input: Data,
        x: UInt32 = 0,
        y: UInt32 = 0,
        width: UInt32,
        height: UInt32,
        anchor: Position? = nil,
        common: CommonOptions = CommonOptions()
    ) throws -> Data {
        try cropImage(
            input,
            options: CropOptions(common: common, x: x, y: y, width: width, height: height, anchor: anchor)
        )
    }

    /// Tool 4 — convert to another format.
    public static func convertImage(_ input: Data, options: ConvertOptions) throws -> Data {
        Data(try ffiConvertImage([UInt8](input), options))
    }

    /// Tool 4 convenience — target format first; background flattens alpha for JPEG/BMP.
    public static func convertImage(
        _ input: Data,
        format: ImageFormat,
        quality: UInt8 = 80,
        background: Colour? = nil,
        progressive: Bool = false,
        common: CommonOptions = CommonOptions()
    ) throws -> Data {
        try convertImage(
            input,
            options: ConvertOptions(
                common: common,
                format: format,
                quality: quality,
                background: background ?? white(),
                progressive: progressive
            )
        )
    }

    /// Tool 5 — stamp a text or logo watermark. At least one of `text` / `logo` is required.
    public static func addWatermark(
        _ input: Data,
        options: WatermarkOptions = WatermarkOptions(common: CommonOptions())
    ) throws -> Data {
        Data(try ffiAddWatermark([UInt8](input), options))
    }

    /// Tool 6 — draw annotations (arrows, rects, ellipses, lines, text).
    public static func annotateImage(_ input: Data, annotations: [Annotation]) throws -> Data {
        Data(try ffiAnnotateImage([UInt8](input), annotations))
    }

    /// Tool 7 — read the colour of one pixel. Out-of-bounds is `PIXEL_OUT_OF_BOUNDS`.
    public static func pickColour(_ input: Data, x: UInt32, y: UInt32) throws -> Colour {
        try ffiPickColour([UInt8](input), x, y)
    }

    /// Tool 8 — dominant colours, most frequent first (count 1-16).
    public static func extractPalette(_ input: Data, count: UInt8 = 5) throws -> [PaletteColour] {
        try ffiExtractPalette([UInt8](input), count)
    }

    /// Tool 9 — build a collage from several images. The output is always PNG.
    public static func makeCollage(
        _ inputs: [Data],
        options: CollageOptions = CollageOptions(
            common: CommonOptions(),
            layout: .grid(rows: 2, cols: 2),
            background: white()
        )
    ) throws -> Data {
        Data(try ffiMakeCollage(inputs.map { [UInt8]($0) }, options))
    }

    /// Tool 10 — pixelate a rectangle (whole image when width/height are 0).
    public static func pixelateImage(
        _ input: Data,
        options: PixelateOptions = PixelateOptions(common: CommonOptions())
    ) throws -> Data {
        Data(try ffiPixelateImage([UInt8](input), options))
    }

    /// Describe an image without modifying it.
    public static func getImageInfo(
        _ input: Data,
        options: CommonOptions = CommonOptions()
    ) throws -> ImageInfo {
        try ffiGetImageInfo([UInt8](input), options)
    }

    /// Run one operation over many inputs in a single call. One bad input
    /// fails only its own item (as `BulkResult.failed` with the per-item
    /// `DusrelError`), never the whole batch. An empty input list returns
    /// an empty list. `BulkOp.collage` is not a per-item op: it returns
    /// one `INVALID_ARGUMENT` result per input — call `makeCollage` once
    /// with the full list instead.
    public static func bulkProcess(_ inputs: [Data], op: BulkOp) -> [BulkResult] {
        let results = ffiBulkProcess(inputs.map { [UInt8]($0) }, op)
        return results.map { item in
            if !item.ok {
                return .failed(
                    index: item.index,
                    error: DusrelError.from(
                        code: item.errorCode ?? "INVALID_ARGUMENT",
                        messageEn: item.errorMessageEn ?? "",
                        messageHi: item.errorMessageHi ?? ""
                    )
                )
            }
            switch item.output {
            case let .image(bytes):
                return .image(index: item.index, bytes: Data(bytes))
            case let .singleColour(colour):
                return .colour(index: item.index, colour: colour)
            case let .palette(palette):
                return .palette(index: item.index, palette: palette)
            case nil:
                return .failed(
                    index: item.index,
                    error: DusrelError.InvalidArgument(
                        code: "INVALID_ARGUMENT",
                        messageEn: "Bulk item succeeded with no output.",
                        messageHi: "Bulk item bina output ke safal hua."
                    )
                )
            }
        }
    }

    /// Build a deterministic test image (PNG bytes). `kind` is `"solid"`,
    /// `"gradient"` or `"noise"`; anything else is `INVALID_ARGUMENT`.
    /// Same builders as the CLI `mock` command — the shared-vector tests
    /// use this for fixtures instead of shelling out.
    public static func makeTestImage(
        kind: String = "noise",
        width: UInt32 = 64,
        height: UInt32 = 64,
        seed: UInt64 = 42
    ) throws -> Data {
        Data(try ffiMakeTestImage(kind, width, height, seed))
    }
}

// ---------------------------------------------------------------------------
// Error accessors — uniform `code` / `messageEn` / `messageHi` over the
// generated `DusrelError` enum (one case per stable code, each carrying
// the same three payloads). This is what the Kotlin wrapper's
// `DusrelError` class and the JS/Python `DusrelError` provide; in Swift
// the generated enum IS that type, so the accessors live here instead
// of in a second type.
// ---------------------------------------------------------------------------

public extension DusrelError {

    /// Stable, machine-readable code, e.g. `"INVALID_IMAGE"`.
    public var code: String {
        switch self {
        case let .InvalidImage(code, _, _): return code
        case let .UnsupportedFormat(code, _, _): return code
        case let .ImageTooLarge(code, _, _): return code
        case let .TargetSizeUnreachable(code, _, _): return code
        case let .CropOutOfBounds(code, _, _): return code
        case let .PixelOutOfBounds(code, _, _): return code
        case let .InvalidQuality(code, _, _): return code
        case let .InvalidDimensions(code, _, _): return code
        case let .InvalidArgument(code, _, _): return code
        case let .EmptyInput(code, _, _): return code
        case let .InvalidType(code, _, _): return code
        }
    }

    /// English message from the Rust core.
    public var messageEn: String {
        switch self {
        case let .InvalidImage(_, messageEn, _): return messageEn
        case let .UnsupportedFormat(_, messageEn, _): return messageEn
        case let .ImageTooLarge(_, messageEn, _): return messageEn
        case let .TargetSizeUnreachable(_, messageEn, _): return messageEn
        case let .CropOutOfBounds(_, messageEn, _): return messageEn
        case let .PixelOutOfBounds(_, messageEn, _): return messageEn
        case let .InvalidQuality(_, messageEn, _): return messageEn
        case let .InvalidDimensions(_, messageEn, _): return messageEn
        case let .InvalidArgument(_, messageEn, _): return messageEn
        case let .EmptyInput(_, messageEn, _): return messageEn
        case let .InvalidType(_, messageEn, _): return messageEn
        }
    }

    /// Hindi message from the Rust core.
    public var messageHi: String {
        switch self {
        case let .InvalidImage(_, _, messageHi): return messageHi
        case let .UnsupportedFormat(_, _, messageHi): return messageHi
        case let .ImageTooLarge(_, _, messageHi): return messageHi
        case let .TargetSizeUnreachable(_, _, messageHi): return messageHi
        case let .CropOutOfBounds(_, _, messageHi): return messageHi
        case let .PixelOutOfBounds(_, _, messageHi): return messageHi
        case let .InvalidQuality(_, _, messageHi): return messageHi
        case let .InvalidDimensions(_, _, messageHi): return messageHi
        case let .InvalidArgument(_, _, messageHi): return messageHi
        case let .EmptyInput(_, _, messageHi): return messageHi
        case let .InvalidType(_, _, messageHi): return messageHi
        }
    }

    /// Alias of `messageEn` (matches the JS/Python wrappers).
    public var reasonEn: String { messageEn }

    /// Alias of `messageHi`.
    public var reasonHi: String { messageHi }

    /// Build the enum case for a stable code string (used for bulk
    /// per-item errors, which arrive as flat code + messages). An
    /// unknown code falls back to `.InvalidArgument` but keeps the
    /// original code string in the payload, so `error.code` is always
    /// exactly what the core reported.
    static func from(code: String, messageEn: String, messageHi: String) -> DusrelError {
        switch code {
        case "INVALID_IMAGE": return .InvalidImage(code: code, messageEn: messageEn, messageHi: messageHi)
        case "UNSUPPORTED_FORMAT": return .UnsupportedFormat(code: code, messageEn: messageEn, messageHi: messageHi)
        case "IMAGE_TOO_LARGE": return .ImageTooLarge(code: code, messageEn: messageEn, messageHi: messageHi)
        case "TARGET_SIZE_UNREACHABLE": return .TargetSizeUnreachable(code: code, messageEn: messageEn, messageHi: messageHi)
        case "CROP_OUT_OF_BOUNDS": return .CropOutOfBounds(code: code, messageEn: messageEn, messageHi: messageHi)
        case "PIXEL_OUT_OF_BOUNDS": return .PixelOutOfBounds(code: code, messageEn: messageEn, messageHi: messageHi)
        case "INVALID_QUALITY": return .InvalidQuality(code: code, messageEn: messageEn, messageHi: messageHi)
        case "INVALID_DIMENSIONS": return .InvalidDimensions(code: code, messageEn: messageEn, messageHi: messageHi)
        case "INVALID_ARGUMENT": return .InvalidArgument(code: code, messageEn: messageEn, messageHi: messageHi)
        case "EMPTY_INPUT": return .EmptyInput(code: code, messageEn: messageEn, messageHi: messageHi)
        case "INVALID_TYPE": return .InvalidType(code: code, messageEn: messageEn, messageHi: messageHi)
        default: return .InvalidArgument(code: code, messageEn: messageEn, messageHi: messageHi)
        }
    }
}

/// One item of a `DusrelImage.bulkProcess` result, in input order.
public enum BulkResult {
    /// The op produced image bytes.
    case image(index: UInt32, bytes: Data)
    /// The op produced a single colour (`BulkOp.pickColour`).
    case colour(index: UInt32, colour: Colour)
    /// The op produced a palette (`BulkOp.palette`).
    case palette(index: UInt32, palette: [PaletteColour])
    /// The op failed for this input only; the batch continues.
    case failed(index: UInt32, error: DusrelError)
}
