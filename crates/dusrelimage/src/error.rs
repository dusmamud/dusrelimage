//! Stable error codes, in English and Hindi — the same contract shape
//! as dusrelpdf / indycheck.
//!
//! Every public function in this crate returns `Result<_, DusrelError>`
//! and never panics on bad input.

use std::fmt;

/// Stable, machine-readable error code (SPEC section 4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ErrorCode {
    /// Input bytes are not a recognisable image, or are truncated/corrupt.
    InvalidImage,
    /// The image format is known but not supported by this build
    /// (e.g. AVIF input without the `avif-native` decoder, TIFF output).
    UnsupportedFormat,
    /// The image exceeds `CommonOptions.max_pixels` / `max_dimension`.
    ImageTooLarge,
    /// Even the lowest quality still exceeds the requested target size.
    TargetSizeUnreachable,
    /// A crop rectangle sticks out of the image.
    CropOutOfBounds,
    /// A pixel coordinate is outside the image.
    PixelOutOfBounds,
    /// A quality value is outside 1-100.
    InvalidQuality,
    /// A dimension is zero, missing, or otherwise unusable.
    InvalidDimensions,
    /// An argument is wrong (bad value, conflicting options, ...).
    InvalidArgument,
    /// An input list / byte buffer is empty where input is required.
    EmptyInput,
    /// A non-image / wrong-typed value reached a typed API (bindings layer).
    InvalidType,
}

impl ErrorCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            ErrorCode::InvalidImage => "INVALID_IMAGE",
            ErrorCode::UnsupportedFormat => "UNSUPPORTED_FORMAT",
            ErrorCode::ImageTooLarge => "IMAGE_TOO_LARGE",
            ErrorCode::TargetSizeUnreachable => "TARGET_SIZE_UNREACHABLE",
            ErrorCode::CropOutOfBounds => "CROP_OUT_OF_BOUNDS",
            ErrorCode::PixelOutOfBounds => "PIXEL_OUT_OF_BOUNDS",
            ErrorCode::InvalidQuality => "INVALID_QUALITY",
            ErrorCode::InvalidDimensions => "INVALID_DIMENSIONS",
            ErrorCode::InvalidArgument => "INVALID_ARGUMENT",
            ErrorCode::EmptyInput => "EMPTY_INPUT",
            ErrorCode::InvalidType => "INVALID_TYPE",
        }
    }
}

/// An error carrying a stable code plus human-readable reasons in
/// English and Hindi.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DusrelError {
    pub code: ErrorCode,
    pub message_en: String,
    pub message_hi: String,
}

impl DusrelError {
    pub fn new(
        code: ErrorCode,
        message_en: impl Into<String>,
        message_hi: impl Into<String>,
    ) -> Self {
        Self {
            code,
            message_en: message_en.into(),
            message_hi: message_hi.into(),
        }
    }

    /// Stable code string, e.g. `"INVALID_IMAGE"`.
    pub fn code_str(&self) -> &'static str {
        self.code.as_str()
    }

    /// English message (same value as the `message_en` field).
    pub fn reason_en(&self) -> &str {
        &self.message_en
    }

    /// Hindi message (same value as the `message_hi` field).
    pub fn reason_hi(&self) -> &str {
        &self.message_hi
    }

    /// English message (same value as the `message_en` field).
    pub fn message_en(&self) -> &str {
        &self.message_en
    }

    /// Hindi message (same value as the `message_hi` field).
    pub fn message_hi(&self) -> &str {
        &self.message_hi
    }

    pub fn invalid_image(detail_en: &str) -> Self {
        Self::new(
            ErrorCode::InvalidImage,
            format!("Invalid image: {detail_en}"),
            format!("Galat image hai: {detail_en}"),
        )
    }

    pub fn unsupported_format(format_name: &str) -> Self {
        Self::new(
            ErrorCode::UnsupportedFormat,
            format!("Unsupported image format: {format_name}."),
            format!("Ye image format support nahi hai: {format_name}."),
        )
    }

    pub fn image_too_large(width: u32, height: u32, max_pixels: u64, max_dimension: u32) -> Self {
        Self::new(
            ErrorCode::ImageTooLarge,
            format!(
                "Image is too large: {width}x{height} ({} pixels) exceeds the limit of {max_pixels} pixels / {max_dimension} px per side.",
                width as u64 * height as u64
            ),
            format!(
                "Image bahut badi hai: {width}x{height} ({} pixels) limit se zyada hai — {max_pixels} pixels / {max_dimension} px per side.",
                width as u64 * height as u64
            ),
        )
    }

    pub fn target_size_unreachable(target_bytes: u64, smallest_bytes: u64) -> Self {
        Self::new(
            ErrorCode::TargetSizeUnreachable,
            format!(
                "Target size of {target_bytes} bytes cannot be reached: even the lowest quality produces {smallest_bytes} bytes."
            ),
            format!(
                "Target size {target_bytes} bytes tak nahi pahunch sakte: sabse low quality par bhi {smallest_bytes} bytes bante hain."
            ),
        )
    }

    pub fn crop_out_of_bounds(
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        img_width: u32,
        img_height: u32,
    ) -> Self {
        Self::new(
            ErrorCode::CropOutOfBounds,
            format!(
                "Crop rectangle ({x}, {y}, {width}x{height}) is outside the image ({img_width}x{img_height})."
            ),
            format!(
                "Crop rectangle ({x}, {y}, {width}x{height}) image ({img_width}x{img_height}) se bahar hai."
            ),
        )
    }

    pub fn pixel_out_of_bounds(x: u32, y: u32, img_width: u32, img_height: u32) -> Self {
        Self::new(
            ErrorCode::PixelOutOfBounds,
            format!("Pixel ({x}, {y}) is outside the image ({img_width}x{img_height})."),
            format!("Pixel ({x}, {y}) image ({img_width}x{img_height}) se bahar hai."),
        )
    }

    pub fn invalid_quality(quality: u8) -> Self {
        Self::new(
            ErrorCode::InvalidQuality,
            format!("Quality {quality} is invalid. Quality must be between 1 and 100."),
            format!("Quality {quality} galat hai. Quality 1 se 100 ke beech honi chahiye."),
        )
    }

    pub fn invalid_dimensions(detail_en: &str) -> Self {
        Self::new(
            ErrorCode::InvalidDimensions,
            format!("Invalid dimensions: {detail_en}"),
            format!("Galat dimensions hain: {detail_en}"),
        )
    }

    pub fn invalid_argument(reason_en: &str, reason_hi: &str) -> Self {
        Self::new(ErrorCode::InvalidArgument, reason_en, reason_hi)
    }

    pub fn empty_input(what: &str) -> Self {
        Self::new(
            ErrorCode::EmptyInput,
            format!("{what} is empty."),
            format!("{what} khaali hai."),
        )
    }

    pub fn invalid_type(detail_en: &str) -> Self {
        Self::new(
            ErrorCode::InvalidType,
            format!("Invalid type: {detail_en}"),
            format!("Galat type hai: {detail_en}"),
        )
    }
}

impl fmt::Display for DusrelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code.as_str(), self.message_en)
    }
}

impl std::error::Error for DusrelError {}

/// Map `image`-crate failures onto stable codes. Decoding failures are
/// bad input (`INVALID_IMAGE`); a format the build cannot handle is
/// `UNSUPPORTED_FORMAT`; anything else is an argument / size problem.
impl From<image::ImageError> for DusrelError {
    fn from(err: image::ImageError) -> Self {
        use image::ImageError;
        match err {
            ImageError::Decoding(e) => DusrelError::invalid_image(&e.to_string()),
            ImageError::Encoding(e) => DusrelError::invalid_argument(
                &format!("Image encoding failed: {e}"),
                "Image encode nahi ho payi.",
            ),
            ImageError::IoError(e) => {
                DusrelError::invalid_image(&format!("could not read image data: {e}"))
            }
            ImageError::Unsupported(e) => DusrelError::unsupported_format(&e.to_string()),
            ImageError::Limits(_) => DusrelError::new(
                ErrorCode::ImageTooLarge,
                "Image exceeds the decoder size limit.",
                "Image decoder ki size limit se badi hai.",
            ),
            ImageError::Parameter(e) => DusrelError::invalid_argument(
                &format!("Invalid image parameter: {e}"),
                "Image ka parameter galat hai.",
            ),
        }
    }
}

impl From<std::io::Error> for DusrelError {
    fn from(err: std::io::Error) -> Self {
        DusrelError::invalid_image(&format!("could not read image data: {err}"))
    }
}

/// Convenience result type for the whole crate.
pub type Result<T> = std::result::Result<T, DusrelError>;
