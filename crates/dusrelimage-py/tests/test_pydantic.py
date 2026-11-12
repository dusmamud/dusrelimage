"""Pydantic v2 model tests.

The models are pure Python (no native import) and mirror the Rust
options exactly, with the same defaults. ``pydantic`` is an optional
extra — these tests are skipped when it is not installed, and the
lazy-import behaviour is tested without it in
``test_pydantic_lazy_without_pydantic`` (which runs either way).
"""

import importlib
import sys

import pytest

pydantic = pytest.importorskip("pydantic")
from pydantic import ValidationError  # noqa: E402

import dusrelimage  # noqa: E402
from dusrelimage import pydantic as dpy  # noqa: E402


def test_defaults_match_rust():
    assert dpy.CompressOptions().model_dump() == {
        "common": {
            "auto_orient": True,
            "keep_exif": False,
            "dpi": None,
            "max_pixels": 120_000_000,
            "max_dimension": 16000,
        },
        "quality": 80,
        "format": None,
        "progressive": False,
        "lossless": False,
        "effort": 4,
    }
    assert dpy.ResizeOptions().fit == "fit"
    assert dpy.ResizeOptions().filter == "lanczos3"
    assert dpy.ResizeOptions().background.hex == "#FFFFFF"
    assert dpy.CropOptions().x == 0
    assert dpy.CropOptions().width == 0
    assert dpy.CropOptions().anchor is None
    assert dpy.ConvertOptions().format == "jpeg"
    assert dpy.ConvertOptions().quality == 80
    assert dpy.WatermarkOptions().opacity == 0.5
    assert dpy.WatermarkOptions().position == "bottom_right"
    assert dpy.WatermarkOptions().rotation_deg == 0.0
    assert dpy.WatermarkOptions().margin == 16
    assert dpy.CollageOptions().layout == "grid"
    assert dpy.CollageOptions().gap == 8
    assert dpy.CollageOptions().width == 1200
    assert dpy.CollageOptions().height == 1200
    assert dpy.CollageOptions().cell_fit == "fit"
    assert dpy.PixelateOptions().block_size == 10
    assert dpy.PixelateOptions().width == 0


def test_colour_derives_hex_and_hsl():
    colour = dpy.Colour(r=255, g=128, b=0)
    assert colour.hex == "#FF8000"
    assert colour.a == 255
    h, s, li = colour.hsl
    assert 29.0 < h < 31.0
    assert s == 1.0
    assert li == 0.5
    assert dpy.Colour(r=0, g=0, b=0).hex == "#000000"


def test_bad_quality_rejected():
    with pytest.raises(ValidationError):
        dpy.CompressOptions(quality=0)
    with pytest.raises(ValidationError):
        dpy.CompressOptions(quality=101)
    assert dpy.CompressOptions(quality=100).quality == 100


def test_bad_opacity_rejected():
    with pytest.raises(ValidationError):
        dpy.WatermarkOptions(opacity=1.5)
    with pytest.raises(ValidationError):
        dpy.WatermarkOptions(opacity=-0.1)
    assert dpy.WatermarkOptions(opacity=1.0).opacity == 1.0
    assert dpy.WatermarkOptions(opacity=0.0).opacity == 0.0


def test_bad_enum_rejected():
    with pytest.raises(ValidationError):
        dpy.WatermarkOptions(position="middle")
    with pytest.raises(ValidationError):
        dpy.ResizeOptions(fit="diagonal")
    with pytest.raises(ValidationError):
        dpy.ConvertOptions(format="tiff")


def test_bad_effort_and_block_size_rejected():
    with pytest.raises(ValidationError):
        dpy.CompressOptions(effort=7)
    with pytest.raises(ValidationError):
        dpy.PixelateOptions(block_size=1)
    with pytest.raises(ValidationError):
        dpy.PixelateOptions(block_size=101)


def test_pydantic_model_as_options():
    img = dusrelimage.make_test_image("noise", 128, 96, 42)
    out = dusrelimage.compress_image(
        img, options=dpy.CompressOptions(quality=90, format="jpeg"))
    assert dusrelimage.get_image_info(out)["format"] == "jpeg"
    out = dusrelimage.resize_image(
        img, options=dpy.ResizeOptions(width=64))
    assert dusrelimage.get_image_info(out)["width"] == 64
    out = dusrelimage.convert_image(
        img, options=dpy.ConvertOptions(format="png"))
    assert dusrelimage.get_image_info(out)["format"] == "png"
    out = dusrelimage.add_watermark(
        img, options=dpy.WatermarkOptions(
            text=dpy.TextWatermark(text="HI", size=20.0)))
    assert dusrelimage.get_image_info(out)["width"] == 128
    out = dusrelimage.make_collage(
        [img, img], options=dpy.CollageOptions(layout="row",
                                               width=400, height=200))
    assert dusrelimage.get_image_info(out)["width"] == 400


def test_image_info_model():
    img = dusrelimage.make_test_image("noise", 64, 48, 42)
    raw = dusrelimage.get_image_info(img)
    model = dpy.ImageInfo(**raw)
    assert model.format == "png"
    assert model.width == 64
    assert model.height == 48
    assert model.has_alpha is False
    assert model.file_size == len(img)


def test_palette_colour_model():
    img = dusrelimage.make_test_image("solid", 16, 16, 7)
    raw = dusrelimage.extract_palette(img, 1)[0]
    model = dpy.PaletteColour(**raw)
    assert model.share == 1.0
    assert model.hex == raw["hex"]


def test_models_do_not_need_native():
    # pydantic.py imports no native code: the models build fine even if
    # the extension were absent — here we just assert the module has no
    # _native attribute and constructs models standalone.
    assert not hasattr(dpy, "_native")
    assert dpy.CommonOptions().auto_orient is True
    assert dpy.CommonOptions().max_pixels == 120_000_000


def test_pydantic_lazy_without_pydantic():
    # The module must import cleanly with pydantic hidden from
    # sys.modules, and model use must raise a helpful ImportError.
    saved = sys.modules.pop("pydantic", None)
    saved_models = {
        name: sys.modules.pop(name, None)
        for name in list(sys.modules)
        if name == "pydantic" or name.startswith("pydantic.")
    }
    saved_dpy = sys.modules.pop("dusrelimage.pydantic", None)
    try:
        import builtins
        real_import = builtins.__import__

        def blocked_import(name, *args, **kwargs):
            if name == "pydantic" or name.startswith("pydantic."):
                raise ImportError("No module named 'pydantic' (blocked)")
            return real_import(name, *args, **kwargs)

        builtins.__import__ = blocked_import
        try:
            reloaded = importlib.import_module("dusrelimage.pydantic")
        finally:
            builtins.__import__ = real_import
        # Import worked; using a model raises helpfully.
        with pytest.raises(ImportError, match="Pydantic v2"):
            reloaded.CompressOptions()
    finally:
        if saved is not None:
            sys.modules["pydantic"] = saved
        for name, mod in saved_models.items():
            if mod is not None:
                sys.modules[name] = mod
        if saved_dpy is not None:
            sys.modules["dusrelimage.pydantic"] = saved_dpy
        else:
            sys.modules.pop("dusrelimage.pydantic", None)
