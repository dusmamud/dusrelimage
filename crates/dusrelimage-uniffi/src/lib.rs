//! UniFFI bindings (Kotlin/Swift) for dusrelimage.
//!
//! Thin translators over the core `dusrelimage` crate — no image logic
//! lives here. The interface is the UDL file next to this one
//! (`src/dusrelimage.udl`); it is the shared contract for the Kotlin
//! and Swift wrappers, so the UDL — not this file — is the source of
//! truth for the cross-language API. Every exported function calls the
//! matching core function and converts the result; every failure
//! surfaces as the UDL `DusrelError` enum with the core's stable code
//! plus English + Hindi messages.
//!
//! The generated Kotlin and Swift sources are checked in — Kotlin
//! under `packages/android/lib/src/main/kotlin/`, Swift under the
//! Swift worker's directory — see the crate README for how to
//! regenerate them.
//!
//! Where the UDL cannot express a Rust type, this file converts
//! between the two shapes (see the UDL header for the full list):
//! * `Colour.hsl`: core `(f32, f32, f32)` tuple <-> UDL
//!   `sequence<f32>` of 3 elements. Inbound, `Colour` is rebuilt
//!   from RGBA (`Colour::from_rgba`), which recomputes `hex`/`hsl`
//!   canonically — a caller-supplied `hex`/`hsl` that disagrees with
//!   the RGBA channels is normalised to the RGBA values.
//! * `CommonOptions.dpi`: core `Option<(u32, u32)>` <-> UDL `Dpi?`.
//! * `Annotation` `from`/`to`: core `(u32, u32)` tuples <-> `Point`.
//! * `BulkOp::CompressToSize` exists only in the UDL (the core
//!   `BulkOp` has no such variant); `bulk_process` implements it per
//!   item by calling `compress_to_size` directly.
//!
//! Honest limits (same as the core, v0.1.0): AVIF input does NOT
//! decode (`UNSUPPORTED_FORMAT`; AVIF output works, lossy 8-bit).
//! WebP output is always lossless. `keep_exif` / `dpi` /
//! `progressive` are accepted but not honoured. `compress_to_size`
//! quality search is JPEG/AVIF-only (other formats take one
//! best-effort encode). TIFF input decodes but falls back to PNG
//! output. GIF output is the first frame only. No HEIC, no streaming.

use dusrelimage::{
    Annotation as CoreAnnotation, BulkOp as CoreBulkOp, BulkOutput as CoreBulkOutput,
    CollageLayout as CoreCollageLayout, Colour as CoreColour, DusrelError as CoreError,
    ErrorCode as CoreErrorCode, Filter as CoreFilter, FitMode as CoreFitMode,
    ImageFormat as CoreImageFormat, ImageInfo as CoreImageInfo,
    ImageWatermark as CoreImageWatermark, PaletteColour as CorePaletteColour,
    Position as CorePosition, TextWatermark as CoreTextWatermark, fixtures::MockKind,
};
use dusrelimage::{
    CollageOptions as CoreCollageOptions, CommonOptions as CoreCommonOptions,
    CompressOptions as CoreCompressOptions, ConvertOptions as CoreConvertOptions,
    CropOptions as CoreCropOptions, PixelateOptions as CorePixelateOptions,
    ResizeOptions as CoreResizeOptions, WatermarkOptions as CoreWatermarkOptions,
};

uniffi::include_scaffolding!("dusrelimage");

// ---------------------------------------------------------------------------
// Value types (field-for-field with the UDL dictionaries / enums)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Point {
    pub x: u32,
    pub y: u32,
}

impl From<(u32, u32)> for Point {
    fn from(value: (u32, u32)) -> Self {
        Self {
            x: value.0,
            y: value.1,
        }
    }
}

impl From<Point> for (u32, u32) {
    fn from(value: Point) -> Self {
        (value.x, value.y)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dpi {
    pub width: u32,
    pub height: u32,
}

impl From<(u32, u32)> for Dpi {
    fn from(value: (u32, u32)) -> Self {
        Self {
            width: value.0,
            height: value.1,
        }
    }
}

impl From<Dpi> for (u32, u32) {
    fn from(value: Dpi) -> Self {
        (value.width, value.height)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Colour {
    pub hex: String,
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
    pub hsl: Vec<f32>,
}

impl From<CoreColour> for Colour {
    fn from(value: CoreColour) -> Self {
        Self {
            hex: value.hex,
            r: value.r,
            g: value.g,
            b: value.b,
            a: value.a,
            hsl: vec![value.hsl.0, value.hsl.1, value.hsl.2],
        }
    }
}

impl From<Colour> for CoreColour {
    fn from(value: Colour) -> Self {
        // Canonical rebuild from RGBA (recomputes hex + hsl); a
        // caller-supplied hex/hsl that disagrees is normalised.
        CoreColour::from_rgba(value.r, value.g, value.b, value.a)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImageInfo {
    pub format: String,
    pub width: u32,
    pub height: u32,
    pub has_alpha: bool,
    pub colour_type: String,
    pub file_size: u64,
}

impl From<CoreImageInfo> for ImageInfo {
    fn from(value: CoreImageInfo) -> Self {
        Self {
            format: value.format,
            width: value.width,
            height: value.height,
            has_alpha: value.has_alpha,
            colour_type: value.colour_type,
            file_size: value.file_size,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PaletteColour {
    pub colour: Colour,
    pub share: f32,
}

impl From<CorePaletteColour> for PaletteColour {
    fn from(value: CorePaletteColour) -> Self {
        Self {
            colour: value.colour.into(),
            share: value.share,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextWatermark {
    pub text: String,
    pub size: f32,
    pub colour: Colour,
}

impl From<CoreTextWatermark> for TextWatermark {
    fn from(value: CoreTextWatermark) -> Self {
        Self {
            text: value.text,
            size: value.size,
            colour: value.colour.into(),
        }
    }
}

impl From<TextWatermark> for CoreTextWatermark {
    fn from(value: TextWatermark) -> Self {
        Self {
            text: value.text,
            size: value.size,
            colour: value.colour.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImageWatermark {
    pub image: Vec<u8>,
    pub scale: f32,
}

impl From<CoreImageWatermark> for ImageWatermark {
    fn from(value: CoreImageWatermark) -> Self {
        Self {
            image: value.image,
            scale: value.scale,
        }
    }
}

impl From<ImageWatermark> for CoreImageWatermark {
    fn from(value: ImageWatermark) -> Self {
        Self {
            image: value.image,
            scale: value.scale,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    Jpeg,
    Png,
    WebP,
    Avif,
    Gif,
    Bmp,
}

impl From<CoreImageFormat> for ImageFormat {
    fn from(value: CoreImageFormat) -> Self {
        match value {
            CoreImageFormat::Jpeg => Self::Jpeg,
            CoreImageFormat::Png => Self::Png,
            CoreImageFormat::WebP => Self::WebP,
            CoreImageFormat::Avif => Self::Avif,
            CoreImageFormat::Gif => Self::Gif,
            CoreImageFormat::Bmp => Self::Bmp,
        }
    }
}

impl From<ImageFormat> for CoreImageFormat {
    fn from(value: ImageFormat) -> Self {
        match value {
            ImageFormat::Jpeg => Self::Jpeg,
            ImageFormat::Png => Self::Png,
            ImageFormat::WebP => Self::WebP,
            ImageFormat::Avif => Self::Avif,
            ImageFormat::Gif => Self::Gif,
            ImageFormat::Bmp => Self::Bmp,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FitMode {
    Fit,
    Fill,
    Stretch,
    Crop,
}

impl From<CoreFitMode> for FitMode {
    fn from(value: CoreFitMode) -> Self {
        match value {
            CoreFitMode::Fit => Self::Fit,
            CoreFitMode::Fill => Self::Fill,
            CoreFitMode::Stretch => Self::Stretch,
            CoreFitMode::Crop => Self::Crop,
        }
    }
}

impl From<FitMode> for CoreFitMode {
    fn from(value: FitMode) -> Self {
        match value {
            FitMode::Fit => Self::Fit,
            FitMode::Fill => Self::Fill,
            FitMode::Stretch => Self::Stretch,
            FitMode::Crop => Self::Crop,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filter {
    Nearest,
    Triangle,
    CatmullRom,
    Gaussian,
    Lanczos3,
}

impl From<CoreFilter> for Filter {
    fn from(value: CoreFilter) -> Self {
        match value {
            CoreFilter::Nearest => Self::Nearest,
            CoreFilter::Triangle => Self::Triangle,
            CoreFilter::CatmullRom => Self::CatmullRom,
            CoreFilter::Gaussian => Self::Gaussian,
            CoreFilter::Lanczos3 => Self::Lanczos3,
        }
    }
}

impl From<Filter> for CoreFilter {
    fn from(value: Filter) -> Self {
        match value {
            Filter::Nearest => Self::Nearest,
            Filter::Triangle => Self::Triangle,
            Filter::CatmullRom => Self::CatmullRom,
            Filter::Gaussian => Self::Gaussian,
            Filter::Lanczos3 => Self::Lanczos3,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Position {
    TopLeft,
    TopCenter,
    TopRight,
    CenterLeft,
    Center,
    CenterRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
    Tiled,
}

impl From<CorePosition> for Position {
    fn from(value: CorePosition) -> Self {
        match value {
            CorePosition::TopLeft => Self::TopLeft,
            CorePosition::TopCenter => Self::TopCenter,
            CorePosition::TopRight => Self::TopRight,
            CorePosition::CenterLeft => Self::CenterLeft,
            CorePosition::Center => Self::Center,
            CorePosition::CenterRight => Self::CenterRight,
            CorePosition::BottomLeft => Self::BottomLeft,
            CorePosition::BottomCenter => Self::BottomCenter,
            CorePosition::BottomRight => Self::BottomRight,
            CorePosition::Tiled => Self::Tiled,
        }
    }
}

impl From<Position> for CorePosition {
    fn from(value: Position) -> Self {
        match value {
            Position::TopLeft => Self::TopLeft,
            Position::TopCenter => Self::TopCenter,
            Position::TopRight => Self::TopRight,
            Position::CenterLeft => Self::CenterLeft,
            Position::Center => Self::Center,
            Position::CenterRight => Self::CenterRight,
            Position::BottomLeft => Self::BottomLeft,
            Position::BottomCenter => Self::BottomCenter,
            Position::BottomRight => Self::BottomRight,
            Position::Tiled => Self::Tiled,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollageLayout {
    Grid { rows: u32, cols: u32 },
    Row,
    Column,
}

impl From<CoreCollageLayout> for CollageLayout {
    fn from(value: CoreCollageLayout) -> Self {
        match value {
            CoreCollageLayout::Grid { rows, cols } => Self::Grid { rows, cols },
            CoreCollageLayout::Row => Self::Row,
            CoreCollageLayout::Column => Self::Column,
        }
    }
}

impl From<CollageLayout> for CoreCollageLayout {
    fn from(value: CollageLayout) -> Self {
        match value {
            CollageLayout::Grid { rows, cols } => Self::Grid { rows, cols },
            CollageLayout::Row => Self::Row,
            CollageLayout::Column => Self::Column,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Annotation {
    Arrow {
        from: Point,
        to: Point,
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
        from: Point,
        to: Point,
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

impl From<CoreAnnotation> for Annotation {
    fn from(value: CoreAnnotation) -> Self {
        match value {
            CoreAnnotation::Arrow {
                from,
                to,
                colour,
                width,
            } => Self::Arrow {
                from: from.into(),
                to: to.into(),
                colour: colour.into(),
                width,
            },
            CoreAnnotation::Rect {
                x,
                y,
                w,
                h,
                colour,
                width,
                fill,
            } => Self::Rect {
                x,
                y,
                w,
                h,
                colour: colour.into(),
                width,
                fill: fill.map(Colour::from),
            },
            CoreAnnotation::Ellipse {
                x,
                y,
                w,
                h,
                colour,
                width,
                fill,
            } => Self::Ellipse {
                x,
                y,
                w,
                h,
                colour: colour.into(),
                width,
                fill: fill.map(Colour::from),
            },
            CoreAnnotation::Line {
                from,
                to,
                colour,
                width,
            } => Self::Line {
                from: from.into(),
                to: to.into(),
                colour: colour.into(),
                width,
            },
            CoreAnnotation::Text {
                x,
                y,
                text,
                size,
                colour,
            } => Self::Text {
                x,
                y,
                text,
                size,
                colour: colour.into(),
            },
        }
    }
}

impl From<Annotation> for CoreAnnotation {
    fn from(value: Annotation) -> Self {
        match value {
            Annotation::Arrow {
                from,
                to,
                colour,
                width,
            } => Self::Arrow {
                from: from.into(),
                to: to.into(),
                colour: colour.into(),
                width,
            },
            Annotation::Rect {
                x,
                y,
                w,
                h,
                colour,
                width,
                fill,
            } => Self::Rect {
                x,
                y,
                w,
                h,
                colour: colour.into(),
                width,
                fill: fill.map(CoreColour::from),
            },
            Annotation::Ellipse {
                x,
                y,
                w,
                h,
                colour,
                width,
                fill,
            } => Self::Ellipse {
                x,
                y,
                w,
                h,
                colour: colour.into(),
                width,
                fill: fill.map(CoreColour::from),
            },
            Annotation::Line {
                from,
                to,
                colour,
                width,
            } => Self::Line {
                from: from.into(),
                to: to.into(),
                colour: colour.into(),
                width,
            },
            Annotation::Text {
                x,
                y,
                text,
                size,
                colour,
            } => Self::Text {
                x,
                y,
                text,
                size,
                colour: colour.into(),
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Options (each embeds `common: CommonOptions`)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct CommonOptions {
    pub auto_orient: bool,
    pub keep_exif: bool,
    pub dpi: Option<Dpi>,
    pub max_pixels: u64,
    pub max_dimension: u32,
}

impl From<CoreCommonOptions> for CommonOptions {
    fn from(value: CoreCommonOptions) -> Self {
        Self {
            auto_orient: value.auto_orient,
            keep_exif: value.keep_exif,
            dpi: value.dpi.map(Dpi::from),
            max_pixels: value.max_pixels,
            max_dimension: value.max_dimension,
        }
    }
}

impl From<CommonOptions> for CoreCommonOptions {
    fn from(value: CommonOptions) -> Self {
        Self {
            auto_orient: value.auto_orient,
            keep_exif: value.keep_exif,
            dpi: value.dpi.map(Into::into),
            max_pixels: value.max_pixels,
            max_dimension: value.max_dimension,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompressOptions {
    pub common: CommonOptions,
    pub quality: u8,
    pub format: Option<ImageFormat>,
    pub progressive: bool,
    pub lossless: bool,
    pub effort: u8,
}

impl From<CoreCompressOptions> for CompressOptions {
    fn from(value: CoreCompressOptions) -> Self {
        Self {
            common: value.common.into(),
            quality: value.quality,
            format: value.format.map(ImageFormat::from),
            progressive: value.progressive,
            lossless: value.lossless,
            effort: value.effort,
        }
    }
}

impl From<CompressOptions> for CoreCompressOptions {
    fn from(value: CompressOptions) -> Self {
        Self {
            common: value.common.into(),
            quality: value.quality,
            format: value.format.map(CoreImageFormat::from),
            progressive: value.progressive,
            lossless: value.lossless,
            effort: value.effort,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResizeOptions {
    pub common: CommonOptions,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub percent: Option<f32>,
    pub fit: FitMode,
    pub filter: Filter,
    pub background: Colour,
}

impl From<CoreResizeOptions> for ResizeOptions {
    fn from(value: CoreResizeOptions) -> Self {
        Self {
            common: value.common.into(),
            width: value.width,
            height: value.height,
            percent: value.percent,
            fit: value.fit.into(),
            filter: value.filter.into(),
            background: value.background.into(),
        }
    }
}

impl From<ResizeOptions> for CoreResizeOptions {
    fn from(value: ResizeOptions) -> Self {
        Self {
            common: value.common.into(),
            width: value.width,
            height: value.height,
            percent: value.percent,
            fit: value.fit.into(),
            filter: value.filter.into(),
            background: value.background.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CropOptions {
    pub common: CommonOptions,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub anchor: Option<Position>,
}

impl From<CoreCropOptions> for CropOptions {
    fn from(value: CoreCropOptions) -> Self {
        Self {
            common: value.common.into(),
            x: value.x,
            y: value.y,
            width: value.width,
            height: value.height,
            anchor: value.anchor.map(Position::from),
        }
    }
}

impl From<CropOptions> for CoreCropOptions {
    fn from(value: CropOptions) -> Self {
        Self {
            common: value.common.into(),
            x: value.x,
            y: value.y,
            width: value.width,
            height: value.height,
            anchor: value.anchor.map(CorePosition::from),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConvertOptions {
    pub common: CommonOptions,
    pub format: ImageFormat,
    pub quality: u8,
    pub background: Colour,
    pub progressive: bool,
}

impl From<CoreConvertOptions> for ConvertOptions {
    fn from(value: CoreConvertOptions) -> Self {
        Self {
            common: value.common.into(),
            format: value.format.into(),
            quality: value.quality,
            background: value.background.into(),
            progressive: value.progressive,
        }
    }
}

impl From<ConvertOptions> for CoreConvertOptions {
    fn from(value: ConvertOptions) -> Self {
        Self {
            common: value.common.into(),
            format: value.format.into(),
            quality: value.quality,
            background: value.background.into(),
            progressive: value.progressive,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct WatermarkOptions {
    pub common: CommonOptions,
    pub text: Option<TextWatermark>,
    pub logo: Option<ImageWatermark>,
    pub opacity: f32,
    pub position: Position,
    pub rotation_deg: f32,
    pub margin: u32,
}

impl From<CoreWatermarkOptions> for WatermarkOptions {
    fn from(value: CoreWatermarkOptions) -> Self {
        Self {
            common: value.common.into(),
            text: value.text.map(TextWatermark::from),
            logo: value.logo.map(ImageWatermark::from),
            opacity: value.opacity,
            position: value.position.into(),
            rotation_deg: value.rotation_deg,
            margin: value.margin,
        }
    }
}

impl From<WatermarkOptions> for CoreWatermarkOptions {
    fn from(value: WatermarkOptions) -> Self {
        Self {
            common: value.common.into(),
            text: value.text.map(CoreTextWatermark::from),
            logo: value.logo.map(CoreImageWatermark::from),
            opacity: value.opacity,
            position: value.position.into(),
            rotation_deg: value.rotation_deg,
            margin: value.margin,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CollageOptions {
    pub common: CommonOptions,
    pub layout: CollageLayout,
    pub gap: u32,
    pub background: Colour,
    pub width: u32,
    pub height: u32,
    pub cell_fit: FitMode,
}

impl From<CoreCollageOptions> for CollageOptions {
    fn from(value: CoreCollageOptions) -> Self {
        Self {
            common: value.common.into(),
            layout: value.layout.into(),
            gap: value.gap,
            background: value.background.into(),
            width: value.width,
            height: value.height,
            cell_fit: value.cell_fit.into(),
        }
    }
}

impl From<CollageOptions> for CoreCollageOptions {
    fn from(value: CollageOptions) -> Self {
        Self {
            common: value.common.into(),
            layout: value.layout.into(),
            gap: value.gap,
            background: value.background.into(),
            width: value.width,
            height: value.height,
            cell_fit: value.cell_fit.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PixelateOptions {
    pub common: CommonOptions,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub block_size: u32,
}

impl From<CorePixelateOptions> for PixelateOptions {
    fn from(value: CorePixelateOptions) -> Self {
        Self {
            common: value.common.into(),
            x: value.x,
            y: value.y,
            width: value.width,
            height: value.height,
            block_size: value.block_size,
        }
    }
}

impl From<PixelateOptions> for CorePixelateOptions {
    fn from(value: PixelateOptions) -> Self {
        Self {
            common: value.common.into(),
            x: value.x,
            y: value.y,
            width: value.width,
            height: value.height,
            block_size: value.block_size,
        }
    }
}

// ---------------------------------------------------------------------------
// Errors — the UDL `DusrelError` enum: one variant per stable code,
// each carrying `code` + `message_en` + `message_hi` from the core.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum DusrelError {
    InvalidImage {
        code: String,
        message_en: String,
        message_hi: String,
    },
    UnsupportedFormat {
        code: String,
        message_en: String,
        message_hi: String,
    },
    ImageTooLarge {
        code: String,
        message_en: String,
        message_hi: String,
    },
    TargetSizeUnreachable {
        code: String,
        message_en: String,
        message_hi: String,
    },
    CropOutOfBounds {
        code: String,
        message_en: String,
        message_hi: String,
    },
    PixelOutOfBounds {
        code: String,
        message_en: String,
        message_hi: String,
    },
    InvalidQuality {
        code: String,
        message_en: String,
        message_hi: String,
    },
    InvalidDimensions {
        code: String,
        message_en: String,
        message_hi: String,
    },
    InvalidArgument {
        code: String,
        message_en: String,
        message_hi: String,
    },
    EmptyInput {
        code: String,
        message_en: String,
        message_hi: String,
    },
    InvalidType {
        code: String,
        message_en: String,
        message_hi: String,
    },
}

impl DusrelError {
    /// Stable code string, e.g. `"INVALID_IMAGE"`.
    pub fn code_str(&self) -> &'static str {
        match self {
            Self::InvalidImage { .. } => "INVALID_IMAGE",
            Self::UnsupportedFormat { .. } => "UNSUPPORTED_FORMAT",
            Self::ImageTooLarge { .. } => "IMAGE_TOO_LARGE",
            Self::TargetSizeUnreachable { .. } => "TARGET_SIZE_UNREACHABLE",
            Self::CropOutOfBounds { .. } => "CROP_OUT_OF_BOUNDS",
            Self::PixelOutOfBounds { .. } => "PIXEL_OUT_OF_BOUNDS",
            Self::InvalidQuality { .. } => "INVALID_QUALITY",
            Self::InvalidDimensions { .. } => "INVALID_DIMENSIONS",
            Self::InvalidArgument { .. } => "INVALID_ARGUMENT",
            Self::EmptyInput { .. } => "EMPTY_INPUT",
            Self::InvalidType { .. } => "INVALID_TYPE",
        }
    }

    pub fn message_en(&self) -> &str {
        match self {
            Self::InvalidImage { message_en, .. }
            | Self::UnsupportedFormat { message_en, .. }
            | Self::ImageTooLarge { message_en, .. }
            | Self::TargetSizeUnreachable { message_en, .. }
            | Self::CropOutOfBounds { message_en, .. }
            | Self::PixelOutOfBounds { message_en, .. }
            | Self::InvalidQuality { message_en, .. }
            | Self::InvalidDimensions { message_en, .. }
            | Self::InvalidArgument { message_en, .. }
            | Self::EmptyInput { message_en, .. }
            | Self::InvalidType { message_en, .. } => message_en,
        }
    }

    pub fn message_hi(&self) -> &str {
        match self {
            Self::InvalidImage { message_hi, .. }
            | Self::UnsupportedFormat { message_hi, .. }
            | Self::ImageTooLarge { message_hi, .. }
            | Self::TargetSizeUnreachable { message_hi, .. }
            | Self::CropOutOfBounds { message_hi, .. }
            | Self::PixelOutOfBounds { message_hi, .. }
            | Self::InvalidQuality { message_hi, .. }
            | Self::InvalidDimensions { message_hi, .. }
            | Self::InvalidArgument { message_hi, .. }
            | Self::EmptyInput { message_hi, .. }
            | Self::InvalidType { message_hi, .. } => message_hi,
        }
    }
}

impl std::fmt::Display for DusrelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code_str(), self.message_en())
    }
}

impl std::error::Error for DusrelError {}

impl From<CoreError> for DusrelError {
    fn from(value: CoreError) -> Self {
        let code = value.code.as_str().to_string();
        let message_en = value.message_en.clone();
        let message_hi = value.message_hi.clone();
        match value.code {
            CoreErrorCode::InvalidImage => Self::InvalidImage {
                code,
                message_en,
                message_hi,
            },
            CoreErrorCode::UnsupportedFormat => Self::UnsupportedFormat {
                code,
                message_en,
                message_hi,
            },
            CoreErrorCode::ImageTooLarge => Self::ImageTooLarge {
                code,
                message_en,
                message_hi,
            },
            CoreErrorCode::TargetSizeUnreachable => Self::TargetSizeUnreachable {
                code,
                message_en,
                message_hi,
            },
            CoreErrorCode::CropOutOfBounds => Self::CropOutOfBounds {
                code,
                message_en,
                message_hi,
            },
            CoreErrorCode::PixelOutOfBounds => Self::PixelOutOfBounds {
                code,
                message_en,
                message_hi,
            },
            CoreErrorCode::InvalidQuality => Self::InvalidQuality {
                code,
                message_en,
                message_hi,
            },
            CoreErrorCode::InvalidDimensions => Self::InvalidDimensions {
                code,
                message_en,
                message_hi,
            },
            CoreErrorCode::InvalidArgument => Self::InvalidArgument {
                code,
                message_en,
                message_hi,
            },
            CoreErrorCode::EmptyInput => Self::EmptyInput {
                code,
                message_en,
                message_hi,
            },
            CoreErrorCode::InvalidType => Self::InvalidType {
                code,
                message_en,
                message_hi,
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Bulk types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum BulkOp {
    Compress {
        options: CompressOptions,
    },
    CompressToSize {
        options: CompressOptions,
        target_bytes: u64,
    },
    Resize {
        options: ResizeOptions,
    },
    Crop {
        options: CropOptions,
    },
    Convert {
        options: ConvertOptions,
    },
    Watermark {
        options: WatermarkOptions,
    },
    Annotate {
        annotations: Vec<Annotation>,
    },
    PickColour {
        x: u32,
        y: u32,
    },
    Palette {
        count: u8,
    },
    Collage {
        options: CollageOptions,
    },
    Pixelate {
        options: PixelateOptions,
    },
}

/// Convert a UDL `BulkOp` to the core `BulkOp` — `None` for
/// `CompressToSize`, which has no core counterpart (the core
/// `BulkOp` has no such variant); `bulk_process` handles that case
/// directly instead of converting.
fn to_core_op(value: BulkOp) -> Option<CoreBulkOp> {
    Some(match value {
        BulkOp::Compress { options } => CoreBulkOp::Compress(options.into()),
        BulkOp::CompressToSize { .. } => return None,
        BulkOp::Resize { options } => CoreBulkOp::Resize(options.into()),
        BulkOp::Crop { options } => CoreBulkOp::Crop(options.into()),
        BulkOp::Convert { options } => CoreBulkOp::Convert(options.into()),
        BulkOp::Watermark { options } => CoreBulkOp::Watermark(options.into()),
        BulkOp::Annotate { annotations } => {
            CoreBulkOp::Annotate(annotations.into_iter().map(CoreAnnotation::from).collect())
        }
        BulkOp::PickColour { x, y } => CoreBulkOp::PickColour { x, y },
        BulkOp::Palette { count } => CoreBulkOp::Palette { count },
        BulkOp::Collage { options } => CoreBulkOp::Collage(options.into()),
        BulkOp::Pixelate { options } => CoreBulkOp::Pixelate(options.into()),
    })
}

#[derive(Debug, Clone, PartialEq)]
pub enum BulkOutput {
    Image { bytes: Vec<u8> },
    Colour { colour: Colour },
    Palette { palette: Vec<PaletteColour> },
}

impl From<CoreBulkOutput> for BulkOutput {
    fn from(value: CoreBulkOutput) -> Self {
        match value {
            CoreBulkOutput::Image(bytes) => Self::Image { bytes },
            CoreBulkOutput::Colour(colour) => Self::Colour {
                colour: colour.into(),
            },
            CoreBulkOutput::Palette(palette) => Self::Palette {
                palette: palette.into_iter().map(PaletteColour::from).collect(),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BulkItemResult {
    pub index: u32,
    pub ok: bool,
    pub output: Option<BulkOutput>,
    pub error_code: Option<String>,
    pub error_message_en: Option<String>,
    pub error_message_hi: Option<String>,
}

fn bulk_item(index: usize, result: Result<CoreBulkOutput, CoreError>) -> BulkItemResult {
    match result {
        Ok(output) => BulkItemResult {
            index: index as u32,
            ok: true,
            output: Some(output.into()),
            error_code: None,
            error_message_en: None,
            error_message_hi: None,
        },
        Err(err) => BulkItemResult {
            index: index as u32,
            ok: false,
            output: None,
            error_code: Some(err.code.as_str().to_string()),
            error_message_en: Some(err.message_en.clone()),
            error_message_hi: Some(err.message_hi.clone()),
        },
    }
}

// ---------------------------------------------------------------------------
// Exported functions (one per UDL namespace entry, same signatures)
// ---------------------------------------------------------------------------

/// Re-encode an image at a fixed quality (and `format`, or the input
/// format when unset).
pub fn compress_image(input: Vec<u8>, options: CompressOptions) -> Result<Vec<u8>, DusrelError> {
    dusrelimage::compress_image(&input, &options.into()).map_err(DusrelError::from)
}

/// Re-encode an image so the result is at most `target_bytes` bytes,
/// at the highest quality that fits. Fails with
/// `TARGET_SIZE_UNREACHABLE` when even the lowest quality is too big.
/// The quality search is JPEG/AVIF-only — other formats take one
/// best-effort encode.
pub fn compress_to_size(
    input: Vec<u8>,
    target_bytes: u64,
    options: CompressOptions,
) -> Result<Vec<u8>, DusrelError> {
    dusrelimage::compress_to_size(&input, target_bytes, &options.into()).map_err(DusrelError::from)
}

/// Resize an image (width and/or height, percent, fit, filter).
pub fn resize_image(input: Vec<u8>, options: ResizeOptions) -> Result<Vec<u8>, DusrelError> {
    dusrelimage::resize_image(&input, &options.into()).map_err(DusrelError::from)
}

/// Crop an image to a rectangle (or an anchor preset).
pub fn crop_image(input: Vec<u8>, options: CropOptions) -> Result<Vec<u8>, DusrelError> {
    dusrelimage::crop_image(&input, &options.into()).map_err(DusrelError::from)
}

/// Convert an image to another format.
pub fn convert_image(input: Vec<u8>, options: ConvertOptions) -> Result<Vec<u8>, DusrelError> {
    dusrelimage::convert_image(&input, &options.into()).map_err(DusrelError::from)
}

/// Stamp a text or logo watermark. `text` and `logo` may be combined
/// (text first, then logo); at least one is required.
pub fn add_watermark(input: Vec<u8>, options: WatermarkOptions) -> Result<Vec<u8>, DusrelError> {
    dusrelimage::add_watermark(&input, &options.into()).map_err(DusrelError::from)
}

/// Draw annotations (arrows, rects, ellipses, lines, text) on an image.
pub fn annotate_image(
    input: Vec<u8>,
    annotations: Vec<Annotation>,
) -> Result<Vec<u8>, DusrelError> {
    let core: Vec<CoreAnnotation> = annotations.into_iter().map(CoreAnnotation::from).collect();
    dusrelimage::annotate_image(&input, &core).map_err(DusrelError::from)
}

/// Read the colour of one pixel. Out-of-bounds is `PIXEL_OUT_OF_BOUNDS`.
pub fn pick_colour(input: Vec<u8>, x: u32, y: u32) -> Result<Colour, DusrelError> {
    dusrelimage::pick_colour(&input, x, y)
        .map(Colour::from)
        .map_err(DusrelError::from)
}

/// Extract the dominant colours, most frequent first (count 1-16).
pub fn extract_palette(input: Vec<u8>, count: u8) -> Result<Vec<PaletteColour>, DusrelError> {
    dusrelimage::extract_palette(&input, count)
        .map(|palette| palette.into_iter().map(PaletteColour::from).collect())
        .map_err(DusrelError::from)
}

/// Build a collage from several images (grid, row or column).
/// The output is always PNG.
pub fn make_collage(inputs: Vec<Vec<u8>>, options: CollageOptions) -> Result<Vec<u8>, DusrelError> {
    dusrelimage::make_collage(&inputs, &options.into()).map_err(DusrelError::from)
}

/// Pixelate a rectangle (whole image when width/height are 0).
pub fn pixelate_image(input: Vec<u8>, options: PixelateOptions) -> Result<Vec<u8>, DusrelError> {
    dusrelimage::pixelate_image(&input, &options.into()).map_err(DusrelError::from)
}

/// Read an image's summary without modifying it.
pub fn get_image_info(input: Vec<u8>, options: CommonOptions) -> Result<ImageInfo, DusrelError> {
    dusrelimage::get_image_info(&input, &options.into())
        .map(ImageInfo::from)
        .map_err(DusrelError::from)
}

/// Run one operation over many inputs in a single call. One bad input
/// fails only its own item (with `error_code` / `error_message_en`),
/// never the whole batch. An empty input list returns an empty list.
/// `Collage` is not a per-item op: it returns one
/// `INVALID_ARGUMENT` result per input — call `make_collage` once
/// with the full list instead.
pub fn bulk_process(inputs: Vec<Vec<u8>>, op: BulkOp) -> Vec<BulkItemResult> {
    // UniFFI-only op: the core `BulkOp` has no `CompressToSize`
    // variant, so run it per item directly (same semantics as the
    // JS/Python bulk `compress_to_size`).
    if let BulkOp::CompressToSize {
        options,
        target_bytes,
    } = op
    {
        let core_options: CoreCompressOptions = options.into();
        return inputs
            .iter()
            .enumerate()
            .map(|(index, input)| {
                bulk_item(
                    index,
                    dusrelimage::compress_to_size(input, target_bytes, &core_options)
                        .map(CoreBulkOutput::Image),
                )
            })
            .collect();
    }
    let core_op = to_core_op(op).expect("non-CompressToSize BulkOp converts to a core BulkOp");
    dusrelimage::bulk_process(&inputs, &core_op)
        .into_iter()
        .map(|item| bulk_item(item.index, item.result))
        .collect()
}

/// Build a deterministic test image (PNG bytes). `kind` is `"solid"`,
/// `"gradient"` or `"noise"`; anything else is `INVALID_ARGUMENT`.
/// Test data only — same builders as the CLI `mock` command and the
/// JS/Python `makeTestImage`.
pub fn make_test_image(
    kind: String,
    width: u32,
    height: u32,
    seed: u64,
) -> Result<Vec<u8>, DusrelError> {
    let kind = match kind.as_str() {
        "solid" => MockKind::Solid,
        "gradient" => MockKind::Gradient,
        "noise" => MockKind::Noise,
        other => {
            return Err(DusrelError::InvalidArgument {
                code: "INVALID_ARGUMENT".to_string(),
                message_en: format!(
                    "Test image kind must be \"solid\", \"gradient\" or \"noise\", got \"{other}\"."
                ),
                message_hi: format!(
                    "Test image kind \"solid\", \"gradient\" ya \"noise\" hona chahiye, \"{other}\" mila."
                ),
            });
        }
    };
    Ok(dusrelimage::fixtures::make_test_image(
        kind, width, height, seed,
    ))
}

/// Crate version, e.g. `"0.1.0"` (lockstep with npm / PyPI / crates.io).
pub fn version() -> String {
    dusrelimage::VERSION.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn common() -> CommonOptions {
        CommonOptions {
            auto_orient: true,
            keep_exif: false,
            dpi: None,
            max_pixels: 120_000_000,
            max_dimension: 16000,
        }
    }

    fn white() -> Colour {
        CoreColour::white().into()
    }

    fn compress_opts() -> CompressOptions {
        CompressOptions {
            common: common(),
            quality: 80,
            format: None,
            progressive: false,
            lossless: false,
            effort: 4,
        }
    }

    fn code_of(err: &DusrelError) -> &str {
        err.code_str()
    }

    /// Every error variant carries the code string plus non-empty
    /// English and Hindi messages.
    fn assert_error_shape(err: &DusrelError, expected_code: &str) {
        assert_eq!(code_of(err), expected_code);
        assert!(!err.message_en().is_empty(), "message_en must not be empty");
        assert!(!err.message_hi().is_empty(), "message_hi must not be empty");
    }

    #[test]
    fn version_matches_core() {
        assert_eq!(version(), "0.1.0");
    }

    #[test]
    fn make_test_image_is_deterministic() {
        let a = make_test_image("noise".to_string(), 64, 48, 42).unwrap();
        let b = make_test_image("noise".to_string(), 64, 48, 42).unwrap();
        assert_eq!(a, b);
        assert!(make_test_image("bogus".to_string(), 64, 48, 42).is_err());
        let err = make_test_image("bogus".to_string(), 64, 48, 42).unwrap_err();
        assert_error_shape(&err, "INVALID_ARGUMENT");
    }

    #[test]
    fn compress_image_keeps_format_and_dimensions() {
        let input = make_test_image("gradient".to_string(), 64, 48, 4).unwrap();
        let out = compress_image(input, compress_opts()).unwrap();
        let info = get_image_info(out, common()).unwrap();
        assert_eq!(info.format, "png");
        assert_eq!(info.width, 64);
        assert_eq!(info.height, 48);
    }

    #[test]
    fn compress_to_jpeg() {
        let input = make_test_image("gradient".to_string(), 100, 80, 2).unwrap();
        let mut opts = compress_opts();
        opts.format = Some(ImageFormat::Jpeg);
        let out = compress_image(input, opts).unwrap();
        let info = get_image_info(out, common()).unwrap();
        assert_eq!(info.format, "jpeg");
        assert_eq!(info.width, 100);
        assert_eq!(info.height, 80);
    }

    #[test]
    fn compress_invalid_quality() {
        let input = make_test_image("solid".to_string(), 32, 32, 1).unwrap();
        let mut opts = compress_opts();
        opts.quality = 0;
        let err = compress_image(input, opts).unwrap_err();
        assert_error_shape(&err, "INVALID_QUALITY");
    }

    #[test]
    fn compress_to_size_reaches_target_and_unreachable() {
        let input = make_test_image("noise".to_string(), 400, 300, 7).unwrap();
        let mut opts = compress_opts();
        opts.format = Some(ImageFormat::Jpeg);
        opts.quality = 90;
        let out = compress_to_size(input.clone(), 200_000, opts.clone()).unwrap();
        assert!(out.len() as u64 <= 200_000);
        let info = get_image_info(out, common()).unwrap();
        assert_eq!(info.width, 400);
        assert_eq!(info.height, 300);
        let err = compress_to_size(input, 100, opts).unwrap_err();
        assert_error_shape(&err, "TARGET_SIZE_UNREACHABLE");
    }

    #[test]
    fn resize_percent_halves_dimensions() {
        let input = make_test_image("gradient".to_string(), 100, 80, 3).unwrap();
        let opts = ResizeOptions {
            common: common(),
            width: None,
            height: None,
            percent: Some(50.0),
            fit: FitMode::Fit,
            filter: Filter::Lanczos3,
            background: white(),
        };
        let out = resize_image(input, opts).unwrap();
        let info = get_image_info(out, common()).unwrap();
        assert_eq!((info.width, info.height), (50, 40));
    }

    #[test]
    fn resize_without_size_is_invalid_dimensions() {
        let input = make_test_image("solid".to_string(), 32, 32, 1).unwrap();
        let opts = ResizeOptions {
            common: common(),
            width: None,
            height: None,
            percent: None,
            fit: FitMode::Fit,
            filter: Filter::Lanczos3,
            background: white(),
        };
        let err = resize_image(input, opts).unwrap_err();
        assert_error_shape(&err, "INVALID_DIMENSIONS");
    }

    #[test]
    fn crop_explicit_rect_and_out_of_bounds() {
        let input = make_test_image("solid".to_string(), 100, 80, 9).unwrap();
        let opts = CropOptions {
            common: common(),
            x: 10,
            y: 10,
            width: 20,
            height: 20,
            anchor: None,
        };
        let out = crop_image(input.clone(), opts).unwrap();
        let info = get_image_info(out, common()).unwrap();
        assert_eq!((info.width, info.height), (20, 20));
        let bad = CropOptions {
            common: common(),
            x: 90,
            y: 10,
            width: 20,
            height: 20,
            anchor: None,
        };
        let err = crop_image(input, bad).unwrap_err();
        assert_error_shape(&err, "CROP_OUT_OF_BOUNDS");
    }

    #[test]
    fn convert_to_png_and_webp() {
        let input = make_test_image("gradient".to_string(), 64, 48, 4).unwrap();
        for format in [ImageFormat::Png, ImageFormat::WebP] {
            let opts = ConvertOptions {
                common: common(),
                format,
                quality: 80,
                background: white(),
                progressive: false,
            };
            let out = convert_image(input.clone(), opts).unwrap();
            let info = get_image_info(out, common()).unwrap();
            assert_eq!((info.width, info.height), (64, 48));
        }
        let bad = ConvertOptions {
            common: common(),
            format: ImageFormat::Png,
            quality: 0,
            background: white(),
            progressive: false,
        };
        let err = convert_image(input, bad).unwrap_err();
        assert_error_shape(&err, "INVALID_QUALITY");
    }

    #[test]
    fn watermark_text_and_no_watermark_is_invalid() {
        let input = dusrelimage::fixtures::solid_png(200, 100, CoreColour::white());
        let opts = WatermarkOptions {
            common: common(),
            text: Some(TextWatermark {
                text: "Hi".to_string(),
                size: 48.0,
                colour: CoreColour::black().into(),
            }),
            logo: None,
            opacity: 1.0,
            position: Position::BottomRight,
            rotation_deg: 0.0,
            margin: 16,
        };
        let out = add_watermark(input.clone(), opts).unwrap();
        let info = get_image_info(out, common()).unwrap();
        assert_eq!((info.width, info.height), (200, 100));
        let empty = WatermarkOptions {
            common: common(),
            text: None,
            logo: None,
            opacity: 0.5,
            position: Position::BottomRight,
            rotation_deg: 0.0,
            margin: 16,
        };
        let err = add_watermark(input, empty).unwrap_err();
        assert_error_shape(&err, "INVALID_ARGUMENT");
    }

    #[test]
    fn annotate_rect_and_empty_list() {
        let input = dusrelimage::fixtures::solid_png(100, 100, CoreColour::white());
        let red: Colour = CoreColour::from_rgb(255, 0, 0).into();
        let annotations = vec![Annotation::Rect {
            x: 10,
            y: 10,
            w: 40,
            h: 30,
            colour: red.clone(),
            width: 2,
            fill: Some(red),
        }];
        let out = annotate_image(input.clone(), annotations).unwrap();
        let filled = pick_colour(out, 30, 25).unwrap();
        assert_eq!(filled.hex, "#FF0000");
        let err = annotate_image(input, vec![]).unwrap_err();
        assert_error_shape(&err, "INVALID_ARGUMENT");
    }

    #[test]
    fn pick_colour_known_pixel_and_out_of_bounds() {
        let input = dusrelimage::fixtures::solid_png(64, 48, CoreColour::from_rgb(10, 20, 30));
        let colour = pick_colour(input.clone(), 5, 5).unwrap();
        assert_eq!(colour.hex, "#0A141E");
        assert_eq!((colour.r, colour.g, colour.b, colour.a), (10, 20, 30, 255));
        assert_eq!(colour.hsl.len(), 3);
        let err = pick_colour(input, 64, 0).unwrap_err();
        assert_error_shape(&err, "PIXEL_OUT_OF_BOUNDS");
    }

    #[test]
    fn extract_palette_solid_is_one_entry_and_count_zero() {
        let input = dusrelimage::fixtures::solid_png(50, 50, CoreColour::from_rgb(1, 2, 3));
        let palette = extract_palette(input.clone(), 4).unwrap();
        assert_eq!(palette.len(), 1);
        assert_eq!(palette[0].colour.hex, "#010203");
        assert!((palette[0].share - 1.0).abs() < 0.001);
        let err = extract_palette(input, 0).unwrap_err();
        assert_error_shape(&err, "INVALID_ARGUMENT");
    }

    #[test]
    fn make_collage_grid_and_empty_input() {
        let red = dusrelimage::fixtures::solid_png(90, 80, CoreColour::from_rgb(255, 0, 0));
        let blue = dusrelimage::fixtures::solid_png(90, 80, CoreColour::from_rgb(0, 0, 255));
        let opts = CollageOptions {
            common: common(),
            layout: CollageLayout::Grid { rows: 1, cols: 2 },
            gap: 10,
            background: white(),
            width: 210,
            height: 100,
            cell_fit: FitMode::Fit,
        };
        let out = make_collage(vec![red, blue], opts).unwrap();
        let info = get_image_info(out, common()).unwrap();
        assert_eq!(info.format, "png");
        assert_eq!((info.width, info.height), (210, 100));
        let empty_opts = CollageOptions {
            common: common(),
            layout: CollageLayout::Grid { rows: 2, cols: 2 },
            gap: 8,
            background: white(),
            width: 1200,
            height: 1200,
            cell_fit: FitMode::Fit,
        };
        let err = make_collage(vec![], empty_opts).unwrap_err();
        assert_error_shape(&err, "EMPTY_INPUT");
    }

    #[test]
    fn pixelate_full_image_and_bad_block_size() {
        let input = make_test_image("gradient".to_string(), 100, 100, 3).unwrap();
        let opts = PixelateOptions {
            common: common(),
            x: 0,
            y: 0,
            width: 100,
            height: 100,
            block_size: 20,
        };
        let out = pixelate_image(input.clone(), opts).unwrap();
        let info = get_image_info(out, common()).unwrap();
        assert_eq!((info.width, info.height), (100, 100));
        let bad = PixelateOptions {
            common: common(),
            x: 0,
            y: 0,
            width: 0,
            height: 0,
            block_size: 1,
        };
        let err = pixelate_image(input, bad).unwrap_err();
        assert_error_shape(&err, "INVALID_ARGUMENT");
    }

    #[test]
    fn get_image_info_png_and_garbage() {
        let input = make_test_image("gradient".to_string(), 64, 48, 4).unwrap();
        let info = get_image_info(input, common()).unwrap();
        assert_eq!(info.format, "png");
        assert_eq!(info.width, 64);
        assert_eq!(info.height, 48);
        assert!(!info.has_alpha);
        assert_eq!(info.colour_type, "rgb8");
        let err = get_image_info(b"this is not an image at all".to_vec(), common()).unwrap_err();
        assert_error_shape(&err, "INVALID_IMAGE");
        let err = get_image_info(vec![], common()).unwrap_err();
        assert_error_shape(&err, "EMPTY_INPUT");
    }

    #[test]
    fn bulk_preserves_order_and_reports_partial_failures() {
        let good = make_test_image("solid".to_string(), 32, 32, 1).unwrap();
        let garbage = vec![0u8, 1, 2, 3];
        let op = BulkOp::Pixelate {
            options: PixelateOptions {
                common: common(),
                x: 0,
                y: 0,
                width: 0,
                height: 0,
                block_size: 10,
            },
        };
        let results = bulk_process(
            vec![
                good,
                garbage,
                make_test_image("solid".to_string(), 32, 32, 1).unwrap(),
            ],
            op,
        );
        assert_eq!(results.len(), 3);
        assert!(results[0].ok);
        assert!(matches!(results[0].output, Some(BulkOutput::Image { .. })));
        assert!(!results[1].ok);
        assert_eq!(results[1].error_code.as_deref(), Some("INVALID_IMAGE"));
        assert!(results[1].error_message_en.is_some());
        assert!(results[1].error_message_hi.is_some());
        assert!(results[2].ok);
        assert_eq!(results[1].index, 1);
    }

    #[test]
    fn bulk_pick_colour_palette_and_collage() {
        let input = dusrelimage::fixtures::solid_png(20, 20, CoreColour::from_rgb(9, 8, 7));
        let colour_results = bulk_process(vec![input.clone()], BulkOp::PickColour { x: 3, y: 3 });
        assert_eq!(colour_results.len(), 1);
        assert!(colour_results[0].ok);
        match &colour_results[0].output {
            Some(BulkOutput::Colour { colour }) => assert_eq!(colour.hex, "#090807"),
            other => panic!("expected BulkOutput::Colour, got {other:?}"),
        }
        let palette_results = bulk_process(vec![input.clone()], BulkOp::Palette { count: 2 });
        assert!(palette_results[0].ok);
        match &palette_results[0].output {
            Some(BulkOutput::Palette { palette }) => assert_eq!(palette.len(), 1),
            other => panic!("expected BulkOutput::Palette, got {other:?}"),
        }
        let collage_results = bulk_process(
            vec![input],
            BulkOp::Collage {
                options: CollageOptions {
                    common: common(),
                    layout: CollageLayout::Grid { rows: 2, cols: 2 },
                    gap: 8,
                    background: white(),
                    width: 1200,
                    height: 1200,
                    cell_fit: FitMode::Fit,
                },
            },
        );
        assert!(!collage_results[0].ok);
        assert_eq!(
            collage_results[0].error_code.as_deref(),
            Some("INVALID_ARGUMENT")
        );
        assert!(bulk_process(vec![], BulkOp::Palette { count: 5 }).is_empty());
    }

    #[test]
    fn bulk_compress_to_size() {
        let input = make_test_image("noise".to_string(), 400, 300, 7).unwrap();
        let results = bulk_process(
            vec![input],
            BulkOp::CompressToSize {
                options: CompressOptions {
                    common: common(),
                    quality: 90,
                    format: Some(ImageFormat::Jpeg),
                    progressive: false,
                    lossless: false,
                    effort: 4,
                },
                target_bytes: 200_000,
            },
        );
        assert_eq!(results.len(), 1);
        assert!(results[0].ok);
        match &results[0].output {
            Some(BulkOutput::Image { bytes }) => {
                assert!(bytes.len() as u64 <= 200_000);
            }
            other => panic!("expected BulkOutput::Image, got {other:?}"),
        }
    }

    #[test]
    fn options_roundtrip_through_core() {
        // The UDL options convert to the core structs and back without
        // losing a field (guards the field-for-field mapping).
        let core = CoreCommonOptions {
            auto_orient: false,
            keep_exif: true,
            dpi: Some((300, 300)),
            max_pixels: 1000,
            max_dimension: 100,
        };
        let uni: CommonOptions = core.clone().into();
        assert_eq!(
            uni.dpi,
            Some(Dpi {
                width: 300,
                height: 300
            })
        );
        assert!(!uni.auto_orient);
        let back: CoreCommonOptions = uni.into();
        assert_eq!(back, core);
        let colour = CoreColour::from_rgb(10, 20, 30);
        let uni_colour: Colour = colour.clone().into();
        assert_eq!(uni_colour.hex, "#0A141E");
        assert_eq!(uni_colour.hsl.len(), 3);
        let back_colour: CoreColour = uni_colour.into();
        assert_eq!(back_colour, colour);
    }
}
