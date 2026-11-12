<?php

declare(strict_types=1);

namespace Dusrelimage\Tests;

use Dusrelimage\DusrelError;
use Dusrelimage\Dusrelimage;
use PHPUnit\Framework\Attributes\DataProvider;
use PHPUnit\Framework\TestCase;

/**
 * Shared test vectors (SPEC section 6): Rust, Python, JS and PHP all
 * run `shared/test-vectors.json` and must agree.
 *
 * This runner loads the JSON (path relative to this file), builds
 * each fixture via Fixtures (the `dusrelimage mock` CLI kinds plus
 * the documented derived fixtures), runs the op through the public
 * PHP wrapper, and asserts the expected dimensions / pixel
 * spot-checks / error code.
 *
 * It FAILS LOUDLY on any case whose fixture/op kind it cannot
 * handle: unknown fixture kinds, ops, bulk ops or expected keys
 * fail naming the case id instead of silently skipping the case.
 * Pixel checks decode PNG output with the pure-PHP PNG decoder in
 * Fixtures; non-PNG outputs fall back to `pickColour` per pixel
 * (only the JPEG flatten case needs that).
 */
final class VectorsTest extends TestCase
{
    private static Dusrelimage $di;

    /** @var list<array> */
    private static array $cases = [];

    private const KNOWN_EXPECTED_KEYS = [
        'format', 'width', 'height', 'has_alpha', 'colour_type', 'error',
        'max_bytes', 'avif_header', 'pixels', 'region_all_white',
        'region_has_dark', 'regions_have_dark', 'colour', 'different_from',
        'palette_hexes', 'palette_length', 'share_each', 'share_sum',
        'share_sum_tolerance', 'share_tolerance', 'palette',
        'differs_from_input', 'pixel_equalities', 'unchanged_pixels',
        'results',
    ];

    public static function setUpBeforeClass(): void
    {
        parent::setUpBeforeClass();
        self::$di = new Dusrelimage();
        self::loadCases();
    }

    /** @return list<array> */
    private static function loadCases(): array
    {
        if (self::$cases !== []) {
            return self::$cases;
        }
        $path = __DIR__ . '/../../../shared/test-vectors.json';
        if (!is_file($path)) {
            throw new \RuntimeException("shared/test-vectors.json not found at {$path}");
        }
        $decoded = json_decode((string) file_get_contents($path), true, 512, JSON_THROW_ON_ERROR);
        if (!is_array($decoded)) {
            throw new \RuntimeException('shared/test-vectors.json did not decode to a list.');
        }
        self::$cases = array_values($decoded);
        return self::$cases;
    }

    /** @return array<string, array{0: array}> */
    public static function casesProvider(): array
    {
        $out = [];
        foreach (self::loadCases() as $case) {
            $out[(string) $case['id']] = [$case];
        }
        return $out;
    }

    public function testVectorCount(): void
    {
        self::assertCount(82, self::loadCases(), 'shared/test-vectors.json case count changed — update this runner');
    }

    #[DataProvider('casesProvider')]
    public function testVector(array $case): void
    {
        $caseId = (string) $case['id'];
        $failures = [];
        $outcome = null;
        $error = null;
        $input = null;
        try {
            [$outcome, $input] = $this->runCase($case, $caseId);
        } catch (DusrelError $e) {
            $error = $e;
            // The input is only needed for pixel comparisons on success.
            if (isset($case['fixture'])) {
                $input = Fixtures::fixtureBytes($case['fixture'], $caseId, self::$di);
            }
        }
        self::assertExpected($case, $caseId, $outcome, $error, $input, $failures);
        self::assertSame([], $failures, implode("\n", $failures));
    }

    /**
     * @return array{0: array|null, 1: string|null} [outcome, input bytes]
     */
    private function runCase(array $case, string $caseId): array
    {
        $di = self::$di;
        $options = $case['options'] ?? [];
        $inputs = [];
        $input = null;
        if (isset($case['fixtures'])) {
            foreach ($case['fixtures'] as $f) {
                $inputs[] = Fixtures::fixtureBytes($f, $caseId, $di);
            }
            $input = $inputs[0] ?? null;
        } elseif (isset($case['fixture'])) {
            $input = Fixtures::fixtureBytes($case['fixture'], $caseId, $di);
            $inputs = [$input];
        }
        $needImage = static function () use ($input, $caseId): string {
            if ($input === null) {
                throw new \RuntimeException("case {$caseId}: op needs one fixture");
            }
            return $input;
        };
        switch ($case['op']) {
            case 'get_image_info':
                return [['kind' => 'info', 'info' => $di->getImageInfo($needImage(), $options)], $input];
            case 'compress_image':
                return [['kind' => 'image', 'bytes' => $di->compress($needImage(), $options)], $input];
            case 'compress_to_size':
                return [['kind' => 'image', 'bytes' => $di->compressToSize($needImage(), (int) $case['target_bytes'], $options)], $input];
            case 'resize_image':
                return [['kind' => 'image', 'bytes' => $di->resize($needImage(), $options)], $input];
            case 'crop_image':
                return [['kind' => 'image', 'bytes' => $di->crop($needImage(), $options)], $input];
            case 'convert_image':
                return [['kind' => 'image', 'bytes' => $di->convert($needImage(), $options)], $input];
            case 'add_watermark':
                return [['kind' => 'image', 'bytes' => $di->addWatermark($needImage(), Fixtures::resolveLogo($options, $caseId, $di))], $input];
            case 'annotate_image':
                return [['kind' => 'image', 'bytes' => $di->annotate($needImage(), $case['annotations'])], $input];
            case 'pick_colour':
                return [['kind' => 'colour', 'colour' => $di->pickColour($needImage(), (int) $options['x'], (int) $options['y'])], $input];
            case 'extract_palette':
                return [['kind' => 'palette', 'palette' => $di->extractPalette($needImage(), (int) $options['count'])], $input];
            case 'make_collage':
                return [['kind' => 'image', 'bytes' => $di->makeCollage($inputs, $options)], null];
            case 'pixelate_image':
                return [['kind' => 'image', 'bytes' => $di->pixelate($needImage(), $options)], $input];
            case 'bulk_process': {
                $map = [
                    'Pixelate' => 'pixelate',
                    'PickColour' => 'pick_colour',
                    'Palette' => 'palette',
                    'Collage' => 'collage',
                    'Watermark' => 'watermark',
                    'Annotate' => 'annotate',
                ];
                $bulkOp = (string) $case['bulk_op'];
                if (!isset($map[$bulkOp])) {
                    throw new \RuntimeException("case {$caseId}: unknown bulk_op {$bulkOp} — update this runner");
                }
                $bulkOptions = match ($bulkOp) {
                    'PickColour' => ['x' => $options['x'] ?? 0, 'y' => $options['y'] ?? 0],
                    'Palette' => ['count' => $options['count'] ?? 5],
                    'Watermark' => Fixtures::resolveLogo($options, $caseId, $di),
                    'Annotate' => ['annotations' => $case['annotations']],
                    default => $options,
                };
                return [['kind' => 'bulk', 'results' => $di->bulkProcess($inputs, $map[$bulkOp], $bulkOptions)], null];
            }
            default:
                throw new \RuntimeException("case {$caseId}: unknown op {$case['op']} — update this runner");
        }
    }

    /**
     * @param array|null $outcome
     * @param string|null $input
     * @param list<string> $failures
     */
    private static function assertExpected(array $case, string $caseId, ?array $outcome, ?DusrelError $error, ?string $input, array &$failures): void
    {
        $expected = $case['expected'];
        foreach (array_keys($expected) as $key) {
            if (!in_array($key, self::KNOWN_EXPECTED_KEYS, true)) {
                $failures[] = "case {$caseId}: unhandled expected key {$key} — update this runner";
            }
        }
        if (isset($expected['error'])) {
            if ($error === null) {
                $failures[] = "case {$caseId}: expected error {$expected['error']}, got success";
            } elseif ($error->code !== $expected['error']) {
                $failures[] = "case {$caseId}: expected error {$expected['error']}, got {$error->code}";
            }
            return;
        }
        if ($error !== null) {
            $failures[] = "case {$caseId}: expected success, got {$error->code} ({$error->messageEn})";
            return;
        }
        if ($outcome === null) {
            $failures[] = "case {$caseId}: no outcome and no error";
            return;
        }

        $info = null;
        if ($outcome['kind'] === 'info') {
            $info = $outcome['info'];
        } elseif ($outcome['kind'] === 'image') {
            // AVIF output cannot be decoded back in this build (no
            // avif-native decoder): a case that only asserts the
            // header/size may skip the decode; anything asserting
            // dims must be decodable.
            try {
                $info = self::$di->getImageInfo($outcome['bytes']);
            } catch (DusrelError $e) {
                if (count(array_intersect(['format', 'width', 'height', 'has_alpha', 'colour_type'], array_keys($expected))) > 0) {
                    throw $e;
                }
                $info = null;
            }
        }
        if ($info !== null) {
            foreach (['format' => 'format', 'width' => 'width', 'height' => 'height', 'has_alpha' => 'has_alpha', 'colour_type' => 'colour_type'] as $key => $field) {
                if (isset($expected[$key]) && $info[$field] !== $expected[$key]) {
                    $failures[] = "case {$caseId}: expected {$key} {$expected[$key]}, got {$info[$field]}";
                }
            }
        }

        if ($outcome['kind'] === 'image') {
            $bytes = $outcome['bytes'];
            if (isset($expected['max_bytes']) && strlen($bytes) > $expected['max_bytes']) {
                $failures[] = "case {$caseId}: expected <= {$expected['max_bytes']} bytes, got " . strlen($bytes);
            }
            if (($expected['avif_header'] ?? false) === true) {
                if (strlen($bytes) < 12 || substr($bytes, 4, 8) !== 'ftypavif') {
                    $failures[] = "case {$caseId}: expected AVIF ftypavif header";
                }
            }
            $pixels = new VectorsPixels($bytes, $caseId, self::$di);
            $inputPixels = $input !== null ? new VectorsPixels($input, $caseId, self::$di) : null;
            foreach ($expected['pixels'] ?? [] as $spec) {
                self::checkPixelAssert($pixels->pixel((int) $spec['x'], (int) $spec['y']), $spec, $caseId, $failures);
            }
            foreach (['region_all_white' => 'regionAllWhite', 'region_has_dark' => 'regionHasDark'] as $key => $method) {
                if (isset($expected[$key])) {
                    $r = $expected[$key];
                    if (!$pixels->$method((int) $r['x0'], (int) $r['y0'], (int) $r['x1'], (int) $r['y1'])) {
                        $failures[] = "case {$caseId}: region ({$r['x0']},{$r['y0']})-({$r['x1']},{$r['y1']}) failed {$key}";
                    }
                }
            }
            foreach ($expected['regions_have_dark'] ?? [] as $r) {
                if (!$pixels->regionHasDark((int) $r['x0'], (int) $r['y0'], (int) $r['x1'], (int) $r['y1'])) {
                    $failures[] = "case {$caseId}: region ({$r['x0']},{$r['y0']})-({$r['x1']},{$r['y1']}) should contain dark pixels";
                }
            }
            if ($inputPixels !== null) {
                foreach ($expected['differs_from_input'] ?? [] as [$x, $y]) {
                    if ($pixels->pixel((int) $x, (int) $y) === $inputPixels->pixel((int) $x, (int) $y)) {
                        $failures[] = "case {$caseId}: pixel ({$x},{$y}) should differ from the input";
                    }
                }
                foreach ($expected['pixel_equalities'] ?? [] as $pair) {
                    [$ax, $ay] = $pair['a'];
                    [$bx, $by] = $pair['b'];
                    $same = $pixels->pixel((int) $ax, (int) $ay) === $pixels->pixel((int) $bx, (int) $by);
                    if ($same !== $pair['equal']) {
                        $failures[] = "case {$caseId}: pixels ({$ax},{$ay}) vs ({$bx},{$by}) equal=" . var_export($same, true) . ", expected " . var_export($pair['equal'], true);
                    }
                }
                foreach ($expected['unchanged_pixels'] ?? [] as [$x, $y]) {
                    if ($pixels->pixel((int) $x, (int) $y) !== $inputPixels->pixel((int) $x, (int) $y)) {
                        $failures[] = "case {$caseId}: pixel ({$x},{$y}) should be unchanged from the input";
                    }
                }
            }
        }

        if ($outcome['kind'] === 'colour') {
            $colour = $outcome['colour'];
            if (isset($expected['colour'])) {
                $want = $expected['colour'];
                foreach (['hex', 'r', 'g', 'b', 'a'] as $key) {
                    if (isset($want[$key]) && $colour[$key] !== $want[$key]) {
                        $failures[] = "case {$caseId}: expected colour {$key}={$want[$key]}, got {$colour[$key]}";
                    }
                }
            }
            if (isset($expected['different_from']) && $input !== null) {
                $other = $expected['different_from'];
                $otherColour = self::$di->pickColour($input, (int) $other['x'], (int) $other['y']);
                if ($otherColour['hex'] === $colour['hex']) {
                    $failures[] = "case {$caseId}: colour should differ from the pixel at ({$other['x']},{$other['y']})";
                }
            }
        }

        if ($outcome['kind'] === 'palette') {
            $palette = $outcome['palette'];
            if (isset($expected['palette_length']) && count($palette) !== $expected['palette_length']) {
                $failures[] = "case {$caseId}: expected palette length {$expected['palette_length']}, got " . count($palette);
            }
            if (isset($expected['palette_hexes'])) {
                // Compared as a set: the contract orders by frequency
                // ("most frequent first") and these shares are equal,
                // so the tie order is unspecified — the core breaks
                // ties by hex ascending. Shares are asserted separately.
                $got = array_map(static fn(array $p): string => $p['hex'], $palette);
                sort($got);
                $want = $expected['palette_hexes'];
                sort($want);
                if ($got !== $want) {
                    $failures[] = "case {$caseId}: expected palette hexes " . implode(',', $want) . ', got ' . implode(',', $got);
                }
            }
            $shareTol = $expected['share_tolerance'] ?? 0.0;
            if (isset($expected['share_each'])) {
                foreach ($palette as $entry) {
                    if (abs($entry['share'] - $expected['share_each']) > $shareTol) {
                        $failures[] = "case {$caseId}: palette {$entry['hex']} share expected {$expected['share_each']}±{$shareTol}, got {$entry['share']}";
                    }
                }
            }
            if (isset($expected['share_sum'])) {
                $sum = array_sum(array_column($palette, 'share'));
                if (abs($sum - $expected['share_sum']) > ($expected['share_sum_tolerance'] ?? 0.0)) {
                    $failures[] = "case {$caseId}: palette share sum expected {$expected['share_sum']}, got {$sum}";
                }
            }
            if (isset($expected['palette'])) {
                $want = $expected['palette'];
                if (count($want) !== count($palette)) {
                    $failures[] = "case {$caseId}: expected palette of " . count($want) . ', got ' . count($palette);
                } else {
                    foreach ($want as $i => $w) {
                        if (isset($w['hex']) && $palette[$i]['hex'] !== $w['hex']) {
                            $failures[] = "case {$caseId}: expected palette hex {$w['hex']}, got {$palette[$i]['hex']}";
                        }
                        if (isset($w['share']) && abs($palette[$i]['share'] - $w['share']) > ($w['share_tolerance'] ?? 0.0)) {
                            $failures[] = "case {$caseId}: palette {$palette[$i]['hex']} share expected {$w['share']}, got {$palette[$i]['share']}";
                        }
                    }
                }
            }
        }

        if ($outcome['kind'] === 'bulk') {
            $results = $outcome['results'];
            $want = $expected['results'] ?? [];
            if (count($results) !== count($want)) {
                $failures[] = "case {$caseId}: expected " . count($want) . ' bulk results, got ' . count($results);
                return;
            }
            foreach ($results as $i => $got) {
                $wantItem = $want[$i];
                $idx = $wantItem['index'] ?? $got['index'];
                if ($got['index'] !== $idx) {
                    $failures[] = "case {$caseId}: bulk item index expected {$idx}, got {$got['index']}";
                }
                $wantOk = $wantItem['ok'] ?? true;
                if ($got['ok'] !== $wantOk) {
                    $failures[] = "case {$caseId}: bulk item {$idx} ok expected " . var_export($wantOk, true) . ', got ' . var_export($got['ok'], true);
                    continue;
                }
                if (!$got['ok']) {
                    if (isset($wantItem['error']) && ($got['error']['code'] ?? null) !== $wantItem['error']) {
                        $failures[] = "case {$caseId}: bulk item {$idx} error expected {$wantItem['error']}, got " . ($got['error']['code'] ?? 'none');
                    }
                    continue;
                }
                if (($wantItem['output'] ?? null) === 'Image') {
                    if (!isset($got['output']) || !is_string($got['output'])) {
                        $failures[] = "case {$caseId}: bulk item {$idx} output expected Image bytes";
                        continue;
                    }
                    $outInfo = self::$di->getImageInfo($got['output']);
                    foreach (['width', 'height'] as $key) {
                        if (isset($wantItem[$key]) && $outInfo[$key] !== $wantItem[$key]) {
                            $failures[] = "case {$caseId}: bulk item {$idx} {$key} expected {$wantItem[$key]}, got {$outInfo[$key]}";
                        }
                    }
                } elseif (($wantItem['output'] ?? null) === 'Colour') {
                    if (isset($wantItem['colour']) && ($got['colour']['hex'] ?? null) !== $wantItem['colour']['hex']) {
                        $failures[] = "case {$caseId}: bulk item {$idx} colour hex expected {$wantItem['colour']['hex']}, got " . ($got['colour']['hex'] ?? 'none');
                    }
                } elseif (($wantItem['output'] ?? null) === 'Palette') {
                    if (isset($wantItem['palette_length']) && count($got['palette'] ?? []) !== $wantItem['palette_length']) {
                        $failures[] = "case {$caseId}: bulk item {$idx} palette length expected {$wantItem['palette_length']}, got " . count($got['palette'] ?? []);
                    }
                    foreach ($wantItem['palette'] ?? [] as $j => $w) {
                        if (isset($w['hex']) && ($got['palette'][$j]['hex'] ?? null) !== $w['hex']) {
                            $failures[] = "case {$caseId}: bulk item {$idx} palette hex expected {$w['hex']}, got " . ($got['palette'][$j]['hex'] ?? 'none');
                        }
                    }
                }
            }
        }
    }

    /**
     * @param array{0: int, 1: int, 2: int, 3: int} $px
     * @param list<string> $failures
     */
    private static function checkPixelAssert(array $px, array $spec, string $caseId, array &$failures): void
    {
        $x = $spec['x'];
        $y = $spec['y'];
        if (isset($spec['rgba'])) {
            if ($px !== array_values($spec['rgba'])) {
                $failures[] = "case {$caseId}: pixel ({$x},{$y}) expected rgba " . implode(',', $spec['rgba']) . ', got ' . implode(',', $px);
            }
        }
        if (isset($spec['rgba_approx'])) {
            $tol = $spec['tolerance'] ?? 0;
            foreach (array_values($spec['rgba_approx']) as $i => $want) {
                if (abs($px[$i] - $want) > $tol) {
                    $failures[] = "case {$caseId}: pixel ({$x},{$y}) channel {$i} expected ~{$want}±{$tol}, got {$px[$i]}";
                }
            }
        }
        foreach (['r' => 0, 'g' => 1, 'b' => 2, 'a' => 3] as $key => $ch) {
            if (isset($spec[$key]) && $px[$ch] !== $spec[$key]) {
                $failures[] = "case {$caseId}: pixel ({$x},{$y}) channel {$key} expected {$spec[$key]}, got {$px[$ch]}";
            }
        }
        if (isset($spec['r_max']) && $px[0] > $spec['r_max']) {
            $failures[] = "case {$caseId}: pixel ({$x},{$y}) r expected <= {$spec['r_max']}, got {$px[0]}";
        }
    }
}

/**
 * Pixel access for an image output: PNG outputs decode with the
 * pure-PHP decoder in Fixtures; anything else goes through
 * pick_colour (only the JPEG flatten case needs that).
 */
final class VectorsPixels
{
    /** @var list<list<array{0: int, 1: int, 2: int, 3: int}>>|null */
    private ?array $rows;

    public function __construct(
        private readonly string $data,
        private readonly string $caseId,
        private readonly Dusrelimage $di,
    ) {
        $this->rows = null;
        if (strlen($data) >= 8 && substr($data, 0, 8) === "\x89PNG\r\n\x1a\n") {
            $this->rows = Fixtures::decodePng($data)['rows'];
        }
    }

    /** @return array{0: int, 1: int, 2: int, 3: int} */
    public function pixel(int $x, int $y): array
    {
        if ($this->rows !== null) {
            return $this->rows[$y][$x];
        }
        $c = $this->di->pickColour($this->data, $x, $y);
        return [$c['r'], $c['g'], $c['b'], $c['a']];
    }

    public function regionHasDark(int $x0, int $y0, int $x1, int $y1): bool
    {
        if ($this->rows === null) {
            throw new \RuntimeException("case {$this->caseId}: region check needs a PNG output");
        }
        for ($y = $y0; $y < $y1; $y++) {
            for ($x = $x0; $x < $x1; $x++) {
                $px = $this->rows[$y][$x];
                if ($px[0] < 128 || $px[1] < 128 || $px[2] < 128) {
                    return true;
                }
            }
        }
        return false;
    }

    public function regionAllWhite(int $x0, int $y0, int $x1, int $y1): bool
    {
        if ($this->rows === null) {
            throw new \RuntimeException("case {$this->caseId}: region check needs a PNG output");
        }
        for ($y = $y0; $y < $y1; $y++) {
            for ($x = $x0; $x < $x1; $x++) {
                $px = $this->rows[$y][$x];
                if ($px[0] !== 255 || $px[1] !== 255 || $px[2] !== 255) {
                    return false;
                }
            }
        }
        return true;
    }
}
