//! Tool 4 — converter (SPEC section 1).
//!
//! Re-encode into another format. When the target has no alpha channel
//! (JPEG/BMP) and the source does, the image is flattened onto
//! `ConvertOptions.background` first. GIF output is the first frame
//! only for animated input (the decode already yields one frame).

use crate::common::{EncodeSettings, decode_image, encode_image, flatten_alpha};
use crate::error::{DusrelError, Result};
use crate::options::ConvertOptions;

/// Convert `input` to `options.format` at `options.quality`.
pub fn convert_image(input: &[u8], options: &ConvertOptions) -> Result<Vec<u8>> {
    if options.quality == 0 || options.quality > 100 {
        return Err(DusrelError::invalid_quality(options.quality));
    }
    let decoded = decode_image(input, &options.common)?;
    let mut image = decoded.image;
    if !options.format.supports_alpha() && image.color().has_alpha() {
        image = flatten_alpha(&image, &options.background);
    }
    encode_image(
        &image,
        options.format,
        &EncodeSettings {
            quality: options.quality,
            lossless: false,
            effort: 4,
            background: options.background.clone(),
        },
    )
}
