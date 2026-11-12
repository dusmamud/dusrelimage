//! Python bindings for dusrelimage — thin PyO3 wrappers over the Rust core.
//!
//! All image logic lives in the `dusrelimage` crate; this crate only
//! converts bytes / option dicts to and from Python. Build with
//! `maturin develop` / `maturin build` (from this directory).
//!
//! Error contract: every Rust `DusrelError` is raised as a Python
//! `ValueError` whose message is `CODE\x1f reason_en \x1f reason_hi`.
//! The Python layer (`python/dusrelimage/__init__.py`) parses that into
//! `dusrelimage.DusrelError`. Options arrive as a Python dict matching
//! the Rust Options JSON shape (snake_case keys); missing keys fall
//! back to the Rust `Default` for that options struct, so callers can
//! pass a partial dict.
//!
//! The native side is forgiving about friendly spellings, so the
//! Python layer and direct `_native` callers agree: colours may be a
//! `"#RRGGBB"` string, an `[r, g, b]` list or a full colour dict;
//! enums accept lowercase names (`"jpeg"`, `"fit"`, `"lanczos3"`,
//! `"bottom-right"`); `layout` accepts `"grid"` / `"row"` /
//! `"column"` / `"2x2"` / `{"rows": r, "cols": c}`; `CommonOptions`
//! fields may be passed flat (`auto_orient`, `max_pixels`, ...) or
//! nested under `"common"`; a watermark `text` may be a plain string
//! and `logo` may be raw bytes.
//!
//! Tools 5-10 (`add_watermark`, `annotate_image`, `pick_colour`,
//! `extract_palette`, `make_collage`, `pixelate_image`) bind to the
//! SPEC-fixed core signatures of the same names. `bulk_process`
//! delegates to the core's `bulk_process(inputs, &BulkOp)` — one
//! `BulkOp` per call, one result per input, failures per item, never
//! aborting. The core has no `BulkOp` for `compress_to_size`, so
//! that one op keeps its batch loop here (same per-item contract).
//! `collage` is a `BulkOp` variant, but the core answers it with a
//! per-item `INVALID_ARGUMENT` (a collage combines all inputs into
//! one image — call `make_collage` once instead).

// clippy 1.99 flags `PyResult<Py<PyAny>>` returns from #[pyfunction]s
// as useless conversions — a PyO3 false positive (the conversion is
// what turns the value into a Python object). Allowed crate-wide,
// same as dusrelpdf-py / indycheck-py.
#![allow(clippy::useless_conversion)]

use dusrelimage::{
    Annotation, BulkOp, BulkOutput, CollageOptions, Colour, CommonOptions, CompressOptions,
    ConvertOptions, CropOptions, PixelateOptions, ResizeOptions, WatermarkOptions,
};
use pyo3::IntoPyObjectExt;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList};

/// Convert an infallible Rust primitive (`bool`, `i64`, `u64`, `f64`,
/// `String`, `&str`) into an owned Python object.
/// (PyO3 0.30: `ToPyObject::to_object` and the `PyObject` alias are
/// gone; this is the indycheck-py idiom.)
fn to_py<'py>(py: Python<'py>, value: impl pyo3::IntoPyObject<'py>) -> Py<PyAny> {
    value
        .into_bound_py_any(py)
        .expect("primitive to Python conversion is infallible")
        .unbind()
}

/// Separator for the native error payload. `\x1f` never appears in
/// the Rust reason strings.
const SEP: char = '\x1f';

fn native_err(e: &dusrelimage::DusrelError) -> PyErr {
    PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
        "{}{}{}{}{}",
        e.code_str(),
        SEP,
        e.reason_en(),
        SEP,
        e.reason_hi()
    ))
}

fn invalid_argument(detail: &str) -> PyErr {
    PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
        "INVALID_ARGUMENT{}Invalid argument: {}{}Galat argument hai: {}",
        SEP, detail, SEP, detail
    ))
}

/// Convert a Python value to a JSON value. Bytes become an array of
/// byte values (so a watermark `logo` image survives the trip).
fn py_to_json(obj: &Bound<'_, PyAny>) -> PyResult<serde_json::Value> {
    if obj.is_none() {
        return Ok(serde_json::Value::Null);
    }
    if let Ok(b) = obj.extract::<bool>() {
        // extract::<bool> only succeeds for real bools in PyO3.
        return Ok(serde_json::Value::Bool(b));
    }
    if let Ok(i) = obj.extract::<i64>() {
        return Ok(serde_json::Value::Number(i.into()));
    }
    if let Ok(u) = obj.extract::<u64>() {
        return Ok(serde_json::Value::Number(u.into()));
    }
    if let Ok(f) = obj.extract::<f64>() {
        return Ok(serde_json::Number::from_f64(f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null));
    }
    if let Ok(s) = obj.extract::<String>() {
        return Ok(serde_json::Value::String(s));
    }
    // bytes / bytearray only (the Python layer normalises memoryview
    // to bytes first, so the native side never sees it).
    if obj.is_instance_of::<PyBytes>() || obj.cast::<pyo3::types::PyByteArray>().is_ok() {
        let bytes: Vec<u8> = obj.extract()?;
        return Ok(serde_json::Value::Array(
            bytes
                .into_iter()
                .map(|b| serde_json::Value::Number(b.into()))
                .collect(),
        ));
    }
    if let Ok(items) = obj.extract::<Vec<Py<PyAny>>>() {
        let mut out = Vec::with_capacity(items.len());
        for item in items {
            out.push(py_to_json(item.bind(obj.py()))?);
        }
        return Ok(serde_json::Value::Array(out));
    }
    if let Ok(dict) = obj.cast::<PyDict>() {
        let mut out = serde_json::Map::new();
        for (k, v) in dict.iter() {
            let key: String = k.extract()?;
            out.insert(key, py_to_json(&v)?);
        }
        return Ok(serde_json::Value::Object(out));
    }
    Err(pyo3::exceptions::PyTypeError::new_err(format!(
        "cannot convert {} to a JSON value",
        obj.get_type()
    )))
}

// ---------------------------------------------------------------------------
// Normalisation: friendly spellings -> the exact serde shapes.
// ---------------------------------------------------------------------------

/// `"#RRGGBB"` / `"#RGB"` / `"#RRGGBBAA"` string, `[r, g, b(, a)]`
/// list, or a (possibly partial) colour dict -> the full Colour JSON
/// object `{hex, r, g, b, a, hsl}`. `Null` passes through (for
/// `fill: None`).
fn normalize_colour(value: &mut serde_json::Value) -> PyResult<()> {
    match value.clone() {
        serde_json::Value::Null => Ok(()),
        serde_json::Value::String(s) => {
            let colour = Colour::from_hex(&s).map_err(|e| invalid_argument(&e.message_en))?;
            *value = serde_json::to_value(&colour).map_err(|e| invalid_argument(&e.to_string()))?;
            Ok(())
        }
        serde_json::Value::Array(items) => {
            let nums: PyResult<Vec<u8>> = items
                .iter()
                .map(|n| {
                    n.as_u64()
                        .and_then(|v| u8::try_from(v).ok())
                        .ok_or_else(|| invalid_argument("colour channels must be 0-255"))
                })
                .collect();
            let nums = nums?;
            let colour = match nums.len() {
                3 => Colour::from_rgb(nums[0], nums[1], nums[2]),
                4 => Colour::from_rgba(nums[0], nums[1], nums[2], nums[3]),
                _ => {
                    return Err(invalid_argument(
                        "colour list must be [r, g, b] or [r, g, b, a]",
                    ));
                }
            };
            *value = serde_json::to_value(&colour).map_err(|e| invalid_argument(&e.to_string()))?;
            Ok(())
        }
        serde_json::Value::Object(map) => {
            if let (Some(r), Some(g), Some(b)) = (
                map.get("r").and_then(serde_json::Value::as_u64),
                map.get("g").and_then(serde_json::Value::as_u64),
                map.get("b").and_then(serde_json::Value::as_u64),
            ) {
                let a = map
                    .get("a")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(255);
                let (r, g, b, a) = (r as u8, g as u8, b as u8, a as u8);
                let colour = Colour::from_rgba(r, g, b, a);
                *value =
                    serde_json::to_value(&colour).map_err(|e| invalid_argument(&e.to_string()))?;
                Ok(())
            } else if let Some(hex) = map.get("hex").and_then(serde_json::Value::as_str) {
                let colour = Colour::from_hex(hex).map_err(|e| invalid_argument(&e.message_en))?;
                *value =
                    serde_json::to_value(&colour).map_err(|e| invalid_argument(&e.to_string()))?;
                Ok(())
            } else {
                Err(invalid_argument(
                    "colour dict needs {r, g, b} or {hex: \"#RRGGBB\"}",
                ))
            }
        }
        _ => Err(invalid_argument(
            "colour must be \"#RRGGBB\", [r, g, b], or a colour dict",
        )),
    }
}

/// Map a lowercase/kebab/snake name onto a serde variant name.
fn map_enum(value: &str, pairs: &[(&str, &str)]) -> PyResult<String> {
    let lower = value.trim().to_ascii_lowercase();
    for (friendly, canonical) in pairs {
        if lower == *friendly {
            return Ok((*canonical).to_string());
        }
    }
    // Already-canonical spellings pass through untouched.
    if pairs.iter().any(|(_, c)| *c == value) {
        return Ok(value.to_string());
    }
    Err(invalid_argument(&format!(
        "unknown value \"{value}\" (expected one of: {})",
        pairs.iter().map(|(f, _)| *f).collect::<Vec<_>>().join(", ")
    )))
}

fn normalize_layout(value: &mut serde_json::Value) -> PyResult<()> {
    match value.clone() {
        serde_json::Value::String(s) => {
            let lower = s.trim().to_ascii_lowercase();
            match lower.as_str() {
                "row" => *value = serde_json::Value::String("Row".to_string()),
                "column" => *value = serde_json::Value::String("Column".to_string()),
                "grid" => {
                    *value = serde_json::json!({"Grid": {"rows": 2, "cols": 2}});
                }
                _ => {
                    let (r, c) = lower.split_once('x').ok_or_else(|| {
                        invalid_argument(
                            "layout must be \"grid\", \"row\", \"column\" or \"RxC\" like \"2x2\"",
                        )
                    })?;
                    let rows: u32 = r
                        .trim()
                        .parse()
                        .map_err(|_| invalid_argument("layout rows must be 1-4"))?;
                    let cols: u32 = c
                        .trim()
                        .parse()
                        .map_err(|_| invalid_argument("layout columns must be 1-4"))?;
                    if !(1..=4).contains(&rows) || !(1..=4).contains(&cols) {
                        return Err(invalid_argument(
                            "collage grid rows and columns must each be 1-4",
                        ));
                    }
                    *value = serde_json::json!({"Grid": {"rows": rows, "cols": cols}});
                }
            }
            Ok(())
        }
        serde_json::Value::Object(map) => {
            if map.contains_key("Grid") {
                return Ok(());
            }
            if let Some(inner) = map.get("grid") {
                *value = serde_json::json!({"Grid": inner});
                return Ok(());
            }
            if map.contains_key("rows") && map.contains_key("cols") {
                *value = serde_json::json!({"Grid": map});
                return Ok(());
            }
            Err(invalid_argument(
                "layout dict needs {\"rows\": r, \"cols\": c} or {\"Grid\": {...}}",
            ))
        }
        _ => Err(invalid_argument(
            "layout must be \"grid\", \"row\", \"column\", \"RxC\" or a dict",
        )),
    }
}

/// Normalise one JSON object in place: colour fields, enum fields,
/// layout, watermark text/logo shorthands. Recurses into nested
/// objects and arrays (annotations, `common`, `text`, `logo`).
fn normalize_value(value: &mut serde_json::Value) -> PyResult<()> {
    match value {
        serde_json::Value::Object(map) => {
            for (key, v) in map.iter_mut() {
                match key.as_str() {
                    "colour" | "background" | "fill" => normalize_colour(v)?,
                    "format" => {
                        if let serde_json::Value::String(s) = v.clone() {
                            let mapped = map_enum(
                                &s,
                                &[
                                    ("jpeg", "Jpeg"),
                                    ("jpg", "Jpeg"),
                                    ("png", "Png"),
                                    ("webp", "WebP"),
                                    ("avif", "Avif"),
                                    ("gif", "Gif"),
                                    ("bmp", "Bmp"),
                                ],
                            )?;
                            *v = serde_json::Value::String(mapped);
                        }
                    }
                    "fit" | "cell_fit" => {
                        if let serde_json::Value::String(s) = v.clone() {
                            let mapped = map_enum(
                                &s,
                                &[
                                    ("fit", "Fit"),
                                    ("fill", "Fill"),
                                    ("stretch", "Stretch"),
                                    ("crop", "Crop"),
                                ],
                            )?;
                            *v = serde_json::Value::String(mapped);
                        }
                    }
                    "filter" => {
                        if let serde_json::Value::String(s) = v.clone() {
                            let mapped = map_enum(
                                &s,
                                &[
                                    ("nearest", "Nearest"),
                                    ("triangle", "Triangle"),
                                    ("catmull-rom", "CatmullRom"),
                                    ("catmullrom", "CatmullRom"),
                                    ("catmull_rom", "CatmullRom"),
                                    ("gaussian", "Gaussian"),
                                    ("lanczos3", "Lanczos3"),
                                ],
                            )?;
                            *v = serde_json::Value::String(mapped);
                        }
                    }
                    "position" | "anchor" => {
                        if let serde_json::Value::String(s) = v.clone() {
                            let mapped = map_enum(
                                &s,
                                &[
                                    ("top-left", "TopLeft"),
                                    ("top_left", "TopLeft"),
                                    ("top-center", "TopCenter"),
                                    ("top_center", "TopCenter"),
                                    ("top-centre", "TopCenter"),
                                    ("top-centre", "TopCenter"),
                                    ("top-right", "TopRight"),
                                    ("top_right", "TopRight"),
                                    ("center-left", "CenterLeft"),
                                    ("center_left", "CenterLeft"),
                                    ("centre-left", "CenterLeft"),
                                    ("center", "Center"),
                                    ("centre", "Center"),
                                    ("center-right", "CenterRight"),
                                    ("center_right", "CenterRight"),
                                    ("centre-right", "CenterRight"),
                                    ("bottom-left", "BottomLeft"),
                                    ("bottom_left", "BottomLeft"),
                                    ("bottom-center", "BottomCenter"),
                                    ("bottom_center", "BottomCenter"),
                                    ("bottom-centre", "BottomCenter"),
                                    ("bottom-right", "BottomRight"),
                                    ("bottom_right", "BottomRight"),
                                    ("tiled", "Tiled"),
                                ],
                            )?;
                            *v = serde_json::Value::String(mapped);
                        }
                    }
                    "layout" => normalize_layout(v)?,
                    _ => normalize_value(v)?,
                }
            }
            Ok(())
        }
        serde_json::Value::Array(items) => {
            for item in items {
                normalize_value(item)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Deep-merge `overlay` into `base`: objects merge key by key
/// (overlay wins per key), everything else replaces. This is what
/// lets a caller pass a partial nested dict — e.g.
/// `{"common": {"max_pixels": 5}}` or a partial watermark `text` —
/// without wiping the Rust defaults for the sibling fields.
fn merge_json(base: &mut serde_json::Value, overlay: serde_json::Value) {
    match (&mut *base, overlay) {
        (serde_json::Value::Object(base_map), serde_json::Value::Object(over_map)) => {
            for (k, v) in over_map {
                match base_map.get_mut(&k) {
                    Some(existing) => merge_json(existing, v),
                    None => {
                        base_map.insert(k, v);
                    }
                }
            }
        }
        (base_slot, overlay) => {
            *base_slot = overlay;
        }
    }
}

/// `CommonOptions` fields passed flat at the top level
/// (`auto_orient`, `keep_exif`, `dpi`, `max_pixels`,
/// `max_dimension`) are hoisted into the nested `"common"` object
/// the Rust structs expect. The nested object wins when both forms
/// are given.
fn hoist_common(value: &mut serde_json::Value) {
    let Some(obj) = value.as_object_mut() else {
        return;
    };
    const COMMON_KEYS: [&str; 5] = [
        "auto_orient",
        "keep_exif",
        "dpi",
        "max_pixels",
        "max_dimension",
    ];
    let mut common = obj
        .get("common")
        .cloned()
        .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
    let mut moved = false;
    for key in COMMON_KEYS {
        if let Some(v) = obj.remove(key) {
            if let serde_json::Value::Object(map) = &mut common {
                map.insert(key.to_string(), v);
            }
            moved = true;
        }
    }
    if moved {
        obj.insert("common".to_string(), common);
    }
}

/// Parse an options dict onto the Rust defaults: start from
/// `T::default()` serialised to JSON, deep-merge the caller's dict,
/// hoist flat `CommonOptions` fields, normalise friendly spellings,
/// then deserialise. Missing keys therefore keep Rust defaults, and
/// an explicit JSON null means `None` for `Option` fields.
fn parse_options<T>(options: Option<&Bound<'_, PyDict>>) -> PyResult<T>
where
    T: Default + serde::Serialize + for<'de> serde::Deserialize<'de>,
{
    let mut value =
        serde_json::to_value(T::default()).map_err(|e| invalid_argument(&e.to_string()))?;
    if let Some(dict) = options {
        let overlay = py_to_json(dict.as_any())?;
        merge_json(&mut value, overlay);
    }
    hoist_common(&mut value);
    normalize_value(&mut value)?;
    serde_json::from_value(value).map_err(|e| invalid_argument(&e.to_string()))
}

/// Parse a `CommonOptions` dict: flat common keys, or a nested
/// `"common"` object, on top of the Rust defaults. Flat keys win
/// over the nested object when both give the same field.
fn parse_common_options(options: Option<&Bound<'_, PyDict>>) -> PyResult<CommonOptions> {
    let mut value = serde_json::to_value(CommonOptions::default())
        .map_err(|e| invalid_argument(&e.to_string()))?;
    if let Some(dict) = options {
        let overlay = py_to_json(dict.as_any())?;
        if let serde_json::Value::Object(map) = &overlay {
            if let Some(common) = map.get("common") {
                merge_json(&mut value, common.clone());
            }
        }
        let mut flat = overlay.clone();
        if let serde_json::Value::Object(map) = &mut flat {
            map.remove("common");
        }
        merge_json(&mut value, flat);
    }
    normalize_value(&mut value)?;
    serde_json::from_value(value).map_err(|e| invalid_argument(&e.to_string()))
}

/// Top-level-only watermark shorthands, applied to a
/// `WatermarkOptions` JSON object before the generic normaliser.
/// (Deliberately top-level only: an annotation's `Text.text` is a
/// plain string too and must never be rewritten.)
///
/// * `text: "DRAFT"` -> `{"text": "DRAFT", "size": 48.0, "colour": white}`
/// * partial `text` object -> missing `size` / `colour` filled in
/// * `logo: <bytes>` (a number array) -> `{"image": [...], "scale": 1.0}`
/// * partial `logo` object -> missing `scale` filled in
fn normalize_watermark_top_level(value: &mut serde_json::Value) -> PyResult<()> {
    let Some(obj) = value.as_object_mut() else {
        return Ok(());
    };
    if let Some(text) = obj.get_mut("text") {
        match text.clone() {
            serde_json::Value::String(s) => {
                *text = serde_json::json!({
                    "text": s,
                    "size": 48.0,
                    "colour": Colour::white(),
                });
            }
            serde_json::Value::Object(_) => {
                let inner = text.as_object_mut().expect("object");
                if !inner.contains_key("text") {
                    return Err(invalid_argument(
                        "watermark text object needs a \"text\" string",
                    ));
                }
                if !inner.contains_key("size") {
                    inner.insert("size".to_string(), serde_json::json!(48.0));
                }
                if !inner.contains_key("colour") {
                    inner.insert(
                        "colour".to_string(),
                        serde_json::to_value(Colour::white())
                            .map_err(|e| invalid_argument(&e.to_string()))?,
                    );
                }
            }
            serde_json::Value::Null => {}
            _ => {
                return Err(invalid_argument(
                    "watermark text must be a string or {text, size, colour}",
                ));
            }
        }
    }
    if let Some(logo) = obj.get_mut("logo") {
        match logo.clone() {
            serde_json::Value::Array(_) => {
                *logo = serde_json::json!({"image": logo.clone(), "scale": 1.0});
            }
            serde_json::Value::Object(_) => {
                let inner = logo.as_object_mut().expect("object");
                if !inner.contains_key("scale") {
                    inner.insert("scale".to_string(), serde_json::json!(1.0));
                }
            }
            serde_json::Value::Null => {}
            _ => {
                return Err(invalid_argument(
                    "watermark logo must be image bytes or {image, scale}",
                ));
            }
        }
    }
    Ok(())
}

/// Parse a `WatermarkOptions` dict: same flow as `parse_options`
/// plus the top-level text/logo shorthands above.
fn parse_watermark_options(options: Option<&Bound<'_, PyDict>>) -> PyResult<WatermarkOptions> {
    let mut value = serde_json::to_value(WatermarkOptions::default())
        .map_err(|e| invalid_argument(&e.to_string()))?;
    if let Some(dict) = options {
        let overlay = py_to_json(dict.as_any())?;
        merge_json(&mut value, overlay);
    }
    hoist_common(&mut value);
    normalize_watermark_top_level(&mut value)?;
    normalize_value(&mut value)?;
    serde_json::from_value(value).map_err(|e| invalid_argument(&e.to_string()))
}

/// One annotation dict -> the serde shape
/// `{"Arrow": {"from": [x, y], ...}}`. Accepts the serde form as-is
/// or the friendly form `{"type": "arrow", "from": [...], ...}`
/// (also `"kind"` instead of `"type"`).
fn normalize_annotation(item: &mut serde_json::Value) -> PyResult<()> {
    let Some(obj) = item.as_object_mut() else {
        return Err(invalid_argument("each annotation must be a dict"));
    };
    let kind = obj
        .remove("type")
        .or_else(|| obj.remove("kind"))
        .and_then(|v| v.as_str().map(str::to_string));
    if let Some(kind) = kind {
        let variant = map_enum(
            &kind,
            &[
                ("arrow", "Arrow"),
                ("rect", "Rect"),
                ("ellipse", "Ellipse"),
                ("line", "Line"),
                ("text", "Text"),
            ],
        )?;
        let inner = serde_json::Value::Object(obj.clone());
        *item = serde_json::json!({ variant: inner });
    }
    normalize_value(item)?;
    Ok(())
}

fn parse_annotations_list(list: &Bound<'_, PyList>) -> PyResult<Vec<Annotation>> {
    let mut out = Vec::with_capacity(list.len());
    for item in list.iter() {
        let mut value = py_to_json(&item)?;
        normalize_annotation(&mut value)?;
        let ann: Annotation =
            serde_json::from_value(value).map_err(|e| invalid_argument(&e.to_string()))?;
        out.push(ann);
    }
    Ok(out)
}

fn value_to_py(py: Python<'_>, value: &serde_json::Value) -> Py<PyAny> {
    match value {
        serde_json::Value::Null => py.None(),
        serde_json::Value::Bool(b) => to_py(py, *b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                to_py(py, i)
            } else if let Some(u) = n.as_u64() {
                to_py(py, u)
            } else if let Some(f) = n.as_f64() {
                to_py(py, f)
            } else {
                py.None()
            }
        }
        serde_json::Value::String(s) => to_py(py, s),
        serde_json::Value::Array(items) => {
            let list = PyList::empty(py);
            for item in items {
                list.append(value_to_py(py, item)).expect("list append");
            }
            list.into_any().unbind()
        }
        serde_json::Value::Object(map) => {
            let dict = PyDict::new(py);
            for (k, v) in map {
                dict.set_item(k, value_to_py(py, v)).expect("dict set");
            }
            dict.into_any().unbind()
        }
    }
}

fn colour_to_py(py: Python<'_>, colour: &Colour) -> Py<PyAny> {
    let dict = PyDict::new(py);
    dict.set_item("hex", &colour.hex).expect("dict set");
    dict.set_item("r", colour.r).expect("dict set");
    dict.set_item("g", colour.g).expect("dict set");
    dict.set_item("b", colour.b).expect("dict set");
    dict.set_item("a", colour.a).expect("dict set");
    let hsl = PyList::empty(py);
    hsl.append(colour.hsl.0).expect("list append");
    hsl.append(colour.hsl.1).expect("list append");
    hsl.append(colour.hsl.2).expect("list append");
    dict.set_item("hsl", hsl).expect("dict set");
    dict.into_any().unbind()
}

fn image_info_to_py(py: Python<'_>, info: &dusrelimage::ImageInfo) -> PyResult<Py<PyAny>> {
    let value = serde_json::to_value(info).map_err(|e| invalid_argument(&e.to_string()))?;
    Ok(value_to_py(py, &value))
}

fn palette_entry_to_py(py: Python<'_>, entry: &dusrelimage::PaletteColour) -> Py<PyAny> {
    let dict = PyDict::new(py);
    dict.set_item("hex", &entry.colour.hex).expect("dict set");
    dict.set_item("r", entry.colour.r).expect("dict set");
    dict.set_item("g", entry.colour.g).expect("dict set");
    dict.set_item("b", entry.colour.b).expect("dict set");
    dict.set_item("a", entry.colour.a).expect("dict set");
    let hsl = PyList::empty(py);
    hsl.append(entry.colour.hsl.0).expect("list append");
    hsl.append(entry.colour.hsl.1).expect("list append");
    hsl.append(entry.colour.hsl.2).expect("list append");
    dict.set_item("hsl", hsl).expect("dict set");
    dict.set_item("share", entry.share).expect("dict set");
    dict.into_any().unbind()
}

// ---------------------------------------------------------------------------
// Operations
// ---------------------------------------------------------------------------

#[pyfunction]
#[pyo3(signature = (image, options=None))]
fn compress_image(
    py: Python<'_>,
    image: Vec<u8>,
    options: Option<&Bound<'_, PyDict>>,
) -> PyResult<Py<PyAny>> {
    let opts: CompressOptions = parse_options(options)?;
    let out = dusrelimage::compress_image(&image, &opts).map_err(|e| native_err(&e))?;
    Ok(PyBytes::new(py, &out).into_any().unbind())
}

#[pyfunction]
#[pyo3(signature = (image, target_bytes, options=None))]
fn compress_to_size(
    py: Python<'_>,
    image: Vec<u8>,
    target_bytes: u64,
    options: Option<&Bound<'_, PyDict>>,
) -> PyResult<Py<PyAny>> {
    let opts: CompressOptions = parse_options(options)?;
    let out =
        dusrelimage::compress_to_size(&image, target_bytes, &opts).map_err(|e| native_err(&e))?;
    Ok(PyBytes::new(py, &out).into_any().unbind())
}

#[pyfunction]
#[pyo3(signature = (image, options=None))]
fn resize_image(
    py: Python<'_>,
    image: Vec<u8>,
    options: Option<&Bound<'_, PyDict>>,
) -> PyResult<Py<PyAny>> {
    let opts: ResizeOptions = parse_options(options)?;
    let out = dusrelimage::resize_image(&image, &opts).map_err(|e| native_err(&e))?;
    Ok(PyBytes::new(py, &out).into_any().unbind())
}

#[pyfunction]
#[pyo3(signature = (image, options=None))]
fn crop_image(
    py: Python<'_>,
    image: Vec<u8>,
    options: Option<&Bound<'_, PyDict>>,
) -> PyResult<Py<PyAny>> {
    let opts: CropOptions = parse_options(options)?;
    let out = dusrelimage::crop_image(&image, &opts).map_err(|e| native_err(&e))?;
    Ok(PyBytes::new(py, &out).into_any().unbind())
}

#[pyfunction]
#[pyo3(signature = (image, options=None))]
fn convert_image(
    py: Python<'_>,
    image: Vec<u8>,
    options: Option<&Bound<'_, PyDict>>,
) -> PyResult<Py<PyAny>> {
    let opts: ConvertOptions = parse_options(options)?;
    let out = dusrelimage::convert_image(&image, &opts).map_err(|e| native_err(&e))?;
    Ok(PyBytes::new(py, &out).into_any().unbind())
}

#[pyfunction]
#[pyo3(signature = (image, options=None))]
fn add_watermark(
    py: Python<'_>,
    image: Vec<u8>,
    options: Option<&Bound<'_, PyDict>>,
) -> PyResult<Py<PyAny>> {
    let opts: WatermarkOptions = parse_watermark_options(options)?;
    let out = dusrelimage::add_watermark(&image, &opts).map_err(|e| native_err(&e))?;
    Ok(PyBytes::new(py, &out).into_any().unbind())
}

#[pyfunction]
#[pyo3(signature = (image, annotations))]
fn annotate_image(
    py: Python<'_>,
    image: Vec<u8>,
    annotations: &Bound<'_, PyList>,
) -> PyResult<Py<PyAny>> {
    // The core takes no options struct for annotate — only the
    // annotation list — so there is no options dict to parse here.
    let anns = parse_annotations_list(annotations)?;
    let out = dusrelimage::annotate_image(&image, &anns).map_err(|e| native_err(&e))?;
    Ok(PyBytes::new(py, &out).into_any().unbind())
}

#[pyfunction]
#[pyo3(signature = (image, x, y))]
fn pick_colour(py: Python<'_>, image: Vec<u8>, x: u32, y: u32) -> PyResult<Py<PyAny>> {
    let colour = dusrelimage::pick_colour(&image, x, y).map_err(|e| native_err(&e))?;
    Ok(colour_to_py(py, &colour))
}

#[pyfunction]
#[pyo3(signature = (image, count))]
fn extract_palette(py: Python<'_>, image: Vec<u8>, count: u8) -> PyResult<Py<PyAny>> {
    let colours = dusrelimage::extract_palette(&image, count).map_err(|e| native_err(&e))?;
    let list = PyList::empty(py);
    for entry in &colours {
        list.append(palette_entry_to_py(py, entry))
            .expect("list append");
    }
    Ok(list.into_any().unbind())
}

#[pyfunction]
#[pyo3(signature = (images, options=None))]
fn make_collage(
    py: Python<'_>,
    images: Vec<Vec<u8>>,
    options: Option<&Bound<'_, PyDict>>,
) -> PyResult<Py<PyAny>> {
    let opts: CollageOptions = parse_options(options)?;
    let out = dusrelimage::make_collage(&images, &opts).map_err(|e| native_err(&e))?;
    Ok(PyBytes::new(py, &out).into_any().unbind())
}

#[pyfunction]
#[pyo3(signature = (image, options=None))]
fn pixelate_image(
    py: Python<'_>,
    image: Vec<u8>,
    options: Option<&Bound<'_, PyDict>>,
) -> PyResult<Py<PyAny>> {
    let opts: PixelateOptions = parse_options(options)?;
    let out = dusrelimage::pixelate_image(&image, &opts).map_err(|e| native_err(&e))?;
    Ok(PyBytes::new(py, &out).into_any().unbind())
}

#[pyfunction]
#[pyo3(signature = (image, options=None))]
fn get_image_info(
    py: Python<'_>,
    image: Vec<u8>,
    options: Option<&Bound<'_, PyDict>>,
) -> PyResult<Py<PyAny>> {
    let common: CommonOptions = parse_common_options(options)?;
    let info = dusrelimage::get_image_info(&image, &common).map_err(|e| native_err(&e))?;
    image_info_to_py(py, &info)
}

/// Batch helper: delegates to the core's `bulk_process`.
///
/// `op` names one tool: `compress`, `compress_to_size` (needs
/// `target_bytes`; the core has no `BulkOp` for it, so the loop
/// lives here), `resize`, `crop`, `convert`, `watermark`,
/// `annotate` (needs `annotations`), `pick_colour` (needs `x`,
/// `y`), `palette` (needs `count`), `collage` (per-item
/// `INVALID_ARGUMENT` from the core — a collage combines all
/// inputs), `pixelate`. Returns one dict per input, in order:
/// `{"index", "output", "error_code", "error"}`. `output` is bytes
/// for image ops, a colour dict for `pick_colour`, a list of
/// palette dicts for `palette`, and `None` on failure. One bad
/// input fails only its own item, never the whole batch.
#[pyfunction]
#[allow(clippy::too_many_arguments)] // pyfunction signature mirrors the Python API
#[pyo3(signature = (images, op, options=None, target_bytes=None, annotations=None, x=None, y=None, count=None))]
fn bulk_process(
    py: Python<'_>,
    images: Vec<Vec<u8>>,
    op: &str,
    options: Option<&Bound<'_, PyDict>>,
    target_bytes: Option<u64>,
    annotations: Option<&Bound<'_, PyList>>,
    x: Option<u32>,
    y: Option<u32>,
    count: Option<u8>,
) -> PyResult<Py<PyAny>> {
    // Parse the options once, up front, so a bad options dict fails
    // the whole call instead of every item.
    let list = PyList::empty(py);
    let push_item = |py: Python<'_>,
                     list: &Bound<'_, PyList>,
                     index: usize,
                     output: Py<PyAny>,
                     error: Option<&dusrelimage::DusrelError>| {
        let dict = PyDict::new(py);
        dict.set_item("index", index).expect("dict set");
        dict.set_item("output", output).expect("dict set");
        match error {
            Some(e) => {
                dict.set_item("error_code", e.code_str()).expect("dict set");
                dict.set_item("error", e.message_en()).expect("dict set");
            }
            None => {
                dict.set_item("error_code", py.None()).expect("dict set");
                dict.set_item("error", py.None()).expect("dict set");
            }
        }
        list.append(dict).expect("list append");
    };

    // compress_to_size has no core BulkOp: loop here, same contract.
    if op == "compress_to_size" {
        let target = target_bytes.ok_or_else(|| {
            invalid_argument("bulk_process op \"compress_to_size\" needs target_bytes")
        })?;
        let opts: CompressOptions = parse_options(options)?;
        for (index, image) in images.iter().enumerate() {
            match dusrelimage::compress_to_size(image, target, &opts) {
                Ok(bytes) => push_item(
                    py,
                    &list,
                    index,
                    PyBytes::new(py, &bytes).into_any().unbind(),
                    None,
                ),
                Err(e) => push_item(py, &list, index, py.None(), Some(&e)),
            }
        }
        return Ok(list.into_any().unbind());
    }

    let bulk_op = match op {
        "compress" => BulkOp::Compress(parse_options(options)?),
        "resize" => BulkOp::Resize(parse_options(options)?),
        "crop" => BulkOp::Crop(parse_options(options)?),
        "convert" => BulkOp::Convert(parse_options(options)?),
        "watermark" => BulkOp::Watermark(parse_watermark_options(options)?),
        "pixelate" => BulkOp::Pixelate(parse_options(options)?),
        "collage" => BulkOp::Collage(parse_options(options)?),
        "annotate" => {
            let list = annotations.ok_or_else(|| {
                invalid_argument("bulk_process op \"annotate\" needs annotations")
            })?;
            let anns = parse_annotations_list(list)?;
            if anns.is_empty() {
                return Err(invalid_argument(
                    "bulk_process op \"annotate\" needs at least one annotation",
                ));
            }
            BulkOp::Annotate(anns)
        }
        "pick_colour" => {
            let (x, y) = match (x, y) {
                (Some(x), Some(y)) => (x, y),
                _ => {
                    return Err(invalid_argument(
                        "bulk_process op \"pick_colour\" needs x and y",
                    ));
                }
            };
            BulkOp::PickColour { x, y }
        }
        "palette" => {
            let count =
                count.ok_or_else(|| invalid_argument("bulk_process op \"palette\" needs count"))?;
            BulkOp::Palette { count }
        }
        other => {
            return Err(invalid_argument(&format!(
                "unknown bulk op \"{other}\"; use compress, compress_to_size, resize, crop, convert, watermark, annotate, pick_colour, palette, collage or pixelate"
            )));
        }
    };

    for item in dusrelimage::bulk_process(&images, &bulk_op) {
        match &item.result {
            Ok(BulkOutput::Image(bytes)) => push_item(
                py,
                &list,
                item.index,
                PyBytes::new(py, bytes).into_any().unbind(),
                None,
            ),
            Ok(BulkOutput::Colour(colour)) => {
                push_item(py, &list, item.index, colour_to_py(py, colour), None)
            }
            Ok(BulkOutput::Palette(colours)) => {
                let palette = PyList::empty(py);
                for entry in colours {
                    palette
                        .append(palette_entry_to_py(py, entry))
                        .expect("list append");
                }
                push_item(py, &list, item.index, palette.into(), None);
            }
            Err(e) => push_item(py, &list, item.index, py.None(), Some(e)),
        }
    }
    Ok(list.into_any().unbind())
}

// ---------------------------------------------------------------------------
// Fixtures (test data — same builders the Rust tests use)
// ---------------------------------------------------------------------------

#[pyfunction]
#[pyo3(signature = (kind="noise", width=64, height=64, seed=42))]
fn make_test_image(
    py: Python<'_>,
    kind: &str,
    width: u32,
    height: u32,
    seed: u64,
) -> PyResult<Py<PyAny>> {
    use dusrelimage::fixtures::MockKind;
    let kind = match kind {
        "solid" => MockKind::Solid,
        "gradient" => MockKind::Gradient,
        "noise" => MockKind::Noise,
        other => {
            return Err(invalid_argument(&format!(
                "unknown test image kind \"{other}\"; use solid, gradient or noise"
            )));
        }
    };
    let out = dusrelimage::fixtures::make_test_image(kind, width, height, seed);
    Ok(PyBytes::new(py, &out).into_any().unbind())
}

#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(compress_image, m)?)?;
    m.add_function(wrap_pyfunction!(compress_to_size, m)?)?;
    m.add_function(wrap_pyfunction!(resize_image, m)?)?;
    m.add_function(wrap_pyfunction!(crop_image, m)?)?;
    m.add_function(wrap_pyfunction!(convert_image, m)?)?;
    m.add_function(wrap_pyfunction!(add_watermark, m)?)?;
    m.add_function(wrap_pyfunction!(annotate_image, m)?)?;
    m.add_function(wrap_pyfunction!(pick_colour, m)?)?;
    m.add_function(wrap_pyfunction!(extract_palette, m)?)?;
    m.add_function(wrap_pyfunction!(make_collage, m)?)?;
    m.add_function(wrap_pyfunction!(pixelate_image, m)?)?;
    m.add_function(wrap_pyfunction!(get_image_info, m)?)?;
    m.add_function(wrap_pyfunction!(bulk_process, m)?)?;
    m.add_function(wrap_pyfunction!(make_test_image, m)?)?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
