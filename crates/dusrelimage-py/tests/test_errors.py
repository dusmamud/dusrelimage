"""Error-code tests: every failure is a DusrelError with the right
code and English + Hindi reasons — including wrong input types, which
must raise INVALID_TYPE and never a bare TypeError (indycheck v0.2.1
lesson).
"""

import pytest

import dusrelimage
from dusrelimage import DusrelError


def check_code(call, code):
    with pytest.raises(DusrelError) as excinfo:
        call()
    err = excinfo.value
    assert err.code == code, f"expected {code}, got {err.code}: {err.reason_en}"
    assert err.reason_en, "reason_en must not be empty"
    assert err.reason_hi, "reason_hi must not be empty"
    return err


def noise(width=128, height=96, seed=42):
    return dusrelimage.make_test_image("noise", width, height, seed)


def solid(width=64, height=64, seed=7):
    return dusrelimage.make_test_image("solid", width, height, seed)


# --- INVALID_IMAGE / EMPTY_INPUT -------------------------------------------

def test_invalid_image_garbage():
    check_code(lambda: dusrelimage.compress_image(b"not an image at all"),
               "INVALID_IMAGE")
    check_code(lambda: dusrelimage.get_image_info(b"garbage"), "INVALID_IMAGE")
    check_code(lambda: dusrelimage.resize_image(b"\x89PNG\r\n\x1a\n\x00\x00"),
               "INVALID_IMAGE")


def test_empty_input():
    check_code(lambda: dusrelimage.compress_image(b""), "EMPTY_INPUT")
    check_code(lambda: dusrelimage.get_image_info(b""), "EMPTY_INPUT")
    check_code(lambda: dusrelimage.convert_image(b"", format="png"),
               "EMPTY_INPUT")
    check_code(lambda: dusrelimage.make_collage([]), "EMPTY_INPUT")


def test_truncated_png():
    img = noise()
    check_code(lambda: dusrelimage.compress_image(img[:20]), "INVALID_IMAGE")


# --- CROP_OUT_OF_BOUNDS / PIXEL_OUT_OF_BOUNDS -------------------------------

def test_crop_out_of_bounds():
    img = noise(64, 64)
    check_code(
        lambda: dusrelimage.crop_image(img, x=50, y=50, width=20, height=20),
        "CROP_OUT_OF_BOUNDS")
    check_code(
        lambda: dusrelimage.crop_image(img, x=0, y=0, width=65, height=64),
        "CROP_OUT_OF_BOUNDS")


def test_crop_zero_size():
    img = noise(64, 64)
    check_code(
        lambda: dusrelimage.crop_image(img, x=0, y=0, width=0, height=10),
        "INVALID_DIMENSIONS")


def test_pick_colour_out_of_bounds():
    img = noise(64, 64)
    check_code(lambda: dusrelimage.pick_colour(img, 64, 0),
               "PIXEL_OUT_OF_BOUNDS")
    check_code(lambda: dusrelimage.pick_colour(img, 0, 64),
               "PIXEL_OUT_OF_BOUNDS")


# --- INVALID_QUALITY / TARGET_SIZE_UNREACHABLE ------------------------------

def test_invalid_quality():
    img = noise()
    check_code(lambda: dusrelimage.compress_image(img, quality=0),
               "INVALID_QUALITY")
    check_code(lambda: dusrelimage.compress_image(img, quality=101),
               "INVALID_QUALITY")
    check_code(lambda: dusrelimage.convert_image(img, format="png", quality=0),
               "INVALID_QUALITY")


def test_target_size_unreachable():
    img = noise(256, 192)
    check_code(
        lambda: dusrelimage.compress_to_size(img, 100, format="jpeg"),
        "TARGET_SIZE_UNREACHABLE")
    check_code(
        lambda: dusrelimage.compress_to_size(img, 0, format="jpeg"),
        "TARGET_SIZE_UNREACHABLE")


# --- INVALID_DIMENSIONS / INVALID_ARGUMENT ----------------------------------

def test_resize_no_size():
    img = noise()
    check_code(lambda: dusrelimage.resize_image(img), "INVALID_DIMENSIONS")


def test_resize_percent_and_size_conflict():
    img = noise()
    check_code(
        lambda: dusrelimage.resize_image(img, width=64, percent=50.0),
        "INVALID_ARGUMENT")


def test_resize_zero_percent():
    img = noise()
    check_code(lambda: dusrelimage.resize_image(img, percent=0.0),
               "INVALID_ARGUMENT")


def test_convert_needs_format():
    img = noise()
    check_code(lambda: dusrelimage.convert_image(img), "INVALID_ARGUMENT")


def test_convert_unknown_format():
    img = noise()
    check_code(lambda: dusrelimage.convert_image(img, format="tiff"),
               "INVALID_ARGUMENT")


def test_watermark_needs_text_or_logo():
    img = noise()
    check_code(lambda: dusrelimage.add_watermark(img), "INVALID_ARGUMENT")


def test_watermark_bad_opacity():
    img = noise()
    check_code(lambda: dusrelimage.add_watermark(img, text="X", opacity=1.5),
               "INVALID_ARGUMENT")


def test_watermark_bad_position():
    img = noise()
    check_code(
        lambda: dusrelimage.add_watermark(img, text="X", position="middle"),
        "INVALID_ARGUMENT")


def test_annotate_needs_annotations():
    img = noise()
    check_code(lambda: dusrelimage.annotate_image(img, []), "INVALID_ARGUMENT")


def test_annotate_unknown_type():
    img = noise()
    check_code(
        lambda: dusrelimage.annotate_image(
            img, [{"type": "star", "x": 0, "y": 0}]),
        "INVALID_ARGUMENT")


def test_annotate_bad_colour():
    img = noise()
    check_code(
        lambda: dusrelimage.annotate_image(img, [
            {"type": "rect", "x": 0, "y": 0, "w": 5, "h": 5,
             "colour": "not-a-colour", "width": 1}]),
        "INVALID_ARGUMENT")


def test_palette_count_bounds():
    img = noise()
    check_code(lambda: dusrelimage.extract_palette(img, 0), "INVALID_ARGUMENT")
    check_code(lambda: dusrelimage.extract_palette(img, 17), "INVALID_ARGUMENT")


def test_pixelate_block_size_bounds():
    img = noise()
    check_code(lambda: dusrelimage.pixelate_image(img, block_size=1),
               "INVALID_ARGUMENT")
    check_code(lambda: dusrelimage.pixelate_image(img, block_size=101),
               "INVALID_ARGUMENT")


def test_collage_bad_layout():
    imgs = [solid(), solid()]
    check_code(lambda: dusrelimage.make_collage(imgs, layout="5x5"),
               "INVALID_ARGUMENT")
    check_code(lambda: dusrelimage.make_collage(imgs, layout="masonry"),
               "INVALID_ARGUMENT")


def test_compress_lossless_wrong_format():
    img = noise()
    check_code(
        lambda: dusrelimage.compress_image(img, format="jpeg", lossless=True),
        "INVALID_ARGUMENT")


def test_compress_effort_bounds():
    img = noise()
    check_code(lambda: dusrelimage.compress_image(img, effort=7),
               "INVALID_ARGUMENT")


def test_bulk_unknown_op():
    imgs = [noise()]
    check_code(lambda: dusrelimage.bulk_process(imgs, "rotate"),
               "INVALID_ARGUMENT")
    check_code(lambda: dusrelimage.bulk_process(imgs, "info"),
               "INVALID_ARGUMENT")


def test_bulk_compress_to_size_needs_target():
    imgs = [noise()]
    check_code(lambda: dusrelimage.bulk_process(imgs, "compress_to_size"),
               "INVALID_ARGUMENT")


def test_bulk_annotate_needs_annotations():
    imgs = [noise()]
    check_code(lambda: dusrelimage.bulk_process(imgs, "annotate"),
               "INVALID_ARGUMENT")


def test_bulk_pick_colour_needs_xy():
    imgs = [noise()]
    check_code(lambda: dusrelimage.bulk_process(imgs, "pick_colour"),
               "INVALID_ARGUMENT")
    check_code(lambda: dusrelimage.bulk_process(imgs, "pick_colour", x=1),
               "INVALID_ARGUMENT")


def test_bulk_palette_count_bounds():
    imgs = [noise()]
    check_code(lambda: dusrelimage.bulk_process(imgs, "palette", count=0),
               "INVALID_ARGUMENT")
    check_code(lambda: dusrelimage.bulk_process(imgs, "palette", count=17),
               "INVALID_ARGUMENT")


def test_image_too_large_via_common():
    img = noise(128, 96)
    check_code(lambda: dusrelimage.get_image_info(img, max_pixels=100),
               "IMAGE_TOO_LARGE")


# --- INVALID_TYPE (never a bare TypeError) ----------------------------------

def test_wrong_input_types():
    img = noise()
    check_code(lambda: dusrelimage.compress_image("not bytes"), "INVALID_TYPE")
    check_code(lambda: dusrelimage.compress_image(12345), "INVALID_TYPE")
    check_code(lambda: dusrelimage.compress_image(None), "INVALID_TYPE")
    check_code(lambda: dusrelimage.resize_image(img, width="64"),
               "INVALID_TYPE")
    check_code(lambda: dusrelimage.pick_colour(img, "x", 0), "INVALID_TYPE")
    check_code(lambda: dusrelimage.pick_colour(img, -1, 0), "INVALID_ARGUMENT")
    check_code(lambda: dusrelimage.annotate_image(img, "not a list"),
               "INVALID_TYPE")
    check_code(lambda: dusrelimage.annotate_image(img, ["not a dict"]),
               "INVALID_TYPE")
    check_code(lambda: dusrelimage.make_collage("not a list"), "INVALID_TYPE")
    check_code(lambda: dusrelimage.bulk_process("not a list", "resize"),
               "INVALID_TYPE")
    check_code(lambda: dusrelimage.compress_image(img, options="nope"),
               "INVALID_TYPE")
    check_code(lambda: dusrelimage.extract_palette(img, "five"),
               "INVALID_TYPE")


def test_bytearray_and_memoryview_accepted():
    img = bytearray(noise(64, 64))
    out = dusrelimage.compress_image(img, format="jpeg")
    assert dusrelimage.get_image_info(out)["format"] == "jpeg"
    out = dusrelimage.compress_image(memoryview(bytes(img)), format="jpeg")
    assert dusrelimage.get_image_info(out)["format"] == "jpeg"
