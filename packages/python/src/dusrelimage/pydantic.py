"""Optional Pydantic v2 models for dusrelimage (thin wrapper).

The models live in the wheel's ``dusrelimage.pydantic`` (built from
``crates/dusrelimage-py``); this module re-exports them from the
installed wheel when it is on ``sys.path``. Pydantic is never
required to import the main ``dusrelimage`` package.
"""

import glob
import importlib.util
import os
import sys

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


def _load_wheel_pydantic():
    here = os.path.dirname(os.path.abspath(__file__))
    candidates = []
    for entry in sys.path:
        if not entry:
            continue
        package_dir = os.path.join(entry, "dusrelimage")
        if os.path.abspath(package_dir) == os.path.abspath(here):
            continue
        candidates.extend(glob.glob(os.path.join(package_dir, "pydantic.py")))
    # Also try a neighbouring wheel install next to this file.
    for path in candidates:
        spec = importlib.util.spec_from_file_location(
            "dusrelimage._wheel_pydantic", path)
        if spec is None or spec.loader is None:
            continue
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        return module
    raise ImportError(
        "dusrelimage Pydantic models need the dusrelimage wheel: "
        "pip install 'dusrelimage[pydantic]'"
    )


_wheel = _load_wheel_pydantic()

Colour = _wheel.Colour
CommonOptions = _wheel.CommonOptions
CompressOptions = _wheel.CompressOptions
ResizeOptions = _wheel.ResizeOptions
CropOptions = _wheel.CropOptions
ConvertOptions = _wheel.ConvertOptions
TextWatermark = _wheel.TextWatermark
ImageWatermark = _wheel.ImageWatermark
WatermarkOptions = _wheel.WatermarkOptions
CollageOptions = _wheel.CollageOptions
PixelateOptions = _wheel.PixelateOptions
ImageInfo = _wheel.ImageInfo
PaletteColour = _wheel.PaletteColour
