//! Tool 10 — pixelator (SPEC section 1).
//!
//! Downscale the rectangle with a Nearest filter, then upscale it
//! back with Nearest — the rect only, the rest of the image untouched.
//! Block size 2-100. A `width`/`height` of 0 means "to the right /
//! bottom edge of the image" (see `PixelateOptions`); the default
//! options therefore pixelate the whole image. A rectangle outside
//! the image is `INVALID_ARGUMENT`.

use image::GenericImageView;

use crate::common::{EncodeSettings, decode_image, encode_image};
use crate::draw::blend_image_onto;
use crate::error::{DusrelError, Result};
use crate::options::PixelateOptions;

/// Pixelate the rectangle in `options` inside `input`.
pub fn pixelate_image(input: &[u8], options: &PixelateOptions) -> Result<Vec<u8>> {
    if options.block_size < 2 || options.block_size > 100 {
        return Err(DusrelError::invalid_argument(
            &format!(
                "Block size {} is invalid. Block size must be between 2 and 100.",
                options.block_size
            ),
            &format!(
                "Block size {} galat hai. Block size 2 se 100 ke beech hona chahiye.",
                options.block_size
            ),
        ));
    }
    let decoded = decode_image(input, &options.common)?;
    let (img_width, img_height) = decoded.image.dimensions();
    let width = if options.width == 0 {
        img_width.saturating_sub(options.x)
    } else {
        options.width
    };
    let height = if options.height == 0 {
        img_height.saturating_sub(options.y)
    } else {
        options.height
    };
    if options.x >= img_width
        || options.y >= img_height
        || width == 0
        || height == 0
        || options.x as u64 + width as u64 > img_width as u64
        || options.y as u64 + height as u64 > img_height as u64
    {
        return Err(DusrelError::invalid_argument(
            &format!(
                "Pixelate rectangle ({}, {}, {}x{}) is outside the image ({}x{}).",
                options.x, options.y, width, height, img_width, img_height
            ),
            &format!(
                "Pixelate rectangle ({}, {}, {}x{}) image ({}x{}) se bahar hai.",
                options.x, options.y, width, height, img_width, img_height
            ),
        ));
    }

    let mut rgba = decoded.image.to_rgba8();
    let block = options.block_size;
    let small_width = width.div_ceil(block).max(1);
    let small_height = height.div_ceil(block).max(1);
    let sub = image::imageops::crop_imm(&rgba, options.x, options.y, width, height).to_image();
    let small = image::imageops::resize(
        &sub,
        small_width,
        small_height,
        image::imageops::FilterType::Nearest,
    );
    let big = image::imageops::resize(&small, width, height, image::imageops::FilterType::Nearest);
    blend_image_onto(&mut rgba, &big, options.x as i32, options.y as i32);

    let format = decoded.output_format(None);
    let out = image::DynamicImage::ImageRgba8(rgba);
    encode_image(
        &out,
        format,
        &EncodeSettings {
            quality: 80,
            ..EncodeSettings::default()
        },
    )
}
