"""dusrelimage — offline image toolkit (thin wrapper).

This is the source-tree wrapper around the compiled ``dusrelimage``
extension. The image work lives in the Rust core, reached through
``dusrelimage._native`` (shipped by the maturin wheel built from
``crates/dusrelimage-py``); this module re-exports that native
module's functions with the same ``DusrelError`` translation as the
wheel's own package, accepting either an ``options`` dict or keyword
arguments (kwargs win).

Install the wheel (``pip install dusrelimage``) — this wrapper on
its own has no image code in it. In a development checkout the
native module is found in the installed wheel on ``sys.path``.
"""

from __future__ import annotations

import dataclasses
import glob
import importlib.util
import os
import sys

__version__ = "0.1.0"

__all__ = [
    "DusrelError",
    "compress_image",
    "compress_to_size",
    "resize_image",
    "crop_image",
    "convert_image",
    "add_watermark",
    "annotate_image",
    "pick_colour",
    "extract_palette",
    "make_collage",
    "pixelate_image",
    "get_image_info",
    "bulk_process",
    "make_test_image",
    "__version__",
]


def _load_native():
    """Find the compiled ``dusrelimage._native`` extension.

    First next to this file (the maturin wheel installed over this
    tree), then anywhere else on ``sys.path`` (development checkout
    with the wheel installed in the environment).
    """
    try:
        from . import _native  # noqa: F401
        return _native
    except ImportError:
        pass
    here = os.path.dirname(os.path.abspath(__file__))
    for entry in sys.path:
        if not entry:
            continue
        package_dir = os.path.join(entry, "dusrelimage")
        if os.path.abspath(package_dir) == os.path.abspath(here):
            continue
        for path in glob.glob(os.path.join(package_dir, "_native*.so")) + \
                glob.glob(os.path.join(package_dir, "_native*.pyd")):
            spec = importlib.util.spec_from_file_location(
                "dusrelimage._native", path)
            if spec is None or spec.loader is None:
                continue
            module = importlib.util.module_from_spec(spec)
            sys.modules.setdefault("dusrelimage._native", module)
            spec.loader.exec_module(module)
            return module
    raise ImportError(
        "The dusrelimage native module is not installed: "
        "pip install dusrelimage"
    )


_native = _load_native()


class DusrelError(Exception):
    """Every dusrelimage failure, with a stable machine-readable code."""

    def __init__(self, code, reason_en="", reason_hi=""):
        super().__init__(f"{code}: {reason_en}")
        self.code = code
        self.reason_en = reason_en
        self.reason_hi = reason_hi


def _from_native(exc):
    parts = str(exc).split("\x1f")
    if len(parts) == 3 and parts[0]:
        return DusrelError(parts[0], parts[1], parts[2])
    return DusrelError(
        "OPERATION_FAILED",
        f"Operation failed: {exc}",
        f"Operation fail hua: {exc}",
    )


def _call(fn, *args):
    try:
        return fn(*args)
    except DusrelError:
        raise
    except ValueError as exc:
        raise _from_native(exc) from None
    except TypeError:
        raise DusrelError(
            "INVALID_TYPE",
            "Wrong argument type passed to the native module.",
            "Native module ko galat type ka argument mila.",
        ) from None
    except Exception as exc:
        raise DusrelError(
            "OPERATION_FAILED",
            f"Operation failed: {exc}",
            f"Operation fail hua: {exc}",
        ) from None


def _as_bytes(value, name):
    if isinstance(value, (bytes, bytearray, memoryview)):
        return bytes(value)
    raise DusrelError(
        "INVALID_TYPE",
        f"{name} must be bytes, bytearray or memoryview.",
        f"{name} bytes, bytearray ya memoryview hona chahiye.",
    )


def _options_to_dict(options):
    """Accept a dict, a dataclass instance or a Pydantic model."""
    if options is None:
        return {}
    if isinstance(options, dict):
        return dict(options)
    if hasattr(options, "model_dump"):  # Pydantic v2 model
        try:
            return dict(options.model_dump(mode="json"))
        except TypeError:
            return dict(options.model_dump())
    if dataclasses.is_dataclass(options) and not isinstance(options, type):
        if hasattr(options, "to_dict"):
            return dict(options.to_dict())
        return dataclasses.asdict(options)
    raise DusrelError(
        "INVALID_TYPE",
        "options must be a dict, an options dataclass, or a Pydantic model.",
        "options dict, options dataclass ya Pydantic model hona chahiye.",
    )


def _merge(options, kwargs):
    """Options dict overlaid with explicit kwargs (kwargs win)."""
    merged = _options_to_dict(options)
    merged.update({k: v for k, v in kwargs.items() if v is not None})
    return merged


def compress_image(image, options=None, **kwargs):
    """Re-encode at ``quality`` (1-100, default 80). See the wheel's README."""
    return _call(_native.compress_image, _as_bytes(image, "image"),
                 _merge(options, kwargs))


def compress_to_size(image, target_bytes, options=None, **kwargs):
    """Re-encode so the result is at most ``target_bytes`` bytes."""
    return _call(_native.compress_to_size, _as_bytes(image, "image"),
                 target_bytes, _merge(options, kwargs))


def resize_image(image, options=None, **kwargs):
    """Resize: ``width`` / ``height`` / ``percent``, fit, filter."""
    return _call(_native.resize_image, _as_bytes(image, "image"),
                 _merge(options, kwargs))


def crop_image(image, options=None, **kwargs):
    """Crop to ``x`` / ``y`` / ``width`` / ``height`` (or ``anchor``)."""
    return _call(_native.crop_image, _as_bytes(image, "image"),
                 _merge(options, kwargs))


def convert_image(image, options=None, **kwargs):
    """Convert to ``format`` (``"jpeg"`` / ``"png"`` / ...)."""
    return _call(_native.convert_image, _as_bytes(image, "image"),
                 _merge(options, kwargs))


def add_watermark(image, options=None, **kwargs):
    """Stamp a text and/or logo watermark (``text`` / ``logo``)."""
    return _call(_native.add_watermark, _as_bytes(image, "image"),
                 _merge(options, kwargs))


def annotate_image(image, annotations, options=None, **kwargs):
    """Draw shapes / text. ``annotations`` is a list of dicts."""
    # The native side takes no options struct for annotate; kwargs
    # are accepted for uniformity and must be empty.
    if options or kwargs:
        raise DusrelError(
            "INVALID_ARGUMENT",
            "annotate_image takes no options.",
            "annotate_image me koi options nahi hote.",
        )
    if not isinstance(annotations, (list, tuple)):
        raise DusrelError(
            "INVALID_TYPE",
            "annotations must be a list of dicts.",
            "annotations dicts ki list honi chahiye.",
        )
    return _call(_native.annotate_image, _as_bytes(image, "image"),
                 list(annotations))


def pick_colour(image, x, y):
    """Read the pixel at (``x``, ``y``): ``{hex, r, g, b, a, hsl}``."""
    result = _call(_native.pick_colour, _as_bytes(image, "image"), x, y)
    result["hsl"] = tuple(result["hsl"])
    return result


def extract_palette(image, count=5):
    """Dominant colours, most frequent first, with ``share`` (0-1)."""
    results = _call(_native.extract_palette, _as_bytes(image, "image"), count)
    for entry in results:
        entry["hsl"] = tuple(entry["hsl"])
    return results


def make_collage(images, options=None, **kwargs):
    """Build a collage from several images."""
    if not isinstance(images, (list, tuple)):
        raise DusrelError(
            "INVALID_TYPE",
            "images must be a list of bytes.",
            "images bytes ki list honi chahiye.",
        )
    return _call(_native.make_collage,
                 [_as_bytes(item, "images") for item in images],
                 _merge(options, kwargs))


def pixelate_image(image, options=None, **kwargs):
    """Pixelate a rectangle (default: the whole image)."""
    return _call(_native.pixelate_image, _as_bytes(image, "image"),
                 _merge(options, kwargs))


def get_image_info(image, options=None, **kwargs):
    """Describe an image: ``{format, width, height, has_alpha,
    colour_type, file_size}``."""
    return _call(_native.get_image_info, _as_bytes(image, "image"),
                 _merge(options, kwargs))


def bulk_process(images, op, options=None, target_bytes=None,
                 annotations=None, x=None, y=None, count=None, **kwargs):
    """Run one op over many images; failures reported per item."""
    if not isinstance(images, (list, tuple)):
        raise DusrelError(
            "INVALID_TYPE",
            "images must be a list of bytes.",
            "images bytes ki list honi chahiye.",
        )
    return _call(
        _native.bulk_process,
        [_as_bytes(item, "images") for item in images],
        op,
        _merge(options, kwargs),
        target_bytes,
        None if annotations is None else list(annotations),
        x,
        y,
        count,
    )


def make_test_image(kind="noise", width=64, height=64, seed=42):
    """Deterministic test PNG: ``"solid"`` / ``"gradient"`` / ``"noise"``."""
    return _call(_native.make_test_image, kind, width, height, seed)
