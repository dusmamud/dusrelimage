"""Shared test vectors (SPEC section 6): Rust, Python and JS all run
``shared/test-vectors.json`` and must agree.

This runner loads the JSON (path relative to this file), builds each
fixture — ``dusrelimage mock`` kinds (``solid``/``gradient``/``noise``
+ seed) via the Python ``make_test_image`` (same Rust builders), plus
the documented derived fixtures (``solid_rgb``, ``transparent_rgba``,
``two_colour``, ``tiff``, ``jpeg``, ``avif``, ``garbage``, ``empty``)
built here with stdlib-only PNG/TIFF encoders (no Pillow: CI only
installs maturin + pytest) — runs the op through the public Python
API, and asserts the expected dimensions / pixel spot-checks /
error code.

It FAILS LOUDLY on any case whose fixture/op kind it cannot handle:
unknown fixture kinds, ops, bulk ops or expected keys raise an
``AssertionError`` naming the case id instead of silently skipping
the case. Pixel checks decode PNG output with a small stdlib PNG
decoder below; non-PNG outputs fall back to ``pick_colour`` per
pixel (only the JPEG flatten case needs that).
"""

import json
import struct
import zlib
from pathlib import Path

import pytest

import dusrelimage
from dusrelimage import DusrelError

VECTORS_PATH = Path(__file__).resolve().parents[3] / "shared" / "test-vectors.json"
CASES = json.loads(VECTORS_PATH.read_text())

# Expected keys this runner knows how to assert. Anything else is a
# schema gap and fails the case (see _assert_expected).
KNOWN_EXPECTED_KEYS = {
    "format",
    "width",
    "height",
    "has_alpha",
    "colour_type",
    "error",
    "max_bytes",
    "avif_header",
    "pixels",
    "region_all_white",
    "region_has_dark",
    "regions_have_dark",
    "colour",
    "different_from",
    "palette_hexes",
    "palette_length",
    "share_each",
    "share_sum",
    "share_sum_tolerance",
    "share_tolerance",
    "palette",
    "differs_from_input",
    "pixel_equalities",
    "unchanged_pixels",
    "results",
}


def test_vector_count():
    assert len(CASES) == 82, (
        "shared/test-vectors.json case count changed — update this "
        "runner's count assertion"
    )


# ---------------------------------------------------------------------------
# Minimal PNG encoder / decoder (stdlib only)
# ---------------------------------------------------------------------------


def _png_chunk(ctype, data):
    chunk = ctype + data
    return struct.pack(">I", len(data)) + chunk + struct.pack(">I", zlib.crc32(chunk))


def _encode_png(width, height, pixel, rgba=False):
    """pixel(x, y) -> (r, g, b) or (r, g, b, a); filter 0 per row."""
    channels = 4 if rgba else 3
    colour_type = 6 if rgba else 2
    raw = bytearray()
    for y in range(height):
        raw.append(0)
        for x in range(width):
            raw.extend(pixel(x, y)[:channels])
    ihdr = struct.pack(">IIBBBBB", width, height, 8, colour_type, 0, 0, 0)
    return (
        b"\x89PNG\r\n\x1a\n"
        + _png_chunk(b"IHDR", ihdr)
        + _png_chunk(b"IDAT", zlib.compress(bytes(raw)))
        + _png_chunk(b"IEND", b"")
    )


def _paeth(a, b, c):
    p = a + b - c
    pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
    if pa <= pb and pa <= pc:
        return a
    if pb <= pc:
        return b
    return c


def _decode_png(data):
    """Return (width, height, rows) with rows of (r, g, b, a) tuples.

    Colour types 2 (RGB) and 6 (RGBA), bit depth 8, filters 0-4.
    """
    assert data[:8] == b"\x89PNG\r\n\x1a\n", "not a PNG"
    pos = 8
    idat = b""
    width = height = colour_type = None
    while pos < len(data):
        (length,) = struct.unpack(">I", data[pos : pos + 4])
        ctype = data[pos + 4 : pos + 8]
        chunk = data[pos + 8 : pos + 8 + length]
        if ctype == b"IHDR":
            width, height, bit_depth, colour_type = struct.unpack(">IIBB", chunk[:10])
            assert bit_depth == 8, f"unsupported PNG bit depth {bit_depth}"
            assert colour_type in (2, 6), f"unsupported PNG colour type {colour_type}"
        elif ctype == b"IDAT":
            idat += chunk
        elif ctype == b"IEND":
            break
        pos += 12 + length
    channels = 3 if colour_type == 2 else 4
    stride = width * channels
    raw = zlib.decompress(idat)
    rows = []
    prev = bytearray(stride)
    p = 0
    for _y in range(height):
        f = raw[p]
        p += 1
        line = bytearray(raw[p : p + stride])
        p += stride
        for i in range(stride):
            a = line[i - channels] if i >= channels else 0
            b = prev[i]
            c = prev[i - channels] if i >= channels else 0
            if f == 0:
                pass
            elif f == 1:
                line[i] = (line[i] + a) & 0xFF
            elif f == 2:
                line[i] = (line[i] + b) & 0xFF
            elif f == 3:
                line[i] = (line[i] + (a + b) // 2) & 0xFF
            elif f == 4:
                line[i] = (line[i] + _paeth(a, b, c)) & 0xFF
            else:  # pragma: no cover - invalid filter
                raise AssertionError(f"unsupported PNG filter {f}")
        prev = line
        row = []
        for x in range(width):
            px = line[x * channels : (x + 1) * channels]
            row.append((px[0], px[1], px[2], px[3] if channels == 4 else 255))
        rows.append(row)
    return width, height, rows


def _encode_tiff(width, height, rgb):
    """Minimal uncompressed little-endian TIFF, RGB 8-bit."""
    bits_offset = 8 + 2 + 10 * 12 + 4  # header + IFD
    data_offset = bits_offset + 6
    pixels = bytes(rgb) * (width * height)

    def entry(tag, typ, count, value):
        return struct.pack("<HHII", tag, typ, count, value)

    ifd = b"".join(
        [
            entry(256, 4, 1, width),
            entry(257, 4, 1, height),
            entry(258, 3, 3, bits_offset),
            entry(259, 3, 1, 1),
            entry(262, 3, 1, 2),
            entry(273, 4, 1, data_offset),
            entry(277, 3, 1, 3),
            entry(278, 4, 1, height),
            entry(279, 4, 1, len(pixels)),
            entry(284, 3, 1, 1),
        ]
    )
    return (
        b"II*\x00"
        + struct.pack("<I", 8)
        + struct.pack("<H", 10)
        + ifd
        + struct.pack("<I", 0)
        + struct.pack("<HHH", 8, 8, 8)
        + pixels
    )


# ---------------------------------------------------------------------------
# Fixtures
# ---------------------------------------------------------------------------


def _parse_dims_from_derived(text, case_id):
    for token in text.replace(",", " ").split():
        if "x" in token:
            parts = token.split("x")
            if len(parts) == 2 and all(p.isdigit() for p in parts):
                return int(parts[0]), int(parts[1])
    raise AssertionError(f"case {case_id}: no WxH dimensions in derived_from: {text}")


def _parse_rgb_from_derived(text, case_id):
    start = text.find("(")
    end = text.find(")", start)
    if start < 0 or end < 0:
        raise AssertionError(f"case {case_id}: no rgb tuple in derived_from: {text}")
    parts = [p.strip() for p in text[start + 1 : end].split(",")]
    if len(parts) != 3 or not all(p.isdigit() for p in parts):
        raise AssertionError(f"case {case_id}: bad rgb tuple in derived_from: {text}")
    return tuple(int(p) for p in parts)


def _fixture_bytes(fixture, case_id):
    kind = fixture.get("kind")
    if kind in ("solid", "gradient", "noise"):
        return dusrelimage.make_test_image(
            kind, fixture["width"], fixture["height"], fixture["seed"]
        )
    if kind == "solid_rgb":
        r, g, b = fixture["rgb"]
        return _encode_png(
            fixture["width"], fixture["height"], lambda x, y: (r, g, b)
        )
    if kind == "transparent_rgba":
        return _encode_png(
            fixture["width"],
            fixture["height"],
            lambda x, y: (0, 0, 0, 0),
            rgba=True,
        )
    if kind == "two_colour":
        # Left half red, right half blue, 50/50 by construction.
        width = fixture["width"]
        return _encode_png(
            width,
            fixture["height"],
            lambda x, y: (255, 0, 0) if x < width // 2 else (0, 0, 255),
        )
    if kind == "tiff":
        # rgb + dimensions live only in derived_from ("rgb (10,20,30)
        # 48x32 TIFF, written with the image crate").
        derived = fixture.get("derived_from", "")
        rgb = fixture.get("rgb") or _parse_rgb_from_derived(derived, case_id)
        if "width" in fixture:
            width, height = fixture["width"], fixture["height"]
        else:
            width, height = _parse_dims_from_derived(derived, case_id)
        return _encode_tiff(width, height, rgb)
    if kind == "jpeg":
        # Derived: compress gradient 100x80 seed 2 to JPEG q80.
        assert "jpeg" in fixture.get("derived_from", ""), (
            f"case {case_id}: jpeg fixture derived_from does not mention jpeg"
        )
        src = dusrelimage.make_test_image("gradient", 100, 80, 2)
        return dusrelimage.compress_image(src, format="jpeg", quality=80)
    if kind == "avif":
        # Derived: convert gradient 64x48 seed 4 to AVIF q80.
        assert "avif" in fixture.get("derived_from", ""), (
            f"case {case_id}: avif fixture derived_from does not mention avif"
        )
        src = dusrelimage.make_test_image("gradient", 64, 48, 4)
        return dusrelimage.convert_image(src, format="avif", quality=80)
    if kind == "garbage":
        derived = fixture.get("derived_from", "")
        if derived.startswith("bytes [") and derived.endswith("]"):
            return bytes(int(n) for n in derived[7:-1].split(","))
        if derived.startswith("ASCII bytes: '") and derived.endswith("'"):
            return derived[len("ASCII bytes: '") : -1].encode()
        raise AssertionError(
            f"case {case_id}: unhandled garbage derived_from: {derived}"
        )
    if kind == "empty":
        return b""
    raise AssertionError(f"case {case_id}: unknown fixture kind {kind!r} — update this runner")


def _resolve_logo(options, case_id):
    """Deep-copy options, replacing a watermark logo fixture dict with
    the real logo bytes (the wrapper expects bytes, not a fixture)."""
    if not isinstance(options, dict) or "logo" not in options:
        return options
    options = json.loads(json.dumps(options))
    logo = options["logo"]
    if isinstance(logo, dict) and isinstance(logo.get("image"), dict):
        logo["image"] = _fixture_bytes(logo["image"], case_id)
    return options


# ---------------------------------------------------------------------------
# Running one case
# ---------------------------------------------------------------------------


def _run_case(case, case_id):
    """Return (outcome, input_bytes_or_None). Outcome is bytes, a dict,
    a list, or a bulk result list; errors are raised as DusrelError
    and caught by the caller."""
    op = case["op"]
    options = dict(case.get("options") or {})
    if "fixtures" in case:
        inputs = [_fixture_bytes(f, case_id) for f in case["fixtures"]]
        image = inputs[0] if inputs else None
    elif "fixture" in case:
        image = _fixture_bytes(case["fixture"], case_id)
        inputs = [image]
    else:
        image = None
        inputs = []

    if op == "get_image_info":
        return dusrelimage.get_image_info(image, options=options), image
    if op == "compress_image":
        return dusrelimage.compress_image(image, options=options), image
    if op == "compress_to_size":
        return (
            dusrelimage.compress_to_size(image, case["target_bytes"], options=options),
            image,
        )
    if op == "resize_image":
        return dusrelimage.resize_image(image, options=options), image
    if op == "crop_image":
        return dusrelimage.crop_image(image, options=options), image
    if op == "convert_image":
        return dusrelimage.convert_image(image, options=options), image
    if op == "add_watermark":
        return (
            dusrelimage.add_watermark(image, options=_resolve_logo(options, case_id)),
            image,
        )
    if op == "annotate_image":
        return dusrelimage.annotate_image(image, case["annotations"]), image
    if op == "pick_colour":
        return (
            dusrelimage.pick_colour(image, options["x"], options["y"]),
            image,
        )
    if op == "extract_palette":
        return dusrelimage.extract_palette(image, options["count"]), image
    if op == "make_collage":
        return dusrelimage.make_collage(inputs, options=options), None
    if op == "pixelate_image":
        return dusrelimage.pixelate_image(image, options=options), image
    if op == "bulk_process":
        bulk_op = case["bulk_op"]
        if bulk_op == "Pixelate":
            return (
                dusrelimage.bulk_process(inputs, "pixelate", options=options),
                None,
            )
        if bulk_op == "PickColour":
            return (
                dusrelimage.bulk_process(inputs, "pick_colour", x=options["x"], y=options["y"]),
                None,
            )
        if bulk_op == "Palette":
            return (
                dusrelimage.bulk_process(inputs, "palette", count=options["count"]),
                None,
            )
        if bulk_op == "Collage":
            return (
                dusrelimage.bulk_process(inputs, "collage", options=options),
                None,
            )
        if bulk_op == "Watermark":
            return (
                dusrelimage.bulk_process(
                    inputs, "watermark", options=_resolve_logo(options, case_id)
                ),
                None,
            )
        if bulk_op == "Annotate":
            return (
                dusrelimage.bulk_process(inputs, "annotate", annotations=case["annotations"]),
                None,
            )
        raise AssertionError(f"case {case_id}: unknown bulk_op {bulk_op!r} — update this runner")
    raise AssertionError(f"case {case_id}: unknown op {op!r} — update this runner")


# ---------------------------------------------------------------------------
# Assertions on the expected object
# ---------------------------------------------------------------------------


class _Pixels:
    """Pixel access for an image output: PNG outputs decode with the
    stdlib decoder above; anything else goes through pick_colour."""

    def __init__(self, data, case_id):
        self._data = data
        self._case_id = case_id
        self._rows = None
        if data[:8] == b"\x89PNG\r\n\x1a\n":
            _w, _h, self._rows = _decode_png(data)

    def pixel(self, x, y):
        if self._rows is not None:
            return self._rows[y][x]
        colour = dusrelimage.pick_colour(self._data, x, y)
        return (colour["r"], colour["g"], colour["b"], colour["a"])

    def region_has_dark(self, x0, y0, x1, y1):
        if self._rows is None:
            raise AssertionError(
                f"case {self._case_id}: region check needs a PNG output"
            )
        for y in range(y0, y1):
            for x in range(x0, x1):
                px = self._rows[y][x]
                if px[0] < 128 or px[1] < 128 or px[2] < 128:
                    return True
        return False

    def region_all_white(self, x0, y0, x1, y1):
        if self._rows is None:
            raise AssertionError(
                f"case {self._case_id}: region check needs a PNG output"
            )
        for y in range(y0, y1):
            for x in range(x0, x1):
                if self._rows[y][x][:3] != (255, 255, 255):
                    return False
        return True


def _check_pixel_assert(px, spec, case_id, failures):
    x, y = spec["x"], spec["y"]
    if "rgba" in spec:
        if list(px) != list(spec["rgba"]):
            failures.append(
                f"case {case_id}: pixel ({x},{y}) expected rgba {spec['rgba']}, got {list(px)}"
            )
    if "rgba_approx" in spec:
        tol = spec.get("tolerance", 0)
        for i, want in enumerate(spec["rgba_approx"]):
            if abs(px[i] - want) > tol:
                failures.append(
                    f"case {case_id}: pixel ({x},{y}) channel {i} expected ~{want}±{tol}, got {px[i]}"
                )
    for key, ch in (("r", 0), ("g", 1), ("b", 2), ("a", 3)):
        if key in spec and px[ch] != spec[key]:
            failures.append(
                f"case {case_id}: pixel ({x},{y}) channel {key} expected {spec[key]}, got {px[ch]}"
            )
    if "r_max" in spec and px[0] > spec["r_max"]:
        failures.append(
            f"case {case_id}: pixel ({x},{y}) r expected <= {spec['r_max']}, got {px[0]}"
        )


def _assert_expected(case, case_id, outcome, error, image, failures):
    expected = case["expected"]
    unknown = set(expected) - KNOWN_EXPECTED_KEYS
    if unknown:
        failures.append(
            f"case {case_id}: unhandled expected key(s) {sorted(unknown)} — update this runner"
        )
    if "error" in expected:
        if error is None:
            failures.append(
                f"case {case_id}: expected error {expected['error']}, got success"
            )
        elif error.code != expected["error"]:
            failures.append(
                f"case {case_id}: expected error {expected['error']}, got {error.code}"
            )
        return
    if error is not None:
        failures.append(
            f"case {case_id}: expected success, got {error.code} ({error.reason_en})"
        )
        return

    if case["op"] == "get_image_info":
        info = outcome
    elif isinstance(outcome, (bytes, bytearray)):
        # AVIF output cannot be decoded back in this build (no
        # avif-native decoder): a case that only asserts the
        # header/size may skip the decode; anything asserting
        # dims/pixels must be decodable.
        try:
            info = dusrelimage.get_image_info(bytes(outcome))
        except DusrelError:
            if any(
                k in expected
                for k in ("format", "width", "height", "has_alpha", "colour_type")
            ):
                raise
            info = None
    else:
        info = None
    if info is not None:
        for key, field in (
            ("format", "format"),
            ("width", "width"),
            ("height", "height"),
            ("has_alpha", "has_alpha"),
            ("colour_type", "colour_type"),
        ):
            if key in expected and info[field] != expected[key]:
                failures.append(
                    f"case {case_id}: expected {key}={expected[key]!r}, got {info[field]!r}"
                )

    if isinstance(outcome, (bytes, bytearray)):
        out = bytes(outcome)
        if "max_bytes" in expected and len(out) > expected["max_bytes"]:
            failures.append(
                f"case {case_id}: expected <= {expected['max_bytes']} bytes, got {len(out)}"
            )
        if expected.get("avif_header") and out[4:12] != b"ftypavif":
            failures.append(f"case {case_id}: expected AVIF ftypavif header")
        pixels = _Pixels(out, case_id)
        input_pixels = _Pixels(image, case_id) if image is not None else None
        for spec in expected.get("pixels", []):
            _check_pixel_assert(pixels.pixel(spec["x"], spec["y"]), spec, case_id, failures)
        for key, check in (
            ("region_all_white", pixels.region_all_white),
            ("region_has_dark", pixels.region_has_dark),
        ):
            if key in expected:
                r = expected[key]
                if not check(r["x0"], r["y0"], r["x1"], r["y1"]):
                    failures.append(
                        f"case {case_id}: region ({r['x0']},{r['y0']})-({r['x1']},{r['y1']}) "
                        f"failed {key}"
                    )
        for r in expected.get("regions_have_dark", []):
            if not pixels.region_has_dark(r["x0"], r["y0"], r["x1"], r["y1"]):
                failures.append(
                    f"case {case_id}: region ({r['x0']},{r['y0']})-({r['x1']},{r['y1']}) "
                    "should contain dark pixels"
                )
        if input_pixels is not None:
            for x, y in expected.get("differs_from_input", []):
                if pixels.pixel(x, y) == input_pixels.pixel(x, y):
                    failures.append(
                        f"case {case_id}: pixel ({x},{y}) should differ from the input"
                    )
            for pair in expected.get("pixel_equalities", []):
                ax, ay = pair["a"]
                bx, by = pair["b"]
                same = pixels.pixel(ax, ay) == pixels.pixel(bx, by)
                if same != pair["equal"]:
                    failures.append(
                        f"case {case_id}: pixels ({ax},{ay}) vs ({bx},{by}) equal={same}, "
                        f"expected {pair['equal']}"
                    )
            for x, y in expected.get("unchanged_pixels", []):
                if pixels.pixel(x, y) != input_pixels.pixel(x, y):
                    failures.append(
                        f"case {case_id}: pixel ({x},{y}) should be unchanged from the input"
                    )

    if case["op"] == "pick_colour":
        colour = outcome
        if "colour" in expected:
            want = expected["colour"]
            for key in ("hex", "r", "g", "b", "a"):
                if key in want and colour[key] != want[key]:
                    failures.append(
                        f"case {case_id}: expected colour {key}={want[key]!r}, got {colour[key]!r}"
                    )
        if "different_from" in expected:
            other = expected["different_from"]
            other_colour = dusrelimage.pick_colour(image, other["x"], other["y"])
            if other_colour["hex"] == colour["hex"]:
                failures.append(
                    f"case {case_id}: colour should differ from the pixel at "
                    f"({other['x']},{other['y']})"
                )

    if case["op"] == "extract_palette":
        palette = outcome
        if "palette_length" in expected and len(palette) != expected["palette_length"]:
            failures.append(
                f"case {case_id}: expected palette length {expected['palette_length']}, "
                f"got {len(palette)}"
            )
        if "palette_hexes" in expected:
            # Compared as a set: the contract orders by frequency
            # ("most frequent first") and these shares are equal, so
            # the tie order is unspecified — the core breaks ties by
            # hex ascending. Shares are asserted separately.
            if sorted(p["hex"] for p in palette) != sorted(expected["palette_hexes"]):
                failures.append(
                    f"case {case_id}: expected palette hexes {expected['palette_hexes']}, "
                    f"got {[p['hex'] for p in palette]}"
                )
        share_tol = expected.get("share_tolerance", 0.0)
        if "share_each" in expected:
            for entry in palette:
                if abs(entry["share"] - expected["share_each"]) > share_tol:
                    failures.append(
                        f"case {case_id}: palette {entry['hex']} share expected "
                        f"{expected['share_each']}±{share_tol}, got {entry['share']}"
                    )
        if "share_sum" in expected:
            total = sum(p["share"] for p in palette)
            if abs(total - expected["share_sum"]) > expected.get("share_sum_tolerance", 0.0):
                failures.append(
                    f"case {case_id}: palette share sum expected {expected['share_sum']}, "
                    f"got {total}"
                )
        if "palette" in expected:
            want = expected["palette"]
            if len(want) != len(palette):
                failures.append(
                    f"case {case_id}: expected palette of {len(want)}, got {len(palette)}"
                )
            else:
                for w, got in zip(want, palette):
                    if "hex" in w and got["hex"] != w["hex"]:
                        failures.append(
                            f"case {case_id}: expected palette hex {w['hex']}, got {got['hex']}"
                        )
                    if "share" in w and abs(got["share"] - w["share"]) > w.get(
                        "share_tolerance", 0.0
                    ):
                        failures.append(
                            f"case {case_id}: palette {got['hex']} share expected {w['share']}, "
                            f"got {got['share']}"
                        )

    if case["op"] == "bulk_process":
        results = outcome
        want = expected.get("results", [])
        if len(results) != len(want):
            failures.append(
                f"case {case_id}: expected {len(want)} bulk results, got {len(results)}"
            )
            return
        for got, want_item in zip(results, want):
            idx = want_item.get("index", got["index"])
            if got["index"] != idx:
                failures.append(
                    f"case {case_id}: bulk item index expected {idx}, got {got['index']}"
                )
            want_ok = want_item.get("ok", True)
            got_ok = got["error_code"] is None
            if got_ok != want_ok:
                failures.append(
                    f"case {case_id}: bulk item {idx} ok expected {want_ok}, got {got_ok}"
                )
                continue
            if not got_ok:
                if "error" in want_item and got["error_code"] != want_item["error"]:
                    failures.append(
                        f"case {case_id}: bulk item {idx} error expected {want_item['error']}, "
                        f"got {got['error_code']}"
                    )
                continue
            output = got["output"]
            if want_item.get("output") == "Image":
                if not isinstance(output, (bytes, bytearray)):
                    failures.append(
                        f"case {case_id}: bulk item {idx} output expected Image bytes"
                    )
                    continue
                info = dusrelimage.get_image_info(bytes(output))
                for key in ("width", "height"):
                    if key in want_item and info[key] != want_item[key]:
                        failures.append(
                            f"case {case_id}: bulk item {idx} {key} expected "
                            f"{want_item[key]}, got {info[key]}"
                        )
            elif want_item.get("output") == "Colour":
                if "colour" in want_item and output["hex"] != want_item["colour"]["hex"]:
                    failures.append(
                        f"case {case_id}: bulk item {idx} colour hex expected "
                        f"{want_item['colour']['hex']}, got {output['hex']}"
                    )
            elif want_item.get("output") == "Palette":
                if "palette_length" in want_item and len(output) != want_item["palette_length"]:
                    failures.append(
                        f"case {case_id}: bulk item {idx} palette length expected "
                        f"{want_item['palette_length']}, got {len(output)}"
                    )
                for w, got_entry in zip(want_item.get("palette", []), output):
                    if "hex" in w and got_entry["hex"] != w["hex"]:
                        failures.append(
                            f"case {case_id}: bulk item {idx} palette hex expected {w['hex']}, "
                            f"got {got_entry['hex']}"
                        )


@pytest.mark.parametrize("case", CASES, ids=[c["id"] for c in CASES])
def test_shared_vector(case):
    case_id = case["id"]
    failures = []
    try:
        outcome, image = _run_case(case, case_id)
        error = None
    except DusrelError as exc:
        outcome, image, error = None, None, exc
        # _run_case may have failed before producing the input; the
        # input is only needed for pixel comparisons on success.
        if "fixture" in case:
            image = _fixture_bytes(case["fixture"], case_id)
    except AssertionError:
        raise
    _assert_expected(case, case_id, outcome, error, image, failures)
    assert not failures, "\n".join(failures)
