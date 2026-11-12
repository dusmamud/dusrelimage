//! C ABI bindings for dusrelimage — the Go / PHP FFI foundation.
//!
//! Thin wrappers over the `dusrelimage` core crate: every function
//! takes bytes in and returns bytes (or JSON) out, with an `int32_t`
//! status code (0 = OK, see `STATUS_*` / `dusrelimage.h`) and a
//! thread-local last-error JSON string (`dusrelimage_last_error`).
//!
//! Contract (mirrors `dusrelimage-wasm`, which the FFI layer follows
//! for option parsing):
//! * Options arrive as a JSON string using the Rust Options structs'
//!   serde snake_case field names. `NULL` means "all defaults".
//!   Partial JSON is deep-merged over the serialised `Default` first,
//!   so e.g. `{"quality": 70}` is a complete `CompressOptions` by the
//!   time serde sees it (plain `serde_json::from_str` would reject it:
//!   the structs' non-`Option` fields have no `#[serde(default)]`).
//! * Flat `CommonOptions` fields (`max_pixels`, `max_dimension`, …)
//!   at the top level of an options object are overlaid onto the
//!   struct's embedded `common`, exactly like the WASM layer; a
//!   nested `common: {...}` object is honoured as-is.
//! * Human-friendly shorthands are normalised before deserialising,
//!   also like the WASM layer: hex-string colours (`"#ff0000"`) in
//!   any `colour` / `background` / `fill` field, collage layout
//!   strings (`"2x2"`, `"grid"`, `"row"`, `"column"`), annotations in
//!   `{ "type": "Arrow", ... }` form, and a base64 string for a logo
//!   watermark's `image` bytes (a JSON number array works too).
//! * Invalid JSON or wrong-typed JSON values -> `INVALID_ARGUMENT`
//!   (status 9). `NULL` input / output pointers -> `INVALID_ARGUMENT`,
//!   never a crash. Empty input (`len == 0`) is passed to the core,
//!   which reports `EMPTY_INPUT` (status 10). Nothing here panics on
//!   bad input: every export is wrapped in `catch_unwind`, and a
//!   panic (which should not happen) surfaces as status 99 `UNKNOWN`.
//!
//! Memory ownership: every `*out` / `*out_json` buffer is heap
//! allocated by Rust and owned by the caller after a successful call.
//! Free it exactly once with `dusrelimage_free_buffer(ptr, len)`.
//! JSON outputs are UTF-8, `*out_len` bytes long, NOT NUL-terminated
//! (use the length). `dusrelimage_last_error` / `dusrelimage_version`
//! return NUL-terminated C strings owned by this library — do NOT
//! free them; the last-error pointer stays valid until the next FFI
//! call on the same thread.

use std::cell::RefCell;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr;
use std::slice;

use dusrelimage::{
    Annotation, BulkOp, BulkOutput, CollageOptions, Colour, CommonOptions, CompressOptions,
    ConvertOptions, CropOptions, DusrelError, ErrorCode, PixelateOptions, ResizeOptions,
    WatermarkOptions, add_watermark as core_add_watermark, annotate_image as core_annotate_image,
    bulk_process as core_bulk_process, compress_image as core_compress_image,
    compress_to_size as core_compress_to_size, convert_image as core_convert_image,
    crop_image as core_crop_image, extract_palette as core_extract_palette,
    get_image_info as core_get_image_info, make_collage as core_make_collage,
    pick_colour as core_pick_colour, pixelate_image as core_pixelate_image,
    resize_image as core_resize_image,
};
use serde::de::DeserializeOwned;
use serde_json::Value;

// ---------------------------------------------------------------------------
// Status codes (must match `dusrelimage.h`)
// ---------------------------------------------------------------------------

/// Success.
pub const STATUS_OK: i32 = 0;
/// Input bytes are not a recognisable image.
pub const STATUS_INVALID_IMAGE: i32 = 1;
/// The image format is known but not supported by this build.
pub const STATUS_UNSUPPORTED_FORMAT: i32 = 2;
/// The image exceeds `max_pixels` / `max_dimension`.
pub const STATUS_IMAGE_TOO_LARGE: i32 = 3;
/// Even the lowest quality still exceeds the requested target size.
pub const STATUS_TARGET_SIZE_UNREACHABLE: i32 = 4;
/// A crop rectangle sticks out of the image.
pub const STATUS_CROP_OUT_OF_BOUNDS: i32 = 5;
/// A pixel coordinate is outside the image.
pub const STATUS_PIXEL_OUT_OF_BOUNDS: i32 = 6;
/// A quality value is outside 1-100.
pub const STATUS_INVALID_QUALITY: i32 = 7;
/// A dimension is zero, missing, or otherwise unusable.
pub const STATUS_INVALID_DIMENSIONS: i32 = 8;
/// An argument is wrong (bad JSON, bad value, NULL pointer, ...).
pub const STATUS_INVALID_ARGUMENT: i32 = 9;
/// An input list / byte buffer is empty where input is required.
pub const STATUS_EMPTY_INPUT: i32 = 10;
/// A non-image / wrong-typed value reached a typed API.
pub const STATUS_INVALID_TYPE: i32 = 11;
/// Anything else (including a Rust panic, which should not happen).
pub const STATUS_UNKNOWN: i32 = 99;

fn status_of(code: ErrorCode) -> i32 {
    match code {
        ErrorCode::InvalidImage => STATUS_INVALID_IMAGE,
        ErrorCode::UnsupportedFormat => STATUS_UNSUPPORTED_FORMAT,
        ErrorCode::ImageTooLarge => STATUS_IMAGE_TOO_LARGE,
        ErrorCode::TargetSizeUnreachable => STATUS_TARGET_SIZE_UNREACHABLE,
        ErrorCode::CropOutOfBounds => STATUS_CROP_OUT_OF_BOUNDS,
        ErrorCode::PixelOutOfBounds => STATUS_PIXEL_OUT_OF_BOUNDS,
        ErrorCode::InvalidQuality => STATUS_INVALID_QUALITY,
        ErrorCode::InvalidDimensions => STATUS_INVALID_DIMENSIONS,
        ErrorCode::InvalidArgument => STATUS_INVALID_ARGUMENT,
        ErrorCode::EmptyInput => STATUS_EMPTY_INPUT,
        ErrorCode::InvalidType => STATUS_INVALID_TYPE,
    }
}

// ---------------------------------------------------------------------------
// Thread-local last error
// ---------------------------------------------------------------------------

thread_local! {
    static LAST_ERROR: RefCell<Option<CString>> = const { RefCell::new(None) };
}

fn set_last_error_json(json: String) {
    LAST_ERROR.with(|cell| *cell.borrow_mut() = CString::new(json).ok());
}

fn set_last_error(e: &DusrelError) {
    set_last_error_json(
        serde_json::json!({
            "code": e.code_str(),
            "message_en": e.message_en(),
            "message_hi": e.message_hi(),
        })
        .to_string(),
    );
}

fn clear_last_error() {
    LAST_ERROR.with(|cell| *cell.borrow_mut() = None);
}

/// Record `e` as the thread-local last error and return its status.
fn fail(e: DusrelError) -> i32 {
    let status = status_of(e.code);
    set_last_error(&e);
    status
}

/// Record an `UNKNOWN` error (no `ErrorCode` variant exists for it)
/// and return status 99.
fn fail_unknown() -> i32 {
    set_last_error_json(
        serde_json::json!({
            "code": "UNKNOWN",
            "message_en": "An unexpected internal error occurred.",
            "message_hi": "Ek anpekkshit internal error hua.",
        })
        .to_string(),
    );
    STATUS_UNKNOWN
}

fn invalid_argument(en: &str) -> DusrelError {
    DusrelError::invalid_argument(en, "Galat argument hai.")
}

fn invalid_options(detail: &str) -> DusrelError {
    DusrelError::invalid_argument(
        &format!("Invalid options: {detail}"),
        &format!("Galat options hain: {detail}"),
    )
}

// ---------------------------------------------------------------------------
// Base64 (bulk outputs; logo watermark bytes given as a base64 string)
// ---------------------------------------------------------------------------

const B64_CHARS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len() * 4 / 3 + 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(B64_CHARS[(n >> 18 & 63) as usize] as char);
        out.push(B64_CHARS[(n >> 12 & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            B64_CHARS[(n >> 6 & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            B64_CHARS[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let bytes = s.as_bytes();
    if bytes.len() % 4 != 0 {
        return None;
    }
    let val = |c: u8| -> Option<u32> {
        match c {
            b'A'..=b'Z' => Some((c - b'A') as u32),
            b'a'..=b'z' => Some((c - b'a' + 26) as u32),
            b'0'..=b'9' => Some((c - b'0' + 52) as u32),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    };
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    for chunk in bytes.chunks(4) {
        let pad = chunk.iter().rev().take_while(|&&c| c == b'=').count();
        if pad > 2 || chunk[..4 - pad].contains(&b'=') {
            return None;
        }
        let mut n: u32 = 0;
        for (i, &c) in chunk.iter().enumerate() {
            if c == b'=' {
                if i < 2 {
                    return None;
                }
            } else {
                n |= val(c)? << (18 - 6 * i);
            }
        }
        out.push((n >> 16) as u8);
        if pad < 2 {
            out.push((n >> 8) as u8);
        }
        if pad < 1 {
            out.push(n as u8);
        }
    }
    Some(out)
}

// ---------------------------------------------------------------------------
// JSON normalisation + options parsing (mirrors the WASM layer)
// ---------------------------------------------------------------------------

/// Deep-merge `patch` over `base` (objects merge key-by-key, anything
/// else is replaced wholesale). `null` in the patch wins over a base
/// object — that is how an explicit `null` stays `None`.
fn deep_merge(base: Value, patch: Value) -> Value {
    match (base, patch) {
        (Value::Object(mut b), Value::Object(p)) => {
            for (k, v) in p {
                let merged = match b.remove(&k) {
                    Some(old) => deep_merge(old, v),
                    None => v,
                };
                b.insert(k, merged);
            }
            Value::Object(b)
        }
        (_, patch) => patch,
    }
}

/// Turn a `"#RRGGBB"` / `"#RGB"` / `"#RRGGBBAA"` string into a full
/// `Colour` JSON object. Returns the input unchanged when it is not a
/// parseable hex colour (deserialisation then fails with a proper
/// `INVALID_ARGUMENT` instead of us guessing).
fn colour_json(value: &Value) -> Value {
    if let Value::String(s) = value
        && let Ok(colour) = Colour::from_hex(s)
        && let Ok(v) = serde_json::to_value(&colour)
    {
        return v;
    }
    value.clone()
}

/// Turn a layout shorthand into the `CollageLayout` serde shape:
/// `"2x2"` -> `{"Grid": {"rows": 2, "cols": 2}}`, `"row"` -> `"Row"`,
/// `"column"` -> `"Column"`, `"grid"` -> 2x2 grid. Anything else is
/// returned unchanged (deserialisation / core validation rejects it).
fn layout_json(value: &Value) -> Value {
    if let Value::String(s) = value {
        let lower = s.trim().to_ascii_lowercase();
        match lower.as_str() {
            "row" => return Value::String("Row".to_string()),
            "column" => return Value::String("Column".to_string()),
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

/// Turn a base64 string for a logo watermark's `image` bytes into the
/// JSON number array serde expects for `Vec<u8>`. An already-numeric
/// array, or an undecodable string, passes through unchanged (the
/// latter then fails deserialisation with `INVALID_ARGUMENT`).
fn image_bytes_json(value: &Value) -> Value {
    if let Value::String(s) = value
        && let Some(bytes) = base64_decode(s)
        && let Ok(v) = serde_json::to_value(bytes)
    {
        return v;
    }
    value.clone()
}

/// Turn one annotation from `{ "type": "Arrow", ... }` form into
/// serde's externally-tagged `{ "Arrow": { ... } }` form. An
/// already-tagged object passes through (recursion still normalises
/// its colour fields).
fn annotation_json(value: &Value) -> Value {
    let Some(obj) = value.as_object() else {
        return value.clone();
    };
    if let Some(Value::String(kind)) = obj.get("type") {
        let mut fields = obj.clone();
        fields.remove("type");
        let inner = normalise(Value::Object(fields));
        let mut out = serde_json::Map::new();
        out.insert(kind.clone(), inner);
        return Value::Object(out);
    }
    normalise(value.clone())
}

/// Normalise a JSON value from a C caller: hex-string colours, layout
/// shorthands, base64 logo bytes and `{ "type": ... }` annotations,
/// recursively.
fn normalise(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut out = serde_json::Map::with_capacity(map.len());
            for (k, v) in map {
                let v = match k.as_str() {
                    "colour" | "background" | "fill" => colour_json(&v),
                    "layout" => layout_json(&v),
                    "image" => image_bytes_json(&v),
                    "annotations" => match v {
                        Value::Array(items) => {
                            Value::Array(items.iter().map(annotation_json).collect())
                        }
                        other => normalise(other),
                    },
                    _ => normalise(v),
                };
                out.insert(k, v);
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.into_iter().map(normalise).collect()),
        other => other,
    }
}

/// Parse an already-deserialised JSON value into a Rust options
/// struct. `null` / `{}` mean "all defaults"; a partial object is
/// deep-merged over the serialised `Default` first.
fn parse_options_value<T: Default + DeserializeOwned + serde::Serialize>(
    value: &Value,
) -> Result<T, DusrelError> {
    if value.is_null() {
        return Ok(T::default());
    }
    let defaults = serde_json::to_value(T::default())
        .map_err(|_| invalid_options("Could not serialise default options."))?;
    let merged = deep_merge(defaults, normalise(value.clone()));
    serde_json::from_value(merged).map_err(|e| invalid_options(&e.to_string()))
}

/// Parse a JSON C string into a `serde_json::Value` (`NULL` = `{}`).
fn parse_json_cstr(ptr: *const c_char) -> Result<Value, DusrelError> {
    if ptr.is_null() {
        return Ok(serde_json::json!({}));
    }
    let text = unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .map_err(|_| invalid_options("JSON is not valid UTF-8."))?;
    serde_json::from_str(text).map_err(|e| invalid_options(&e.to_string()))
}

/// Parse a C string into a Rust `String` (`NULL` -> `INVALID_ARGUMENT`).
fn parse_str_cstr(ptr: *const c_char, what: &str) -> Result<String, DusrelError> {
    if ptr.is_null() {
        return Err(invalid_argument(&format!("{what} pointer is NULL.")));
    }
    unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .map(|s| s.to_string())
        .map_err(|_| invalid_argument(&format!("{what} is not valid UTF-8.")))
}

/// Parse an options struct, then — only if the JSON object carries
/// any `CommonOptions` field FLAT at the top level (`max_pixels`, …)
/// — overlay those flat fields onto the struct's embedded `common`.
/// A nested `common: {...}` object is honoured by `parse_options_*`
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

fn with_common_value<T: Default + DeserializeOwned + serde::Serialize + HasCommon>(
    value: &Value,
) -> Result<T, DusrelError> {
    let mut parsed = parse_options_value::<T>(value)?;
    let Some(obj) = value.as_object() else {
        return Ok(parsed);
    };
    let flat: serde_json::Map<String, Value> = obj
        .iter()
        .filter(|(k, _)| COMMON_KEYS.contains(&k.as_str()))
        .map(|(k, v)| (k.clone(), normalise(v.clone())))
        .collect();
    if flat.is_empty() {
        return Ok(parsed);
    }
    let base = serde_json::to_value(parsed.common())
        .map_err(|_| invalid_options("Could not serialise common options."))?;
    let merged = deep_merge(base, Value::Object(flat));
    let common: CommonOptions =
        serde_json::from_value(merged).map_err(|e| invalid_options(&e.to_string()))?;
    parsed.set_common(common);
    Ok(parsed)
}

fn with_common_cstr<T: Default + DeserializeOwned + serde::Serialize + HasCommon>(
    ptr: *const c_char,
) -> Result<T, DusrelError> {
    let value = parse_json_cstr(ptr)?;
    with_common_value(&value)
}

/// Parse a bare `CommonOptions` C string: NULL / `{}` / `null` mean
/// all defaults; a nested `common` object and/or flat common fields
/// (`max_pixels`, `max_dimension`, …) are deep-merged over the
/// defaults — the same convention as `with_common_cstr` for the
/// other ops. Anything that is not a JSON object is
/// `INVALID_ARGUMENT`.
fn parse_common_cstr(ptr: *const c_char) -> Result<CommonOptions, DusrelError> {
    let value = parse_json_cstr(ptr)?;
    if value.is_null() {
        return Ok(CommonOptions::default());
    }
    let Some(obj) = value.as_object() else {
        return Err(invalid_options("Common options must be a JSON object."));
    };
    let base = serde_json::to_value(CommonOptions::default())
        .map_err(|_| invalid_options("Could not serialise default options."))?;
    let mut merged = base;
    if let Some(common) = obj.get("common") {
        merged = deep_merge(merged, normalise(common.clone()));
    }
    let flat: serde_json::Map<String, Value> = obj
        .iter()
        .filter(|(k, _)| COMMON_KEYS.contains(&k.as_str()))
        .map(|(k, v)| (k.clone(), normalise(v.clone())))
        .collect();
    if !flat.is_empty() {
        merged = deep_merge(merged, Value::Object(flat));
    }
    serde_json::from_value(merged).map_err(|e| invalid_options(&e.to_string()))
}

/// Parse annotations from an already-deserialised JSON value
/// (a JSON array, in serde-tagged or `{ "type": ... }` form).
fn parse_annotations_value(value: &Value) -> Result<Vec<Annotation>, DusrelError> {
    if value.is_null() {
        return Ok(Vec::new());
    }
    let normalised = match value {
        Value::Array(items) => Value::Array(items.iter().map(annotation_json).collect()),
        other => other.clone(),
    };
    serde_json::from_value(normalised)
        .map_err(|e| invalid_argument(&format!("Invalid annotations: {e}")))
}

fn parse_annotations_cstr(ptr: *const c_char) -> Result<Vec<Annotation>, DusrelError> {
    if ptr.is_null() {
        return Err(invalid_argument("Annotations JSON pointer is NULL."));
    }
    let text = unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .map_err(|_| invalid_argument("Annotations JSON is not valid UTF-8."))?;
    let value: Value = serde_json::from_str(text)
        .map_err(|e| invalid_argument(&format!("Invalid annotations JSON: {e}")))?;
    parse_annotations_value(&value)
}

// ---------------------------------------------------------------------------
// Pointer helpers + panic guard
// ---------------------------------------------------------------------------

/// Borrow a byte input: `len == 0` yields an empty slice (even for a
/// `NULL` pointer — the core then reports `EMPTY_INPUT`); a `NULL`
/// pointer with `len > 0` is `INVALID_ARGUMENT`.
fn input_slice<'a>(ptr: *const u8, len: usize) -> Result<&'a [u8], DusrelError> {
    if len == 0 {
        return Ok(&[]);
    }
    if ptr.is_null() {
        return Err(invalid_argument(
            "Input pointer is NULL but input length is not zero.",
        ));
    }
    Ok(unsafe { slice::from_raw_parts(ptr, len) })
}

/// Read the `count` inputs of a multi-input call. `count == 0` yields
/// an empty list (even for `NULL` pointers); a `NULL` array pointer
/// with `count > 0`, or a `NULL` entry with a non-zero length, is
/// `INVALID_ARGUMENT`.
fn read_inputs(
    inputs: *const *const u8,
    input_lens: *const usize,
    count: usize,
) -> Result<Vec<Vec<u8>>, DusrelError> {
    if count == 0 {
        return Ok(Vec::new());
    }
    if inputs.is_null() || input_lens.is_null() {
        return Err(invalid_argument(
            "Inputs pointer is NULL but input count is not zero.",
        ));
    }
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let ptr = unsafe { *inputs.add(i) };
        let len = unsafe { *input_lens.add(i) };
        out.push(input_slice(ptr, len)?.to_vec());
    }
    Ok(out)
}

/// Validate an output-pointer pair before doing any work, so a caller
/// bug is reported as `INVALID_ARGUMENT` even when the image itself
/// would also have failed.
fn check_out(out: *mut *mut u8, out_len: *mut usize) -> Result<(), DusrelError> {
    if out.is_null() || out_len.is_null() {
        return Err(invalid_argument("Output pointer is NULL."));
    }
    Ok(())
}

/// Hand ownership of `bytes` to the caller: `*out` points at a
/// heap buffer of `*out_len` bytes, freed with
/// `dusrelimage_free_buffer`. Clears the last error on success.
fn write_output(out: *mut *mut u8, out_len: *mut usize, bytes: Vec<u8>) -> i32 {
    let len = bytes.len();
    let ptr = Box::leak(bytes.into_boxed_slice()).as_mut_ptr();
    unsafe {
        *out = ptr;
        *out_len = len;
    }
    clear_last_error();
    STATUS_OK
}

/// Same as `write_output`, for a JSON string (UTF-8, `len` bytes,
/// NOT NUL-terminated).
fn write_json_output(out: *mut *mut u8, out_len: *mut usize, json: String) -> i32 {
    write_output(out, out_len, json.into_bytes())
}

/// Run `body`, converting a Rust panic (which should not happen — the
/// core never panics on bad input) into status 99 `UNKNOWN` instead
/// of unwinding across the FFI boundary.
fn guard(body: impl FnOnce() -> i32) -> i32 {
    match catch_unwind(AssertUnwindSafe(body)) {
        Ok(code) => code,
        Err(_) => fail_unknown(),
    }
}

// ---------------------------------------------------------------------------
// Bulk helpers
// ---------------------------------------------------------------------------

/// Flatten palette entries for JSON: `{ hex, r, g, b, a, hsl, share }`
/// per entry — the WASM bulk shape (the standalone
/// `dusrelimage_extract_palette` instead returns the nested
/// `PaletteColour` shape, `{"colour": {...}, "share": ...}`).
fn palette_flat_json(entries: &[dusrelimage::PaletteColour]) -> Vec<Value> {
    entries
        .iter()
        .map(|entry| {
            let mut v = serde_json::to_value(&entry.colour).unwrap_or(serde_json::json!({}));
            if let Value::Object(ref mut map) = v {
                map.insert("share".to_string(), serde_json::json!(entry.share));
            }
            v
        })
        .collect()
}

fn bulk_item_json(index: usize, result: &dusrelimage::Result<BulkOutput>) -> Value {
    match result {
        Ok(BulkOutput::Image(out)) => serde_json::json!({
            "index": index,
            "ok": true,
            "output": base64_encode(out),
        }),
        Ok(BulkOutput::Colour(colour)) => serde_json::json!({
            "index": index,
            "ok": true,
            "colour": serde_json::to_value(colour).unwrap_or(serde_json::json!({})),
        }),
        Ok(BulkOutput::Palette(entries)) => serde_json::json!({
            "index": index,
            "ok": true,
            "palette": palette_flat_json(entries),
        }),
        Err(e) => serde_json::json!({
            "index": index,
            "ok": false,
            "error": {
                "code": e.code_str(),
                "message_en": e.message_en(),
                "message_hi": e.message_hi(),
            },
        }),
    }
}

/// Map a bulk `op` name plus its options JSON value onto a core
/// `BulkOp`. `compress_to_size` has no `BulkOp` variant and is
/// dispatched by the caller instead (see `dusrelimage_bulk_process`).
fn bulk_op_from(op: &str, options: &Value) -> Result<BulkOp, DusrelError> {
    match op {
        "compress" | "compress_image" => Ok(BulkOp::Compress(with_common_value(options)?)),
        "resize" | "resize_image" => Ok(BulkOp::Resize(with_common_value(options)?)),
        "crop" | "crop_image" => Ok(BulkOp::Crop(with_common_value(options)?)),
        "convert" | "convert_image" => Ok(BulkOp::Convert(with_common_value(options)?)),
        "watermark" | "add_watermark" => Ok(BulkOp::Watermark(with_common_value(options)?)),
        "annotate" | "annotate_image" => {
            let anns = options.get("annotations").cloned().unwrap_or(Value::Null);
            Ok(BulkOp::Annotate(parse_annotations_value(&anns)?))
        }
        "pick_colour" | "pick-colour" | "pickColour" => Ok(BulkOp::PickColour {
            x: options.get("x").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
            y: options.get("y").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
        }),
        "palette" | "extract_palette" => Ok(BulkOp::Palette {
            count: options.get("count").and_then(|v| v.as_u64()).unwrap_or(5) as u8,
        }),
        "collage" | "make_collage" => Ok(BulkOp::Collage(with_common_value(options)?)),
        "pixelate" | "pixelate_image" => Ok(BulkOp::Pixelate(with_common_value(options)?)),
        other => Err(invalid_argument(&format!(
            "Unknown bulk op \"{other}\". Use compress, compress_to_size, resize, crop, convert, watermark, annotate, pick_colour, palette or pixelate."
        ))),
    }
}

// ---------------------------------------------------------------------------
// Exports — bytes in / bytes out (tools 1-6, 9, 10)
// ---------------------------------------------------------------------------

/// Tool 1 — re-encode at a fixed quality (and `format`, or the input
/// format when unset). `options_json` is a `CompressOptions` JSON
/// object, or `NULL` for all defaults.
///
/// # Safety
///
/// Pointer arguments must be valid for their use: `input` must
/// point at `input_len` readable bytes (or be NULL when
/// `input_len` is 0), `inputs` / `input_lens` must point at
/// `count` readable entries, string arguments must be NULL or
/// point at valid NUL-terminated C strings, and `out` / `out_len`
/// must be NULL or point at valid writable locations. Passing a
/// dangling non-NULL pointer is undefined behaviour; NULL
/// pointers are checked and reported as `INVALID_ARGUMENT`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dusrelimage_compress(
    input: *const u8,
    input_len: usize,
    options_json: *const c_char,
    out: *mut *mut u8,
    out_len: *mut usize,
) -> i32 {
    guard(|| {
        if let Err(e) = check_out(out, out_len) {
            return fail(e);
        }
        let data = match input_slice(input, input_len) {
            Ok(d) => d,
            Err(e) => return fail(e),
        };
        let opts = match with_common_cstr::<CompressOptions>(options_json) {
            Ok(o) => o,
            Err(e) => return fail(e),
        };
        match core_compress_image(data, &opts) {
            Ok(bytes) => write_output(out, out_len, bytes),
            Err(e) => fail(e),
        }
    })
}

/// Tool 1 — re-encode so the result is at most `target_bytes` bytes
/// (iterative quality search; `TARGET_SIZE_UNREACHABLE` when even the
/// lowest quality exceeds the target).
///
/// # Safety
///
/// Pointer arguments must be valid for their use: `input` must
/// point at `input_len` readable bytes (or be NULL when
/// `input_len` is 0), `inputs` / `input_lens` must point at
/// `count` readable entries, string arguments must be NULL or
/// point at valid NUL-terminated C strings, and `out` / `out_len`
/// must be NULL or point at valid writable locations. Passing a
/// dangling non-NULL pointer is undefined behaviour; NULL
/// pointers are checked and reported as `INVALID_ARGUMENT`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dusrelimage_compress_to_size(
    input: *const u8,
    input_len: usize,
    target_bytes: u64,
    options_json: *const c_char,
    out: *mut *mut u8,
    out_len: *mut usize,
) -> i32 {
    guard(|| {
        if let Err(e) = check_out(out, out_len) {
            return fail(e);
        }
        let data = match input_slice(input, input_len) {
            Ok(d) => d,
            Err(e) => return fail(e),
        };
        let opts = match with_common_cstr::<CompressOptions>(options_json) {
            Ok(o) => o,
            Err(e) => return fail(e),
        };
        match core_compress_to_size(data, target_bytes, &opts) {
            Ok(bytes) => write_output(out, out_len, bytes),
            Err(e) => fail(e),
        }
    })
}

/// Tool 2 — resize (width and/or height, percent, fit, filter).
/// `options_json` is a `ResizeOptions` JSON object, or `NULL`.
///
/// # Safety
///
/// Pointer arguments must be valid for their use: `input` must
/// point at `input_len` readable bytes (or be NULL when
/// `input_len` is 0), `inputs` / `input_lens` must point at
/// `count` readable entries, string arguments must be NULL or
/// point at valid NUL-terminated C strings, and `out` / `out_len`
/// must be NULL or point at valid writable locations. Passing a
/// dangling non-NULL pointer is undefined behaviour; NULL
/// pointers are checked and reported as `INVALID_ARGUMENT`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dusrelimage_resize(
    input: *const u8,
    input_len: usize,
    options_json: *const c_char,
    out: *mut *mut u8,
    out_len: *mut usize,
) -> i32 {
    guard(|| {
        if let Err(e) = check_out(out, out_len) {
            return fail(e);
        }
        let data = match input_slice(input, input_len) {
            Ok(d) => d,
            Err(e) => return fail(e),
        };
        let opts = match with_common_cstr::<ResizeOptions>(options_json) {
            Ok(o) => o,
            Err(e) => return fail(e),
        };
        match core_resize_image(data, &opts) {
            Ok(bytes) => write_output(out, out_len, bytes),
            Err(e) => fail(e),
        }
    })
}

/// Tool 3 — crop to a rectangle (or an anchor preset).
/// `options_json` is a `CropOptions` JSON object, or `NULL`.
///
/// # Safety
///
/// Pointer arguments must be valid for their use: `input` must
/// point at `input_len` readable bytes (or be NULL when
/// `input_len` is 0), `inputs` / `input_lens` must point at
/// `count` readable entries, string arguments must be NULL or
/// point at valid NUL-terminated C strings, and `out` / `out_len`
/// must be NULL or point at valid writable locations. Passing a
/// dangling non-NULL pointer is undefined behaviour; NULL
/// pointers are checked and reported as `INVALID_ARGUMENT`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dusrelimage_crop(
    input: *const u8,
    input_len: usize,
    options_json: *const c_char,
    out: *mut *mut u8,
    out_len: *mut usize,
) -> i32 {
    guard(|| {
        if let Err(e) = check_out(out, out_len) {
            return fail(e);
        }
        let data = match input_slice(input, input_len) {
            Ok(d) => d,
            Err(e) => return fail(e),
        };
        let opts = match with_common_cstr::<CropOptions>(options_json) {
            Ok(o) => o,
            Err(e) => return fail(e),
        };
        match core_crop_image(data, &opts) {
            Ok(bytes) => write_output(out, out_len, bytes),
            Err(e) => fail(e),
        }
    })
}

/// Tool 4 — convert to another format. `options_json` is a
/// `ConvertOptions` JSON object, or `NULL` (defaults to JPEG).
///
/// # Safety
///
/// Pointer arguments must be valid for their use: `input` must
/// point at `input_len` readable bytes (or be NULL when
/// `input_len` is 0), `inputs` / `input_lens` must point at
/// `count` readable entries, string arguments must be NULL or
/// point at valid NUL-terminated C strings, and `out` / `out_len`
/// must be NULL or point at valid writable locations. Passing a
/// dangling non-NULL pointer is undefined behaviour; NULL
/// pointers are checked and reported as `INVALID_ARGUMENT`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dusrelimage_convert(
    input: *const u8,
    input_len: usize,
    options_json: *const c_char,
    out: *mut *mut u8,
    out_len: *mut usize,
) -> i32 {
    guard(|| {
        if let Err(e) = check_out(out, out_len) {
            return fail(e);
        }
        let data = match input_slice(input, input_len) {
            Ok(d) => d,
            Err(e) => return fail(e),
        };
        let opts = match with_common_cstr::<ConvertOptions>(options_json) {
            Ok(o) => o,
            Err(e) => return fail(e),
        };
        match core_convert_image(data, &opts) {
            Ok(bytes) => write_output(out, out_len, bytes),
            Err(e) => fail(e),
        }
    })
}

/// Tool 5 — stamp a text or logo watermark. `options_json` is a
/// `WatermarkOptions` JSON object, or `NULL`.
///
/// # Safety
///
/// Pointer arguments must be valid for their use: `input` must
/// point at `input_len` readable bytes (or be NULL when
/// `input_len` is 0), `inputs` / `input_lens` must point at
/// `count` readable entries, string arguments must be NULL or
/// point at valid NUL-terminated C strings, and `out` / `out_len`
/// must be NULL or point at valid writable locations. Passing a
/// dangling non-NULL pointer is undefined behaviour; NULL
/// pointers are checked and reported as `INVALID_ARGUMENT`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dusrelimage_add_watermark(
    input: *const u8,
    input_len: usize,
    options_json: *const c_char,
    out: *mut *mut u8,
    out_len: *mut usize,
) -> i32 {
    guard(|| {
        if let Err(e) = check_out(out, out_len) {
            return fail(e);
        }
        let data = match input_slice(input, input_len) {
            Ok(d) => d,
            Err(e) => return fail(e),
        };
        let opts = match with_common_cstr::<WatermarkOptions>(options_json) {
            Ok(o) => o,
            Err(e) => return fail(e),
        };
        match core_add_watermark(data, &opts) {
            Ok(bytes) => write_output(out, out_len, bytes),
            Err(e) => fail(e),
        }
    })
}

/// Tool 6 — draw annotations. `annotations_json` is a JSON array of
/// annotations in serde externally-tagged form
/// (`[{"Rect": {"x": 10, ...}}]`) or `{ "type": "Rect", ... }` form.
///
/// # Safety
///
/// Pointer arguments must be valid for their use: `input` must
/// point at `input_len` readable bytes (or be NULL when
/// `input_len` is 0), `inputs` / `input_lens` must point at
/// `count` readable entries, string arguments must be NULL or
/// point at valid NUL-terminated C strings, and `out` / `out_len`
/// must be NULL or point at valid writable locations. Passing a
/// dangling non-NULL pointer is undefined behaviour; NULL
/// pointers are checked and reported as `INVALID_ARGUMENT`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dusrelimage_annotate(
    input: *const u8,
    input_len: usize,
    annotations_json: *const c_char,
    out: *mut *mut u8,
    out_len: *mut usize,
) -> i32 {
    guard(|| {
        if let Err(e) = check_out(out, out_len) {
            return fail(e);
        }
        let data = match input_slice(input, input_len) {
            Ok(d) => d,
            Err(e) => return fail(e),
        };
        let annotations = match parse_annotations_cstr(annotations_json) {
            Ok(a) => a,
            Err(e) => return fail(e),
        };
        match core_annotate_image(data, &annotations) {
            Ok(bytes) => write_output(out, out_len, bytes),
            Err(e) => fail(e),
        }
    })
}

/// Tool 9 — collage from several images. `options_json` is a
/// `CollageOptions` JSON object, or `NULL`.
///
/// # Safety
///
/// Pointer arguments must be valid for their use: `input` must
/// point at `input_len` readable bytes (or be NULL when
/// `input_len` is 0), `inputs` / `input_lens` must point at
/// `count` readable entries, string arguments must be NULL or
/// point at valid NUL-terminated C strings, and `out` / `out_len`
/// must be NULL or point at valid writable locations. Passing a
/// dangling non-NULL pointer is undefined behaviour; NULL
/// pointers are checked and reported as `INVALID_ARGUMENT`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dusrelimage_make_collage(
    inputs: *const *const u8,
    input_lens: *const usize,
    count: usize,
    options_json: *const c_char,
    out: *mut *mut u8,
    out_len: *mut usize,
) -> i32 {
    guard(|| {
        if let Err(e) = check_out(out, out_len) {
            return fail(e);
        }
        let bufs = match read_inputs(inputs, input_lens, count) {
            Ok(b) => b,
            Err(e) => return fail(e),
        };
        let opts = match with_common_cstr::<CollageOptions>(options_json) {
            Ok(o) => o,
            Err(e) => return fail(e),
        };
        match core_make_collage(&bufs, &opts) {
            Ok(bytes) => write_output(out, out_len, bytes),
            Err(e) => fail(e),
        }
    })
}

/// Tool 10 — pixelate a rectangle (whole image when width/height are
/// 0). `options_json` is a `PixelateOptions` JSON object, or `NULL`.
///
/// # Safety
///
/// Pointer arguments must be valid for their use: `input` must
/// point at `input_len` readable bytes (or be NULL when
/// `input_len` is 0), `inputs` / `input_lens` must point at
/// `count` readable entries, string arguments must be NULL or
/// point at valid NUL-terminated C strings, and `out` / `out_len`
/// must be NULL or point at valid writable locations. Passing a
/// dangling non-NULL pointer is undefined behaviour; NULL
/// pointers are checked and reported as `INVALID_ARGUMENT`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dusrelimage_pixelate(
    input: *const u8,
    input_len: usize,
    options_json: *const c_char,
    out: *mut *mut u8,
    out_len: *mut usize,
) -> i32 {
    guard(|| {
        if let Err(e) = check_out(out, out_len) {
            return fail(e);
        }
        let data = match input_slice(input, input_len) {
            Ok(d) => d,
            Err(e) => return fail(e),
        };
        let opts = match with_common_cstr::<PixelateOptions>(options_json) {
            Ok(o) => o,
            Err(e) => return fail(e),
        };
        match core_pixelate_image(data, &opts) {
            Ok(bytes) => write_output(out, out_len, bytes),
            Err(e) => fail(e),
        }
    })
}

// ---------------------------------------------------------------------------
// Exports — JSON out (tools 7, 8, info) and bulk
// ---------------------------------------------------------------------------

/// Tool 7 — colour of one pixel, as JSON:
/// `{"hex": "#RRGGBB", "r": …, "g": …, "b": …, "a": …, "hsl": [h, s, l]}`.
///
/// # Safety
///
/// Pointer arguments must be valid for their use: `input` must
/// point at `input_len` readable bytes (or be NULL when
/// `input_len` is 0), `inputs` / `input_lens` must point at
/// `count` readable entries, string arguments must be NULL or
/// point at valid NUL-terminated C strings, and `out` / `out_len`
/// must be NULL or point at valid writable locations. Passing a
/// dangling non-NULL pointer is undefined behaviour; NULL
/// pointers are checked and reported as `INVALID_ARGUMENT`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dusrelimage_pick_colour(
    input: *const u8,
    input_len: usize,
    x: u32,
    y: u32,
    out_json: *mut *mut u8,
    out_len: *mut usize,
) -> i32 {
    guard(|| {
        if let Err(e) = check_out(out_json, out_len) {
            return fail(e);
        }
        let data = match input_slice(input, input_len) {
            Ok(d) => d,
            Err(e) => return fail(e),
        };
        match core_pick_colour(data, x, y) {
            Ok(colour) => match serde_json::to_string(&colour) {
                Ok(json) => write_json_output(out_json, out_len, json),
                Err(_) => fail(invalid_argument("Could not serialise colour.")),
            },
            Err(e) => fail(e),
        }
    })
}

/// Tool 8 — dominant colours as a JSON array of `PaletteColour`:
/// `[{"colour": {"hex": …, "r": …, …}, "share": 0.5}, …]`, most
/// frequent first. `count` is 1-16.
///
/// # Safety
///
/// Pointer arguments must be valid for their use: `input` must
/// point at `input_len` readable bytes (or be NULL when
/// `input_len` is 0), `inputs` / `input_lens` must point at
/// `count` readable entries, string arguments must be NULL or
/// point at valid NUL-terminated C strings, and `out` / `out_len`
/// must be NULL or point at valid writable locations. Passing a
/// dangling non-NULL pointer is undefined behaviour; NULL
/// pointers are checked and reported as `INVALID_ARGUMENT`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dusrelimage_extract_palette(
    input: *const u8,
    input_len: usize,
    count: u8,
    out_json: *mut *mut u8,
    out_len: *mut usize,
) -> i32 {
    guard(|| {
        if let Err(e) = check_out(out_json, out_len) {
            return fail(e);
        }
        let data = match input_slice(input, input_len) {
            Ok(d) => d,
            Err(e) => return fail(e),
        };
        match core_extract_palette(data, count) {
            Ok(palette) => match serde_json::to_string(&palette) {
                Ok(json) => write_json_output(out_json, out_len, json),
                Err(_) => fail(invalid_argument("Could not serialise palette.")),
            },
            Err(e) => fail(e),
        }
    })
}

/// Image summary as JSON:
/// `{"format": "png", "width": …, "height": …, "has_alpha": …,
/// "colour_type": "rgb8", "file_size": …}`.
/// `options_json` is a `CommonOptions` JSON object (nested `common`
/// object and/or flat common fields, e.g. `{"max_dimension": 10}`),
/// or NULL for all defaults — the same convention as the other ops.
/// (Before the 0.1.0 bindings pass this took no options, which made
/// the `info-too-large-*` shared vectors unpassable from FFI-based
/// bindings; the options parameter was added for parity with the
/// WASM binding, whose `get_image_info` takes options.)
///
/// # Safety
///
/// Pointer arguments must be valid for their use: `input` must
/// point at `input_len` readable bytes (or be NULL when
/// `input_len` is 0), `inputs` / `input_lens` must point at
/// `count` readable entries, string arguments must be NULL or
/// point at valid NUL-terminated C strings, and `out` / `out_len`
/// must be NULL or point at valid writable locations. Passing a
/// dangling non-NULL pointer is undefined behaviour; NULL
/// pointers are checked and reported as `INVALID_ARGUMENT`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dusrelimage_get_image_info(
    input: *const u8,
    input_len: usize,
    options_json: *const c_char,
    out_json: *mut *mut u8,
    out_len: *mut usize,
) -> i32 {
    guard(|| {
        if let Err(e) = check_out(out_json, out_len) {
            return fail(e);
        }
        let data = match input_slice(input, input_len) {
            Ok(d) => d,
            Err(e) => return fail(e),
        };
        let common = match parse_common_cstr(options_json) {
            Ok(c) => c,
            Err(e) => return fail(e),
        };
        match core_get_image_info(data, &common) {
            Ok(info) => match serde_json::to_string(&info) {
                Ok(json) => write_json_output(out_json, out_len, json),
                Err(_) => fail(invalid_argument("Could not serialise image info.")),
            },
            Err(e) => fail(e),
        }
    })
}

/// Run one operation over many inputs; one result per input, same
/// order. `op` is a short name (`"compress"`, `"resize"`, `"crop"`,
/// `"convert"`, `"watermark"`, `"annotate"`, `"pick_colour"`,
/// `"palette"`, `"pixelate"`, `"compress_to_size"`, `"collage"`) or
/// the full snake_case export name (`"compress_image"`, …).
/// `options_json` carries the op's options; `compress_to_size` also
/// reads `target_bytes` from it, `annotate` reads `annotations`,
/// `pick_colour` reads `x`/`y`, `palette` reads `count`. A zero
/// `count` returns `"[]"` with status 0. `"collage"` is accepted but
/// reported per item as `INVALID_ARGUMENT` (a collage combines all
/// inputs — call `dusrelimage_make_collage` once instead).
///
/// Returns a JSON array of per-item results, mirroring the WASM bulk
/// shape: `{"index": 0, "ok": true, "output": "<base64>"}`
/// (image ops), `{"index": 0, "ok": true, "colour": {...}}`
/// (`pick_colour`), `{"index": 0, "ok": true, "palette": [{"hex": …,
/// "share": …}, …]}` (flattened palette entries), or
/// `{"index": 0, "ok": false, "error": {"code": …, "message_en": …,
/// "message_hi": …}}`. One bad input fails only its own item.
///
/// # Safety
///
/// Pointer arguments must be valid for their use: `input` must
/// point at `input_len` readable bytes (or be NULL when
/// `input_len` is 0), `inputs` / `input_lens` must point at
/// `count` readable entries, string arguments must be NULL or
/// point at valid NUL-terminated C strings, and `out` / `out_len`
/// must be NULL or point at valid writable locations. Passing a
/// dangling non-NULL pointer is undefined behaviour; NULL
/// pointers are checked and reported as `INVALID_ARGUMENT`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dusrelimage_bulk_process(
    inputs: *const *const u8,
    input_lens: *const usize,
    count: usize,
    op: *const c_char,
    options_json: *const c_char,
    out_json: *mut *mut u8,
    out_len: *mut usize,
) -> i32 {
    guard(|| {
        if let Err(e) = check_out(out_json, out_len) {
            return fail(e);
        }
        let bufs = match read_inputs(inputs, input_lens, count) {
            Ok(b) => b,
            Err(e) => return fail(e),
        };
        if bufs.is_empty() {
            return write_json_output(out_json, out_len, "[]".to_string());
        }
        let op_name = match parse_str_cstr(op, "Bulk op") {
            Ok(s) => s,
            Err(e) => return fail(e),
        };
        let options = match parse_json_cstr(options_json) {
            Ok(v) => v,
            Err(e) => return fail(e),
        };

        // `compress_to_size` has no core `BulkOp` variant: run it here.
        if op_name == "compress_to_size" {
            let target_bytes = options
                .get("target_bytes")
                .or_else(|| options.get("targetBytes"))
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            if !target_bytes.is_finite() || target_bytes <= 0.0 {
                return fail(invalid_argument(
                    "bulk compress_to_size needs target_bytes in options.",
                ));
            }
            let opts = match with_common_value::<CompressOptions>(&options) {
                Ok(o) => o,
                Err(e) => return fail(e),
            };
            let results: Vec<Value> = bufs
                .iter()
                .enumerate()
                .map(|(index, input)| {
                    match core_compress_to_size(input, target_bytes as u64, &opts) {
                        Ok(out) => serde_json::json!({
                            "index": index,
                            "ok": true,
                            "output": base64_encode(&out),
                        }),
                        Err(e) => serde_json::json!({
                            "index": index,
                            "ok": false,
                            "error": {
                                "code": e.code_str(),
                                "message_en": e.message_en(),
                                "message_hi": e.message_hi(),
                            },
                        }),
                    }
                })
                .collect();
            return match serde_json::to_string(&results) {
                Ok(json) => write_json_output(out_json, out_len, json),
                Err(_) => fail(invalid_argument("Could not serialise bulk results.")),
            };
        }

        let bulk_op = match bulk_op_from(&op_name, &options) {
            Ok(o) => o,
            Err(e) => return fail(e),
        };
        let results: Vec<Value> = core_bulk_process(&bufs, &bulk_op)
            .iter()
            .map(|item| bulk_item_json(item.index, &item.result))
            .collect();
        match serde_json::to_string(&results) {
            Ok(json) => write_json_output(out_json, out_len, json),
            Err(_) => fail(invalid_argument("Could not serialise bulk results.")),
        }
    })
}

// ---------------------------------------------------------------------------
// Exports — diagnostics
// ---------------------------------------------------------------------------

/// The last error on this thread, as JSON:
/// `{"code": "…", "message_en": "…", "message_hi": "…"}`.
/// Returns `NULL` when the last call on this thread succeeded (or no
/// FFI call has failed yet). Do NOT free the pointer; it stays valid
/// until the next FFI call on the same thread.
#[unsafe(no_mangle)]
pub extern "C" fn dusrelimage_last_error() -> *const c_char {
    LAST_ERROR.with(|cell| match cell.borrow().as_ref() {
        Some(s) => s.as_ptr(),
        None => ptr::null(),
    })
}

/// Free a buffer returned by any `*out` / `*out_json` parameter:
/// exactly the pointer and length that call returned, exactly once.
/// A `NULL` pointer or a zero length is a no-op.
///
/// # Safety
///
/// `ptr` / `len` must be exactly a buffer (pointer and length)
/// returned by a previous successful call of this library, freed
/// exactly once. Freeing any other pointer, or the same buffer
/// twice, is undefined behaviour. NULL / zero length is a no-op.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dusrelimage_free_buffer(ptr: *mut u8, len: usize) {
    if ptr.is_null() || len == 0 {
        return;
    }
    drop(unsafe { Box::from_raw(ptr::slice_from_raw_parts_mut(ptr, len)) });
}

/// Library version, e.g. `"0.1.0"` (lockstep with the core crate's
/// `CARGO_PKG_VERSION`). Do NOT free the pointer.
#[unsafe(no_mangle)]
pub extern "C" fn dusrelimage_version() -> *const c_char {
    static VERSION_BYTES: &[u8] = concat!(env!("CARGO_PKG_VERSION"), "\0").as_bytes();
    VERSION_BYTES.as_ptr().cast::<c_char>()
}
