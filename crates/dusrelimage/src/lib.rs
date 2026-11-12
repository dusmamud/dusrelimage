//! dusrelimage — offline image toolkit (v0.1.0, Phase 1: all 10 tools).
//!
//! Compress, resize, crop, convert, watermark, annotate, colour
//! picker, palette, collage and pixelate: bytes in, bytes out, no
//! network, no uploads. Every operation takes an options struct (each
//! embedding `CommonOptions`) and returns `Result<_, DusrelError>` —
//! never panics on bad input, with stable error codes in English and
//! Hindi (SPEC section 4). `bulk_process` runs one op over many
//! inputs, one result per input, never aborting the batch.
//!
//! Honest limits of v0.1.0 (see also `common`):
//! * AVIF input needs the `avif-native` decoder (libdav1d, C library),
//!   deliberately not enabled — AVIF input is `UNSUPPORTED_FORMAT`.
//!   AVIF output works (lossy, 8-bit).
//! * WebP output is always lossless (`image` 0.25 encoder); GIF output
//!   is the first frame only; `keep_exif` / `dpi` / `progressive` are
//!   accepted but not honoured yet (encoder limits).
//! * Whole images live in memory; there is no streaming mode.

pub mod bulk;
pub mod common;
mod draw;
pub mod error;
pub mod fixtures;
pub mod ops_annotate;
pub mod ops_collage;
pub mod ops_colour;
pub mod ops_compress;
pub mod ops_convert;
pub mod ops_crop;
pub mod ops_pixelate;
pub mod ops_resize;
pub mod ops_watermark;
pub mod options;
pub mod types;

pub use bulk::{BulkItemResult, BulkOp, BulkOutput, bulk_process};
pub use common::get_image_info;
pub use error::{DusrelError, ErrorCode, Result};
pub use ops_annotate::annotate_image;
pub use ops_collage::make_collage;
pub use ops_colour::{extract_palette, pick_colour};
pub use ops_compress::{compress_image, compress_to_size};
pub use ops_convert::convert_image;
pub use ops_crop::crop_image;
pub use ops_pixelate::pixelate_image;
pub use ops_resize::resize_image;
pub use ops_watermark::add_watermark;
pub use options::{
    CollageOptions, CommonOptions, CompressOptions, ConvertOptions, CropOptions, PixelateOptions,
    ResizeOptions, WatermarkOptions,
};
pub use types::{
    Annotation, CollageLayout, Colour, Filter, FitMode, ImageFormat, ImageInfo, ImageWatermark,
    PaletteColour, Position, TextWatermark,
};

/// Crate version (lockstep with the npm / PyPI packages).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
