//! Shared decode / encode / info helpers used by every tool.
//!
//! Bytes in, bytes out. Decode applies EXIF orientation when
//! `CommonOptions.auto_orient` is set (native `image` 0.25.8+ support —
//! `ImageDecoder::orientation()` + `DynamicImage::apply_orientation()` —
//! no separate `exif` crate). Every function returns `Result<_, DusrelError>`
//! and never panics on bad input.
//!
//! Honest v0.1.0 limits (documented here so bindings don't promise more):
//! * AVIF input decodes only with the `avif-native` feature (libdav1d,
//!   C library), which this crate deliberately does NOT enable — it
//!   would break the WASM build. AVIF input returns `UNSUPPORTED_FORMAT`.
//!   AVIF *output* works (lossy, 8-bit, via ravif — pure Rust).
//! * WebP output is always lossless (`image` 0.25 WebP encoder); the
//!   `quality` / `lossless` settings are accepted but do not change it.
//! * GIF output is the first frame only for animated input.
//! * `CommonOptions.keep_exif = true` and `CommonOptions.dpi` are
//!   accepted but not honoured yet — the `image` 0.25 encoders expose
//!   no metadata/DPI settings, so output is always stripped.
//! * The whole image lives in memory; there is no streaming mode.

use std::io::Cursor;

use image::codecs::avif::AvifEncoder;
use image::codecs::bmp::BmpEncoder;
use image::codecs::gif::GifEncoder;
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::{CompressionType, FilterType as PngFilterType, PngEncoder};
use image::codecs::webp::WebPEncoder;
use image::metadata::Orientation;
use image::{DynamicImage, GenericImageView, ImageDecoder, ImageReader};
use image::{Rgb, RgbImage};

use crate::error::{DusrelError, Result};
use crate::types::{Colour, ImageFormat, ImageInfo};

/// A decoded image plus its detected format.
///
/// `format` is `Some` for the six output formats; it is `None` for
/// decodable inputs outside that set (notably TIFF — SPEC section 3
/// input). `format_name` is always the detected lowercase format name
/// (`"jpeg"`, `"png"`, `"tiff"`, ...).
#[derive(Debug)]
pub struct DecodedImage {
    pub image: DynamicImage,
    pub format: Option<ImageFormat>,
    pub format_name: String,
}

impl DecodedImage {
    /// Resolve the output format for an op: an explicitly requested
    /// format wins, then the input format, then PNG as the safe
    /// lossless fallback (e.g. for TIFF input).
    pub fn output_format(&self, requested: Option<ImageFormat>) -> ImageFormat {
        requested.or(self.format).unwrap_or(ImageFormat::Png)
    }
}

/// Settings for [`encode_image`]. Built from the per-op options structs.
#[derive(Debug, Clone, PartialEq)]
pub struct EncodeSettings {
    /// Quality 1-100 (JPEG/AVIF direct; PNG level-mapped; ignored for
    /// WebP/GIF/BMP).
    pub quality: u8,
    /// Lossless output (WebP/PNG only — enforced by the callers).
    pub lossless: bool,
    /// Encoding effort 0-6 (AVIF speed = 10 - effort).
    pub effort: u8,
    /// Background used to flatten alpha for formats without it
    /// (JPEG/BMP).
    pub background: Colour,
}

impl Default for EncodeSettings {
    fn default() -> Self {
        Self {
            quality: 80,
            lossless: false,
            effort: 4,
            background: Colour::white(),
        }
    }
}

/// Refuse images larger than the caller's limits.
pub fn check_limits(
    width: u32,
    height: u32,
    options: &crate::options::CommonOptions,
) -> Result<()> {
    let pixels = width as u64 * height as u64;
    if pixels > options.max_pixels
        || width > options.max_dimension
        || height > options.max_dimension
    {
        return Err(DusrelError::image_too_large(
            width,
            height,
            options.max_pixels,
            options.max_dimension,
        ));
    }
    Ok(())
}

/// Detect the format name of `input` (`"jpeg"`, `"png"`, `"tiff"`, ...).
///
/// `EMPTY_INPUT` for empty bytes, `INVALID_IMAGE` when the bytes match
/// no known image format.
pub fn detect_format_name(input: &[u8]) -> Result<String> {
    if input.is_empty() {
        return Err(DusrelError::empty_input("image bytes"));
    }
    match image::guess_format(input) {
        Ok(format) => Ok(format!("{format:?}").to_lowercase()),
        Err(_) => Err(DusrelError::invalid_image(
            "the bytes do not match any known image format",
        )),
    }
}

/// Decode bytes into a [`DynamicImage`]:
/// detect the format, enforce `CommonOptions` size limits *before* the
/// full decode (via the decoder header), and apply EXIF orientation
/// when `auto_orient` is set.
///
/// Errors: `EMPTY_INPUT`, `INVALID_IMAGE` (garbage/truncated bytes),
/// `UNSUPPORTED_FORMAT` (a format this build cannot decode, e.g. AVIF
/// without `avif-native`), `IMAGE_TOO_LARGE`.
pub fn decode_image(input: &[u8], options: &crate::options::CommonOptions) -> Result<DecodedImage> {
    if input.is_empty() {
        return Err(DusrelError::empty_input("image bytes"));
    }
    let guessed = image::guess_format(input)
        .map_err(|_| DusrelError::invalid_image("the bytes do not match any known image format"))?;
    let format_name = format!("{guessed:?}").to_lowercase();
    let format = ImageFormat::from_image_format(guessed);

    let reader = ImageReader::new(Cursor::new(input.to_vec()))
        .with_guessed_format()
        .map_err(DusrelError::from)?;
    let mut decoder = reader.into_decoder().map_err(DusrelError::from)?;

    // Size gate before allocating the full pixel buffer.
    let (width, height) = decoder.dimensions();
    check_limits(width, height, options)?;

    let orientation = if options.auto_orient {
        decoder.orientation().unwrap_or(Orientation::NoTransforms)
    } else {
        Orientation::NoTransforms
    };
    let mut image = DynamicImage::from_decoder(decoder).map_err(DusrelError::from)?;
    // NoTransforms is a no-op inside apply_orientation.
    image.apply_orientation(orientation);

    Ok(DecodedImage {
        image,
        format,
        format_name,
    })
}

/// Map a JPEG-style quality 1-100 onto the PNG compression level the
/// `image` crate exposes. Higher "quality" = stronger compression =
/// smaller file (PNG is lossless, so this trades time for size).
fn png_compression(quality: u8) -> CompressionType {
    if quality >= 67 {
        CompressionType::Best
    } else if quality >= 34 {
        CompressionType::Default
    } else {
        CompressionType::Fast
    }
}

/// Map effort 0-6 onto the AVIF encoder speed 1-10 (10 = fastest).
fn avif_speed(effort: u8) -> u8 {
    10u8.saturating_sub(effort).clamp(1, 10)
}

/// Composite an image with alpha onto `background`, returning RGB8.
/// Used for formats without alpha (JPEG/BMP). Images without alpha are
/// returned as RGB8 unchanged apart from the conversion.
pub fn flatten_alpha(image: &DynamicImage, background: &Colour) -> DynamicImage {
    let rgba = image.to_rgba8();
    let (width, height) = rgba.dimensions();
    let mut out = RgbImage::new(width, height);
    for (x, y, px) in rgba.enumerate_pixels() {
        let a = px[3] as f32 / 255.0;
        let r = (px[0] as f32 * a + background.r as f32 * (1.0 - a)).round() as u8;
        let g = (px[1] as f32 * a + background.g as f32 * (1.0 - a)).round() as u8;
        let b = (px[2] as f32 * a + background.b as f32 * (1.0 - a)).round() as u8;
        out.put_pixel(x, y, Rgb([r, g, b]));
    }
    DynamicImage::ImageRgb8(out)
}

/// Encode a [`DynamicImage`] to bytes in `format`.
///
/// * JPEG: `JpegEncoder::new_with_quality`, alpha flattened onto
///   `settings.background`.
/// * PNG: quality mapped to `Best`/`Default`/`Fast` compression.
/// * WebP: always lossless (`WebPEncoder::new_lossless`) — the
///   `image` 0.25 encoder has no lossy mode; quality is ignored.
/// * AVIF: lossy 8-bit, `AvifEncoder::new_with_speed_quality`.
/// * GIF: first frame only (the input is already a single frame).
/// * BMP: alpha flattened onto `settings.background`.
///
/// Errors: `INVALID_QUALITY` for quality outside 1-100,
/// `INVALID_ARGUMENT` for encoder failures.
pub fn encode_image(
    image: &DynamicImage,
    format: ImageFormat,
    settings: &EncodeSettings,
) -> Result<Vec<u8>> {
    if settings.quality == 0 || settings.quality > 100 {
        return Err(DusrelError::invalid_quality(settings.quality));
    }
    let mut buf = Cursor::new(Vec::new());
    match format {
        ImageFormat::Jpeg => {
            let rgb = flatten_alpha(image, &settings.background);
            let encoder = JpegEncoder::new_with_quality(&mut buf, settings.quality);
            rgb.write_with_encoder(encoder).map_err(DusrelError::from)?;
        }
        ImageFormat::Png => {
            let encoder = PngEncoder::new_with_quality(
                &mut buf,
                png_compression(settings.quality),
                PngFilterType::Adaptive,
            );
            image
                .write_with_encoder(encoder)
                .map_err(DusrelError::from)?;
        }
        ImageFormat::WebP => {
            let rgba = DynamicImage::ImageRgba8(image.to_rgba8());
            let encoder = WebPEncoder::new_lossless(&mut buf);
            rgba.write_with_encoder(encoder)
                .map_err(DusrelError::from)?;
        }
        ImageFormat::Avif => {
            let rgba = DynamicImage::ImageRgba8(image.to_rgba8());
            let encoder = AvifEncoder::new_with_speed_quality(
                &mut buf,
                avif_speed(settings.effort),
                settings.quality,
            );
            rgba.write_with_encoder(encoder)
                .map_err(DusrelError::from)?;
        }
        ImageFormat::Gif => {
            let rgba = DynamicImage::ImageRgba8(image.to_rgba8());
            let encoder = GifEncoder::new(&mut buf);
            rgba.write_with_encoder(encoder)
                .map_err(DusrelError::from)?;
        }
        ImageFormat::Bmp => {
            let rgb = flatten_alpha(image, &settings.background);
            let encoder = BmpEncoder::new(&mut buf);
            rgb.write_with_encoder(encoder).map_err(DusrelError::from)?;
        }
    }
    Ok(buf.into_inner())
}

/// Describe an image without transforming it (SPEC section 1).
/// Dimensions are post-orientation when `auto_orient` is on.
pub fn get_image_info(input: &[u8], options: &crate::options::CommonOptions) -> Result<ImageInfo> {
    let decoded = decode_image(input, options)?;
    let (width, height) = decoded.image.dimensions();
    Ok(ImageInfo {
        format: decoded.format_name,
        width,
        height,
        has_alpha: decoded.image.color().has_alpha(),
        colour_type: format!("{:?}", decoded.image.color()).to_lowercase(),
        file_size: input.len() as u64,
    })
}
