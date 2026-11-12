package com.dusrel.image

import java.io.ByteArrayOutputStream
import java.io.DataOutputStream
import java.io.File
import java.util.zip.CRC32
import java.util.zip.Deflater
import java.util.zip.Inflater
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.junit.runners.Parameterized

/**
 * Shared test vectors (SPEC section 6): Rust, Python, JS and Kotlin all
 * run `shared/test-vectors.json` and must agree.
 *
 * This runner mirrors `packages/js/src/vectors.test.ts` case-for-case:
 * it loads the JSON, builds each fixture — `dusrelimage mock` kinds
 * (`solid`/`gradient`/`noise` + seed) via [DusrelImage.makeTestImage]
 * (the same Rust builders as the CLI `mock` command), plus the
 * documented derived fixtures (`solid_rgb`, `transparent_rgba`,
 * `two_colour`, `tiff`, `jpeg`, `avif`, `garbage`, `empty`) built here
 * with a minimal PNG/TIFF encoder (no binary blobs committed) — runs
 * the op through the public [DusrelImage] wrapper, and asserts the
 * expected dimensions / pixel spot-checks / error code.
 *
 * It FAILS LOUDLY on any case whose fixture/op kind it cannot handle:
 * unknown fixture kinds, ops, bulk ops or expected keys throw naming
 * the case id instead of silently skipping the case.
 *
 * Pixel checks decode PNG output with the minimal decoder below
 * (`java.awt` / `javax.imageio` do not exist on the Android
 * unit-test classpath); anything else falls back to
 * [DusrelImage.pickColour] per pixel (only the JPEG-flatten case
 * needs that). AVIF output cannot be decoded back in this build
 * (no AVIF decoder): a case that only asserts the header/size may
 * skip the decode; anything asserting dims must be decodable.
 *
 * These are JVM unit tests: they load the HOST cdylib built by
 * `cargo build -p dusrelimage-uniffi --release`, via the
 * `uniffi.component.dusrelimage.libraryOverride` system property set
 * in `lib/build.gradle.kts` (or `DUSRELIMAGE_UNIFFI_LIB`).
 */
@RunWith(Parameterized::class)
class VectorsTest(private val caseId: String, private val c: JSONObject) {

    companion object {
        /** Expected keys this runner knows how to assert. Anything else is a schema gap. */
        val KNOWN_EXPECTED_KEYS = setOf(
            "format", "width", "height", "has_alpha", "colour_type", "error",
            "max_bytes", "avif_header", "pixels", "region_all_white",
            "region_has_dark", "regions_have_dark", "colour", "different_from",
            "palette_hexes", "palette_length", "share_each", "share_sum",
            "share_sum_tolerance", "share_tolerance", "palette",
            "differs_from_input", "pixel_equalities", "unchanged_pixels",
            "results",
        )

        @JvmStatic
        @Parameterized.Parameters(name = "{0}")
        fun cases(): List<Array<Any>> {
            val cases = loadCases()
            return cases.map { arrayOf<Any>(it.getString("id"), it) }
        }

        fun loadCases(): List<JSONObject> {
            val path = vectorsPath()
            val arr = JSONArray(File(path).readText(Charsets.UTF_8))
            return (0 until arr.length()).map { arr.getJSONObject(it) }
        }

        /** Locate shared/test-vectors.json. Fails loudly. */
        fun vectorsPath(): String {
            val candidates = listOf(
                System.getenv("DUSRELIMAGE_VECTORS"),
                "src/test/resources/shared/test-vectors.json",
                "../../../shared/test-vectors.json",
                "../../shared/test-vectors.json",
                "shared/test-vectors.json",
            ).filterNotNull()
            for (p in candidates) {
                if (File(p).exists()) return File(p).canonicalPath
            }
            throw IllegalStateException(
                "shared/test-vectors.json not found (tried: ${candidates.joinToString(", ")})",
            )
        }
    }

    @Test
    fun vectorCase() {
        val failures = mutableListOf<String>()
        var outcome: Outcome? = null
        var error: DusrelError? = null
        var input: ByteArray? = null
        try {
            val run = runCase(c, caseId)
            outcome = run.outcome
            input = run.input
        } catch (e: DusrelError) {
            error = e
            if (c.has("fixture")) input = fixtureBytes(c.getJSONObject("fixture"), caseId)
        }
        assertExpected(c, caseId, outcome, error, input, failures)
        assertTrue(
            "case $caseId failed:\n" + failures.joinToString("\n"),
            failures.isEmpty(),
        )
    }
}

/** The shared-vectors file itself: 82 cases, same as the JS runner asserts. */
class VectorsSanityTest {
    @Test
    fun has82Cases() {
        assertEquals(82, VectorsTest.loadCases().size)
    }
}

// ---------------------------------------------------------------------------
// Minimal PNG encoder + TIFF writer (test fixtures only)
// ---------------------------------------------------------------------------

private fun pngChunk(type: String, data: ByteArray): ByteArray {
    val out = ByteArrayOutputStream()
    val dos = DataOutputStream(out)
    dos.writeInt(data.size)
    val typeBytes = type.toByteArray(Charsets.US_ASCII)
    dos.write(typeBytes)
    dos.write(data)
    val crc = CRC32()
    crc.update(typeBytes)
    crc.update(data)
    dos.writeInt(crc.value.toInt())
    return out.toByteArray()
}

private fun encodePng(
    width: Int,
    height: Int,
    pixel: (x: Int, y: Int) -> IntArray,
    rgba: Boolean = false,
): ByteArray {
    val channels = if (rgba) 4 else 3
    val raw = ByteArray(height * (1 + width * channels))
    for (y in 0 until height) {
        raw[y * (1 + width * channels)] = 0 // filter: none
        for (x in 0 until width) {
            val px = pixel(x, y)
            val off = y * (1 + width * channels) + 1 + x * channels
            for (ch in 0 until channels) raw[off + ch] = px[ch].toByte()
        }
    }
    val deflater = Deflater()
    deflater.setInput(raw)
    deflater.finish()
    val compressed = ByteArray(raw.size + 64)
    val compressedLen = deflater.deflate(compressed)
    deflater.end()
    val ihdr = ByteArrayOutputStream().also { out ->
        DataOutputStream(out).apply {
            writeInt(width)
            writeInt(height)
            write(8) // bit depth
            write(if (rgba) 6 else 2) // colour type
            write(0) // compression
            write(0) // filter
            write(0) // interlace
        }
    }.toByteArray()
    val out = ByteArrayOutputStream()
    out.write(byteArrayOf(0x89.toByte(), 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A))
    out.write(pngChunk("IHDR", ihdr))
    out.write(pngChunk("IDAT", compressed.copyOf(compressedLen)))
    out.write(pngChunk("IEND", ByteArray(0)))
    return out.toByteArray()
}

/** Minimal uncompressed little-endian TIFF, RGB 8-bit. */
private fun encodeTiff(width: Int, height: Int, rgb: IntArray): ByteArray {
    fun u16(v: Int): ByteArray = byteArrayOf((v and 0xFF).toByte(), ((v shr 8) and 0xFF).toByte())
    fun u32(v: Int): ByteArray = byteArrayOf(
        (v and 0xFF).toByte(), ((v shr 8) and 0xFF).toByte(),
        ((v shr 16) and 0xFF).toByte(), ((v shr 24) and 0xFF).toByte(),
    )
    fun entry(tag: Int, type: Int, count: Int, value: Int): ByteArray =
        u16(tag) + u16(type) + u32(count) + u32(value)

    val bitsOffset = 8 + 2 + 10 * 12 + 4
    val dataOffset = bitsOffset + 6
    val pixels = ByteArray(width * height * 3)
    for (i in 0 until width * height) {
        pixels[i * 3] = rgb[0].toByte()
        pixels[i * 3 + 1] = rgb[1].toByte()
        pixels[i * 3 + 2] = rgb[2].toByte()
    }
    val out = ByteArrayOutputStream()
    out.write("II".toByteArray(Charsets.US_ASCII))
    out.write(u16(42))
    out.write(u32(8))
    out.write(u16(10))
    out.write(entry(256, 4, 1, width))
    out.write(entry(257, 4, 1, height))
    out.write(entry(258, 3, 3, bitsOffset))
    out.write(entry(259, 3, 1, 1))
    out.write(entry(262, 3, 1, 2))
    out.write(entry(273, 4, 1, dataOffset))
    out.write(entry(277, 3, 1, 3))
    out.write(entry(278, 4, 1, height))
    out.write(entry(279, 4, 1, pixels.size))
    out.write(entry(284, 3, 1, 1))
    out.write(ByteArray(4))
    out.write(u16(8))
    out.write(u16(8))
    out.write(u16(8))
    out.write(pixels)
    return out.toByteArray()
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

private fun parseDimsFromDerived(text: String, caseId: String): Pair<Int, Int> {
    for (token in text.replace(",", " ").split(Regex("\\s+"))) {
        val m = Regex("^(\\d+)x(\\d+)$").matchEntire(token)
        if (m != null) return Pair(m.groupValues[1].toInt(), m.groupValues[2].toInt())
    }
    throw IllegalStateException("case $caseId: no WxH dimensions in derived_from: $text")
}

private fun parseRgbFromDerived(text: String, caseId: String): IntArray {
    val m = Regex("\\((\\d+),\\s*(\\d+),\\s*(\\d+)\\)").find(text)
        ?: throw IllegalStateException("case $caseId: no rgb tuple in derived_from: $text")
    return intArrayOf(m.groupValues[1].toInt(), m.groupValues[2].toInt(), m.groupValues[3].toInt())
}

private fun fixtureBytes(fixture: JSONObject, caseId: String): ByteArray {
    when (val kind = fixture.getString("kind")) {
        "solid", "gradient", "noise" -> return DusrelImage.makeTestImage(
            kind,
            fixture.getInt("width").toUInt(),
            fixture.getInt("height").toUInt(),
            fixture.getLong("seed").toULong(),
        )
        "solid_rgb" -> {
            val rgb = fixture.getJSONArray("rgb")
            return encodePng(fixture.getInt("width"), fixture.getInt("height"), { _, _ ->
                intArrayOf(rgb.getInt(0), rgb.getInt(1), rgb.getInt(2), 255)
            })
        }
        "transparent_rgba" -> return encodePng(
            fixture.getInt("width"),
            fixture.getInt("height"),
            { _, _ -> intArrayOf(0, 0, 0, 0) },
            rgba = true,
        )
        "two_colour" -> {
            val width = fixture.getInt("width")
            return encodePng(fixture.getInt("width"), fixture.getInt("height"), { x, _ ->
                if (x < width / 2) intArrayOf(255, 0, 0, 255) else intArrayOf(0, 0, 255, 255)
            })
        }
        "tiff" -> {
            val derived = fixture.optString("derived_from", "")
            val rgb = if (fixture.has("rgb")) {
                val a = fixture.getJSONArray("rgb")
                intArrayOf(a.getInt(0), a.getInt(1), a.getInt(2))
            } else {
                parseRgbFromDerived(derived, caseId)
            }
            val (width, height) = if (fixture.has("width")) {
                Pair(fixture.getInt("width"), fixture.getInt("height"))
            } else {
                parseDimsFromDerived(derived, caseId)
            }
            return encodeTiff(width, height, rgb)
        }
        "jpeg" -> {
            // Derived: compress gradient 100x80 seed 2 to JPEG q80.
            if (!fixture.optString("derived_from", "").contains("jpeg")) {
                throw IllegalStateException("case $caseId: jpeg fixture derived_from does not mention jpeg")
            }
            return DusrelImage.compressImage(
                DusrelImage.makeTestImage("gradient", 100u, 80u, 2u),
                CompressOptions(CommonOptions(), format = ImageFormat.JPEG, quality = 80u),
            )
        }
        "avif" -> {
            // Derived: convert gradient 64x48 seed 4 to AVIF q80.
            if (!fixture.optString("derived_from", "").contains("avif")) {
                throw IllegalStateException("case $caseId: avif fixture derived_from does not mention avif")
            }
            return DusrelImage.convertImage(
                DusrelImage.makeTestImage("gradient", 64u, 48u, 4u),
                ImageFormat.AVIF,
                quality = 80u,
            )
        }
        "garbage" -> {
            val derived = fixture.optString("derived_from", "")
            val bytesMatch = Regex("^bytes \\[(.*)\\]$").matchEntire(derived)
            if (bytesMatch != null) {
                return bytesMatch.groupValues[1].split(",")
                    .map { it.trim().toInt().toByte() }.toByteArray()
            }
            val asciiMatch = Regex("^ASCII bytes: '(.*)'$").matchEntire(derived)
            if (asciiMatch != null) return asciiMatch.groupValues[1].toByteArray(Charsets.UTF_8)
            throw IllegalStateException("case $caseId: unhandled garbage derived_from: $derived")
        }
        "empty" -> return ByteArray(0)
        else -> throw IllegalStateException("case $caseId: unknown fixture kind $kind — update this runner")
    }
}

// ---------------------------------------------------------------------------
// Options translation (JSON snake_case -> Kotlin wrapper options)
// ---------------------------------------------------------------------------

private fun imageFormatOf(name: String, caseId: String): ImageFormat = when (name) {
    "Jpeg" -> ImageFormat.JPEG
    "Png" -> ImageFormat.PNG
    "WebP" -> ImageFormat.WEB_P
    "Avif" -> ImageFormat.AVIF
    "Gif" -> ImageFormat.GIF
    "Bmp" -> ImageFormat.BMP
    else -> throw IllegalStateException("case $caseId: unknown ImageFormat $name")
}

private fun fitModeOf(name: String, caseId: String): FitMode = when (name) {
    "Fit" -> FitMode.FIT
    "Fill" -> FitMode.FILL
    "Stretch" -> FitMode.STRETCH
    "Crop" -> FitMode.CROP
    else -> throw IllegalStateException("case $caseId: unknown FitMode $name")
}

private fun filterOf(name: String, caseId: String): Filter = when (name) {
    "Nearest" -> Filter.NEAREST
    "Triangle" -> Filter.TRIANGLE
    "CatmullRom" -> Filter.CATMULL_ROM
    "Gaussian" -> Filter.GAUSSIAN
    "Lanczos3" -> Filter.LANCZOS3
    else -> throw IllegalStateException("case $caseId: unknown Filter $name")
}

private fun positionOf(name: String, caseId: String): Position = when (name) {
    "TopLeft" -> Position.TOP_LEFT
    "TopCenter" -> Position.TOP_CENTER
    "TopRight" -> Position.TOP_RIGHT
    "CenterLeft" -> Position.CENTER_LEFT
    "Center" -> Position.CENTER
    "CenterRight" -> Position.CENTER_RIGHT
    "BottomLeft" -> Position.BOTTOM_LEFT
    "BottomCenter" -> Position.BOTTOM_CENTER
    "BottomRight" -> Position.BOTTOM_RIGHT
    "Tiled" -> Position.TILED
    else -> throw IllegalStateException("case $caseId: unknown Position $name")
}

private fun toCommon(options: JSONObject, caseId: String): CommonOptions {
    val common = if (options.has("common")) options.getJSONObject("common") else JSONObject()
    checkKnownOptions(common, setOf("auto_orient", "keep_exif", "dpi", "max_pixels", "max_dimension"), caseId, "common")
    return CommonOptions(
        autoOrient = common.optBoolean("auto_orient", true),
        keepExif = common.optBoolean("keep_exif", false),
        dpi = if (common.has("dpi") && !common.isNull("dpi")) {
            val dpi = common.get("dpi")
            when (dpi) {
                is JSONObject -> Dpi(dpi.getInt("width").toUInt(), dpi.getInt("height").toUInt())
                is JSONArray -> Dpi(dpi.getInt(0).toUInt(), dpi.getInt(1).toUInt())
                else -> throw IllegalStateException("unknown dpi shape: $dpi")
            }
        } else {
            null
        },
        maxPixels = if (common.has("max_pixels")) common.getLong("max_pixels").toULong() else 120000000uL,
        maxDimension = if (common.has("max_dimension")) common.getInt("max_dimension").toUInt() else 16000u,
    )
}

/** Fail loudly on an option key this runner does not know — a schema gap, not a skip. */
private fun checkKnownOptions(options: JSONObject, known: Set<String>, caseId: String, what: String) {
    for (key in options.keys()) {
        if (key !in known) {
            throw IllegalStateException("case $caseId: unknown $what option $key — update this runner")
        }
    }
}

private fun toCompressOptions(options: JSONObject, caseId: String): CompressOptions {
    checkKnownOptions(options, setOf("common", "quality", "format", "progressive", "lossless", "effort"), caseId, "compress")
    return CompressOptions(
        common = toCommon(options, caseId),
        quality = if (options.has("quality")) options.getInt("quality").toUByte() else 80u,
        format = if (options.has("format") && !options.isNull("format")) imageFormatOf(options.getString("format"), caseId) else null,
        progressive = options.optBoolean("progressive", false),
        lossless = options.optBoolean("lossless", false),
        effort = if (options.has("effort")) options.getInt("effort").toUByte() else 4u,
    )
}

private fun toResizeOptions(options: JSONObject, caseId: String): ResizeOptions {
    checkKnownOptions(options, setOf("common", "width", "height", "percent", "fit", "filter", "background"), caseId, "resize")
    return ResizeOptions(
        common = toCommon(options, caseId),
        width = if (options.has("width") && !options.isNull("width")) options.getInt("width").toUInt() else null,
        height = if (options.has("height") && !options.isNull("height")) options.getInt("height").toUInt() else null,
        percent = if (options.has("percent") && !options.isNull("percent")) options.getDouble("percent").toFloat() else null,
        fit = if (options.has("fit")) fitModeOf(options.getString("fit"), caseId) else FitMode.FIT,
        filter = if (options.has("filter")) filterOf(options.getString("filter"), caseId) else Filter.LANCZOS3,
        background = if (options.has("background")) DusrelImage.colourFromHex(options.getString("background")) else DusrelImage.white(),
    )
}

private fun toCropOptions(options: JSONObject, caseId: String): CropOptions {
    checkKnownOptions(options, setOf("common", "x", "y", "width", "height", "anchor"), caseId, "crop")
    return CropOptions(
        common = toCommon(options, caseId),
        x = if (options.has("x")) options.getInt("x").toUInt() else 0u,
        y = if (options.has("y")) options.getInt("y").toUInt() else 0u,
        width = options.getInt("width").toUInt(),
        height = options.getInt("height").toUInt(),
        anchor = if (options.has("anchor") && !options.isNull("anchor")) positionOf(options.getString("anchor"), caseId) else null,
    )
}

private fun toConvertOptions(options: JSONObject, caseId: String): ConvertOptions {
    checkKnownOptions(options, setOf("common", "format", "quality", "background", "progressive"), caseId, "convert")
    return ConvertOptions(
        common = toCommon(options, caseId),
        format = imageFormatOf(options.getString("format"), caseId),
        quality = if (options.has("quality")) options.getInt("quality").toUByte() else 80u,
        background = if (options.has("background")) DusrelImage.colourFromHex(options.getString("background")) else DusrelImage.white(),
        progressive = options.optBoolean("progressive", false),
    )
}

private fun toWatermarkOptions(options: JSONObject, caseId: String): WatermarkOptions {
    checkKnownOptions(options, setOf("common", "text", "logo", "opacity", "position", "rotation_deg", "margin"), caseId, "watermark")
    val text = if (options.has("text") && !options.isNull("text")) {
        val t = options.getJSONObject("text")
        TextWatermark(
            text = t.getString("text"),
            size = if (t.has("size")) t.getDouble("size").toFloat() else 48f,
            colour = DusrelImage.colourFromHex(t.getString("colour")),
        )
    } else {
        null
    }
    val logo = if (options.has("logo") && !options.isNull("logo")) {
        val l = options.getJSONObject("logo")
        ImageWatermark(
            image = fixtureBytes(l.getJSONObject("image"), caseId).map { it.toUByte() },
            scale = if (l.has("scale")) l.getDouble("scale").toFloat() else 1f,
        )
    } else {
        null
    }
    return WatermarkOptions(
        common = toCommon(options, caseId),
        text = text,
        logo = logo,
        opacity = if (options.has("opacity")) options.getDouble("opacity").toFloat() else 0.5f,
        position = if (options.has("position")) positionOf(options.getString("position"), caseId) else Position.BOTTOM_RIGHT,
        rotationDeg = if (options.has("rotation_deg")) options.getDouble("rotation_deg").toFloat() else 0f,
        margin = if (options.has("margin")) options.getInt("margin").toUInt() else 16u,
    )
}

private fun toCollageOptions(options: JSONObject, caseId: String): CollageOptions {
    checkKnownOptions(options, setOf("common", "layout", "gap", "background", "width", "height", "cell_fit"), caseId, "collage")
    val layout = if (options.has("layout")) {
        val l = options.get("layout")
        when (l) {
            is String -> when (l) {
                "Row" -> CollageLayout.Row
                "Column" -> CollageLayout.Column
                else -> throw IllegalStateException("case $caseId: unknown collage layout $l")
            }
            is JSONObject -> {
                if (!l.has("Grid")) throw IllegalStateException("case $caseId: unknown collage layout $l")
                val g = l.getJSONObject("Grid")
                CollageLayout.Grid(rows = g.getInt("rows").toUInt(), cols = g.getInt("cols").toUInt())
            }
            else -> throw IllegalStateException("case $caseId: unknown collage layout $l")
        }
    } else {
        CollageLayout.Grid(rows = 2u, cols = 2u)
    }
    return CollageOptions(
        common = toCommon(options, caseId),
        layout = layout,
        gap = if (options.has("gap")) options.getInt("gap").toUInt() else 8u,
        background = if (options.has("background")) DusrelImage.colourFromHex(options.getString("background")) else DusrelImage.white(),
        width = if (options.has("width")) options.getInt("width").toUInt() else 1200u,
        height = if (options.has("height")) options.getInt("height").toUInt() else 1200u,
        cellFit = if (options.has("cell_fit")) fitModeOf(options.getString("cell_fit"), caseId) else FitMode.FIT,
    )
}

private fun toPixelateOptions(options: JSONObject, caseId: String): PixelateOptions {
    checkKnownOptions(options, setOf("common", "x", "y", "width", "height", "block_size"), caseId, "pixelate")
    return PixelateOptions(
        common = toCommon(options, caseId),
        x = if (options.has("x")) options.getInt("x").toUInt() else 0u,
        y = if (options.has("y")) options.getInt("y").toUInt() else 0u,
        width = if (options.has("width")) options.getInt("width").toUInt() else 0u,
        height = if (options.has("height")) options.getInt("height").toUInt() else 0u,
        blockSize = if (options.has("block_size")) options.getInt("block_size").toUInt() else 10u,
    )
}

private fun toAnnotations(annotations: JSONArray, caseId: String): List<Annotation> {
    return (0 until annotations.length()).map { i ->
        val a = annotations.getJSONObject(i)
        when (val type = a.getString("type")) {
            "Arrow", "Line" -> {
                val from = a.getJSONArray("from")
                val to = a.getJSONArray("to")
                val colour = DusrelImage.colourFromHex(a.getString("colour"))
                val width = a.getInt("width").toUInt()
                if (type == "Arrow") {
                    Annotation.Arrow(Point(from.getInt(0).toUInt(), from.getInt(1).toUInt()), Point(to.getInt(0).toUInt(), to.getInt(1).toUInt()), colour, width)
                } else {
                    Annotation.Line(Point(from.getInt(0).toUInt(), from.getInt(1).toUInt()), Point(to.getInt(0).toUInt(), to.getInt(1).toUInt()), colour, width)
                }
            }
            "Rect", "Ellipse" -> {
                val colour = DusrelImage.colourFromHex(a.getString("colour"))
                val fill = if (a.has("fill") && !a.isNull("fill")) DusrelImage.colourFromHex(a.getString("fill")) else null
                val x = a.getInt("x").toUInt()
                val y = a.getInt("y").toUInt()
                val w = a.getInt("w").toUInt()
                val h = a.getInt("h").toUInt()
                val width = a.getInt("width").toUInt()
                if (type == "Rect") Annotation.Rect(x, y, w, h, colour, width, fill)
                else Annotation.Ellipse(x, y, w, h, colour, width, fill)
            }
            "Text" -> Annotation.Text(
                a.getInt("x").toUInt(),
                a.getInt("y").toUInt(),
                a.getString("text"),
                a.getDouble("size").toFloat(),
                DusrelImage.colourFromHex(a.getString("colour")),
            )
            else -> throw IllegalStateException("case $caseId: unknown annotation type $type — update this runner")
        }
    }
}

// ---------------------------------------------------------------------------
// Running one case
// ---------------------------------------------------------------------------

private sealed interface Outcome {
    data class Image(val bytes: ByteArray) : Outcome
    data class Info(val info: ImageInfo) : Outcome
    data class ColourOut(val colour: Colour) : Outcome
    data class PaletteOut(val palette: List<PaletteColour>) : Outcome
    data class BulkOut(val results: List<BulkResult>) : Outcome
}

private data class RunResult(val outcome: Outcome, val input: ByteArray?)

private fun runCase(c: JSONObject, caseId: String): RunResult {
    val options: JSONObject = if (c.has("options")) c.getJSONObject("options") else JSONObject()
    val inputs: List<ByteArray>
    val input: ByteArray?
    when {
        c.has("fixtures") -> {
            val arr = c.getJSONArray("fixtures")
            inputs = (0 until arr.length()).map { fixtureBytes(arr.getJSONObject(it), caseId) }
            input = inputs.firstOrNull()
        }
        c.has("fixture") -> {
            input = fixtureBytes(c.getJSONObject("fixture"), caseId)
            inputs = listOf(input)
        }
        else -> {
            inputs = emptyList()
            input = null
        }
    }
    fun needImage(): ByteArray = input ?: throw IllegalStateException("case $caseId: op needs one fixture")
    return when (val op = c.getString("op")) {
        "get_image_info" -> RunResult(Outcome.Info(DusrelImage.getImageInfo(needImage(), toCommon(options, caseId))), input)
        "compress_image" -> RunResult(Outcome.Image(DusrelImage.compressImage(needImage(), toCompressOptions(options, caseId))), input)
        "compress_to_size" -> RunResult(
            Outcome.Image(DusrelImage.compressToSize(needImage(), c.getLong("target_bytes").toULong(), toCompressOptions(options, caseId))),
            input,
        )
        "resize_image" -> RunResult(Outcome.Image(DusrelImage.resizeImage(needImage(), toResizeOptions(options, caseId))), input)
        "crop_image" -> RunResult(Outcome.Image(DusrelImage.cropImage(needImage(), toCropOptions(options, caseId))), input)
        "convert_image" -> RunResult(Outcome.Image(DusrelImage.convertImage(needImage(), toConvertOptions(options, caseId))), input)
        "add_watermark" -> RunResult(Outcome.Image(DusrelImage.addWatermark(needImage(), toWatermarkOptions(options, caseId))), input)
        "annotate_image" -> RunResult(
            Outcome.Image(DusrelImage.annotateImage(needImage(), toAnnotations(c.getJSONArray("annotations"), caseId))),
            input,
        )
        "pick_colour" -> RunResult(Outcome.ColourOut(DusrelImage.pickColour(needImage(), options.getInt("x").toUInt(), options.getInt("y").toUInt())), input)
        "extract_palette" -> RunResult(Outcome.PaletteOut(DusrelImage.extractPalette(needImage(), options.getInt("count").toUByte())), input)
        "make_collage" -> RunResult(Outcome.Image(DusrelImage.makeCollage(inputs, toCollageOptions(options, caseId))), null)
        "pixelate_image" -> RunResult(Outcome.Image(DusrelImage.pixelateImage(needImage(), toPixelateOptions(options, caseId))), input)
        "bulk_process" -> {
            val bulkOp = c.getString("bulk_op")
            val op = when (bulkOp) {
                "Pixelate" -> BulkOp.Pixelate(toPixelateOptions(options, caseId))
                "PickColour" -> BulkOp.PickColour(options.getInt("x").toUInt(), options.getInt("y").toUInt())
                "Palette" -> BulkOp.Palette(options.getInt("count").toUByte())
                "Collage" -> BulkOp.Collage(toCollageOptions(options, caseId))
                "Watermark" -> BulkOp.Watermark(toWatermarkOptions(options, caseId))
                "Annotate" -> BulkOp.Annotate(toAnnotations(c.getJSONArray("annotations"), caseId))
                else -> throw IllegalStateException("case $caseId: unknown bulk_op $bulkOp — update this runner")
            }
            RunResult(Outcome.BulkOut(DusrelImage.bulkProcess(inputs, op)), null)
        }
        else -> throw IllegalStateException("case $caseId: unknown op $op — update this runner")
    }
}

// ---------------------------------------------------------------------------
// Assertions on the expected object
// ---------------------------------------------------------------------------

/** Decoded PNG pixels, stored ARGB — the same layout `BufferedImage.getRGB` used to give. */
private class DecodedPng(val width: Int, val height: Int, val argb: IntArray)

private fun paeth(a: Int, b: Int, c: Int): Int {
    val p = a + b - c
    val pa = kotlin.math.abs(p - a)
    val pb = kotlin.math.abs(p - b)
    val pc = kotlin.math.abs(p - c)
    return if (pa <= pb && pa <= pc) a else if (pb <= pc) b else c
}

/**
 * Minimal PNG decoder (test-only). Handles colour types 0 (grey),
 * 2 (RGB), 3 (palette), 4 (grey+alpha) and 6 (RGBA), bit depths 8
 * and 16 (16-bit takes the high byte), all five filters, non-interlaced
 * images. Returns null for anything else (JPEG/BMP/GIF/WebP/AVIF/TIFF,
 * interlaced, sub-byte depths, corrupt data) — the caller falls back
 * to [DusrelImage.pickColour] per pixel. `tRNS` is ignored (the core
 * never emits it).
 */
private fun decodePng(data: ByteArray): DecodedPng? {
    val signature = byteArrayOf(0x89.toByte(), 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A)
    if (data.size < 8 || !data.copyOf(8).contentEquals(signature)) return null
    fun u32be(off: Int): Int =
        ((data[off].toInt() and 0xFF) shl 24) or ((data[off + 1].toInt() and 0xFF) shl 16) or
            ((data[off + 2].toInt() and 0xFF) shl 8) or (data[off + 3].toInt() and 0xFF)
    var pos = 8
    var width = 0
    var height = 0
    var bitDepth = 8
    var colourType = 2
    var interlace = 0
    var palette: ByteArray? = null
    val idat = ByteArrayOutputStream()
    while (pos + 8 <= data.size) {
        val len = u32be(pos)
        if (len < 0 || pos + 12 + len > data.size) return null
        val type = String(data, pos + 4, 4, Charsets.US_ASCII)
        val chunk = data.copyOfRange(pos + 8, pos + 8 + len)
        when (type) {
            "IHDR" -> {
                if (chunk.size < 13) return null
                width = u32be(pos + 8)
                height = u32be(pos + 12)
                bitDepth = chunk[8].toInt() and 0xFF
                colourType = chunk[9].toInt() and 0xFF
                interlace = chunk[12].toInt() and 0xFF
            }
            "PLTE" -> palette = chunk
            "IDAT" -> idat.write(chunk)
            "IEND" -> break
        }
        pos += 12 + len
    }
    if (width <= 0 || height <= 0 || interlace != 0) return null
    if (bitDepth != 8 && bitDepth != 16) return null
    val channels = when (colourType) {
        0 -> 1
        2 -> 3
        3 -> 1
        4 -> 2
        6 -> 4
        else -> return null
    }
    if (colourType == 3 && palette == null) return null
    val bytesPerPixel = channels * (bitDepth / 8)
    val rowBytes = width * bytesPerPixel
    val inflater = Inflater()
    inflater.setInput(idat.toByteArray())
    val raw = ByteArray(height * (1 + rowBytes))
    var produced = 0
    try {
        while (!inflater.finished() && produced < raw.size) {
            val n = inflater.inflate(raw, produced, raw.size - produced)
            if (n == 0) break
            produced += n
        }
    } catch (e: Exception) {
        return null
    } finally {
        inflater.end()
    }
    if (produced < raw.size) return null
    val argb = IntArray(width * height)
    var prevRow = ByteArray(rowBytes)
    for (y in 0 until height) {
        val filter = raw[y * (1 + rowBytes)].toInt() and 0xFF
        val row = raw.copyOfRange(y * (1 + rowBytes) + 1, (y + 1) * (1 + rowBytes))
        for (i in row.indices) {
            val a = if (i >= bytesPerPixel) row[i - bytesPerPixel].toInt() and 0xFF else 0
            val b = prevRow[i].toInt() and 0xFF
            val c = if (i >= bytesPerPixel) prevRow[i - bytesPerPixel].toInt() and 0xFF else 0
            val v = row[i].toInt() and 0xFF
            val recon = when (filter) {
                0 -> v
                1 -> v + a
                2 -> v + b
                3 -> v + (a + b) / 2
                4 -> v + paeth(a, b, c)
                else -> return null
            }
            row[i] = (recon and 0xFF).toByte()
        }
        for (x in 0 until width) {
            val o = x * bytesPerPixel
            fun sample(ch: Int): Int =
                if (bitDepth == 16) row[o + ch * 2].toInt() and 0xFF else row[o + ch].toInt() and 0xFF
            val r: Int
            val g: Int
            val b: Int
            val a: Int
            when (colourType) {
                0 -> {
                    val v = sample(0)
                    r = v; g = v; b = v; a = 255
                }
                2 -> {
                    r = sample(0); g = sample(1); b = sample(2); a = 255
                }
                3 -> {
                    val idx = sample(0) * 3
                    val pal = palette!!
                    if (idx + 2 >= pal.size) return null
                    r = pal[idx].toInt() and 0xFF
                    g = pal[idx + 1].toInt() and 0xFF
                    b = pal[idx + 2].toInt() and 0xFF
                    a = 255
                }
                4 -> {
                    val v = sample(0)
                    r = v; g = v; b = v; a = sample(1)
                }
                else -> {
                    r = sample(0); g = sample(1); b = sample(2); a = sample(3)
                }
            }
            argb[y * width + x] = (a shl 24) or (r shl 16) or (g shl 8) or b
        }
        prevRow = row
    }
    return DecodedPng(width, height, argb)
}

/** Pixel access over an output image: minimal PNG decode when possible, else [DusrelImage.pickColour] per pixel. */
private class Pixels(private val data: ByteArray, private val caseId: String) {
    private val image: DecodedPng? = try {
        decodePng(data)
    } catch (e: Exception) {
        null
    }

    fun pixel(x: Int, y: Int): IntArray {
        val img = image
        if (img != null) {
            val argb = img.argb[y * img.width + x]
            return intArrayOf((argb shr 16) and 0xFF, (argb shr 8) and 0xFF, argb and 0xFF, (argb ushr 24) and 0xFF)
        }
        val col = DusrelImage.pickColour(data, x.toUInt(), y.toUInt())
        return intArrayOf(col.r.toInt(), col.g.toInt(), col.b.toInt(), col.a.toInt())
    }

    private fun requireImage(): DecodedPng = image
        ?: throw IllegalStateException("case $caseId: region check needs a decodable (PNG) output")

    fun regionHasDark(x0: Int, y0: Int, x1: Int, y1: Int): Boolean {
        requireImage()
        for (y in y0 until y1) {
            for (x in x0 until x1) {
                val px = pixel(x, y)
                if (px[0] < 128 || px[1] < 128 || px[2] < 128) return true
            }
        }
        return false
    }

    fun regionAllWhite(x0: Int, y0: Int, x1: Int, y1: Int): Boolean {
        requireImage()
        for (y in y0 until y1) {
            for (x in x0 until x1) {
                val px = pixel(x, y)
                if (px[0] != 255 || px[1] != 255 || px[2] != 255) return false
            }
        }
        return true
    }
}

private fun checkPixelAssert(px: IntArray, spec: JSONObject, caseId: String, failures: MutableList<String>) {
    val x = spec.getInt("x")
    val y = spec.getInt("y")
    if (spec.has("rgba")) {
        val want = spec.getJSONArray("rgba")
        val got = px.joinToString(",")
        val wantStr = (0 until want.length()).joinToString(",") { want.getInt(it).toString() }
        if (got != wantStr) failures.add("case $caseId: pixel ($x,$y) expected rgba $wantStr, got $got")
    }
    if (spec.has("rgba_approx")) {
        val want = spec.getJSONArray("rgba_approx")
        val tol = spec.optInt("tolerance", 0)
        for (i in 0 until want.length()) {
            if (kotlin.math.abs(px[i] - want.getInt(i)) > tol) {
                failures.add("case $caseId: pixel ($x,$y) channel $i expected ~${want.getInt(i)}±$tol, got ${px[i]}")
            }
        }
    }
    for ((key, ch) in listOf("r" to 0, "g" to 1, "b" to 2, "a" to 3)) {
        if (spec.has(key) && px[ch] != spec.getInt(key)) {
            failures.add("case $caseId: pixel ($x,$y) channel $key expected ${spec.getInt(key)}, got ${px[ch]}")
        }
    }
    if (spec.has("r_max") && px[0] > spec.getInt("r_max")) {
        failures.add("case $caseId: pixel ($x,$y) r expected <= ${spec.getInt("r_max")}, got ${px[0]}")
    }
}

private fun assertExpected(
    c: JSONObject,
    caseId: String,
    outcome: Outcome?,
    error: DusrelError?,
    input: ByteArray?,
    failures: MutableList<String>,
) {
    val expected: JSONObject = c.getJSONObject("expected")
    for (key in expected.keys()) {
        if (key !in VectorsTest.KNOWN_EXPECTED_KEYS) {
            failures.add("case $caseId: unhandled expected key $key — update this runner")
        }
    }
    if (expected.has("error")) {
        if (error == null) {
            failures.add("case $caseId: expected error ${expected.getString("error")}, got success")
        } else if (error.code != expected.getString("error")) {
            failures.add("case $caseId: expected error ${expected.getString("error")}, got ${error.code}")
        }
        return
    }
    if (error != null) {
        failures.add("case $caseId: expected success, got ${error.code} (${error.messageEn})")
        return
    }
    if (outcome == null) {
        failures.add("case $caseId: no outcome and no error")
        return
    }

    var info: ImageInfo? = null
    if (outcome is Outcome.Info) {
        info = outcome.info
    } else if (outcome is Outcome.Image) {
        // AVIF output cannot be decoded back in this build (no
        // avif-native decoder): a case that only asserts the header/size
        // may skip the decode; anything asserting dims must be decodable.
        try {
            info = DusrelImage.getImageInfo(outcome.bytes)
        } catch (e: DusrelError) {
            if (listOf("format", "width", "height", "has_alpha", "colour_type").any { expected.has(it) }) throw e
            info = null
        }
    }
    if (info != null) {
        if (expected.has("format") && info.format != expected.getString("format")) {
            failures.add("case $caseId: expected format ${expected.getString("format")}, got ${info.format}")
        }
        if (expected.has("width") && info.width.toInt() != expected.getInt("width")) {
            failures.add("case $caseId: expected width ${expected.getInt("width")}, got ${info.width}")
        }
        if (expected.has("height") && info.height.toInt() != expected.getInt("height")) {
            failures.add("case $caseId: expected height ${expected.getInt("height")}, got ${info.height}")
        }
        if (expected.has("has_alpha") && info.hasAlpha != expected.getBoolean("has_alpha")) {
            failures.add("case $caseId: expected has_alpha ${expected.getBoolean("has_alpha")}, got ${info.hasAlpha}")
        }
        if (expected.has("colour_type") && info.colourType != expected.getString("colour_type")) {
            failures.add("case $caseId: expected colour_type ${expected.getString("colour_type")}, got ${info.colourType}")
        }
    }

    if (outcome is Outcome.Image) {
        val bytes = outcome.bytes
        if (expected.has("max_bytes") && bytes.size > expected.getInt("max_bytes")) {
            failures.add("case $caseId: expected <= ${expected.getInt("max_bytes")} bytes, got ${bytes.size}")
        }
        if (expected.optBoolean("avif_header", false)) {
            if (bytes.size < 12 || bytes.copyOfRange(4, 12).toString(Charsets.US_ASCII) != "ftypavif") {
                failures.add("case $caseId: expected AVIF ftypavif header")
            }
        }
        val pixels = Pixels(bytes, caseId)
        val inputPixels = if (input != null) Pixels(input, caseId) else null
        if (expected.has("pixels")) {
            val arr = expected.getJSONArray("pixels")
            for (i in 0 until arr.length()) {
                val spec = arr.getJSONObject(i)
                checkPixelAssert(pixels.pixel(spec.getInt("x"), spec.getInt("y")), spec, caseId, failures)
            }
        }
        for ((key, check) in listOf(
            "region_all_white" to { r: JSONObject -> pixels.regionAllWhite(r.getInt("x0"), r.getInt("y0"), r.getInt("x1"), r.getInt("y1")) },
            "region_has_dark" to { r: JSONObject -> pixels.regionHasDark(r.getInt("x0"), r.getInt("y0"), r.getInt("x1"), r.getInt("y1")) },
        )) {
            if (expected.has(key)) {
                val r = expected.getJSONObject(key)
                if (!check(r)) {
                    failures.add("case $caseId: region (${r.getInt("x0")},${r.getInt("y0")})-(${r.getInt("x1")},${r.getInt("y1")}) failed $key")
                }
            }
        }
        if (expected.has("regions_have_dark")) {
            val arr = expected.getJSONArray("regions_have_dark")
            for (i in 0 until arr.length()) {
                val r = arr.getJSONObject(i)
                if (!pixels.regionHasDark(r.getInt("x0"), r.getInt("y0"), r.getInt("x1"), r.getInt("y1"))) {
                    failures.add("case $caseId: region (${r.getInt("x0")},${r.getInt("y0")})-(${r.getInt("x1")},${r.getInt("y1")}) should contain dark pixels")
                }
            }
        }
        if (inputPixels != null) {
            if (expected.has("differs_from_input")) {
                val arr = expected.getJSONArray("differs_from_input")
                for (i in 0 until arr.length()) {
                    val pair = arr.getJSONArray(i)
                    val (x, y) = pair.getInt(0) to pair.getInt(1)
                    if (pixels.pixel(x, y).contentEquals(inputPixels.pixel(x, y))) {
                        failures.add("case $caseId: pixel ($x,$y) should differ from the input")
                    }
                }
            }
            if (expected.has("pixel_equalities")) {
                val arr = expected.getJSONArray("pixel_equalities")
                for (i in 0 until arr.length()) {
                    val pair = arr.getJSONObject(i)
                    val a = pair.getJSONArray("a")
                    val b = pair.getJSONArray("b")
                    val same = pixels.pixel(a.getInt(0), a.getInt(1))
                        .contentEquals(pixels.pixel(b.getInt(0), b.getInt(1)))
                    if (same != pair.getBoolean("equal")) {
                        failures.add("case $caseId: pixels (${a.getInt(0)},${a.getInt(1)}) vs (${b.getInt(0)},${b.getInt(1)}) equal=$same, expected ${pair.getBoolean("equal")}")
                    }
                }
            }
            if (expected.has("unchanged_pixels")) {
                val arr = expected.getJSONArray("unchanged_pixels")
                for (i in 0 until arr.length()) {
                    val pair = arr.getJSONArray(i)
                    val (x, y) = pair.getInt(0) to pair.getInt(1)
                    if (!pixels.pixel(x, y).contentEquals(inputPixels.pixel(x, y))) {
                        failures.add("case $caseId: pixel ($x,$y) should be unchanged from the input")
                    }
                }
            }
        }
    }

    if (outcome is Outcome.ColourOut) {
        val colour = outcome.colour
        if (expected.has("colour")) {
            val want = expected.getJSONObject("colour")
            if (want.has("hex") && colour.hex != want.getString("hex")) {
                failures.add("case $caseId: expected colour hex=${want.getString("hex")}, got ${colour.hex}")
            }
            if (want.has("r") && colour.r.toInt() != want.getInt("r")) {
                failures.add("case $caseId: expected colour r=${want.getInt("r")}, got ${colour.r}")
            }
            if (want.has("g") && colour.g.toInt() != want.getInt("g")) {
                failures.add("case $caseId: expected colour g=${want.getInt("g")}, got ${colour.g}")
            }
            if (want.has("b") && colour.b.toInt() != want.getInt("b")) {
                failures.add("case $caseId: expected colour b=${want.getInt("b")}, got ${colour.b}")
            }
            if (want.has("a") && colour.a.toInt() != want.getInt("a")) {
                failures.add("case $caseId: expected colour a=${want.getInt("a")}, got ${colour.a}")
            }
        }
        if (expected.has("different_from") && input != null) {
            val other = expected.getJSONObject("different_from")
            val otherColour = DusrelImage.pickColour(input, other.getInt("x").toUInt(), other.getInt("y").toUInt())
            if (otherColour.hex == colour.hex) {
                failures.add("case $caseId: colour should differ from the pixel at (${other.getInt("x")},${other.getInt("y")})")
            }
        }
    }

    if (outcome is Outcome.PaletteOut) {
        val palette = outcome.palette
        if (expected.has("palette_length") && palette.size != expected.getInt("palette_length")) {
            failures.add("case $caseId: expected palette length ${expected.getInt("palette_length")}, got ${palette.size}")
        }
        if (expected.has("palette_hexes")) {
            // Compared as a set: the contract orders by frequency ("most
            // frequent first") and these shares are equal, so the tie order
            // is unspecified — the core breaks ties by hex ascending.
            val got = palette.map { it.colour.hex }.sorted()
            val wantArr = expected.getJSONArray("palette_hexes")
            val want = (0 until wantArr.length()).map { wantArr.getString(it) }.sorted()
            if (got != want) {
                failures.add("case $caseId: expected palette hexes ${want.joinToString(",")}, got ${got.joinToString(",")}")
            }
        }
        val shareTol = expected.optDouble("share_tolerance", 0.0)
        if (expected.has("share_each")) {
            for (entry in palette) {
                if (kotlin.math.abs(entry.share - expected.getDouble("share_each").toFloat()) > shareTol) {
                    failures.add("case $caseId: palette ${entry.colour.hex} share expected ${expected.getDouble("share_each")}±$shareTol, got ${entry.share}")
                }
            }
        }
        if (expected.has("share_sum")) {
            val sum = palette.sumOf { it.share.toDouble() }
            if (kotlin.math.abs(sum - expected.getDouble("share_sum")) > expected.optDouble("share_sum_tolerance", 0.0)) {
                failures.add("case $caseId: palette share sum expected ${expected.getDouble("share_sum")}, got $sum")
            }
        }
        if (expected.has("palette")) {
            val want = expected.getJSONArray("palette")
            if (want.length() != palette.size) {
                failures.add("case $caseId: expected palette of ${want.length()}, got ${palette.size}")
            } else {
                for (i in 0 until want.length()) {
                    val w = want.getJSONObject(i)
                    if (w.has("hex") && palette[i].colour.hex != w.getString("hex")) {
                        failures.add("case $caseId: expected palette hex ${w.getString("hex")}, got ${palette[i].colour.hex}")
                    }
                    if (w.has("share") && kotlin.math.abs(palette[i].share - w.getDouble("share").toFloat()) > w.optDouble("share_tolerance", 0.0)) {
                        failures.add("case $caseId: palette ${palette[i].colour.hex} share expected ${w.getDouble("share")}, got ${palette[i].share}")
                    }
                }
            }
        }
    }

    if (outcome is Outcome.BulkOut) {
        val results = outcome.results
        val want = if (expected.has("results")) expected.getJSONArray("results") else JSONArray()
        if (results.size != want.length()) {
            failures.add("case $caseId: expected ${want.length()} bulk results, got ${results.size}")
            return
        }
        results.forEachIndexed { i, got ->
            val wantItem = want.getJSONObject(i)
            val idx = if (wantItem.has("index")) wantItem.getInt("index").toUInt() else got.index
            if (got.index != idx) {
                failures.add("case $caseId: bulk item index expected $idx, got ${got.index}")
            }
            val wantOk = wantItem.optBoolean("ok", true)
            val gotOk = got !is BulkResult.Failed
            if (gotOk != wantOk) {
                failures.add("case $caseId: bulk item $idx ok expected $wantOk, got $gotOk")
                return@forEachIndexed
            }
            if (!gotOk) {
                val err = (got as BulkResult.Failed).error
                if (wantItem.has("error") && err.code != wantItem.getString("error")) {
                    failures.add("case $caseId: bulk item $idx error expected ${wantItem.getString("error")}, got ${err.code}")
                }
                return@forEachIndexed
            }
            when (wantItem.optString("output", "")) {
                "Image" -> {
                    val bytes = (got as BulkResult.Image).bytes
                    val info = DusrelImage.getImageInfo(bytes)
                    if (wantItem.has("width") && info.width.toInt() != wantItem.getInt("width")) {
                        failures.add("case $caseId: bulk item $idx width expected ${wantItem.getInt("width")}, got ${info.width}")
                    }
                    if (wantItem.has("height") && info.height.toInt() != wantItem.getInt("height")) {
                        failures.add("case $caseId: bulk item $idx height expected ${wantItem.getInt("height")}, got ${info.height}")
                    }
                }
                "Colour" -> {
                    if (wantItem.has("colour")) {
                        val hex = (got as BulkResult.Colour).colour.hex
                        if (hex != wantItem.getJSONObject("colour").getString("hex")) {
                            failures.add("case $caseId: bulk item $idx colour hex expected ${wantItem.getJSONObject("colour").getString("hex")}, got $hex")
                        }
                    }
                }
                "Palette" -> {
                    val pal = (got as BulkResult.Palette).palette
                    if (wantItem.has("palette_length") && pal.size != wantItem.getInt("palette_length")) {
                        failures.add("case $caseId: bulk item $idx palette length expected ${wantItem.getInt("palette_length")}, got ${pal.size}")
                    }
                    if (wantItem.has("palette")) {
                        val wantPal = wantItem.getJSONArray("palette")
                        for (j in 0 until wantPal.length()) {
                            val w = wantPal.getJSONObject(j)
                            if (w.has("hex") && pal[j].colour.hex != w.getString("hex")) {
                                failures.add("case $caseId: bulk item $idx palette hex expected ${w.getString("hex")}, got ${pal[j].colour.hex}")
                            }
                        }
                    }
                }
            }
        }
    }
}
