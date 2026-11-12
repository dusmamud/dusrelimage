<?php

declare(strict_types=1);

namespace Dusrelimage\Tests;

use Dusrelimage\DusrelError;
use Dusrelimage\Dusrelimage;
use PHPUnit\Framework\TestCase;

/**
 * Pure-PHP tests for the colour / options translation layer. These
 * need no native library at all — they run even before the .so is
 * built — so they pin the JSON the wrapper sends to the FFI layer:
 * complete objects, Rust defaults, snake_case keys, serde enum
 * spellings, externally-tagged annotations and layouts.
 */
final class OptionsTest extends TestCase
{
    public function testColourFromHex(): void
    {
        $red = Dusrelimage::colourFromHex('#FF0000');
        self::assertSame('#FF0000', $red['hex']);
        self::assertSame([255, 0, 0, 255], [$red['r'], $red['g'], $red['b'], $red['a']]);
        self::assertEqualsWithDelta([0.0, 1.0, 0.5], $red['hsl'], 0.0001);

        $short = Dusrelimage::colourFromHex('#f00');
        self::assertSame('#FF0000', $short['hex']);
        self::assertSame(255, $short['a']);

        $withAlpha = Dusrelimage::colourFromHex('#01020380');
        self::assertSame([1, 2, 3, 128], [$withAlpha['r'], $withAlpha['g'], $withAlpha['b'], $withAlpha['a']]);

        $noHash = Dusrelimage::colourFromHex('00FF00');
        self::assertSame('#00FF00', $noHash['hex']);

        $white = Dusrelimage::colourFromHex('#FFFFFF');
        self::assertEqualsWithDelta([0.0, 0.0, 1.0], $white['hsl'], 0.0001);
    }

    public function testColourFromHexInvalid(): void
    {
        foreach (['#GGGGGG', '#12345', '#1234567', 'not-a-colour', ''] as $bad) {
            try {
                Dusrelimage::colourFromHex($bad);
                self::fail("expected INVALID_ARGUMENT for {$bad}");
            } catch (DusrelError $e) {
                self::assertSame('INVALID_ARGUMENT', $e->code);
            }
        }
    }

    public function testToColourPassesFullColourThrough(): void
    {
        $colour = Dusrelimage::toColour(['hex' => '#0A141E', 'r' => 10, 'g' => 20, 'b' => 30, 'a' => 255, 'hsl' => [210.0, 0.5, 0.078]]);
        self::assertSame('#0A141E', $colour['hex']);
        self::assertSame(10, $colour['r']);

        // Missing hsl is recomputed from rgb.
        $recomputed = Dusrelimage::toColour(['r' => 255, 'g' => 0, 'b' => 0]);
        self::assertSame('#FF0000', $recomputed['hex']);
        self::assertEqualsWithDelta([0.0, 1.0, 0.5], $recomputed['hsl'], 0.0001);
    }

    public function testRgbToHslKnownValues(): void
    {
        self::assertEqualsWithDelta([0.0, 1.0, 0.5], Dusrelimage::rgbToHsl(255, 0, 0), 0.0001);
        self::assertEqualsWithDelta([120.0, 1.0, 0.5], Dusrelimage::rgbToHsl(0, 255, 0), 0.0001);
        self::assertEqualsWithDelta([240.0, 1.0, 0.5], Dusrelimage::rgbToHsl(0, 0, 255), 0.0001);
        self::assertEqualsWithDelta([0.0, 0.0, 0.0], Dusrelimage::rgbToHsl(0, 0, 0), 0.0001);
        self::assertEqualsWithDelta([0.0, 0.0, 1.0], Dusrelimage::rgbToHsl(255, 255, 255), 0.0001);
    }

    public function testCommonOptionsDefaults(): void
    {
        self::assertSame(
            ['auto_orient' => true, 'keep_exif' => false, 'dpi' => null, 'max_pixels' => 120000000, 'max_dimension' => 16000],
            Dusrelimage::commonOptions([]),
        );
    }

    public function testCommonOptionsFlatAndNested(): void
    {
        // Nested wins over flat; flat fills the gaps.
        $common = Dusrelimage::commonOptions([
            'common' => ['max_pixels' => 100],
            'max_dimension' => 10,
            'auto_orient' => false,
        ]);
        self::assertSame(100, $common['max_pixels']);
        self::assertSame(10, $common['max_dimension']);
        self::assertFalse($common['auto_orient']);
        self::assertSame([300, 300], Dusrelimage::commonOptions(['dpi' => [300, 300]])['dpi']);
    }

    public function testCompressOptionsDefaultsAndValidation(): void
    {
        $opts = Dusrelimage::compressOptions([]);
        self::assertSame(80, $opts['quality']);
        self::assertNull($opts['format']);
        self::assertFalse($opts['progressive']);
        self::assertFalse($opts['lossless']);
        self::assertSame(4, $opts['effort']);
        self::assertTrue($opts['common']['auto_orient']);

        self::assertSame('Jpeg', Dusrelimage::compressOptions(['format' => 'jpeg'])['format']);
        self::assertSame('WebP', Dusrelimage::compressOptions(['format' => 'webp'])['format']);

        foreach ([0, 101] as $quality) {
            try {
                Dusrelimage::compressOptions(['quality' => $quality]);
                self::fail("expected INVALID_QUALITY for {$quality}");
            } catch (DusrelError $e) {
                self::assertSame('INVALID_QUALITY', $e->code);
            }
        }
        try {
            Dusrelimage::compressOptions(['effort' => 7]);
            self::fail('expected INVALID_ARGUMENT for effort 7');
        } catch (DusrelError $e) {
            self::assertSame('INVALID_ARGUMENT', $e->code);
        }
    }

    public function testResizeOptionsDefaults(): void
    {
        $opts = Dusrelimage::resizeOptions([]);
        self::assertNull($opts['width']);
        self::assertNull($opts['height']);
        self::assertNull($opts['percent']);
        self::assertSame('Fit', $opts['fit']);
        self::assertSame('Lanczos3', $opts['filter']);
        self::assertSame('#FFFFFF', $opts['background']['hex']);
        self::assertSame('Fill', Dusrelimage::resizeOptions(['fit' => 'fill'])['fit']);
        self::assertSame('#FF0000', Dusrelimage::resizeOptions(['background' => '#FF0000'])['background']['hex']);
    }

    public function testCropOptionsValidation(): void
    {
        self::assertSame(
            ['common' => Dusrelimage::commonOptions([]), 'x' => 10, 'y' => 10, 'width' => 20, 'height' => 20, 'anchor' => null],
            Dusrelimage::cropOptions(['x' => 10, 'y' => 10, 'width' => 20, 'height' => 20]),
        );
        self::assertSame('Center', Dusrelimage::cropOptions(['width' => 20, 'height' => 20, 'anchor' => 'center'])['anchor']);
        try {
            Dusrelimage::cropOptions(['width' => 0, 'height' => 20]);
            self::fail('expected INVALID_DIMENSIONS for width 0');
        } catch (DusrelError $e) {
            self::assertSame('INVALID_DIMENSIONS', $e->code);
        }
    }

    public function testConvertOptionsRequiresFormat(): void
    {
        $opts = Dusrelimage::convertOptions(['format' => 'png', 'quality' => 90, 'background' => '#FF0000']);
        self::assertSame('Png', $opts['format']);
        self::assertSame(90, $opts['quality']);
        self::assertSame('#FF0000', $opts['background']['hex']);
        try {
            Dusrelimage::convertOptions([]);
            self::fail('expected INVALID_ARGUMENT without format');
        } catch (DusrelError $e) {
            self::assertSame('INVALID_ARGUMENT', $e->code);
        }
    }

    public function testWatermarkOptions(): void
    {
        $opts = Dusrelimage::watermarkOptions([
            'opacity' => 1.0,
            'position' => 'TopLeft',
            'text' => ['text' => 'Hi', 'size' => 48.0, 'colour' => '#000000'],
        ]);
        self::assertSame(1.0, $opts['opacity']);
        self::assertSame('TopLeft', $opts['position']);
        self::assertSame('Hi', $opts['text']['text']);
        self::assertSame('#000000', $opts['text']['colour']['hex']);

        // Plain-string text and raw-bytes logo (the Python-friendly spellings).
        $plain = Dusrelimage::watermarkOptions(['text' => 'X']);
        self::assertSame('X', $plain['text']['text']);
        $logo = Dusrelimage::watermarkOptions(['logo' => "\x01\x02\x03", 'opacity' => 1.0]);
        self::assertSame([1, 2, 3], $logo['logo']['image']);
        self::assertSame(1.0, $logo['logo']['scale']);

        try {
            Dusrelimage::watermarkOptions(['opacity' => 1.5, 'text' => 'Hi']);
            self::fail('expected INVALID_ARGUMENT for opacity 1.5');
        } catch (DusrelError $e) {
            self::assertSame('INVALID_ARGUMENT', $e->code);
        }
    }

    public function testLayoutToSerde(): void
    {
        self::assertSame(['Grid' => ['rows' => 2, 'cols' => 2]], Dusrelimage::layoutToSerde(null));
        self::assertSame(['Grid' => ['rows' => 2, 'cols' => 2]], Dusrelimage::layoutToSerde('grid'));
        self::assertSame(['Grid' => ['rows' => 1, 'cols' => 2]], Dusrelimage::layoutToSerde('1x2'));
        self::assertSame('Row', Dusrelimage::layoutToSerde('row'));
        self::assertSame('Column', Dusrelimage::layoutToSerde('column'));
        self::assertSame(['Grid' => ['rows' => 3, 'cols' => 1]], Dusrelimage::layoutToSerde(['rows' => 3, 'cols' => 1]));
        self::assertSame(
            ['Grid' => ['rows' => 2, 'cols' => 2]],
            Dusrelimage::layoutToSerde(['Grid' => ['rows' => 2, 'cols' => 2]]),
        );
    }

    public function testPixelateOptionsValidation(): void
    {
        $opts = Dusrelimage::pixelateOptions(['block_size' => 20, 'width' => 100, 'height' => 100]);
        self::assertSame(20, $opts['block_size']);
        foreach ([1, 101] as $block) {
            try {
                Dusrelimage::pixelateOptions(['block_size' => $block]);
                self::fail("expected INVALID_ARGUMENT for block_size {$block}");
            } catch (DusrelError $e) {
                self::assertSame('INVALID_ARGUMENT', $e->code);
            }
        }
    }

    public function testAnnotationToSerde(): void
    {
        $rect = Dusrelimage::annotationToSerde([
            'type' => 'Rect', 'x' => 10, 'y' => 10, 'w' => 40, 'h' => 30,
            'colour' => '#FF0000', 'width' => 2, 'fill' => '#FF0000',
        ]);
        self::assertSame(['Rect'], array_keys($rect));
        self::assertSame('#FF0000', $rect['Rect']['colour']['hex']);
        self::assertSame('#FF0000', $rect['Rect']['fill']['hex']);
        self::assertSame(40, $rect['Rect']['w']);

        // Lowercase type (Python spelling) + null fill passes through.
        $outline = Dusrelimage::annotationToSerde([
            'type' => 'rect', 'x' => 60, 'y' => 10, 'w' => 30, 'h' => 30,
            'colour' => '#0000FF', 'width' => 3, 'fill' => null,
        ]);
        self::assertSame(['Rect'], array_keys($outline));
        self::assertNull($outline['Rect']['fill']);

        // Already-tagged form passes through with colours expanded.
        $tagged = Dusrelimage::annotationToSerde(['Line' => ['from' => [0, 0], 'to' => [99, 99], 'colour' => '#000000', 'width' => 1]]);
        self::assertSame('#000000', $tagged['Line']['colour']['hex']);

        $list = Dusrelimage::annotationsToSerde([
            ['type' => 'Text', 'x' => 10, 'y' => 10, 'text' => 'A', 'size' => 40.0, 'colour' => '#000000'],
        ]);
        self::assertSame(['Text'], array_keys($list[0]));

        try {
            Dusrelimage::annotationsToSerde('not-an-array');
            self::fail('expected INVALID_TYPE');
        } catch (DusrelError $e) {
            self::assertSame('INVALID_TYPE', $e->code);
        }
    }

    public function testCdefDeclaresEveryContractFunction(): void
    {
        foreach ([
            'dusrelimage_compress', 'dusrelimage_compress_to_size', 'dusrelimage_resize',
            'dusrelimage_crop', 'dusrelimage_convert', 'dusrelimage_add_watermark',
            'dusrelimage_annotate', 'dusrelimage_pick_colour', 'dusrelimage_extract_palette',
            'dusrelimage_make_collage', 'dusrelimage_pixelate', 'dusrelimage_get_image_info', 'dusrelimage_bulk_process',
            'dusrelimage_last_error', 'dusrelimage_free_buffer', 'dusrelimage_version',
        ] as $fn) {
            self::assertStringContainsString($fn, Dusrelimage::CDEF, "CDEF is missing {$fn}");
        }
    }
}
