package com.dusrel.image

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Focused tests for the [DusrelImage] wrapper itself (colour helpers,
 * error mapping, defaults) — the 82 shared-vector cases live in
 * [VectorsTest]. Needs the host cdylib like [VectorsTest] does (see
 * `lib/build.gradle.kts`).
 */
class DusrelImageTest {

    @Test
    fun versionMatchesCore() {
        assertEquals("0.1.0", DusrelImage.version())
    }

    @Test
    fun colourHelpers() {
        val white = DusrelImage.white()
        assertEquals("#FFFFFF", white.hex)
        assertEquals(listOf(255, 255, 255, 255), listOf(white.r.toInt(), white.g.toInt(), white.b.toInt(), white.a.toInt()))
        assertEquals(3, white.hsl.size)
        assertEquals("#0A141E", DusrelImage.colourFromRgb(10, 20, 30).hex)
        assertEquals("#0A141E", DusrelImage.colourFromHex("#0A141E").hex)
        assertEquals("#0A141E", DusrelImage.colourFromHex("0A141E").hex)
        assertEquals("#FF0000", DusrelImage.colourFromHex("#F00").hex)
        val withAlpha = DusrelImage.colourFromHex("#0A141E80")
        assertEquals(128, withAlpha.a.toInt())
        // Red is hue 0, full saturation, half lightness.
        val red = DusrelImage.colourFromRgb(255, 0, 0)
        assertTrue(kotlin.math.abs(red.hsl[0] - 0f) < 0.001f)
        assertTrue(kotlin.math.abs(red.hsl[1] - 1f) < 0.001f)
        assertTrue(kotlin.math.abs(red.hsl[2] - 0.5f) < 0.001f)
    }

    @Test
    fun colourFromHexInvalidIsInvalidArgument() {
        val err = assertDusrelError("INVALID_ARGUMENT") {
            DusrelImage.colourFromHex("not-a-colour")
        }
        assertTrue(err.messageEn.isNotEmpty())
        assertTrue(err.messageHi.isNotEmpty())
    }

    @Test
    fun makeTestImageIsDeterministic() {
        val a = DusrelImage.makeTestImage("noise", 64u, 48u, 42u)
        val b = DusrelImage.makeTestImage("noise", 64u, 48u, 42u)
        assertTrue(a.contentEquals(b))
        assertDusrelError("INVALID_ARGUMENT") {
            DusrelImage.makeTestImage("bogus", 64u, 48u, 42u)
        }
    }

    @Test
    fun compressInvalidQualityMapsCodeAndMessages() {
        val input = DusrelImage.makeTestImage("solid", 32u, 32u, 1u)
        val err = assertDusrelError("INVALID_QUALITY") {
            DusrelImage.compressImage(input, CompressOptions(CommonOptions(), quality = 0u))
        }
        assertTrue(err.messageEn.isNotEmpty())
        assertTrue(err.messageHi.isNotEmpty())
        assertTrue(err.reasonEn == err.messageEn)
        assertTrue(err.reasonHi == err.messageHi)
    }

    @Test
    fun getImageInfoEmptyInput() {
        assertDusrelError("EMPTY_INPUT") {
            DusrelImage.getImageInfo(ByteArray(0))
        }
    }

    @Test
    fun pickColourOutOfBounds() {
        val input = DusrelImage.makeTestImage("solid", 64u, 48u, 1u)
        assertDusrelError("PIXEL_OUT_OF_BOUNDS") {
            DusrelImage.pickColour(input, 64u, 0u)
        }
    }

    @Test
    fun bulkEmptyInputListIsEmpty() {
        val results = DusrelImage.bulkProcess(
            emptyList(),
            BulkOp.Pixelate(PixelateOptions(CommonOptions())),
        )
        assertTrue(results.isEmpty())
    }

    @Test
    fun bulkPartialFailureIsPerItem() {
        val good = DusrelImage.makeTestImage("solid", 32u, 32u, 1u)
        val garbage = byteArrayOf(0, 1, 2, 3)
        val results = DusrelImage.bulkProcess(
            listOf(good, garbage),
            BulkOp.Pixelate(PixelateOptions(CommonOptions())),
        )
        assertEquals(2, results.size)
        assertTrue(results[0] is BulkResult.Image)
        val failed = results[1] as BulkResult.Failed
        assertEquals("INVALID_IMAGE", failed.error.code)
        assertTrue(failed.error.messageHi.isNotEmpty())
    }

    private inline fun assertDusrelError(expectedCode: String, block: () -> Unit): DusrelError {
        try {
            block()
        } catch (e: DusrelError) {
            assertEquals(expectedCode, e.code)
            return e
        }
        throw AssertionError("expected DusrelError $expectedCode, got success")
    }
}
