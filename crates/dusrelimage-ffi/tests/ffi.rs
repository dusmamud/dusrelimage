//! Integration tests for the C ABI: they call the exported
//! `extern "C"` functions directly, exactly as a Go / PHP caller
//! would (same signatures, same status codes, same ownership rules).
//!
//! The crate sets `crate-type = ["cdylib", "staticlib", "rlib"]`,
//! so the symbols resolve from the rlib here; the real cdylib
//! (`libdusrelimage_ffi.so`) is exercised separately by the C smoke
//! test in `tests/c_smoke/`.
//!
//! Fixtures come from the core crate's `fixtures` module (dev-dep).

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::ptr;

use dusrelimage::fixtures::{MockKind, make_test_image, solid_png};
use dusrelimage::types::Colour;
use dusrelimage::{CommonOptions, get_image_info};

// Referencing the crate (without calling into it) makes cargo build
// its rlib for this test target and link it, so the `extern "C"`
// declarations below resolve on a bare `cargo test` run, which does
// not build the cdylib/staticlib by itself. The calls themselves
// still go through the C ABI, exactly as a Go / PHP caller would;
// the real cdylib is exercised by the C smoke test in
// `tests/c_smoke/`.
use dusrelimage_ffi as _;

// No `#[link]` attribute here: the crate is already linked as an
// rlib through the `use dusrelimage_ffi as _;` above, and a `#[link]`
// would make the linker also demand a `libdusrelimage_ffi.so`/`.a`
// file, which `cargo test` does not build (only `cargo build` does).
unsafe extern "C" {
    fn dusrelimage_compress(
        input: *const u8,
        input_len: usize,
        options_json: *const c_char,
        out: *mut *mut u8,
        out_len: *mut usize,
    ) -> i32;
    fn dusrelimage_compress_to_size(
        input: *const u8,
        input_len: usize,
        target_bytes: u64,
        options_json: *const c_char,
        out: *mut *mut u8,
        out_len: *mut usize,
    ) -> i32;
    fn dusrelimage_resize(
        input: *const u8,
        input_len: usize,
        options_json: *const c_char,
        out: *mut *mut u8,
        out_len: *mut usize,
    ) -> i32;
    fn dusrelimage_crop(
        input: *const u8,
        input_len: usize,
        options_json: *const c_char,
        out: *mut *mut u8,
        out_len: *mut usize,
    ) -> i32;
    fn dusrelimage_convert(
        input: *const u8,
        input_len: usize,
        options_json: *const c_char,
        out: *mut *mut u8,
        out_len: *mut usize,
    ) -> i32;
    fn dusrelimage_add_watermark(
        input: *const u8,
        input_len: usize,
        options_json: *const c_char,
        out: *mut *mut u8,
        out_len: *mut usize,
    ) -> i32;
    fn dusrelimage_annotate(
        input: *const u8,
        input_len: usize,
        annotations_json: *const c_char,
        out: *mut *mut u8,
        out_len: *mut usize,
    ) -> i32;
    fn dusrelimage_make_collage(
        inputs: *const *const u8,
        input_lens: *const usize,
        count: usize,
        options_json: *const c_char,
        out: *mut *mut u8,
        out_len: *mut usize,
    ) -> i32;
    fn dusrelimage_pixelate(
        input: *const u8,
        input_len: usize,
        options_json: *const c_char,
        out: *mut *mut u8,
        out_len: *mut usize,
    ) -> i32;
    fn dusrelimage_bulk_process(
        inputs: *const *const u8,
        input_lens: *const usize,
        count: usize,
        op: *const c_char,
        options_json: *const c_char,
        out_json: *mut *mut u8,
        out_len: *mut usize,
    ) -> i32;
    fn dusrelimage_pick_colour(
        input: *const u8,
        input_len: usize,
        x: u32,
        y: u32,
        out_json: *mut *mut u8,
        out_len: *mut usize,
    ) -> i32;
    fn dusrelimage_extract_palette(
        input: *const u8,
        input_len: usize,
        count: u8,
        out_json: *mut *mut u8,
        out_len: *mut usize,
    ) -> i32;
    fn dusrelimage_get_image_info(
        input: *const u8,
        input_len: usize,
        options_json: *const c_char,
        out_json: *mut *mut u8,
        out_len: *mut usize,
    ) -> i32;
    fn dusrelimage_last_error() -> *const c_char;
    fn dusrelimage_free_buffer(ptr: *mut u8, len: usize);
    fn dusrelimage_version() -> *const c_char;
}

// Status codes (must match `dusrelimage.h`).
const OK: i32 = 0;
const INVALID_IMAGE: i32 = 1;
const IMAGE_TOO_LARGE: i32 = 3;
const TARGET_SIZE_UNREACHABLE: i32 = 4;
const CROP_OUT_OF_BOUNDS: i32 = 5;
const PIXEL_OUT_OF_BOUNDS: i32 = 6;
const INVALID_ARGUMENT: i32 = 9;
const EMPTY_INPUT: i32 = 10;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Owned FFI output: frees itself with `dusrelimage_free_buffer`.
struct OutBuf {
    ptr: *mut u8,
    len: usize,
}

impl OutBuf {
    fn bytes(&self) -> &[u8] {
        // SAFETY: `ptr`/`len` are exactly what an FFI call returned.
        unsafe { std::slice::from_raw_parts(self.ptr, self.len) }
    }
    fn json(&self) -> serde_json::Value {
        serde_json::from_slice(self.bytes()).expect("FFI JSON output parses")
    }
}

impl Drop for OutBuf {
    fn drop(&mut self) {
        // SAFETY: freed exactly once, with the returned pointer/length.
        unsafe { dusrelimage_free_buffer(self.ptr, self.len) };
    }
}

fn cstr(s: &str) -> CString {
    CString::new(s).unwrap()
}

/// Call a bytes-in/bytes-out FFI op; on success return the output.
fn call_bytes(
    f: unsafe extern "C" fn(*const u8, usize, *const c_char, *mut *mut u8, *mut usize) -> i32,
    input: &[u8],
    options_json: Option<&str>,
) -> (i32, Option<OutBuf>) {
    let mut out: *mut u8 = ptr::null_mut();
    let mut out_len: usize = 0;
    let opts = options_json.map(cstr);
    let opts_ptr = opts.as_ref().map_or(ptr::null(), |c| c.as_ptr());
    let status = unsafe {
        f(
            input.as_ptr(),
            input.len(),
            opts_ptr,
            &mut out,
            &mut out_len,
        )
    };
    if status == OK {
        (
            status,
            Some(OutBuf {
                ptr: out,
                len: out_len,
            }),
        )
    } else {
        assert!(out.is_null(), "failed call must not return a buffer");
        (status, None)
    }
}

fn last_error_json() -> serde_json::Value {
    let ptr = unsafe { dusrelimage_last_error() };
    assert!(!ptr.is_null(), "last_error must be set after a failure");
    let text = unsafe { CStr::from_ptr(ptr) }.to_str().unwrap();
    serde_json::from_str(text).expect("last_error is JSON")
}

fn last_error_is_null() -> bool {
    unsafe { dusrelimage_last_error() }.is_null()
}

/// Dimensions + format of image bytes, via the core (test oracle).
fn info_of(bytes: &[u8]) -> (String, u32, u32) {
    let info = get_image_info(bytes, &CommonOptions::default()).unwrap();
    (info.format, info.width, info.height)
}

fn gradient() -> Vec<u8> {
    make_test_image(MockKind::Gradient, 100, 80, 3)
}

fn noise_400x300() -> Vec<u8> {
    make_test_image(MockKind::Noise, 400, 300, 7)
}

fn solid_white_100() -> Vec<u8> {
    solid_png(100, 100, Colour::white())
}

// ---------------------------------------------------------------------------
// Tool 1 — compress / compress_to_size
// ---------------------------------------------------------------------------

#[test]
fn compress_to_jpeg_happy_path() {
    let input = gradient();
    let (status, out) = call_bytes(
        dusrelimage_compress,
        &input,
        Some(r#"{"format": "Jpeg", "quality": 80}"#),
    );
    assert_eq!(status, OK);
    let out = out.unwrap();
    assert_eq!(out.bytes()[..2], [0xFF, 0xD8], "JPEG magic");
    assert_eq!(info_of(out.bytes()), ("jpeg".to_string(), 100, 80));
}

#[test]
fn compress_null_options_uses_defaults() {
    let input = gradient();
    let (status, out) = call_bytes(dusrelimage_compress, &input, None);
    assert_eq!(status, OK);
    let out = out.unwrap();
    // Defaults keep the input format (PNG).
    assert_eq!(info_of(out.bytes()).0, "png");
}

#[test]
fn compress_partial_options_deep_merge() {
    // `{"quality": 50}` alone must work: the other fields come from
    // the Rust Default (this is the serde-partial-object trap).
    let input = gradient();
    let (status, out) = call_bytes(
        dusrelimage_compress,
        &input,
        Some(r#"{"quality": 50, "format": "Jpeg"}"#),
    );
    assert_eq!(status, OK);
    assert_eq!(info_of(out.unwrap().bytes()).0, "jpeg");
}

#[test]
fn compress_to_size_reaches_target() {
    let input = noise_400x300();
    let mut raw_out: *mut u8 = ptr::null_mut();
    let mut raw_len: usize = 0;
    let opts = cstr(r#"{"format": "Jpeg", "quality": 90}"#);
    let status = unsafe {
        dusrelimage_compress_to_size(
            input.as_ptr(),
            input.len(),
            200_000,
            opts.as_ptr(),
            &mut raw_out,
            &mut raw_len,
        )
    };
    assert_eq!(status, OK);
    let out = OutBuf {
        ptr: raw_out,
        len: raw_len,
    };
    assert!(out.len <= 200_000, "output {} > target", out.len);
    assert_eq!(info_of(out.bytes()), ("jpeg".to_string(), 400, 300));
}

#[test]
fn compress_to_size_unreachable() {
    let input = noise_400x300();
    let mut raw_out: *mut u8 = ptr::null_mut();
    let mut raw_len: usize = 0;
    let opts = cstr(r#"{"format": "Jpeg", "quality": 90}"#);
    let status = unsafe {
        dusrelimage_compress_to_size(
            input.as_ptr(),
            input.len(),
            100,
            opts.as_ptr(),
            &mut raw_out,
            &mut raw_len,
        )
    };
    assert_eq!(status, TARGET_SIZE_UNREACHABLE);
    assert!(raw_out.is_null());
    assert_eq!(last_error_json()["code"], "TARGET_SIZE_UNREACHABLE");
}

// ---------------------------------------------------------------------------
// Tool 2 — resize
// ---------------------------------------------------------------------------

#[test]
fn resize_percent_halves_dimensions() {
    let input = gradient();
    let (status, out) = call_bytes(dusrelimage_resize, &input, Some(r#"{"percent": 50.0}"#));
    assert_eq!(status, OK);
    assert_eq!(info_of(out.unwrap().bytes()), ("png".to_string(), 50, 40));
}

#[test]
fn resize_width_only_keeps_aspect_ratio() {
    let input = gradient();
    let (status, out) = call_bytes(dusrelimage_resize, &input, Some(r#"{"width": 50}"#));
    assert_eq!(status, OK);
    assert_eq!(info_of(out.unwrap().bytes()), ("png".to_string(), 50, 40));
}

// ---------------------------------------------------------------------------
// Tool 3 — crop
// ---------------------------------------------------------------------------

#[test]
fn crop_explicit_rect() {
    let input = gradient();
    let (status, out) = call_bytes(
        dusrelimage_crop,
        &input,
        Some(r#"{"x": 10, "y": 10, "width": 20, "height": 20}"#),
    );
    assert_eq!(status, OK);
    assert_eq!(info_of(out.unwrap().bytes()), ("png".to_string(), 20, 20));
}

#[test]
fn crop_out_of_bounds() {
    let input = gradient();
    let (status, _) = call_bytes(
        dusrelimage_crop,
        &input,
        Some(r#"{"x": 90, "y": 10, "width": 20, "height": 20}"#),
    );
    assert_eq!(status, CROP_OUT_OF_BOUNDS);
    assert_eq!(last_error_json()["code"], "CROP_OUT_OF_BOUNDS");
}

// ---------------------------------------------------------------------------
// Tool 4 — convert
// ---------------------------------------------------------------------------

#[test]
fn convert_to_png_and_bmp() {
    let input = gradient();
    for format in ["Png", "Bmp"] {
        let opts = format!(r#"{{"format": "{format}"}}"#);
        let (status, out) = call_bytes(dusrelimage_convert, &input, Some(&opts));
        assert_eq!(status, OK, "convert to {format}");
        assert_eq!(info_of(out.unwrap().bytes()).0, format.to_ascii_lowercase());
    }
}

// ---------------------------------------------------------------------------
// Tool 5 — watermark
// ---------------------------------------------------------------------------

#[test]
fn watermark_text_bottom_right() {
    let input = solid_white_100();
    let (status, out) = call_bytes(
        dusrelimage_add_watermark,
        &input,
        Some(
            r##"{"opacity": 1.0, "position": "BottomRight",
                "text": {"text": "Hi", "size": 48.0, "colour": "#000000"}}"##,
        ),
    );
    assert_eq!(status, OK);
    assert_eq!(info_of(out.unwrap().bytes()), ("png".to_string(), 100, 100));
}

#[test]
fn watermark_no_text_no_logo_is_invalid_argument() {
    let input = solid_white_100();
    let (status, _) = call_bytes(dusrelimage_add_watermark, &input, Some(r#"{}"#));
    assert_eq!(status, INVALID_ARGUMENT);
}

// ---------------------------------------------------------------------------
// Tool 6 — annotate
// ---------------------------------------------------------------------------

#[test]
fn annotate_tagged_and_type_forms() {
    let input = solid_white_100();
    // Serde externally-tagged form.
    let tagged = r##"[{"Rect": {"x": 10, "y": 10, "w": 40, "h": 30, "colour": "#FF0000", "width": 2, "fill": "#FF0000"}}]"##;
    let (status, out) = call_bytes(dusrelimage_annotate, &input, Some(tagged));
    assert_eq!(status, OK);
    assert_eq!(info_of(out.unwrap().bytes()), ("png".to_string(), 100, 100));
    // {"type": "..."} form (playground / WASM style).
    let typed =
        r##"[{"type": "Line", "from": [0, 0], "to": [99, 99], "colour": "#000000", "width": 1}]"##;
    let (status, out) = call_bytes(dusrelimage_annotate, &input, Some(typed));
    assert_eq!(status, OK);
    assert_eq!(info_of(out.unwrap().bytes()), ("png".to_string(), 100, 100));
}

#[test]
fn annotate_empty_list_is_invalid_argument() {
    let input = solid_white_100();
    let (status, _) = call_bytes(dusrelimage_annotate, &input, Some(r#"[]"#));
    assert_eq!(status, INVALID_ARGUMENT);
}

// ---------------------------------------------------------------------------
// Tool 9 — collage
// ---------------------------------------------------------------------------

#[test]
fn collage_grid_1x2() {
    let red = solid_png(90, 80, Colour::from_rgb(255, 0, 0));
    let blue = solid_png(90, 80, Colour::from_rgb(0, 0, 255));
    let inputs = [red.as_ptr(), blue.as_ptr()];
    let lens = [red.len(), blue.len()];
    let mut out: *mut u8 = ptr::null_mut();
    let mut out_len: usize = 0;
    let opts = cstr(
        r##"{"layout": {"Grid": {"rows": 1, "cols": 2}}, "gap": 10,
            "background": "#FFFFFF", "width": 210, "height": 100}"##,
    );
    let status = unsafe {
        dusrelimage_make_collage(
            inputs.as_ptr(),
            lens.as_ptr(),
            2,
            opts.as_ptr(),
            &mut out,
            &mut out_len,
        )
    };
    assert_eq!(status, OK);
    let out = OutBuf {
        ptr: out,
        len: out_len,
    };
    assert_eq!(info_of(out.bytes()), ("png".to_string(), 210, 100));
}

#[test]
fn collage_layout_shorthand_2x2() {
    let red = solid_png(40, 40, Colour::from_rgb(255, 0, 0));
    let inputs = [red.as_ptr()];
    let lens = [red.len()];
    let opts = cstr(r#"{"layout": "2x2", "width": 200, "height": 200}"#);
    let mut out: *mut u8 = ptr::null_mut();
    let mut out_len: usize = 0;
    let status = unsafe {
        dusrelimage_make_collage(
            inputs.as_ptr(),
            lens.as_ptr(),
            1,
            opts.as_ptr(),
            &mut out,
            &mut out_len,
        )
    };
    assert_eq!(status, OK);
    let out = OutBuf {
        ptr: out,
        len: out_len,
    };
    assert_eq!(info_of(out.bytes()), ("png".to_string(), 200, 200));
}

#[test]
fn collage_empty_input_list() {
    let opts = cstr(r#"{"width": 200, "height": 200}"#);
    let mut out: *mut u8 = ptr::null_mut();
    let mut out_len: usize = 0;
    let status = unsafe {
        dusrelimage_make_collage(
            ptr::null(),
            ptr::null(),
            0,
            opts.as_ptr(),
            &mut out,
            &mut out_len,
        )
    };
    assert_eq!(status, EMPTY_INPUT);
}

// ---------------------------------------------------------------------------
// Tool 10 — pixelate
// ---------------------------------------------------------------------------

#[test]
fn pixelate_full_image() {
    let input = gradient();
    let (status, out) = call_bytes(
        dusrelimage_pixelate,
        &input,
        Some(r#"{"x": 0, "y": 0, "width": 100, "height": 80, "block_size": 20}"#),
    );
    assert_eq!(status, OK);
    assert_eq!(info_of(out.unwrap().bytes()), ("png".to_string(), 100, 80));
}

#[test]
fn pixelate_block_size_one_is_invalid_argument() {
    let input = gradient();
    let (status, _) = call_bytes(dusrelimage_pixelate, &input, Some(r#"{"block_size": 1}"#));
    assert_eq!(status, INVALID_ARGUMENT);
}

// ---------------------------------------------------------------------------
// Tool 7 — pick_colour
// ---------------------------------------------------------------------------

#[test]
fn pick_colour_known_pixel() {
    let input = solid_png(64, 48, Colour::from_rgb(10, 20, 30));
    let mut out: *mut u8 = ptr::null_mut();
    let mut out_len: usize = 0;
    let status = unsafe {
        dusrelimage_pick_colour(input.as_ptr(), input.len(), 5, 5, &mut out, &mut out_len)
    };
    assert_eq!(status, OK);
    let out = OutBuf {
        ptr: out,
        len: out_len,
    };
    let json = out.json();
    assert_eq!(json["hex"], "#0A141E");
    assert_eq!(json["r"], 10);
    assert_eq!(json["g"], 20);
    assert_eq!(json["b"], 30);
    assert_eq!(json["a"], 255);
}

#[test]
fn pick_colour_out_of_bounds() {
    let input = solid_png(64, 48, Colour::white());
    let mut out: *mut u8 = ptr::null_mut();
    let mut out_len: usize = 0;
    let status = unsafe {
        dusrelimage_pick_colour(input.as_ptr(), input.len(), 64, 0, &mut out, &mut out_len)
    };
    assert_eq!(status, PIXEL_OUT_OF_BOUNDS);
    assert_eq!(last_error_json()["code"], "PIXEL_OUT_OF_BOUNDS");
}

// ---------------------------------------------------------------------------
// Tool 8 — extract_palette
// ---------------------------------------------------------------------------

#[test]
fn extract_palette_solid_is_one_entry() {
    let input = solid_png(50, 50, Colour::from_rgb(1, 2, 3));
    let mut out: *mut u8 = ptr::null_mut();
    let mut out_len: usize = 0;
    let status = unsafe {
        dusrelimage_extract_palette(input.as_ptr(), input.len(), 4, &mut out, &mut out_len)
    };
    assert_eq!(status, OK);
    let out = OutBuf {
        ptr: out,
        len: out_len,
    };
    let json = out.json();
    assert_eq!(json.as_array().unwrap().len(), 1);
    assert_eq!(json[0]["colour"]["hex"], "#010203");
    assert_eq!(json[0]["share"], 1.0);
}

#[test]
fn extract_palette_count_zero_is_invalid_argument() {
    let input = solid_png(50, 50, Colour::white());
    let mut out: *mut u8 = ptr::null_mut();
    let mut out_len: usize = 0;
    let status = unsafe {
        dusrelimage_extract_palette(input.as_ptr(), input.len(), 0, &mut out, &mut out_len)
    };
    assert_eq!(status, INVALID_ARGUMENT);
}

// ---------------------------------------------------------------------------
// Info
// ---------------------------------------------------------------------------

#[test]
fn get_image_info_png_gradient() {
    let input = make_test_image(MockKind::Gradient, 64, 48, 4);
    // NULL options = all defaults.
    let mut out: *mut u8 = ptr::null_mut();
    let mut out_len: usize = 0;
    let status = unsafe {
        dusrelimage_get_image_info(
            input.as_ptr(),
            input.len(),
            ptr::null(),
            &mut out,
            &mut out_len,
        )
    };
    assert_eq!(status, OK);
    let out = OutBuf {
        ptr: out,
        len: out_len,
    };
    let json = out.json();
    assert_eq!(json["format"], "png");
    assert_eq!(json["width"], 64);
    assert_eq!(json["height"], 48);
    assert_eq!(json["has_alpha"], false);
    assert_eq!(json["colour_type"], "rgb8");
    assert_eq!(json["file_size"], input.len() as u64);
}

#[test]
fn get_image_info_options_enforced() {
    // The two `info-too-large-*` shared vectors: options must reach
    // the core (nested `common` form and flat form both work).
    let input = make_test_image(MockKind::Solid, 64, 48, 1);
    for options in [
        r#"{"common": {"max_dimension": 10}}"#,
        r#"{"max_pixels": 10}"#,
    ] {
        let opts = cstr(options);
        let mut out: *mut u8 = ptr::null_mut();
        let mut out_len: usize = 0;
        let status = unsafe {
            dusrelimage_get_image_info(
                input.as_ptr(),
                input.len(),
                opts.as_ptr(),
                &mut out,
                &mut out_len,
            )
        };
        assert_eq!(status, IMAGE_TOO_LARGE, "options {options}");
        let _ = OutBuf {
            ptr: out,
            len: out_len,
        };
    }
    // Bad (non-object) options JSON is INVALID_ARGUMENT, not defaults.
    let opts = cstr(r#""hello""#);
    let mut out: *mut u8 = ptr::null_mut();
    let mut out_len: usize = 0;
    let status = unsafe {
        dusrelimage_get_image_info(
            input.as_ptr(),
            input.len(),
            opts.as_ptr(),
            &mut out,
            &mut out_len,
        )
    };
    assert_eq!(status, INVALID_ARGUMENT);
    let _ = OutBuf {
        ptr: out,
        len: out_len,
    };
}

// ---------------------------------------------------------------------------
// Bulk
// ---------------------------------------------------------------------------

fn call_bulk(
    inputs: &[Vec<u8>],
    op: &str,
    options_json: Option<&str>,
) -> (i32, Option<serde_json::Value>) {
    let ptrs: Vec<*const u8> = inputs.iter().map(|b| b.as_ptr()).collect();
    let lens: Vec<usize> = inputs.iter().map(|b| b.len()).collect();
    let op_c = cstr(op);
    let opts = options_json.map(cstr);
    let opts_ptr = opts.as_ref().map_or(ptr::null(), |c| c.as_ptr());
    let mut out: *mut u8 = ptr::null_mut();
    let mut out_len: usize = 0;
    let status = unsafe {
        dusrelimage_bulk_process(
            ptrs.as_ptr(),
            lens.as_ptr(),
            inputs.len(),
            op_c.as_ptr(),
            opts_ptr,
            &mut out,
            &mut out_len,
        )
    };
    if status == OK {
        let buf = OutBuf {
            ptr: out,
            len: out_len,
        };
        (status, Some(buf.json()))
    } else {
        assert!(out.is_null());
        (status, None)
    }
}

fn base64_decode_for_test(s: &str) -> Vec<u8> {
    // Minimal decoder: bulk outputs are checked by re-decoding the
    // image, so only standard alphabet + padding appear.
    let mut out = Vec::new();
    let mut acc: u32 = 0;
    let mut nbits = 0;
    for c in s.bytes() {
        let v = match c {
            b'A'..=b'Z' => (c - b'A') as u32,
            b'a'..=b'z' => (c - b'a' + 26) as u32,
            b'0'..=b'9' => (c - b'0' + 52) as u32,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            _ => panic!("bad base64 char"),
        };
        acc = (acc << 6) | v;
        nbits += 6;
        if nbits >= 8 {
            nbits -= 8;
            out.push((acc >> nbits) as u8);
        }
    }
    out
}

#[test]
fn bulk_pixelate_preserves_order_and_partial_failures() {
    let good = make_test_image(MockKind::Solid, 32, 32, 1);
    let garbage = b"this is not an image".to_vec();
    let inputs = vec![good.clone(), garbage, good.clone()];
    let (status, json) = call_bulk(&inputs, "pixelate", Some(r#"{"block_size": 10}"#));
    assert_eq!(status, OK);
    let results = json.unwrap();
    let arr = results.as_array().unwrap();
    assert_eq!(arr.len(), 3);
    assert_eq!(arr[0]["ok"], true);
    assert_eq!(arr[1]["ok"], false);
    assert_eq!(arr[1]["error"]["code"], "INVALID_IMAGE");
    assert_eq!(arr[2]["ok"], true);
    // Item 0's base64 output decodes to a 32x32 image.
    let decoded = base64_decode_for_test(arr[0]["output"].as_str().unwrap());
    assert_eq!(info_of(&decoded), ("png".to_string(), 32, 32));
}

#[test]
fn bulk_pick_colour() {
    let input = solid_png(20, 20, Colour::from_rgb(9, 8, 7));
    let (status, json) = call_bulk(&[input], "pick_colour", Some(r#"{"x": 3, "y": 3}"#));
    assert_eq!(status, OK);
    let arr = json.unwrap().as_array().unwrap().clone();
    assert_eq!(arr[0]["ok"], true);
    assert_eq!(arr[0]["colour"]["hex"], "#090807");
}

#[test]
fn bulk_palette_flattened_shape() {
    let input = solid_png(20, 20, Colour::from_rgb(9, 8, 7));
    let (status, json) = call_bulk(&[input], "palette", Some(r#"{"count": 2}"#));
    assert_eq!(status, OK);
    let arr = json.unwrap().as_array().unwrap().clone();
    assert_eq!(arr[0]["ok"], true);
    // WASM bulk shape: flattened entries, hex at the top level.
    assert_eq!(arr[0]["palette"][0]["hex"], "#090807");
    assert_eq!(arr[0]["palette"][0]["share"], 1.0);
}

#[test]
fn bulk_compress_to_size_uses_target_bytes() {
    let input = noise_400x300();
    let (status, json) = call_bulk(
        &[input],
        "compress_to_size",
        Some(r#"{"format": "Jpeg", "quality": 90, "target_bytes": 200000}"#),
    );
    assert_eq!(status, OK);
    let arr = json.unwrap().as_array().unwrap().clone();
    assert_eq!(arr[0]["ok"], true);
    let decoded = base64_decode_for_test(arr[0]["output"].as_str().unwrap());
    assert!(decoded.len() <= 200_000);
}

#[test]
fn bulk_collage_is_per_item_invalid_argument() {
    let input = make_test_image(MockKind::Solid, 32, 32, 1);
    let (status, json) = call_bulk(&[input], "collage", Some(r#"{}"#));
    assert_eq!(status, OK);
    let arr = json.unwrap().as_array().unwrap().clone();
    assert_eq!(arr[0]["ok"], false);
    assert_eq!(arr[0]["error"]["code"], "INVALID_ARGUMENT");
}

#[test]
fn bulk_empty_input_list_returns_empty_array() {
    let (status, json) = call_bulk(&[], "pixelate", Some(r#"{"block_size": 10}"#));
    assert_eq!(status, OK);
    assert_eq!(json.unwrap(), serde_json::json!([]));
}

#[test]
fn bulk_unknown_op_is_invalid_argument() {
    let input = gradient();
    let (status, _) = call_bulk(&[input], "definitely_not_an_op", Some(r#"{}"#));
    assert_eq!(status, INVALID_ARGUMENT);
    assert_eq!(last_error_json()["code"], "INVALID_ARGUMENT");
}

// ---------------------------------------------------------------------------
// Error paths — bad input, bad JSON, NULL pointers
// ---------------------------------------------------------------------------

#[test]
fn garbage_bytes_are_invalid_image() {
    let garbage = b"this is not an image at all, just plain text bytes 1234567890";
    let (status, _) = call_bytes(dusrelimage_compress, garbage, None);
    assert_eq!(status, INVALID_IMAGE);
    let err = last_error_json();
    assert_eq!(err["code"], "INVALID_IMAGE");
    assert!(
        err["message_en"]
            .as_str()
            .unwrap()
            .contains("Invalid image")
    );
    assert!(err["message_hi"].as_str().unwrap().contains("Galat image"));
}

#[test]
fn empty_input_is_empty_input() {
    let (status, _) = call_bytes(dusrelimage_compress, &[], None);
    assert_eq!(status, EMPTY_INPUT);
    assert_eq!(last_error_json()["code"], "EMPTY_INPUT");
}

#[test]
fn invalid_json_options_are_invalid_argument() {
    let input = gradient();
    let (status, _) = call_bytes(dusrelimage_compress, &input, Some(r#"{"quality": "#));
    assert_eq!(status, INVALID_ARGUMENT);
}

#[test]
fn wrong_typed_json_value_is_invalid_argument() {
    let input = gradient();
    // quality is a string, not a number.
    let (status, _) = call_bytes(dusrelimage_compress, &input, Some(r#"{"quality": "high"}"#));
    assert_eq!(status, INVALID_ARGUMENT);
}

#[test]
fn null_input_pointer_with_length_is_invalid_argument() {
    let mut out: *mut u8 = ptr::null_mut();
    let mut out_len: usize = 0;
    let status =
        unsafe { dusrelimage_compress(ptr::null(), 10, ptr::null(), &mut out, &mut out_len) };
    assert_eq!(status, INVALID_ARGUMENT);
    assert!(out.is_null());
}

#[test]
fn null_output_pointers_are_invalid_argument() {
    let input = gradient();
    let status = unsafe {
        dusrelimage_compress(
            input.as_ptr(),
            input.len(),
            ptr::null(),
            ptr::null_mut(),
            ptr::null_mut(),
        )
    };
    assert_eq!(status, INVALID_ARGUMENT);
    // NULL out_len alone is also invalid.
    let mut out: *mut u8 = ptr::null_mut();
    let status = unsafe {
        dusrelimage_compress(
            input.as_ptr(),
            input.len(),
            ptr::null(),
            &mut out,
            ptr::null_mut(),
        )
    };
    assert_eq!(status, INVALID_ARGUMENT);
    assert!(out.is_null());
}

#[test]
fn null_input_pointer_with_zero_length_is_empty_input() {
    let mut out: *mut u8 = ptr::null_mut();
    let mut out_len: usize = 0;
    let status =
        unsafe { dusrelimage_compress(ptr::null(), 0, ptr::null(), &mut out, &mut out_len) };
    assert_eq!(status, EMPTY_INPUT);
}

// ---------------------------------------------------------------------------
// Diagnostics — free_buffer, last_error, version
// ---------------------------------------------------------------------------

#[test]
fn free_buffer_round_trip_then_reuse() {
    // Compress, free the buffer, then compress again: the second call
    // must still work (no allocator corruption from the first free).
    let input = gradient();
    for _ in 0..2 {
        let (status, out) = call_bytes(dusrelimage_compress, &input, None);
        assert_eq!(status, OK);
        drop(out.unwrap()); // OutBuf::drop calls dusrelimage_free_buffer
    }
    // Freeing NULL / zero-length is a no-op, never a crash.
    unsafe {
        dusrelimage_free_buffer(ptr::null_mut(), 0);
        dusrelimage_free_buffer(ptr::null_mut(), 10);
    }
}

#[test]
fn last_error_cleared_after_success() {
    let garbage = b"not an image";
    let (status, _) = call_bytes(dusrelimage_compress, garbage, None);
    assert_eq!(status, INVALID_IMAGE);
    assert!(!last_error_is_null());
    // A successful call clears the error.
    let input = gradient();
    let (status, _) = call_bytes(dusrelimage_compress, &input, None);
    assert_eq!(status, OK);
    assert!(last_error_is_null());
}

#[test]
fn version_is_0_1_0() {
    let ptr = unsafe { dusrelimage_version() };
    assert!(!ptr.is_null());
    let version = unsafe { CStr::from_ptr(ptr) }.to_str().unwrap();
    assert_eq!(version, "0.1.0");
    assert_eq!(version, dusrelimage::VERSION);
}
