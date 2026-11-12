<?php

declare(strict_types=1);

namespace Dusrelimage;

/**
 * dusrelimage — offline image toolkit (PHP FFI wrapper over the Rust core).
 *
 * The image work lives in the Rust core (`crates/dusrelimage`), reached
 * here through the C API in `crates/dusrelimage-ffi`. Every operation
 * takes bytes in and returns bytes out, synchronously — the FFI calls
 * are synchronous, so there is nothing to `await`.
 *
 * FFI contract (see `crates/dusrelimage-ffi`; the whole C declaration
 * lives in the single `CDEF` constant below, so a name fix is a
 * one-line change):
 * - Image functions return a status int (`0` = OK, otherwise one of
 *   the `STATUS_CODES` below) and write their output to `*out` /
 *   `*out_len`. The wrapper reads the output bytes, frees the buffer
 *   with `dusrelimage_free_buffer`, and returns the bytes as a PHP
 *   string. On a non-zero status it reads `dusrelimage_last_error()`
 *   (a JSON string `{"code","message_en","message_hi"}`) and throws
 *   a `DusrelError` — never a raw FFI error, never a raw JSON string.
 * - `get_image_info`, `pick_colour` and `extract_palette` return JSON
 *   in the output buffer; this wrapper decodes it into PHP arrays.
 *   `get_image_info` takes a `CommonOptions` JSON `options_json`
 *   (nested `common` object and/or flat common fields), like the
 *   other ops — its `$options` are sent, not ignored.
 *   Palette entries are returned FLATTENED
 *   (`{hex, r, g, b, a, hsl, share}`), like the JS/Python wrappers,
 *   even if the FFI layer nests them as `{colour: {...}, share}`.
 * - `bulk_process` returns a JSON string of bulk results whose image
 *   outputs are base64; this wrapper decodes them to strings.
 * - Options are plain PHP arrays with snake_case keys matching the
 *   Rust `*Options` serde names (see `SPEC.md` section 2 and
 *   `crates/dusrelimage/src/options.rs`). This wrapper ALWAYS sends
 *   complete objects: every field is filled with the Rust default,
 *   because serde rejects partial objects for non-`Option` fields
 *   (the JS wrapper's complete-object approach). `common` fields may
 *   be passed nested under a `"common"` key or flat at the top level
 *   of the options array (nested wins); the Python native layer
 *   accepts the same two spellings.
 * - Colours are `"#RRGGBB"` hex strings (or `"#RGB"` / `"#RRGGBBAA"`)
 *   or full `Colour` arrays; the wrapper expands hex strings to full
 *   `Colour` arrays (with HSL) before sending them. Enums
 *   (`format`, `fit`, `filter`, `position`, layout) accept the Rust
 *   serde spelling (`"Jpeg"`, `"Fit"`, `"Lanczos3"`, `"BottomRight"`,
 *   `"2x2"`) or a lowercase spelling (`"jpeg"`, `"fit"`,
 *   `"lanczos3"`, `"bottom-right"`, `"row"`); both are normalised to
 *   the serde spelling.
 * - Annotations use the `{"type": "Rect", ...}` form (like the JS
 *   wrapper and `shared/test-vectors.json`), translated here to
 *   serde's externally-tagged form (`{"Rect": {...}}`).
 *
 * Input rule (the dusrelpdf lesson, applied to bytes): functions
 * accept PHP strings only. ANY non-string input (int, array, null,
 * object) throws a `DusrelError` with code `INVALID_TYPE` — never a
 * raw `TypeError`. Empty strings throw `EMPTY_INPUT` (client-side,
 * like the JS wrapper).
 *
 * Honest limits (same as the Rust core, v0.1.0): AVIF input does NOT
 * decode (`UNSUPPORTED_FORMAT`; AVIF output works, lossy 8-bit).
 * WebP output is always lossless — quality/effort are ignored for
 * WebP. `keep_exif` / `dpi` / `progressive` are accepted but not
 * honoured. `compress_to_size` quality search is JPEG/AVIF-only
 * (other formats take one best-effort encode). TIFF input decodes
 * but falls back to PNG output. GIF output is the first frame only.
 * No HEIC, no streaming (the whole image is in memory).
 *
 * The native library is NOT bundled with this package (see
 * README.md): build it with `scripts/build-ffi.sh` first. The
 * library path resolves in this order: explicit constructor
 * argument, the `DUSRELIMAGE_FFI_LIB` environment variable,
 * `packages/php/lib/libdusrelimage_ffi.so`, then
 * `target/release/libdusrelimage_ffi.so` in a repo checkout.
 */
final class Dusrelimage
{
    /**
     * The whole FFI contract in one place. `uint8_t`, `int32_t`,
     * `uint32_t`, `uint64_t` and `size_t` are PHP FFI's predefined
     * stdint types — no typedefs needed.
     *
     * Shape of the image functions:
     * `int32_t fn(const uint8_t *input, size_t input_len,
     * const char *options_json, uint8_t **out, size_t *out_len)`
     * with these exceptions:
     * - `compress_to_size` adds `uint64_t target_bytes` before
     *   `options_json`;
     * - `annotate` takes `const char *annotations_json` instead of
     *   `options_json`;
     * - `pick_colour` takes `uint32_t x, uint32_t y` instead of
     *   `options_json` and returns the colour as JSON;
     * - `extract_palette` takes `uint8_t count` instead of
     *   `options_json` and returns the palette as JSON;
     * - `make_collage` takes pointer arrays
     *   `(const uint8_t **inputs, const size_t *input_lens,
     *   size_t input_count)` instead of a single input;
     * - `get_image_info` takes a `CommonOptions` JSON `options_json`
     *   (5-arg shape: input, input_len, options_json, out, out_len);
     * - the pixelate export is `dusrelimage_pixelate` (not
     *   `dusrelimage_pixelate_image`);
     * - `bulk_process` takes pointer arrays plus
     *   `const char *op, const char *options_json` and returns the
     *   per-item results as JSON (image outputs base64-encoded).
     */
    public const CDEF = <<<'C'
        int32_t dusrelimage_compress(const uint8_t *input, size_t input_len, const char *options_json, uint8_t **out, size_t *out_len);
        int32_t dusrelimage_compress_to_size(const uint8_t *input, size_t input_len, uint64_t target_bytes, const char *options_json, uint8_t **out, size_t *out_len);
        int32_t dusrelimage_resize(const uint8_t *input, size_t input_len, const char *options_json, uint8_t **out, size_t *out_len);
        int32_t dusrelimage_crop(const uint8_t *input, size_t input_len, const char *options_json, uint8_t **out, size_t *out_len);
        int32_t dusrelimage_convert(const uint8_t *input, size_t input_len, const char *options_json, uint8_t **out, size_t *out_len);
        int32_t dusrelimage_add_watermark(const uint8_t *input, size_t input_len, const char *options_json, uint8_t **out, size_t *out_len);
        int32_t dusrelimage_annotate(const uint8_t *input, size_t input_len, const char *annotations_json, uint8_t **out, size_t *out_len);
        int32_t dusrelimage_pick_colour(const uint8_t *input, size_t input_len, uint32_t x, uint32_t y, uint8_t **out, size_t *out_len);
        int32_t dusrelimage_extract_palette(const uint8_t *input, size_t input_len, uint8_t count, uint8_t **out, size_t *out_len);
        int32_t dusrelimage_make_collage(const uint8_t **inputs, const size_t *input_lens, size_t input_count, const char *options_json, uint8_t **out, size_t *out_len);
        int32_t dusrelimage_pixelate(const uint8_t *input, size_t input_len, const char *options_json, uint8_t **out, size_t *out_len);
        int32_t dusrelimage_get_image_info(const uint8_t *input, size_t input_len, const char *options_json, uint8_t **out, size_t *out_len);
        int32_t dusrelimage_bulk_process(const uint8_t **inputs, const size_t *input_lens, size_t input_count, const char *op, const char *options_json, uint8_t **out, size_t *out_len);
        const char *dusrelimage_last_error(void);
        void dusrelimage_free_buffer(uint8_t *ptr, size_t len);
        const char *dusrelimage_version(void);
        C;

    /**
     * FFI status int -> stable error code, verbatim from
     * crates/dusrelimage/src/error.rs (plus 99 = UNKNOWN, which has
     * no Rust `ErrorCode` variant — it means the FFI layer itself
     * failed in a way the core did not classify).
     */
    public const STATUS_CODES = [
        0 => 'OK',
        1 => 'INVALID_IMAGE',
        2 => 'UNSUPPORTED_FORMAT',
        3 => 'IMAGE_TOO_LARGE',
        4 => 'TARGET_SIZE_UNREACHABLE',
        5 => 'CROP_OUT_OF_BOUNDS',
        6 => 'PIXEL_OUT_OF_BOUNDS',
        7 => 'INVALID_QUALITY',
        8 => 'INVALID_DIMENSIONS',
        9 => 'INVALID_ARGUMENT',
        10 => 'EMPTY_INPUT',
        11 => 'INVALID_TYPE',
        99 => 'UNKNOWN',
    ];

    /** Default library file name produced by `scripts/build-ffi.sh`. */
    public const DEFAULT_LIB_NAME = 'libdusrelimage_ffi.so';

    private \FFI $ffi;

    /** FFI instances cached per library path (cdef is parsed once). */
    private static array $ffiCache = [];

    /**
     * @param string|null $libPath Path to the built shared library.
     *   When omitted, see the resolution order in the class docblock.
     *
     * @throws \RuntimeException when no library can be found or loaded.
     */
    public function __construct(?string $libPath = null)
    {
        $path = self::resolveLibPath($libPath);
        if (!isset(self::$ffiCache[$path])) {
            try {
                self::$ffiCache[$path] = \FFI::cdef(self::CDEF, $path);
            } catch (\FFI\Exception $e) {
                throw new \RuntimeException(
                    "Could not load the dusrelimage native library at {$path}: {$e->getMessage()}",
                    0,
                    $e,
                );
            }
        }
        $this->ffi = self::$ffiCache[$path];
    }

    /**
     * Resolve the native library path: explicit argument, then the
     * `DUSRELIMAGE_FFI_LIB` environment variable, then the conventional
     * locations (`packages/php/lib/` and the workspace `target/`).
     *
     * @throws \RuntimeException when no candidate file exists.
     */
    public static function resolveLibPath(?string $libPath = null): string
    {
        $candidates = [];
        if ($libPath !== null && $libPath !== '') {
            $candidates[] = $libPath;
        }
        $env = getenv('DUSRELIMAGE_FFI_LIB');
        if (is_string($env) && $env !== '') {
            $candidates[] = $env;
        }
        $candidates[] = __DIR__ . '/../lib/' . self::DEFAULT_LIB_NAME;
        $candidates[] = __DIR__ . '/../lib/libdusrelimage.so';
        // __DIR__ is packages/php/src; three levels up is the repo root.
        $candidates[] = dirname(__DIR__, 3) . '/target/release/' . self::DEFAULT_LIB_NAME;
        foreach ($candidates as $candidate) {
            if (is_file($candidate)) {
                return $candidate;
            }
        }
        throw new \RuntimeException(
            'Dusrelimage native library not found. Build it first with '
            . 'packages/php/scripts/build-ffi.sh (see packages/php/README.md), '
            . 'or set DUSRELIMAGE_FFI_LIB to the built .so path. Tried: '
            . implode(', ', $candidates),
        );
    }

    /** Crate version, e.g. `"0.1.0"` (lockstep with the Rust crate). */
    public function version(): string
    {
        $version = $this->ffi->dusrelimage_version();
        if (!is_string($version) || $version === '') {
            throw new \RuntimeException('dusrelimage_version() returned no version string.');
        }
        return $version;
    }

    // ------------------------------------------------------------------
    // FFI plumbing
    // ------------------------------------------------------------------

    /**
     * Copy PHP bytes into a C buffer kept alive by the caller.
     *
     * @return array{0: \FFI\CData, 1: int} The buffer and its length.
     */
    private function inputBuffer(string $bytes): array
    {
        $len = strlen($bytes);
        // A zero-length buffer is never passed (empty input is
        // rejected client-side), but keep a 1-byte buffer so the
        // pointer is always valid if this is ever called with one.
        $buf = \FFI::new('uint8_t[' . max($len, 1) . ']');
        if ($len > 0) {
            \FFI::memcpy($buf, $bytes, $len);
        }
        return [$buf, $len];
    }

    /**
     * Build the pointer array + lengths array for a multi-input call
     * (collage / bulk). The returned input buffers must stay alive
     * for the duration of the FFI call — the caller keeps `$bufs`.
     *
     * @param string[] $inputs
     * @return array{0: \FFI\CData, 1: \FFI\CData, 2: list<\FFI\CData>, 3: int}
     */
    private function inputArray(array $inputs): array
    {
        $count = count($inputs);
        $ptrs = \FFI::new("uint8_t*[{$count}]");
        $lens = \FFI::new("size_t[{$count}]");
        $bufs = [];
        foreach ($inputs as $i => $bytes) {
            [$buf, $len] = $this->inputBuffer($bytes);
            $bufs[] = $buf;
            $ptrs[$i] = \FFI::addr($buf[0]);
            $lens[$i] = $len;
        }
        return [$ptrs, $lens, $bufs, $count];
    }

    /** Fresh `*out` / `*out_len` slots for one FFI call. */
    private function outParams(): array
    {
        return [\FFI::new('uint8_t*[1]'), \FFI::new('size_t[1]')];
    }

    /**
     * Finish one FFI call: on status 0 read the output bytes and
     * free the buffer; on any other status free any partial output
     * and throw the error from `dusrelimage_last_error()`.
     */
    private function finish(int $status, \FFI\CData $outPtr, \FFI\CData $outLenPtr): string
    {
        $ptr = $outPtr[0];
        $len = (int) $outLenPtr[0];
        if ($status === 0) {
            if ($ptr === null) {
                return '';
            }
            $bytes = $len > 0 ? \FFI::string($ptr, $len) : '';
            $this->ffi->dusrelimage_free_buffer($ptr, $len);
            return $bytes;
        }
        if ($ptr !== null) {
            $this->ffi->dusrelimage_free_buffer($ptr, $len);
        }
        throw $this->lastErrorAsException($status);
    }

    /** Turn a non-zero FFI status into a `DusrelError` via `last_error`. */
    private function lastErrorAsException(int $status): DusrelError
    {
        $code = self::STATUS_CODES[$status] ?? 'UNKNOWN';
        $messageEn = "Operation failed (FFI status {$status}).";
        $messageHi = "Operation fail ho gaya (FFI status {$status}).";
        try {
            $raw = $this->ffi->dusrelimage_last_error();
            if (is_string($raw) && $raw !== '') {
                $decoded = json_decode($raw, true);
                if (is_array($decoded) && isset($decoded['code']) && is_string($decoded['code'])) {
                    $code = $decoded['code'];
                    $messageEn = (string) ($decoded['message_en'] ?? $decoded['messageEn'] ?? $messageEn);
                    $messageHi = (string) ($decoded['message_hi'] ?? $decoded['messageHi'] ?? $messageHi);
                }
            }
        } catch (\Throwable) {
            // Fall back to the mapped code and generic messages.
        }
        return new DusrelError($code, $messageEn, $messageHi);
    }

    /** JSON-encode options, throwing `INVALID_ARGUMENT` on failure. */
    private static function optionsJson(array $options): string
    {
        try {
            return json_encode($options, JSON_THROW_ON_ERROR | JSON_UNESCAPED_SLASHES);
        } catch (\JsonException $e) {
            throw new DusrelError(
                'INVALID_ARGUMENT',
                "Options could not be encoded as JSON: {$e->getMessage()}",
                "Options ko JSON me encode nahi kar paye: {$e->getMessage()}",
            );
        }
    }

    /** JSON-decode an FFI output buffer, throwing `INVALID_TYPE` on failure. */
    private static function decodeJson(string $json, string $what): array
    {
        try {
            $decoded = json_decode($json, true, 512, JSON_THROW_ON_ERROR);
        } catch (\JsonException $e) {
            throw new DusrelError(
                'INVALID_TYPE',
                "The native library returned invalid JSON for {$what}: {$e->getMessage()}",
                "Native library ne {$what} ke liye galat JSON diya: {$e->getMessage()}",
            );
        }
        if (!is_array($decoded)) {
            throw new DusrelError(
                'INVALID_TYPE',
                "The native library returned a non-object JSON value for {$what}.",
                "Native library ne {$what} ke liye non-object JSON diya.",
            );
        }
        return $decoded;
    }

    // ------------------------------------------------------------------
    // Client-side validation (cheap checks; the core does the real ones)
    // ------------------------------------------------------------------

    /**
     * Assert a value is image bytes (a PHP string). Empty bytes are
     * EMPTY_INPUT, like the Rust core and the JS wrapper.
     */
    private static function assertImageBytes(mixed $value, string $what = 'Image bytes'): string
    {
        if (!is_string($value)) {
            throw new DusrelError(
                'INVALID_TYPE',
                "{$what} must be a string of image bytes.",
                "{$what} image bytes ki string honi chahiye.",
            );
        }
        if ($value === '') {
            throw new DusrelError(
                'EMPTY_INPUT',
                "{$what} is empty.",
                "{$what} khaali hai.",
            );
        }
        return $value;
    }

    /** Assert a list of image byte strings (collage / bulk inputs). */
    private static function assertImageArray(mixed $value, string $what, string $emptyWhat): array
    {
        if (!is_array($value)) {
            throw new DusrelError(
                'INVALID_TYPE',
                "{$what} must be an array of image byte strings.",
                "{$what} image bytes ki strings ka array hona chahiye.",
            );
        }
        if ($value === []) {
            throw new DusrelError(
                'EMPTY_INPUT',
                "{$emptyWhat} is empty.",
                "{$emptyWhat} khaali hai.",
            );
        }
        foreach ($value as $item) {
            if (!is_string($item)) {
                throw new DusrelError(
                    'INVALID_TYPE',
                    "{$what} must be an array of image byte strings.",
                    "{$what} image bytes ki strings ka array hona chahiye.",
                );
            }
        }
        return array_values($value);
    }

    private static function assertNonNegativeInt(mixed $value, string $what): int
    {
        if (!is_int($value) || $value < 0) {
            throw new DusrelError(
                'INVALID_ARGUMENT',
                "{$what} must be a non-negative integer.",
                "{$what} non-negative integer hona chahiye.",
            );
        }
        return $value;
    }

    private static function invalidArgument(string $messageEn, string $messageHi): DusrelError
    {
        return new DusrelError('INVALID_ARGUMENT', $messageEn, $messageHi);
    }

    // ------------------------------------------------------------------
    // Public API — all synchronous, all throw DusrelError on failure.
    // ------------------------------------------------------------------

    /** Re-encode an image at a fixed quality (and `format`, or the input format when unset). */
    public function compress(mixed $input, array $options = []): string
    {
        $bytes = self::assertImageBytes($input);
        [$buf, $len] = $this->inputBuffer($bytes);
        [$out, $outLen] = $this->outParams();
        $status = $this->ffi->dusrelimage_compress(
            \FFI::addr($buf[0]),
            $len,
            self::optionsJson(self::compressOptions($options)),
            \FFI::addr($out[0]),
            \FFI::addr($outLen[0]),
        );
        return $this->finish($status, $out, $outLen);
    }

    /**
     * Re-encode an image so the result is at most `$targetBytes`
     * bytes, at the highest quality that fits. Fails with
     * `TARGET_SIZE_UNREACHABLE` when even the lowest quality is too
     * big. The quality search is JPEG/AVIF-only — other formats take
     * one best-effort encode.
     */
    public function compressToSize(mixed $input, mixed $targetBytes, array $options = []): string
    {
        $bytes = self::assertImageBytes($input);
        $target = self::assertNonNegativeInt($targetBytes, 'Target size');
        [$buf, $len] = $this->inputBuffer($bytes);
        [$out, $outLen] = $this->outParams();
        $status = $this->ffi->dusrelimage_compress_to_size(
            \FFI::addr($buf[0]),
            $len,
            $target,
            self::optionsJson(self::compressOptions($options)),
            \FFI::addr($out[0]),
            \FFI::addr($outLen[0]),
        );
        return $this->finish($status, $out, $outLen);
    }

    /** Resize an image (width and/or height, percent, fit, filter). */
    public function resize(mixed $input, array $options = []): string
    {
        $bytes = self::assertImageBytes($input);
        [$buf, $len] = $this->inputBuffer($bytes);
        [$out, $outLen] = $this->outParams();
        $status = $this->ffi->dusrelimage_resize(
            \FFI::addr($buf[0]),
            $len,
            self::optionsJson(self::resizeOptions($options)),
            \FFI::addr($out[0]),
            \FFI::addr($outLen[0]),
        );
        return $this->finish($status, $out, $outLen);
    }

    /** Crop an image to a rectangle (or an anchor preset). */
    public function crop(mixed $input, array $options = []): string
    {
        $bytes = self::assertImageBytes($input);
        [$buf, $len] = $this->inputBuffer($bytes);
        [$out, $outLen] = $this->outParams();
        $status = $this->ffi->dusrelimage_crop(
            \FFI::addr($buf[0]),
            $len,
            self::optionsJson(self::cropOptions($options)),
            \FFI::addr($out[0]),
            \FFI::addr($outLen[0]),
        );
        return $this->finish($status, $out, $outLen);
    }

    /** Convert an image to another format. */
    public function convert(mixed $input, array $options = []): string
    {
        $bytes = self::assertImageBytes($input);
        [$buf, $len] = $this->inputBuffer($bytes);
        [$out, $outLen] = $this->outParams();
        $status = $this->ffi->dusrelimage_convert(
            \FFI::addr($buf[0]),
            $len,
            self::optionsJson(self::convertOptions($options)),
            \FFI::addr($out[0]),
            \FFI::addr($outLen[0]),
        );
        return $this->finish($status, $out, $outLen);
    }

    /**
     * Stamp a text or logo watermark. `text` and `logo` may be
     * combined (text first, then logo); at least one is required.
     * Defaults: opacity 0.5, position "BottomRight", rotation 0,
     * margin 16.
     */
    public function addWatermark(mixed $input, array $options = []): string
    {
        $bytes = self::assertImageBytes($input);
        [$buf, $len] = $this->inputBuffer($bytes);
        [$out, $outLen] = $this->outParams();
        $status = $this->ffi->dusrelimage_add_watermark(
            \FFI::addr($buf[0]),
            $len,
            self::optionsJson(self::watermarkOptions($options)),
            \FFI::addr($out[0]),
            \FFI::addr($outLen[0]),
        );
        return $this->finish($status, $out, $outLen);
    }

    /** Draw annotations (arrows, rects, ellipses, lines, text) on an image. */
    public function annotate(mixed $input, mixed $annotations): string
    {
        $bytes = self::assertImageBytes($input);
        $parsed = self::annotationsToSerde($annotations);
        if ($parsed === []) {
            // Core semantics: an empty annotation list is
            // INVALID_ARGUMENT, not EMPTY_INPUT (matches the shared
            // test vectors).
            throw self::invalidArgument(
                'No annotations given: pass at least one annotation.',
                'Koi annotation nahi diya: kam se kam ek annotation dein.',
            );
        }
        [$buf, $len] = $this->inputBuffer($bytes);
        [$out, $outLen] = $this->outParams();
        $status = $this->ffi->dusrelimage_annotate(
            \FFI::addr($buf[0]),
            $len,
            self::optionsJson($parsed),
            \FFI::addr($out[0]),
            \FFI::addr($outLen[0]),
        );
        return $this->finish($status, $out, $outLen);
    }

    /**
     * Read the colour of one pixel. Out-of-bounds is
     * `PIXEL_OUT_OF_BOUNDS`.
     *
     * @return array{hex: string, r: int, g: int, b: int, a: int, hsl: array{0: float, 1: float, 2: float}}
     */
    public function pickColour(mixed $input, mixed $x, mixed $y): array
    {
        $bytes = self::assertImageBytes($input);
        $px = self::assertNonNegativeInt($x, 'Pixel x');
        $py = self::assertNonNegativeInt($y, 'Pixel y');
        [$buf, $len] = $this->inputBuffer($bytes);
        [$out, $outLen] = $this->outParams();
        $status = $this->ffi->dusrelimage_pick_colour(
            \FFI::addr($buf[0]),
            $len,
            $px,
            $py,
            \FFI::addr($out[0]),
            \FFI::addr($outLen[0]),
        );
        $json = $this->finish($status, $out, $outLen);
        return self::normalizeColour(self::decodeJson($json, 'pick_colour'));
    }

    /**
     * Extract the dominant colours, most frequent first (count 1-16).
     *
     * @return list<array{hex: string, r: int, g: int, b: int, a: int, hsl: array{0: float, 1: float, 2: float}, share: float}>
     */
    public function extractPalette(mixed $input, mixed $count = 5): array
    {
        $bytes = self::assertImageBytes($input);
        if (!is_int($count) || $count < 1 || $count > 16) {
            throw self::invalidArgument(
                'Palette count must be an integer between 1 and 16.',
                'Palette count 1 se 16 ke beech integer hona chahiye.',
            );
        }
        [$buf, $len] = $this->inputBuffer($bytes);
        [$out, $outLen] = $this->outParams();
        $status = $this->ffi->dusrelimage_extract_palette(
            \FFI::addr($buf[0]),
            $len,
            $count,
            \FFI::addr($out[0]),
            \FFI::addr($outLen[0]),
        );
        $json = $this->finish($status, $out, $outLen);
        $decoded = self::decodeJson($json, 'extract_palette');
        return array_map([self::class, 'normalizePaletteEntry'], array_values($decoded));
    }

    /** Build a collage from several images (grid, row or column). */
    public function makeCollage(mixed $inputs, array $options = []): string
    {
        $bufs = self::assertImageArray($inputs, 'Collage inputs', 'collage inputs');
        [$ptrs, $lens, $keepAlive, $count] = $this->inputArray($bufs);
        [$out, $outLen] = $this->outParams();
        $status = $this->ffi->dusrelimage_make_collage(
            \FFI::addr($ptrs[0]),
            \FFI::addr($lens[0]),
            $count,
            self::optionsJson(self::collageOptions($options)),
            \FFI::addr($out[0]),
            \FFI::addr($outLen[0]),
        );
        return $this->finish($status, $out, $outLen);
    }

    /** Pixelate a rectangle (whole image when width/height are 0/omitted). */
    public function pixelate(mixed $input, array $options = []): string
    {
        $bytes = self::assertImageBytes($input);
        [$buf, $len] = $this->inputBuffer($bytes);
        [$out, $outLen] = $this->outParams();
        $status = $this->ffi->dusrelimage_pixelate(
            \FFI::addr($buf[0]),
            $len,
            self::optionsJson(self::pixelateOptions($options)),
            \FFI::addr($out[0]),
            \FFI::addr($outLen[0]),
        );
        return $this->finish($status, $out, $outLen);
    }

    /**
     * Read an image's summary without modifying it.
     *
     * `$options` are the `CommonOptions` fields (`auto_orient`,
     * `keep_exif`, `dpi`, `max_pixels`, `max_dimension`), nested
     * under `common` and/or flat — they are sent to the FFI layer,
     * so `max_pixels` / `max_dimension` limits ARE honoured here
     * (the shared vectors `info-too-large-max-dimension` /
     * `info-too-large-max-pixels` pass).
     *
     * @return array{format: string, width: int, height: int, has_alpha: bool, colour_type: string, file_size: int}
     */
    public function getImageInfo(mixed $input, array $options = []): array
    {
        $bytes = self::assertImageBytes($input);
        [$buf, $len] = $this->inputBuffer($bytes);
        [$out, $outLen] = $this->outParams();
        $status = $this->ffi->dusrelimage_get_image_info(
            \FFI::addr($buf[0]),
            $len,
            self::optionsJson(self::commonOptions($options)),
            \FFI::addr($out[0]),
            \FFI::addr($outLen[0]),
        );
        $json = $this->finish($status, $out, $outLen);
        $info = self::decodeJson($json, 'get_image_info');
        return [
            'format' => (string) ($info['format'] ?? ''),
            'width' => (int) ($info['width'] ?? 0),
            'height' => (int) ($info['height'] ?? 0),
            'has_alpha' => (bool) ($info['has_alpha'] ?? false),
            'colour_type' => (string) ($info['colour_type'] ?? ''),
            'file_size' => (int) ($info['file_size'] ?? 0),
        ];
    }

    /**
     * Run one operation over many inputs in a single call. One bad
     * input fails only its own item (with `error.code` /
     * `error.messageEn`), never the whole batch. An empty input list
     * returns `[]` (core semantics). `collage` is not a per-item op:
     * it returns one `INVALID_ARGUMENT` result per input — call
     * `makeCollage` once with the full list instead. For op
     * `"compress_to_size"` pass `target_bytes` in `$options`; for
     * `"annotate"` pass `annotations`; for `"pick_colour"` pass
     * `x` / `y` (result items carry `colour`); for `"palette"` pass
     * `count` (result items carry `palette`).
     *
     * Op names: the short name (`"compress"`, `"resize"`, `"crop"`,
     * `"convert"`, `"watermark"`, `"annotate"`, `"pick_colour"`,
     * `"palette"`, `"collage"`, `"pixelate"`, `"compress_to_size"`)
     * or the full snake_case name (`"compress_image"`, …).
     *
     * @param mixed $inputs List of image byte strings.
     * @param mixed $op Operation name.
     * @return list<array{index: int, ok: bool, output?: string, colour?: array, palette?: list<array>, error?: array{code: string, messageEn: string, messageHi: string}}>
     */
    public function bulkProcess(mixed $inputs, mixed $op, array $options = []): array
    {
        if (is_array($inputs) && $inputs === []) {
            return [];
        }
        $bufs = self::assertImageArray($inputs, 'Bulk inputs', 'bulk inputs');
        $canonicalOp = self::normalizeBulkOp($op);
        $translated = self::bulkOptions($canonicalOp, $options);
        [$ptrs, $lens, $keepAlive, $count] = $this->inputArray($bufs);
        [$out, $outLen] = $this->outParams();
        $status = $this->ffi->dusrelimage_bulk_process(
            \FFI::addr($ptrs[0]),
            \FFI::addr($lens[0]),
            $count,
            $canonicalOp,
            self::optionsJson($translated),
            \FFI::addr($out[0]),
            \FFI::addr($outLen[0]),
        );
        $json = $this->finish($status, $out, $outLen);
        return self::normalizeBulkResults(self::decodeJson($json, 'bulk_process'));
    }

    /** Normalise a bulk op name to the short snake_case form the FFI layer expects. */
    private static function normalizeBulkOp(mixed $op): string
    {
        if (!is_string($op)) {
            throw self::invalidArgument(
                'Bulk op must be a string, e.g. "compress" or "resize".',
                'Bulk op string hona chahiye, jaise "compress" ya "resize".',
            );
        }
        $map = [
            'compress' => 'compress',
            'compress_image' => 'compress',
            'compress_to_size' => 'compress_to_size',
            'resize' => 'resize',
            'resize_image' => 'resize',
            'crop' => 'crop',
            'crop_image' => 'crop',
            'convert' => 'convert',
            'convert_image' => 'convert',
            'watermark' => 'watermark',
            'add_watermark' => 'watermark',
            'annotate' => 'annotate',
            'annotate_image' => 'annotate',
            'pick_colour' => 'pick_colour',
            'pick-colour' => 'pick_colour',
            'palette' => 'palette',
            'extract_palette' => 'palette',
            'collage' => 'collage',
            'make_collage' => 'collage',
            'pixelate' => 'pixelate',
            'pixelate_image' => 'pixelate',
        ];
        $key = strtolower($op);
        if (!isset($map[$key])) {
            throw self::invalidArgument(
                "Unknown bulk op \"{$op}\". Use compress, compress_to_size, resize, crop, convert, watermark, annotate, pick_colour, palette, collage or pixelate.",
                "Unknown bulk op \"{$op}\" hai. compress, compress_to_size, resize, crop, convert, watermark, annotate, pick_colour, palette, collage ya pixelate use karein.",
            );
        }
        return $map[$key];
    }

    /** Translate raw bulk options into the complete object the FFI layer expects for `$op`. */
    private static function bulkOptions(string $op, array $options): array
    {
        switch ($op) {
            case 'compress':
                return self::compressOptions($options);
            case 'compress_to_size':
                $target = $options['target_bytes'] ?? $options['targetBytes'] ?? null;
                if (!is_int($target)) {
                    throw self::invalidArgument(
                        'Bulk "compress_to_size" needs target_bytes in options.',
                        'Bulk "compress_to_size" ke liye options me target_bytes chahiye.',
                    );
                }
                $translated = self::compressOptions($options);
                $translated['target_bytes'] = self::assertNonNegativeInt($target, 'Target size');
                return $translated;
            case 'resize':
                return self::resizeOptions($options);
            case 'crop':
                return self::cropOptions($options);
            case 'convert':
                return self::convertOptions($options);
            case 'watermark':
                return self::watermarkOptions($options);
            case 'annotate':
                $translated = ['common' => self::commonOptions($options)];
                $translated['annotations'] = self::annotationsToSerde($options['annotations'] ?? null);
                return $translated;
            case 'pick_colour':
                return [
                    'common' => self::commonOptions($options),
                    'x' => $options['x'] ?? 0,
                    'y' => $options['y'] ?? 0,
                ];
            case 'palette':
                return [
                    'common' => self::commonOptions($options),
                    'count' => $options['count'] ?? 5,
                ];
            case 'collage':
                // Not a per-item op: the core answers INVALID_ARGUMENT
                // per item (a collage combines all inputs — use
                // makeCollage).
                return self::collageOptions($options);
            case 'pixelate':
                return self::pixelateOptions($options);
            default:
                throw self::invalidArgument(
                    "Unknown bulk op \"{$op}\".",
                    "Unknown bulk op \"{$op}\" hai.",
                );
        }
    }

    /** Normalise the FFI bulk JSON into the documented result shape. */
    private static function normalizeBulkResults(array $items): array
    {
        $out = [];
        foreach (array_values($items) as $item) {
            if (!is_array($item)) {
                continue;
            }
            $entry = [
                'index' => (int) ($item['index'] ?? 0),
                'ok' => (bool) ($item['ok'] ?? false),
            ];
            if (isset($item['output']) && is_string($item['output'])) {
                $decoded = base64_decode($item['output'], true);
                if ($decoded !== false) {
                    $entry['output'] = $decoded;
                }
            }
            if (isset($item['colour']) && is_array($item['colour'])) {
                $entry['colour'] = self::normalizeColour($item['colour']);
            }
            if (isset($item['palette']) && is_array($item['palette'])) {
                $entry['palette'] = array_map(
                    [self::class, 'normalizePaletteEntry'],
                    array_values($item['palette']),
                );
            }
            if (isset($item['error']) && is_array($item['error'])) {
                $error = $item['error'];
                $entry['error'] = [
                    'code' => (string) ($error['code'] ?? 'UNKNOWN'),
                    'messageEn' => (string) ($error['message_en'] ?? $error['messageEn'] ?? ''),
                    'messageHi' => (string) ($error['message_hi'] ?? $error['messageHi'] ?? ''),
                ];
            } elseif (isset($item['error_code']) && is_string($item['error_code'])) {
                $entry['error'] = [
                    'code' => $item['error_code'],
                    'messageEn' => (string) ($item['error_en'] ?? $item['error_message'] ?? ''),
                    'messageHi' => (string) ($item['error_hi'] ?? ''),
                ];
            }
            $out[] = $entry;
        }
        return $out;
    }

    /** Normalise a colour JSON object to `{hex, r, g, b, a, hsl}`. */
    private static function normalizeColour(array $colour): array
    {
        $r = (int) ($colour['r'] ?? 0);
        $g = (int) ($colour['g'] ?? 0);
        $b = (int) ($colour['b'] ?? 0);
        $a = (int) ($colour['a'] ?? 255);
        $hsl = $colour['hsl'] ?? null;
        if (!is_array($hsl) || count($hsl) !== 3) {
            $hsl = self::rgbToHsl($r, $g, $b);
        } else {
            $hsl = [(float) $hsl[0], (float) $hsl[1], (float) $hsl[2]];
        }
        return [
            'hex' => (string) ($colour['hex'] ?? self::rgbToHex($r, $g, $b)),
            'r' => $r,
            'g' => $g,
            'b' => $b,
            'a' => $a,
            'hsl' => $hsl,
        ];
    }

    /**
     * Normalise one palette entry to the flattened
     * `{hex, r, g, b, a, hsl, share}` shape (accepts the nested
     * `{colour: {...}, share}` shape too).
     */
    private static function normalizePaletteEntry(mixed $entry): array
    {
        if (!is_array($entry)) {
            return ['hex' => '', 'r' => 0, 'g' => 0, 'b' => 0, 'a' => 255, 'hsl' => [0.0, 0.0, 0.0], 'share' => 0.0];
        }
        $colour = $entry;
        if (isset($entry['colour']) && is_array($entry['colour'])) {
            $colour = $entry['colour'];
        }
        $flat = self::normalizeColour($colour);
        $flat['share'] = (float) ($entry['share'] ?? 0.0);
        return $flat;
    }

    // ------------------------------------------------------------------
    // Colour helpers — hex string <-> full Colour array (mirrors the
    // Rust Colour::from_hex / rgb_to_hsl so a hex string round-trips
    // exactly). Public so tests can check them without the native lib.
    // ------------------------------------------------------------------

    /** RGB (0-255) -> HSL (h degrees 0..360, s/l 0..1). */
    public static function rgbToHsl(int $r, int $g, int $b): array
    {
        $rn = $r / 255.0;
        $gn = $g / 255.0;
        $bn = $b / 255.0;
        $max = max($rn, $gn, $bn);
        $min = min($rn, $gn, $bn);
        $l = ($max + $min) / 2.0;
        if (abs($max - $min) < PHP_FLOAT_EPSILON) {
            return [0.0, 0.0, $l];
        }
        $d = $max - $min;
        $s = $l > 0.5 ? $d / (2.0 - $max - $min) : $d / ($max + $min);
        if (abs($max - $rn) < PHP_FLOAT_EPSILON) {
            // Rust: 60 * ((g - b) / d).rem_euclid(6.0)
            $h = 60.0 * self::remEuclid(($gn - $bn) / $d, 6.0);
        } elseif (abs($max - $gn) < PHP_FLOAT_EPSILON) {
            $h = 60.0 * ((($bn - $rn) / $d) + 2.0);
        } else {
            $h = 60.0 * ((($rn - $gn) / $d) + 4.0);
        }
        return [self::remEuclid($h, 360.0), $s, $l];
    }

    /** Rust `f64::rem_euclid` (always non-negative for a positive divisor). */
    private static function remEuclid(float $x, float $y): float
    {
        return $x - $y * floor($x / $y);
    }

    public static function rgbToHex(int $r, int $g, int $b): string
    {
        return sprintf('#%02X%02X%02X', $r, $g, $b);
    }

    /**
     * Expand a hex string (`"#RGB"`, `"#RRGGBB"`, `"#RRGGBBAA"`, with
     * or without `#`) into a full `Colour` array.
     *
     * @return array{hex: string, r: int, g: int, b: int, a: int, hsl: array{0: float, 1: float, 2: float}}
     */
    public static function colourFromHex(string $hex): array
    {
        if (!is_string($hex)) {
            throw self::invalidArgument(
                "Invalid hex colour \"{$hex}\". Use \"#RRGGBB\".",
                "Galat hex colour \"{$hex}\" hai. \"#RRGGBB\" use karein.",
            );
        }
        $h = str_starts_with($hex, '#') ? substr($hex, 1) : $hex;
        $bad = static fn() => self::invalidArgument(
            "Invalid hex colour \"{$hex}\". Use \"#RRGGBB\".",
            "Galat hex colour \"{$hex}\" hai. \"#RRGGBB\" use karein.",
        );
        $nibble = static function (string $ch) use ($bad): int {
            $v = hexdec($ch);
            if (!ctype_xdigit($ch)) {
                throw $bad();
            }
            return $v;
        };
        $pair = static function (string $s, int $i) use ($bad): int {
            $part = substr($s, $i, 2);
            if (strlen($part) !== 2 || !ctype_xdigit($part)) {
                throw $bad();
            }
            return hexdec($part);
        };
        $len = strlen($h);
        if ($len === 3) {
            $r = $nibble($h[0]) * 17;
            $g = $nibble($h[1]) * 17;
            $b = $nibble($h[2]) * 17;
            $a = 255;
        } elseif ($len === 6) {
            $r = $pair($h, 0);
            $g = $pair($h, 2);
            $b = $pair($h, 4);
            $a = 255;
        } elseif ($len === 8) {
            $r = $pair($h, 0);
            $g = $pair($h, 2);
            $b = $pair($h, 4);
            $a = $pair($h, 6);
        } else {
            throw $bad();
        }
        return [
            'hex' => self::rgbToHex($r, $g, $b),
            'r' => $r,
            'g' => $g,
            'b' => $b,
            'a' => $a,
            'hsl' => self::rgbToHsl($r, $g, $b),
        ];
    }

    /**
     * Normalise a colour input to a full `Colour` array. A `Colour`
     * array passes through (missing `hsl` is recomputed from rgb).
     *
     * @param string|array $value `"#RRGGBB"` or a Colour array.
     */
    public static function toColour(mixed $value): array
    {
        if (is_string($value)) {
            return self::colourFromHex($value);
        }
        if (
            is_array($value)
            && isset($value['r'], $value['g'], $value['b'])
            && is_int($value['r']) && is_int($value['g']) && is_int($value['b'])
        ) {
            $r = $value['r'];
            $g = $value['g'];
            $b = $value['b'];
            $a = isset($value['a']) && is_int($value['a']) ? $value['a'] : 255;
            $hex = isset($value['hex']) && is_string($value['hex']) && $value['hex'] !== ''
                ? $value['hex']
                : self::rgbToHex($r, $g, $b);
            $hsl = $value['hsl'] ?? null;
            if (!is_array($hsl) || count($hsl) !== 3) {
                $hsl = self::rgbToHsl($r, $g, $b);
            } else {
                $hsl = [(float) $hsl[0], (float) $hsl[1], (float) $hsl[2]];
            }
            return ['hex' => $hex, 'r' => $r, 'g' => $g, 'b' => $b, 'a' => $a, 'hsl' => $hsl];
        }
        throw self::invalidArgument(
            'Colour must be a "#RRGGBB" hex string or a { hex, r, g, b, a, hsl } array.',
            'Colour "#RRGGBB" hex string ya { hex, r, g, b, a, hsl } array hona chahiye.',
        );
    }

    // ------------------------------------------------------------------
    // Enum normalisation (lowercase / kebab spellings -> serde spelling)
    // ------------------------------------------------------------------

    private static function normalizeFormat(mixed $format): ?string
    {
        if ($format === null) {
            return null;
        }
        if (!is_string($format)) {
            throw self::invalidArgument(
                'Image format must be a string, e.g. "Jpeg".',
                'Image format string hona chahiye, jaise "Jpeg".',
            );
        }
        return match (strtolower($format)) {
            'jpeg', 'jpg' => 'Jpeg',
            'png' => 'Png',
            'webp' => 'WebP',
            'avif' => 'Avif',
            'gif' => 'Gif',
            'bmp' => 'Bmp',
            default => throw self::invalidArgument(
                "Unknown image format \"{$format}\". Use Jpeg, Png, WebP, Avif, Gif or Bmp.",
                "Unknown image format \"{$format}\" hai. Jpeg, Png, WebP, Avif, Gif ya Bmp use karein.",
            ),
        };
    }

    private static function normalizeFitMode(mixed $fit): string
    {
        $key = is_string($fit) ? strtolower(str_replace(['-', '_'], '', $fit)) : '';
        return match ($key) {
            'fit' => 'Fit',
            'fill' => 'Fill',
            'stretch' => 'Stretch',
            'crop' => 'Crop',
            default => throw self::invalidArgument(
                "Unknown fit mode \"{$fit}\". Use Fit, Fill, Stretch or Crop.",
                "Unknown fit mode \"{$fit}\" hai. Fit, Fill, Stretch ya Crop use karein.",
            ),
        };
    }

    private static function normalizeFilter(mixed $filter): string
    {
        $key = is_string($filter) ? strtolower(str_replace(['-', '_'], '', $filter)) : '';
        return match ($key) {
            'nearest' => 'Nearest',
            'triangle' => 'Triangle',
            'catmullrom' => 'CatmullRom',
            'gaussian' => 'Gaussian',
            'lanczos3' => 'Lanczos3',
            default => throw self::invalidArgument(
                "Unknown filter \"{$filter}\". Use Nearest, Triangle, CatmullRom, Gaussian or Lanczos3.",
                "Unknown filter \"{$filter}\" hai. Nearest, Triangle, CatmullRom, Gaussian ya Lanczos3 use karein.",
            ),
        };
    }

    private static function normalizePosition(mixed $position): string
    {
        $key = is_string($position) ? strtolower(str_replace(['-', '_'], '', $position)) : '';
        return match ($key) {
            'topleft' => 'TopLeft',
            'topcenter' => 'TopCenter',
            'topright' => 'TopRight',
            'centerleft' => 'CenterLeft',
            'center' => 'Center',
            'centerright' => 'CenterRight',
            'bottomleft' => 'BottomLeft',
            'bottomcenter' => 'BottomCenter',
            'bottomright' => 'BottomRight',
            'tiled' => 'Tiled',
            default => throw self::invalidArgument(
                "Unknown position \"{$position}\". Use TopLeft, TopCenter, TopRight, CenterLeft, Center, CenterRight, BottomLeft, BottomCenter, BottomRight or Tiled.",
                "Unknown position \"{$position}\" hai. TopLeft, TopCenter, TopRight, CenterLeft, Center, CenterRight, BottomLeft, BottomCenter, BottomRight ya Tiled use karein.",
            ),
        };
    }

    /**
     * Normalise a collage layout to serde's externally-tagged form:
     * `"2x2"` / `["rows" => 2, "cols" => 2]` / `["Grid" => [...]]`
     * become `["Grid" => ["rows" => 2, "cols" => 2]]`; `"row"` /
     * `"column"` / `"grid"` become `"Row"` / `"Column"` / the default
     * 2x2 grid.
     */
    public static function layoutToSerde(mixed $layout): mixed
    {
        if ($layout === null) {
            return ['Grid' => ['rows' => 2, 'cols' => 2]];
        }
        if (is_string($layout)) {
            $lower = strtolower(trim($layout));
            if ($lower === 'row') {
                return 'Row';
            }
            if ($lower === 'column') {
                return 'Column';
            }
            if ($lower === 'grid') {
                return ['Grid' => ['rows' => 2, 'cols' => 2]];
            }
            if (preg_match('/^(\d+)x(\d+)$/', $lower, $m) === 1) {
                return ['Grid' => ['rows' => (int) $m[1], 'cols' => (int) $m[2]]];
            }
            throw self::invalidArgument(
                "Unknown collage layout \"{$layout}\". Use grid, row, column or a WxH grid like 2x2.",
                "Unknown collage layout \"{$layout}\" hai. grid, row, column ya 2x2 jaisa WxH grid use karein.",
            );
        }
        if (is_array($layout)) {
            if (isset($layout['Grid']) && is_array($layout['Grid'])) {
                return ['Grid' => ['rows' => (int) $layout['Grid']['rows'], 'cols' => (int) $layout['Grid']['cols']]];
            }
            if (isset($layout['rows'], $layout['cols'])) {
                return ['Grid' => ['rows' => (int) $layout['rows'], 'cols' => (int) $layout['cols']]];
            }
            if (isset($layout[0]) && $layout[0] === 'Row') {
                return 'Row';
            }
            if (isset($layout[0]) && $layout[0] === 'Column') {
                return 'Column';
            }
        }
        throw self::invalidArgument(
            'Collage layout must be "grid", "row", "column", a "WxH" string or ["rows" => r, "cols" => c].',
            'Collage layout "grid", "row", "column", "WxH" string ya ["rows" => r, "cols" => c] hona chahiye.',
        );
    }

    // ------------------------------------------------------------------
    // Options translation — raw PHP arrays -> complete snake_case Rust
    // objects. Every translator fills ALL Rust-required fields with
    // the Rust defaults, because serde rejects partial objects (the
    // JS wrapper's complete-object approach). Public so tests can
    // check them without the native lib.
    // ------------------------------------------------------------------

    /**
     * Options shared by every operation. Accepts the `common` fields
     * nested under `"common"` or flat at the top level (nested wins).
     */
    public static function commonOptions(array $options = []): array
    {
        $nested = $options['common'] ?? [];
        if (!is_array($nested)) {
            throw self::invalidArgument(
                'Common options must be an array.',
                'Common options array honi chahiye.',
            );
        }
        $get = static fn(string $key, mixed $default) => $nested[$key] ?? $options[$key] ?? $default;
        $dpi = $get('dpi', null);
        if ($dpi !== null) {
            if (!is_array($dpi) || count($dpi) !== 2) {
                throw self::invalidArgument(
                    'DPI must be [x, y] or null.',
                    'DPI [x, y] ya null hona chahiye.',
                );
            }
            $dpi = [(int) $dpi[0], (int) $dpi[1]];
        }
        return [
            'auto_orient' => (bool) $get('auto_orient', true),
            'keep_exif' => (bool) $get('keep_exif', false),
            'dpi' => $dpi,
            'max_pixels' => (int) $get('max_pixels', 120_000_000),
            'max_dimension' => (int) $get('max_dimension', 16000),
        ];
    }

    public static function compressOptions(array $options = []): array
    {
        $quality = $options['quality'] ?? 80;
        if (!is_int($quality) || $quality < 1 || $quality > 100) {
            throw new DusrelError(
                'INVALID_QUALITY',
                "Quality {$quality} is invalid. Quality must be between 1 and 100.",
                "Quality {$quality} galat hai. Quality 1 se 100 ke beech honi chahiye.",
            );
        }
        $effort = $options['effort'] ?? 4;
        if (!is_int($effort) || $effort < 0 || $effort > 6) {
            throw self::invalidArgument(
                "Effort {$effort} is invalid. Effort must be between 0 and 6.",
                "Effort {$effort} galat hai. Effort 0 se 6 ke beech hona chahiye.",
            );
        }
        return [
            'common' => self::commonOptions($options),
            'quality' => $quality,
            'format' => self::normalizeFormat($options['format'] ?? null),
            'progressive' => (bool) ($options['progressive'] ?? false),
            'lossless' => (bool) ($options['lossless'] ?? false),
            'effort' => $effort,
        ];
    }

    public static function resizeOptions(array $options = []): array
    {
        return [
            'common' => self::commonOptions($options),
            'width' => isset($options['width']) ? (int) $options['width'] : null,
            'height' => isset($options['height']) ? (int) $options['height'] : null,
            'percent' => isset($options['percent']) ? (float) $options['percent'] : null,
            'fit' => self::normalizeFitMode($options['fit'] ?? 'Fit'),
            'filter' => self::normalizeFilter($options['filter'] ?? 'Lanczos3'),
            'background' => self::toColour($options['background'] ?? '#FFFFFF'),
        ];
    }

    public static function cropOptions(array $options = []): array
    {
        if (!isset($options['width']) || !is_int($options['width'])) {
            throw self::invalidArgument(
                'Crop width must be a positive integer.',
                'Crop width positive integer honi chahiye.',
            );
        }
        if (!isset($options['height']) || !is_int($options['height'])) {
            throw self::invalidArgument(
                'Crop height must be a positive integer.',
                'Crop height positive integer honi chahiye.',
            );
        }
        if ($options['width'] <= 0 || $options['height'] <= 0) {
            // Core semantics: a zero crop size is INVALID_DIMENSIONS,
            // not INVALID_ARGUMENT (matches the shared test vectors).
            throw new DusrelError(
                'INVALID_DIMENSIONS',
                'Invalid dimensions: crop width and height must be greater than 0.',
                'Galat dimensions hain: crop width and height must be greater than 0.',
            );
        }
        return [
            'common' => self::commonOptions($options),
            'x' => (int) ($options['x'] ?? 0),
            'y' => (int) ($options['y'] ?? 0),
            'width' => $options['width'],
            'height' => $options['height'],
            'anchor' => isset($options['anchor']) ? self::normalizePosition($options['anchor']) : null,
        ];
    }

    public static function convertOptions(array $options = []): array
    {
        if (!isset($options['format'])) {
            throw self::invalidArgument(
                'Convert needs a target format, e.g. ["format" => "WebP"].',
                'Convert ke liye target format chahiye, jaise ["format" => "WebP"].',
            );
        }
        $quality = $options['quality'] ?? 80;
        if (!is_int($quality) || $quality < 1 || $quality > 100) {
            throw new DusrelError(
                'INVALID_QUALITY',
                "Quality {$quality} is invalid. Quality must be between 1 and 100.",
                "Quality {$quality} galat hai. Quality 1 se 100 ke beech honi chahiye.",
            );
        }
        return [
            'common' => self::commonOptions($options),
            'format' => self::normalizeFormat($options['format']),
            'quality' => $quality,
            'background' => self::toColour($options['background'] ?? '#FFFFFF'),
            'progressive' => (bool) ($options['progressive'] ?? false),
        ];
    }

    public static function watermarkOptions(array $options = []): array
    {
        $opacity = $options['opacity'] ?? 0.5;
        if (!is_numeric($opacity) || $opacity < 0 || $opacity > 1) {
            throw self::invalidArgument(
                'Opacity must be between 0.0 and 1.0.',
                'Opacity 0.0 se 1.0 ke beech honi chahiye.',
            );
        }
        $out = [
            'common' => self::commonOptions($options),
            'text' => null,
            'logo' => null,
            'opacity' => (float) $opacity,
            'position' => self::normalizePosition($options['position'] ?? 'BottomRight'),
            'rotation_deg' => (float) ($options['rotation_deg'] ?? 0),
            'margin' => (int) ($options['margin'] ?? 16),
        ];
        if (array_key_exists('text', $options) && $options['text'] !== null) {
            $text = $options['text'];
            if (is_string($text)) {
                // Python-friendly: a plain string is a text watermark.
                $text = ['text' => $text];
            }
            if (!is_array($text) || !isset($text['text']) || !is_string($text['text'])) {
                throw self::invalidArgument(
                    'Text watermark needs a "text" string.',
                    'Text watermark ke liye "text" string chahiye.',
                );
            }
            $out['text'] = [
                'text' => $text['text'],
                'size' => (float) ($text['size'] ?? 48),
                'colour' => self::toColour($text['colour'] ?? '#FFFFFF'),
            ];
        }
        if (array_key_exists('logo', $options) && $options['logo'] !== null) {
            $logo = $options['logo'];
            if (is_string($logo)) {
                // Python-friendly: raw bytes are a logo at scale 1.0.
                $logo = ['image' => $logo];
            }
            if (!is_array($logo) || !isset($logo['image']) || !is_string($logo['image'])) {
                throw self::invalidArgument(
                    'Logo watermark needs "image" bytes.',
                    'Logo watermark ke liye "image" bytes chahiye.',
                );
            }
            $out['logo'] = [
                // A JSON array of byte values, NOT a string: serde
                // cannot decode a string into Vec<u8> (the JS
                // wrapper sends Array.from(bytes) the same way).
                'image' => array_values(unpack('C*', $logo['image'])),
                'scale' => (float) ($logo['scale'] ?? 1.0),
            ];
        }
        return $out;
    }

    public static function collageOptions(array $options = []): array
    {
        return [
            'common' => self::commonOptions($options),
            'layout' => self::layoutToSerde($options['layout'] ?? null),
            'gap' => (int) ($options['gap'] ?? 8),
            'background' => self::toColour($options['background'] ?? '#FFFFFF'),
            'width' => (int) ($options['width'] ?? 1200),
            'height' => (int) ($options['height'] ?? 1200),
            'cell_fit' => self::normalizeFitMode($options['cell_fit'] ?? $options['cellFit'] ?? 'Fit'),
        ];
    }

    public static function pixelateOptions(array $options = []): array
    {
        $blockSize = $options['block_size'] ?? $options['blockSize'] ?? 10;
        if (!is_int($blockSize) || $blockSize < 2 || $blockSize > 100) {
            throw self::invalidArgument(
                "Block size {$blockSize} is invalid. Block size must be between 2 and 100.",
                "Block size {$blockSize} galat hai. Block size 2 se 100 ke beech hona chahiye.",
            );
        }
        return [
            'common' => self::commonOptions($options),
            'x' => (int) ($options['x'] ?? 0),
            'y' => (int) ($options['y'] ?? 0),
            'width' => (int) ($options['width'] ?? 0),
            'height' => (int) ($options['height'] ?? 0),
            'block_size' => $blockSize,
        ];
    }

    /**
     * Translate one `{"type": "Rect", ...}` annotation to serde's
     * externally-tagged form `{"Rect": {...}}`, expanding colours.
     * An already-tagged `{"Rect": {...}}` array passes through (with
     * its colours expanded). The `type` name is case-insensitive
     * (`"rect"` works, like the Python wrapper).
     */
    public static function annotationToSerde(array $annotation): array
    {
        if (isset($annotation['type'])) {
            $type = $annotation['type'];
            if (!is_string($type)) {
                throw self::invalidArgument(
                    'Annotation "type" must be a string.',
                    'Annotation "type" string hona chahiye.',
                );
            }
            $canonical = match (strtolower($type)) {
                'arrow' => 'Arrow',
                'rect' => 'Rect',
                'ellipse' => 'Ellipse',
                'line' => 'Line',
                'text' => 'Text',
                default => throw self::invalidArgument(
                    "Unknown annotation type \"{$type}\". Use Arrow, Rect, Ellipse, Line or Text.",
                    "Unknown annotation type \"{$type}\" hai. Arrow, Rect, Ellipse, Line ya Text use karein.",
                ),
            };
            $fields = $annotation;
            unset($fields['type']);
            if (isset($fields['colour'])) {
                $fields['colour'] = self::toColour($fields['colour']);
            }
            if (array_key_exists('fill', $fields) && $fields['fill'] !== null) {
                $fields['fill'] = self::toColour($fields['fill']);
            }
            return [$canonical => $fields];
        }
        // Already serde-tagged: exactly one key, expand its colours.
        if (count($annotation) === 1) {
            $tag = array_key_first($annotation);
            $fields = $annotation[$tag];
            if (!is_array($fields)) {
                throw self::invalidArgument(
                    'Annotation fields must be an array.',
                    'Annotation fields array hone chahiye.',
                );
            }
            if (isset($fields['colour'])) {
                $fields['colour'] = self::toColour($fields['colour']);
            }
            if (array_key_exists('fill', $fields) && $fields['fill'] !== null) {
                $fields['fill'] = self::toColour($fields['fill']);
            }
            return [$tag => $fields];
        }
        throw self::invalidArgument(
            'Annotation needs a "type" key, e.g. ["type" => "Rect", ...].',
            'Annotation me "type" key chahiye, jaise ["type" => "Rect", ...].',
        );
    }

    /** Translate a list of annotations to serde's tagged form. */
    public static function annotationsToSerde(mixed $annotations): array
    {
        if (!is_array($annotations)) {
            throw new DusrelError(
                'INVALID_TYPE',
                'Annotations must be an array.',
                'Annotations array honi chahiye.',
            );
        }
        return array_map([self::class, 'annotationToSerde'], array_values($annotations));
    }
}
