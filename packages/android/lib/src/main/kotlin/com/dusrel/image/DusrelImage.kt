package com.dusrel.image

/**
 * DusrelImage — thin idiomatic Kotlin wrapper over the UniFFI-generated
 * bindings in `dusrelimage.kt` (same package, generated from
 * `crates/dusrelimage-uniffi/src/dusrelimage.udl` with uniffi 0.29).
 *
 * What this wrapper adds over the generated API:
 * - `ByteArray` in / out (the generated functions use `List<UByte>`).
 * - Kotlin-friendly defaults: every `*Options` parameter defaults to a
 *   fully-defaulted options object. The UDL cannot give a default to a
 *   field that is itself a dictionary, so the generated `common`,
 *   `background` and `layout` fields are REQUIRED there — this wrapper
 *   fills them (`CommonOptions()` with all UDL defaults, white
 *   `#FFFFFF`, `Grid { rows: 2, cols: 2 }`) where the caller does not
 *   pass them. The defaults reproduce the Rust `Default` values
 *   (SPEC section 2) exactly.
 * - One exception type, [DusrelError], carrying the stable `code` plus
 *   `messageEn` / `messageHi` from the Rust core. The generated code
 *   throws `DusrelException` (one subclass per code); this wrapper
 *   maps every variant to [DusrelError] so callers only ever see one
 *   type — the same shape as the JS/Python `DusrelError`.
 * - Colour helpers ([white], [black], [colourFromRgb], [colourFromHex]):
 *   the generated `Colour` record has no defaults, so callers build
 *   colours through these (hex + HSL are derived from RGB, exactly as
 *   the Rust core does — the Rust side re-derives them from RGBA
 *   anyway, so a caller-supplied hex/HSL that disagrees is normalised).
 *
 * All image work still happens in the Rust core via JNI/JNA; this file
 * contains no image logic. Honest v0.1.0 limits (same as the core):
 * AVIF input does not decode, WebP output is always lossless,
 * `keepExif` / `dpi` / `progressive` are accepted but not honoured,
 * `compress_to_size` quality search is JPEG/AVIF-only, TIFF input
 * falls back to PNG, collage output is always PNG, GIF output is the
 * first frame only, no HEIC, no streaming.
 */
object DusrelImage {

    // ------------------------------------------------------------------
    // Colours
    // ------------------------------------------------------------------

    /** White `#FFFFFF` — the background default for convert / resize-fill / collage. */
    fun white(): Colour = colourFromRgb(255, 255, 255)

    /** Black `#000000`. */
    fun black(): Colour = colourFromRgb(0, 0, 0)

    /**
     * Build a [Colour] from 8-bit channels. `hex` is always `"#RRGGBB"`
     * (alpha is NOT in the hex string) and `hsl` is
     * `[h_degrees_0..360, s_0..1, l_0..1]` — both derived from RGB,
     * mirroring `Colour::from_rgba` in the Rust core.
     */
    fun colourFromRgb(r: Int, g: Int, b: Int, a: Int = 255): Colour {
        require(r in 0..255 && g in 0..255 && b in 0..255 && a in 0..255) {
            "colour channels must be 0..255, got r=$r g=$g b=$b a=$a"
        }
        return Colour(
            hex = "#%02X%02X%02X".format(r, g, b),
            r = r.toUByte(),
            g = g.toUByte(),
            b = b.toUByte(),
            a = a.toUByte(),
            hsl = rgbToHsl(r, g, b),
        )
    }

    /**
     * Parse `"#RRGGBB"`, `"RRGGBB"`, `"#RGB"` or `"#RRGGBBAA"` —
     * the same forms as `Colour::from_hex` in the Rust core.
     * Anything else throws [DusrelError] with code `INVALID_ARGUMENT`.
     */
    fun colourFromHex(hex: String): Colour {
        val h = hex.removePrefix("#")
        val bad = {
            throw DusrelError(
                code = "INVALID_ARGUMENT",
                messageEn = "Invalid hex colour \"$hex\". Use \"#RRGGBB\".",
                messageHi = "Galat hex colour \"$hex\" hai. \"#RRGGBB\" use karein.",
            )
        }
        val (r, g, b, a) = when (h.length) {
            3 -> {
                val n = { i: Int ->
                    h.substring(i, i + 1).toIntOrNull(16)?.let { it * 16 + it } ?: bad()
                }
                Quad(n(0), n(1), n(2), 255)
            }
            6 -> {
                val n = { i: Int -> h.substring(i, i + 2).toIntOrNull(16) ?: bad() }
                Quad(n(0), n(2), n(4), 255)
            }
            8 -> {
                val n = { i: Int -> h.substring(i, i + 2).toIntOrNull(16) ?: bad() }
                Quad(n(0), n(2), n(4), n(6))
            }
            else -> bad()
        }
        return colourFromRgb(r, g, b, a)
    }

    private data class Quad(val r: Int, val g: Int, val b: Int, val a: Int)

    /** RGB (0-255) -> HSL (h degrees 0..360, s/l 0..1); mirrors the core `rgb_to_hsl`. */
    internal fun rgbToHsl(r: Int, g: Int, b: Int): List<Float> {
        val rf = r / 255f
        val gf = g / 255f
        val bf = b / 255f
        val max = maxOf(rf, gf, bf)
        val min = minOf(rf, gf, bf)
        val l = (max + min) / 2f
        if (max - min < 1e-6f) return listOf(0f, 0f, l)
        val d = max - min
        val s = if (l > 0.5f) d / (2f - max - min) else d / (max + min)
        val h = when {
            max == rf -> 60f * (((gf - bf) / d).mod(6f))
            max == gf -> 60f * ((bf - rf) / d + 2f)
            else -> 60f * ((rf - gf) / d + 4f)
        }
        return listOf(h, s, l)
    }

    // ------------------------------------------------------------------
    // Small option builders
    // ------------------------------------------------------------------

    /** Text watermark: string, size in px, colour (default white, like the Rust `Default`). */
    fun textWatermark(text: String, size: Float = 48f, colour: Colour = white()): TextWatermark =
        TextWatermark(text = text, size = size, colour = colour)

    /** Logo watermark: raw image bytes + scale (`1.0` = natural size). */
    fun imageWatermark(image: ByteArray, scale: Float = 1f): ImageWatermark =
        ImageWatermark(image = image.toUByteList(), scale = scale)

    // ------------------------------------------------------------------
    // Tools
    // ------------------------------------------------------------------

    /** Crate version, e.g. `"0.1.0"` (lockstep with npm / PyPI / crates.io). */
    fun version(): String = com.dusrel.image.version()

    /** Tool 1 — re-encode an image at a fixed quality (and `format`, or the input format when unset). */
    fun compressImage(
        input: ByteArray,
        options: CompressOptions = CompressOptions(CommonOptions()),
    ): ByteArray = catching {
        com.dusrel.image.compressImage(input.toUByteList(), options).toByteArray()
    }

    /** Tool 1b — re-encode so the result is at most [targetBytes] bytes, at the highest quality that fits. */
    fun compressToSize(
        input: ByteArray,
        targetBytes: ULong,
        options: CompressOptions = CompressOptions(CommonOptions()),
    ): ByteArray = catching {
        com.dusrel.image.compressToSize(input.toUByteList(), targetBytes, options).toByteArray()
    }

    /** Tool 2 — resize (width and/or height, percent, fit, filter). */
    fun resizeImage(
        input: ByteArray,
        options: ResizeOptions = ResizeOptions(CommonOptions(), background = white()),
    ): ByteArray = catching {
        com.dusrel.image.resizeImage(input.toUByteList(), options).toByteArray()
    }

    /** Tool 3 — crop to a rectangle (or an anchor preset). */
    fun cropImage(input: ByteArray, options: CropOptions): ByteArray = catching {
        com.dusrel.image.cropImage(input.toUByteList(), options).toByteArray()
    }

    /** Tool 3 convenience — explicit rectangle; `anchor` places the rect and ignores `x`/`y`. */
    fun cropImage(
        input: ByteArray,
        x: UInt = 0u,
        y: UInt = 0u,
        width: UInt,
        height: UInt,
        anchor: Position? = null,
        common: CommonOptions = CommonOptions(),
    ): ByteArray = cropImage(
        input,
        CropOptions(common = common, x = x, y = y, width = width, height = height, anchor = anchor),
    )

    /** Tool 4 — convert to another format. */
    fun convertImage(input: ByteArray, options: ConvertOptions): ByteArray = catching {
        com.dusrel.image.convertImage(input.toUByteList(), options).toByteArray()
    }

    /** Tool 4 convenience — target format first; background flattens alpha for JPEG/BMP. */
    fun convertImage(
        input: ByteArray,
        format: ImageFormat,
        quality: UByte = 80u,
        background: Colour = white(),
        progressive: Boolean = false,
        common: CommonOptions = CommonOptions(),
    ): ByteArray = convertImage(
        input,
        ConvertOptions(
            common = common,
            format = format,
            quality = quality,
            background = background,
            progressive = progressive,
        ),
    )

    /** Tool 5 — stamp a text or logo watermark. At least one of `text` / `logo` is required. */
    fun addWatermark(
        input: ByteArray,
        options: WatermarkOptions = WatermarkOptions(CommonOptions()),
    ): ByteArray = catching {
        com.dusrel.image.addWatermark(input.toUByteList(), options).toByteArray()
    }

    /** Tool 6 — draw annotations (arrows, rects, ellipses, lines, text). */
    fun annotateImage(input: ByteArray, annotations: List<Annotation>): ByteArray = catching {
        com.dusrel.image.annotateImage(input.toUByteList(), annotations).toByteArray()
    }

    /** Tool 7 — read the colour of one pixel. Out-of-bounds is `PIXEL_OUT_OF_BOUNDS`. */
    fun pickColour(input: ByteArray, x: UInt, y: UInt): Colour = catching {
        com.dusrel.image.pickColour(input.toUByteList(), x, y)
    }

    /** Tool 8 — dominant colours, most frequent first (count 1-16). */
    fun extractPalette(input: ByteArray, count: UByte = 5u): List<PaletteColour> = catching {
        com.dusrel.image.extractPalette(input.toUByteList(), count)
    }

    /** Tool 9 — build a collage from several images. The output is always PNG. */
    fun makeCollage(
        inputs: List<ByteArray>,
        options: CollageOptions = CollageOptions(
            CommonOptions(),
            layout = CollageLayout.Grid(rows = 2u, cols = 2u),
            background = white(),
        ),
    ): ByteArray = catching {
        com.dusrel.image.makeCollage(inputs.map { it.toUByteList() }, options).toByteArray()
    }

    /** Tool 10 — pixelate a rectangle (whole image when width/height are 0). */
    fun pixelateImage(
        input: ByteArray,
        options: PixelateOptions = PixelateOptions(CommonOptions()),
    ): ByteArray = catching {
        com.dusrel.image.pixelateImage(input.toUByteList(), options).toByteArray()
    }

    /** Describe an image without modifying it. */
    fun getImageInfo(
        input: ByteArray,
        options: CommonOptions = CommonOptions(),
    ): ImageInfo = catching {
        com.dusrel.image.getImageInfo(input.toUByteList(), options)
    }

    /**
     * Run one operation over many inputs in a single call. One bad input
     * fails only its own item (as [BulkResult.Failed] with the per-item
     * [DusrelError]), never the whole batch. An empty input list returns
     * an empty list. `BulkOp.Collage` is not a per-item op: it returns
     * one `INVALID_ARGUMENT` result per input — call [makeCollage] once
     * with the full list instead.
     */
    fun bulkProcess(inputs: List<ByteArray>, op: BulkOp): List<BulkResult> {
        val results = com.dusrel.image.bulkProcess(inputs.map { it.toUByteList() }, op)
        return results.map { item ->
            if (!item.ok) {
                BulkResult.Failed(
                    index = item.index,
                    error = DusrelError(
                        code = item.errorCode ?: "INVALID_ARGUMENT",
                        messageEn = item.errorMessageEn ?: "",
                        messageHi = item.errorMessageHi ?: "",
                    ),
                )
            } else {
                when (val out = item.output) {
                    is BulkOutput.Image -> BulkResult.Image(item.index, out.bytes.toByteArray())
                    is BulkOutput.Colour -> BulkResult.Colour(item.index, out.colour)
                    is BulkOutput.Palette -> BulkResult.Palette(item.index, out.palette)
                    null -> BulkResult.Failed(
                        item.index,
                        DusrelError("INVALID_ARGUMENT", "Bulk item succeeded with no output.", "Bulk item bina output ke safal hua."),
                    )
                }
            }
        }
    }

    /**
     * Build a deterministic test image (PNG bytes). `kind` is `"solid"`,
     * `"gradient"` or `"noise"`; anything else is `INVALID_ARGUMENT`.
     * Same builders as the CLI `mock` command — the shared-vector tests
     * use this for fixtures instead of shelling out.
     */
    fun makeTestImage(
        kind: String = "noise",
        width: UInt = 64u,
        height: UInt = 64u,
        seed: ULong = 42u,
    ): ByteArray = catching {
        com.dusrel.image.makeTestImage(kind, width, height, seed).toByteArray()
    }

    // ------------------------------------------------------------------
    // Internals
    // ------------------------------------------------------------------

    /** Map the generated `DusrelException` (one subclass per code) to the single [DusrelError]. */
    private fun mapError(e: DusrelException): DusrelError = when (e) {
        is DusrelException.InvalidImage -> DusrelError(e.code, e.messageEn, e.messageHi)
        is DusrelException.UnsupportedFormat -> DusrelError(e.code, e.messageEn, e.messageHi)
        is DusrelException.ImageTooLarge -> DusrelError(e.code, e.messageEn, e.messageHi)
        is DusrelException.TargetSizeUnreachable -> DusrelError(e.code, e.messageEn, e.messageHi)
        is DusrelException.CropOutOfBounds -> DusrelError(e.code, e.messageEn, e.messageHi)
        is DusrelException.PixelOutOfBounds -> DusrelError(e.code, e.messageEn, e.messageHi)
        is DusrelException.InvalidQuality -> DusrelError(e.code, e.messageEn, e.messageHi)
        is DusrelException.InvalidDimensions -> DusrelError(e.code, e.messageEn, e.messageHi)
        is DusrelException.InvalidArgument -> DusrelError(e.code, e.messageEn, e.messageHi)
        is DusrelException.EmptyInput -> DusrelError(e.code, e.messageEn, e.messageHi)
        is DusrelException.InvalidType -> DusrelError(e.code, e.messageEn, e.messageHi)
    }

    private inline fun <T> catching(block: () -> T): T = try {
        block()
    } catch (e: DusrelException) {
        throw mapError(e)
    }
}

/** Every failure from this package. `code` is stable and machine-readable. */
class DusrelError(
    val code: String,
    val messageEn: String,
    val messageHi: String,
) : Exception("$code: $messageEn") {
    /** Alias of `messageEn` (matches the JS/Python wrappers). */
    val reasonEn: String get() = messageEn

    /** Alias of `messageHi`. */
    val reasonHi: String get() = messageHi
}

/** One item of a [DusrelImage.bulkProcess] result, in input order. */
sealed interface BulkResult {
    val index: UInt

    /** The op produced image bytes. */
    data class Image(override val index: UInt, val bytes: ByteArray) : BulkResult {
        // ByteArray is reference-compared by data-class equals; compare with contentEquals.
        override fun equals(other: Any?): Boolean =
            other is Image && index == other.index && bytes.contentEquals(other.bytes)

        override fun hashCode(): Int = 31 * index.hashCode() + bytes.contentHashCode()
    }

    /** The op produced a single colour ([BulkOp.PickColour]). */
    data class Colour(override val index: UInt, val colour: com.dusrel.image.Colour) : BulkResult

    /** The op produced a palette ([BulkOp.Palette]). */
    data class Palette(override val index: UInt, val palette: List<PaletteColour>) : BulkResult

    /** The op failed for this input only; the batch continues. */
    data class Failed(override val index: UInt, val error: DusrelError) : BulkResult
}

private fun ByteArray.toUByteList(): List<UByte> = map { it.toUByte() }

private fun List<UByte>.toByteArray(): ByteArray = ByteArray(size) { this[it].toByte() }
