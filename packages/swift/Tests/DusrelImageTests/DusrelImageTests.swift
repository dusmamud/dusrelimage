import Foundation
import XCTest
@testable import DusrelImage

/// Focused wrapper tests: colours, error mapping, defaults, bulk
/// partial failure. The 82 shared vectors live in `VectorsTests`.
final class DusrelImageTests: XCTestCase {

    func testVersionIs010() {
        XCTAssertEqual(DusrelImage.version(), "0.1.0")
    }

    func testMakeTestImageIsDeterministic() throws {
        let a = try DusrelImage.makeTestImage(kind: "noise", width: 64, height: 48, seed: 42)
        let b = try DusrelImage.makeTestImage(kind: "noise", width: 64, height: 48, seed: 42)
        XCTAssertEqual(a, b)
        XCTAssertFalse(a.isEmpty)
    }

    func testMakeTestImageBadKindIsInvalidArgument() throws {
        _ = try DusrelImage.makeTestImage(kind: "gradient", width: 32, height: 32, seed: 1)
        do {
            _ = try DusrelImage.makeTestImage(kind: "bogus", width: 32, height: 32, seed: 1)
            XCTFail("expected INVALID_ARGUMENT for kind \"bogus\"")
        } catch let error as DusrelError {
            XCTAssertEqual(error.code, "INVALID_ARGUMENT")
            XCTAssertFalse(error.messageEn.isEmpty)
            XCTAssertFalse(error.messageHi.isEmpty)
        }
    }

    func testColourHelpers() throws {
        let white = DusrelImage.white()
        XCTAssertEqual(white.hex, "#FFFFFF")
        XCTAssertEqual(white.r, 255)
        XCTAssertEqual(white.g, 255)
        XCTAssertEqual(white.b, 255)
        XCTAssertEqual(white.a, 255)
        XCTAssertEqual(white.hsl, [0, 0, 1])

        let black = DusrelImage.black()
        XCTAssertEqual(black.hex, "#000000")
        XCTAssertEqual(black.hsl, [0, 0, 0])

        let rgb = DusrelImage.colourFromRgb(10, 20, 30)
        XCTAssertEqual(rgb.hex, "#0A141E")
        XCTAssertEqual(rgb.a, 255)

        XCTAssertEqual(try DusrelImage.colourFromHex("#FF0000").hex, "#FF0000")
        XCTAssertEqual(try DusrelImage.colourFromHex("#F00").hex, "#FF0000")
        XCTAssertEqual(try DusrelImage.colourFromHex("00FF00").hex, "#00FF00")
        let withAlpha = try DusrelImage.colourFromHex("#FF000080")
        XCTAssertEqual(withAlpha.hex, "#FF0000")
        XCTAssertEqual(withAlpha.a, 128)

        for bad in ["red", "#GGGGGG", "#12345", ""] {
            do {
                _ = try DusrelImage.colourFromHex(bad)
                XCTFail("expected INVALID_ARGUMENT for hex \"\(bad)\"")
            } catch let error as DusrelError {
                XCTAssertEqual(error.code, "INVALID_ARGUMENT", "hex \"\(bad)\"")
            }
        }
    }

    func testErrorMapping() throws {
        // Empty input.
        do {
            _ = try DusrelImage.compressImage(Data())
            XCTFail("expected EMPTY_INPUT")
        } catch let error as DusrelError {
            XCTAssertEqual(error.code, "EMPTY_INPUT")
            XCTAssertFalse(error.messageEn.isEmpty)
            XCTAssertFalse(error.messageHi.isEmpty)
        }

        // Bad quality.
        let solid = try DusrelImage.makeTestImage(kind: "solid", width: 32, height: 32, seed: 1)
        do {
            _ = try DusrelImage.compressImage(
                solid,
                options: CompressOptions(common: CommonOptions(), quality: 0)
            )
            XCTFail("expected INVALID_QUALITY")
        } catch let error as DusrelError {
            XCTAssertEqual(error.code, "INVALID_QUALITY")
        }

        // Garbage bytes are not an image.
        do {
            _ = try DusrelImage.getImageInfo(Data([0, 1, 2, 3]))
            XCTFail("expected INVALID_IMAGE")
        } catch let error as DusrelError {
            XCTAssertEqual(error.code, "INVALID_IMAGE")
        }

        // Pixel out of bounds.
        do {
            _ = try DusrelImage.pickColour(solid, x: 32, y: 0)
            XCTFail("expected PIXEL_OUT_OF_BOUNDS")
        } catch let error as DusrelError {
            XCTAssertEqual(error.code, "PIXEL_OUT_OF_BOUNDS")
        }

        // Empty annotation list.
        do {
            _ = try DusrelImage.annotateImage(solid, annotations: [])
            XCTFail("expected INVALID_ARGUMENT")
        } catch let error as DusrelError {
            XCTAssertEqual(error.code, "INVALID_ARGUMENT")
        }
    }

    func testDefaults() throws {
        let gradient = try DusrelImage.makeTestImage(kind: "gradient", width: 64, height: 48, seed: 4)
        // Compress with all defaults keeps format and dimensions.
        let compressed = try DusrelImage.compressImage(gradient)
        let info = try DusrelImage.getImageInfo(compressed)
        XCTAssertEqual(info.format, "png")
        XCTAssertEqual(info.width, 64)
        XCTAssertEqual(info.height, 48)

        // Resize percent 50 halves dimensions.
        let resized = try DusrelImage.resizeImage(
            gradient,
            options: ResizeOptions(common: CommonOptions(), percent: 50, background: DusrelImage.white())
        )
        let resizedInfo = try DusrelImage.getImageInfo(resized)
        XCTAssertEqual(resizedInfo.width, 32)
        XCTAssertEqual(resizedInfo.height, 24)

        // getImageInfo with all defaults.
        let direct = try DusrelImage.getImageInfo(gradient)
        XCTAssertEqual(direct.format, "png")
        XCTAssertEqual(direct.colourType, "rgb8")
        XCTAssertFalse(direct.hasAlpha)
        XCTAssertEqual(direct.fileSize, UInt64(gradient.count))
    }

    func testConvenienceOverloads() throws {
        let solid = try DusrelImage.makeTestImage(kind: "solid", width: 100, height: 80, seed: 9)
        let cropped = try DusrelImage.cropImage(solid, x: 10, y: 10, width: 20, height: 20)
        let cropInfo = try DusrelImage.getImageInfo(cropped)
        XCTAssertEqual(cropInfo.width, 20)
        XCTAssertEqual(cropInfo.height, 20)

        let converted = try DusrelImage.convertImage(solid, format: .png)
        XCTAssertEqual(try DusrelImage.getImageInfo(converted).format, "png")
    }

    func testBulkPreservesOrderAndReportsPartialFailures() throws {
        let good = try DusrelImage.makeTestImage(kind: "solid", width: 32, height: 32, seed: 1)
        let garbage = Data([0, 1, 2, 3])
        let results = DusrelImage.bulkProcess(
            [good, garbage, good],
            op: .pixelate(options: PixelateOptions(common: CommonOptions()))
        )
        XCTAssertEqual(results.count, 3)
        guard case let .image(index0, _) = results[0] else {
            return XCTFail("bulk item 0 should be .image, got \(results[0])")
        }
        XCTAssertEqual(index0, 0)
        guard case let .failed(index1, error1) = results[1] else {
            return XCTFail("bulk item 1 should be .failed, got \(results[1])")
        }
        XCTAssertEqual(index1, 1)
        XCTAssertEqual(error1.code, "INVALID_IMAGE")
        guard case let .image(index2, _) = results[2] else {
            return XCTFail("bulk item 2 should be .image, got \(results[2])")
        }
        XCTAssertEqual(index2, 2)
    }

    func testBulkEmptyInputList() {
        let results = DusrelImage.bulkProcess(
            [],
            op: .pixelate(options: PixelateOptions(common: CommonOptions()))
        )
        XCTAssertTrue(results.isEmpty)
    }

    func testBulkCollageIsPerItemInvalidArgument() throws {
        let good = try DusrelImage.makeTestImage(kind: "solid", width: 32, height: 32, seed: 1)
        let results = DusrelImage.bulkProcess(
            [good],
            op: .collage(options: CollageOptions(
                common: CommonOptions(),
                layout: .grid(rows: 2, cols: 2),
                background: DusrelImage.white()
            ))
        )
        XCTAssertEqual(results.count, 1)
        guard case let .failed(_, error) = results[0] else {
            return XCTFail("bulk Collage should fail per item, got \(results[0])")
        }
        XCTAssertEqual(error.code, "INVALID_ARGUMENT")
    }

    func testBulkPickColourAndPalette() throws {
        let solid = try DusrelImage.makeTestImage(kind: "solid", width: 20, height: 20, seed: 1)
        // Solid colour from seed 1 — just check the shape, not the seed value.
        let colourResults = DusrelImage.bulkProcess([solid], op: .pickColour(x: 3, y: 3))
        guard case let .colour(_, colour) = colourResults[0] else {
            return XCTFail("bulk PickColour should be .colour, got \(colourResults[0])")
        }
        XCTAssertFalse(colour.hex.isEmpty)

        let paletteResults = DusrelImage.bulkProcess([solid], op: .palette(count: 2))
        guard case let .palette(_, palette) = paletteResults[0] else {
            return XCTFail("bulk Palette should be .palette, got \(paletteResults[0])")
        }
        XCTAssertEqual(palette.count, 1)
        XCTAssertEqual(palette[0].colour.hex, colour.hex)
    }
}
