//! WASM bindings for dusrelimage — thin wrappers over the Rust core.
//!
//! All image logic lives in the `dusrelimage` crate; this crate only
//! converts between JavaScript values and Rust types. Build with:
//! `wasm-pack build --target web` (website playground) or
//! `wasm-pack build --target nodejs` (the npm package's tests).
//!
//! Contract (mirrors dusrelpdf):
//! * Binary results are `Uint8Array`s.
//! * On failure the function throws a JSON string:
//!   `{"ok":false,"code":"INVALID_IMAGE","message_en":"…","message_hi":"…",
//!   "reason_en":"…","reason_hi":"…"}` — both `message_*` and
//!   `reason_*` keys are emitted because the playground reads
//!   `message_*` and the TS wrapper accepts either.
//! * Options are plain JS objects. Unlike dusrelpdf, PARTIAL objects
//!   work here: the playground passes e.g. `{ quality: 70 }` or
//!   `{ filter: "Lanczos3" }`, so missing fields are filled from the
//!   Rust `Default` before deserialising (see `parse_options`). The TS
//!   wrapper still sends complete objects.
//! * `get_image_info`, `pick_colour` and `extract_palette` return JSON
//!   strings (the playground does `JSON.parse()` on them).
//! * `extract_palette` JSON is FLATTENED per entry —
//!   `{ hex, r, g, b, a, hsl, share }` — because the playground reads
//!   `c.hex` / `c.share` at the top level, not `c.colour.hex`.
//!   (The Rust `PaletteColour` is `{ colour, share }`; the flattening
//!   happens only in this JSON serialisation.)
//! * `bulk_process` returns a JSON string of `BulkResult` objects
//!   (see the docs page `reference/info-and-bulk`); outputs are
//!   base64 inside the JSON — the TS wrapper decodes them back to
//!   `Uint8Array`s.
//! * `target_bytes` for `compress_to_size` is `f64` on the JS side,
//!   NOT `u64`: wasm-bindgen would demand a `BigInt` for `u64`, but the
//!   playground passes a plain JS number (`parseInt(...)`).
//!
//! JS-shorthand normalisation (playground sends human-friendly JSON,
//! the core wants strict serde shapes) — all done in `normalise`:
//! * Colour strings (`"#ff0000"`, `"#fff"`, `"#RRGGBBAA"`) in any
//!   `colour` / `background` / `fill` field become full `Colour`
//!   objects (`{ hex, r, g, b, a, hsl }`).
//! * Collage `layout` strings (`"2x2"`, `"grid"`, `"row"`, `"column"`)
//!   become the `CollageLayout` enum shape.
//! * Annotations in `{ "type": "Arrow", ...fields }` form (as the
//!   playground's JSON textarea produces) become serde's
//!   externally-tagged form (`{ "Arrow": { ...fields } }`).

use dusrelimage::{
    Annotation, BulkOp, BulkOutput, CollageOptions, Colour, CommonOptions, CompressOptions,
    ConvertOptions, CropOptions, DusrelError, PixelateOptions, ResizeOptions, WatermarkOptions,
    add_watermark as core_add_watermark, annotate_image as core_annotate_image,
    bulk_process as core_bulk_process, compress_image as core_compress_image,
    compress_to_size as core_compress_to_size, convert_image as core_convert_image,
    crop_image as core_crop_image, extract_palette as core_extract_palette,
    get_image_info as core_get_image_info, make_collage as core_make_collage,
    pick_colour as core_pick_colour, pixelate_image as core_pixelate_image,
    resize_image as core_resize_image,
};
use js_sys::{Array, Uint8Array};
use serde::de::DeserializeOwned;
use wasm_bindgen::prelude::*;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

fn err_js(e: DusrelError) -> JsValue {
    JsValue::from_str(
        &serde_json::json!({
            "ok": false,
            "code": e.code_str(),
            "message_en": e.message_en(),
            "message_hi": e.message_hi(),
            "reason_en": e.reason_en(),
            "reason_hi": e.reason_hi(),
        })
        .to_string(),
    )
}

fn invalid_argument(en: &str, hi: &str) -> JsValue {
    err_js(DusrelError::invalid_argument(en, hi))
}

// ---------------------------------------------------------------------------
// Options parsing: partial JS object -> complete Rust struct.
// ---------------------------------------------------------------------------

/// Deep-merge `patch` over `base` (objects merge key-by-key, anything
/// else is replaced wholesale). `null` in the patch wins over a base
/// object — that is how an explicit `null` stays `None`.
fn deep_merge(base: serde_json::Value, patch: serde_json::Value) -> serde_json::Value {
    match (base, patch) {
        (serde_json::Value::Object(mut b), serde_json::Value::Object(p)) => {
            for (k, v) in p {
                let merged = match b.remove(&k) {
                    Some(old) => deep_merge(old, v),
                    None => v,
                };
                b.insert(k, merged);
            }
            serde_json::Value::Object(b)
        }
        (_, patch) => patch,
    }
}

/// Turn a `"#RRGGBB"` / `"#RGB"` / `"#RRGGBBAA"` string into a full
/// `Colour` JSON object. Returns the input unchanged when it is not a
/// parseable hex colour (deserialisation then fails with a proper
/// `INVALID_ARGUMENT` instead of us guessing).
fn colour_json(value: &serde_json::Value) -> serde_json::Value {
    if let serde_json::Value::String(s) = value
        && let Ok(colour) = Colour::from_hex(s)
        && let Ok(v) = serde_json::to_value(&colour)
    {
        return v;
    }
    value.clone()
}

/// Turn a layout shorthand into the `CollageLayout` serde shape:
/// `"2x2"` -> `{"Grid":{"rows":2,"cols":2}}`, `"row"` -> `"Row"`,
/// `"column"` -> `"Column"`, `"grid"` -> 2x2 grid. Anything else is
/// returned unchanged (deserialisation / core validation rejects it).
fn layout_json(value: &serde_json::Value) -> serde_json::Value {
    if let serde_json::Value::String(s) = value {
        let lower = s.trim().to_ascii_lowercase();
        match lower.as_str() {
            "row" => return serde_json::Value::String("Row".to_string()),
            "column" => return serde_json::Value::String("Column".to_string()),
            "grid" => {
                return serde_json::json!({"Grid": {"rows": 2, "cols": 2}});
            }
            _ => {
                if let Some((r, c)) = lower.split_once('x')
                    && let (Ok(rows), Ok(cols)) = (r.trim().parse::<u32>(), c.trim().parse::<u32>())
                {
                    return serde_json::json!({"Grid": {"rows": rows, "cols": cols}});
                }
            }
        }
    }
    value.clone()
}

/// Turn one annotation from the playground's `{ "type": "Arrow", ... }`
/// form into serde's externally-tagged `{ "Arrow": { ... } }` form.
/// An already-tagged object passes through (recursion still normalises
/// its colour fields).
fn annotation_json(value: &serde_json::Value) -> serde_json::Value {
    let Some(obj) = value.as_object() else {
        return value.clone();
    };
    if let Some(serde_json::Value::String(kind)) = obj.get("type") {
        let mut fields = obj.clone();
        fields.remove("type");
        let inner = normalise(serde_json::Value::Object(fields));
        let mut out = serde_json::Map::new();
        out.insert(kind.clone(), inner);
        return serde_json::Value::Object(out);
    }
    normalise(value.clone())
}

/// Normalise a JSON value coming from JS: hex-string colours, layout
/// shorthands and `{ "type": ... }` annotations, recursively.
fn normalise(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut out = serde_json::Map::with_capacity(map.len());
            for (k, v) in map {
                let v = match k.as_str() {
                    "colour" | "background" | "fill" => colour_json(&v),
                    "layout" => layout_json(&v),
                    "annotations" => match v {
                        serde_json::Value::Array(items) => {
                            serde_json::Value::Array(items.iter().map(annotation_json).collect())
                        }
                        other => normalise(other),
                    },
                    _ => normalise(v),
                };
                out.insert(k, v);
            }
            serde_json::Value::Object(out)
        }
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.into_iter().map(normalise).collect())
        }
        other => other,
    }
}

/// Parse a JS options value into a Rust options struct. `undefined` /
/// `null` / `{}` mean "all defaults". A partial object is deep-merged
/// over the serialised `Default` first, so e.g. `{ quality: 70 }` is
/// a complete `CompressOptions` by the time serde sees it.
fn parse_options<T: Default + DeserializeOwned + serde::Serialize>(
    value: &JsValue,
) -> Result<T, JsValue> {
    if value.is_undefined() || value.is_null() {
        return Ok(T::default());
    }
    let provided: serde_json::Value =
        serde_wasm_bindgen::from_value(value.clone()).map_err(|e| {
            invalid_argument(
                &format!("Invalid options: {e}"),
                &format!("Galat options hain: {e}"),
            )
        })?;
    let defaults = serde_json::to_value(T::default()).map_err(|_| {
        invalid_argument(
            "Could not serialise default options.",
            "Default options serialise nahi ho paye.",
        )
    })?;
    let merged = deep_merge(defaults, normalise(provided));
    serde_json::from_value(merged).map_err(|e| {
        invalid_argument(
            &format!("Invalid options: {e}"),
            &format!("Galat options hain: {e}"),
        )
    })
}

/// Parse a `CommonOptions` out of an options object (flat: the
/// `common` fields live at the top level of the JS object, exactly
/// like dusrelpdf's `common()` helper).
fn common(options: &JsValue) -> Result<CommonOptions, JsValue> {
    parse_options::<CommonOptions>(options)
}

/// Parse an options struct, then — only if the JS object carries any
/// `CommonOptions` field FLAT at the top level (`max_pixels`, …) —
/// overlay those flat fields onto the struct's embedded `common`.
/// A nested `common: {...}` object is honoured by `parse_options`
/// itself (deep-merged over the defaults) and is NOT clobbered here.
trait HasCommon {
    fn set_common(&mut self, common: CommonOptions);
    fn common(&self) -> &CommonOptions;
}

macro_rules! impl_has_common {
    ($t:ty) => {
        impl HasCommon for $t {
            fn set_common(&mut self, common: CommonOptions) {
                self.common = common;
            }
            fn common(&self) -> &CommonOptions {
                &self.common
            }
        }
    };
}

impl_has_common!(CompressOptions);
impl_has_common!(ConvertOptions);
impl_has_common!(CropOptions);
impl_has_common!(ResizeOptions);
impl_has_common!(WatermarkOptions);
impl_has_common!(CollageOptions);
impl_has_common!(PixelateOptions);

const COMMON_KEYS: [&str; 5] = [
    "auto_orient",
    "keep_exif",
    "dpi",
    "max_pixels",
    "max_dimension",
];

fn with_common<T: Default + DeserializeOwned + serde::Serialize + HasCommon>(
    options: &JsValue,
) -> Result<T, JsValue> {
    let mut parsed = parse_options::<T>(options)?;
    if options.is_undefined() || options.is_null() {
        return Ok(parsed);
    }
    let provided: serde_json::Value =
        serde_wasm_bindgen::from_value(options.clone()).map_err(|e| {
            invalid_argument(
                &format!("Invalid options: {e}"),
                &format!("Galat options hain: {e}"),
            )
        })?;
    let Some(obj) = provided.as_object() else {
        return Ok(parsed);
    };
    let flat: serde_json::Map<String, serde_json::Value> = obj
        .iter()
        .filter(|(k, _)| COMMON_KEYS.contains(&k.as_str()))
        .map(|(k, v)| (k.clone(), normalise(v.clone())))
        .collect();
    if flat.is_empty() {
        return Ok(parsed);
    }
    let base = serde_json::to_value(parsed.common()).map_err(|_| {
        invalid_argument(
            "Could not serialise common options.",
            "Common options serialise nahi ho paye.",
        )
    })?;
    let merged = deep_merge(base, serde_json::Value::Object(flat));
    let common: CommonOptions = serde_json::from_value(merged).map_err(|e| {
        invalid_argument(
            &format!("Invalid options: {e}"),
            &format!("Galat options hain: {e}"),
        )
    })?;
    parsed.set_common(common);
    Ok(parsed)
}

fn bytes_array(value: &JsValue) -> Result<Vec<Vec<u8>>, JsValue> {
    if !Array::is_array(value) {
        return Err(invalid_argument(
            "Inputs must be an array of Uint8Array.",
            "Inputs Uint8Array ki array honi chahiye.",
        ));
    }
    let arr = Array::from(value);
    let mut out = Vec::with_capacity(arr.length() as usize);
    for item in arr.iter() {
        out.push(Uint8Array::new(&item).to_vec());
    }
    Ok(out)
}

/// `Uint8Array` logo bytes inside an options object arrive as a plain
/// JS number array (serde-wasm-bindgen cannot decode a `Uint8Array`
/// into `Vec<u8>`); pull them out by hand when present, mirroring
/// dusrelpdf's `options_image_bytes`.
fn logo_image_bytes(options: &JsValue) -> Option<Vec<u8>> {
    let map = serde_wasm_bindgen::from_value::<serde_json::Map<String, serde_json::Value>>(
        options.clone(),
    )
    .ok()?;
    let logo = map.get("logo")?.as_object()?;
    let arr = logo.get("image")?.as_array()?;
    Some(arr.iter().map(|v| v.as_u64().unwrap_or(0) as u8).collect())
}

fn parse_annotations(value: &JsValue) -> Result<Vec<Annotation>, JsValue> {
    if value.is_undefined() || value.is_null() {
        return Ok(Vec::new());
    }
    let provided: serde_json::Value =
        serde_wasm_bindgen::from_value(value.clone()).map_err(|e| {
            invalid_argument(
                &format!("Invalid annotations: {e}"),
                &format!("Galat annotations hain: {e}"),
            )
        })?;
    let normalised = match provided {
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.iter().map(annotation_json).collect())
        }
        other => other,
    };
    serde_json::from_value(normalised).map_err(|e| {
        invalid_argument(
            &format!("Invalid annotations: {e}"),
            &format!("Galat annotations hain: {e}"),
        )
    })
}

mod base64_for_wasm {
    const CHARS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    pub fn encode(data: &[u8]) -> String {
        let mut out = String::with_capacity(data.len() * 4 / 3 + 4);
        for chunk in data.chunks(3) {
            let b = [
                chunk[0],
                *chunk.get(1).unwrap_or(&0),
                *chunk.get(2).unwrap_or(&0),
            ];
            let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
            out.push(CHARS[(n >> 18 & 63) as usize] as char);
            out.push(CHARS[(n >> 12 & 63) as usize] as char);
            out.push(if chunk.len() > 1 {
                CHARS[(n >> 6 & 63) as usize] as char
            } else {
                '='
            });
            out.push(if chunk.len() > 2 {
                CHARS[(n & 63) as usize] as char
            } else {
                '='
            });
        }
        out
    }
}

// ---------------------------------------------------------------------------
// Exports
// ---------------------------------------------------------------------------

#[wasm_bindgen]
pub fn version() -> String {
    dusrelimage::VERSION.to_string()
}

/// Build a deterministic test image (PNG bytes) — `kind` is
/// `"solid"`, `"gradient"` or `"noise"` (see `fixtures::MockKind`).
/// Test data only, mirroring dusrelpdf's `make_test_pdf`.
#[wasm_bindgen]
pub fn make_test_image(kind: &str, width: u32, height: u32, seed: u64) -> Result<Vec<u8>, JsValue> {
    use dusrelimage::fixtures::{MockKind, make_test_image as core_make_test_image};
    let kind = match kind {
        "solid" => MockKind::Solid,
        "gradient" => MockKind::Gradient,
        "noise" => MockKind::Noise,
        other => {
            return Err(invalid_argument(
                &format!("Unknown test image kind \"{other}\". Use solid, gradient or noise."),
                &format!(
                    "Unknown test image kind \"{other}\" hai. solid, gradient ya noise use karein."
                ),
            ));
        }
    };
    Ok(core_make_test_image(kind, width, height, seed))
}

/// Tool 1 — re-encode at a fixed quality (and `format`, or the input
/// format when unset).
#[wasm_bindgen]
pub fn compress_image(input: &[u8], options: JsValue) -> Result<Vec<u8>, JsValue> {
    let opts = with_common::<CompressOptions>(&options)?;
    core_compress_image(input, &opts).map_err(err_js)
}

/// Tool 1 — re-encode so the result is at most `target_bytes` bytes.
/// `target_bytes` is `f64` (plain JS number) — see the header note.
#[wasm_bindgen]
pub fn compress_to_size(
    input: &[u8],
    target_bytes: f64,
    options: JsValue,
) -> Result<Vec<u8>, JsValue> {
    if !target_bytes.is_finite() || target_bytes < 0.0 || target_bytes > u64::MAX as f64 {
        return Err(invalid_argument(
            "Target size must be a non-negative number of bytes.",
            "Target size bytes ki non-negative number honi chahiye.",
        ));
    }
    let opts = with_common::<CompressOptions>(&options)?;
    core_compress_to_size(input, target_bytes as u64, &opts).map_err(err_js)
}

/// Tool 2 — resize (width and/or height, percent, fit, filter).
#[wasm_bindgen]
pub fn resize_image(input: &[u8], options: JsValue) -> Result<Vec<u8>, JsValue> {
    let opts = with_common::<ResizeOptions>(&options)?;
    core_resize_image(input, &opts).map_err(err_js)
}

/// Tool 3 — crop to a rectangle (or an anchor preset).
#[wasm_bindgen]
pub fn crop_image(input: &[u8], options: JsValue) -> Result<Vec<u8>, JsValue> {
    let opts = with_common::<CropOptions>(&options)?;
    core_crop_image(input, &opts).map_err(err_js)
}

/// Tool 4 — convert to another format.
#[wasm_bindgen]
pub fn convert_image(input: &[u8], options: JsValue) -> Result<Vec<u8>, JsValue> {
    let opts = with_common::<ConvertOptions>(&options)?;
    core_convert_image(input, &opts).map_err(err_js)
}

/// Tool 5 — stamp a text or logo watermark.
#[wasm_bindgen]
pub fn add_watermark(input: &[u8], options: JsValue) -> Result<Vec<u8>, JsValue> {
    let mut opts = with_common::<WatermarkOptions>(&options)?;
    // Logo bytes arrive as a plain JS number array inside the options
    // object (see `logo_image_bytes`); fill them in when present.
    if let Some(bytes) = logo_image_bytes(&options)
        && let Some(logo) = opts.logo.as_mut()
    {
        logo.image = bytes;
    }
    core_add_watermark(input, &opts).map_err(err_js)
}

/// Tool 6 — draw annotations (JS Array of `{ "type": ..., ... }`
/// objects, as the playground produces, or serde-tagged objects).
#[wasm_bindgen]
pub fn annotate_image(input: &[u8], annotations: JsValue) -> Result<Vec<u8>, JsValue> {
    let parsed = parse_annotations(&annotations)?;
    core_annotate_image(input, &parsed).map_err(err_js)
}

/// Tool 7 — colour of one pixel, as a JSON string:
/// `{ hex, r, g, b, a, hsl: [h, s, l] }`.
#[wasm_bindgen]
pub fn pick_colour(input: &[u8], x: u32, y: u32) -> Result<String, JsValue> {
    let colour = core_pick_colour(input, x, y).map_err(err_js)?;
    serde_json::to_string(&colour).map_err(|_| {
        invalid_argument(
            "Could not serialise colour.",
            "Colour serialise nahi ho paya.",
        )
    })
}

/// Tool 8 — dominant colours as a JSON string. Each entry is
/// FLATTENED — `{ hex, r, g, b, a, hsl, share }` — because the
/// playground reads `c.hex` / `c.share` at the top level.
#[wasm_bindgen]
pub fn extract_palette(input: &[u8], count: u8) -> Result<String, JsValue> {
    let palette = core_extract_palette(input, count).map_err(err_js)?;
    let flat: Vec<serde_json::Value> = palette
        .iter()
        .map(|entry| {
            let mut v = serde_json::to_value(&entry.colour).unwrap_or(serde_json::json!({}));
            if let serde_json::Value::Object(ref mut map) = v {
                map.insert("share".to_string(), serde_json::json!(entry.share));
            }
            v
        })
        .collect();
    serde_json::to_string(&flat).map_err(|_| {
        invalid_argument(
            "Could not serialise palette.",
            "Palette serialise nahi ho paya.",
        )
    })
}

/// Tool 9 — collage from several images (JS Array of Uint8Array).
#[wasm_bindgen]
pub fn make_collage(inputs: JsValue, options: JsValue) -> Result<Vec<u8>, JsValue> {
    let bufs = bytes_array(&inputs)?;
    let opts = with_common::<CollageOptions>(&options)?;
    core_make_collage(&bufs, &opts).map_err(err_js)
}

/// Tool 10 — pixelate a rectangle (whole image when width/height
/// are 0).
#[wasm_bindgen]
pub fn pixelate_image(input: &[u8], options: JsValue) -> Result<Vec<u8>, JsValue> {
    let opts = with_common::<PixelateOptions>(&options)?;
    core_pixelate_image(input, &opts).map_err(err_js)
}

/// Image summary as a JSON string:
/// `{ format, width, height, has_alpha, colour_type, file_size }`.
/// Called by the playground with ONE argument (no options) — the
/// options parameter is optional and defaults when omitted.
#[wasm_bindgen]
pub fn get_image_info(input: &[u8], options: JsValue) -> Result<String, JsValue> {
    let info = core_get_image_info(input, &common(&options)?).map_err(err_js)?;
    serde_json::to_string(&info).map_err(|_| {
        invalid_argument(
            "Could not serialise image info.",
            "Image info serialise nahi ho paya.",
        )
    })
}

/// Run one operation over many inputs; one result per input, same
/// order. Delegates to the core's `bulk_process` (SPEC section 1),
/// so the per-item logic lives in exactly one place.
///
/// `op` is a short name (`"compress"`, `"resize"`, `"crop"`,
/// `"convert"`, `"watermark"`, `"annotate"`, `"pick_colour"`,
/// `"palette"`, `"pixelate"`) or the full snake_case export name
/// (`"compress_image"`, …). `"compress_to_size"` is the one op the
/// core's `BulkOp` does NOT carry, so it is dispatched here, per item,
/// with `target_bytes` (or `targetBytes`) read from `options`;
/// `"annotate"` reads `annotations` from `options`, `"pick_colour"`
/// reads `x`/`y`, `"palette"` reads `count`. `"collage"` is accepted
/// but the core reports it per item as `INVALID_ARGUMENT` (a collage
/// combines all inputs — call `make_collage` once instead).
///
/// Returns a JSON string of `BulkResult`s
/// (`{ index, ok, output?: base64, colour?: {...}, palette?: [...],
/// error?: { code, message_en, message_hi } }`); one bad input fails
/// only its own item.
#[wasm_bindgen]
pub fn bulk_process(inputs: JsValue, op: &str, options: JsValue) -> Result<String, JsValue> {
    use base64_for_wasm::encode as b64;
    let bufs = bytes_array(&inputs)?;
    if bufs.is_empty() {
        // Core semantics: `bulk_process` over zero inputs returns an
        // empty result list, it does not throw `EMPTY_INPUT` (the TS
        // wrapper short-circuits `[]` the same way before calling us).
        return Ok("[]".to_string());
    }
    // Options JSON, for the op-specific extras (target_bytes,
    // annotations, x/y, count). Missing/undefined options = {}.
    let opts_json: serde_json::Value = if options.is_undefined() || options.is_null() {
        serde_json::json!({})
    } else {
        serde_wasm_bindgen::from_value(options.clone()).map_err(|e| {
            invalid_argument(
                &format!("Invalid options: {e}"),
                &format!("Galat options hain: {e}"),
            )
        })?
    };

    // `compress_to_size` has no core `BulkOp` variant: run it here.
    if op == "compress_to_size" {
        let target_bytes = opts_json
            .get("target_bytes")
            .or_else(|| opts_json.get("targetBytes"))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        if !target_bytes.is_finite() || target_bytes <= 0.0 {
            return Err(invalid_argument(
                "bulk compress_to_size needs target_bytes in options.",
                "bulk compress_to_size ke liye options me target_bytes chahiye.",
            ));
        }
        let opts = parse_with_common_wasm::<CompressOptions>(&options).map_err(err_js_from_bulk)?;
        let results: Vec<serde_json::Value> = bufs
            .iter()
            .enumerate()
            .map(
                |(index, input)| match core_compress_to_size(input, target_bytes as u64, &opts) {
                    Ok(out) => serde_json::json!({
                        "index": index,
                        "ok": true,
                        "output": b64(&out),
                    }),
                    Err(e) => bulk_item_error(index, &e),
                },
            )
            .collect();
        return Ok(serde_json::to_string(&results).unwrap());
    }

    let bulk_op = match op {
        "compress" | "compress_image" => BulkOp::Compress(
            parse_with_common_wasm::<CompressOptions>(&options).map_err(err_js_from_bulk)?,
        ),
        "resize" | "resize_image" => BulkOp::Resize(
            parse_with_common_wasm::<ResizeOptions>(&options).map_err(err_js_from_bulk)?,
        ),
        "crop" | "crop_image" => {
            BulkOp::Crop(parse_with_common_wasm::<CropOptions>(&options).map_err(err_js_from_bulk)?)
        }
        "convert" | "convert_image" => BulkOp::Convert(
            parse_with_common_wasm::<ConvertOptions>(&options).map_err(err_js_from_bulk)?,
        ),
        "watermark" | "add_watermark" => {
            let mut opts =
                parse_with_common_wasm::<WatermarkOptions>(&options).map_err(err_js_from_bulk)?;
            if let Some(bytes) = logo_image_bytes(&options)
                && let Some(logo) = opts.logo.as_mut()
            {
                logo.image = bytes;
            }
            BulkOp::Watermark(opts)
        }
        "annotate" | "annotate_image" => BulkOp::Annotate(parse_annotations_json(
            opts_json
                .get("annotations")
                .cloned()
                .unwrap_or(serde_json::Value::Null),
        )?),
        "pick_colour" | "pick-colour" | "pickColour" => BulkOp::PickColour {
            x: opts_json.get("x").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
            y: opts_json.get("y").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
        },
        "palette" | "extract_palette" => BulkOp::Palette {
            count: opts_json.get("count").and_then(|v| v.as_u64()).unwrap_or(5) as u8,
        },
        "collage" | "make_collage" => BulkOp::Collage(
            parse_with_common_wasm::<CollageOptions>(&options).map_err(err_js_from_bulk)?,
        ),
        "pixelate" | "pixelate_image" => BulkOp::Pixelate(
            parse_with_common_wasm::<PixelateOptions>(&options).map_err(err_js_from_bulk)?,
        ),
        other => {
            return Err(invalid_argument(
                &format!(
                    "Unknown bulk op \"{other}\". Use compress, compress_to_size, resize, crop, convert, watermark, annotate, pick_colour, palette or pixelate."
                ),
                &format!(
                    "Unknown bulk op \"{other}\" hai. compress, compress_to_size, resize, crop, convert, watermark, annotate, pick_colour, palette ya pixelate use karein."
                ),
            ));
        }
    };

    let results: Vec<serde_json::Value> = core_bulk_process(&bufs, &bulk_op)
        .into_iter()
        .map(|item| match item.result {
            Ok(BulkOutput::Image(out)) => serde_json::json!({
                "index": item.index,
                "ok": true,
                "output": b64(&out),
            }),
            Ok(BulkOutput::Colour(colour)) => serde_json::json!({
                "index": item.index,
                "ok": true,
                "colour": serde_json::to_value(&colour).unwrap_or(serde_json::json!({})),
            }),
            Ok(BulkOutput::Palette(entries)) => serde_json::json!({
                "index": item.index,
                "ok": true,
                "palette": palette_flat_json(&entries),
            }),
            Err(e) => bulk_item_error(item.index, &e),
        })
        .collect();
    Ok(serde_json::to_string(&results).unwrap())
}

/// One failed bulk item as a JSON value.
fn bulk_item_error(index: usize, e: &DusrelError) -> serde_json::Value {
    serde_json::json!({
        "index": index,
        "ok": false,
        "error": {
            "code": e.code_str(),
            "message_en": e.message_en(),
            "message_hi": e.message_hi(),
        },
    })
}

/// Convert a `DusrelError` from the bulk per-item parse path into the
/// `JsValue` throw shape (same JSON as `err_js`).
fn err_js_from_bulk(e: DusrelError) -> JsValue {
    err_js(e)
}

/// Flatten palette entries for JSON: `{ hex, r, g, b, a, hsl, share }`
/// per entry (same shape as `extract_palette` — the playground reads
/// `c.hex` / `c.share` at the top level).
fn palette_flat_json(entries: &[dusrelimage::PaletteColour]) -> Vec<serde_json::Value> {
    entries
        .iter()
        .map(|entry| {
            let mut v = serde_json::to_value(&entry.colour).unwrap_or(serde_json::json!({}));
            if let serde_json::Value::Object(ref mut map) = v {
                map.insert("share".to_string(), serde_json::json!(entry.share));
            }
            v
        })
        .collect()
}

/// Parse annotations from an already-deserialised JSON value
/// (the `annotations` field of a bulk-process options object).
fn parse_annotations_json(value: serde_json::Value) -> Result<Vec<Annotation>, JsValue> {
    if value.is_null() {
        return Ok(Vec::new());
    }
    let normalised = match value {
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.iter().map(annotation_json).collect())
        }
        other => other,
    };
    serde_json::from_value(normalised).map_err(|e| {
        invalid_argument(
            &format!("Invalid annotations: {e}"),
            &format!("Galat annotations hain: {e}"),
        )
    })
}

/// Same as `with_common`, but usable inside the `bulk_process`
/// per-item closure (which returns `Result<_, DusrelError>`).
fn parse_with_common_wasm<T: Default + DeserializeOwned + serde::Serialize + HasCommon>(
    value: &JsValue,
) -> Result<T, DusrelError> {
    with_common::<T>(value).map_err(|e| {
        // The JsValue is our own INVALID_ARGUMENT JSON string; recover
        // the message from it, falling back to a generic one.
        if let Some(s) = e.as_string()
            && let Ok(v) = serde_json::from_str::<serde_json::Value>(&s)
        {
            return DusrelError::invalid_argument(
                v.get("message_en")
                    .and_then(|m| m.as_str())
                    .unwrap_or("Invalid options."),
                v.get("message_hi")
                    .and_then(|m| m.as_str())
                    .unwrap_or("Galat options hain."),
            );
        }
        DusrelError::invalid_argument("Invalid options.", "Galat options hain.")
    })
}
