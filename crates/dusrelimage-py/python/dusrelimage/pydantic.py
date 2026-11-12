"""Optional Pydantic v2 models for dusrelimage.

Pydantic is **never** required to import or use the main
``dusrelimage`` package. This module is pure Python — it does not
import the native extension — and it imports cleanly without Pydantic
installed; the models below raise an informative ``ImportError`` only
when actually instantiated without Pydantic (the indycheck lazy
pattern).

Usage (requires ``pip install dusrelimage[pydantic]``)::

    from dusrelimage.pydantic import CompressOptions, ResizeOptions

    opts = CompressOptions(quality=90, format="webp")
    out = dusrelimage.compress_image(png, options=opts)

    class ResizeRequest(BaseModel):
        image: bytes
        options: ResizeOptions = ResizeOptions()

Field names are snake_case and match the Rust core options exactly,
with the same defaults. ``common`` is nested, exactly like the Rust
structs (``CompressOptions { common: CommonOptions, ... }``); the
friendly spellings (``"jpeg"``, ``"fit"``, ``"lanczos3"``,
``"bottom_right"``, ``"#RRGGBB"``) are normalised by the main
package before they reach the native module.
"""

from typing import Literal

try:
    from pydantic import BaseModel, Field, model_validator
    _HAS_PYDANTIC = True
except ImportError:  # pragma: no cover - depends on env
    _HAS_PYDANTIC = False

    class BaseModel:  # type: ignore[no-redef]
        """Placeholder: raises a helpful error only when used."""

        def __init__(self, *args, **kwargs):
            raise ImportError(
                "dusrelimage Pydantic models need Pydantic v2: "
                "pip install 'dusrelimage[pydantic]'"
            )

    def Field(default=None, **kwargs):  # type: ignore[no-redef]  # noqa: N802
        return default

    def model_validator(*args, **kwargs):  # type: ignore[no-redef]
        def wrap(fn):
            return fn

        if args and callable(args[0]):
            return args[0]
        return wrap


__all__ = [
    "Colour",
    "CommonOptions",
    "CompressOptions",
    "ResizeOptions",
    "CropOptions",
    "ConvertOptions",
    "TextWatermark",
    "ImageWatermark",
    "WatermarkOptions",
    "CollageOptions",
    "PixelateOptions",
    "ImageInfo",
    "PaletteColour",
]

# ---------------------------------------------------------------------------
# Shared field types
# ---------------------------------------------------------------------------

ImageFormatName = Literal["jpeg", "jpg", "png", "webp", "avif", "gif", "bmp"]
FitModeName = Literal["fit", "fill", "stretch", "crop"]
FilterName = Literal["nearest", "triangle", "catmull-rom", "gaussian", "lanczos3"]
PositionName = Literal[
    "top_left", "top_center", "top_right",
    "center_left", "center", "center_right",
    "bottom_left", "bottom_center", "bottom_right",
    "tiled",
]
CollageLayoutName = Literal["grid", "row", "column"]


def _rgb_to_hsl(r, g, b):
    r, g, b = r / 255.0, g / 255.0, b / 255.0
    mx, mn = max(r, g, b), min(r, g, b)
    light = (mx + mn) / 2.0
    if mx == mn:
        return (0.0, 0.0, light)
    d = mx - mn
    sat = d / (2.0 - mx - mn) if light > 0.5 else d / (mx + mn)
    if mx == r:
        hue = 60.0 * (((g - b) / d) % 6.0)
    elif mx == g:
        hue = 60.0 * ((b - r) / d + 2.0)
    else:
        hue = 60.0 * ((r - g) / d + 4.0)
    return (hue, sat, light)


# ---------------------------------------------------------------------------
# Models — defaults mirror crates/dusrelimage/src/options.rs exactly.
# ---------------------------------------------------------------------------

class Colour(BaseModel):
    """An 8-bit RGBA colour; ``hex`` / ``hsl`` fill in from ``r/g/b``."""

    r: int = 0
    g: int = 0
    b: int = 0
    a: int = 255
    hex: str | None = None
    hsl: tuple[float, float, float] | None = None

    if _HAS_PYDANTIC:

        @model_validator(mode="after")
        def _fill_derived(self):
            if self.hex is None:
                self.hex = f"#{self.r:02X}{self.g:02X}{self.b:02X}"
            if self.hsl is None:
                self.hsl = _rgb_to_hsl(self.r, self.g, self.b)
            return self


class CommonOptions(BaseModel):
    """Options shared by every operation."""

    auto_orient: bool = True
    keep_exif: bool = False
    dpi: tuple[int, int] | None = None
    max_pixels: int = 120_000_000
    max_dimension: int = 16000


class CompressOptions(BaseModel):
    """Options for ``compress_image`` / ``compress_to_size``."""

    common: CommonOptions = Field(default_factory=CommonOptions)
    quality: int = Field(default=80, ge=1, le=100)
    format: ImageFormatName | None = None
    progressive: bool = False
    lossless: bool = False
    effort: int = Field(default=4, ge=0, le=6)


class ResizeOptions(BaseModel):
    """Options for ``resize_image``."""

    common: CommonOptions = Field(default_factory=CommonOptions)
    width: int | None = None
    height: int | None = None
    percent: float | None = None
    fit: FitModeName = "fit"
    filter: FilterName = "lanczos3"
    background: Colour = Field(default_factory=lambda: Colour(r=255, g=255, b=255))


class CropOptions(BaseModel):
    """Options for ``crop_image``."""

    common: CommonOptions = Field(default_factory=CommonOptions)
    x: int = 0
    y: int = 0
    width: int = 0
    height: int = 0
    anchor: PositionName | None = None


class ConvertOptions(BaseModel):
    """Options for ``convert_image``."""

    common: CommonOptions = Field(default_factory=CommonOptions)
    format: ImageFormatName = "jpeg"
    quality: int = Field(default=80, ge=1, le=100)
    background: Colour = Field(default_factory=lambda: Colour(r=255, g=255, b=255))
    progressive: bool = False


class TextWatermark(BaseModel):
    """A text watermark: string, size in px, colour."""

    text: str = ""
    size: float = 48.0
    colour: Colour = Field(default_factory=lambda: Colour(r=255, g=255, b=255))


class ImageWatermark(BaseModel):
    """A logo watermark: raw image bytes + a scale factor."""

    image: bytes = b""
    scale: float = 1.0


class WatermarkOptions(BaseModel):
    """Options for ``add_watermark`` (text and/or logo)."""

    common: CommonOptions = Field(default_factory=CommonOptions)
    text: TextWatermark | None = None
    logo: ImageWatermark | None = None
    opacity: float = Field(default=0.5, ge=0.0, le=1.0)
    position: PositionName = "bottom_right"
    rotation_deg: float = 0.0
    margin: int = 16


class CollageOptions(BaseModel):
    """Options for ``make_collage``."""

    common: CommonOptions = Field(default_factory=CommonOptions)
    layout: CollageLayoutName | str | dict = "grid"
    gap: int = 8
    background: Colour = Field(default_factory=lambda: Colour(r=255, g=255, b=255))
    width: int = 1200
    height: int = 1200
    cell_fit: FitModeName = "fit"


class PixelateOptions(BaseModel):
    """Options for ``pixelate_image``."""

    common: CommonOptions = Field(default_factory=CommonOptions)
    x: int = 0
    y: int = 0
    width: int = 0
    height: int = 0
    block_size: int = Field(default=10, ge=2, le=100)


class ImageInfo(BaseModel):
    """Summary of an image, from ``get_image_info``."""

    format: str
    width: int
    height: int
    has_alpha: bool
    colour_type: str
    file_size: int


class PaletteColour(BaseModel):
    """One palette entry: a dominant colour and its share (0-1)."""

    hex: str
    r: int
    g: int
    b: int
    a: int = 255
    hsl: tuple[float, float, float]
    share: float
