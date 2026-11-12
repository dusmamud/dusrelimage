//! Tool 1 — compressor (SPEC section 1).
//!
//! `compress_image` re-encodes at a fixed quality;
//! `compress_to_size` binary-searches the quality (max 10 encodes
//! total: 9 search steps + one quality-1 retry) for
//! the best quality that still fits the target size.

use crate::common::{EncodeSettings, decode_image, encode_image};
use crate::error::{DusrelError, Result};
use crate::options::CompressOptions;
use crate::types::ImageFormat;

fn validate_options(options: &CompressOptions) -> Result<()> {
    if options.quality == 0 || options.quality > 100 {
        return Err(DusrelError::invalid_quality(options.quality));
    }
    if options.effort > 6 {
        return Err(DusrelError::invalid_argument(
            &format!(
                "Effort {} is invalid. Effort must be between 0 and 6.",
                options.effort
            ),
            &format!(
                "Effort {} galat hai. Effort 0 se 6 ke beech hona chahiye.",
                options.effort
            ),
        ));
    }
    Ok(())
}

fn validate_lossless(options: &CompressOptions, format: ImageFormat) -> Result<()> {
    if options.lossless && !matches!(format, ImageFormat::WebP | ImageFormat::Png) {
        return Err(DusrelError::invalid_argument(
            &format!(
                "Lossless output is only supported for WebP and PNG, not {}.",
                format.as_str()
            ),
            "Lossless output sirf WebP aur PNG ke liye hai.",
        ));
    }
    Ok(())
}

fn settings_for(options: &CompressOptions, quality: u8) -> EncodeSettings {
    EncodeSettings {
        quality,
        lossless: options.lossless,
        effort: options.effort,
        background: crate::types::Colour::white(),
    }
}

/// Re-encode `input` at `options.quality` (and `options.format`, or the
/// input format when unset). See [`crate::common::encode_image`] for
/// the per-format meaning of quality.
pub fn compress_image(input: &[u8], options: &CompressOptions) -> Result<Vec<u8>> {
    validate_options(options)?;
    let decoded = decode_image(input, &options.common)?;
    let format = decoded.output_format(options.format);
    validate_lossless(options, format)?;
    encode_image(
        &decoded.image,
        format,
        &settings_for(options, options.quality),
    )
}

/// Re-encode `input` so the result is at most `target_bytes` bytes,
/// at the highest quality that fits.
///
/// Binary search over quality `1..=options.quality`, max 10 encodes
/// in total (9 search steps + one quality-1 retry).
/// Formats whose size does not depend on quality (PNG/WebP/GIF/BMP)
/// take a single best-effort encode instead: PNG at quality 100 (its
/// `Best` compression level — for PNG a higher quality number means a
/// *smaller* file, so a quality search would be backwards), the others
/// at `options.quality`.
///
/// Errors: `TARGET_SIZE_UNREACHABLE` when even the smallest encoding
/// exceeds the target (including a target of 0 bytes).
pub fn compress_to_size(
    input: &[u8],
    target_bytes: u64,
    options: &CompressOptions,
) -> Result<Vec<u8>> {
    validate_options(options)?;
    if target_bytes == 0 {
        return Err(DusrelError::target_size_unreachable(0, 1));
    }
    let decoded = decode_image(input, &options.common)?;
    let format = decoded.output_format(options.format);
    validate_lossless(options, format)?;

    // Quality-insensitive formats: one best-effort encode.
    if matches!(
        format,
        ImageFormat::Png | ImageFormat::WebP | ImageFormat::Gif | ImageFormat::Bmp
    ) {
        let best_quality = if format == ImageFormat::Png {
            100
        } else {
            options.quality
        };
        let bytes = encode_image(&decoded.image, format, &settings_for(options, best_quality))?;
        if bytes.len() as u64 <= target_bytes {
            return Ok(bytes);
        }
        return Err(DusrelError::target_size_unreachable(
            target_bytes,
            bytes.len() as u64,
        ));
    }

    // JPEG / AVIF: quality really does move the size. Binary search.
    // Total encodes stay at most 10 (SPEC "max 10 steps"): 9 search
    // encodes here plus the quality-1 retry below.
    let mut lo: u32 = 1;
    let mut hi: u32 = u32::from(options.quality);
    let mut best: Option<Vec<u8>> = None;
    for _ in 0..9 {
        if lo > hi {
            break;
        }
        let mid = lo + (hi - lo) / 2;
        let bytes = encode_image(&decoded.image, format, &settings_for(options, mid as u8))?;
        if bytes.len() as u64 <= target_bytes {
            best = Some(bytes);
            lo = mid + 1;
        } else {
            hi = mid - 1;
        }
    }
    if let Some(bytes) = best {
        return Ok(bytes);
    }
    // The search may have ended without ever trying quality 1 (9-step
    // cap): try it once before giving up — the 10th and last encode.
    let lowest = encode_image(&decoded.image, format, &settings_for(options, 1))?;
    if lowest.len() as u64 <= target_bytes {
        return Ok(lowest);
    }
    Err(DusrelError::target_size_unreachable(
        target_bytes,
        lowest.len() as u64,
    ))
}
