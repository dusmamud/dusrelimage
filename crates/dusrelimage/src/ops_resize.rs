//! Tool 2 — resizer (SPEC section 1).
//!
//! Width and/or height, percent scaling, aspect lock, and the four
//! fit modes. The output keeps the input format (TIFF input falls
//! back to PNG) and is re-encoded at quality 80.

use image::{DynamicImage, GenericImageView, RgbaImage};

use crate::common::{EncodeSettings, check_limits, decode_image, encode_image};
use crate::error::{DusrelError, Result};
use crate::options::ResizeOptions;
use crate::types::FitMode;

/// Work out the target size. `percent` is in percent points
/// (`100.0` = original size) and cannot be combined with
/// `width`/`height`. With only one of `width`/`height`, the other
/// follows the aspect ratio.
fn target_dimensions(
    src_width: u32,
    src_height: u32,
    options: &ResizeOptions,
) -> Result<(u32, u32)> {
    if let Some(percent) = options.percent {
        if !percent.is_finite() || percent <= 0.0 {
            return Err(DusrelError::invalid_argument(
                &format!("Percent {percent} is invalid. Percent must be a positive number."),
                "Percent galat hai. Percent ek positive number hona chahiye.",
            ));
        }
        if options.width.is_some() || options.height.is_some() {
            return Err(DusrelError::invalid_argument(
                "Percent cannot be combined with width/height. Set either percent or width/height.",
                "Percent ko width/height ke saath nahi de sakte. Ya to percent dein ya width/height.",
            ));
        }
        let width = ((src_width as f32 * percent / 100.0).round() as u32).max(1);
        let height = ((src_height as f32 * percent / 100.0).round() as u32).max(1);
        return Ok((width, height));
    }
    match (options.width, options.height) {
        (None, None) => Err(DusrelError::invalid_dimensions(
            "no target size given: set width, height, or percent",
        )),
        (Some(0), _) | (_, Some(0)) => Err(DusrelError::invalid_dimensions(
            "width and height must be greater than 0",
        )),
        (Some(width), None) => {
            let height = ((src_height as u64 * width as u64) / src_width as u64).max(1) as u32;
            Ok((width, height))
        }
        (None, Some(height)) => {
            let width = ((src_width as u64 * height as u64) / src_height as u64).max(1) as u32;
            Ok((width, height))
        }
        (Some(width), Some(height)) => Ok((width, height)),
    }
}

/// Resize `input` per `options`.
pub fn resize_image(input: &[u8], options: &ResizeOptions) -> Result<Vec<u8>> {
    let decoded = decode_image(input, &options.common)?;
    let (src_width, src_height) = decoded.image.dimensions();
    let (target_width, target_height) = target_dimensions(src_width, src_height, options)?;
    check_limits(target_width, target_height, &options.common)?;

    let filter = options.filter.to_image_filter();
    let resized: DynamicImage = match options.fit {
        FitMode::Fit => decoded.image.resize(target_width, target_height, filter),
        FitMode::Stretch => decoded
            .image
            .resize_exact(target_width, target_height, filter),
        FitMode::Fill => {
            // Letterbox: fit inside, then centre on a background canvas.
            let fitted = decoded.image.resize(target_width, target_height, filter);
            let (fitted_width, fitted_height) = fitted.dimensions();
            let fitted_rgba = fitted.to_rgba8();
            let mut canvas = RgbaImage::new(target_width, target_height);
            for px in canvas.pixels_mut() {
                *px = image::Rgba(options.background.to_rgba());
            }
            let offset_x = (target_width - fitted_width) / 2;
            let offset_y = (target_height - fitted_height) / 2;
            for (x, y, px) in fitted_rgba.enumerate_pixels() {
                canvas.put_pixel(offset_x + x, offset_y + y, *px);
            }
            DynamicImage::ImageRgba8(canvas)
        }
        FitMode::Crop => {
            // Cover: scale so the box is covered, then centre-crop.
            let scale = (target_width as f32 / src_width as f32)
                .max(target_height as f32 / src_height as f32);
            let cover_width = ((src_width as f32 * scale).ceil() as u32).max(target_width);
            let cover_height = ((src_height as f32 * scale).ceil() as u32).max(target_height);
            let covered = decoded
                .image
                .resize_exact(cover_width, cover_height, filter);
            let x = (cover_width - target_width) / 2;
            let y = (cover_height - target_height) / 2;
            covered.crop_imm(x, y, target_width, target_height)
        }
    };

    let format = decoded.output_format(None);
    encode_image(
        &resized,
        format,
        &EncodeSettings {
            quality: 80,
            ..EncodeSettings::default()
        },
    )
}
