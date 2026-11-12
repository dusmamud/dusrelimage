//! Options structs — the pro control layer (SPEC section 2).
//!
//! Every op's options struct embeds `common: CommonOptions` (the
//! dusrelpdf idiom), so size limits and orientation apply uniformly.
//! All structs implement `Default` with the SPEC defaults; anything the
//! SPEC leaves silent is marked "chosen default" in its doc comment.

use crate::types::{
    CollageLayout, Colour, Filter, FitMode, ImageFormat, ImageWatermark, Position, TextWatermark,
};

/// Options shared by every tool.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CommonOptions {
    /// Apply EXIF orientation on load (default true).
    pub auto_orient: bool,
    /// Keep EXIF / ICC / XMP on output (default false = strip).
    ///
    /// Honest v0.1.0 limit: the `image`-crate encoders never copy
    /// metadata, so output is always stripped for now; `true` is
    /// accepted but behaves like `false` until EXIF re-injection lands.
    pub keep_exif: bool,
    /// DPI to write on JPEG/PNG output, where the encoder supports it.
    ///
    /// Honest v0.1.0 limit: the `image` 0.25 encoders expose no DPI
    /// setting, so this is accepted but not embedded yet.
    pub dpi: Option<(u32, u32)>,
    /// Refuse images with more pixels than this (`IMAGE_TOO_LARGE`).
    pub max_pixels: u64,
    /// Refuse images with a side longer than this (`IMAGE_TOO_LARGE`).
    pub max_dimension: u32,
}

impl Default for CommonOptions {
    fn default() -> Self {
        Self {
            auto_orient: true,
            keep_exif: false,
            dpi: None,
            max_pixels: 120_000_000,
            max_dimension: 16000,
        }
    }
}

/// Tool 1 — compressor options.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CompressOptions {
    pub common: CommonOptions,
    /// Quality 1-100 (default 80). JPEG/WebP/AVIF use it directly;
    /// PNG maps it to a compression level (see `common::encode_image`).
    pub quality: u8,
    /// Output format; `None` keeps the input format (TIFF input falls
    /// back to PNG, which is the closest lossless output).
    pub format: Option<ImageFormat>,
    /// Progressive JPEG / interlaced PNG.
    ///
    /// Honest v0.1.0 limit: the `image` 0.25 encoders expose no
    /// progressive/interlace switch, so this is accepted but not
    /// honoured yet.
    pub progressive: bool,
    /// Lossless output (WebP/PNG only; any other format + `true` is
    /// `INVALID_ARGUMENT`).
    pub lossless: bool,
    /// Encoding effort 0-6 (default 4; AVIF speed = 10 - effort).
    pub effort: u8,
}

impl Default for CompressOptions {
    fn default() -> Self {
        Self {
            common: CommonOptions::default(),
            quality: 80,
            format: None,
            progressive: false,
            lossless: false,
            effort: 4,
        }
    }
}

/// Tool 2 — resizer options.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ResizeOptions {
    pub common: CommonOptions,
    /// Target width in px. With `height` unset, height follows the
    /// aspect ratio.
    pub width: Option<u32>,
    /// Target height in px. With `width` unset, width follows the
    /// aspect ratio.
    pub height: Option<u32>,
    /// Scale in percent (`100.0` = original size, `50.0` = half).
    /// Cannot be combined with `width`/`height`.
    pub percent: Option<f32>,
    /// How the image fits the target box.
    pub fit: FitMode,
    /// Resampling filter.
    pub filter: Filter,
    /// Canvas colour used by `FitMode::Fill` (letterbox).
    pub background: Colour,
}

impl Default for ResizeOptions {
    fn default() -> Self {
        Self {
            common: CommonOptions::default(),
            width: None,
            height: None,
            percent: None,
            fit: FitMode::Fit,
            filter: Filter::Lanczos3,
            background: Colour::white(),
        }
    }
}

/// Tool 3 — cropper options.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CropOptions {
    pub common: CommonOptions,
    /// Left edge in px (used when `anchor` is `None`).
    pub x: u32,
    /// Top edge in px (used when `anchor` is `None`).
    pub y: u32,
    /// Crop width in px (must be > 0).
    pub width: u32,
    /// Crop height in px (must be > 0).
    pub height: u32,
    /// Anchor preset: when `Some`, the crop rectangle of
    /// `width` x `height` is placed at the anchor and `x`/`y` are
    /// ignored. `Position::Tiled` is treated as `Position::Center`.
    /// `None` (default) uses the explicit `x`/`y`.
    pub anchor: Option<Position>,
}

impl CropOptions {
    /// Explicit rectangle.
    pub fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
            ..Self::default()
        }
    }

    /// `width` x `height` rectangle placed at an anchor preset.
    pub fn anchored(anchor: Position, width: u32, height: u32) -> Self {
        Self {
            anchor: Some(anchor),
            width,
            height,
            ..Self::default()
        }
    }
}

/// Tool 4 — converter options.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ConvertOptions {
    pub common: CommonOptions,
    /// Target format (required; default is JPEG for `Default` only —
    /// callers should always set it explicitly).
    pub format: ImageFormat,
    /// Quality 1-100 (default 80; JPEG/AVIF direct, PNG level-mapped).
    pub quality: u8,
    /// Background used to flatten alpha when the target has none
    /// (JPEG/BMP). Default white.
    pub background: Colour,
    /// Progressive JPEG / interlaced PNG (accepted, not honoured yet —
    /// see `CompressOptions::progressive`).
    pub progressive: bool,
}

impl Default for ConvertOptions {
    fn default() -> Self {
        Self {
            common: CommonOptions::default(),
            format: ImageFormat::Jpeg,
            quality: 80,
            background: Colour::white(),
            progressive: false,
        }
    }
}

/// Tool 5 — watermark options (implemented by the core-draw worker).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WatermarkOptions {
    pub common: CommonOptions,
    /// Text watermark, if any.
    pub text: Option<TextWatermark>,
    /// Logo watermark (image bytes + scale), if any.
    pub logo: Option<ImageWatermark>,
    /// Opacity 0-1 (chosen default 0.5; SPEC is silent).
    pub opacity: f32,
    /// Anchor position, incl. `Position::Tiled` for a repeated pattern.
    pub position: Position,
    /// Rotation in degrees (chosen default 0.0; SPEC is silent).
    pub rotation_deg: f32,
    /// Margin from the image edge in px (chosen default 16).
    pub margin: u32,
}

impl Default for WatermarkOptions {
    fn default() -> Self {
        Self {
            common: CommonOptions::default(),
            text: None,
            logo: None,
            opacity: 0.5,
            position: Position::BottomRight,
            rotation_deg: 0.0,
            margin: 16,
        }
    }
}

/// Tool 9 — collage options (implemented by the core-draw worker).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CollageOptions {
    pub common: CommonOptions,
    /// Layout (grid 1x1..4x4, row, column).
    pub layout: CollageLayout,
    /// Gap between cells in px (chosen default 8).
    pub gap: u32,
    /// Background colour.
    pub background: Colour,
    /// Output width in px (chosen default 1200).
    pub width: u32,
    /// Output height in px (chosen default 1200).
    pub height: u32,
    /// Per-cell fit mode.
    pub cell_fit: FitMode,
}

impl Default for CollageOptions {
    fn default() -> Self {
        Self {
            common: CommonOptions::default(),
            layout: CollageLayout::default(),
            gap: 8,
            background: Colour::white(),
            width: 1200,
            height: 1200,
            cell_fit: FitMode::Fit,
        }
    }
}

/// Tool 10 — pixelator options (implemented by the core-draw worker).
///
/// `width`/`height` of 0 mean "to the right/bottom edge of the image",
/// so the default (all zeros) pixelates the whole image.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PixelateOptions {
    pub common: CommonOptions,
    /// Left edge in px.
    pub x: u32,
    /// Top edge in px.
    pub y: u32,
    /// Rectangle width in px (0 = to the image edge).
    pub width: u32,
    /// Rectangle height in px (0 = to the image edge).
    pub height: u32,
    /// Block size in px, 2-100 (chosen default 10).
    pub block_size: u32,
}

impl Default for PixelateOptions {
    fn default() -> Self {
        Self {
            common: CommonOptions::default(),
            x: 0,
            y: 0,
            width: 0,
            height: 0,
            block_size: 10,
        }
    }
}
