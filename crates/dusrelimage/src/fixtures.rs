//! Deterministic mock image builders (SPEC section 5).
//!
//! Used by the CLI `mock` command and by the integration tests.
//! Everything here is seeded — the same `(kind, width, height, seed)`
//! always produces byte-identical PNG output, with no randomness
//! across runs. No external RNG crate: a small splitmix-style hash
//! over `(x, y, seed)` is all the noise needs.

use std::io::Cursor;

use image::{DynamicImage, ImageFormat, Rgb, RgbImage};

use crate::types::Colour;

/// Kind of mock image to generate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MockKind {
    /// Flat fill; the colour is derived from the seed.
    Solid,
    /// Horizontal gradient between two seed-derived colours.
    Gradient,
    /// Photo-like: smooth value noise over a gradient, plus fine
    /// per-pixel jitter. Compresses like a real photo (badly at low
    /// quality), which is what `compress_to_size` tests need.
    Noise,
}

/// Deterministic 64-bit hash of `(x, y, seed)` (splitmix64 finaliser).
fn hash2(x: u32, y: u32, seed: u64) -> u64 {
    let mut z = seed
        .wrapping_add((x as u64) << 32 | y as u64)
        .wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Three seed-derived channel values in 0..255.
fn seed_rgb(seed: u64) -> (u8, u8, u8) {
    let h = hash2(0, 0, seed);
    (
        (h & 0xFF) as u8,
        ((h >> 8) & 0xFF) as u8,
        ((h >> 16) & 0xFF) as u8,
    )
}

fn encode_png(image: &DynamicImage) -> Vec<u8> {
    let mut buf = Cursor::new(Vec::new());
    image
        .write_to(&mut buf, ImageFormat::Png)
        .expect("in-memory PNG encoding of a valid image cannot fail");
    buf.into_inner()
}

/// Build a solid-colour PNG of `width` x `height` (dims clamp to >= 1).
pub fn solid_png(width: u32, height: u32, colour: Colour) -> Vec<u8> {
    let (width, height) = (width.max(1), height.max(1));
    let mut img = RgbImage::new(width, height);
    for px in img.pixels_mut() {
        *px = Rgb([colour.r, colour.g, colour.b]);
    }
    encode_png(&DynamicImage::ImageRgb8(img))
}

/// Build a horizontal-gradient PNG from `from` (left) to `to` (right).
pub fn gradient_png(width: u32, height: u32, from: Colour, to: Colour) -> Vec<u8> {
    let (width, height) = (width.max(1), height.max(1));
    let mut img = RgbImage::new(width, height);
    for x in 0..width {
        let t = if width > 1 {
            x as f32 / (width - 1) as f32
        } else {
            0.0
        };
        let px = Rgb([
            (from.r as f32 + (to.r as f32 - from.r as f32) * t).round() as u8,
            (from.g as f32 + (to.g as f32 - from.g as f32) * t).round() as u8,
            (from.b as f32 + (to.b as f32 - from.b as f32) * t).round() as u8,
        ]);
        for y in 0..height {
            img.put_pixel(x, y, px);
        }
    }
    encode_png(&DynamicImage::ImageRgb8(img))
}

/// Build a photo-like noise PNG: smooth value noise (16 px cells,
/// bilinearly interpolated) over a gradient, plus fine jitter.
pub fn photo_like_png(width: u32, height: u32, seed: u64) -> Vec<u8> {
    let (width, height) = (width.max(1), height.max(1));
    let (r0, g0, b0) = seed_rgb(seed);
    let (r1, g1, b1) = seed_rgb(seed ^ 0xA5A5_A5A5_5A5A_5A5A);
    // Value noise: random grid values, bilinear interpolation.
    let cell = 16u32;
    let grid_value = |gx: u32, gy: u32, ch: u64| -> f32 {
        (hash2(gx, gy, seed ^ ch.wrapping_mul(0x0101_0101_0101_0101)) % 256) as f32 / 255.0
    };
    let mut img = RgbImage::new(width, height);
    for y in 0..height {
        for x in 0..width {
            let gx = x / cell;
            let gy = y / cell;
            let fx = (x % cell) as f32 / cell as f32;
            let fy = (y % cell) as f32 / cell as f32;
            let smooth = |ch: u64| -> f32 {
                let v00 = grid_value(gx, gy, ch);
                let v10 = grid_value(gx + 1, gy, ch);
                let v01 = grid_value(gx, gy + 1, ch);
                let v11 = grid_value(gx + 1, gy + 1, ch);
                let top = v00 + (v10 - v00) * fx;
                let bottom = v01 + (v11 - v01) * fx;
                top + (bottom - top) * fy
            };
            let t = if width > 1 {
                x as f32 / (width - 1) as f32
            } else {
                0.0
            };
            // Base gradient +- noise, plus fine per-pixel jitter.
            let jitter = |ch: u64| -> f32 { (hash2(x, y, seed ^ ch) % 33) as f32 - 16.0 };
            let mix = |c0: u8, c1: u8, n: f32, j: f32| -> u8 {
                (c0 as f32 + (c1 as f32 - c0 as f32) * t + (n - 0.5) * 64.0 + j)
                    .round()
                    .clamp(0.0, 255.0) as u8
            };
            img.put_pixel(
                x,
                y,
                Rgb([
                    mix(r0, r1, smooth(11), jitter(101)),
                    mix(g0, g1, smooth(23), jitter(202)),
                    mix(b0, b1, smooth(37), jitter(303)),
                ]),
            );
        }
    }
    encode_png(&DynamicImage::ImageRgb8(img))
}

/// Generate a deterministic mock image (PNG bytes) for the CLI `mock`
/// command and the shared test vectors. Same inputs -> identical bytes.
pub fn make_test_image(kind: MockKind, width: u32, height: u32, seed: u64) -> Vec<u8> {
    match kind {
        MockKind::Solid => {
            let (r, g, b) = seed_rgb(seed);
            solid_png(width, height, Colour::from_rgb(r, g, b))
        }
        MockKind::Gradient => {
            let (r0, g0, b0) = seed_rgb(seed);
            let (r1, g1, b1) = seed_rgb(seed ^ 0xA5A5_A5A5_5A5A_5A5A);
            gradient_png(
                width,
                height,
                Colour::from_rgb(r0, g0, b0),
                Colour::from_rgb(r1, g1, b1),
            )
        }
        MockKind::Noise => photo_like_png(width, height, seed),
    }
}
