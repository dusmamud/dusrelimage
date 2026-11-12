"""Options tests: kwargs, options dicts, dataclasses and Pydantic
models — kwargs win over the dict, and both ride the Rust defaults
for anything omitted.
"""

import pytest

import dusrelimage
from dusrelimage import DusrelError


def noise(width=128, height=96, seed=42):
    return dusrelimage.make_test_image("noise", width, height, seed)


def info(image):
    return dusrelimage.get_image_info(image)


def test_kwargs_win_over_options_dict():
    img = noise()
    # options dict says quality 0 (invalid), kwarg says 80 (valid):
    # the kwarg wins, so this must succeed.
    out = dusrelimage.compress_image(
        img, format="jpeg", quality=80, options={"quality": 0})
    assert info(out)["format"] == "jpeg"
    # And without the kwarg, the dict value is used — and fails.
    with pytest.raises(DusrelError) as excinfo:
        dusrelimage.compress_image(img, format="jpeg", options={"quality": 0})
    assert excinfo.value.code == "INVALID_QUALITY"


def test_options_dict_only():
    img = noise()
    out = dusrelimage.resize_image(img, options={"width": 64})
    assert info(out)["width"] == 64
    out = dusrelimage.convert_image(img, options={"format": "png", "quality": 90})
    assert info(out)["format"] == "png"


def test_options_dataclass():
    img = noise()
    out = dusrelimage.compress_image(
        img, options=dusrelimage.CompressOptions(quality=90, format="jpeg"))
    assert info(out)["format"] == "jpeg"
    out = dusrelimage.resize_image(
        img, options=dusrelimage.ResizeOptions(width=64, height=64, fit="stretch"))
    assert (info(out)["width"], info(out)["height"]) == (64, 64)
    out = dusrelimage.crop_image(
        img, options=dusrelimage.CropOptions(x=0, y=0, width=32, height=32))
    assert (info(out)["width"], info(out)["height"]) == (32, 32)
    out = dusrelimage.pixelate_image(
        img, options=dusrelimage.PixelateOptions(block_size=4))
    assert info(out)["width"] == 128


def test_options_nested_common_dict():
    img = noise()
    out = dusrelimage.compress_image(
        img, format="jpeg",
        options={"common": {"max_pixels": 120_000_000}, "quality": 80})
    assert info(out)["format"] == "jpeg"


def test_common_kwargs_flat():
    img = noise()
    out = dusrelimage.compress_image(img, format="jpeg", max_pixels=120_000_000)
    assert info(out)["format"] == "jpeg"
    out = dusrelimage.resize_image(img, width=64, auto_orient=True)
    assert info(out)["width"] == 64


def test_common_max_pixels_limit():
    img = noise(128, 96)
    with pytest.raises(DusrelError) as excinfo:
        dusrelimage.compress_image(img, max_pixels=100)
    assert excinfo.value.code == "IMAGE_TOO_LARGE"


def test_common_max_dimension_limit():
    img = noise(128, 96)
    with pytest.raises(DusrelError) as excinfo:
        dusrelimage.resize_image(img, width=64, max_dimension=10)
    assert excinfo.value.code == "IMAGE_TOO_LARGE"


def test_quality_bounds():
    img = noise()
    for quality in (1, 50, 100):
        out = dusrelimage.compress_image(img, format="jpeg", quality=quality)
        assert info(out)["format"] == "jpeg"


def test_background_colour_forms():
    img = dusrelimage.make_test_image("solid", 64, 32, 1)
    for background in ("#00FF00", [0, 255, 0], (0, 255, 0),
                       {"r": 0, "g": 255, "b": 0},
                       dusrelimage.Colour.from_hex("#00FF00")):
        out = dusrelimage.resize_image(
            img, width=64, height=64, fit="fill", background=background)
        assert dusrelimage.pick_colour(out, 0, 0)["hex"] == "#00FF00"


def test_enum_spellings():
    img = noise(64, 64)
    out = dusrelimage.resize_image(
        img, width=32, height=32, fit="Stretch", filter="Lanczos3")
    assert (info(out)["width"], info(out)["height"]) == (32, 32)
    out = dusrelimage.crop_image(img, width=32, height=32, anchor="BottomRight")
    assert (info(out)["width"], info(out)["height"]) == (32, 32)
    out = dusrelimage.convert_image(img, format="PNG")
    assert info(out)["format"] == "png"


def test_dpi_option_accepted():
    # Accepted but not honoured in v0.1.0 (encoder limit): the call
    # succeeds and the output is a valid image.
    img = noise(64, 64)
    out = dusrelimage.compress_image(img, format="jpeg", dpi=[300, 300])
    assert info(out)["format"] == "jpeg"
    out = dusrelimage.compress_image(
        img, format="jpeg", options={"dpi": [72, 72]})
    assert info(out)["format"] == "jpeg"


def test_keep_exif_and_progressive_accepted():
    img = noise(64, 64)
    out = dusrelimage.compress_image(
        img, format="jpeg", keep_exif=True, progressive=True)
    assert info(out)["format"] == "jpeg"


def test_collage_options_dict_layout_forms():
    imgs = [dusrelimage.make_test_image("solid", 32, 32, s) for s in (1, 2)]
    for layout in ("row", "column", {"rows": 1, "cols": 2},
                   {"Grid": {"rows": 1, "cols": 2}}, "1x2"):
        out = dusrelimage.make_collage(imgs, layout=layout,
                                       width=200, height=100)
        assert (info(out)["width"], info(out)["height"]) == (200, 100)


def test_watermark_text_and_logo_together():
    img = noise(128, 96)
    logo = dusrelimage.make_test_image("solid", 16, 16, 5)
    out = dusrelimage.add_watermark(img, text="X", logo=logo, opacity=0.5)
    assert info(out)["width"] == 128
