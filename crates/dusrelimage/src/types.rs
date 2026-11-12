//! Shared value types: formats, colours, image info, fit/filter modes,
//! positions, collage layouts, palette entries and annotations.
//!
//! These types are the contract the CLI / Python / JS bindings build on,
//! so field names and variant shapes are stable API — do not rename
//! without a SPEC change.

use crate::error::{DusrelError, Result};

/// Output image format (SPEC section 1, tool 4).
///
/// This is the *output* set. Input decoding additionally accepts TIFF
/// (SPEC section 3) — TIFF has no variant here on purpose: it cannot be
/// an output format, so a TIFF input is reported through
/// [`DecodedImage::format`] as `None` (see `common`) and through
/// [`ImageInfo::format`] as the string `"tiff"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ImageFormat {
    Jpeg,
    Png,
    WebP,
    Avif,
    Gif,
    Bmp,
}

impl ImageFormat {
    /// Lowercase short name, e.g. `"jpeg"`. Also used for `ImageInfo.format`.
    pub fn as_str(&self) -> &'static str {
        match self {
            ImageFormat::Jpeg => "jpeg",
            ImageFormat::Png => "png",
            ImageFormat::WebP => "webp",
            ImageFormat::Avif => "avif",
            ImageFormat::Gif => "gif",
            ImageFormat::Bmp => "bmp",
        }
    }

    /// Usual file extension (no dot).
    pub fn file_extension(&self) -> &'static str {
        match self {
            ImageFormat::Jpeg => "jpg",
            ImageFormat::Png => "png",
            ImageFormat::WebP => "webp",
            ImageFormat::Avif => "avif",
            ImageFormat::Gif => "gif",
            ImageFormat::Bmp => "bmp",
        }
    }

    /// MIME type.
    pub fn mime_type(&self) -> &'static str {
        match self {
            ImageFormat::Jpeg => "image/jpeg",
            ImageFormat::Png => "image/png",
            ImageFormat::WebP => "image/webp",
            ImageFormat::Avif => "image/avif",
            ImageFormat::Gif => "image/gif",
            ImageFormat::Bmp => "image/bmp",
        }
    }

    /// True when the format can store an alpha channel.
    pub fn supports_alpha(&self) -> bool {
        match self {
            ImageFormat::Jpeg | ImageFormat::Bmp => false,
            ImageFormat::Png | ImageFormat::WebP | ImageFormat::Avif | ImageFormat::Gif => true,
        }
    }

    /// Map an `image`-crate format onto this enum. Returns `None` for
    /// formats outside the output set (TIFF, PNM, TGA, ...).
    pub fn from_image_format(format: image::ImageFormat) -> Option<Self> {
        use image::ImageFormat as F;
        match format {
            F::Jpeg => Some(ImageFormat::Jpeg),
            F::Png => Some(ImageFormat::Png),
            F::Gif => Some(ImageFormat::Gif),
            F::WebP => Some(ImageFormat::WebP),
            F::Avif => Some(ImageFormat::Avif),
            F::Bmp => Some(ImageFormat::Bmp),
            _ => None,
        }
    }
}

/// A colour: 8-bit RGBA + hex string + HSL.
///
/// `hex` is always `"#RRGGBB"` derived from `r, g, b` (alpha is NOT in
/// the hex string). `hsl` is `(h, s, l)` with `h` in degrees `0..360`
/// and `s, l` in `0..1`.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Colour {
    pub hex: String,
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
    pub hsl: (f32, f32, f32),
}

/// RGB (0-255) -> HSL (h degrees 0..360, s/l 0..1).
fn rgb_to_hsl(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let (r, g, b) = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    if (max - min).abs() < f32::EPSILON {
        return (0.0, 0.0, l);
    }
    let d = max - min;
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };
    let h = if (max - r).abs() < f32::EPSILON {
        60.0 * ((g - b) / d).rem_euclid(6.0)
    } else if (max - g).abs() < f32::EPSILON {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    (h, s, l)
}

/// HSL (h degrees 0..360, s/l 0..1) -> RGB (0-255).
fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
    let h = h.rem_euclid(360.0);
    let s = s.clamp(0.0, 1.0);
    let l = l.clamp(0.0, 1.0);
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0).rem_euclid(2.0) - 1.0).abs());
    let m = l - c / 2.0;
    let (r, g, b) = match h {
        h if h < 60.0 => (c, x, 0.0),
        h if h < 120.0 => (x, c, 0.0),
        h if h < 180.0 => (0.0, c, x),
        h if h < 240.0 => (0.0, x, c),
        h if h < 300.0 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    (
        ((r + m) * 255.0).round() as u8,
        ((g + m) * 255.0).round() as u8,
        ((b + m) * 255.0).round() as u8,
    )
}

impl Colour {
    pub fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        Self::from_rgba(r, g, b, 255)
    }

    pub fn from_rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self {
            hex: format!("#{r:02X}{g:02X}{b:02X}"),
            r,
            g,
            b,
            a,
            hsl: rgb_to_hsl(r, g, b),
        }
    }

    /// Parse `"#RRGGBB"`, `"RRGGBB"`, `"#RGB"` or `"#RRGGBBAA"`.
    pub fn from_hex(hex: &str) -> Result<Self> {
        let h = hex.strip_prefix('#').unwrap_or(hex);
        let bad = || {
            DusrelError::invalid_argument(
                &format!("Invalid hex colour \"{hex}\". Use \"#RRGGBB\"."),
                &format!("Galat hex colour \"{hex}\" hai. \"#RRGGBB\" use karein."),
            )
        };
        let (r, g, b, a) = match h.len() {
            3 => {
                let n = |i: usize| {
                    u8::from_str_radix(&h[i..=i], 16)
                        .map(|v| v * 16 + v)
                        .map_err(|_| bad())
                };
                (n(0)?, n(1)?, n(2)?, 255)
            }
            6 => {
                let n = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).map_err(|_| bad());
                (n(0)?, n(2)?, n(4)?, 255)
            }
            8 => {
                let n = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).map_err(|_| bad());
                (n(0)?, n(2)?, n(4)?, n(6)?)
            }
            _ => return Err(bad()),
        };
        Ok(Self::from_rgba(r, g, b, a))
    }

    pub fn from_hsl(h: f32, s: f32, l: f32) -> Self {
        let (r, g, b) = hsl_to_rgb(h, s, l);
        Self::from_rgb(r, g, b)
    }

    pub fn white() -> Self {
        Self::from_rgb(255, 255, 255)
    }

    pub fn black() -> Self {
        Self::from_rgb(0, 0, 0)
    }

    pub fn to_rgba(&self) -> [u8; 4] {
        [self.r, self.g, self.b, self.a]
    }

    pub fn to_rgb(&self) -> [u8; 3] {
        [self.r, self.g, self.b]
    }
}

impl Default for Colour {
    /// White — the background default for convert / resize-fill / collage.
    fn default() -> Self {
        Self::white()
    }
}

/// Info about an image, from [`crate::common::get_image_info`].
///
/// `format` is a lowercase short name (`"jpeg"`, `"png"`, `"webp"`,
/// `"avif"`, `"gif"`, `"bmp"`, `"tiff"`, ...). `colour_type` is the
/// lowercase `image`-crate colour type (`"rgb8"`, `"rgba8"`, `"luma8"`,
/// ...). Dimensions are post-orientation when `auto_orient` is on.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ImageInfo {
    pub format: String,
    pub width: u32,
    pub height: u32,
    pub has_alpha: bool,
    pub colour_type: String,
    pub file_size: u64,
}

/// How a resize fits the image into the target box (SPEC section 1,
/// tool 2).
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum FitMode {
    /// Scale to fit inside the box, keep aspect ratio. Output size is
    /// the fitted size (no canvas, background unused).
    #[default]
    Fit,
    /// Scale to fit inside the box, then centre it on a canvas of
    /// exactly the target size filled with `ResizeOptions.background`
    /// (letterbox). Output size is exactly the target size.
    Fill,
    /// Scale to exactly the target size, ignoring aspect ratio.
    Stretch,
    /// Scale to cover the box, then centre-crop to exactly the target
    /// size. Output size is exactly the target size.
    Crop,
}

/// Resampling filter (SPEC section 1, tool 2).
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum Filter {
    Nearest,
    Triangle,
    CatmullRom,
    Gaussian,
    #[default]
    Lanczos3,
}

impl Filter {
    pub fn to_image_filter(&self) -> image::imageops::FilterType {
        use image::imageops::FilterType as F;
        match self {
            Filter::Nearest => F::Nearest,
            Filter::Triangle => F::Triangle,
            Filter::CatmullRom => F::CatmullRom,
            Filter::Gaussian => F::Gaussian,
            Filter::Lanczos3 => F::Lanczos3,
        }
    }
}

/// Anchor position: 9 anchors + `Tiled` (watermark only).
///
/// `CropOptions.anchor` reuses this enum (`None` = use explicit x/y;
/// `Tiled` is treated as `Center` for cropping).
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum Position {
    TopLeft,
    TopCenter,
    TopRight,
    CenterLeft,
    #[default]
    Center,
    CenterRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
    Tiled,
}

/// Collage layout (SPEC section 1, tool 9). `masonry` is out of scope
/// for v0.1.0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum CollageLayout {
    /// `rows` x `cols` grid, each 1..=4.
    Grid { rows: u32, cols: u32 },
    /// One row, one column per input image.
    Row,
    /// One column, one row per input image.
    Column,
}

impl Default for CollageLayout {
    fn default() -> Self {
        CollageLayout::Grid { rows: 2, cols: 2 }
    }
}

impl CollageLayout {
    /// Validate a grid layout (rows/cols 1..=4). `Row`/`Column` are
    /// always valid.
    pub fn validate(&self) -> Result<()> {
        if let CollageLayout::Grid { rows, cols } = self {
            if !(1..=4).contains(rows) || !(1..=4).contains(cols) {
                return Err(DusrelError::invalid_argument(
                    &format!("Collage grid must be 1x1 to 4x4, got {rows}x{cols}."),
                    &format!("Collage grid 1x1 se 4x4 tak honi chahiye, {rows}x{cols} mila."),
                ));
            }
        }
        Ok(())
    }

    /// `(rows, cols)` for `count` input images.
    pub fn grid_dimensions(&self, count: usize) -> (u32, u32) {
        match self {
            CollageLayout::Grid { rows, cols } => (*rows, *cols),
            CollageLayout::Row => (1, count as u32),
            CollageLayout::Column => (count as u32, 1),
        }
    }
}

/// One palette entry: a dominant colour and its share of the sampled
/// pixels, `0..1`. Returned by `extract_palette` (tool 8, implemented
/// by the core-draw worker).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PaletteColour {
    pub colour: Colour,
    pub share: f32,
}

/// A text watermark: string, size in px, colour.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TextWatermark {
    pub text: String,
    pub size: f32,
    pub colour: Colour,
}

impl Default for TextWatermark {
    fn default() -> Self {
        Self {
            text: String::new(),
            size: 48.0,
            colour: Colour::white(),
        }
    }
}

/// A logo watermark: raw image bytes + a scale factor
/// (`1.0` = natural size, `0.5` = half size).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ImageWatermark {
    pub image: Vec<u8>,
    pub scale: f32,
}

impl Default for ImageWatermark {
    fn default() -> Self {
        Self {
            image: Vec::new(),
            scale: 1.0,
        }
    }
}

/// One annotation shape (SPEC section 1, tool 6; implemented by the
/// core-draw worker). All coordinates are absolute pixels. `fill` is
/// `None` for outline-only, `Some(colour)` for a filled shape.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Annotation {
    Arrow {
        from: (u32, u32),
        to: (u32, u32),
        colour: Colour,
        width: u32,
    },
    Rect {
        x: u32,
        y: u32,
        w: u32,
        h: u32,
        colour: Colour,
        width: u32,
        fill: Option<Colour>,
    },
    Ellipse {
        x: u32,
        y: u32,
        w: u32,
        h: u32,
        colour: Colour,
        width: u32,
        fill: Option<Colour>,
    },
    Line {
        from: (u32, u32),
        to: (u32, u32),
        colour: Colour,
        width: u32,
    },
    Text {
        x: u32,
        y: u32,
        text: String,
        size: f32,
        colour: Colour,
    },
}
