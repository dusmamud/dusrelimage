"""dusrelimage — offline image toolkit (Python).

Compress, resize, crop, convert, watermark, annotate, colour picker,
palette, collage and pixelate. Every operation takes bytes in and
returns bytes out, and every failure raises :class:`DusrelError`
with a stable ``code`` plus ``reason_en`` / ``reason_hi``
(Roman-script Hinglish) — never a bare ``TypeError``.

The heavy lifting is the Rust core, reached through the compiled
``dusrelimage._native`` extension module. This module is the friendly
Python API on top of it: keyword arguments mirroring the Rust option
fields, an ``options`` dict (or dataclass / Pydantic model) override,
and input-type validation before any native call.

Honest limits (v0.1.0, same as the Rust core): AVIF input does not
decode (``UNSUPPORTED_FORMAT``; AVIF output works, lossy 8-bit);
WebP output is always lossless (quality / effort ignored for WebP);
``keep_exif`` / ``dpi`` / ``progressive`` are accepted but not
honoured yet; ``compress_to_size`` quality search is JPEG/AVIF-only
(PNG/WebP/GIF/BMP take one best-effort encode); TIFF input decodes
but falls back to PNG output; GIF output is the first frame only for
animated input; whole images live in memory.
"""

from __future__ import annotations

import dataclasses
from dataclasses import dataclass, field

try:
    from . import _native
except ImportError:  # pragma: no cover - built wheel always ships it
    _native = None  # type: ignore[assignment]

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
    "default_options",
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
    "__version__",
]


# ---------------------------------------------------------------------------
# Errors
# ---------------------------------------------------------------------------

class DusrelError(Exception):
    """Every dusrelimage failure, with a stable machine-readable code."""

    def __init__(self, code, reason_en="", reason_hi=""):
        super().__init__(f"{code}: {reason_en}")
        self.code = code
        self.reason_en = reason_en
        self.reason_hi = reason_hi


def _invalid_type(what_en, what_hi):
    return DusrelError("INVALID_TYPE", what_en, what_hi)


def _invalid_argument(what_en, what_hi):
    return DusrelError("INVALID_ARGUMENT", what_en, what_hi)


def _from_native(exc):
    """Parse the native `CODE\\x1f reason_en \\x1f reason_hi` payload."""
    parts = str(exc).split("\x1f")
    if len(parts) == 3 and parts[0]:
        return DusrelError(parts[0], parts[1], parts[2])
    return DusrelError(
        "OPERATION_FAILED",
        f"Operation failed: {exc}",
        f"Operation fail hua: {exc}",
    )


def _call(fn, *args):
    """Call a native function, translating every failure to DusrelError."""
    if _native is None:  # pragma: no cover
        raise DusrelError(
            "OPERATION_FAILED",
            "The dusrelimage native module is not available in this install.",
            "Is install me dusrelimage ka native module nahi hai.",
        )
    try:
        return fn(*args)
    except DusrelError:
        raise
    except ValueError as exc:
        raise _from_native(exc) from None
    except TypeError:
        raise _invalid_type(
            "Wrong argument type passed to the native module.",
            "Native module ko galat type ka argument mila.",
        ) from None
    except Exception as exc:  # native panic / unexpected failure
        raise DusrelError(
            "OPERATION_FAILED",
            f"Operation failed: {exc}",
            f"Operation fail hua: {exc}",
        ) from None


# ---------------------------------------------------------------------------
# Option resolution: kwargs win over the `options` dict, which wins over
# the Rust defaults. A keyword left at _UNSET means "not specified".
# ---------------------------------------------------------------------------

class _Unset:
    def __repr__(self):
        return "<unset>"


_UNSET = _Unset()

#: Fields of CommonOptions, accepted flat on every op and hoisted
#: into the nested "common" dict the Rust structs expect.
COMMON_KEYS = ("auto_orient", "keep_exif", "dpi", "max_pixels", "max_dimension")

#: Ops bulk_process can run. "collage" is accepted but the core
#: answers it per item with INVALID_ARGUMENT (a collage combines all
#: inputs — call make_collage once instead). "info" is not batchable.
BULK_OPS = (
    "compress",
    "compress_to_size",
    "resize",
    "crop",
    "convert",
    "watermark",
    "annotate",
    "pick_colour",
    "palette",
    "collage",
    "pixelate",
)


def default_options():
    """Default shared options: auto-orient on, metadata stripped."""
    return {
        "auto_orient": True,
        "keep_exif": False,
        "dpi": None,
        "max_pixels": 120_000_000,
        "max_dimension": 16000,
    }


def _options_to_dict(options):
    """Accept a dict, a dataclass instance or a Pydantic model."""
    if options is None:
        return {}
    if isinstance(options, dict):
        return dict(options)
    if dataclasses.is_dataclass(options) and not isinstance(options, type):
        if hasattr(options, "to_dict"):
            return dict(options.to_dict())
        return dataclasses.asdict(options)
    if hasattr(options, "model_dump"):  # Pydantic v2 model
        try:
            return dict(options.model_dump(mode="json"))
        except TypeError:
            return dict(options.model_dump())
    raise _invalid_type(
        "options must be a dict, an options dataclass, or a Pydantic model.",
        "options dict, options dataclass ya Pydantic model hona chahiye.",
    )


def _resolve_kwargs(kwargs, options):
    """Merge explicit kwargs over the options dict. _UNSET = not given."""
    resolved = _options_to_dict(options)
    for key, value in kwargs.items():
        if value is not _UNSET:
            resolved[key] = value
    return resolved


# ---------------------------------------------------------------------------
# Input validation (Python layer — the indycheck v0.2.1 lesson: wrong
# input types raise DusrelError("INVALID_TYPE"), never TypeError).
# ---------------------------------------------------------------------------

def _as_bytes(value, name):
    if isinstance(value, (bytes, bytearray, memoryview)):
        return bytes(value)
    raise _invalid_type(
        f"{name} must be bytes, bytearray or memoryview.",
        f"{name} bytes, bytearray ya memoryview hona chahiye.",
    )


def _as_bytes_list(value, name):
    if not isinstance(value, (list, tuple)):
        raise _invalid_type(
            f"{name} must be a list of bytes.",
            f"{name} bytes ki list honi chahiye.",
        )
    return [_as_bytes(item, name) for item in value]


def _is_int(value):
    return isinstance(value, int) and not isinstance(value, bool)


def _as_int(value, name, minimum=None):
    if not _is_int(value):
        raise _invalid_type(
            f"{name} must be an integer.",
            f"{name} integer hona chahiye.",
        )
    if minimum is not None and value < minimum:
        raise _invalid_argument(
            f"{name} must be at least {minimum}.",
            f"{name} kam se kam {minimum} hona chahiye.",
        )
    return value


def _as_number(value, name):
    if isinstance(value, (int, float)) and not isinstance(value, bool):
        return value
    raise _invalid_type(
        f"{name} must be a number.",
        f"{name} number hona chahiye.",
    )


# ---------------------------------------------------------------------------
# Colours — accepted as "#RRGGBB" / "#RGB" / "#RRGGBBAA", (r, g, b),
# (r, g, b, a), a dict, a Colour dataclass or a Pydantic Colour; always
# normalised to the full dict {hex, r, g, b, a, hsl} the core expects.
# ---------------------------------------------------------------------------

def _rgb_to_hsl(r, g, b):
    r, g, b = r / 255.0, g / 255.0, b / 255.0
    mx, mn = max(r, g, b), min(r, g, b)
    light = (mx + mn) / 2.0
    if mx == mn:
        return [0.0, 0.0, light]
    d = mx - mn
    sat = d / (2.0 - mx - mn) if light > 0.5 else d / (mx + mn)
    if mx == r:
        hue = 60.0 * (((g - b) / d) % 6.0)
    elif mx == g:
        hue = 60.0 * ((b - r) / d + 2.0)
    else:
        hue = 60.0 * ((r - g) / d + 4.0)
    return [hue, sat, light]


def _colour_dict(r, g, b, a=255):
    return {
        "hex": f"#{r:02X}{g:02X}{b:02X}",
        "r": r,
        "g": g,
        "b": b,
        "a": a,
        "hsl": _rgb_to_hsl(r, g, b),
    }


def _parse_hex_colour(text):
    s = text.strip()
    if s.startswith("#"):
        s = s[1:]
    if len(s) not in (3, 6, 8) or any(c not in "0123456789abcdefABCDEF" for c in s):
        raise _invalid_argument(
            f"\"{text}\" is not a colour. Use \"#RRGGBB\".",
            f"\"{text}\" colour nahi hai. \"#RRGGBB\" use karein.",
        )
    if len(s) == 3:
        s = "".join(c * 2 for c in s)
    r, g, b = int(s[0:2], 16), int(s[2:4], 16), int(s[4:6], 16)
    a = int(s[6:8], 16) if len(s) == 8 else 255
    return _colour_dict(r, g, b, a)


def _as_colour(value, name="colour"):
    """Normalise any accepted colour form to the full colour dict."""
    if value is None:
        return None
    if isinstance(value, Colour):
        return value.to_dict()
    if hasattr(value, "model_dump"):  # Pydantic Colour
        value = value.model_dump(mode="json")
    if isinstance(value, str):
        return _parse_hex_colour(value)
    if isinstance(value, (list, tuple)):
        if len(value) not in (3, 4) or any(
            not _is_int(c) or c < 0 or c > 255 for c in value
        ):
            raise _invalid_argument(
                f"{name} list must be [r, g, b] or [r, g, b, a] with 0-255 values.",
                f"{name} list [r, g, b] ya [r, g, b, a] honi chahiye, 0-255 values ke saath.",
            )
        return _colour_dict(*value) if len(value) == 4 else _colour_dict(
            value[0], value[1], value[2]
        )
    if isinstance(value, dict):
        if all(k in value for k in ("r", "g", "b")):
            for k in ("r", "g", "b", "a"):
                if k in value and value[k] is not None:
                    if not _is_int(value[k]) or value[k] < 0 or value[k] > 255:
                        raise _invalid_argument(
                            f"{name} channel {k!r} must be 0-255.",
                            f"{name} channel {k!r} 0-255 ke beech hona chahiye.",
                        )
            return _colour_dict(
                value["r"], value["g"], value["b"], value.get("a") or 255
            )
        if "hex" in value and isinstance(value["hex"], str):
            return _parse_hex_colour(value["hex"])
        raise _invalid_argument(
            f"{name} dict needs {{r, g, b}} or {{hex: \"#RRGGBB\"}}.",
            f"{name} dict me {{r, g, b}} ya {{hex: \"#RRGGBB\"}} chahiye.",
        )
    raise _invalid_type(
        f"{name} must be \"#RRGGBB\", [r, g, b], a dict, or a Colour.",
        f"{name} \"#RRGGBB\", [r, g, b], dict ya Colour hona chahiye.",
    )


def _white():
    return _colour_dict(255, 255, 255)


# ---------------------------------------------------------------------------
# Enum spellings — lowercase / kebab / snake accepted, canonical
# serde names produced ("jpeg" -> "Jpeg", "bottom-right" -> "BottomRight").
# ---------------------------------------------------------------------------

_FORMATS = {
    "jpeg": "Jpeg", "jpg": "Jpeg", "png": "Png", "webp": "WebP",
    "avif": "Avif", "gif": "Gif", "bmp": "Bmp",
}
_FITS = {"fit": "Fit", "fill": "Fill", "stretch": "Stretch", "crop": "Crop"}
_FILTERS = {
    "nearest": "Nearest", "triangle": "Triangle",
    "catmull-rom": "CatmullRom", "catmullrom": "CatmullRom",
    "catmull_rom": "CatmullRom", "gaussian": "Gaussian",
    "lanczos3": "Lanczos3",
}
_POSITIONS = {
    "top-left": "TopLeft", "top_left": "TopLeft", "topleft": "TopLeft",
    "top-center": "TopCenter", "top_center": "TopCenter",
    "top-centre": "TopCenter", "topcentre": "TopCenter",
    "top-right": "TopRight", "top_right": "TopRight", "topright": "TopRight",
    "center-left": "CenterLeft", "center_left": "CenterLeft",
    "centre-left": "CenterLeft", "centerleft": "CenterLeft",
    "center": "Center", "centre": "Center",
    "center-right": "CenterRight", "center_right": "CenterRight",
    "centre-right": "CenterRight", "centerright": "CenterRight",
    "bottom-left": "BottomLeft", "bottom_left": "BottomLeft",
    "bottomleft": "BottomLeft",
    "bottom-center": "BottomCenter", "bottom_center": "BottomCenter",
    "bottom-centre": "BottomCenter", "bottomcenter": "BottomCenter",
    "bottom-right": "BottomRight", "bottom_right": "BottomRight",
    "bottomright": "BottomRight",
    "tiled": "Tiled",
}


def _norm_enum(value, table, name):
    if value is None:
        return None
    if not isinstance(value, str):
        raise _invalid_type(
            f"{name} must be a string.",
            f"{name} string hona chahiye.",
        )
    key = value.strip().lower()
    if key in table:
        return table[key]
    raise _invalid_argument(
        f"Unknown {name} \"{value}\". Use one of: {', '.join(sorted(table))}.",
        f"Unknown {name} \"{value}\" hai. Inme se ek use karein: {', '.join(sorted(table))}.",
    )


def _norm_layout(value):
    """Layout to the serde shape: "Row" / "Column" / {"Grid": {...}}."""
    if value is None:
        return None
    if isinstance(value, dict):
        if "Grid" in value:
            inner = value["Grid"]
            if not isinstance(inner, dict):
                raise _invalid_argument(
                    "layout {\"Grid\": ...} needs {\"rows\": r, \"cols\": c}.",
                    "layout {\"Grid\": ...} me {\"rows\": r, \"cols\": c} chahiye.",
                )
            return {"Grid": _norm_grid(inner)}
        if "grid" in value:
            inner = value["grid"]
            if not isinstance(inner, dict):
                raise _invalid_argument(
                    "layout {\"grid\": ...} needs {\"rows\": r, \"cols\": c}.",
                    "layout {\"grid\": ...} me {\"rows\": r, \"cols\": c} chahiye.",
                )
            return {"Grid": _norm_grid(inner)}
        if "rows" in value and "cols" in value:
            return {"Grid": _norm_grid(value)}
        raise _invalid_argument(
            "layout dict needs {\"rows\": r, \"cols\": c}.",
            "layout dict me {\"rows\": r, \"cols\": c} chahiye.",
        )
    if isinstance(value, str):
        lower = value.strip().lower()
        if lower == "row":
            return "Row"
        if lower == "column":
            return "Column"
        if lower == "grid":
            return {"Grid": {"rows": 2, "cols": 2}}
        if "x" in lower:
            parts = lower.split("x")
            if len(parts) == 2:
                try:
                    rows, cols = int(parts[0]), int(parts[1])
                except ValueError:
                    rows = cols = 0
                if 1 <= rows <= 4 and 1 <= cols <= 4:
                    return {"Grid": {"rows": rows, "cols": cols}}
        raise _invalid_argument(
            "layout must be \"grid\", \"row\", \"column\" or \"RxC\" like \"2x2\".",
            "layout \"grid\", \"row\", \"column\" ya \"2x2\" jaisa \"RxC\" hona chahiye.",
        )
    raise _invalid_type(
        "layout must be a string or a dict.",
        "layout string ya dict hona chahiye.",
    )


def _norm_grid(inner):
    rows = inner.get("rows")
    cols = inner.get("cols")
    if not _is_int(rows) or not _is_int(cols) or not 1 <= rows <= 4 or not 1 <= cols <= 4:
        raise _invalid_argument(
            "collage grid rows and columns must each be 1-4.",
            "collage grid ki rows aur columns 1-4 ke beech honi chahiye.",
        )
    return {"rows": rows, "cols": cols}


# ---------------------------------------------------------------------------
# Annotations — friendly {"type": "rect", ...} or the serde form
# {"Rect": {...}}; colours normalised, coordinates kept as [x, y].
# ---------------------------------------------------------------------------

_ANNOTATION_KINDS = ("arrow", "rect", "ellipse", "line", "text")
_ANNOTATION_VARIANTS = ("Arrow", "Rect", "Ellipse", "Line", "Text")
_ANNOTATION_REQUIRED = {
    "Arrow": ("from", "to", "colour", "width"),
    "Rect": ("x", "y", "w", "h", "colour", "width"),
    "Ellipse": ("x", "y", "w", "h", "colour", "width"),
    "Line": ("from", "to", "colour", "width"),
    "Text": ("x", "y", "text", "size", "colour"),
}


def _as_point(value, name):
    if isinstance(value, (list, tuple)) and len(value) == 2 and all(
        _is_int(c) for c in value
    ):
        return [value[0], value[1]]
    raise _invalid_argument(
        f"{name} must be [x, y] with integer coordinates.",
        f"{name} [x, y] hona chahiye, integer coordinates ke saath.",
    )


def _normalize_annotation(item):
    if not isinstance(item, dict):
        raise _invalid_type(
            "each annotation must be a dict.",
            "har annotation dict honi chahiye.",
        )
    item = dict(item)
    kind = item.pop("type", item.pop("kind", None))
    if kind is not None:
        if not isinstance(kind, str) or kind.strip().lower() not in _ANNOTATION_KINDS:
            raise _invalid_argument(
                f"Unknown annotation type \"{kind}\". Use: {', '.join(_ANNOTATION_KINDS)}.",
                f"Unknown annotation type \"{kind}\" hai. Use karein: {', '.join(_ANNOTATION_KINDS)}.",
            )
        variant = kind.strip().lower().capitalize()
        if variant == "Rect":
            variant = "Rect"
        body = item
    else:
        matches = [v for v in _ANNOTATION_VARIANTS if v in item]
        if len(matches) != 1:
            raise _invalid_argument(
                "each annotation needs \"type\" or exactly one of "
                "Arrow/Rect/Ellipse/Line/Text.",
                "har annotation me \"type\" ya Arrow/Rect/Ellipse/Line/Text me se ek chahiye.",
            )
        variant = matches[0]
        body = item[variant]
        if not isinstance(body, dict):
            raise _invalid_type(
                f"annotation {variant!r} body must be a dict.",
                f"annotation {variant!r} body dict honi chahiye.",
            )
        body = dict(body)
    for req in _ANNOTATION_REQUIRED[variant]:
        if req not in body:
            raise _invalid_argument(
                f"{variant} annotation needs {req!r}.",
                f"{variant} annotation ke liye {req!r} chahiye.",
            )
    if "colour" in body:
        body["colour"] = _as_colour(body["colour"], "annotation colour")
    if "fill" in body:
        body["fill"] = _as_colour(body["fill"], "annotation fill")
    for point_key in ("from", "to"):
        if point_key in body:
            body[point_key] = _as_point(body[point_key], f"annotation {point_key!r}")
    for int_key in ("x", "y", "w", "h", "width"):
        if int_key in body:
            _as_int(body[int_key], f"annotation {int_key!r}", minimum=0)
    if "size" in body:
        _as_number(body["size"], "annotation size")
    if "text" in body and not isinstance(body["text"], str):
        raise _invalid_type(
            "annotation text must be a string.",
            "annotation text string hona chahiye.",
        )
    return {variant: body}


def _normalize_annotations(annotations):
    if not isinstance(annotations, (list, tuple)):
        raise _invalid_type(
            "annotations must be a list of dicts.",
            "annotations dicts ki list honi chahiye.",
        )
    if len(annotations) == 0:
        raise _invalid_argument(
            "annotate needs at least one annotation.",
            "annotate ke liye kam se kam ek annotation chahiye.",
        )
    return [_normalize_annotation(item) for item in annotations]


# ---------------------------------------------------------------------------
# Options normalisation + validation (runs on the merged kwargs/options
# dict before it reaches the native module).
# ---------------------------------------------------------------------------

def _normalize_options(resolved, op):
    """Hoist flat CommonOptions, normalise enums/colours/layout."""
    resolved = dict(resolved)
    # Flat CommonOptions -> nested "common" (flat wins per key).
    common = {}
    if isinstance(resolved.get("common"), dict):
        common = dict(resolved["common"])
    for key in COMMON_KEYS:
        if key in resolved:
            common[key] = resolved.pop(key)
    if common:
        if "dpi" in common and common["dpi"] is not None:
            dpi = common["dpi"]
            if isinstance(dpi, dict) and "width" in dpi and "height" in dpi:
                dpi = [dpi["width"], dpi["height"]]
            if not (
                isinstance(dpi, (list, tuple))
                and len(dpi) == 2
                and all(_is_int(v) and v >= 0 for v in dpi)
            ):
                raise _invalid_argument(
                    "dpi must be [x, y] with non-negative integers.",
                    "dpi [x, y] hona chahiye, non-negative integers ke saath.",
                )
            common["dpi"] = list(dpi)
        resolved["common"] = common

    if "format" in resolved:
        resolved["format"] = _norm_enum(resolved["format"], _FORMATS, "format") \
            if resolved["format"] is not None else None
    if "fit" in resolved:
        resolved["fit"] = _norm_enum(resolved["fit"], _FITS, "fit")
    if "cell_fit" in resolved:
        resolved["cell_fit"] = _norm_enum(resolved["cell_fit"], _FITS, "cell_fit")
    if "filter" in resolved:
        resolved["filter"] = _norm_enum(resolved["filter"], _FILTERS, "filter")
    if "position" in resolved:
        resolved["position"] = _norm_enum(resolved["position"], _POSITIONS, "position")
    if "anchor" in resolved:
        resolved["anchor"] = _norm_enum(resolved["anchor"], _POSITIONS, "anchor") \
            if resolved["anchor"] is not None else None
    if "background" in resolved:
        resolved["background"] = _as_colour(resolved["background"], "background")
    if "layout" in resolved:
        resolved["layout"] = _norm_layout(resolved["layout"])

    if op == "watermark":
        # text: str | dict | TextWatermark -> full TextWatermark dict.
        # text_size / text_colour kwargs override the text object's
        # size / colour when given.
        text = resolved.get("text")
        if text is not None:
            if isinstance(text, TextWatermark):
                text = text.to_dict()
            elif isinstance(text, str):
                text = {"text": text, "size": 48.0, "colour": _white()}
            elif isinstance(text, dict):
                text = dict(text)
                text.setdefault("size", 48.0)
                text.setdefault("colour", _white())
            else:
                raise _invalid_type(
                    "text must be a string, a dict, or a TextWatermark.",
                    "text string, dict ya TextWatermark hona chahiye.",
                )
            text_size = resolved.pop("text_size", None)
            if text_size is not None:
                text["size"] = text_size
            text_colour = resolved.pop("text_colour", None)
            if text_colour is not None:
                text["colour"] = _as_colour(text_colour, "text_colour")
            text["colour"] = _as_colour(text.get("colour"), "text colour")
            resolved["text"] = text
        else:
            resolved.pop("text_size", None)
            resolved.pop("text_colour", None)
        logo = resolved.get("logo")
        if logo is not None:
            if isinstance(logo, ImageWatermark):
                logo = logo.to_dict()
            elif isinstance(logo, (bytes, bytearray, memoryview)):
                logo = {"image": bytes(logo), "scale": 1.0}
            elif isinstance(logo, dict):
                logo = dict(logo)
                logo.setdefault("scale", 1.0)
            else:
                raise _invalid_type(
                    "logo must be image bytes, a dict, or an ImageWatermark.",
                    "logo image bytes, dict ya ImageWatermark hona chahiye.",
                )
            logo_scale = resolved.pop("logo_scale", None)
            if logo_scale is not None:
                logo["scale"] = logo_scale
            resolved["logo"] = logo
        else:
            resolved.pop("logo_scale", None)
    return resolved


def _validate_options(resolved):
    """Type-check a merged options dict before it reaches native code."""
    for key, value in list(resolved.items()):
        if value is None:
            continue
        if key in ("format", "fit", "cell_fit", "filter", "position", "anchor"):
            if not isinstance(value, str):
                raise _invalid_type(
                    f"Option {key!r} must be a string.",
                    f"Option {key!r} string hona chahiye.",
                )
        elif key in ("auto_orient", "keep_exif", "progressive", "lossless"):
            if not isinstance(value, bool):
                raise _invalid_type(
                    f"Option {key!r} must be a bool.",
                    f"Option {key!r} bool hona chahiye.",
                )
        elif key == "quality":
            if not _is_int(value):
                raise _invalid_type(
                    "Option 'quality' must be an integer.",
                    "Option 'quality' integer hona chahiye.",
                )
            if not 1 <= value <= 100:
                raise DusrelError(
                    "INVALID_QUALITY",
                    f"Quality {value} is invalid. Quality must be between 1 and 100.",
                    f"Quality {value} galat hai. Quality 1 se 100 ke beech honi chahiye.",
                )
        elif key == "effort":
            if not _is_int(value):
                raise _invalid_type(
                    "Option 'effort' must be an integer.",
                    "Option 'effort' integer hona chahiye.",
                )
            if not 0 <= value <= 6:
                raise _invalid_argument(
                    "Option 'effort' must be between 0 and 6.",
                    "Option 'effort' 0 se 6 ke beech hona chahiye.",
                )
        elif key == "opacity":
            _as_number(value, "Option 'opacity'")
            if not 0.0 <= float(value) <= 1.0:
                raise _invalid_argument(
                    "Option 'opacity' must be between 0 and 1.",
                    "Option 'opacity' 0 se 1 ke beech hona chahiye.",
                )
        elif key == "block_size":
            if not _is_int(value):
                raise _invalid_type(
                    "Option 'block_size' must be an integer.",
                    "Option 'block_size' integer hona chahiye.",
                )
            if not 2 <= value <= 100:
                raise _invalid_argument(
                    "Option 'block_size' must be between 2 and 100.",
                    "Option 'block_size' 2 se 100 ke beech hona chahiye.",
                )
        elif key == "percent":
            _as_number(value, "Option 'percent'")
            if float(value) <= 0:
                raise _invalid_argument(
                    "Option 'percent' must be a positive number.",
                    "Option 'percent' positive number hona chahiye.",
                )
        elif key in ("rotation_deg", "size", "scale"):
            _as_number(value, f"Option {key!r}")
        elif key in ("x", "y", "width", "height", "margin", "gap"):
            if not _is_int(value):
                raise _invalid_type(
                    f"Option {key!r} must be an integer.",
                    f"Option {key!r} integer hona chahiye.",
                )
            if value < 0:
                raise _invalid_argument(
                    f"Option {key!r} must be a non-negative integer.",
                    f"Option {key!r} non-negative integer hona chahiye.",
                )
        elif key in ("max_pixels", "max_dimension"):
            if not _is_int(value):
                raise _invalid_type(
                    f"Option {key!r} must be an integer.",
                    f"Option {key!r} integer hona chahiye.",
                )
            if value < 0:
                raise _invalid_argument(
                    f"Option {key!r} must be a non-negative integer.",
                    f"Option {key!r} non-negative integer hona chahiye.",
                )
        elif key == "dpi":
            pass  # validated in _normalize_options
        elif key == "common":
            if not isinstance(value, dict):
                raise _invalid_type(
                    "Option 'common' must be a dict.",
                    "Option 'common' dict hona chahiye.",
                )
            _validate_options(value)
        elif key in ("background", "colour", "text", "logo", "layout"):
            pass  # normalised in _normalize_options
        # Unknown keys are ignored (forward compatibility with new options).


def _final_options(kwargs, options, op):
    resolved = _resolve_kwargs(kwargs, options)
    resolved = _normalize_options(resolved, op)
    _validate_options(resolved)
    return resolved


def _common_kwargs(auto_orient=_UNSET, keep_exif=_UNSET, dpi=_UNSET,
                   max_pixels=_UNSET, max_dimension=_UNSET):
    return {
        "auto_orient": auto_orient,
        "keep_exif": keep_exif,
        "dpi": dpi,
        "max_pixels": max_pixels,
        "max_dimension": max_dimension,
    }


# ---------------------------------------------------------------------------
# Option dataclasses — one clear way to build options objects.
# Defaults mirror the Rust core. Pass an instance as `options=...`.
# ---------------------------------------------------------------------------

@dataclass
class Colour:
    """An 8-bit RGBA colour. ``hex`` / ``hsl`` are derived on demand."""

    r: int = 0
    g: int = 0
    b: int = 0
    a: int = 255
    hex: str | None = None
    hsl: list | tuple | None = None

    def __post_init__(self):
        for channel in ("r", "g", "b", "a"):
            value = getattr(self, channel)
            if not _is_int(value) or not 0 <= value <= 255:
                raise _invalid_argument(
                    f"Colour channel {channel!r} must be 0-255.",
                    f"Colour channel {channel!r} 0-255 ke beech hona chahiye.",
                )

    def to_dict(self):
        return _colour_dict(self.r, self.g, self.b, self.a)

    @classmethod
    def from_hex(cls, hex_string):
        d = _parse_hex_colour(hex_string)
        return cls(r=d["r"], g=d["g"], b=d["b"], a=d["a"])

    @classmethod
    def from_rgb(cls, r, g, b, a=255):
        return cls(r=r, g=g, b=b, a=a)

    @classmethod
    def white(cls):
        return cls(r=255, g=255, b=255)

    @classmethod
    def black(cls):
        return cls(r=0, g=0, b=0)


@dataclass
class CommonOptions:
    auto_orient: bool = True
    keep_exif: bool = False
    dpi: list | tuple | None = None
    max_pixels: int = 120_000_000
    max_dimension: int = 16000

    def to_dict(self):
        return dataclasses.asdict(self)


@dataclass
class CompressOptions:
    common: CommonOptions = field(default_factory=CommonOptions)
    quality: int = 80
    format: str | None = None
    progressive: bool = False
    lossless: bool = False
    effort: int = 4

    def to_dict(self):
        d = dataclasses.asdict(self)
        d["common"] = self.common.to_dict()
        return d


@dataclass
class ResizeOptions:
    common: CommonOptions = field(default_factory=CommonOptions)
    width: int | None = None
    height: int | None = None
    percent: float | None = None
    fit: str = "fit"
    filter: str = "lanczos3"
    background: Colour = field(default_factory=Colour.white)

    def to_dict(self):
        d = dataclasses.asdict(self)
        d["common"] = self.common.to_dict()
        d["background"] = self.background.to_dict()
        return d


@dataclass
class CropOptions:
    common: CommonOptions = field(default_factory=CommonOptions)
    x: int = 0
    y: int = 0
    width: int = 0
    height: int = 0
    anchor: str | None = None

    def to_dict(self):
        d = dataclasses.asdict(self)
        d["common"] = self.common.to_dict()
        return d


@dataclass
class ConvertOptions:
    common: CommonOptions = field(default_factory=CommonOptions)
    format: str = "jpeg"
    quality: int = 80
    background: Colour = field(default_factory=Colour.white)
    progressive: bool = False

    def to_dict(self):
        d = dataclasses.asdict(self)
        d["common"] = self.common.to_dict()
        d["background"] = self.background.to_dict()
        return d


@dataclass
class TextWatermark:
    text: str = ""
    size: float = 48.0
    colour: Colour = field(default_factory=Colour.white)

    def to_dict(self):
        return {
            "text": self.text,
            "size": self.size,
            "colour": self.colour.to_dict(),
        }


@dataclass
class ImageWatermark:
    image: bytes = b""
    scale: float = 1.0

    def to_dict(self):
        return {"image": bytes(self.image), "scale": self.scale}


@dataclass
class WatermarkOptions:
    common: CommonOptions = field(default_factory=CommonOptions)
    text: TextWatermark | None = None
    logo: ImageWatermark | None = None
    opacity: float = 0.5
    position: str = "bottom_right"
    rotation_deg: float = 0.0
    margin: int = 16

    def to_dict(self):
        d = dataclasses.asdict(self)
        d["common"] = self.common.to_dict()
        d["text"] = self.text.to_dict() if self.text is not None else None
        d["logo"] = self.logo.to_dict() if self.logo is not None else None
        return d


@dataclass
class CollageOptions:
    common: CommonOptions = field(default_factory=CommonOptions)
    layout: str | dict = "grid"
    gap: int = 8
    background: Colour = field(default_factory=Colour.white)
    width: int = 1200
    height: int = 1200
    cell_fit: str = "fit"

    def to_dict(self):
        d = dataclasses.asdict(self)
        d["common"] = self.common.to_dict()
        d["background"] = self.background.to_dict()
        return d


@dataclass
class PixelateOptions:
    common: CommonOptions = field(default_factory=CommonOptions)
    x: int = 0
    y: int = 0
    width: int = 0
    height: int = 0
    block_size: int = 10

    def to_dict(self):
        d = dataclasses.asdict(self)
        d["common"] = self.common.to_dict()
        return d


@dataclass
class ImageInfo:
    format: str
    width: int
    height: int
    has_alpha: bool
    colour_type: str
    file_size: int

    def to_dict(self):
        return dataclasses.asdict(self)

    @classmethod
    def from_dict(cls, data):
        return cls(
            format=data["format"],
            width=data["width"],
            height=data["height"],
            has_alpha=data["has_alpha"],
            colour_type=data["colour_type"],
            file_size=data["file_size"],
        )


@dataclass
class PaletteColour:
    colour: Colour
    share: float = 0.0

    def to_dict(self):
        return {**self.colour.to_dict(), "share": self.share}

    @classmethod
    def from_dict(cls, data):
        return cls(
            colour=Colour(
                r=data["r"], g=data["g"], b=data["b"], a=data.get("a", 255),
            ),
            share=data["share"],
        )


# ---------------------------------------------------------------------------
# Operations
# ---------------------------------------------------------------------------

def compress_image(image, *, quality=_UNSET, format=_UNSET,
                   progressive=_UNSET, lossless=_UNSET, effort=_UNSET,
                   auto_orient=_UNSET, keep_exif=_UNSET, dpi=_UNSET,
                   max_pixels=_UNSET, max_dimension=_UNSET, options=None):
    """Re-encode an image at ``quality`` (1-100, default 80).

    ``format`` keeps the input format when unset (TIFF input falls
    back to PNG). Returns the encoded image bytes.
    """
    data = _as_bytes(image, "image")
    opts = _final_options({
        "quality": quality, "format": format, "progressive": progressive,
        "lossless": lossless, "effort": effort,
        **_common_kwargs(auto_orient, keep_exif, dpi, max_pixels, max_dimension),
    }, options, "compress")
    return _call(_native.compress_image, data, opts)


def compress_to_size(image, target_bytes, *, quality=_UNSET, format=_UNSET,
                     progressive=_UNSET, lossless=_UNSET, effort=_UNSET,
                     auto_orient=_UNSET, keep_exif=_UNSET, dpi=_UNSET,
                     max_pixels=_UNSET, max_dimension=_UNSET, options=None):
    """Re-encode so the result is at most ``target_bytes`` bytes.

    Binary quality search (JPEG/AVIF only; PNG/WebP/GIF/BMP take one
    best-effort encode). Raises ``TARGET_SIZE_UNREACHABLE`` when even
    the lowest quality is too big.
    """
    data = _as_bytes(image, "image")
    _as_int(target_bytes, "target_bytes", minimum=0)
    opts = _final_options({
        "quality": quality, "format": format, "progressive": progressive,
        "lossless": lossless, "effort": effort,
        **_common_kwargs(auto_orient, keep_exif, dpi, max_pixels, max_dimension),
    }, options, "compress")
    return _call(_native.compress_to_size, data, target_bytes, opts)


def resize_image(image, *, width=_UNSET, height=_UNSET, percent=_UNSET,
                 fit=_UNSET, filter=_UNSET, background=_UNSET,
                 auto_orient=_UNSET, keep_exif=_UNSET, dpi=_UNSET,
                 max_pixels=_UNSET, max_dimension=_UNSET, options=None):
    """Resize an image: ``width`` and/or ``height``, or ``percent``.

    ``fit`` is ``"fit"`` (default) / ``"fill"`` / ``"stretch"`` /
    ``"crop"``; ``filter`` is ``"nearest"`` / ``"triangle"`` /
    ``"catmull-rom"`` / ``"gaussian"`` / ``"lanczos3"`` (default).
    ``background`` (a colour) is the letterbox colour for ``"fill"``.
    Returns the encoded image bytes in the input format.
    """
    data = _as_bytes(image, "image")
    opts = _final_options({
        "width": width, "height": height, "percent": percent,
        "fit": fit, "filter": filter, "background": background,
        **_common_kwargs(auto_orient, keep_exif, dpi, max_pixels, max_dimension),
    }, options, "resize")
    return _call(_native.resize_image, data, opts)


def crop_image(image, *, x=_UNSET, y=_UNSET, width=_UNSET, height=_UNSET,
               anchor=_UNSET, auto_orient=_UNSET, keep_exif=_UNSET,
               dpi=_UNSET, max_pixels=_UNSET, max_dimension=_UNSET,
               options=None):
    """Crop to the ``x``/``y``/``width``/``height`` rectangle.

    ``anchor`` (``"center"``, ``"top_left"``, ...) places a
    ``width`` x ``height`` rectangle at a preset and ignores ``x``/``y``.
    Out-of-bounds raises ``CROP_OUT_OF_BOUNDS``.
    """
    data = _as_bytes(image, "image")
    opts = _final_options({
        "x": x, "y": y, "width": width, "height": height, "anchor": anchor,
        **_common_kwargs(auto_orient, keep_exif, dpi, max_pixels, max_dimension),
    }, options, "crop")
    return _call(_native.crop_image, data, opts)


def convert_image(image, *, format=_UNSET, quality=_UNSET, background=_UNSET,
                  progressive=_UNSET, auto_orient=_UNSET, keep_exif=_UNSET,
                  dpi=_UNSET, max_pixels=_UNSET, max_dimension=_UNSET,
                  options=None):
    """Convert to ``format`` (``"jpeg"`` / ``"png"`` / ``"webp"`` /
    ``"avif"`` / ``"gif"`` / ``"bmp"``) at ``quality`` (default 80).

    ``background`` (default white) flattens alpha when the target has
    none (JPEG/BMP).
    """
    data = _as_bytes(image, "image")
    opts = _final_options({
        "format": format, "quality": quality, "background": background,
        "progressive": progressive,
        **_common_kwargs(auto_orient, keep_exif, dpi, max_pixels, max_dimension),
    }, options, "convert")
    if "format" not in opts or opts["format"] is None:
        raise _invalid_argument(
            "convert_image needs format=, e.g. format=\"png\".",
            "convert_image ke liye format= chahiye, jaise format=\"png\".",
        )
    return _call(_native.convert_image, data, opts)


def add_watermark(image, *, text=_UNSET, text_size=_UNSET, text_colour=_UNSET,
                  logo=_UNSET, logo_scale=_UNSET, opacity=_UNSET,
                  position=_UNSET, rotation_deg=_UNSET, margin=_UNSET,
                  auto_orient=_UNSET, keep_exif=_UNSET, dpi=_UNSET,
                  max_pixels=_UNSET, max_dimension=_UNSET, options=None):
    """Stamp a text and/or logo watermark on an image.

    ``text`` is a string (size 48, white) or a ``{"text", "size",
    "colour"}`` dict / ``TextWatermark``; ``logo`` is image bytes (or
    a ``{"image", "scale"}`` dict / ``ImageWatermark``). ``opacity``
    is 0-1 (default 0.5); ``position`` is a 9-anchor name or
    ``"tiled"`` (default ``"bottom_right"``). At least one of
    ``text`` / ``logo`` is required.
    """
    data = _as_bytes(image, "image")
    opts = _final_options({
        "text": text, "text_size": text_size, "text_colour": text_colour,
        "logo": logo, "logo_scale": logo_scale, "opacity": opacity,
        "position": position, "rotation_deg": rotation_deg, "margin": margin,
        **_common_kwargs(auto_orient, keep_exif, dpi, max_pixels, max_dimension),
    }, options, "watermark")
    if opts.get("text") is None and opts.get("logo") is None:
        raise _invalid_argument(
            "add_watermark needs text= or logo=.",
            "add_watermark ke liye text= ya logo= chahiye.",
        )
    return _call(_native.add_watermark, data, opts)


def annotate_image(image, annotations):
    """Draw shapes / text on an image (raster, absolute pixels).

    ``annotations`` is a list of dicts, each ``{"type": "arrow" |
    "rect" | "ellipse" | "line" | "text", ...}`` (or the serde form
    ``{"Rect": {...}}``):

    * arrow/line: ``from`` / ``to`` as ``[x, y]``, ``colour``, ``width``
    * rect/ellipse: ``x``, ``y``, ``w``, ``h``, ``colour``, ``width``,
      optional ``fill`` colour
    * text: ``x``, ``y``, ``text``, ``size``, ``colour``

    Colours accept ``"#RRGGBB"``, ``[r, g, b]`` or a dict. Returns the
    encoded image bytes in the input format.
    """
    data = _as_bytes(image, "image")
    anns = _normalize_annotations(annotations)
    return _call(_native.annotate_image, data, anns)


def pick_colour(image, x, y):
    """Read the pixel at (``x``, ``y``).

    Returns ``{"hex", "r", "g", "b", "a", "hsl"}`` with ``hsl`` a
    ``(h, s, l)`` tuple (h in degrees 0-360, s/l in 0-1).
    Out-of-bounds raises ``PIXEL_OUT_OF_BOUNDS``.
    """
    data = _as_bytes(image, "image")
    _as_int(x, "x", minimum=0)
    _as_int(y, "y", minimum=0)
    result = _call(_native.pick_colour, data, x, y)
    result["hsl"] = tuple(result["hsl"])
    return result


def extract_palette(image, count=5):
    """Extract the ``count`` (1-16) dominant colours, most frequent first.

    Returns a list of ``{"hex", "r", "g", "b", "a", "hsl", "share"}``
    dicts, ``share`` in 0-1, ``hsl`` a ``(h, s, l)`` tuple.
    """
    data = _as_bytes(image, "image")
    _as_int(count, "count")
    if not 1 <= count <= 16:
        raise _invalid_argument(
            "count must be between 1 and 16.",
            "count 1 se 16 ke beech hona chahiye.",
        )
    results = _call(_native.extract_palette, data, count)
    for entry in results:
        entry["hsl"] = tuple(entry["hsl"])
    return results


def make_collage(images, *, layout=_UNSET, gap=_UNSET, background=_UNSET,
                 width=_UNSET, height=_UNSET, cell_fit=_UNSET,
                 auto_orient=_UNSET, keep_exif=_UNSET, dpi=_UNSET,
                 max_pixels=_UNSET, max_dimension=_UNSET, options=None):
    """Build a collage from several images.

    ``layout`` is ``"grid"`` (default 2x2), ``"row"``, ``"column"``,
    ``"2x2"``-style ``"RxC"`` (1-4 each), or ``{"rows": r, "cols": c}``.
    Output is ``width`` x ``height`` (default 1200x1200).
    ``gap`` is the cell gap in px (default 8), ``cell_fit`` the
    per-cell fit mode. Returns the encoded image bytes.
    """
    imgs = _as_bytes_list(images, "images")
    if len(imgs) == 0:
        raise DusrelError(
            "EMPTY_INPUT",
            "collage inputs is empty.",
            "collage inputs khaali hai.",
        )
    opts = _final_options({
        "layout": layout, "gap": gap, "background": background,
        "width": width, "height": height, "cell_fit": cell_fit,
        **_common_kwargs(auto_orient, keep_exif, dpi, max_pixels, max_dimension),
    }, options, "collage")
    return _call(_native.make_collage, imgs, opts)


def pixelate_image(image, *, x=_UNSET, y=_UNSET, width=_UNSET, height=_UNSET,
                   block_size=_UNSET, auto_orient=_UNSET, keep_exif=_UNSET,
                   dpi=_UNSET, max_pixels=_UNSET, max_dimension=_UNSET,
                   options=None):
    """Pixelate a rectangle (default: the whole image).

    ``width`` / ``height`` of 0 (or unset) mean "to the right/bottom
    edge". ``block_size`` is 2-100 px (default 10). Implemented as
    downscale-Nearest + upscale-Nearest of the rect only.
    """
    data = _as_bytes(image, "image")
    opts = _final_options({
        "x": x, "y": y, "width": width, "height": height,
        "block_size": block_size,
        **_common_kwargs(auto_orient, keep_exif, dpi, max_pixels, max_dimension),
    }, options, "pixelate")
    return _call(_native.pixelate_image, data, opts)


def get_image_info(image, *, auto_orient=_UNSET, keep_exif=_UNSET,
                   dpi=_UNSET, max_pixels=_UNSET, max_dimension=_UNSET,
                   options=None):
    """Describe an image without transforming it.

    Returns ``{"format", "width", "height", "has_alpha",
    "colour_type", "file_size"}`` where ``format`` is the lowercase
    short name (``"jpeg"``, ``"png"``, ``"tiff"``, ...) and
    ``colour_type`` the lowercase image-crate colour type.
    """
    data = _as_bytes(image, "image")
    opts = _final_options({
        **_common_kwargs(auto_orient, keep_exif, dpi, max_pixels, max_dimension),
    }, options, "info")
    return _call(_native.get_image_info, data, opts)


def bulk_process(images, op, *, target_bytes=None, annotations=None,
                 x=None, y=None, count=None, options=None):
    """Run one op over many images in a single call.

    ``op`` is ``"compress"``, ``"compress_to_size"`` (needs
    ``target_bytes``), ``"resize"``, ``"crop"``, ``"convert"``,
    ``"watermark"``, ``"annotate"`` (needs ``annotations``),
    ``"pick_colour"`` (needs ``x`` and ``y``), ``"palette"`` (needs
    ``count``, default 5), ``"collage"`` or ``"pixelate"``.
    ``options`` is the op's options dict / dataclass / Pydantic
    model (same kwargs as the single-image function, minus the
    per-call extras). Returns one dict per input, in order:
    ``{"index", "output", "error_code", "error"}`` — ``output`` is
    bytes for image ops, a colour dict for ``"pick_colour"``, a list
    of palette dicts for ``"palette"``, and ``None`` on failure. One
    bad input fails only its own item, never the whole batch.
    ``"collage"`` answers every item with ``INVALID_ARGUMENT`` (a
    collage combines all inputs — call ``make_collage`` once
    instead); ``"info"`` is not a bulk op.
    """
    imgs = _as_bytes_list(images, "images")
    if not isinstance(op, str) or op not in BULK_OPS:
        raise _invalid_argument(
            f"Unknown bulk op \"{op}\". Use one of: {', '.join(BULK_OPS)}.",
            f"Unknown bulk op \"{op}\" hai. Inme se ek use karein: {', '.join(BULK_OPS)}.",
        )
    if op == "compress_to_size":
        if target_bytes is None:
            raise _invalid_argument(
                "bulk_process op \"compress_to_size\" needs target_bytes.",
                "bulk_process op \"compress_to_size\" ke liye target_bytes chahiye.",
            )
        _as_int(target_bytes, "target_bytes", minimum=0)
    anns = None
    if op == "annotate":
        if annotations is None:
            raise _invalid_argument(
                "bulk_process op \"annotate\" needs annotations.",
                "bulk_process op \"annotate\" ke liye annotations chahiye.",
            )
        anns = _normalize_annotations(annotations)
    if op == "pick_colour":
        if x is None or y is None:
            raise _invalid_argument(
                "bulk_process op \"pick_colour\" needs x and y.",
                "bulk_process op \"pick_colour\" ke liye x aur y chahiye.",
            )
        _as_int(x, "x", minimum=0)
        _as_int(y, "y", minimum=0)
    if op == "palette":
        if count is None:
            count = 5
        _as_int(count, "count")
        if not 1 <= count <= 16:
            raise _invalid_argument(
                "count must be between 1 and 16.",
                "count 1 se 16 ke beech hona chahiye.",
            )
    if op in ("pick_colour", "palette") and options is not None:
        resolved = _options_to_dict(options)
        if resolved:
            raise _invalid_argument(
                f"bulk_process op \"{op}\" takes no options.",
                f"bulk_process op \"{op}\" me koi options nahi hote.",
            )
    if op in ("pick_colour", "palette"):
        opts = {}
    else:
        op_for_opts = "compress" if op == "compress_to_size" else op
        opts = _final_options({}, options, op_for_opts)
    return _call(_native.bulk_process, imgs, op, opts, target_bytes, anns,
                 x, y, count)


# ---------------------------------------------------------------------------
# Fixtures — test data built by the Rust core (same builders the Rust
# tests use). For tests, demos and form previews only.
# ---------------------------------------------------------------------------

def make_test_image(kind="noise", width=64, height=64, seed=42):
    """Build a deterministic test PNG: ``kind`` is ``"solid"``,
    ``"gradient"`` or ``"noise"`` (photo-like, default). Same inputs
    always produce identical bytes."""
    if kind not in ("solid", "gradient", "noise"):
        raise _invalid_argument(
            "kind must be \"solid\", \"gradient\" or \"noise\".",
            "kind \"solid\", \"gradient\" ya \"noise\" hona chahiye.",
        )
    _as_int(width, "width", minimum=1)
    _as_int(height, "height", minimum=1)
    _as_int(seed, "seed", minimum=0)
    return _call(_native.make_test_image, kind, width, height, seed)
