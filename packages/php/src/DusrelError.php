<?php

declare(strict_types=1);

namespace Dusrelimage;

/**
 * Every dusrelimage failure, with a stable machine-readable code.
 *
 * Mirrors the Rust `DusrelError { code, message_en, message_hi }`
 * (crates/dusrelimage/src/error.rs) and the JS/Python wrappers:
 * `$code` is one of the stable codes (`INVALID_IMAGE`,
 * `UNSUPPORTED_FORMAT`, `IMAGE_TOO_LARGE`, `TARGET_SIZE_UNREACHABLE`,
 * `CROP_OUT_OF_BOUNDS`, `PIXEL_OUT_OF_BOUNDS`, `INVALID_QUALITY`,
 * `INVALID_DIMENSIONS`, `INVALID_ARGUMENT`, `EMPTY_INPUT`,
 * `INVALID_TYPE`, plus `UNKNOWN` for an unrecognised FFI status);
 * `$messageEn` / `$messageHi` are the human-readable messages from
 * the Rust core (Roman-script Hinglish, like indycheck).
 */
class DusrelError extends \Exception
{
    /**
     * Stable, machine-readable error code, e.g. `"INVALID_IMAGE"`.
     *
     * Deliberately untyped: `\Exception` already defines `$code`
     * (untyped, `protected`), and PHP fatals if a child redeclares an
     * inherited property with a type the parent does not have.
     *
     * @var string
     */
    public $code;

    /** English message from the Rust core. */
    public string $messageEn;

    /** Hindi (Roman-script) message from the Rust core. */
    public string $messageHi;

    /** Alias of `$messageEn` (the Rust JSON also carries `reason_*`). */
    public string $reasonEn;

    /** Alias of `$messageHi`. */
    public string $reasonHi;

    public function __construct(string $code, string $messageEn = '', string $messageHi = '')
    {
        parent::__construct("{$code}: {$messageEn}");
        $this->code = $code;
        $this->messageEn = $messageEn;
        $this->messageHi = $messageHi;
        $this->reasonEn = $messageEn;
        $this->reasonHi = $messageHi;
    }
}
