<?php

declare(strict_types=1);

// PHPUnit bootstrap: Composer autoloader (src/ + tests/), plus a
// warning — not a failure — when the native library is missing, so
// the pure-PHP tests (OptionsTest) can still run. Any test that
// needs the FFI library fails loudly in its setUp instead.
require __DIR__ . '/../vendor/autoload.php';

use Dusrelimage\Dusrelimage;

try {
    Dusrelimage::resolveLibPath();
} catch (\RuntimeException $e) {
    fwrite(
        STDERR,
        "\n[phpunit bootstrap] WARNING: {$e->getMessage()}\n"
        . "[phpunit bootstrap] FFI-backed tests will fail until the library is built.\n\n",
    );
}
