import Foundation
import XCTest
@testable import DusrelImage

/// Errors raised by the test support code itself (unknown fixture kind,
/// unknown op, schema gap, missing vectors file). They fail the case
/// loudly by name instead of silently skipping it — same rule as the
/// JS/Kotlin vector runners.
enum TestError: Error, CustomStringConvertible {
    case message(String)

    var description: String {
        switch self {
        case let .message(text): return text
        }
    }
}

// ---------------------------------------------------------------------------
// JSON helpers (Foundation JSONSerialization; numbers arrive as NSNumber)
// ---------------------------------------------------------------------------

func jsonInt(_ dict: [String: Any], _ key: String) -> Int? {
    (dict[key] as? NSNumber)?.intValue
}

func jsonUInt32(_ dict: [String: Any], _ key: String) -> UInt32? {
    guard let v = jsonInt(dict, key) else { return nil }
    return UInt32(v)
}

func jsonDouble(_ dict: [String: Any], _ key: String) -> Double? {
    (dict[key] as? NSNumber)?.doubleValue
}

func jsonString(_ dict: [String: Any], _ key: String) -> String? {
    dict[key] as? String
}

func jsonBool(_ dict: [String: Any], _ key: String) -> Bool? {
    (dict[key] as? NSNumber)?.boolValue
}

func jsonDict(_ value: Any?) -> [String: Any]? {
    value as? [String: Any]
}

func jsonArray(_ value: Any?) -> [Any]? {
    value as? [Any]
}

// ---------------------------------------------------------------------------
// Checksums + stored-deflate zlib (test fixtures only — no Compression
// framework, so this runs on macOS and Linux Swift alike)
// ---------------------------------------------------------------------------

private let crcTable: [UInt32] = {
    (0..<256).map { n in
        var c = UInt32(n)
        for _ in 0..<8 {
            c = (c & 1) != 0 ? 0xEDB8_8320 ^ (c >> 1) : c >> 1
        }
        return c
    }
}()

func crc32(_ bytes: [UInt8]) -> UInt32 {
    var c: UInt32 = 0xFFFF_FFFF
    for byte in bytes {
        c = crcTable[Int((c ^ UInt32(byte)) & 0xFF)] ^ (c >> 8)
    }
    return c ^ 0xFFFF_FFFF
}

func adler32(_ bytes: [UInt8]) -> UInt32 {
    var a: UInt32 = 1
    var b: UInt32 = 0
    for byte in bytes {
        a = (a + UInt32(byte)) % 65_521
        b = (b + a) % 65_521
    }
    return (b << 16) | a
}

func appendBE32(_ out: inout [UInt8], _ value: UInt32) {
    out.append(UInt8((value >> 24) & 0xFF))
    out.append(UInt8((value >> 16) & 0xFF))
    out.append(UInt8((value >> 8) & 0xFF))
    out.append(UInt8(value & 0xFF))
}

func appendLE16(_ out: inout [UInt8], _ value: UInt16) {
    out.append(UInt8(value & 0xFF))
    out.append(UInt8((value >> 8) & 0xFF))
}

func appendLE32(_ out: inout [UInt8], _ value: UInt32) {
    out.append(UInt8(value & 0xFF))
    out.append(UInt8((value >> 8) & 0xFF))
    out.append(UInt8((value >> 16) & 0xFF))
    out.append(UInt8((value >> 24) & 0xFF))
}

/// A zlib stream holding `raw` in stored (uncompressed) DEFLATE blocks.
/// Valid per RFC 1950/1951; decoders accept it, and it needs no zlib.
func zlibStored(_ raw: [UInt8]) -> [UInt8] {
    var out: [UInt8] = [0x78, 0x01]
    var offset = 0
    repeat {
        let chunk = min(65_535, raw.count - offset)
        let isFinal = offset + chunk == raw.count
        out.append(isFinal ? 0x01 : 0x00)
        let len = UInt16(chunk)
        appendLE16(&out, len)
        appendLE16(&out, ~len)
        out.append(contentsOf: raw[offset..<offset + chunk])
        offset += chunk
    } while offset < raw.count
    appendBE32(&out, adler32(raw))
    return out
}

func pngChunk(type: String, data: [UInt8]) -> [UInt8] {
    var out: [UInt8] = []
    appendBE32(&out, UInt32(data.count))
    let typeBytes = [UInt8](type.utf8)
    out.append(contentsOf: typeBytes)
    out.append(contentsOf: data)
    appendBE32(&out, crc32(typeBytes + data))
    return out
}

/// Minimal PNG encoder (filter 0 on every row). `pixel` returns RGBA;
/// `rgba == false` writes colour type 2 (RGB), else colour type 6.
func encodePNG(
    width: Int,
    height: Int,
    rgba: Bool = false,
    pixel: (Int, Int) -> (UInt8, UInt8, UInt8, UInt8)
) -> Data {
    let channels = rgba ? 4 : 3
    var raw: [UInt8] = []
    raw.reserveCapacity(height * (1 + width * channels))
    for y in 0..<height {
        raw.append(0) // filter: none
        for x in 0..<width {
            let px = pixel(x, y)
            raw.append(px.0)
            raw.append(px.1)
            raw.append(px.2)
            if rgba { raw.append(px.3) }
        }
    }
    var ihdr: [UInt8] = []
    appendBE32(&ihdr, UInt32(width))
    appendBE32(&ihdr, UInt32(height))
    ihdr.append(8) // bit depth
    ihdr.append(rgba ? 6 : 2) // colour type
    ihdr.append(0) // compression
    ihdr.append(0) // filter
    ihdr.append(0) // interlace
    var out: [UInt8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]
    out.append(contentsOf: pngChunk(type: "IHDR", data: ihdr))
    out.append(contentsOf: pngChunk(type: "IDAT", data: zlibStored(raw)))
    out.append(contentsOf: pngChunk(type: "IEND", data: []))
    return Data(out)
}

/// Minimal uncompressed little-endian TIFF, RGB 8-bit.
func encodeTIFF(width: Int, height: Int, rgb: (UInt8, UInt8, UInt8)) -> Data {
    func u16(_ v: UInt16) -> [UInt8] { [UInt8(v & 0xFF), UInt8((v >> 8) & 0xFF)] }
    func u32(_ v: UInt32) -> [UInt8] {
        [UInt8(v & 0xFF), UInt8((v >> 8) & 0xFF), UInt8((v >> 16) & 0xFF), UInt8((v >> 24) & 0xFF)]
    }
    func entry(tag: UInt16, type: UInt16, count: UInt32, value: UInt32) -> [UInt8] {
        u16(tag) + u16(type) + u32(count) + u32(value)
    }
    let bitsOffset = 8 + 2 + 10 * 12 + 4
    let dataOffset = bitsOffset + 6
    var pixels: [UInt8] = []
    pixels.reserveCapacity(width * height * 3)
    for _ in 0..<width * height {
        pixels.append(rgb.0)
        pixels.append(rgb.1)
        pixels.append(rgb.2)
    }
    var out: [UInt8] = []
    out.append(contentsOf: [UInt8]("II".utf8))
    out.append(contentsOf: u16(42))
    out.append(contentsOf: u32(8))
    out.append(contentsOf: u16(10))
    out.append(contentsOf: entry(tag: 256, type: 4, count: 1, value: UInt32(width)))
    out.append(contentsOf: entry(tag: 257, type: 4, count: 1, value: UInt32(height)))
    out.append(contentsOf: entry(tag: 258, type: 3, count: 3, value: UInt32(bitsOffset)))
    out.append(contentsOf: entry(tag: 259, type: 3, count: 1, value: 1))
    out.append(contentsOf: entry(tag: 262, type: 3, count: 1, value: 2))
    out.append(contentsOf: entry(tag: 273, type: 4, count: 1, value: UInt32(dataOffset)))
    out.append(contentsOf: entry(tag: 277, type: 3, count: 1, value: 3))
    out.append(contentsOf: entry(tag: 278, type: 4, count: 1, value: UInt32(height)))
    out.append(contentsOf: entry(tag: 279, type: 4, count: 1, value: UInt32(pixels.count)))
    out.append(contentsOf: entry(tag: 284, type: 3, count: 1, value: 1))
    out.append(contentsOf: [0, 0, 0, 0])
    out.append(contentsOf: u16(8))
    out.append(contentsOf: u16(8))
    out.append(contentsOf: u16(8))
    out.append(contentsOf: pixels)
    return Data(out)
}

// ---------------------------------------------------------------------------
// Fixtures — mirrors packages/js/src/vectors.test.ts and the Kotlin runner
// ---------------------------------------------------------------------------

func parseDimsFromDerived(_ text: String, caseId: String) throws -> (Int, Int) {
    for token in text.replacingOccurrences(of: ",", with: " ").split(separator: " ") {
        let parts = token.split(separator: "x")
        if parts.count == 2, let w = Int(parts[0]), let h = Int(parts[1]) {
            return (w, h)
        }
    }
    throw TestError.message("case \(caseId): no WxH dimensions in derived_from: \(text)")
}

func parseRgbFromDerived(_ text: String, caseId: String) throws -> (UInt8, UInt8, UInt8) {
    guard let open = text.firstIndex(of: "("), let close = text.firstIndex(of: ")"), open < close else {
        throw TestError.message("case \(caseId): no rgb tuple in derived_from: \(text)")
    }
    let inner = String(text[text.index(after: open)..<close])
    let parts = inner.split(separator: ",").map { $0.trimmingCharacters(in: .whitespaces) }
    guard parts.count == 3,
          let r = UInt8(parts[0]), let g = UInt8(parts[1]), let b = UInt8(parts[2]) else {
        throw TestError.message("case \(caseId): no rgb tuple in derived_from: \(text)")
    }
    return (r, g, b)
}

func fixtureBytes(_ fixture: [String: Any], caseId: String) throws -> Data {
    guard let kind = jsonString(fixture, "kind") else {
        throw TestError.message("case \(caseId): fixture has no kind")
    }
    switch kind {
    case "solid", "gradient", "noise":
        guard let width = jsonUInt32(fixture, "width"),
              let height = jsonUInt32(fixture, "height"),
              let seed = (fixture["seed"] as? NSNumber)?.uint64Value else {
            throw TestError.message("case \(caseId): \(kind) fixture needs width/height/seed")
        }
        return try DusrelImage.makeTestImage(kind: kind, width: width, height: height, seed: seed)
    case "solid_rgb":
        guard let rgb = jsonArray(fixture["rgb"]), rgb.count == 3,
              let r = (rgb[0] as? NSNumber)?.uint8Value,
              let g = (rgb[1] as? NSNumber)?.uint8Value,
              let b = (rgb[2] as? NSNumber)?.uint8Value,
              let width = jsonInt(fixture, "width"),
              let height = jsonInt(fixture, "height") else {
            throw TestError.message("case \(caseId): solid_rgb fixture needs rgb/width/height")
        }
        return encodePNG(width: width, height: height) { _, _ in (r, g, b, 255) }
    case "transparent_rgba":
        guard let width = jsonInt(fixture, "width"),
              let height = jsonInt(fixture, "height") else {
            throw TestError.message("case \(caseId): transparent_rgba fixture needs width/height")
        }
        return encodePNG(width: width, height: height, rgba: true) { _, _ in (0, 0, 0, 0) }
    case "two_colour":
        guard let width = jsonInt(fixture, "width"),
              let height = jsonInt(fixture, "height") else {
            throw TestError.message("case \(caseId): two_colour fixture needs width/height")
        }
        // Left half red, right half blue, 50/50 by construction.
        return encodePNG(width: width, height: height) { x, _ in
            x < width / 2 ? (255, 0, 0, 255) : (0, 0, 255, 255)
        }
    case "tiff":
        let derived = jsonString(fixture, "derived_from") ?? ""
        let rgb: (UInt8, UInt8, UInt8)
        if let arr = jsonArray(fixture["rgb"]), arr.count == 3,
           let r = (arr[0] as? NSNumber)?.uint8Value,
           let g = (arr[1] as? NSNumber)?.uint8Value,
           let b = (arr[2] as? NSNumber)?.uint8Value {
            rgb = (r, g, b)
        } else {
            rgb = try parseRgbFromDerived(derived, caseId: caseId)
        }
        let dims: (Int, Int)
        if let width = jsonInt(fixture, "width"), let height = jsonInt(fixture, "height") {
            dims = (width, height)
        } else {
            dims = try parseDimsFromDerived(derived, caseId: caseId)
        }
        return encodeTIFF(width: dims.0, height: dims.1, rgb: rgb)
    case "jpeg":
        // Derived: compress gradient 100x80 seed 2 to JPEG q80.
        guard (jsonString(fixture, "derived_from") ?? "").contains("jpeg") else {
            throw TestError.message("case \(caseId): jpeg fixture derived_from does not mention jpeg")
        }
        let base = try DusrelImage.makeTestImage(kind: "gradient", width: 100, height: 80, seed: 2)
        return try DusrelImage.compressImage(
            base,
            options: CompressOptions(common: CommonOptions(), format: .jpeg)
        )
    case "avif":
        // Derived: convert gradient 64x48 seed 4 to AVIF q80.
        guard (jsonString(fixture, "derived_from") ?? "").contains("avif") else {
            throw TestError.message("case \(caseId): avif fixture derived_from does not mention avif")
        }
        let base = try DusrelImage.makeTestImage(kind: "gradient", width: 64, height: 48, seed: 4)
        return try DusrelImage.convertImage(base, format: .avif, quality: 80)
    case "garbage":
        let derived = jsonString(fixture, "derived_from") ?? ""
        if derived.hasPrefix("bytes [") && derived.hasSuffix("]") {
            let inner = String(derived.dropFirst("bytes [".count).dropLast())
            let parts = inner.split(separator: ",")
            var bytes: [UInt8] = []
            for part in parts {
                guard let v = UInt8(part.trimmingCharacters(in: .whitespaces)) else {
                    throw TestError.message("case \(caseId): unhandled garbage derived_from: \(derived)")
                }
                bytes.append(v)
            }
            return Data(bytes)
        }
        if derived.hasPrefix("ASCII bytes: '") && derived.hasSuffix("'") {
            let inner = String(derived.dropFirst("ASCII bytes: '".count).dropLast())
            return Data(inner.utf8)
        }
        throw TestError.message("case \(caseId): unhandled garbage derived_from: \(derived)")
    case "empty":
        return Data()
    default:
        throw TestError.message("case \(caseId): unknown fixture kind \(kind) — update this runner")
    }
}

/// One pixel as plain ints. (Swift tuples have no `==`, so pixel
/// equality goes through `rgbaEqual` below, not tuple comparison.)
typealias RGBA = (r: Int, g: Int, b: Int, a: Int)

func rgbaEqual(_ a: RGBA, _ b: RGBA) -> Bool {
    a.r == b.r && a.g == b.g && a.b == b.b && a.a == b.a
}

// ---------------------------------------------------------------------------
// Pixel access — via the wrapper's own pickColour (the core decodes),
// so no PNG/JPEG decoder is needed in the tests. Region scans stop at
// the first decisive pixel.
// ---------------------------------------------------------------------------

struct Pixels {
    let data: Data
    let caseId: String

    func pixel(x: Int, y: Int) throws -> RGBA {
        let c = try DusrelImage.pickColour(data, x: UInt32(x), y: UInt32(y))
        return (r: Int(c.r), g: Int(c.g), b: Int(c.b), a: Int(c.a))
    }

    func regionHasDark(x0: Int, y0: Int, x1: Int, y1: Int) throws -> Bool {
        for y in y0..<y1 {
            for x in x0..<x1 {
                let px = try pixel(x: x, y: y)
                if px.r < 128 || px.g < 128 || px.b < 128 { return true }
            }
        }
        return false
    }

    func regionAllWhite(x0: Int, y0: Int, x1: Int, y1: Int) throws -> Bool {
        for y in y0..<y1 {
            for x in x0..<x1 {
                let px = try pixel(x: x, y: y)
                if px.r != 255 || px.g != 255 || px.b != 255 { return false }
            }
        }
        return true
    }
}

// ---------------------------------------------------------------------------
// Locating shared/test-vectors.json (no copy is committed into the
// package — the runner reads the shared file, like the JS/Kotlin
// runners, so the vectors cannot drift between languages)
// ---------------------------------------------------------------------------

func vectorsURL() throws -> URL {
    if let env = ProcessInfo.processInfo.environment["DUSRELIMAGE_VECTORS"], !env.isEmpty {
        return URL(fileURLWithPath: env)
    }
    // This file lives at <repo>/packages/swift/Tests/DusrelImageTests/.
    let thisFile = URL(fileURLWithPath: #filePath)
    var dir = thisFile.deletingLastPathComponent()
    for _ in 0..<4 { dir = dir.deletingLastPathComponent() }
    let candidate = dir.appendingPathComponent("shared/test-vectors.json")
    if FileManager.default.fileExists(atPath: candidate.path) { return candidate }
    let cwd = FileManager.default.currentDirectoryPath
    for rel in ["shared/test-vectors.json", "../../shared/test-vectors.json", "../../../shared/test-vectors.json"] {
        let url = URL(fileURLWithPath: cwd).appendingPathComponent(rel)
        if FileManager.default.fileExists(atPath: url.path) { return url }
    }
    throw TestError.message(
        "shared/test-vectors.json not found (tried \(candidate.path) and CWD-relative paths); "
            + "run `swift test` from the repo root or packages/swift, or set DUSRELIMAGE_VECTORS."
    )
}

func loadCases() throws -> [[String: Any]] {
    let data = try Data(contentsOf: vectorsURL())
    let obj = try JSONSerialization.jsonObject(with: data)
    guard let arr = obj as? [[String: Any]] else {
        throw TestError.message("shared/test-vectors.json is not a JSON array of cases")
    }
    return arr
}
