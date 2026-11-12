import Foundation
import XCTest
@testable import DusrelImage

/// Shared test vectors (SPEC section 6): Rust, Python, JS, Kotlin and
/// Swift all run `shared/test-vectors.json` and must agree.
///
/// This runner mirrors `packages/js/src/vectors.test.ts` and the
/// Kotlin `VectorsTest` case-for-case: it loads the JSON, builds each
/// fixture — `dusrelimage mock` kinds (`solid`/`gradient`/`noise` +
/// seed) via `DusrelImage.makeTestImage` (the same Rust builders as
/// the CLI `mock` command), plus the documented derived fixtures
/// (`solid_rgb`, `transparent_rgba`, `two_colour`, `tiff`, `jpeg`,
/// `avif`, `garbage`, `empty`) built in `TestSupport.swift` with a
/// minimal PNG/TIFF encoder (no binary blobs committed) — runs the op
/// through the public `DusrelImage` wrapper, and asserts the expected
/// dimensions / pixel spot-checks / error code.
///
/// It FAILS LOUDLY on any case whose fixture/op kind it cannot handle:
/// unknown fixture kinds, ops, bulk ops or expected keys throw naming
/// the case id instead of silently skipping the case. Pixel checks use
/// `DusrelImage.pickColour` per pixel (the core decodes), so no
/// PNG/JPEG decoder is needed in the tests; AVIF output cannot be
/// decoded back in this build (no avif-native decoder): a case that
/// only asserts the header/size skips the decode, anything asserting
/// dims must be decodable.
final class VectorsTests: XCTestCase {

    /// Expected keys this runner knows how to assert. Anything else is a schema gap.
    static let knownExpectedKeys: Set<String> = [
        "format", "width", "height", "has_alpha", "colour_type", "error",
        "max_bytes", "avif_header", "pixels", "region_all_white",
        "region_has_dark", "regions_have_dark", "colour", "different_from",
        "palette_hexes", "palette_length", "share_each", "share_sum",
        "share_sum_tolerance", "share_tolerance", "palette",
        "differs_from_input", "pixel_equalities", "unchanged_pixels",
        "results",
    ]

    func testVectorCountIs82() throws {
        XCTAssertEqual(try loadCases().count, 82)
    }

    /// One test method loops all 82 cases (XCTest has no parameterized
    /// runner); every failure names its case id, and the test fails
    /// with the full list — no case is ever silently skipped.
    func testAllVectors() throws {
        let cases = try loadCases()
        var failures: [String] = []
        for c in cases {
            guard let caseId = jsonString(c, "id") else {
                failures.append("case without id: \(c)")
                continue
            }
            var outcome: Outcome? = nil
            var error: DusrelError? = nil
            var input: Data? = nil
            do {
                let run = try runCase(c, caseId: caseId)
                outcome = run.outcome
                input = run.input
            } catch let e as DusrelError {
                error = e
                if let fixture = jsonDict(c["fixture"]) {
                    input = try? fixtureBytes(fixture, caseId: caseId)
                }
            } catch {
                failures.append("case \(caseId): unexpected error \(error)")
                continue
            }
            assertExpected(c, caseId: caseId, outcome: outcome, error: error, input: input, failures: &failures)
        }
        XCTAssertTrue(failures.isEmpty, "shared vector failures:\n" + failures.joined(separator: "\n"))
    }
}

// ---------------------------------------------------------------------------
// Options translation (JSON snake_case -> Swift wrapper options)
// ---------------------------------------------------------------------------

private func checkKnownOptions(_ options: [String: Any], known: Set<String>, caseId: String, what: String) throws {
    for key in options.keys where !known.contains(key) {
        throw TestError.message("case \(caseId): unknown \(what) option \(key) — update this runner")
    }
}

private func imageFormatOf(_ name: String, caseId: String) throws -> ImageFormat {
    switch name {
    case "Jpeg": return .jpeg
    case "Png": return .png
    case "WebP": return .webP
    case "Avif": return .avif
    case "Gif": return .gif
    case "Bmp": return .bmp
    default: throw TestError.message("case \(caseId): unknown ImageFormat \(name)")
    }
}

private func fitModeOf(_ name: String, caseId: String) throws -> FitMode {
    switch name {
    case "Fit": return .fit
    case "Fill": return .fill
    case "Stretch": return .stretch
    case "Crop": return .crop
    default: throw TestError.message("case \(caseId): unknown FitMode \(name)")
    }
}

private func filterOf(_ name: String, caseId: String) throws -> Filter {
    switch name {
    case "Nearest": return .nearest
    case "Triangle": return .triangle
    case "CatmullRom": return .catmullRom
    case "Gaussian": return .gaussian
    case "Lanczos3": return .lanczos3
    default: throw TestError.message("case \(caseId): unknown Filter \(name)")
    }
}

private func positionOf(_ name: String, caseId: String) throws -> Position {
    switch name {
    case "TopLeft": return .topLeft
    case "TopCenter": return .topCenter
    case "TopRight": return .topRight
    case "CenterLeft": return .centerLeft
    case "Center": return .center
    case "CenterRight": return .centerRight
    case "BottomLeft": return .bottomLeft
    case "BottomCenter": return .bottomCenter
    case "BottomRight": return .bottomRight
    case "Tiled": return .tiled
    default: throw TestError.message("case \(caseId): unknown Position \(name)")
    }
}

private func toCommon(_ options: [String: Any], caseId: String) throws -> CommonOptions {
    let common = jsonDict(options["common"]) ?? [:]
    try checkKnownOptions(common, known: ["auto_orient", "keep_exif", "dpi", "max_pixels", "max_dimension"], caseId: caseId, what: "common")
    var dpi: Dpi? = nil
    if let dpiValue = common["dpi"], !(dpiValue is NSNull) {
        if let arr = jsonArray(dpiValue), arr.count == 2,
           let w = (arr[0] as? NSNumber)?.uint32Value,
           let h = (arr[1] as? NSNumber)?.uint32Value {
            dpi = Dpi(width: w, height: h)
        } else if let dict = jsonDict(dpiValue),
                  let w = jsonUInt32(dict, "width"),
                  let h = jsonUInt32(dict, "height") {
            dpi = Dpi(width: w, height: h)
        } else {
            throw TestError.message("case \(caseId): unknown dpi shape: \(dpiValue)")
        }
    }
    return CommonOptions(
        autoOrient: jsonBool(common, "auto_orient") ?? true,
        keepExif: jsonBool(common, "keep_exif") ?? false,
        dpi: dpi,
        maxPixels: (common["max_pixels"] as? NSNumber)?.uint64Value ?? 120_000_000,
        maxDimension: jsonUInt32(common, "max_dimension") ?? 16000
    )
}

private func toCompressOptions(_ options: [String: Any], caseId: String) throws -> CompressOptions {
    try checkKnownOptions(options, known: ["common", "quality", "format", "progressive", "lossless", "effort"], caseId: caseId, what: "compress")
    var format: ImageFormat? = nil
    if let name = jsonString(options, "format") {
        format = try imageFormatOf(name, caseId: caseId)
    }
    return CompressOptions(
        common: try toCommon(options, caseId: caseId),
        quality: UInt8(jsonInt(options, "quality") ?? 80),
        format: format,
        progressive: jsonBool(options, "progressive") ?? false,
        lossless: jsonBool(options, "lossless") ?? false,
        effort: UInt8(jsonInt(options, "effort") ?? 4)
    )
}

private func toResizeOptions(_ options: [String: Any], caseId: String) throws -> ResizeOptions {
    try checkKnownOptions(options, known: ["common", "width", "height", "percent", "fit", "filter", "background"], caseId: caseId, what: "resize")
    return ResizeOptions(
        common: try toCommon(options, caseId: caseId),
        width: jsonUInt32(options, "width"),
        height: jsonUInt32(options, "height"),
        percent: jsonDouble(options, "percent").map { Float($0) },
        fit: try options["fit"].map { try fitModeOf($0 as? String ?? "", caseId: caseId) } ?? .fit,
        filter: try options["filter"].map { try filterOf($0 as? String ?? "", caseId: caseId) } ?? .lanczos3,
        background: try options["background"].map { try DusrelImage.colourFromHex($0 as? String ?? "") } ?? DusrelImage.white()
    )
}

private func toCropOptions(_ options: [String: Any], caseId: String) throws -> CropOptions {
    try checkKnownOptions(options, known: ["common", "x", "y", "width", "height", "anchor"], caseId: caseId, what: "crop")
    guard let width = jsonUInt32(options, "width"), let height = jsonUInt32(options, "height") else {
        throw TestError.message("case \(caseId): crop needs width and height")
    }
    var anchor: Position? = nil
    if let name = jsonString(options, "anchor") {
        anchor = try positionOf(name, caseId: caseId)
    }
    return CropOptions(
        common: try toCommon(options, caseId: caseId),
        x: jsonUInt32(options, "x") ?? 0,
        y: jsonUInt32(options, "y") ?? 0,
        width: width,
        height: height,
        anchor: anchor
    )
}

private func toConvertOptions(_ options: [String: Any], caseId: String) throws -> ConvertOptions {
    try checkKnownOptions(options, known: ["common", "format", "quality", "background", "progressive"], caseId: caseId, what: "convert")
    guard let formatName = jsonString(options, "format") else {
        throw TestError.message("case \(caseId): convert needs a format")
    }
    return ConvertOptions(
        common: try toCommon(options, caseId: caseId),
        format: try imageFormatOf(formatName, caseId: caseId),
        quality: UInt8(jsonInt(options, "quality") ?? 80),
        background: try options["background"].map { try DusrelImage.colourFromHex($0 as? String ?? "") } ?? DusrelImage.white(),
        progressive: jsonBool(options, "progressive") ?? false
    )
}

private func toWatermarkOptions(_ options: [String: Any], caseId: String) throws -> WatermarkOptions {
    try checkKnownOptions(options, known: ["common", "text", "logo", "opacity", "position", "rotation_deg", "margin"], caseId: caseId, what: "watermark")
    var text: TextWatermark? = nil
    if let t = jsonDict(options["text"]) {
        guard let textString = jsonString(t, "text") else {
            throw TestError.message("case \(caseId): watermark text needs a string")
        }
        text = TextWatermark(
            text: textString,
            size: jsonDouble(t, "size").map { Float($0) } ?? 48,
            colour: try DusrelImage.colourFromHex(jsonString(t, "colour") ?? "#FFFFFF")
        )
    }
    var logo: ImageWatermark? = nil
    if let l = jsonDict(options["logo"]) {
        guard let imageFixture = jsonDict(l["image"]) else {
            throw TestError.message("case \(caseId): watermark logo needs an image fixture")
        }
        logo = ImageWatermark(
            image: [UInt8](try fixtureBytes(imageFixture, caseId: caseId)),
            scale: jsonDouble(l, "scale").map { Float($0) } ?? 1
        )
    }
    var position: Position = .bottomRight
    if let name = jsonString(options, "position") {
        position = try positionOf(name, caseId: caseId)
    }
    return WatermarkOptions(
        common: try toCommon(options, caseId: caseId),
        text: text,
        logo: logo,
        opacity: jsonDouble(options, "opacity").map { Float($0) } ?? 0.5,
        position: position,
        rotationDeg: jsonDouble(options, "rotation_deg").map { Float($0) } ?? 0,
        margin: jsonUInt32(options, "margin") ?? 16
    )
}

private func toCollageOptions(_ options: [String: Any], caseId: String) throws -> CollageOptions {
    try checkKnownOptions(options, known: ["common", "layout", "gap", "background", "width", "height", "cell_fit"], caseId: caseId, what: "collage")
    var layout: CollageLayout = .grid(rows: 2, cols: 2)
    if let raw = options["layout"] {
        if let name = raw as? String {
            switch name.lowercased() {
            case "row": layout = .row
            case "column": layout = .column
            case "grid": layout = .grid(rows: 2, cols: 2)
            default: throw TestError.message("case \(caseId): unknown collage layout \(name)")
            }
        } else if let dict = jsonDict(raw), let grid = jsonDict(dict["Grid"]),
                  let rows = jsonUInt32(grid, "rows"),
                  let cols = jsonUInt32(grid, "cols") {
            layout = .grid(rows: rows, cols: cols)
        } else {
            throw TestError.message("case \(caseId): unknown collage layout \(raw)")
        }
    }
    var cellFit: FitMode = .fit
    if let name = jsonString(options, "cell_fit") {
        cellFit = try fitModeOf(name, caseId: caseId)
    }
    return CollageOptions(
        common: try toCommon(options, caseId: caseId),
        layout: layout,
        gap: jsonUInt32(options, "gap") ?? 8,
        background: try options["background"].map { try DusrelImage.colourFromHex($0 as? String ?? "") } ?? DusrelImage.white(),
        width: jsonUInt32(options, "width") ?? 1200,
        height: jsonUInt32(options, "height") ?? 1200,
        cellFit: cellFit
    )
}

private func toPixelateOptions(_ options: [String: Any], caseId: String) throws -> PixelateOptions {
    try checkKnownOptions(options, known: ["common", "x", "y", "width", "height", "block_size"], caseId: caseId, what: "pixelate")
    return PixelateOptions(
        common: try toCommon(options, caseId: caseId),
        x: jsonUInt32(options, "x") ?? 0,
        y: jsonUInt32(options, "y") ?? 0,
        width: jsonUInt32(options, "width") ?? 0,
        height: jsonUInt32(options, "height") ?? 0,
        blockSize: jsonUInt32(options, "block_size") ?? 10
    )
}

private func toAnnotations(_ value: Any?, caseId: String) throws -> [Annotation] {
    guard let arr = jsonArray(value) else {
        throw TestError.message("case \(caseId): annotations must be an array")
    }
    return try arr.map { item in
        guard let a = jsonDict(item), let type = jsonString(a, "type") else {
            throw TestError.message("case \(caseId): annotation needs a type")
        }
        func point(_ key: String) throws -> Point {
            guard let pair = jsonArray(a[key]), pair.count == 2,
                  let x = (pair[0] as? NSNumber)?.uint32Value,
                  let y = (pair[1] as? NSNumber)?.uint32Value else {
                throw TestError.message("case \(caseId): annotation \(type) needs a [x, y] \(key)")
            }
            return Point(x: x, y: y)
        }
        func colour(_ key: String) throws -> Colour {
            guard let hex = jsonString(a, key) else {
                throw TestError.message("case \(caseId): annotation \(type) needs a \(key) colour")
            }
            return try DusrelImage.colourFromHex(hex)
        }
        switch type {
        case "Arrow":
            return .arrow(from: try point("from"), to: try point("to"), colour: try colour("colour"), width: jsonUInt32(a, "width") ?? 1)
        case "Line":
            return .line(from: try point("from"), to: try point("to"), colour: try colour("colour"), width: jsonUInt32(a, "width") ?? 1)
        case "Rect", "Ellipse":
            guard let x = jsonUInt32(a, "x"), let y = jsonUInt32(a, "y"),
                  let w = jsonUInt32(a, "w"), let h = jsonUInt32(a, "h") else {
                throw TestError.message("case \(caseId): annotation \(type) needs x/y/w/h")
            }
            let width = jsonUInt32(a, "width") ?? 1
            var fill: Colour? = nil
            if let fillHex = a["fill"] as? String {
                fill = try DusrelImage.colourFromHex(fillHex)
            }
            if type == "Rect" {
                return .rect(x: x, y: y, w: w, h: h, colour: try colour("colour"), width: width, fill: fill)
            } else {
                return .ellipse(x: x, y: y, w: w, h: h, colour: try colour("colour"), width: width, fill: fill)
            }
        case "Text":
            guard let x = jsonUInt32(a, "x"), let y = jsonUInt32(a, "y"),
                  let text = jsonString(a, "text") else {
                throw TestError.message("case \(caseId): annotation Text needs x/y/text")
            }
            return .text(x: x, y: y, text: text, size: jsonDouble(a, "size").map { Float($0) } ?? 48, colour: try colour("colour"))
        default:
            throw TestError.message("case \(caseId): unknown annotation type \(type) — update this runner")
        }
    }
}

// ---------------------------------------------------------------------------
// Running one case
// ---------------------------------------------------------------------------

private enum Outcome {
    case image(Data)
    case info(ImageInfo)
    case colour(Colour)
    case palette([PaletteColour])
    case bulk([BulkResult])
}

private struct RunResult {
    let outcome: Outcome
    let input: Data?
}

private func runCase(_ c: [String: Any], caseId: String) throws -> RunResult {
    let options = jsonDict(c["options"]) ?? [:]
    var inputs: [Data] = []
    var input: Data? = nil
    if let fixtures = jsonArray(c["fixtures"]) {
        inputs = try fixtures.map { try fixtureBytes(jsonDict($0) ?? [:], caseId: caseId) }
        input = inputs.first
    } else if let fixture = jsonDict(c["fixture"]) {
        input = try fixtureBytes(fixture, caseId: caseId)
        inputs = [input!]
    }
    func needImage() throws -> Data {
        guard let image = input else {
            throw TestError.message("case \(caseId): op needs one fixture")
        }
        return image
    }
    guard let op = jsonString(c, "op") else {
        throw TestError.message("case \(caseId): no op")
    }
    switch op {
    case "get_image_info":
        return RunResult(outcome: .info(try DusrelImage.getImageInfo(needImage(), options: try toCommon(options, caseId: caseId))), input: input)
    case "compress_image":
        return RunResult(outcome: .image(try DusrelImage.compressImage(needImage(), options: try toCompressOptions(options, caseId: caseId))), input: input)
    case "compress_to_size":
        guard let target = (c["target_bytes"] as? NSNumber)?.uint64Value else {
            throw TestError.message("case \(caseId): compress_to_size needs target_bytes")
        }
        return RunResult(outcome: .image(try DusrelImage.compressToSize(needImage(), targetBytes: target, options: try toCompressOptions(options, caseId: caseId))), input: input)
    case "resize_image":
        return RunResult(outcome: .image(try DusrelImage.resizeImage(needImage(), options: try toResizeOptions(options, caseId: caseId))), input: input)
    case "crop_image":
        return RunResult(outcome: .image(try DusrelImage.cropImage(needImage(), options: try toCropOptions(options, caseId: caseId))), input: input)
    case "convert_image":
        return RunResult(outcome: .image(try DusrelImage.convertImage(needImage(), options: try toConvertOptions(options, caseId: caseId))), input: input)
    case "add_watermark":
        return RunResult(outcome: .image(try DusrelImage.addWatermark(needImage(), options: try toWatermarkOptions(options, caseId: caseId))), input: input)
    case "annotate_image":
        return RunResult(outcome: .image(try DusrelImage.annotateImage(needImage(), annotations: try toAnnotations(c["annotations"], caseId: caseId))), input: input)
    case "pick_colour":
        guard let x = jsonUInt32(options, "x"), let y = jsonUInt32(options, "y") else {
            throw TestError.message("case \(caseId): pick_colour needs x/y")
        }
        return RunResult(outcome: .colour(try DusrelImage.pickColour(needImage(), x: x, y: y)), input: input)
    case "extract_palette":
        guard let count = jsonInt(options, "count") else {
            throw TestError.message("case \(caseId): extract_palette needs count")
        }
        return RunResult(outcome: .palette(try DusrelImage.extractPalette(needImage(), count: UInt8(count))), input: input)
    case "make_collage":
        return RunResult(outcome: .image(try DusrelImage.makeCollage(inputs, options: try toCollageOptions(options, caseId: caseId))), input: nil)
    case "pixelate_image":
        return RunResult(outcome: .image(try DusrelImage.pixelateImage(needImage(), options: try toPixelateOptions(options, caseId: caseId))), input: input)
    case "bulk_process":
        guard let bulkOp = jsonString(c, "bulk_op") else {
            throw TestError.message("case \(caseId): bulk_process needs bulk_op")
        }
        let op: BulkOp
        switch bulkOp {
        case "Pixelate":
            op = .pixelate(options: try toPixelateOptions(options, caseId: caseId))
        case "PickColour":
            guard let x = jsonUInt32(options, "x"), let y = jsonUInt32(options, "y") else {
                throw TestError.message("case \(caseId): bulk PickColour needs x/y")
            }
            op = .pickColour(x: x, y: y)
        case "Palette":
            guard let count = jsonInt(options, "count") else {
                throw TestError.message("case \(caseId): bulk Palette needs count")
            }
            op = .palette(count: UInt8(count))
        case "Collage":
            op = .collage(options: try toCollageOptions(options, caseId: caseId))
        case "Watermark":
            op = .watermark(options: try toWatermarkOptions(options, caseId: caseId))
        case "Annotate":
            op = .annotate(annotations: try toAnnotations(c["annotations"], caseId: caseId))
        default:
            throw TestError.message("case \(caseId): unknown bulk_op \(bulkOp) — update this runner")
        }
        return RunResult(outcome: .bulk(DusrelImage.bulkProcess(inputs, op: op)), input: nil)
    default:
        throw TestError.message("case \(caseId): unknown op \(op) — update this runner")
    }
}

// ---------------------------------------------------------------------------
// Assertions on the expected object
// ---------------------------------------------------------------------------

private func checkPixelAssert(_ px: RGBA, spec: [String: Any], caseId: String, failures: inout [String]) {
    let x = jsonInt(spec, "x") ?? -1
    let y = jsonInt(spec, "y") ?? -1
    if let want = jsonArray(spec["rgba"]) {
        let got = [px.r, px.g, px.b, px.a]
        let wantInts = want.map { ($0 as? NSNumber)?.intValue ?? -1 }
        if got != wantInts {
            failures.append("case \(caseId): pixel (\(x),\(y)) expected rgba \(wantInts), got \(got)")
        }
    }
    if let want = jsonArray(spec["rgba_approx"]) {
        let tol = jsonInt(spec, "tolerance") ?? 0
        let channels = [px.r, px.g, px.b, px.a]
        for (i, value) in want.enumerated() {
            let wantValue = (value as? NSNumber)?.intValue ?? 0
            if abs(channels[i] - wantValue) > tol {
                failures.append("case \(caseId): pixel (\(x),\(y)) channel \(i) expected ~\(wantValue)±\(tol), got \(channels[i])")
            }
        }
    }
    for (key, channel) in [("r", px.r), ("g", px.g), ("b", px.b), ("a", px.a)] {
        if let want = jsonInt(spec, key), want != channel {
            failures.append("case \(caseId): pixel (\(x),\(y)) channel \(key) expected \(want), got \(channel)")
        }
    }
    if let max = jsonInt(spec, "r_max"), px.r > max {
        failures.append("case \(caseId): pixel (\(x),\(y)) r expected <= \(max), got \(px.r)")
    }
}

private func assertExpected(
    _ c: [String: Any],
    caseId: String,
    outcome: Outcome?,
    error: DusrelError?,
    input: Data?,
    failures: inout [String]
) {
    guard let expected = jsonDict(c["expected"]) else {
        failures.append("case \(caseId): no expected object")
        return
    }
    for key in expected.keys where !VectorsTests.knownExpectedKeys.contains(key) {
        failures.append("case \(caseId): unhandled expected key \(key) — update this runner")
    }
    if let wantError = jsonString(expected, "error") {
        if error == nil {
            failures.append("case \(caseId): expected error \(wantError), got success")
        } else if error!.code != wantError {
            failures.append("case \(caseId): expected error \(wantError), got \(error!.code)")
        }
        return
    }
    if let error {
        failures.append("case \(caseId): expected success, got \(error.code) (\(error.messageEn))")
        return
    }
    guard let outcome else {
        failures.append("case \(caseId): no outcome and no error")
        return
    }

    var info: ImageInfo? = nil
    switch outcome {
    case let .info(value):
        info = value
    case let .image(bytes):
        // AVIF output cannot be decoded back in this build (no
        // avif-native decoder): a case that only asserts the header/size
        // may skip the decode; anything asserting dims must be decodable.
        do {
            info = try DusrelImage.getImageInfo(bytes)
        } catch {
            if ["format", "width", "height", "has_alpha", "colour_type"].contains(where: { expected[$0] != nil }) {
                failures.append("case \(caseId): getImageInfo on output failed with \(error)")
            }
            info = nil
        }
    default:
        break
    }
    if let info {
        if let want = jsonString(expected, "format"), info.format != want {
            failures.append("case \(caseId): expected format \(want), got \(info.format)")
        }
        if let want = jsonInt(expected, "width"), Int(info.width) != want {
            failures.append("case \(caseId): expected width \(want), got \(info.width)")
        }
        if let want = jsonInt(expected, "height"), Int(info.height) != want {
            failures.append("case \(caseId): expected height \(want), got \(info.height)")
        }
        if let want = jsonBool(expected, "has_alpha"), info.hasAlpha != want {
            failures.append("case \(caseId): expected has_alpha \(want), got \(info.hasAlpha)")
        }
        if let want = jsonString(expected, "colour_type"), info.colourType != want {
            failures.append("case \(caseId): expected colour_type \(want), got \(info.colourType)")
        }
    }

    if case let .image(bytes) = outcome {
        if let max = jsonInt(expected, "max_bytes"), bytes.count > max {
            failures.append("case \(caseId): expected <= \(max) bytes, got \(bytes.count)")
        }
        if jsonBool(expected, "avif_header") == true {
            if bytes.count < 12 || bytes.subdata(in: 4..<12) != Data("ftypavif".utf8) {
                failures.append("case \(caseId): expected AVIF ftypavif header")
            }
        }
        do {
            let pixels = Pixels(data: bytes, caseId: caseId)
            let inputPixels = input.map { Pixels(data: $0, caseId: caseId) }
            if let arr = jsonArray(expected["pixels"]) {
                for item in arr {
                    guard let spec = jsonDict(item),
                          let x = jsonInt(spec, "x"),
                          let y = jsonInt(spec, "y") else { continue }
                    checkPixelAssert(try pixels.pixel(x: x, y: y), spec: spec, caseId: caseId, failures: &failures)
                }
            }
            for (key, check) in [
                ("region_all_white", { (r: [String: Any]) throws -> Bool in
                    try pixels.regionAllWhite(x0: jsonInt(r, "x0") ?? 0, y0: jsonInt(r, "y0") ?? 0, x1: jsonInt(r, "x1") ?? 0, y1: jsonInt(r, "y1") ?? 0)
                }),
                ("region_has_dark", { (r: [String: Any]) throws -> Bool in
                    try pixels.regionHasDark(x0: jsonInt(r, "x0") ?? 0, y0: jsonInt(r, "y0") ?? 0, x1: jsonInt(r, "x1") ?? 0, y1: jsonInt(r, "y1") ?? 0)
                }),
            ] as [(String, ([String: Any]) throws -> Bool)] {
                if let region = jsonDict(expected[key]) {
                    if try !check(region) {
                        failures.append("case \(caseId): region (\(jsonInt(region, "x0") ?? 0),\(jsonInt(region, "y0") ?? 0))-(\(jsonInt(region, "x1") ?? 0),\(jsonInt(region, "y1") ?? 0)) failed \(key)")
                    }
                }
            }
            if let arr = jsonArray(expected["regions_have_dark"]) {
                for item in arr {
                    guard let r = jsonDict(item) else { continue }
                    if try !pixels.regionHasDark(x0: jsonInt(r, "x0") ?? 0, y0: jsonInt(r, "y0") ?? 0, x1: jsonInt(r, "x1") ?? 0, y1: jsonInt(r, "y1") ?? 0) {
                        failures.append("case \(caseId): region (\(jsonInt(r, "x0") ?? 0),\(jsonInt(r, "y0") ?? 0))-(\(jsonInt(r, "x1") ?? 0),\(jsonInt(r, "y1") ?? 0)) should contain dark pixels")
                    }
                }
            }
            if let inputPixels {
                if let arr = jsonArray(expected["differs_from_input"]) {
                    for item in arr {
                        guard let pair = jsonArray(item), pair.count == 2,
                              let x = (pair[0] as? NSNumber)?.intValue,
                              let y = (pair[1] as? NSNumber)?.intValue else { continue }
                        if !rgbaEqual(try pixels.pixel(x: x, y: y), try inputPixels.pixel(x: x, y: y)) {
                            failures.append("case \(caseId): pixel (\(x),\(y)) should differ from the input")
                        }
                    }
                }
                if let arr = jsonArray(expected["pixel_equalities"]) {
                    for item in arr {
                        guard let pair = jsonDict(item),
                              let a = jsonArray(pair["a"]), a.count == 2,
                              let b = jsonArray(pair["b"]), b.count == 2,
                              let ax = (a[0] as? NSNumber)?.intValue,
                              let ay = (a[1] as? NSNumber)?.intValue,
                              let bx = (b[0] as? NSNumber)?.intValue,
                              let by = (b[1] as? NSNumber)?.intValue,
                              let wantEqual = jsonBool(pair, "equal") else { continue }
                        let same = rgbaEqual(try pixels.pixel(x: ax, y: ay), try pixels.pixel(x: bx, y: by))
                        if same != wantEqual {
                            failures.append("case \(caseId): pixels (\(ax),\(ay)) vs (\(bx),\(by)) equal=\(same), expected \(wantEqual)")
                        }
                    }
                }
                if let arr = jsonArray(expected["unchanged_pixels"]) {
                    for item in arr {
                        guard let pair = jsonArray(item), pair.count == 2,
                              let x = (pair[0] as? NSNumber)?.intValue,
                              let y = (pair[1] as? NSNumber)?.intValue else { continue }
                        if !rgbaEqual(try pixels.pixel(x: x, y: y), try inputPixels.pixel(x: x, y: y)) {
                            failures.append("case \(caseId): pixel (\(x),\(y)) should be unchanged from the input")
                        }
                    }
                }
            }
        } catch {
            failures.append("case \(caseId): pixel check failed with \(error)")
        }
    }

    if case let .colour(colour) = outcome {
        if let want = jsonDict(expected["colour"]) {
            if let hex = jsonString(want, "hex"), colour.hex != hex {
                failures.append("case \(caseId): expected colour hex=\(hex), got \(colour.hex)")
            }
            if let r = jsonInt(want, "r"), Int(colour.r) != r {
                failures.append("case \(caseId): expected colour r=\(r), got \(colour.r)")
            }
            if let g = jsonInt(want, "g"), Int(colour.g) != g {
                failures.append("case \(caseId): expected colour g=\(g), got \(colour.g)")
            }
            if let b = jsonInt(want, "b"), Int(colour.b) != b {
                failures.append("case \(caseId): expected colour b=\(b), got \(colour.b)")
            }
            if let a = jsonInt(want, "a"), Int(colour.a) != a {
                failures.append("case \(caseId): expected colour a=\(a), got \(colour.a)")
            }
        }
        if let other = jsonDict(expected["different_from"]), let input {
            do {
                let otherColour = try DusrelImage.pickColour(input, x: jsonUInt32(other, "x") ?? 0, y: jsonUInt32(other, "y") ?? 0)
                if otherColour.hex == colour.hex {
                    failures.append("case \(caseId): colour should differ from the pixel at (\(jsonInt(other, "x") ?? 0),\(jsonInt(other, "y") ?? 0))")
                }
            } catch {
                failures.append("case \(caseId): different_from check failed with \(error)")
            }
        }
    }

    if case let .palette(palette) = outcome {
        if let want = jsonInt(expected, "palette_length"), palette.count != want {
            failures.append("case \(caseId): expected palette length \(want), got \(palette.count)")
        }
        if let wantArr = jsonArray(expected["palette_hexes"]) {
            // Compared as a set: the contract orders by frequency ("most
            // frequent first") and these shares are equal, so the tie order
            // is unspecified — the core breaks ties by hex ascending.
            let got = palette.map { $0.colour.hex }.sorted()
            let want = wantArr.compactMap { $0 as? String }.sorted()
            if got != want {
                failures.append("case \(caseId): expected palette hexes \(want.joined(separator: ",")), got \(got.joined(separator: ","))")
            }
        }
        let shareTol = jsonDouble(expected, "share_tolerance") ?? 0
        if let wantEach = jsonDouble(expected, "share_each") {
            for entry in palette {
                if abs(Double(entry.share) - wantEach) > shareTol {
                    failures.append("case \(caseId): palette \(entry.colour.hex) share expected \(wantEach)±\(shareTol), got \(entry.share)")
                }
            }
        }
        if let wantSum = jsonDouble(expected, "share_sum") {
            let sum = palette.reduce(0.0) { $0 + Double($1.share) }
            if abs(sum - wantSum) > (jsonDouble(expected, "share_sum_tolerance") ?? 0) {
                failures.append("case \(caseId): palette share sum expected \(wantSum), got \(sum)")
            }
        }
        if let wantArr = jsonArray(expected["palette"]) {
            if wantArr.count != palette.count {
                failures.append("case \(caseId): expected palette of \(wantArr.count), got \(palette.count)")
            } else {
                for (i, item) in wantArr.enumerated() {
                    guard let w = jsonDict(item) else { continue }
                    if let hex = jsonString(w, "hex"), palette[i].colour.hex != hex {
                        failures.append("case \(caseId): expected palette hex \(hex), got \(palette[i].colour.hex)")
                    }
                    if let share = jsonDouble(w, "share"),
                       abs(Double(palette[i].share) - share) > (jsonDouble(w, "share_tolerance") ?? 0) {
                        failures.append("case \(caseId): palette \(palette[i].colour.hex) share expected \(share), got \(palette[i].share)")
                    }
                }
            }
        }
    }

    if case let .bulk(results) = outcome {
        let wantArr = jsonArray(expected["results"]) ?? []
        if results.count != wantArr.count {
            failures.append("case \(caseId): expected \(wantArr.count) bulk results, got \(results.count)")
            return
        }
        for (i, got) in results.enumerated() {
            guard let wantItem = jsonDict(wantArr[i]) else { continue }
            let gotIndex: UInt32
            let gotOk: Bool
            let gotError: DusrelError?
            switch got {
            case let .image(index, _): gotIndex = index; gotOk = true; gotError = nil
            case let .colour(index, _): gotIndex = index; gotOk = true; gotError = nil
            case let .palette(index, _): gotIndex = index; gotOk = true; gotError = nil
            case let .failed(index, error): gotIndex = index; gotOk = false; gotError = error
            }
            let wantIndex = UInt32(jsonInt(wantItem, "index") ?? Int(gotIndex))
            if gotIndex != wantIndex {
                failures.append("case \(caseId): bulk item index expected \(wantIndex), got \(gotIndex)")
            }
            let wantOk = jsonBool(wantItem, "ok") ?? true
            if gotOk != wantOk {
                failures.append("case \(caseId): bulk item \(wantIndex) ok expected \(wantOk), got \(gotOk)")
                continue
            }
            if !gotOk {
                if let wantError = jsonString(wantItem, "error"), gotError?.code != wantError {
                    failures.append("case \(caseId): bulk item \(wantIndex) error expected \(wantError), got \(gotError?.code ?? "nil")")
                }
                continue
            }
            switch jsonString(wantItem, "output") ?? "" {
            case "Image":
                if case let .image(_, bytes) = got {
                    do {
                        let itemInfo = try DusrelImage.getImageInfo(bytes)
                        if let want = jsonInt(wantItem, "width"), Int(itemInfo.width) != want {
                            failures.append("case \(caseId): bulk item \(wantIndex) width expected \(want), got \(itemInfo.width)")
                        }
                        if let want = jsonInt(wantItem, "height"), Int(itemInfo.height) != want {
                            failures.append("case \(caseId): bulk item \(wantIndex) height expected \(want), got \(itemInfo.height)")
                        }
                    } catch {
                        failures.append("case \(caseId): bulk item \(wantIndex) getImageInfo failed with \(error)")
                    }
                } else {
                    failures.append("case \(caseId): bulk item \(wantIndex) output expected Image bytes")
                }
            case "Colour":
                if let wantColour = jsonDict(wantItem["colour"]), case let .colour(_, colour) = got {
                    if colour.hex != (jsonString(wantColour, "hex") ?? "") {
                        failures.append("case \(caseId): bulk item \(wantIndex) colour hex expected \(jsonString(wantColour, "hex") ?? ""), got \(colour.hex)")
                    }
                }
            case "Palette":
                if case let .palette(_, palette) = got {
                    if let want = jsonInt(wantItem, "palette_length"), palette.count != want {
                        failures.append("case \(caseId): bulk item \(wantIndex) palette length expected \(want), got \(palette.count)")
                    }
                    if let wantPal = jsonArray(wantItem["palette"]) {
                        for (j, item) in wantPal.enumerated() {
                            if let w = jsonDict(item), let hex = jsonString(w, "hex"), j < palette.count,
                               palette[j].colour.hex != hex {
                                failures.append("case \(caseId): bulk item \(wantIndex) palette hex expected \(hex), got \(palette[j].colour.hex)")
                            }
                        }
                    }
                }
            default:
                break
            }
        }
    }
}
