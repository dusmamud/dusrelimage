//! `dusrelimage` CLI — every core operation, plus `info` and `mock`.
//!
//! Exit code 0 on success, 1 on any error. Errors print the stable
//! error code (`INVALID_IMAGE`, `CROP_OUT_OF_BOUNDS`, …) plus the
//! English and Hindi reasons, and never panic on bad input.
//!
//! All image work is done by the `dusrelimage` core crate; this binary
//! only parses arguments, reads/writes files and prints results.
//! Option structs are built exactly as SPEC section 2 defines them.

use clap::{Parser, Subcommand};
use dusrelimage::fixtures::{MockKind, make_test_image};
use dusrelimage::{
    Annotation, CollageLayout, CollageOptions, Colour, CommonOptions, CompressOptions,
    ConvertOptions, CropOptions, DusrelError, Filter, FitMode, ImageFormat, ImageWatermark,
    PixelateOptions, Position, ResizeOptions, TextWatermark, WatermarkOptions, add_watermark,
    annotate_image, compress_image, compress_to_size, convert_image, crop_image, extract_palette,
    get_image_info, make_collage, pick_colour, pixelate_image, resize_image,
};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Parser)]
#[command(
    name = "dusrelimage",
    version,
    about = "Offline image toolkit — compress, resize, crop, convert, watermark, annotate, pick colours, extract palettes, build collages and pixelate, without uploads."
)]
struct Cli {
    /// Keep EXIF / ICC / XMP metadata in output (default: strip it).
    #[arg(long, global = true)]
    keep_exif: bool,
    /// Do not apply EXIF orientation on load (default: apply it).
    #[arg(long, global = true)]
    no_auto_orient: bool,
    /// Output DPI, e.g. 300 or 300x300 (JPEG/PNG output where supported).
    #[arg(long, global = true, value_name = "DPI")]
    dpi: Option<String>,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Compress an image (quality / format / target size).
    Compress {
        /// Input image file.
        input: PathBuf,
        /// Output image file.
        #[arg(short, long)]
        output: PathBuf,
        /// Quality 1-100 (default 80).
        #[arg(long, default_value_t = 80)]
        quality: u8,
        /// Output format: jpeg, png, webp, avif, gif, bmp (default: keep input format).
        #[arg(long, value_name = "FORMAT")]
        format: Option<String>,
        /// Target output size in bytes (binary-searches quality, max 10 steps).
        #[arg(long, value_name = "BYTES")]
        target_size: Option<u64>,
        /// JPEG progressive / PNG interlaced output.
        #[arg(long)]
        progressive: bool,
        /// Lossless output (WebP/PNG only).
        #[arg(long)]
        lossless: bool,
        /// WebP effort 0-6 (default 4).
        #[arg(long, default_value_t = 4)]
        effort: u8,
    },
    /// Resize an image (width / height / percent, fit mode, filter).
    Resize {
        /// Input image file.
        input: PathBuf,
        /// Output image file.
        #[arg(short, long)]
        output: PathBuf,
        /// Target width in pixels.
        #[arg(long)]
        width: Option<u32>,
        /// Target height in pixels.
        #[arg(long)]
        height: Option<u32>,
        /// Scale by percent, e.g. 50 for half size.
        #[arg(long)]
        percent: Option<f32>,
        /// Fit mode: fit, fill, stretch, crop (default fit).
        #[arg(long, default_value = "fit", value_name = "MODE")]
        fit: String,
        /// Resampling filter: nearest, triangle, catmull-rom, gaussian, lanczos3 (default lanczos3).
        #[arg(long, default_value = "lanczos3", value_name = "FILTER")]
        filter: String,
        /// Background colour as #RRGGBB, used by the fill fit mode (default #FFFFFF).
        #[arg(long, default_value = "#FFFFFF", value_name = "COLOUR")]
        background: String,
    },
    /// Crop a rectangle out of an image.
    Crop {
        /// Input image file.
        input: PathBuf,
        /// Output image file.
        #[arg(short, long)]
        output: PathBuf,
        /// Left edge in pixels (default 0; ignored when --anchor is given).
        #[arg(long, default_value_t = 0)]
        x: u32,
        /// Top edge in pixels (default 0; ignored when --anchor is given).
        #[arg(long, default_value_t = 0)]
        y: u32,
        /// Crop width in pixels.
        #[arg(long)]
        width: u32,
        /// Crop height in pixels.
        #[arg(long)]
        height: u32,
        /// Anchor preset instead of --x/--y: center, top-left, top-right, bottom-left, bottom-right.
        #[arg(long, value_name = "ANCHOR")]
        anchor: Option<String>,
    },
    /// Convert an image to another format.
    Convert {
        /// Input image file.
        input: PathBuf,
        /// Output image file.
        #[arg(short, long)]
        output: PathBuf,
        /// Output format: jpeg, png, webp, avif, gif, bmp.
        #[arg(long, value_name = "FORMAT")]
        format: String,
        /// Quality 1-100 (default 80).
        #[arg(long, default_value_t = 80)]
        quality: u8,
        /// Background colour as #RRGGBB, used to flatten alpha when the target has none (default #FFFFFF).
        #[arg(long, default_value = "#FFFFFF", value_name = "COLOUR")]
        background: String,
        /// JPEG progressive / PNG interlaced output.
        #[arg(long)]
        progressive: bool,
    },
    /// Stamp a text or logo watermark on an image.
    Watermark {
        /// Input image file.
        input: PathBuf,
        /// Output image file.
        #[arg(short, long)]
        output: PathBuf,
        /// Watermark text (text and logo may be combined: text is
        /// drawn first, then the logo).
        #[arg(long)]
        text: Option<String>,
        /// Watermark logo image file (PNG/JPEG).
        #[arg(long)]
        logo: Option<PathBuf>,
        /// Opacity 0.0-1.0 (default 0.5).
        #[arg(long, default_value_t = 0.5)]
        opacity: f32,
        /// Position: top-left, top-center, top-right, center-left, center, center-right, bottom-left, bottom-center, bottom-right, tiled (default center).
        #[arg(long, default_value = "center", value_name = "POSITION")]
        position: String,
        /// Rotation in degrees, clockwise (default 0).
        #[arg(long, default_value_t = 0.0)]
        rotation: f32,
        /// Font size in pixels for text watermarks (default 48).
        #[arg(long, default_value_t = 48.0)]
        font_size: f32,
        /// Text colour as #RRGGBB (default #FFFFFF).
        #[arg(long, default_value = "#FFFFFF", value_name = "COLOUR")]
        colour: String,
        /// Margin in pixels from the image edge (default 16).
        #[arg(long, default_value_t = 16)]
        margin: u32,
        /// Logo scale factor, e.g. 0.5 for half size (default 1).
        #[arg(long, default_value_t = 1.0)]
        scale: f32,
    },
    /// Draw annotations on an image (repeatable flags).
    Annotate {
        /// Input image file.
        input: PathBuf,
        /// Output image file.
        #[arg(short, long)]
        output: PathBuf,
        /// Arrow: 'x1,y1,x2,y2,#RRGGBB,width' (repeatable).
        #[arg(long, value_name = "SPEC")]
        arrow: Vec<String>,
        /// Rectangle: 'x,y,w,h,#RRGGBB,width[,fill #RRGGBB]' (repeatable).
        #[arg(long, value_name = "SPEC")]
        rect: Vec<String>,
        /// Ellipse: 'x,y,w,h,#RRGGBB,width[,fill #RRGGBB]' (repeatable).
        #[arg(long, value_name = "SPEC")]
        ellipse: Vec<String>,
        /// Line: 'x1,y1,x2,y2,#RRGGBB,width' (repeatable).
        #[arg(long, value_name = "SPEC")]
        line: Vec<String>,
        /// Text: 'x,y,size,#RRGGBB,text' — text may contain commas (repeatable).
        #[arg(long, value_name = "SPEC")]
        text: Vec<String>,
    },
    /// Print the colour of one pixel.
    PickColour {
        /// Input image file.
        input: PathBuf,
        /// Pixel x coordinate.
        #[arg(long)]
        x: u32,
        /// Pixel y coordinate.
        #[arg(long)]
        y: u32,
    },
    /// Print the dominant colours of an image, most frequent first.
    Palette {
        /// Input image file.
        input: PathBuf,
        /// Number of colours, 1-16 (default 5).
        #[arg(long, default_value_t = 5)]
        count: u8,
    },
    /// Build a collage from several images.
    Collage {
        /// Input image files, in order.
        inputs: Vec<PathBuf>,
        /// Output image file.
        #[arg(short, long)]
        output: PathBuf,
        /// Layout: grid, row, column, or WxH grid like 2x2, 3x3 (default grid).
        #[arg(long, default_value = "grid", value_name = "LAYOUT")]
        layout: String,
        /// Gap between cells in pixels (default 0).
        #[arg(long, default_value_t = 0)]
        gap: u32,
        /// Background colour as #RRGGBB (default #FFFFFF).
        #[arg(long, default_value = "#FFFFFF", value_name = "COLOUR")]
        background: String,
        /// Output width in pixels (default 1200).
        #[arg(long, default_value_t = 1200)]
        width: u32,
        /// Output height in pixels (default 1200).
        #[arg(long, default_value_t = 1200)]
        height: u32,
        /// Per-cell fit mode: fit, fill, stretch, crop (default fit).
        #[arg(long, default_value = "fit", value_name = "MODE")]
        cell_fit: String,
    },
    /// Pixelate a rectangle of an image (block size 2-100).
    Pixelate {
        /// Input image file.
        input: PathBuf,
        /// Output image file.
        #[arg(short, long)]
        output: PathBuf,
        /// Left edge in pixels (default 0).
        #[arg(long, default_value_t = 0)]
        x: u32,
        /// Top edge in pixels (default 0).
        #[arg(long, default_value_t = 0)]
        y: u32,
        /// Rectangle width in pixels (default: to the right edge).
        #[arg(long)]
        width: Option<u32>,
        /// Rectangle height in pixels (default: to the bottom edge).
        #[arg(long)]
        height: Option<u32>,
        /// Block size in pixels, 2-100 (default 10).
        #[arg(long, default_value_t = 10)]
        block_size: u32,
    },
    /// Print an image's format, dimensions, alpha, colour type and file size.
    Info {
        /// Input image file.
        input: PathBuf,
    },
    /// Generate a deterministic test image (solid / gradient / noise).
    Mock {
        /// Output image file.
        #[arg(short, long)]
        output: PathBuf,
        /// Kind: solid, gradient, noise (default solid).
        #[arg(long, default_value = "solid", value_name = "KIND")]
        kind: String,
        /// Width in pixels (default 256).
        #[arg(long, default_value_t = 256)]
        width: u32,
        /// Height in pixels (default 256).
        #[arg(long, default_value_t = 256)]
        height: u32,
        /// Seed for deterministic output (default 42).
        #[arg(long, default_value_t = 42)]
        seed: u64,
    },
}

fn read(path: &Path) -> Result<Vec<u8>, DusrelError> {
    std::fs::read(path).map_err(|e| {
        DusrelError::invalid_argument(
            &format!("Cannot read {}: {e}", path.display()),
            &format!("{} padh nahi paye: {e}", path.display()),
        )
    })
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), DusrelError> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).map_err(|e| {
            DusrelError::invalid_argument(
                &format!("Cannot create directory {}: {e}", parent.display()),
                &format!("{} directory bana nahi paye: {e}", parent.display()),
            )
        })?;
    }
    std::fs::write(path, bytes).map_err(|e| {
        DusrelError::invalid_argument(
            &format!("Cannot write {}: {e}", path.display()),
            &format!("{} likh nahi paye: {e}", path.display()),
        )
    })
}

fn print_error(err: &DusrelError) -> ExitCode {
    eprintln!("ERROR {}: {}", err.code_str(), err.reason_en());
    eprintln!("({})", err.reason_hi());
    ExitCode::from(1)
}

/// Shared options for every operation that reads an image (SPEC §2).
fn common_options(cli: &Cli) -> Result<CommonOptions, DusrelError> {
    Ok(CommonOptions {
        auto_orient: !cli.no_auto_orient,
        keep_exif: cli.keep_exif,
        dpi: match &cli.dpi {
            Some(d) => Some(parse_dpi(d)?),
            None => None,
        },
        ..Default::default()
    })
}

fn parse_u32(s: &str, what: &str) -> Result<u32, DusrelError> {
    s.trim().parse::<u32>().map_err(|_| {
        DusrelError::invalid_argument(
            &format!("\"{s}\" is not a valid {what} (expected a whole number)."),
            &format!("\"{s}\" valid {what} nahi hai (poora number chahiye)."),
        )
    })
}

fn parse_f32(s: &str, what: &str) -> Result<f32, DusrelError> {
    s.trim().parse::<f32>().map_err(|_| {
        DusrelError::invalid_argument(
            &format!("\"{s}\" is not a valid {what} (expected a number)."),
            &format!("\"{s}\" valid {what} nahi hai (number chahiye)."),
        )
    })
}

/// Parse "300" or "300x300" into (x_dpi, y_dpi).
fn parse_dpi(s: &str) -> Result<(u32, u32), DusrelError> {
    let lower = s.trim().to_ascii_lowercase();
    if let Some((x, y)) = lower.split_once('x') {
        Ok((
            parse_u32(x, "horizontal DPI")?,
            parse_u32(y, "vertical DPI")?,
        ))
    } else {
        let d = parse_u32(&lower, "DPI")?;
        Ok((d, d))
    }
}

/// Standard RGB -> HSL (h in degrees 0-360, s and l in 0-1).
fn rgb_to_hsl(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let r = r as f32 / 255.0;
    let g = g as f32 / 255.0;
    let b = b as f32 / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    let d = max - min;
    if d.abs() < f32::EPSILON {
        return (0.0, 0.0, l);
    }
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };
    let h = if max == r {
        60.0 * ((g - b) / d + if g < b { 6.0 } else { 0.0 })
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    (h, s, l)
}

fn colour_from_rgb(r: u8, g: u8, b: u8, a: u8) -> Colour {
    let (h, s, l) = rgb_to_hsl(r, g, b);
    Colour {
        hex: format!("#{r:02X}{g:02X}{b:02X}"),
        r,
        g,
        b,
        a,
        hsl: (h, s, l),
    }
}

/// Parse #RRGGBB (with or without the leading '#').
fn parse_colour(s: &str) -> Result<Colour, DusrelError> {
    let hex = s.trim().strip_prefix('#').unwrap_or_else(|| s.trim());
    if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(DusrelError::invalid_argument(
            &format!("\"{s}\" is not a colour. Use #RRGGBB, e.g. #FF8800."),
            &format!("\"{s}\" colour nahi hai. #RRGGBB use karein, jaise #FF8800."),
        ));
    }
    let v = u32::from_str_radix(hex, 16).map_err(|_| {
        DusrelError::invalid_argument(
            &format!("\"{s}\" is not a colour. Use #RRGGBB, e.g. #FF8800."),
            &format!("\"{s}\" colour nahi hai. #RRGGBB use karein, jaise #FF8800."),
        )
    })?;
    Ok(colour_from_rgb(
        ((v >> 16) & 0xFF) as u8,
        ((v >> 8) & 0xFF) as u8,
        (v & 0xFF) as u8,
        255,
    ))
}

fn parse_format(s: &str) -> Result<ImageFormat, DusrelError> {
    match s.trim().to_ascii_lowercase().as_str() {
        "jpeg" | "jpg" => Ok(ImageFormat::Jpeg),
        "png" => Ok(ImageFormat::Png),
        "webp" => Ok(ImageFormat::WebP),
        "avif" => Ok(ImageFormat::Avif),
        "gif" => Ok(ImageFormat::Gif),
        "bmp" => Ok(ImageFormat::Bmp),
        _ => Err(DusrelError::invalid_argument(
            &format!("Unknown format \"{s}\". Use jpeg, png, webp, avif, gif or bmp."),
            &format!("Unknown format \"{s}\" hai. jpeg, png, webp, avif, gif ya bmp use karein."),
        )),
    }
}

fn parse_fit(s: &str) -> Result<FitMode, DusrelError> {
    match s.trim().to_ascii_lowercase().as_str() {
        "fit" => Ok(FitMode::Fit),
        "fill" => Ok(FitMode::Fill),
        "stretch" => Ok(FitMode::Stretch),
        "crop" => Ok(FitMode::Crop),
        _ => Err(DusrelError::invalid_argument(
            &format!("Unknown fit mode \"{s}\". Use fit, fill, stretch or crop."),
            &format!("Unknown fit mode \"{s}\" hai. fit, fill, stretch ya crop use karein."),
        )),
    }
}

fn parse_filter(s: &str) -> Result<Filter, DusrelError> {
    match s.trim().to_ascii_lowercase().as_str() {
        "nearest" => Ok(Filter::Nearest),
        "triangle" => Ok(Filter::Triangle),
        "catmull-rom" | "catmullrom" => Ok(Filter::CatmullRom),
        "gaussian" => Ok(Filter::Gaussian),
        "lanczos3" => Ok(Filter::Lanczos3),
        _ => Err(DusrelError::invalid_argument(
            &format!(
                "Unknown filter \"{s}\". Use nearest, triangle, catmull-rom, gaussian or lanczos3."
            ),
            &format!(
                "Unknown filter \"{s}\" hai. nearest, triangle, catmull-rom, gaussian ya lanczos3 use karein."
            ),
        )),
    }
}

/// 9 anchors + tiled (SPEC §2 WatermarkOptions).
fn parse_position(s: &str) -> Result<Position, DusrelError> {
    match s.trim().to_ascii_lowercase().as_str() {
        "top-left" => Ok(Position::TopLeft),
        "top-center" | "top-centre" => Ok(Position::TopCenter),
        "top-right" => Ok(Position::TopRight),
        "center-left" | "centre-left" => Ok(Position::CenterLeft),
        "center" | "centre" => Ok(Position::Center),
        "center-right" | "centre-right" => Ok(Position::CenterRight),
        "bottom-left" => Ok(Position::BottomLeft),
        "bottom-center" | "bottom-centre" => Ok(Position::BottomCenter),
        "bottom-right" => Ok(Position::BottomRight),
        "tiled" => Ok(Position::Tiled),
        _ => Err(DusrelError::invalid_argument(
            "Unknown position. Use top-left, top-center, top-right, center-left, center, center-right, bottom-left, bottom-center, bottom-right or tiled.",
            "Unknown position hai. top-left, top-center, top-right, center-left, center, center-right, bottom-left, bottom-center, bottom-right ya tiled use karein.",
        )),
    }
}

/// Crop anchor presets (SPEC §1: Center | TopLeft | TopRight | BottomLeft | BottomRight).
fn parse_anchor(s: &str) -> Result<Position, DusrelError> {
    match s.trim().to_ascii_lowercase().as_str() {
        "center" | "centre" => Ok(Position::Center),
        "top-left" => Ok(Position::TopLeft),
        "top-right" => Ok(Position::TopRight),
        "bottom-left" => Ok(Position::BottomLeft),
        "bottom-right" => Ok(Position::BottomRight),
        _ => Err(DusrelError::invalid_argument(
            &format!(
                "Unknown anchor \"{s}\". Use center, top-left, top-right, bottom-left or bottom-right."
            ),
            &format!(
                "Unknown anchor \"{s}\" hai. center, top-left, top-right, bottom-left ya bottom-right use karein."
            ),
        )),
    }
}

/// "grid" (auto square from the input count), "row", "column", or "WxH" like "2x2".
fn parse_layout(s: &str, input_count: usize) -> Result<CollageLayout, DusrelError> {
    let lower = s.trim().to_ascii_lowercase();
    match lower.as_str() {
        "grid" => {
            let n = (input_count as f64).sqrt().ceil() as u32;
            Ok(CollageLayout::Grid {
                rows: n.max(1),
                cols: n.max(1),
            })
        }
        "row" => Ok(CollageLayout::Row),
        "column" => Ok(CollageLayout::Column),
        _ => {
            let (r, c) = lower.split_once('x').ok_or_else(|| {
                DusrelError::invalid_argument(
                    &format!(
                        "Unknown layout \"{s}\". Use grid, row, column or a WxH grid like 2x2."
                    ),
                    &format!(
                        "Unknown layout \"{s}\" hai. grid, row, column ya 2x2 jaisa WxH grid use karein."
                    ),
                )
            })?;
            let rows = parse_u32(r, "layout rows")?;
            let cols = parse_u32(c, "layout columns")?;
            if !(1..=4).contains(&rows) || !(1..=4).contains(&cols) {
                return Err(DusrelError::invalid_argument(
                    "Grid layout rows and columns must each be 1-4 (e.g. 2x2).",
                    "Grid layout ki rows aur columns 1-4 ke beech honi chahiye (jaise 2x2).",
                ));
            }
            Ok(CollageLayout::Grid { rows, cols })
        }
    }
}

fn bad_annotation(s: &str, kind: &str, form: &str) -> DusrelError {
    DusrelError::invalid_argument(
        &format!("Bad --{kind} \"{s}\". Use {form}."),
        &format!("Galat --{kind} \"{s}\" hai. {form} use karein."),
    )
}

fn parse_arrow(s: &str) -> Result<Annotation, DusrelError> {
    let p: Vec<&str> = s.split(',').collect();
    if p.len() != 6 {
        return Err(bad_annotation(s, "arrow", "x1,y1,x2,y2,#RRGGBB,width"));
    }
    Ok(Annotation::Arrow {
        from: (parse_u32(p[0], "x1")?, parse_u32(p[1], "y1")?),
        to: (parse_u32(p[2], "x2")?, parse_u32(p[3], "y2")?),
        colour: parse_colour(p[4])?,
        width: parse_u32(p[5], "width")?,
    })
}

fn parse_line(s: &str) -> Result<Annotation, DusrelError> {
    let p: Vec<&str> = s.split(',').collect();
    if p.len() != 6 {
        return Err(bad_annotation(s, "line", "x1,y1,x2,y2,#RRGGBB,width"));
    }
    Ok(Annotation::Line {
        from: (parse_u32(p[0], "x1")?, parse_u32(p[1], "y1")?),
        to: (parse_u32(p[2], "x2")?, parse_u32(p[3], "y2")?),
        colour: parse_colour(p[4])?,
        width: parse_u32(p[5], "width")?,
    })
}

/// Shared shape of --rect / --ellipse: x,y,w,h,#RRGGBB,width[,fill #RRGGBB].
/// (x, y, width, height, colour, line width, optional fill colour).
type BoxParts = (u32, u32, u32, u32, Colour, u32, Option<Colour>);

fn parse_box_parts(s: &str, kind: &str) -> Result<BoxParts, DusrelError> {
    let p: Vec<&str> = s.split(',').collect();
    if p.len() != 6 && p.len() != 7 {
        return Err(bad_annotation(
            s,
            kind,
            "x,y,w,h,#RRGGBB,width[,fill #RRGGBB]",
        ));
    }
    let fill = if p.len() == 7 {
        Some(parse_colour(p[6])?)
    } else {
        None
    };
    Ok((
        parse_u32(p[0], "x")?,
        parse_u32(p[1], "y")?,
        parse_u32(p[2], "width")?,
        parse_u32(p[3], "height")?,
        parse_colour(p[4])?,
        parse_u32(p[5], "line width")?,
        fill,
    ))
}

fn parse_rect(s: &str) -> Result<Annotation, DusrelError> {
    let (x, y, w, h, colour, width, fill) = parse_box_parts(s, "rect")?;
    Ok(Annotation::Rect {
        x,
        y,
        w,
        h,
        colour,
        width,
        fill,
    })
}

fn parse_ellipse(s: &str) -> Result<Annotation, DusrelError> {
    let (x, y, w, h, colour, width, fill) = parse_box_parts(s, "ellipse")?;
    Ok(Annotation::Ellipse {
        x,
        y,
        w,
        h,
        colour,
        width,
        fill,
    })
}

fn parse_text_annotation(s: &str) -> Result<Annotation, DusrelError> {
    // splitn: the text itself may contain commas.
    let p: Vec<&str> = s.splitn(5, ',').collect();
    if p.len() != 5 {
        return Err(bad_annotation(s, "text", "x,y,size,#RRGGBB,text"));
    }
    Ok(Annotation::Text {
        x: parse_u32(p[0], "x")?,
        y: parse_u32(p[1], "y")?,
        text: p[4].to_string(),
        size: parse_f32(p[2], "size")?,
        colour: parse_colour(p[3])?,
    })
}

fn parse_annotations(
    arrows: &[String],
    rects: &[String],
    ellipses: &[String],
    lines: &[String],
    texts: &[String],
) -> Result<Vec<Annotation>, DusrelError> {
    let mut out = Vec::new();
    for s in arrows {
        out.push(parse_arrow(s)?);
    }
    for s in rects {
        out.push(parse_rect(s)?);
    }
    for s in ellipses {
        out.push(parse_ellipse(s)?);
    }
    for s in lines {
        out.push(parse_line(s)?);
    }
    for s in texts {
        out.push(parse_text_annotation(s)?);
    }
    if out.is_empty() {
        return Err(DusrelError::invalid_argument(
            "Annotate needs at least one --arrow, --rect, --ellipse, --line or --text.",
            "Annotate ke liye kam se kam ek --arrow, --rect, --ellipse, --line ya --text chahiye.",
        ));
    }
    Ok(out)
}

fn run(cli: &Cli) -> Result<Option<String>, DusrelError> {
    match &cli.command {
        Commands::Compress {
            input,
            output,
            quality,
            format,
            target_size,
            progressive,
            lossless,
            effort,
        } => {
            let bytes = read(input)?;
            let opts = CompressOptions {
                common: common_options(cli)?,
                quality: *quality,
                format: match format {
                    Some(f) => Some(parse_format(f)?),
                    None => None,
                },
                progressive: *progressive,
                lossless: *lossless,
                effort: *effort,
            };
            let out = match target_size {
                Some(target) => compress_to_size(&bytes, *target, &opts)?,
                None => compress_image(&bytes, &opts)?,
            };
            write(output, &out)?;
            Ok(Some(format!(
                "Compressed {} -> {} ({} bytes)",
                input.display(),
                output.display(),
                out.len()
            )))
        }
        Commands::Resize {
            input,
            output,
            width,
            height,
            percent,
            fit,
            filter,
            background,
        } => {
            if width.is_none() && height.is_none() && percent.is_none() {
                return Err(DusrelError::invalid_argument(
                    "Resize needs --width, --height or --percent.",
                    "Resize ke liye --width, --height ya --percent chahiye.",
                ));
            }
            let bytes = read(input)?;
            let out = resize_image(
                &bytes,
                &ResizeOptions {
                    common: common_options(cli)?,
                    width: *width,
                    height: *height,
                    percent: *percent,
                    fit: parse_fit(fit)?,
                    filter: parse_filter(filter)?,
                    background: parse_colour(background)?,
                },
            )?;
            write(output, &out)?;
            Ok(Some(format!(
                "Resized {} -> {} ({} bytes)",
                input.display(),
                output.display(),
                out.len()
            )))
        }
        Commands::Crop {
            input,
            output,
            x,
            y,
            width,
            height,
            anchor,
        } => {
            let bytes = read(input)?;
            let out = crop_image(
                &bytes,
                &CropOptions {
                    common: common_options(cli)?,
                    x: *x,
                    y: *y,
                    width: *width,
                    height: *height,
                    anchor: match anchor {
                        Some(a) => Some(parse_anchor(a)?),
                        None => None,
                    },
                },
            )?;
            write(output, &out)?;
            Ok(Some(format!(
                "Cropped {} -> {} ({} bytes)",
                input.display(),
                output.display(),
                out.len()
            )))
        }
        Commands::Convert {
            input,
            output,
            format,
            quality,
            background,
            progressive,
        } => {
            let bytes = read(input)?;
            let fmt = parse_format(format)?;
            let out = convert_image(
                &bytes,
                &ConvertOptions {
                    common: common_options(cli)?,
                    format: fmt,
                    quality: *quality,
                    background: parse_colour(background)?,
                    progressive: *progressive,
                },
            )?;
            write(output, &out)?;
            Ok(Some(format!(
                "Converted {} -> {} ({} bytes)",
                input.display(),
                output.display(),
                out.len()
            )))
        }
        Commands::Watermark {
            input,
            output,
            text,
            logo,
            opacity,
            position,
            rotation,
            font_size,
            colour,
            margin,
            scale,
        } => {
            if text.is_none() && logo.is_none() {
                return Err(DusrelError::invalid_argument(
                    "Watermark needs --text or --logo.",
                    "Watermark ke liye --text ya --logo chahiye.",
                ));
            }
            let bytes = read(input)?;
            let text_colour = parse_colour(colour)?;
            let logo_bytes = match logo {
                Some(p) => Some(ImageWatermark {
                    image: read(p)?,
                    scale: *scale,
                }),
                None => None,
            };
            let out = add_watermark(
                &bytes,
                &WatermarkOptions {
                    common: common_options(cli)?,
                    text: text.as_ref().map(|t| TextWatermark {
                        text: t.clone(),
                        size: *font_size,
                        colour: text_colour,
                    }),
                    logo: logo_bytes,
                    opacity: *opacity,
                    position: parse_position(position)?,
                    rotation_deg: *rotation,
                    margin: *margin,
                },
            )?;
            write(output, &out)?;
            Ok(Some(format!(
                "Watermarked {} -> {} ({} bytes)",
                input.display(),
                output.display(),
                out.len()
            )))
        }
        Commands::Annotate {
            input,
            output,
            arrow,
            rect,
            ellipse,
            line,
            text,
        } => {
            let annotations = parse_annotations(arrow, rect, ellipse, line, text)?;
            let bytes = read(input)?;
            let out = annotate_image(&bytes, &annotations)?;
            write(output, &out)?;
            Ok(Some(format!(
                "Annotated {} with {} op(s) -> {} ({} bytes)",
                input.display(),
                annotations.len(),
                output.display(),
                out.len()
            )))
        }
        Commands::PickColour { input, x, y } => {
            let bytes = read(input)?;
            let colour = pick_colour(&bytes, *x, *y)?;
            Ok(Some(format!(
                "{}\nr: {}, g: {}, b: {}, a: {}\nhsl: {:.1}, {:.3}, {:.3}",
                colour.hex,
                colour.r,
                colour.g,
                colour.b,
                colour.a,
                colour.hsl.0,
                colour.hsl.1,
                colour.hsl.2
            )))
        }
        Commands::Palette { input, count } => {
            let bytes = read(input)?;
            let colours = extract_palette(&bytes, *count)?;
            let mut s = String::new();
            for c in &colours {
                s.push_str(&format!("{}  share: {:.4}\n", c.colour.hex, c.share));
            }
            Ok(Some(s.trim_end().to_string()))
        }
        Commands::Collage {
            inputs,
            output,
            layout,
            gap,
            background,
            width,
            height,
            cell_fit,
        } => {
            if inputs.is_empty() {
                return Err(DusrelError::empty_input("collage inputs"));
            }
            let mut bufs = Vec::new();
            for p in inputs {
                bufs.push(read(p)?);
            }
            let out = make_collage(
                &bufs,
                &CollageOptions {
                    common: common_options(cli)?,
                    layout: parse_layout(layout, bufs.len())?,
                    gap: *gap,
                    background: parse_colour(background)?,
                    width: *width,
                    height: *height,
                    cell_fit: parse_fit(cell_fit)?,
                },
            )?;
            write(output, &out)?;
            Ok(Some(format!(
                "Collage of {} images -> {} ({} bytes)",
                inputs.len(),
                output.display(),
                out.len()
            )))
        }
        Commands::Pixelate {
            input,
            output,
            x,
            y,
            width,
            height,
            block_size,
        } => {
            let bytes = read(input)?;
            let common = common_options(cli)?;
            // No --width/--height means "to the edge of the image".
            let (w, h) = match (width, height) {
                (Some(w), Some(h)) => (*w, *h),
                _ => {
                    let info = get_image_info(&bytes, &common)?;
                    (
                        width.unwrap_or_else(|| info.width.saturating_sub(*x)),
                        height.unwrap_or_else(|| info.height.saturating_sub(*y)),
                    )
                }
            };
            let out = pixelate_image(
                &bytes,
                &PixelateOptions {
                    common,
                    x: *x,
                    y: *y,
                    width: w,
                    height: h,
                    block_size: *block_size,
                },
            )?;
            write(output, &out)?;
            Ok(Some(format!(
                "Pixelated {} -> {} ({} bytes)",
                input.display(),
                output.display(),
                out.len()
            )))
        }
        Commands::Info { input } => {
            let bytes = read(input)?;
            let info = get_image_info(&bytes, &common_options(cli)?)?;
            Ok(Some(format!(
                "format: {}\nwidth: {}\nheight: {}\nhas_alpha: {}\ncolour_type: {}\nfile_size: {}",
                info.format,
                info.width,
                info.height,
                info.has_alpha,
                info.colour_type,
                info.file_size
            )))
        }
        Commands::Mock {
            output,
            kind,
            width,
            height,
            seed,
        } => {
            let kind = match kind.trim().to_ascii_lowercase().as_str() {
                "solid" => MockKind::Solid,
                "gradient" => MockKind::Gradient,
                "noise" => MockKind::Noise,
                _ => {
                    return Err(DusrelError::invalid_argument(
                        &format!("Unknown mock kind \"{kind}\". Use solid, gradient or noise."),
                        &format!(
                            "Unknown mock kind \"{kind}\" hai. solid, gradient ya noise use karein."
                        ),
                    ));
                }
            };
            let kind_name = kind_str(&kind);
            let bytes = make_test_image(kind, *width, *height, *seed);
            write(output, &bytes)?;
            Ok(Some(format!(
                "Mock image ({kind_name} {}x{}, seed {}) -> {} ({} bytes)",
                width,
                height,
                seed,
                output.display(),
                bytes.len()
            )))
        }
    }
}

fn kind_str(kind: &MockKind) -> &'static str {
    match kind {
        MockKind::Solid => "solid",
        MockKind::Gradient => "gradient",
        MockKind::Noise => "noise",
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(&cli) {
        Ok(Some(msg)) => {
            println!("{msg}");
            ExitCode::SUCCESS
        }
        Ok(None) => ExitCode::SUCCESS,
        Err(e) => print_error(&e),
    }
}
