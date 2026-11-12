//! Tool 3 — cropper (SPEC section 1).
//!
//! Explicit `x`/`y` rectangle, or an anchor preset
//! (`CropOptions.anchor`) that places a `width` x `height` rectangle.
//! Out-of-bounds is `CROP_OUT_OF_BOUNDS` — never a panic, never a
//! silent clamp.

use image::GenericImageView;

use crate::common::{EncodeSettings, decode_image, encode_image};
use crate::error::{DusrelError, Result};
use crate::options::CropOptions;
use crate::types::Position;

/// Resolve the crop origin. `anchor = None` uses the explicit `x`/`y`;
/// a preset computes the origin from the image size (`Tiled` is
/// treated as `Center`).
fn crop_origin(
    anchor: Option<Position>,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    img_width: u32,
    img_height: u32,
) -> (u32, u32) {
    match anchor {
        None => (x, y),
        Some(Position::TopLeft) => (0, 0),
        Some(Position::TopCenter) => (img_width.saturating_sub(width) / 2, 0),
        Some(Position::TopRight) => (img_width.saturating_sub(width), 0),
        Some(Position::CenterLeft) => (0, img_height.saturating_sub(height) / 2),
        Some(Position::Center) | Some(Position::Tiled) => (
            img_width.saturating_sub(width) / 2,
            img_height.saturating_sub(height) / 2,
        ),
        Some(Position::CenterRight) => (
            img_width.saturating_sub(width),
            img_height.saturating_sub(height) / 2,
        ),
        Some(Position::BottomLeft) => (0, img_height.saturating_sub(height)),
        Some(Position::BottomCenter) => (
            img_width.saturating_sub(width) / 2,
            img_height.saturating_sub(height),
        ),
        Some(Position::BottomRight) => (
            img_width.saturating_sub(width),
            img_height.saturating_sub(height),
        ),
    }
}

/// Crop `input` to the requested rectangle.
pub fn crop_image(input: &[u8], options: &CropOptions) -> Result<Vec<u8>> {
    if options.width == 0 || options.height == 0 {
        return Err(DusrelError::invalid_dimensions(
            "crop width and height must be greater than 0",
        ));
    }
    let decoded = decode_image(input, &options.common)?;
    let (img_width, img_height) = decoded.image.dimensions();
    let (x, y) = crop_origin(
        options.anchor,
        options.x,
        options.y,
        options.width,
        options.height,
        img_width,
        img_height,
    );
    if x as u64 + options.width as u64 > img_width as u64
        || y as u64 + options.height as u64 > img_height as u64
    {
        return Err(DusrelError::crop_out_of_bounds(
            x,
            y,
            options.width,
            options.height,
            img_width,
            img_height,
        ));
    }
    let cropped = decoded.image.crop_imm(x, y, options.width, options.height);
    let format = decoded.output_format(None);
    encode_image(
        &cropped,
        format,
        &EncodeSettings {
            quality: 80,
            ..EncodeSettings::default()
        },
    )
}
