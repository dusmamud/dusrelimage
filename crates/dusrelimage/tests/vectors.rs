//! Shared test vectors (SPEC section 6): Rust, Python and JS all run
//! `shared/test-vectors.json` and must agree.
//!
//! This runner loads the JSON (path relative to the crate manifest
//! dir), builds each fixture — `dusrelimage mock` kinds
//! (`solid`/`gradient`/`noise` + seed) via the core
//! `fixtures::make_test_image`, plus the documented derived fixtures
//! (`solid_rgb`, `transparent_rgba`, `two_colour`, `tiff`, `jpeg`,
//! `avif`, `garbage`, `empty`) — runs the op, and asserts the
//! expected dimensions / pixel spot-checks / error code.
//!
//! It FAILS LOUDLY on any case whose fixture/op kind it cannot
//! handle: unknown fixture kinds, ops, bulk ops or expected keys
//! panic with the case id instead of silently skipping the case.
//! The case count is asserted too (82 at the time of writing), so
//! the JSON growing without a runner update is caught here.

use std::io::Cursor;

use dusrelimage::bulk::{BulkOp, BulkOutput};
use dusrelimage::common::get_image_info;
use dusrelimage::fixtures::{MockKind, make_test_image, solid_png};
use dusrelimage::options::{
    CollageOptions, CommonOptions, CompressOptions, ConvertOptions, CropOptions, PixelateOptions,
    ResizeOptions, WatermarkOptions,
};
use dusrelimage::types::{
    Annotation, CollageLayout, Colour, Filter, FitMode, ImageFormat, ImageInfo, ImageWatermark,
    Position, TextWatermark,
};
use dusrelimage::{
    DusrelError, add_watermark, annotate_image, bulk_process, compress_image, compress_to_size,
    convert_image, crop_image, extract_palette, make_collage, pick_colour, pixelate_image,
    resize_image,
};

type Json = serde_json::Value;

/// One case's outcome, for the shared assertion step.
enum Outcome {
    Image(Vec<u8>),
    Info(ImageInfo),
    Colour(Colour),
    Palette(Vec<dusrelimage::PaletteColour>),
    Bulk(Vec<dusrelimage::BulkItemResult>),
}

/// Decode bytes to RGBA8 with the `image` crate (test-only helper).
fn decode_rgba(bytes: &[u8]) -> image::RgbaImage {
    image::ImageReader::new(Cursor::new(bytes.to_vec()))
        .with_guessed_format()
        .unwrap_or_else(|e| panic!("test output is not a decodable image: {e}"))
        .decode()
        .unwrap_or_else(|e| panic!("test output failed to decode: {e}"))
        .to_rgba8()
}

/// True when any pixel in `[x0, x1) x [y0, y1)` is dark (any channel
/// < 128) — the same definition the draw-tools tests use.
fn region_has_dark(img: &image::RgbaImage, x0: u32, y0: u32, x1: u32, y1: u32) -> bool {
    for y in y0..y1 {
        for x in x0..x1 {
            let px = img.get_pixel(x, y);
            if px[0] < 128 || px[1] < 128 || px[2] < 128 {
                return true;
            }
        }
    }
    false
}

/// True when every pixel in `[x0, x1) x [y0, y1)` is white.
fn region_all_white(img: &image::RgbaImage, x0: u32, y0: u32, x1: u32, y1: u32) -> bool {
    for y in y0..y1 {
        for x in x0..x1 {
            let px = img.get_pixel(x, y);
            if px[0] != 255 || px[1] != 255 || px[2] != 255 {
                return false;
            }
        }
    }
    true
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn rgb_of(v: &Json, id: &str) -> [u8; 3] {
    let arr = v
        .as_array()
        .unwrap_or_else(|| panic!("case {id}: rgb must be an array"));
    assert_eq!(arr.len(), 3, "case {id}: rgb must have 3 channels");
    [
        u_of(&arr[0], id, "rgb[0]"),
        u_of(&arr[1], id, "rgb[1]"),
        u_of(&arr[2], id, "rgb[2]"),
    ]
}

fn u_of(v: &Json, id: &str, what: &str) -> u8 {
    v.as_u64()
        .unwrap_or_else(|| panic!("case {id}: {what} must be an unsigned integer")) as u8
}

fn u32_of(v: &Json, id: &str, what: &str) -> u32 {
    v.as_u64()
        .unwrap_or_else(|| panic!("case {id}: {what} must be an unsigned integer")) as u32
}

fn fixture_dims(f: &Json, id: &str) -> (u32, u32) {
    (
        u32_of(&f["width"], id, "fixture width"),
        u32_of(&f["height"], id, "fixture height"),
    )
}

/// Parse a "48x32" dimension pair out of a derived_from string.
fn parse_dims_from_derived(s: &str, id: &str) -> (u32, u32) {
    for token in s.split(|c: char| !c.is_ascii_digit() && c != 'x') {
        if let Some((w, h)) = token.split_once('x') {
            if let (Ok(w), Ok(h)) = (w.parse::<u32>(), h.parse::<u32>()) {
                return (w, h);
            }
        }
    }
    panic!("case {id}: no WxH dimensions in derived_from: {s}")
}

/// Build one fixture's bytes. `f` is the case's `fixture` object.
fn fixture_bytes(f: &Json, id: &str) -> Vec<u8> {
    let kind = f["kind"]
        .as_str()
        .unwrap_or_else(|| panic!("case {id}: fixture has no string kind"));
    match kind {
        "solid" | "gradient" | "noise" => {
            let (width, height) = fixture_dims(f, id);
            let seed = f["seed"]
                .as_u64()
                .unwrap_or_else(|| panic!("case {id}: mock fixture needs a seed"));
            let mock = match kind {
                "solid" => MockKind::Solid,
                "gradient" => MockKind::Gradient,
                _ => MockKind::Noise,
            };
            make_test_image(mock, width, height, seed)
        }
        "solid_rgb" => {
            let (width, height) = fixture_dims(f, id);
            let rgb = rgb_of(&f["rgb"], id);
            solid_png(width, height, Colour::from_rgb(rgb[0], rgb[1], rgb[2]))
        }
        "transparent_rgba" => {
            // Fully transparent RGBA PNG (fixtures are opaque, so this
            // one is built directly, like the existing convert test).
            let (width, height) = fixture_dims(f, id);
            let mut buf = Cursor::new(Vec::new());
            image::DynamicImage::ImageRgba8(image::RgbaImage::new(width, height))
                .write_to(&mut buf, image::ImageFormat::Png)
                .unwrap();
            buf.into_inner()
        }
        "two_colour" => {
            // Left half red, right half blue, 50/50 by construction.
            let (width, height) = fixture_dims(f, id);
            let mut img = image::RgbImage::new(width, height);
            for (x, y, px) in img.enumerate_pixels_mut() {
                *px = if x < width / 2 {
                    image::Rgb([255, 0, 0])
                } else {
                    image::Rgb([0, 0, 255])
                };
                let _ = y;
            }
            let mut buf = Cursor::new(Vec::new());
            image::DynamicImage::ImageRgb8(img)
                .write_to(&mut buf, image::ImageFormat::Png)
                .unwrap();
            buf.into_inner()
        }
        "tiff" => {
            // rgb (10,20,30) TIFF, written with the image crate.
            // Dimensions live only in derived_from ("48x32 TIFF").
            let (width, height) = match (f.get("width"), f.get("height")) {
                (Some(_), Some(_)) => fixture_dims(f, id),
                _ => {
                    let derived = f["derived_from"].as_str().unwrap_or_else(|| {
                        panic!("case {id}: tiff fixture needs width/height or derived_from")
                    });
                    parse_dims_from_derived(derived, id)
                }
            };
            // rgb lives only in derived_from: "rgb (10,20,30) 48x32 TIFF".
            let rgb = match f.get("rgb") {
                Some(_) => rgb_of(&f["rgb"], id),
                None => {
                    let derived = f["derived_from"].as_str().unwrap_or_else(|| {
                        panic!("case {id}: tiff fixture needs rgb or derived_from")
                    });
                    let start = derived.find('(').unwrap_or_else(|| {
                        panic!("case {id}: no rgb tuple in derived_from: {derived}")
                    });
                    let end = derived[start..].find(')').unwrap_or_else(|| {
                        panic!("case {id}: unterminated rgb tuple in derived_from: {derived}")
                    }) + start;
                    let parts: Vec<u8> = derived[start + 1..end]
                        .split(',')
                        .map(|n| {
                            n.trim().parse::<u8>().unwrap_or_else(|_| {
                                panic!("case {id}: bad rgb channel {n:?} in {derived}")
                            })
                        })
                        .collect();
                    assert_eq!(parts.len(), 3, "case {id}: rgb tuple needs 3 channels");
                    [parts[0], parts[1], parts[2]]
                }
            };
            let mut buf = Cursor::new(Vec::new());
            image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
                width,
                height,
                image::Rgb(rgb),
            ))
            .write_to(&mut buf, image::ImageFormat::Tiff)
            .unwrap();
            buf.into_inner()
        }
        "jpeg" => {
            // Derived: compress gradient 100x80 seed 2 to JPEG q80.
            let derived = f["derived_from"]
                .as_str()
                .unwrap_or_else(|| panic!("case {id}: jpeg fixture needs derived_from"));
            assert!(
                derived.contains("jpeg"),
                "case {id}: jpeg fixture derived_from does not mention jpeg: {derived}"
            );
            let input = make_test_image(MockKind::Gradient, 100, 80, 2);
            compress_image(
                &input,
                &CompressOptions {
                    format: Some(ImageFormat::Jpeg),
                    quality: 80,
                    ..CompressOptions::default()
                },
            )
            .unwrap()
        }
        "avif" => {
            // Derived: convert gradient 64x48 seed 4 to AVIF q80.
            let derived = f["derived_from"]
                .as_str()
                .unwrap_or_else(|| panic!("case {id}: avif fixture needs derived_from"));
            assert!(
                derived.contains("avif"),
                "case {id}: avif fixture derived_from does not mention avif: {derived}"
            );
            let input = make_test_image(MockKind::Gradient, 64, 48, 4);
            convert_image(
                &input,
                &ConvertOptions {
                    format: ImageFormat::Avif,
                    quality: 80,
                    ..ConvertOptions::default()
                },
            )
            .unwrap()
        }
        "garbage" => {
            // Derived: either "bytes [0, 1, 2, 3]" or an ASCII string.
            let derived = f["derived_from"]
                .as_str()
                .unwrap_or_else(|| panic!("case {id}: garbage fixture needs derived_from"));
            if let Some(list) = derived
                .strip_prefix("bytes [")
                .and_then(|s| s.strip_suffix(']'))
            {
                list.split(',')
                    .map(|n| {
                        n.trim().parse::<u8>().unwrap_or_else(|_| {
                            panic!("case {id}: bad garbage byte {n:?} in {derived}")
                        })
                    })
                    .collect()
            } else if let Some(text) = derived
                .strip_prefix("ASCII bytes: '")
                .and_then(|s| s.strip_suffix('\''))
            {
                text.as_bytes().to_vec()
            } else {
                panic!("case {id}: unhandled garbage derived_from: {derived}")
            }
        }
        "empty" => Vec::new(),
        other => panic!("case {id}: unknown fixture kind {other:?} — update this runner"),
    }
}

// ---------------------------------------------------------------------------
// Options parsing (JSON -> Rust structs; unknown values fail loudly)
// ---------------------------------------------------------------------------

fn common_options(o: &Json, id: &str) -> CommonOptions {
    let mut common = CommonOptions::default();
    if let Some(c) = o.get("common").and_then(|v| v.as_object()) {
        for (key, value) in c {
            match key.as_str() {
                "max_pixels" => {
                    common.max_pixels = value.as_u64().unwrap_or_else(|| {
                        panic!("case {id}: common.max_pixels must be an integer")
                    });
                }
                "max_dimension" => {
                    common.max_dimension = u32_of(value, id, "common.max_dimension");
                }
                "auto_orient" => {
                    common.auto_orient = value
                        .as_bool()
                        .unwrap_or_else(|| panic!("case {id}: common.auto_orient must be a bool"));
                }
                "keep_exif" => {
                    common.keep_exif = value
                        .as_bool()
                        .unwrap_or_else(|| panic!("case {id}: common.keep_exif must be a bool"));
                }
                other => panic!("case {id}: unknown common option {other:?}"),
            }
        }
    }
    common
}

fn parse_format(s: &str, id: &str) -> ImageFormat {
    match s {
        "Jpeg" => ImageFormat::Jpeg,
        "Png" => ImageFormat::Png,
        "WebP" => ImageFormat::WebP,
        "Avif" => ImageFormat::Avif,
        "Gif" => ImageFormat::Gif,
        "Bmp" => ImageFormat::Bmp,
        other => panic!("case {id}: unknown format {other:?}"),
    }
}

fn parse_fit(s: &str, id: &str) -> FitMode {
    match s {
        "Fit" => FitMode::Fit,
        "Fill" => FitMode::Fill,
        "Stretch" => FitMode::Stretch,
        "Crop" => FitMode::Crop,
        other => panic!("case {id}: unknown fit mode {other:?}"),
    }
}

fn parse_position(s: &str, id: &str) -> Position {
    match s {
        "TopLeft" => Position::TopLeft,
        "TopCenter" => Position::TopCenter,
        "TopRight" => Position::TopRight,
        "CenterLeft" => Position::CenterLeft,
        "Center" => Position::Center,
        "CenterRight" => Position::CenterRight,
        "BottomLeft" => Position::BottomLeft,
        "BottomCenter" => Position::BottomCenter,
        "BottomRight" => Position::BottomRight,
        "Tiled" => Position::Tiled,
        other => panic!("case {id}: unknown position {other:?}"),
    }
}

fn parse_colour(v: &Json, id: &str) -> Colour {
    let s = v
        .as_str()
        .unwrap_or_else(|| panic!("case {id}: colour must be a hex string"));
    Colour::from_hex(s).unwrap_or_else(|e| panic!("case {id}: bad colour {s:?}: {e}"))
}

fn parse_layout(v: &Json, id: &str) -> CollageLayout {
    if let Some(s) = v.as_str() {
        return match s {
            "Row" => CollageLayout::Row,
            "Column" => CollageLayout::Column,
            other => panic!("case {id}: unknown layout string {other:?}"),
        };
    }
    let grid = v.get("Grid").unwrap_or_else(|| {
        panic!("case {id}: layout must be \"Row\"/\"Column\" or {{\"Grid\": ...}}")
    });
    CollageLayout::Grid {
        rows: u32_of(&grid["rows"], id, "layout rows"),
        cols: u32_of(&grid["cols"], id, "layout cols"),
    }
}

fn parse_annotation(v: &Json, id: &str) -> Annotation {
    let kind = v["type"]
        .as_str()
        .unwrap_or_else(|| panic!("case {id}: annotation has no string type"));
    let colour = || parse_colour(&v["colour"], id);
    let width = || u32_of(&v["width"], id, "annotation width");
    let point = |key: &str| -> (u32, u32) {
        let p = v[key]
            .as_array()
            .unwrap_or_else(|| panic!("case {id}: annotation {key} must be [x, y]"));
        (u32_of(&p[0], id, "point x"), u32_of(&p[1], id, "point y"))
    };
    match kind {
        "Arrow" => Annotation::Arrow {
            from: point("from"),
            to: point("to"),
            colour: colour(),
            width: width(),
        },
        "Rect" => Annotation::Rect {
            x: u32_of(&v["x"], id, "rect x"),
            y: u32_of(&v["y"], id, "rect y"),
            w: u32_of(&v["w"], id, "rect w"),
            h: u32_of(&v["h"], id, "rect h"),
            colour: colour(),
            width: width(),
            fill: if v.get("fill").is_some_and(|f| !f.is_null()) {
                Some(parse_colour(&v["fill"], id))
            } else {
                None
            },
        },
        "Ellipse" => Annotation::Ellipse {
            x: u32_of(&v["x"], id, "ellipse x"),
            y: u32_of(&v["y"], id, "ellipse y"),
            w: u32_of(&v["w"], id, "ellipse w"),
            h: u32_of(&v["h"], id, "ellipse h"),
            colour: colour(),
            width: width(),
            fill: if v.get("fill").is_some_and(|f| !f.is_null()) {
                Some(parse_colour(&v["fill"], id))
            } else {
                None
            },
        },
        "Line" => Annotation::Line {
            from: point("from"),
            to: point("to"),
            colour: colour(),
            width: width(),
        },
        "Text" => Annotation::Text {
            x: u32_of(&v["x"], id, "text x"),
            y: u32_of(&v["y"], id, "text y"),
            text: v["text"]
                .as_str()
                .unwrap_or_else(|| panic!("case {id}: text annotation needs a string"))
                .to_string(),
            size: v["size"]
                .as_f64()
                .unwrap_or_else(|| panic!("case {id}: text annotation needs a size"))
                as f32,
            colour: colour(),
        },
        other => panic!("case {id}: unknown annotation type {other:?}"),
    }
}

fn parse_annotations(case: &Json, id: &str) -> Vec<Annotation> {
    case["annotations"]
        .as_array()
        .unwrap_or_else(|| panic!("case {id}: annotate case needs an annotations array"))
        .iter()
        .map(|a| parse_annotation(a, id))
        .collect()
}

fn watermark_options(o: &Json, id: &str) -> WatermarkOptions {
    let mut opts = WatermarkOptions {
        common: common_options(o, id),
        ..WatermarkOptions::default()
    };
    if let Some(text) = o.get("text") {
        opts.text = Some(TextWatermark {
            text: text["text"]
                .as_str()
                .unwrap_or_else(|| panic!("case {id}: watermark text needs a string"))
                .to_string(),
            size: text["size"]
                .as_f64()
                .unwrap_or_else(|| panic!("case {id}: watermark text needs a size"))
                as f32,
            colour: parse_colour(&text["colour"], id),
        });
    }
    if let Some(logo) = o.get("logo") {
        opts.logo = Some(ImageWatermark {
            image: fixture_bytes(&logo["image"], id),
            scale: logo["scale"]
                .as_f64()
                .unwrap_or_else(|| panic!("case {id}: watermark logo needs a scale"))
                as f32,
        });
    }
    if let Some(v) = o.get("opacity").and_then(|v| v.as_f64()) {
        opts.opacity = v as f32;
    }
    if let Some(v) = o.get("position").and_then(|v| v.as_str()) {
        opts.position = parse_position(v, id);
    }
    if let Some(v) = o.get("rotation_deg").and_then(|v| v.as_f64()) {
        opts.rotation_deg = v as f32;
    }
    if let Some(v) = o.get("margin").and_then(|v| v.as_u64()) {
        opts.margin = v as u32;
    }
    opts
}

fn compress_options(o: &Json, id: &str) -> CompressOptions {
    let mut opts = CompressOptions {
        common: common_options(o, id),
        ..CompressOptions::default()
    };
    if let Some(v) = o.get("quality").and_then(|v| v.as_u64()) {
        opts.quality = v as u8;
    }
    if let Some(v) = o.get("format").and_then(|v| v.as_str()) {
        opts.format = Some(parse_format(v, id));
    }
    if let Some(v) = o.get("effort").and_then(|v| v.as_u64()) {
        opts.effort = v as u8;
    }
    opts
}

fn collage_options(o: &Json, id: &str) -> CollageOptions {
    let mut opts = CollageOptions {
        common: common_options(o, id),
        ..CollageOptions::default()
    };
    if let Some(v) = o.get("layout") {
        opts.layout = parse_layout(v, id);
    }
    if let Some(v) = o.get("gap").and_then(|v| v.as_u64()) {
        opts.gap = v as u32;
    }
    if let Some(v) = o.get("background") {
        opts.background = parse_colour(v, id);
    }
    if let Some(v) = o.get("width").and_then(|v| v.as_u64()) {
        opts.width = v as u32;
    }
    if let Some(v) = o.get("height").and_then(|v| v.as_u64()) {
        opts.height = v as u32;
    }
    if let Some(v) = o.get("cell_fit").and_then(|v| v.as_str()) {
        opts.cell_fit = parse_fit(v, id);
    }
    opts
}

// ---------------------------------------------------------------------------
// Running one case
// ---------------------------------------------------------------------------

/// Run one case. Returns the outcome, or the core error, plus the
/// single input (or `None` for collage/bulk, which take a list);
/// callers keep the input for pixel comparisons.
fn run_case(case: &Json, id: &str) -> (Result<Outcome, DusrelError>, Option<Vec<u8>>) {
    let op = case["op"]
        .as_str()
        .unwrap_or_else(|| panic!("case {id}: no op"));
    let options = case.get("options").cloned().unwrap_or(Json::Null);
    let o = &options;
    let inputs: Vec<Vec<u8>> = if let Some(list) = case.get("fixtures").and_then(|v| v.as_array()) {
        list.iter().map(|f| fixture_bytes(f, id)).collect()
    } else if case.get("fixture").is_some() {
        vec![fixture_bytes(&case["fixture"], id)]
    } else {
        vec![]
    };
    let input = inputs.first().cloned();
    let result = match op {
        "get_image_info" => {
            let bytes = inputs
                .first()
                .unwrap_or_else(|| panic!("case {id}: get_image_info needs one fixture"));
            get_image_info(bytes, &common_options(o, id)).map(Outcome::Info)
        }
        "compress_image" => {
            let bytes = inputs
                .first()
                .unwrap_or_else(|| panic!("case {id}: compress_image needs one fixture"));
            compress_image(bytes, &compress_options(o, id)).map(Outcome::Image)
        }
        "compress_to_size" => {
            let bytes = inputs
                .first()
                .unwrap_or_else(|| panic!("case {id}: compress_to_size needs one fixture"));
            let target = case["target_bytes"]
                .as_u64()
                .unwrap_or_else(|| panic!("case {id}: compress_to_size needs target_bytes"));
            compress_to_size(bytes, target, &compress_options(o, id)).map(Outcome::Image)
        }
        "resize_image" => {
            let bytes = inputs
                .first()
                .unwrap_or_else(|| panic!("case {id}: resize_image needs one fixture"));
            let mut opts = ResizeOptions {
                common: common_options(o, id),
                ..ResizeOptions::default()
            };
            if let Some(v) = o.get("width").and_then(|v| v.as_u64()) {
                opts.width = Some(v as u32);
            }
            if let Some(v) = o.get("height").and_then(|v| v.as_u64()) {
                opts.height = Some(v as u32);
            }
            if let Some(v) = o.get("percent").and_then(|v| v.as_f64()) {
                opts.percent = Some(v as f32);
            }
            if let Some(v) = o.get("fit").and_then(|v| v.as_str()) {
                opts.fit = parse_fit(v, id);
            }
            if let Some(v) = o.get("filter").and_then(|v| v.as_str()) {
                opts.filter = match v {
                    "Nearest" => Filter::Nearest,
                    "Triangle" => Filter::Triangle,
                    "CatmullRom" => Filter::CatmullRom,
                    "Gaussian" => Filter::Gaussian,
                    "Lanczos3" => Filter::Lanczos3,
                    other => panic!("case {id}: unknown filter {other:?}"),
                };
            }
            if let Some(v) = o.get("background") {
                opts.background = parse_colour(v, id);
            }
            resize_image(bytes, &opts).map(Outcome::Image)
        }
        "crop_image" => {
            let bytes = inputs
                .first()
                .unwrap_or_else(|| panic!("case {id}: crop_image needs one fixture"));
            let opts = CropOptions {
                common: common_options(o, id),
                x: o.get("x").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                y: o.get("y").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                width: u32_of(&o["width"], id, "crop width"),
                height: u32_of(&o["height"], id, "crop height"),
                anchor: o
                    .get("anchor")
                    .and_then(|v| v.as_str())
                    .map(|s| parse_position(s, id)),
            };
            crop_image(bytes, &opts).map(Outcome::Image)
        }
        "convert_image" => {
            let bytes = inputs
                .first()
                .unwrap_or_else(|| panic!("case {id}: convert_image needs one fixture"));
            let mut opts = ConvertOptions {
                common: common_options(o, id),
                ..ConvertOptions::default()
            };
            opts.format = parse_format(
                o["format"]
                    .as_str()
                    .unwrap_or_else(|| panic!("case {id}: convert_image needs a format")),
                id,
            );
            if let Some(v) = o.get("quality").and_then(|v| v.as_u64()) {
                opts.quality = v as u8;
            }
            if let Some(v) = o.get("background") {
                opts.background = parse_colour(v, id);
            }
            convert_image(bytes, &opts).map(Outcome::Image)
        }
        "add_watermark" => {
            let bytes = inputs
                .first()
                .unwrap_or_else(|| panic!("case {id}: add_watermark needs one fixture"));
            add_watermark(bytes, &watermark_options(o, id)).map(Outcome::Image)
        }
        "annotate_image" => {
            let bytes = inputs
                .first()
                .unwrap_or_else(|| panic!("case {id}: annotate_image needs one fixture"));
            annotate_image(bytes, &parse_annotations(case, id)).map(Outcome::Image)
        }
        "pick_colour" => {
            let bytes = inputs
                .first()
                .unwrap_or_else(|| panic!("case {id}: pick_colour needs one fixture"));
            let x = u32_of(&o["x"], id, "pick x");
            let y = u32_of(&o["y"], id, "pick y");
            pick_colour(bytes, x, y).map(Outcome::Colour)
        }
        "extract_palette" => {
            let bytes = inputs
                .first()
                .unwrap_or_else(|| panic!("case {id}: extract_palette needs one fixture"));
            let count = o
                .get("count")
                .and_then(|v| v.as_u64())
                .unwrap_or_else(|| panic!("case {id}: extract_palette needs a count"))
                as u8;
            extract_palette(bytes, count).map(Outcome::Palette)
        }
        "make_collage" => make_collage(&inputs, &collage_options(o, id)).map(Outcome::Image),
        "pixelate_image" => {
            let bytes = inputs
                .first()
                .unwrap_or_else(|| panic!("case {id}: pixelate_image needs one fixture"));
            let mut opts = PixelateOptions {
                common: common_options(o, id),
                ..PixelateOptions::default()
            };
            if let Some(v) = o.get("x").and_then(|v| v.as_u64()) {
                opts.x = v as u32;
            }
            if let Some(v) = o.get("y").and_then(|v| v.as_u64()) {
                opts.y = v as u32;
            }
            if let Some(v) = o.get("width").and_then(|v| v.as_u64()) {
                opts.width = v as u32;
            }
            if let Some(v) = o.get("height").and_then(|v| v.as_u64()) {
                opts.height = v as u32;
            }
            if let Some(v) = o.get("block_size").and_then(|v| v.as_u64()) {
                opts.block_size = v as u32;
            }
            pixelate_image(bytes, &opts).map(Outcome::Image)
        }
        "bulk_process" => {
            let bulk_op_name = case["bulk_op"]
                .as_str()
                .unwrap_or_else(|| panic!("case {id}: bulk_process needs bulk_op"));
            let bulk_op = match bulk_op_name {
                "Pixelate" => BulkOp::Pixelate({
                    let mut opts = PixelateOptions {
                        common: common_options(o, id),
                        ..PixelateOptions::default()
                    };
                    if let Some(v) = o.get("block_size").and_then(|v| v.as_u64()) {
                        opts.block_size = v as u32;
                    }
                    opts
                }),
                "PickColour" => BulkOp::PickColour {
                    x: u32_of(&o["x"], id, "bulk pick x"),
                    y: u32_of(&o["y"], id, "bulk pick y"),
                },
                "Palette" => BulkOp::Palette {
                    count: o
                        .get("count")
                        .and_then(|v| v.as_u64())
                        .unwrap_or_else(|| panic!("case {id}: bulk palette needs a count"))
                        as u8,
                },
                "Collage" => BulkOp::Collage(collage_options(o, id)),
                "Watermark" => BulkOp::Watermark(watermark_options(o, id)),
                "Annotate" => BulkOp::Annotate(parse_annotations(case, id)),
                other => panic!("case {id}: unknown bulk_op {other:?} — update this runner"),
            };
            Ok(Outcome::Bulk(bulk_process(&inputs, &bulk_op)))
        }
        other => panic!("case {id}: unknown op {other:?} — update this runner"),
    };
    (result, input)
}

// ---------------------------------------------------------------------------
// Assertions on the expected object
// ---------------------------------------------------------------------------

fn expect_u32(expected: &Json, key: &str, actual: u32, id: &str, failures: &mut Vec<String>) {
    if let Some(v) = expected.get(key).and_then(|v| v.as_u64()) {
        if actual as u64 != v {
            failures.push(format!("case {id}: expected {key}={v}, got {actual}"));
        }
    }
}

fn check_pixel_assert(px: [u8; 4], spec: &Json, id: &str, failures: &mut Vec<String>) {
    let (x, y) = (
        u32_of(&spec["x"], id, "pixel x"),
        u32_of(&spec["y"], id, "pixel y"),
    );
    if let Some(rgba) = spec.get("rgba").and_then(|v| v.as_array()) {
        let want: Vec<u8> = rgba.iter().map(|v| u_of(v, id, "rgba channel")).collect();
        if px != [want[0], want[1], want[2], want[3]] {
            failures.push(format!(
                "case {id}: pixel ({x},{y}) expected rgba {want:?}, got {px:?}"
            ));
        }
    }
    if let Some(approx) = spec.get("rgba_approx").and_then(|v| v.as_array()) {
        let tol = spec.get("tolerance").and_then(|v| v.as_u64()).unwrap_or(0) as u8;
        let want: Vec<u8> = approx.iter().map(|v| u_of(v, id, "rgba_approx")).collect();
        for (i, w) in want.iter().enumerate() {
            if px[i].abs_diff(*w) > tol {
                failures.push(format!(
                    "case {id}: pixel ({x},{y}) channel {i} expected ~{w}±{tol}, got {}",
                    px[i]
                ));
            }
        }
    }
    for (key, ch) in [("r", 0usize), ("g", 1), ("b", 2), ("a", 3)] {
        if let Some(v) = spec.get(key).and_then(|v| v.as_u64()) {
            if px[ch] as u64 != v {
                failures.push(format!(
                    "case {id}: pixel ({x},{y}) channel {key} expected {v}, got {}",
                    px[ch]
                ));
            }
        }
    }
    if let Some(v) = spec.get("r_max").and_then(|v| v.as_u64()) {
        if px[0] as u64 > v {
            failures.push(format!(
                "case {id}: pixel ({x},{y}) r expected <= {v}, got {}",
                px[0]
            ));
        }
    }
}

fn assert_expected(
    case: &Json,
    id: &str,
    result: Result<Outcome, DusrelError>,
    input: &Option<Vec<u8>>,
    failures: &mut Vec<String>,
) {
    let expected = &case["expected"];
    if let Some(err) = expected.get("error").and_then(|v| v.as_str()) {
        match result {
            Err(e) if e.code_str() == err => {}
            Err(e) => failures.push(format!(
                "case {id}: expected error {err}, got {} ({})",
                e.code_str(),
                e.message_en
            )),
            Ok(_) => failures.push(format!("case {id}: expected error {err}, got success")),
        }
        return;
    }
    let outcome = match result {
        Ok(o) => o,
        Err(e) => {
            failures.push(format!(
                "case {id}: expected success, got {} ({})",
                e.code_str(),
                e.message_en
            ));
            return;
        }
    };

    // Image outcomes: decode once for dims/pixels; info outcomes use
    // the struct directly.
    let (info, img) = match &outcome {
        Outcome::Image(bytes) => {
            // AVIF output cannot be decoded back in this build (no
            // avif-native decoder): a case that only asserts the
            // header/size may skip the decode; anything asserting
            // dims/pixels must be decodable.
            match get_image_info(bytes, &CommonOptions::default()) {
                Ok(info) => (Some(info), Some(decode_rgba(bytes))),
                Err(e) => {
                    let needs_decode = [
                        "format",
                        "width",
                        "height",
                        "has_alpha",
                        "colour_type",
                        "pixels",
                        "region_all_white",
                        "region_has_dark",
                        "regions_have_dark",
                        "differs_from_input",
                        "pixel_equalities",
                        "unchanged_pixels",
                    ]
                    .iter()
                    .any(|k| expected.get(k).is_some());
                    if needs_decode {
                        panic!("case {id}: output failed get_image_info: {e}");
                    }
                    (None, None)
                }
            }
        }
        Outcome::Info(info) => (Some(info.clone()), None),
        _ => (None, None),
    };
    if let Some(info) = &info {
        if let Some(v) = expected.get("format").and_then(|v| v.as_str()) {
            if info.format != v {
                failures.push(format!(
                    "case {id}: expected format {v}, got {}",
                    info.format
                ));
            }
        }
        expect_u32(expected, "width", info.width, id, failures);
        expect_u32(expected, "height", info.height, id, failures);
        if let Some(v) = expected.get("has_alpha").and_then(|v| v.as_bool()) {
            if info.has_alpha != v {
                failures.push(format!(
                    "case {id}: expected has_alpha={v}, got {}",
                    info.has_alpha
                ));
            }
        }
        if let Some(v) = expected.get("colour_type").and_then(|v| v.as_str()) {
            if info.colour_type != v {
                failures.push(format!(
                    "case {id}: expected colour_type {v}, got {}",
                    info.colour_type
                ));
            }
        }
    }
    if let Outcome::Image(bytes) = &outcome {
        if let Some(v) = expected.get("max_bytes").and_then(|v| v.as_u64()) {
            if bytes.len() as u64 > v {
                failures.push(format!(
                    "case {id}: expected <= {v} bytes, got {}",
                    bytes.len()
                ));
            }
        }
        if expected
            .get("avif_header")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
            && (bytes.len() < 12 || &bytes[4..12] != b"ftypavif")
        {
            failures.push(format!("case {id}: expected AVIF ftypavif header"));
        }
    }
    if let (Some(img), Some(inp)) = (&img, input) {
        if let Some(points) = expected
            .get("differs_from_input")
            .and_then(|v| v.as_array())
        {
            let in_img = decode_rgba(inp);
            for p in points {
                let arr = p.as_array().unwrap_or_else(|| {
                    panic!("case {id}: differs_from_input entries must be [x, y]")
                });
                let (x, y) = (u32_of(&arr[0], id, "x"), u32_of(&arr[1], id, "y"));
                if img.get_pixel(x, y) == in_img.get_pixel(x, y) {
                    failures.push(format!(
                        "case {id}: pixel ({x},{y}) should differ from the input"
                    ));
                }
            }
        }
        if let Some(pairs) = expected.get("pixel_equalities").and_then(|v| v.as_array()) {
            for pair in pairs {
                let a = pair["a"].as_array().unwrap();
                let b = pair["b"].as_array().unwrap();
                let (ax, ay) = (u32_of(&a[0], id, "x"), u32_of(&a[1], id, "y"));
                let (bx, by) = (u32_of(&b[0], id, "x"), u32_of(&b[1], id, "y"));
                let equal = pair["equal"].as_bool().unwrap_or_else(|| {
                    panic!("case {id}: pixel_equalities entries need an equal bool")
                });
                let same = img.get_pixel(ax, ay) == img.get_pixel(bx, by);
                if same != equal {
                    failures.push(format!(
                        "case {id}: pixels ({ax},{ay}) vs ({bx},{by}) equal={same}, expected {equal}"
                    ));
                }
            }
        }
        if let Some(points) = expected.get("unchanged_pixels").and_then(|v| v.as_array()) {
            let in_img = decode_rgba(inp);
            for p in points {
                let arr = p.as_array().unwrap_or_else(|| {
                    panic!("case {id}: unchanged_pixels entries must be [x, y]")
                });
                let (x, y) = (u32_of(&arr[0], id, "x"), u32_of(&arr[1], id, "y"));
                if img.get_pixel(x, y) != in_img.get_pixel(x, y) {
                    failures.push(format!(
                        "case {id}: pixel ({x},{y}) should be unchanged from the input"
                    ));
                }
            }
        }
    }
    if let Some(img) = &img {
        if let Some(pixels) = expected.get("pixels").and_then(|v| v.as_array()) {
            for spec in pixels {
                let (x, y) = (
                    u32_of(&spec["x"], id, "pixel x"),
                    u32_of(&spec["y"], id, "pixel y"),
                );
                check_pixel_assert(img.get_pixel(x, y).0, spec, id, failures);
            }
        }
        for key in ["region_all_white", "region_has_dark"] {
            if let Some(spec) = expected.get(key) {
                let (x0, y0, x1, y1) = (
                    u32_of(&spec["x0"], id, "region x0"),
                    u32_of(&spec["y0"], id, "region y0"),
                    u32_of(&spec["x1"], id, "region x1"),
                    u32_of(&spec["y1"], id, "region y1"),
                );
                if key == "region_all_white" && !region_all_white(img, x0, y0, x1, y1) {
                    failures.push(format!(
                        "case {id}: region ({x0},{y0})-({x1},{y1}) should be all white"
                    ));
                }
                if key == "region_has_dark" && !region_has_dark(img, x0, y0, x1, y1) {
                    failures.push(format!(
                        "case {id}: region ({x0},{y0})-({x1},{y1}) should contain dark pixels"
                    ));
                }
            }
        }
        if let Some(regions) = expected.get("regions_have_dark").and_then(|v| v.as_array()) {
            for spec in regions {
                let (x0, y0, x1, y1) = (
                    u32_of(&spec["x0"], id, "region x0"),
                    u32_of(&spec["y0"], id, "region y0"),
                    u32_of(&spec["x1"], id, "region x1"),
                    u32_of(&spec["y1"], id, "region y1"),
                );
                if !region_has_dark(img, x0, y0, x1, y1) {
                    failures.push(format!(
                        "case {id}: region ({x0},{y0})-({x1},{y1}) should contain dark pixels"
                    ));
                }
            }
        }
    }
    if let Outcome::Colour(colour) = &outcome {
        if let Some(c) = expected.get("colour") {
            if let Some(v) = c.get("hex").and_then(|v| v.as_str()) {
                if colour.hex != v {
                    failures.push(format!(
                        "case {id}: expected colour hex {v}, got {}",
                        colour.hex
                    ));
                }
            }
            expect_u32(c, "r", u32::from(colour.r), id, failures);
            expect_u32(c, "g", u32::from(colour.g), id, failures);
            expect_u32(c, "b", u32::from(colour.b), id, failures);
            expect_u32(c, "a", u32::from(colour.a), id, failures);
        }
        if let Some(other) = expected.get("different_from") {
            let (x, y) = (
                u32_of(&other["x"], id, "different_from x"),
                u32_of(&other["y"], id, "different_from y"),
            );
            let bytes = input
                .as_ref()
                .unwrap_or_else(|| panic!("case {id}: different_from needs one fixture"));
            let other_colour = pick_colour(bytes, x, y)
                .unwrap_or_else(|e| panic!("case {id}: different_from pick failed: {e}"));
            if other_colour.hex == colour.hex {
                failures.push(format!(
                    "case {id}: colour should differ from the pixel at ({x},{y})"
                ));
            }
        }
    }
    if let Outcome::Palette(palette) = &outcome {
        if let Some(v) = expected.get("palette_length").and_then(|v| v.as_u64()) {
            if palette.len() as u64 != v {
                failures.push(format!(
                    "case {id}: expected palette length {v}, got {}",
                    palette.len()
                ));
            }
        }
        if let Some(hexes) = expected.get("palette_hexes").and_then(|v| v.as_array()) {
            // Compared as a set: the contract orders by frequency
            // ("most frequent first") and these two shares are equal,
            // so the tie order is unspecified — the core breaks ties
            // by hex ascending. Shares are asserted separately.
            let mut got: Vec<&str> = palette.iter().map(|p| p.colour.hex.as_str()).collect();
            let mut want: Vec<&str> = hexes
                .iter()
                .map(|v| {
                    v.as_str()
                        .unwrap_or_else(|| panic!("case {id}: palette_hexes must be strings"))
                })
                .collect();
            got.sort_unstable();
            want.sort_unstable();
            if got != want {
                failures.push(format!(
                    "case {id}: expected palette hexes {want:?}, got {got:?}"
                ));
            }
        }
        let share_tol = expected
            .get("share_tolerance")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        if let Some(v) = expected.get("share_each").and_then(|v| v.as_f64()) {
            for entry in palette {
                if (entry.share as f64 - v).abs() > share_tol {
                    failures.push(format!(
                        "case {id}: palette {} share expected {v}±{share_tol}, got {}",
                        entry.colour.hex, entry.share
                    ));
                }
            }
        }
        if let Some(v) = expected.get("share_sum").and_then(|v| v.as_f64()) {
            let tol = expected
                .get("share_sum_tolerance")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let sum: f64 = palette.iter().map(|p| p.share as f64).sum();
            if (sum - v).abs() > tol {
                failures.push(format!(
                    "case {id}: palette share sum expected {v}±{tol}, got {sum}"
                ));
            }
        }
        if let Some(entries) = expected.get("palette").and_then(|v| v.as_array()) {
            if entries.len() != palette.len() {
                failures.push(format!(
                    "case {id}: expected palette of {}, got {}",
                    entries.len(),
                    palette.len()
                ));
            } else {
                for (want, got) in entries.iter().zip(palette.iter()) {
                    if let Some(v) = want.get("hex").and_then(|v| v.as_str()) {
                        if got.colour.hex != v {
                            failures.push(format!(
                                "case {id}: expected palette hex {v}, got {}",
                                got.colour.hex
                            ));
                        }
                    }
                    if let Some(v) = want.get("share").and_then(|v| v.as_f64()) {
                        let tol = want
                            .get("share_tolerance")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(0.0);
                        if (got.share as f64 - v).abs() > tol {
                            failures.push(format!(
                                "case {id}: palette {} share expected {v}±{tol}, got {}",
                                got.colour.hex, got.share
                            ));
                        }
                    }
                }
            }
        }
    }
    if let Outcome::Bulk(results) = &outcome {
        let want = expected
            .get("results")
            .and_then(|v| v.as_array())
            .unwrap_or_else(|| panic!("case {id}: bulk case needs expected.results"));
        if results.len() != want.len() {
            failures.push(format!(
                "case {id}: expected {} bulk results, got {}",
                want.len(),
                results.len()
            ));
            return;
        }
        for (got, want_item) in results.iter().zip(want.iter()) {
            let idx = want_item
                .get("index")
                .and_then(|v| v.as_u64())
                .unwrap_or(got.index as u64) as usize;
            if got.index != idx {
                failures.push(format!(
                    "case {id}: bulk item index expected {idx}, got {}",
                    got.index
                ));
            }
            let want_ok = want_item
                .get("ok")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            if got.is_ok() != want_ok {
                failures.push(format!(
                    "case {id}: bulk item {idx} ok expected {want_ok}, got {}",
                    got.is_ok()
                ));
                continue;
            }
            match &got.result {
                Ok(BulkOutput::Image(bytes)) => {
                    if let Some(v) = want_item.get("output").and_then(|v| v.as_str()) {
                        if v != "Image" {
                            failures.push(format!(
                                "case {id}: bulk item {idx} output expected {v}, got Image"
                            ));
                        }
                    }
                    let info =
                        get_image_info(bytes, &CommonOptions::default()).unwrap_or_else(|e| {
                            panic!("case {id}: bulk item {idx} output not decodable: {e}")
                        });
                    expect_u32(want_item, "width", info.width, id, failures);
                    expect_u32(want_item, "height", info.height, id, failures);
                }
                Ok(BulkOutput::Colour(colour)) => {
                    if let Some(v) = want_item.get("output").and_then(|v| v.as_str()) {
                        if v != "Colour" {
                            failures.push(format!(
                                "case {id}: bulk item {idx} output expected {v}, got Colour"
                            ));
                        }
                    }
                    if let Some(c) = want_item.get("colour") {
                        if let Some(v) = c.get("hex").and_then(|v| v.as_str()) {
                            if colour.hex != v {
                                failures.push(format!(
                                    "case {id}: bulk item {idx} colour hex expected {v}, got {}",
                                    colour.hex
                                ));
                            }
                        }
                    }
                }
                Ok(BulkOutput::Palette(palette)) => {
                    if let Some(v) = want_item.get("output").and_then(|v| v.as_str()) {
                        if v != "Palette" {
                            failures.push(format!(
                                "case {id}: bulk item {idx} output expected {v}, got Palette"
                            ));
                        }
                    }
                    if let Some(v) = want_item.get("palette_length").and_then(|v| v.as_u64()) {
                        if palette.len() as u64 != v {
                            failures.push(format!(
                                "case {id}: bulk item {idx} palette length expected {v}, got {}",
                                palette.len()
                            ));
                        }
                    }
                    if let Some(entries) = want_item.get("palette").and_then(|v| v.as_array()) {
                        for (want_entry, got_entry) in entries.iter().zip(palette.iter()) {
                            if let Some(v) = want_entry.get("hex").and_then(|v| v.as_str()) {
                                if got_entry.colour.hex != v {
                                    failures.push(format!(
                                        "case {id}: bulk item {idx} palette hex expected {v}, got {}",
                                        got_entry.colour.hex
                                    ));
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    if let Some(v) = want_item.get("error").and_then(|v| v.as_str()) {
                        if e.code_str() != v {
                            failures.push(format!(
                                "case {id}: bulk item {idx} error expected {v}, got {}",
                                e.code_str()
                            ));
                        }
                    } else {
                        failures.push(format!(
                            "case {id}: bulk item {idx} failed unexpectedly: {}",
                            e.code_str()
                        ));
                    }
                }
            }
        }
    }
    // Any expected key this runner did not look at is a schema gap:
    // fail loudly instead of silently ignoring the case's contract.
    const KNOWN_KEYS: &[&str] = &[
        "format",
        "width",
        "height",
        "has_alpha",
        "colour_type",
        "error",
        "max_bytes",
        "avif_header",
        "pixels",
        "region_all_white",
        "region_has_dark",
        "regions_have_dark",
        "colour",
        "different_from",
        "palette_hexes",
        "palette_length",
        "share_each",
        "share_sum",
        "share_sum_tolerance",
        "share_tolerance",
        "palette",
        "differs_from_input",
        "pixel_equalities",
        "unchanged_pixels",
        "results",
    ];
    if let Some(obj) = expected.as_object() {
        for key in obj.keys() {
            if !KNOWN_KEYS.contains(&key.as_str()) {
                failures.push(format!(
                    "case {id}: unhandled expected key {key:?} — update this runner"
                ));
            }
        }
    }
}

#[test]
fn shared_test_vectors() {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let text = std::fs::read_to_string(format!("{manifest}/../../shared/test-vectors.json"))
        .expect("shared/test-vectors.json must exist next to the repo");
    let cases: Vec<Json> =
        serde_json::from_str(&text).expect("shared/test-vectors.json must be valid JSON");
    assert_eq!(
        cases.len(),
        82,
        "shared/test-vectors.json case count changed — update this runner's count assertion"
    );
    let mut failures: Vec<String> = Vec::new();
    for case in &cases {
        let id = case["id"]
            .as_str()
            .unwrap_or_else(|| panic!("a test-vector case has no string id: {case:?}"));
        let (result, input) = run_case(case, id);
        assert_expected(case, id, result, &input, &mut failures);
    }
    assert!(
        failures.is_empty(),
        "shared test vectors: {} failure(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
}
