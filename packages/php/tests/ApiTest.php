<?php

declare(strict_types=1);

namespace Dusrelimage\Tests;

use Dusrelimage\DusrelError;
use Dusrelimage\Dusrelimage;
use PHPUnit\Framework\TestCase;

/**
 * Wrapper-level tests, mirroring packages/python/tests/test_dusrelimage.py:
 * all 10 tools, options pass-through, error codes, INVALID_TYPE for
 * non-string input, and bulk partial failure.
 *
 * Fixtures come from `dusrelimage mock` via Fixtures::mock() (the
 * `dusrelimage` CLI binary, built by scripts/build-ffi.sh).
 */
final class ApiTest extends TestCase
{
    private Dusrelimage $di;

    protected function setUp(): void
    {
        parent::setUp();
        $this->di = new Dusrelimage();
    }

    private function noise(int $width = 128, int $height = 96, int $seed = 42): string
    {
        return Fixtures::mock('noise', $width, $height, $seed);
    }

    public function testVersion(): void
    {
        self::assertSame('0.1.0', $this->di->version());
    }

    public function testAllToolsSmoke(): void
    {
        $img = $this->noise(64, 64);
        $info = fn(string $bytes): array => $this->di->getImageInfo($bytes);

        self::assertSame(64, $info($this->di->compress($img, ['quality' => 80]))['width']);
        self::assertSame(32, $info($this->di->resize($img, ['width' => 32]))['width']);
        self::assertSame(16, $info($this->di->crop($img, ['x' => 0, 'y' => 0, 'width' => 16, 'height' => 16]))['width']);
        self::assertSame('jpeg', $info($this->di->convert($img, ['format' => 'jpeg']))['format']);
        self::assertSame(64, $info($this->di->addWatermark($img, ['text' => 'X']))['width']);
        $annotated = $this->di->annotate($img, [
            ['type' => 'rect', 'x' => 5, 'y' => 5, 'w' => 10, 'h' => 10, 'colour' => '#FF0000', 'width' => 1, 'fill' => '#FF0000'],
        ]);
        self::assertSame(64, $info($annotated)['width']);
        self::assertStringStartsWith('#', $this->di->pickColour($img, 0, 0)['hex']);
        self::assertGreaterThanOrEqual(1, count($this->di->extractPalette($img, 2)));
        $collage = $this->di->makeCollage([$img, $img], ['layout' => 'row', 'width' => 200, 'height' => 100]);
        self::assertSame([200, 100], [$info($collage)['width'], $info($collage)['height']]);
        self::assertSame(64, $info($this->di->pixelate($img, ['block_size' => 8]))['width']);
    }

    public function testOptionsPassThrough(): void
    {
        $img = $this->noise(64, 64);
        $out = $this->di->compress($img, ['format' => 'jpeg', 'quality' => 90]);
        self::assertSame('jpeg', $this->di->getImageInfo($out)['format']);
        $out = $this->di->resize($img, ['width' => 32]);
        self::assertSame(32, $this->di->getImageInfo($out)['width']);
    }

    public function testErrorsAreDusrelError(): void
    {
        $img = $this->noise(64, 64);
        try {
            $this->di->crop($img, ['x' => 60, 'y' => 60, 'width' => 20, 'height' => 20]);
            self::fail('expected CROP_OUT_OF_BOUNDS');
        } catch (DusrelError $e) {
            self::assertSame('CROP_OUT_OF_BOUNDS', $e->code);
            self::assertNotSame('', $e->messageEn);
            self::assertNotSame('', $e->messageHi);
        }
        try {
            $this->di->compress('garbage');
            self::fail('expected INVALID_IMAGE');
        } catch (DusrelError $e) {
            self::assertSame('INVALID_IMAGE', $e->code);
        }
        try {
            // @phpstan-ignore-next-line intentional wrong type
            $this->di->compress(123);
            self::fail('expected INVALID_TYPE');
        } catch (DusrelError $e) {
            self::assertSame('INVALID_TYPE', $e->code);
        }
        try {
            $this->di->compress('');
            self::fail('expected EMPTY_INPUT');
        } catch (DusrelError $e) {
            self::assertSame('EMPTY_INPUT', $e->code);
        }
    }

    public function testBulkPartialFailure(): void
    {
        $good = $this->noise(64, 64);
        $items = $this->di->bulkProcess([$good, 'garbage', $good], 'resize', ['width' => 32]);
        self::assertCount(3, $items);
        self::assertTrue($items[0]['ok']);
        self::assertSame(32, $this->di->getImageInfo($items[0]['output'])['width']);
        self::assertFalse($items[1]['ok']);
        self::assertArrayNotHasKey('output', $items[1]);
        self::assertSame('INVALID_IMAGE', $items[1]['error']['code']);
        self::assertTrue($items[2]['ok']);
        self::assertSame(32, $this->di->getImageInfo($items[2]['output'])['width']);
    }

    public function testBulkEmptyInputList(): void
    {
        self::assertSame([], $this->di->bulkProcess([], 'resize', ['width' => 32]));
    }

    public function testBulkUnknownOp(): void
    {
        try {
            $this->di->bulkProcess([$this->noise(32, 32)], 'explode', []);
            self::fail('expected INVALID_ARGUMENT');
        } catch (DusrelError $e) {
            self::assertSame('INVALID_ARGUMENT', $e->code);
        }
    }

    public function testCompressToSizeReachesTarget(): void
    {
        $img = $this->noise(400, 300, 7);
        $out = $this->di->compressToSize($img, 200000, ['format' => 'Jpeg', 'quality' => 90]);
        self::assertLessThanOrEqual(200000, strlen($out));
    }

    public function testClientSideValidationNeedsNoNativeCall(): void
    {
        $img = $this->noise(32, 32);
        foreach ([
            [fn() => $this->di->compress($img, ['quality' => 0]), 'INVALID_QUALITY'],
            [fn() => $this->di->crop($img, ['width' => 0, 'height' => 20]), 'INVALID_DIMENSIONS'],
            [fn() => $this->di->convert($img, []), 'INVALID_ARGUMENT'],
            [fn() => $this->di->annotate($img, []), 'INVALID_ARGUMENT'],
            [fn() => $this->di->extractPalette($img, 0), 'INVALID_ARGUMENT'],
            [fn() => $this->di->extractPalette($img, 17), 'INVALID_ARGUMENT'],
            [fn() => $this->di->pixelate($img, ['block_size' => 1]), 'INVALID_ARGUMENT'],
            [fn() => $this->di->makeCollage([], []), 'EMPTY_INPUT'],
        ] as [$call, $code]) {
            try {
                $call();
                self::fail("expected {$code}");
            } catch (DusrelError $e) {
                self::assertSame($code, $e->code);
            }
        }
    }
}
