//! Tool 5 — watermark (SPEC section 1).
//!
//! Text (bundled DejaVu Sans via `ab_glyph`, see [`crate::draw`]) or
//! a logo image, at an anchor position (`Position::Tiled` repeats the
//! watermark across the whole image), with opacity 0-1 and an optional
//! rotation in degrees. Text and logo may be combined (text first,
//! then logo). The output keeps the input format (TIFF input falls
//! back to PNG) and is re-encoded at quality 80.

use image::GenericImageView;
use image::imageops::FilterType;

use crate::common::{EncodeSettings, decode_image, encode_image};
use crate::draw::{
    anchor_offset, apply_opacity, blend_image_onto, render_text_layer, rotate_layer, tile_layer,
};
use crate::error::{DusrelError, Result};
use crate::options::WatermarkOptions;
use crate::types::Position;

fn validate_options(options: &WatermarkOptions) -> Result<()> {
    if options.text.is_none() && options.logo.is_none() {
        return Err(DusrelError::invalid_argument(
            "No watermark given: set either text or a logo image.",
            "Koi watermark nahi diya: text ya logo image dein.",
        ));
    }
    if !options.opacity.is_finite() || !(0.0..=1.0).contains(&options.opacity) {
        return Err(DusrelError::invalid_argument(
            &format!(
                "Opacity {} is invalid. Opacity must be between 0 and 1.",
                options.opacity
            ),
            &format!(
                "Opacity {} galat hai. Opacity 0 se 1 ke beech honi chahiye.",
                options.opacity
            ),
        ));
    }
    if !options.rotation_deg.is_finite() {
        return Err(DusrelError::invalid_argument(
            "Rotation must be a finite number of degrees.",
            "Rotation ek finite number (degrees) honi chahiye.",
        ));
    }
    if let Some(text) = &options.text {
        if text.text.is_empty() {
            return Err(DusrelError::invalid_argument(
                "Watermark text is empty.",
                "Watermark text khaali hai.",
            ));
        }
        if !text.size.is_finite() || text.size <= 0.0 {
            return Err(DusrelError::invalid_argument(
                &format!(
                    "Watermark font size {} is invalid. Font size must be greater than 0.",
                    text.size
                ),
                "Watermark font size galat hai. Font size 0 se badi honi chahiye.",
            ));
        }
    }
    if let Some(logo) = &options.logo {
        if logo.image.is_empty() {
            return Err(DusrelError::empty_input("logo image bytes"));
        }
        if !logo.scale.is_finite() || logo.scale <= 0.0 {
            return Err(DusrelError::invalid_argument(
                &format!(
                    "Logo scale {} is invalid. Logo scale must be greater than 0.",
                    logo.scale
                ),
                "Logo scale galat hai. Logo scale 0 se bada hona chahiye.",
            ));
        }
    }
    Ok(())
}

/// Add a text and/or logo watermark to `input`.
pub fn add_watermark(input: &[u8], options: &WatermarkOptions) -> Result<Vec<u8>> {
    validate_options(options)?;
    let decoded = decode_image(input, &options.common)?;
    let mut rgba = decoded.image.to_rgba8();
    let (img_width, img_height) = rgba.dimensions();

    if let Some(text) = &options.text {
        let mut layer = render_text_layer(&text.text, text.size, &text.colour, options.opacity)?;
        if options.rotation_deg != 0.0 {
            layer = rotate_layer(&layer, options.rotation_deg);
        }
        place_layer(&mut rgba, &layer, options, img_width, img_height);
    }

    if let Some(logo) = &options.logo {
        let logo_decoded = decode_image(&logo.image, &options.common)?;
        let (logo_width, logo_height) = logo_decoded.image.dimensions();
        let scaled_width = ((logo_width as f32 * logo.scale).round() as u32).max(1);
        let scaled_height = ((logo_height as f32 * logo.scale).round() as u32).max(1);
        crate::common::check_limits(scaled_width, scaled_height, &options.common)?;
        let resized =
            logo_decoded
                .image
                .resize_exact(scaled_width, scaled_height, FilterType::Lanczos3);
        let mut layer = resized.to_rgba8();
        apply_opacity(&mut layer, options.opacity);
        if options.rotation_deg != 0.0 {
            layer = rotate_layer(&layer, options.rotation_deg);
        }
        place_layer(&mut rgba, &layer, options, img_width, img_height);
    }

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

/// Place `layer` on `image` at `options.position` (or tiled).
fn place_layer(
    image: &mut image::RgbaImage,
    layer: &image::RgbaImage,
    options: &WatermarkOptions,
    img_width: u32,
    img_height: u32,
) {
    if options.position == Position::Tiled {
        tile_layer(image, layer, options.margin);
        return;
    }
    let (layer_width, layer_height) = layer.dimensions();
    let (x, y) = anchor_offset(
        options.position,
        img_width,
        img_height,
        layer_width,
        layer_height,
        options.margin,
    );
    blend_image_onto(image, layer, x, y);
}
