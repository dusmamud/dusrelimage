//! Tools 7 and 8 — colour picker and palette extractor
//! (SPEC section 1).
//!
//! `pick_colour` reads one pixel (out-of-bounds is
//! `PIXEL_OUT_OF_BOUNDS`). `extract_palette` runs median-cut
//! quantisation over sampled pixels: the most frequent colours first,
//! each with its share (0-1) of the sampled pixels.

use std::cmp::Ordering;

use image::GenericImageView;

use crate::common::decode_image;
use crate::error::{DusrelError, Result};
use crate::options::CommonOptions;
use crate::types::{Colour, PaletteColour};

/// Maximum pixels sampled for the palette (stride-sampled when the
/// image is larger, so a 120 MP image stays fast).
const MAX_SAMPLED_PIXELS: u64 = 200_000;

/// Read the colour of the pixel at `(x, y)` in `input`.
pub fn pick_colour(input: &[u8], x: u32, y: u32) -> Result<Colour> {
    let decoded = decode_image(input, &CommonOptions::default())?;
    let (width, height) = decoded.image.dimensions();
    if x >= width || y >= height {
        return Err(DusrelError::pixel_out_of_bounds(x, y, width, height));
    }
    let pixel = decoded.image.get_pixel(x, y);
    Ok(Colour::from_rgba(pixel[0], pixel[1], pixel[2], pixel[3]))
}

/// Extract the `count` dominant colours of `input` (1-16), most
/// frequent first. Each entry carries its share (0-1) of the sampled
/// pixels. Returns fewer than `count` entries when the image has fewer
/// distinct colours.
pub fn extract_palette(input: &[u8], count: u8) -> Result<Vec<PaletteColour>> {
    if count == 0 || count > 16 {
        return Err(DusrelError::invalid_argument(
            &format!("Palette count {count} is invalid. Count must be between 1 and 16."),
            &format!("Palette count {count} galat hai. Count 1 se 16 ke beech hona chahiye."),
        ));
    }
    let decoded = decode_image(input, &CommonOptions::default())?;
    let rgba = decoded.image.to_rgba8();
    let (width, height) = rgba.dimensions();

    // Stride-sample large images down to ~MAX_SAMPLED_PIXELS.
    let total = width as u64 * height as u64;
    let stride = ((total as f64 / MAX_SAMPLED_PIXELS as f64).sqrt().ceil() as u32).max(1);
    // Nearly-transparent pixels are invisible: skip them (a<128). If
    // the whole image is transparent, fall back to all pixels' RGB.
    let mut pixels: Vec<[u8; 3]> = Vec::new();
    let mut fallback: Vec<[u8; 3]> = Vec::new();
    for y in (0..height).step_by(stride as usize) {
        for x in (0..width).step_by(stride as usize) {
            let px = rgba.get_pixel(x, y);
            let rgb = [px[0], px[1], px[2]];
            fallback.push(rgb);
            if px[3] >= 128 {
                pixels.push(rgb);
            }
        }
    }
    if pixels.is_empty() {
        pixels = fallback;
    }
    let sampled = pixels.len() as f32;

    // Median cut: repeatedly split the box with the largest channel
    // range, at the median along that channel, until `count` boxes.
    let mut boxes: Vec<Vec<[u8; 3]>> = vec![pixels];
    while boxes.len() < count as usize {
        let mut best: Option<(usize, usize, usize)> = None; // (box, range, channel)
        for (i, b) in boxes.iter().enumerate() {
            if b.len() < 2 {
                continue;
            }
            for ch in 0..3 {
                let (mut lo, mut hi) = (u8::MAX, u8::MIN);
                for p in b {
                    lo = lo.min(p[ch]);
                    hi = hi.max(p[ch]);
                }
                let range = (hi - lo) as usize;
                if range > 0 && best.is_none_or(|(_, best_range, _)| range > best_range) {
                    best = Some((i, range, ch));
                }
            }
        }
        let Some((idx, _, channel)) = best else {
            break; // every box is a single colour
        };
        let mut box_pixels = boxes.swap_remove(idx);
        box_pixels.sort_unstable_by_key(|p| p[channel]);
        let mid = box_pixels.len() / 2;
        let second = box_pixels.split_off(mid);
        boxes.push(box_pixels);
        boxes.push(second);
    }

    let mut palette: Vec<PaletteColour> = boxes
        .iter()
        .map(|b| {
            let n = b.len() as f32;
            let (mut r, mut g, mut bl) = (0u64, 0u64, 0u64);
            for p in b {
                r += p[0] as u64;
                g += p[1] as u64;
                bl += p[2] as u64;
            }
            let n_u = b.len().max(1) as u64;
            PaletteColour {
                colour: Colour::from_rgb((r / n_u) as u8, (g / n_u) as u8, (bl / n_u) as u8),
                share: n / sampled,
            }
        })
        .collect();
    palette.sort_by(|a, b| {
        b.share
            .partial_cmp(&a.share)
            .unwrap_or(Ordering::Equal)
            .then_with(|| a.colour.hex.cmp(&b.colour.hex))
    });
    Ok(palette)
}
