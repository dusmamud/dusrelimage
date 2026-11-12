"""End-to-end tests for all 10 dusrelimage tools + info + bulk.

Fixtures come from the native module itself (the same builders the
Rust tests use), so every language tests the same bytes. Every output
is re-loaded with get_image_info and verified — never just "it
returned bytes". Pixel spot-checks use pick_colour on solid images,
where the expected colour is exact.
"""

import dusrelimage
from dusrelimage import DusrelError


def info(image):
    return dusrelimage.get_image_info(image)


def solid(width=64, height=64, seed=7):
    return dusrelimage.make_test_image("solid", width, height, seed)


def noise(width=128, height=96, seed=42):
    return dusrelimage.make_test_image("noise", width, height, seed)


# --- Tool 1: compressor ----------------------------------------------------

def test_compress_png_round_trip():
    img = noise()
    out = dusrelimage.compress_image(img, quality=80)
    assert isinstance(out, bytes)
    assert info(out)["width"] == 128
    assert info(out)["height"] == 96
    assert info(out)["format"] == "png"


def test_compress_to_jpeg():
    img = noise()
    out = dusrelimage.compress_image(img, format="jpeg", quality=80)
    assert info(out)["format"] == "jpeg"
    assert info(out)["width"] == 128


def test_compress_quality_changes_jpeg_size():
    img = noise(256, 192)
    hi = dusrelimage.compress_image(img, format="jpeg", quality=95)
    lo = dusrelimage.compress_image(img, format="jpeg", quality=20)
    assert len(lo) < len(hi)


def test_compress_to_size_jpeg():
    img = noise(256, 192)
    out = dusrelimage.compress_to_size(img, 20 * 1024, format="jpeg")
    assert len(out) <= 20 * 1024
    assert info(out)["format"] == "jpeg"


def test_compress_to_webp():
    img = noise()
    out = dusrelimage.compress_image(img, format="webp")
    assert info(out)["format"] == "webp"


def test_compress_to_avif():
    img = noise(64, 64)
    out = dusrelimage.compress_image(img, format="avif", quality=50)
    # AVIF input does not decode, so get_image_info cannot read the
    # output back — verify the AVIF container magic instead.
    assert out[4:8] == b"ftyp" and out[8:12] == b"avif"


def test_compress_keeps_input_format():
    # No TIFF fixture builder exists in the Python layer (the core has
    # no TIFF encoder), so the TIFF-falls-back-to-PNG path is covered
    # by the Rust tests; here we assert PNG input keeps PNG output.
    img = noise()
    out = dusrelimage.compress_image(img)
    assert info(out)["format"] == "png"


# --- Tool 2: resizer -------------------------------------------------------

def test_resize_width_only_keeps_aspect():
    img = noise(128, 96)
    out = dusrelimage.resize_image(img, width=64)
    result = info(out)
    assert result["width"] == 64
    assert result["height"] == 48


def test_resize_height_only_keeps_aspect():
    img = noise(128, 96)
    out = dusrelimage.resize_image(img, height=48)
    result = info(out)
    assert result["width"] == 64
    assert result["height"] == 48


def test_resize_percent():
    img = noise(128, 96)
    out = dusrelimage.resize_image(img, percent=50.0)
    result = info(out)
    assert result["width"] == 64
    assert result["height"] == 48


def test_resize_exact_and_fits():
    img = noise(128, 96)
    for fit in ("fit", "fill", "stretch", "crop"):
        out = dusrelimage.resize_image(img, width=64, height=64, fit=fit)
        result = info(out)
        if fit == "fit":
            assert (result["width"], result["height"]) == (64, 48)
        else:
            assert (result["width"], result["height"]) == (64, 64)


def test_resize_filters():
    img = noise(64, 64)
    for filt in ("nearest", "triangle", "catmull-rom", "gaussian", "lanczos3"):
        out = dusrelimage.resize_image(img, width=32, height=32, filter=filt)
        assert info(out)["width"] == 32


def test_resize_fill_background():
    img = solid(64, 32, seed=1)
    out = dusrelimage.resize_image(
        img, width=64, height=64, fit="fill", background="#00FF00")
    assert info(out)["width"] == 64
    assert info(out)["height"] == 64
    # Letterbox bars are green: the top-left corner is background.
    assert dusrelimage.pick_colour(out, 0, 0)["hex"] == "#00FF00"


# --- Tool 3: cropper -------------------------------------------------------

def test_crop_explicit_rect():
    img = noise(128, 96)
    out = dusrelimage.crop_image(img, x=10, y=10, width=40, height=30)
    result = info(out)
    assert (result["width"], result["height"]) == (40, 30)


def test_crop_anchor_center():
    img = noise(128, 96)
    out = dusrelimage.crop_image(img, width=64, height=48, anchor="center")
    result = info(out)
    assert (result["width"], result["height"]) == (64, 48)


def test_crop_anchor_corners():
    img = noise(128, 96)
    for anchor in ("top_left", "top_right", "bottom_left", "bottom_right"):
        out = dusrelimage.crop_image(img, width=32, height=32, anchor=anchor)
        assert (info(out)["width"], info(out)["height"]) == (32, 32)


# --- Tool 4: converter -----------------------------------------------------

def test_convert_each_format():
    img = noise(64, 64)
    for fmt in ("jpeg", "png", "webp", "gif", "bmp"):
        out = dusrelimage.convert_image(img, format=fmt)
        assert info(out)["format"] == fmt, fmt
    # AVIF output cannot be read back (AVIF input does not decode),
    # so verify the container magic instead.
    out = dusrelimage.convert_image(img, format="avif")
    assert out[4:8] == b"ftyp" and out[8:12] == b"avif", fmt


def test_convert_png_alpha_flattens_to_jpeg():
    # Noise PNGs are opaque RGB; flattening is exercised by the Rust
    # tests with RGBA inputs. Here: JPEG output has no alpha.
    img = noise(64, 64)
    out = dusrelimage.convert_image(img, format="jpeg", background="#FF0000")
    result = info(out)
    assert result["format"] == "jpeg"
    assert result["has_alpha"] is False


def test_convert_format_alias_jpg():
    img = noise(64, 64)
    out = dusrelimage.convert_image(img, format="jpg")
    assert info(out)["format"] == "jpeg"


# --- Tool 5: watermark -----------------------------------------------------

def test_watermark_text():
    img = noise(128, 96)
    out = dusrelimage.add_watermark(img, text="DRAFT")
    result = info(out)
    assert (result["width"], result["height"]) == (128, 96)
    assert isinstance(out, bytes)


def test_watermark_text_options():
    img = noise(128, 96)
    out = dusrelimage.add_watermark(
        img, text="SECRET", text_size=24.0, text_colour="#FF0000",
        opacity=0.5, position="top_left", rotation_deg=0.0, margin=8)
    assert info(out)["width"] == 128


def test_watermark_logo():
    img = noise(128, 96)
    logo = solid(32, 32, seed=3)
    out = dusrelimage.add_watermark(img, logo=logo, logo_scale=0.5,
                                    opacity=0.8, position="bottom_right")
    result = info(out)
    assert (result["width"], result["height"]) == (128, 96)
    # The logo is pasted bottom-right: that corner is no longer the
    # original noise pixel.
    assert out != img


def test_watermark_tiled():
    img = noise(128, 96)
    out = dusrelimage.add_watermark(img, text="X", position="tiled")
    assert info(out)["width"] == 128


def test_watermark_dataclass_options():
    img = noise(64, 64)
    opts = dusrelimage.WatermarkOptions(
        text=dusrelimage.TextWatermark(text="HI", size=20.0,
                                       colour=dusrelimage.Colour.from_hex("#00FF00")),
        opacity=0.7)
    out = dusrelimage.add_watermark(img, options=opts)
    assert info(out)["width"] == 64


# --- Tool 6: annotator -----------------------------------------------------

def test_annotate_rect_fills_pixels():
    img = solid(64, 64, seed=1)
    before = dusrelimage.pick_colour(img, 20, 20)
    out = dusrelimage.annotate_image(img, [
        {"type": "rect", "x": 10, "y": 10, "w": 30, "h": 30,
         "colour": "#FF0000", "width": 2,
         "fill": "#FF0000"},
    ])
    assert info(out)["width"] == 64
    assert dusrelimage.pick_colour(out, 20, 20)["hex"] == "#FF0000"
    assert dusrelimage.pick_colour(out, 20, 20) != before


def test_annotate_line_and_arrow():
    img = solid(64, 64, seed=1)
    out = dusrelimage.annotate_image(img, [
        {"type": "line", "from": [0, 0], "to": [63, 63],
         "colour": "#00FF00", "width": 2},
        {"type": "arrow", "from": [0, 63], "to": [63, 0],
         "colour": "#0000FF", "width": 2},
    ])
    assert info(out)["width"] == 64
    assert out != img


def test_annotate_ellipse_and_text():
    img = solid(64, 64, seed=1)
    out = dusrelimage.annotate_image(img, [
        {"type": "ellipse", "x": 10, "y": 10, "w": 40, "h": 30,
         "colour": "#FF00FF", "width": 2},
        {"type": "text", "x": 5, "y": 5, "text": "Hi",
         "size": 16.0, "colour": "#000000"},
    ])
    assert info(out)["width"] == 64


def test_annotate_serde_form():
    img = solid(64, 64, seed=1)
    out = dusrelimage.annotate_image(img, [
        {"Rect": {"x": 5, "y": 5, "w": 10, "h": 10,
                  "colour": {"hex": "#FF0000", "r": 255, "g": 0, "b": 0,
                             "a": 255, "hsl": [0.0, 1.0, 0.5]},
                  "width": 1, "fill": None}},
    ])
    assert info(out)["width"] == 64


# --- Tool 7: colour picker -------------------------------------------------

def test_pick_colour_solid():
    img = dusrelimage.make_test_image("solid", 32, 32, 7)
    colour = dusrelimage.pick_colour(img, 0, 0)
    assert colour == dusrelimage.pick_colour(img, 31, 31)
    assert colour["hex"].startswith("#") and len(colour["hex"]) == 7
    assert 0 <= colour["r"] <= 255
    assert colour["a"] == 255
    h, s, li = colour["hsl"]
    assert 0.0 <= h < 360.0 and 0.0 <= s <= 1.0 and 0.0 <= li <= 1.0


def test_pick_colour_known_solid():
    # A solid image built from seed 7 has one colour everywhere; the
    # picker must agree with the palette's top entry.
    img = solid(16, 16, seed=7)
    picked = dusrelimage.pick_colour(img, 5, 5)
    top = dusrelimage.extract_palette(img, 1)[0]
    assert picked["hex"] == top["hex"]
    assert picked["r"] == top["r"]


# --- Tool 8: palette -------------------------------------------------------

def test_extract_palette_solid_is_one_colour():
    img = solid(32, 32, seed=7)
    palette = dusrelimage.extract_palette(img, 4)
    assert len(palette) == 1
    assert palette[0]["share"] == 1.0
    assert palette[0]["hex"] == dusrelimage.pick_colour(img, 0, 0)["hex"]


def test_extract_palette_gradient_sorted_by_share():
    img = dusrelimage.make_test_image("gradient", 64, 64, 7)
    palette = dusrelimage.extract_palette(img, 5)
    assert 1 <= len(palette) <= 5
    shares = [entry["share"] for entry in palette]
    assert shares == sorted(shares, reverse=True)
    assert abs(sum(shares) - 1.0) < 0.01
    for entry in palette:
        assert entry["hex"].startswith("#")
        assert 0.0 < entry["share"] <= 1.0


def test_extract_palette_noise():
    img = noise(64, 64)
    palette = dusrelimage.extract_palette(img, 3)
    assert len(palette) == 3
    assert all(0.0 < entry["share"] <= 1.0 for entry in palette)


# --- Tool 9: collage -------------------------------------------------------

def test_collage_grid_default():
    imgs = [solid(64, 64, seed=s) for s in (1, 2, 3, 4)]
    out = dusrelimage.make_collage(imgs)
    result = info(out)
    assert (result["width"], result["height"]) == (1200, 1200)


def test_collage_layouts_and_size():
    imgs = [solid(64, 64, seed=s) for s in (1, 2, 3)]
    out = dusrelimage.make_collage(imgs, layout="row", width=900, height=300)
    assert (info(out)["width"], info(out)["height"]) == (900, 300)
    out = dusrelimage.make_collage(imgs, layout="column", width=300, height=900)
    assert (info(out)["width"], info(out)["height"]) == (300, 900)
    out = dusrelimage.make_collage(imgs, layout="2x2", width=400, height=400,
                                   gap=0, background="#000000")
    assert (info(out)["width"], info(out)["height"]) == (400, 400)


def test_collage_single_image():
    img = solid(64, 64, seed=1)
    out = dusrelimage.make_collage([img], layout="1x1", width=200, height=200)
    assert (info(out)["width"], info(out)["height"]) == (200, 200)


# --- Tool 10: pixelator ----------------------------------------------------

def test_pixelate_whole_image_changes_bytes():
    img = noise(64, 64)
    out = dusrelimage.pixelate_image(img, block_size=8)
    assert info(out)["width"] == 64
    assert out != img


def test_pixelate_rect_only():
    img = noise(64, 64)
    out = dusrelimage.pixelate_image(img, x=0, y=0, width=32, height=32,
                                     block_size=8)
    assert info(out)["width"] == 64
    # The untouched corner keeps its exact pixel: pixelation only
    # touched the rect.
    assert (dusrelimage.pick_colour(out, 63, 63)
            == dusrelimage.pick_colour(img, 63, 63))


def test_pixelate_block_sizes():
    img = noise(64, 64)
    for block in (2, 10, 100):
        out = dusrelimage.pixelate_image(img, block_size=block)
        assert info(out)["width"] == 64


# --- Info + fixtures + bulk ------------------------------------------------

def test_get_image_info():
    img = noise(128, 96)
    result = info(img)
    assert result == {
        "format": "png",
        "width": 128,
        "height": 96,
        "has_alpha": False,
        "colour_type": "rgb8",
        "file_size": len(img),
    }


def test_get_image_info_jpeg():
    img = noise(64, 64)
    jpeg = dusrelimage.convert_image(img, format="jpeg")
    result = info(jpeg)
    assert result["format"] == "jpeg"
    assert result["file_size"] == len(jpeg)


def test_fixtures_deterministic():
    a = dusrelimage.make_test_image("noise", 32, 32, 42)
    b = dusrelimage.make_test_image("noise", 32, 32, 42)
    assert a == b
    assert a[:8] == b"\x89PNG\r\n\x1a\n"
    assert dusrelimage.make_test_image("solid", 8, 8, 1) != \
        dusrelimage.make_test_image("solid", 8, 8, 2)
    assert dusrelimage.make_test_image("gradient", 8, 8, 1)[:8] == \
        b"\x89PNG\r\n\x1a\n"


def test_bulk_resize_all_ok():
    imgs = [noise(64, 64, seed=s) for s in (1, 2, 3)]
    items = dusrelimage.bulk_process(imgs, "resize", options={"width": 32})
    assert len(items) == 3
    assert [item["index"] for item in items] == [0, 1, 2]
    for item in items:
        assert item["error_code"] is None
        assert info(item["output"])["width"] == 32


def test_bulk_partial_failure_never_aborts():
    good = noise(64, 64)
    items = dusrelimage.bulk_process(
        [good, b"not an image", good], "resize", options={"width": 32})
    assert len(items) == 3
    assert info(items[0]["output"])["width"] == 32
    assert items[1]["output"] is None
    assert items[1]["error_code"] == "INVALID_IMAGE"
    assert items[1]["error"] != ""
    assert info(items[2]["output"])["width"] == 32


def test_bulk_compress_to_size():
    imgs = [noise(128, 128, seed=s) for s in (1, 2)]
    items = dusrelimage.bulk_process(
        imgs, "compress_to_size", target_bytes=8 * 1024,
        options={"format": "jpeg"})
    assert len(items) == 2
    for item in items:
        assert item["error_code"] is None
        assert len(item["output"]) <= 8 * 1024


def test_bulk_annotate():
    imgs = [solid(64, 64, seed=s) for s in (1, 2)]
    items = dusrelimage.bulk_process(imgs, "annotate", annotations=[
        {"type": "rect", "x": 5, "y": 5, "w": 10, "h": 10,
         "colour": "#FF0000", "width": 1, "fill": "#FF0000"},
    ])
    assert len(items) == 2
    for item in items:
        assert item["error_code"] is None
        assert dusrelimage.pick_colour(item["output"], 8, 8)["hex"] == "#FF0000"


def test_bulk_pick_colour_and_palette():
    imgs = [solid(32, 32, seed=s) for s in (1, 2)]
    items = dusrelimage.bulk_process(imgs, "pick_colour", x=5, y=5)
    assert len(items) == 2
    for item, img in zip(items, imgs):
        assert item["error_code"] is None
        assert item["output"]["hex"] == dusrelimage.pick_colour(img, 5, 5)["hex"]
    items = dusrelimage.bulk_process(imgs, "palette", count=2)
    assert len(items) == 2
    for item in items:
        assert item["error_code"] is None
        assert len(item["output"]) == 1
        assert item["output"][0]["share"] == 1.0


def test_bulk_collage_is_per_item_error():
    imgs = [solid(32, 32, seed=s) for s in (1, 2)]
    items = dusrelimage.bulk_process(imgs, "collage")
    assert len(items) == 2
    for item in items:
        assert item["output"] is None
        assert item["error_code"] == "INVALID_ARGUMENT"
