<?php

declare(strict_types=1);

namespace Dusrelimage\Tests;

use Dusrelimage\DusrelError;
use Dusrelimage\Dusrelimage;

/**
 * Fixture builders for the shared test vectors, mirroring the JS
 * (`packages/js/src/vectors.test.ts`) and Python
 * (`crates/dusrelimage-py/tests/test_vectors.py`) runners:
 *
 * - `solid` / `gradient` / `noise` fixtures come from the
 *   `dusrelimage mock` CLI command (deterministic seed), exactly as
 *   the task specifies — never from a checked-in binary blob. The
 *   CLI binary is built by `scripts/build-ffi.sh`; its path resolves
 *   from `$DUSRELIMAGE_CLI`, else `target/release/dusrelimage` in a
 *   repo checkout, else a `dusrelimage` on PATH.
 * - The documented derived fixtures (`solid_rgb`,
 *   `transparent_rgba`, `two_colour`, `tiff`, `jpeg`, `avif`,
 *   `garbage`, `empty`) are built here with a minimal pure-PHP
 *   PNG/TIFF encoder (stdlib `zlib` only — no GD, no Pillow), and
 *   the `jpeg` / `avif` fixtures are derived through the public PHP
 *   wrapper, exactly like the JS/Python runners do.
 *
 * It FAILS LOUDLY on any fixture kind it cannot handle: unknown
 * kinds raise naming the case id instead of silently skipping.
 */
final class Fixtures
{
    /** @var array<string, string> mock cache: "kind/w/h/seed" -> bytes */
    private static array $mockCache = [];

    /** Locate the `dusrelimage` CLI binary. */
    public static function cliPath(): string
    {
        $env = getenv('DUSRELIMAGE_CLI');
        if (is_string($env) && $env !== '' && is_file($env) && is_executable($env)) {
            return $env;
        }
        // tests/ -> php/ -> packages/ -> repo root.
        $repo = dirname(__DIR__, 3);
        $candidate = $repo . '/target/release/dusrelimage';
        if (is_file($candidate) && is_executable($candidate)) {
            return $candidate;
        }
        $onPath = trim((string) shell_exec('command -v dusrelimage'));
        if ($onPath !== '' && is_executable($onPath)) {
            return $onPath;
        }
        throw new \RuntimeException(
            'The dusrelimage CLI binary was not found. Build it with '
            . 'packages/php/scripts/build-ffi.sh (cargo build -p dusrelimage-cli --release), '
            . 'or set DUSRELIMAGE_CLI to the built binary. Tried: '
            . ($env !== '' ? "{$env}, " : '') . "{$candidate}, dusrelimage on PATH.",
        );
    }

    /**
     * One deterministic mock image from the CLI (`solid` / `gradient`
     * / `noise`), cached per kind/size/seed for the whole run.
     */
    public static function mock(string $kind, int $width, int $height, int $seed): string
    {
        $key = "{$kind}/{$width}/{$height}/{$seed}";
        if (isset(self::$mockCache[$key])) {
            return self::$mockCache[$key];
        }
        $dir = sys_get_temp_dir() . '/dusrelimage-php-fixtures';
        if (!is_dir($dir) && !mkdir($dir, 0777, true) && !is_dir($dir)) {
            throw new \RuntimeException("Could not create fixture dir {$dir}.");
        }
        $file = $dir . '/' . md5($key) . '.png';
        if (!is_file($file)) {
            $cmd = sprintf(
                '%s mock --output %s --kind %s --width %d --height %d --seed %d 2>&1',
                escapeshellarg(self::cliPath()),
                escapeshellarg($file),
                escapeshellarg($kind),
                $width,
                $height,
                $seed,
            );
            $output = [];
            $code = 0;
            exec($cmd, $output, $code);
            if ($code !== 0 || !is_file($file)) {
                throw new \RuntimeException(
                    "dusrelimage mock failed (exit {$code}): " . implode("\n", $output),
                );
            }
        }
        $bytes = file_get_contents($file);
        if ($bytes === false || $bytes === '') {
            throw new \RuntimeException("Mock fixture {$file} is empty.");
        }
        self::$mockCache[$key] = $bytes;
        return $bytes;
    }

    // ------------------------------------------------------------------
    // Minimal PNG encoder / decoder (test fixtures only; zlib stdlib)
    // ------------------------------------------------------------------

    private static function pngChunk(string $type, string $data): string
    {
        // CRC-32 as 4 raw big-endian bytes (hash('crc32b') is
        // endian-safe, unlike crc32() on 32-bit builds).
        return pack('N', strlen($data)) . $type . $data . hash('crc32b', $type . $data, true);
    }

    /**
     * @param callable(int, int): array{0: int, 1: int, 2: int, 3?: int} $pixel
     */
    public static function encodePng(int $width, int $height, callable $pixel, bool $rgba = false): string
    {
        $channels = $rgba ? 4 : 3;
        $raw = '';
        for ($y = 0; $y < $height; $y++) {
            $raw .= "\x00"; // filter: none
            for ($x = 0; $x < $width; $x++) {
                $px = $pixel($x, $y);
                for ($c = 0; $c < $channels; $c++) {
                    $raw .= chr($px[$c]);
                }
            }
        }
        $ihdr = pack('NNCCCCC', $width, $height, 8, $rgba ? 6 : 2, 0, 0, 0);
        return "\x89PNG\r\n\x1a\n"
            . self::pngChunk('IHDR', $ihdr)
            . self::pngChunk('IDAT', (string) gzcompress($raw, 6))
            . self::pngChunk('IEND', '');
    }

    private static function paeth(int $a, int $b, int $c): int
    {
        $p = $a + $b - $c;
        $pa = abs($p - $a);
        $pb = abs($p - $b);
        $pc = abs($p - $c);
        if ($pa <= $pb && $pa <= $pc) {
            return $a;
        }
        if ($pb <= $pc) {
            return $b;
        }
        return $c;
    }

    /**
     * Decode a PNG (colour types 2 RGB / 6 RGBA, bit depth 8, filters
     * 0-4) into rows of [r, g, b, a].
     *
     * @return array{width: int, height: int, rows: list<list<array{0: int, 1: int, 2: int, 3: int}>>}
     */
    public static function decodePng(string $data): array
    {
        if (substr($data, 0, 8) !== "\x89PNG\r\n\x1a\n") {
            throw new \RuntimeException('not a PNG');
        }
        $pos = 8;
        $idat = '';
        $width = $height = $colourType = 0;
        $len = strlen($data);
        while ($pos < $len) {
            $chunkLen = unpack('N', substr($data, $pos, 4))[1];
            $type = substr($data, $pos + 4, 4);
            $chunk = substr($data, $pos + 8, $chunkLen);
            if ($type === 'IHDR') {
                $parts = unpack('Nwidth/Nheight/Cbit_depth/Ccolour_type', $chunk);
                $width = $parts['width'];
                $height = $parts['height'];
                if ($parts['bit_depth'] !== 8) {
                    throw new \RuntimeException("unsupported PNG bit depth {$parts['bit_depth']}");
                }
                $colourType = $parts['colour_type'];
                if ($colourType !== 2 && $colourType !== 6) {
                    throw new \RuntimeException("unsupported PNG colour type {$colourType}");
                }
            } elseif ($type === 'IDAT') {
                $idat .= $chunk;
            } elseif ($type === 'IEND') {
                break;
            }
            $pos += 12 + $chunkLen;
        }
        $channels = $colourType === 2 ? 3 : 4;
        $stride = $width * $channels;
        $raw = gzuncompress($idat);
        if ($raw === false) {
            throw new \RuntimeException('PNG IDAT zlib data is corrupt.');
        }
        $rows = [];
        $prev = array_fill(0, $stride, 0);
        $p = 0;
        for ($y = 0; $y < $height; $y++) {
            $f = ord($raw[$p]);
            $p += 1;
            $line = array_values(unpack('C*', substr($raw, $p, $stride)));
            $p += $stride;
            for ($i = 0; $i < $stride; $i++) {
                $a = $i >= $channels ? $line[$i - $channels] : 0;
                $b = $prev[$i];
                $c = $i >= $channels ? $prev[$i - $channels] : 0;
                if ($f === 1) {
                    $line[$i] = ($line[$i] + $a) & 0xFF;
                } elseif ($f === 2) {
                    $line[$i] = ($line[$i] + $b) & 0xFF;
                } elseif ($f === 3) {
                    $line[$i] = ($line[$i] + (($a + $b) >> 1)) & 0xFF;
                } elseif ($f === 4) {
                    $line[$i] = ($line[$i] + self::paeth($a, $b, $c)) & 0xFF;
                } elseif ($f !== 0) {
                    throw new \RuntimeException("unsupported PNG filter {$f}");
                }
            }
            $prev = $line;
            $row = [];
            for ($x = 0; $x < $width; $x++) {
                $row[] = [
                    $line[$x * $channels],
                    $line[$x * $channels + 1],
                    $line[$x * $channels + 2],
                    $channels === 4 ? $line[$x * $channels + 3] : 255,
                ];
            }
            $rows[] = $row;
        }
        return ['width' => $width, 'height' => $height, 'rows' => $rows];
    }

    /** Minimal uncompressed little-endian TIFF, RGB 8-bit. */
    public static function encodeTiff(int $width, int $height, array $rgb): string
    {
        $bitsOffset = 8 + 2 + 10 * 12 + 4;
        $dataOffset = $bitsOffset + 6;
        $pixels = str_repeat(chr($rgb[0]) . chr($rgb[1]) . chr($rgb[2]), $width * $height);
        $entry = static fn(int $tag, int $type, int $count, int $value): string =>
            pack('vvVV', $tag, $type, $count, $value);
        $ifd = $entry(256, 4, 1, $width)
            . $entry(257, 4, 1, $height)
            . $entry(258, 3, 3, $bitsOffset)
            . $entry(259, 3, 1, 1)
            . $entry(262, 3, 1, 2)
            . $entry(273, 4, 1, $dataOffset)
            . $entry(277, 3, 1, 3)
            . $entry(278, 4, 1, $height)
            . $entry(279, 4, 1, strlen($pixels))
            . $entry(284, 3, 1, 1);
        return "II*\x00"
            . pack('V', 8)
            . pack('v', 10)
            . $ifd
            . pack('V', 0)
            . pack('vvv', 8, 8, 8)
            . $pixels;
    }

    // ------------------------------------------------------------------
    // Fixture resolution (mirrors the JS/Python runners)
    // ------------------------------------------------------------------

    private static function parseDimsFromDerived(string $text, string $caseId): array
    {
        foreach (preg_split('/[\s,]+/', $text) as $token) {
            if (preg_match('/^(\d+)x(\d+)$/', $token, $m) === 1) {
                return [(int) $m[1], (int) $m[2]];
            }
        }
        throw new \RuntimeException("case {$caseId}: no WxH dimensions in derived_from: {$text}");
    }

    private static function parseRgbFromDerived(string $text, string $caseId): array
    {
        if (preg_match('/\((\d+),\s*(\d+),\s*(\d+)\)/', $text, $m) !== 1) {
            throw new \RuntimeException("case {$caseId}: no rgb tuple in derived_from: {$text}");
        }
        return [(int) $m[1], (int) $m[2], (int) $m[3]];
    }

    /**
     * Build one fixture's bytes. `$di` is needed for the derived
     * `jpeg` / `avif` fixtures (built through the public wrapper).
     *
     * @param array $fixture One `fixture` / `fixtures[]` entry.
     */
    public static function fixtureBytes(array $fixture, string $caseId, ?Dusrelimage $di = null): string
    {
        $kind = $fixture['kind'] ?? null;
        if ($kind === 'solid' || $kind === 'gradient' || $kind === 'noise') {
            return self::mock($kind, (int) $fixture['width'], (int) $fixture['height'], (int) $fixture['seed']);
        }
        if ($kind === 'solid_rgb') {
            [$r, $g, $b] = $fixture['rgb'];
            return self::encodePng((int) $fixture['width'], (int) $fixture['height'], static fn(int $x, int $y) => [$r, $g, $b, 255]);
        }
        if ($kind === 'transparent_rgba') {
            return self::encodePng((int) $fixture['width'], (int) $fixture['height'], static fn(int $x, int $y) => [0, 0, 0, 0], true);
        }
        if ($kind === 'two_colour') {
            // Left half red, right half blue, 50/50 by construction.
            $width = (int) $fixture['width'];
            return self::encodePng($width, (int) $fixture['height'], static fn(int $x, int $y) => $x < $width / 2 ? [255, 0, 0, 255] : [0, 0, 255, 255]);
        }
        if ($kind === 'tiff') {
            // rgb + dimensions live only in derived_from ("rgb
            // (10,20,30) 48x32 TIFF, written with the image crate").
            $derived = (string) ($fixture['derived_from'] ?? '');
            $rgb = $fixture['rgb'] ?? self::parseRgbFromDerived($derived, $caseId);
            if (isset($fixture['width'])) {
                $width = (int) $fixture['width'];
                $height = (int) $fixture['height'];
            } else {
                [$width, $height] = self::parseDimsFromDerived($derived, $caseId);
            }
            return self::encodeTiff($width, $height, $rgb);
        }
        if ($kind === 'jpeg') {
            // Derived: compress gradient 100x80 seed 2 to JPEG q80.
            if (!str_contains((string) ($fixture['derived_from'] ?? ''), 'jpeg')) {
                throw new \RuntimeException("case {$caseId}: jpeg fixture derived_from does not mention jpeg");
            }
            if ($di === null) {
                throw new \RuntimeException("case {$caseId}: jpeg fixture needs a Dusrelimage instance");
            }
            return $di->compress(self::mock('gradient', 100, 80, 2), ['format' => 'Jpeg', 'quality' => 80]);
        }
        if ($kind === 'avif') {
            // Derived: convert gradient 64x48 seed 4 to AVIF q80.
            if (!str_contains((string) ($fixture['derived_from'] ?? ''), 'avif')) {
                throw new \RuntimeException("case {$caseId}: avif fixture derived_from does not mention avif");
            }
            if ($di === null) {
                throw new \RuntimeException("case {$caseId}: avif fixture needs a Dusrelimage instance");
            }
            return $di->convert(self::mock('gradient', 64, 48, 4), ['format' => 'Avif', 'quality' => 80]);
        }
        if ($kind === 'garbage') {
            $derived = (string) ($fixture['derived_from'] ?? '');
            if (preg_match('/^bytes \[(.*)\]$/', $derived, $m) === 1) {
                $bytes = '';
                foreach (explode(',', $m[1]) as $n) {
                    $bytes .= chr((int) trim($n));
                }
                return $bytes;
            }
            if (preg_match("/^ASCII bytes: '(.*)'$/", $derived, $m) === 1) {
                return $m[1];
            }
            throw new \RuntimeException("case {$caseId}: unhandled garbage derived_from: {$derived}");
        }
        if ($kind === 'empty') {
            return '';
        }
        throw new \RuntimeException("case {$caseId}: unknown fixture kind {$kind} — update this runner");
    }

    /**
     * Deep-copy options, replacing a watermark logo fixture dict
     * with the real logo bytes (the wrapper expects bytes, not a
     * fixture description).
     */
    public static function resolveLogo(array $options, string $caseId, ?Dusrelimage $di = null): array
    {
        if (!isset($options['logo']) || !is_array($options['logo'])) {
            return $options;
        }
        $options = json_decode((string) json_encode($options), true);
        $logo = $options['logo'];
        if (is_array($logo) && isset($logo['image']) && is_array($logo['image'])) {
            $logo['image'] = self::fixtureBytes($logo['image'], $caseId, $di);
            $options['logo'] = $logo;
        }
        return $options;
    }
}
