"""Tests for the packages/python thin wrapper.

The wrapper delegates to the compiled ``dusrelimage._native``
extension from the installed wheel (maturin develop / pip install),
so these tests assert the delegation: same bytes, same info, same
error codes. The full API (dataclasses, colour forms, enum
spellings) is covered by crates/dusrelimage-py/tests/.
"""

import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

import dusrelimage  # noqa: E402
from dusrelimage import DusrelError  # noqa: E402


def noise(width=128, height=96, seed=42):
    return dusrelimage.make_test_image("noise", width, height, seed)


def info(image):
    return dusrelimage.get_image_info(image)


def test_wrapper_reexports_native():
    img = noise(64, 64)
    assert info(dusrelimage.compress_image(img, quality=80))["width"] == 64
    assert info(dusrelimage.resize_image(img, width=32))["width"] == 32
    assert info(dusrelimage.crop_image(
        img, x=0, y=0, width=16, height=16))["width"] == 16
    assert info(dusrelimage.convert_image(img, format="jpeg"))["format"] == "jpeg"
    assert info(dusrelimage.add_watermark(img, text="X"))["width"] == 64
    out = dusrelimage.annotate_image(img, [
        {"type": "rect", "x": 5, "y": 5, "w": 10, "h": 10,
         "colour": "#FF0000", "width": 1, "fill": "#FF0000"},
    ])
    assert info(out)["width"] == 64
    assert dusrelimage.pick_colour(img, 0, 0)["hex"].startswith("#")
    assert len(dusrelimage.extract_palette(img, 2)) >= 1
    collage = dusrelimage.make_collage(
        [img, img], layout="row", width=200, height=100)
    assert (info(collage)["width"], info(collage)["height"]) == (200, 100)
    assert info(dusrelimage.pixelate_image(img, block_size=8))["width"] == 64


def test_wrapper_options_dict_and_kwargs():
    img = noise(64, 64)
    out = dusrelimage.compress_image(img, {"format": "jpeg"}, quality=90)
    assert info(out)["format"] == "jpeg"
    out = dusrelimage.resize_image(img, {"width": 32})
    assert info(out)["width"] == 32


def test_wrapper_errors_are_dusrel_error():
    img = noise(64, 64)
    with pytest.raises(DusrelError) as excinfo:
        dusrelimage.crop_image(img, x=60, y=60, width=20, height=20)
    assert excinfo.value.code == "CROP_OUT_OF_BOUNDS"
    assert excinfo.value.reason_en
    assert excinfo.value.reason_hi
    with pytest.raises(DusrelError) as excinfo:
        dusrelimage.compress_image(b"garbage")
    assert excinfo.value.code == "INVALID_IMAGE"
    with pytest.raises(DusrelError) as excinfo:
        dusrelimage.compress_image("not bytes")
    assert excinfo.value.code == "INVALID_TYPE"


def test_wrapper_bulk_partial_failure():
    good = noise(64, 64)
    items = dusrelimage.bulk_process(
        [good, b"garbage", good], "resize", options={"width": 32})
    assert len(items) == 3
    assert info(items[0]["output"])["width"] == 32
    assert items[1]["output"] is None
    assert items[1]["error_code"] == "INVALID_IMAGE"
    assert info(items[2]["output"])["width"] == 32


def test_wrapper_pydantic_models():
    pydantic = pytest.importorskip("pydantic")
    from dusrelimage import pydantic as dpy

    assert dpy.CompressOptions().quality == 80
    assert dpy.ResizeOptions().filter == "lanczos3"
    img = noise(64, 64)
    out = dusrelimage.compress_image(
        img, options=dpy.CompressOptions(quality=90, format="jpeg"))
    assert info(out)["format"] == "jpeg"
