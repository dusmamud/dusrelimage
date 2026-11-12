//! Shared raster-drawing helpers for tools 5 (watermark) and 6
//! (annotator).
//!
//! Hand-rolled on the `image` crate + `ab_glyph` — no `imageproc`
//! dependency, deterministic, pure Rust (WASM-safe). All drawing is
//! src-over alpha blending; coordinates are clipped to the canvas,
//! never panicking on out-of-range input.

use ab_glyph::{Font, FontRef, Glyph, PxScale, ScaleFont, point};
use image::{Rgba, RgbaImage};

use crate::error::{DusrelError, Result};
use crate::types::{Colour, Position};

/// Bundled DejaVu Sans 2.37 (Bitstream Vera licence — see
/// `assets/DejaVu-LICENSE.txt`).
///
/// Source:
/// <https://github.com/dejavu-fonts/dejavu-fonts/releases/download/version_2_37/dejavu-fonts-ttf-2.37.tar.bz2>
/// (`ttf/DejaVuSans.ttf` inside the tarball, SHA-256
/// `7da195a74c55bef988d0d48f9508bd5d849425c1770dba5d7bfc6ce9ed848954`).
/// Embedded with `include_bytes!` so the crate stays self-contained
/// (works in the WASM build too, with no font file on disk).
pub static DEJAVU_SANS_TTF: &[u8] = include_bytes!("../assets/DejaVuSans.ttf");

/// Load the bundled font. Fails only if the embedded bytes are corrupt
/// (a build-time problem, not bad user input).
pub fn load_font() -> Result<FontRef<'static>> {
    FontRef::try_from_slice(DEJAVU_SANS_TTF).map_err(|e| {
        DusrelError::invalid_argument(
            &format!("Bundled font could not be loaded: {e}"),
            "Bundled font load nahi ho paya.",
        )
    })
}

/// Measure `text` at `size` px: `(width, height)` of the tightest box
/// holding the rendered glyphs. Multi-line text (`'\n'`) stacks lines
/// at the font's line height.
pub fn measure_text(text: &str, size: f32) -> Result<(u32, u32)> {
    let font = load_font()?;
    let scaled = font.as_scaled(PxScale::from(size));
    let line_height = scaled.height().max(1.0);
    let mut width: f32 = 0.0;
    for line in text.split('\n') {
        let mut line_width: f32 = 0.0;
        for ch in line.chars() {
            line_width += scaled.h_advance(scaled.glyph_id(ch));
        }
        width = width.max(line_width);
    }
    let lines = text.split('\n').count().max(1) as f32;
    Ok((
        width.ceil().max(1.0) as u32,
        (line_height * lines).ceil().max(1.0) as u32,
    ))
}

/// Render `text` at `size` px into a transparent RGBA layer, drawn in
/// `colour` with `opacity` (0-1) folded into the glyph alpha.
/// The layer is exactly [`measure_text`] sized; the text's top-left
/// sits at the layer's top-left.
pub fn render_text_layer(
    text: &str,
    size: f32,
    colour: &Colour,
    opacity: f32,
) -> Result<RgbaImage> {
    let (width, height) = measure_text(text, size)?;
    let mut layer = RgbaImage::new(width, height);
    if text.is_empty() {
        return Ok(layer);
    }
    let font = load_font()?;
    let scaled = font.as_scaled(PxScale::from(size));
    let line_height = scaled.height().max(1.0);
    let alpha_scale = (colour.a as f32 / 255.0) * opacity.clamp(0.0, 1.0);
    for (line_idx, line) in text.split('\n').enumerate() {
        let y_base = scaled.ascent() + line_idx as f32 * line_height;
        let mut x: f32 = 0.0;
        for ch in line.chars() {
            let id = scaled.glyph_id(ch);
            let glyph = Glyph {
                id,
                scale: PxScale::from(size),
                position: point(x, y_base),
            };
            if let Some(outlined) = scaled.outline_glyph(glyph) {
                let bounds = outlined.px_bounds();
                outlined.draw(|gx, gy, coverage| {
                    let px = bounds.min.x as i32 + gx as i32;
                    let py = bounds.min.y as i32 + gy as i32;
                    if px >= 0 && py >= 0 && (px as u32) < width && (py as u32) < height {
                        let a = (coverage * alpha_scale * 255.0).round().clamp(0.0, 255.0) as u8;
                        if a > 0 {
                            layer.put_pixel(
                                px as u32,
                                py as u32,
                                Rgba([colour.r, colour.g, colour.b, a]),
                            );
                        }
                    }
                });
            }
            x += scaled.h_advance(id);
        }
    }
    Ok(layer)
}

/// Rotate `layer` by `degrees` (clockwise, any angle) around its
/// centre, nearest-neighbour sampling. The output canvas grows to hold
/// the whole rotated layer.
pub fn rotate_layer(layer: &RgbaImage, degrees: f32) -> RgbaImage {
    let (w, h) = layer.dimensions();
    if w == 0 || h == 0 || degrees % 360.0 == 0.0 {
        return layer.clone();
    }
    let angle = degrees.to_radians();
    let (cos, sin) = (angle.cos(), angle.sin());
    let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
    // Rotated corners of the source, relative to its centre.
    let corners = [(-cx, -cy), (cx, -cy), (cx, cy), (-cx, cy)];
    let (mut min_x, mut max_x) = (f32::MAX, f32::MIN);
    let (mut min_y, mut max_y) = (f32::MAX, f32::MIN);
    for (x, y) in corners {
        let rx = x * cos - y * sin;
        let ry = x * sin + y * cos;
        min_x = min_x.min(rx);
        max_x = max_x.max(rx);
        min_y = min_y.min(ry);
        max_y = max_y.max(ry);
    }
    let out_w = (max_x - min_x).ceil().max(1.0) as u32;
    let out_h = (max_y - min_y).ceil().max(1.0) as u32;
    let mut out = RgbaImage::new(out_w, out_h);
    for oy in 0..out_h {
        for ox in 0..out_w {
            // Output pixel centre in rotated coords, then inverse-rotate.
            let px = min_x + ox as f32 + 0.5;
            let py = min_y + oy as f32 + 0.5;
            let sx = px * cos + py * sin + cx;
            let sy = -px * sin + py * cos + cy;
            let ix = sx.round() as i32;
            let iy = sy.round() as i32;
            if ix >= 0 && iy >= 0 && (ix as u32) < w && (iy as u32) < h {
                out.put_pixel(ox, oy, *layer.get_pixel(ix as u32, iy as u32));
            }
        }
    }
    out
}

/// Blend one RGBA pixel over another (src-over). Transparent source is
/// a no-op; opaque source replaces.
pub fn blend_pixel(dst: &mut Rgba<u8>, src: Rgba<u8>) {
    let sa = src[3] as f32 / 255.0;
    if sa <= 0.0 {
        return;
    }
    if sa >= 1.0 {
        *dst = src;
        return;
    }
    let da = dst[3] as f32 / 255.0;
    let out_a = sa + da * (1.0 - sa);
    if out_a <= 0.0 {
        *dst = Rgba([0, 0, 0, 0]);
        return;
    }
    for i in 0..3 {
        dst[i] = ((src[i] as f32 * sa + dst[i] as f32 * da * (1.0 - sa)) / out_a).round() as u8;
    }
    dst[3] = (out_a * 255.0).round() as u8;
}

/// Blend `src` onto `dst` with its top-left at `(offset_x, offset_y)`.
/// Out-of-canvas parts are clipped.
pub fn blend_image_onto(dst: &mut RgbaImage, src: &RgbaImage, offset_x: i32, offset_y: i32) {
    let (dw, dh) = dst.dimensions();
    let (sw, sh) = src.dimensions();
    for sy in 0..sh {
        let dy = offset_y + sy as i32;
        if dy < 0 || dy >= dh as i32 {
            continue;
        }
        for sx in 0..sw {
            let dx = offset_x + sx as i32;
            if dx < 0 || dx >= dw as i32 {
                continue;
            }
            let pixel = *src.get_pixel(sx, sy);
            if pixel[3] > 0 {
                let target = dst.get_pixel_mut(dx as u32, dy as u32);
                blend_pixel(target, pixel);
            }
        }
    }
}

/// Multiply every pixel's alpha by `opacity` (0-1), in place.
pub fn apply_opacity(layer: &mut RgbaImage, opacity: f32) {
    let factor = opacity.clamp(0.0, 1.0);
    if (factor - 1.0).abs() < f32::EPSILON {
        return;
    }
    for px in layer.pixels_mut() {
        px[3] = (px[3] as f32 * factor).round() as u8;
    }
}

/// Top-left offset for a `layer_w` x `layer_h` layer placed at
/// `position` on an `img_w` x `img_h` image with `margin` px from the
/// edge. May be negative when the layer is larger than the image
/// (callers clip while blending).
pub fn anchor_offset(
    position: Position,
    img_w: u32,
    img_h: u32,
    layer_w: u32,
    layer_h: u32,
    margin: u32,
) -> (i32, i32) {
    let m = margin as i32;
    let (iw, ih, lw, lh) = (img_w as i32, img_h as i32, layer_w as i32, layer_h as i32);
    let x = match position {
        Position::TopLeft | Position::CenterLeft | Position::BottomLeft => m,
        Position::TopCenter | Position::Center | Position::BottomCenter | Position::Tiled => {
            (iw - lw) / 2
        }
        Position::TopRight | Position::CenterRight | Position::BottomRight => iw - lw - m,
    };
    let y = match position {
        Position::TopLeft | Position::TopCenter | Position::TopRight => m,
        Position::CenterLeft | Position::Center | Position::CenterRight | Position::Tiled => {
            (ih - lh) / 2
        }
        Position::BottomLeft | Position::BottomCenter | Position::BottomRight => ih - lh - m,
    };
    (x, y)
}

/// Tile `layer` across the whole image: repeats every
/// `layer + margin` px, starting at (0, 0).
pub fn tile_layer(image: &mut RgbaImage, layer: &RgbaImage, margin: u32) {
    let (img_w, img_h) = image.dimensions();
    let (lw, lh) = layer.dimensions();
    if lw == 0 || lh == 0 {
        return;
    }
    let step_x = lw + margin.max(1);
    let step_y = lh + margin.max(1);
    let mut y: i32 = 0;
    while y < img_h as i32 {
        let mut x: i32 = 0;
        while x < img_w as i32 {
            blend_image_onto(image, layer, x, y);
            x += step_x as i32;
        }
        y += step_y as i32;
    }
}

/// Stamp a filled `width` x `width` square centred on `(cx, cy)`,
/// blended in `colour`. `width` of 0 is treated as 1.
fn stamp(image: &mut RgbaImage, cx: i32, cy: i32, colour: &Colour, width: u32) {
    let width = width.max(1) as i32;
    let start = -((width - 1) / 2);
    let rgba = Rgba(colour.to_rgba());
    let (w, h) = image.dimensions();
    for oy in 0..width {
        for ox in 0..width {
            let px = cx + start + ox;
            let py = cy + start + oy;
            if px >= 0 && py >= 0 && px < w as i32 && py < h as i32 {
                let target = image.get_pixel_mut(px as u32, py as u32);
                blend_pixel(target, rgba);
            }
        }
    }
}

/// Bresenham line with a square brush of `width` px (`width` 0 = 1).
pub fn draw_line(
    image: &mut RgbaImage,
    from: (i32, i32),
    to: (i32, i32),
    colour: &Colour,
    width: u32,
) {
    let (mut x0, mut y0) = from;
    let (x1, y1) = to;
    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    loop {
        stamp(image, x0, y0, colour, width);
        if x0 == x1 && y0 == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x0 += sx;
        }
        if e2 <= dx {
            err += dx;
            y0 += sy;
        }
    }
}

/// Fill a rectangle (clipped to the canvas) in `colour`.
pub fn fill_rect(image: &mut RgbaImage, x: u32, y: u32, w: u32, h: u32, colour: &Colour) {
    let (img_w, img_h) = image.dimensions();
    let rgba = Rgba(colour.to_rgba());
    let x1 = (x as u64 + w as u64).min(img_w as u64) as u32;
    let y1 = (y as u64 + h as u64).min(img_h as u64) as u32;
    for py in y.min(img_h)..y1 {
        for px in x.min(img_w)..x1 {
            let target = image.get_pixel_mut(px, py);
            blend_pixel(target, rgba);
        }
    }
}

/// Rectangle outline: four lines of `width` px. Callers fill first,
/// then call this, so the border paints over the fill.
pub fn draw_rect_outline(
    image: &mut RgbaImage,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    colour: &Colour,
    width: u32,
) {
    if w == 0 || h == 0 {
        return;
    }
    let (x0, y0) = (x as i32, y as i32);
    let (x1, y1) = ((x + w - 1) as i32, (y + h - 1) as i32);
    draw_line(image, (x0, y0), (x1, y0), colour, width);
    draw_line(image, (x0, y1), (x1, y1), colour, width);
    draw_line(image, (x0, y0), (x0, y1), colour, width);
    draw_line(image, (x1, y0), (x1, y1), colour, width);
}

/// Fill an ellipse inside the `x, y, w, h` bounding box.
pub fn fill_ellipse(image: &mut RgbaImage, x: u32, y: u32, w: u32, h: u32, colour: &Colour) {
    if w == 0 || h == 0 {
        return;
    }
    let (img_w, img_h) = image.dimensions();
    let rgba = Rgba(colour.to_rgba());
    let (cx, cy) = (x as f32 + w as f32 / 2.0, y as f32 + h as f32 / 2.0);
    let (rx, ry) = (w as f32 / 2.0, h as f32 / 2.0);
    let x1 = (x as u64 + w as u64).min(img_w as u64) as u32;
    let y1 = (y as u64 + h as u64).min(img_h as u64) as u32;
    for py in y.min(img_h)..y1 {
        for px in x.min(img_w)..x1 {
            let dx = (px as f32 + 0.5 - cx) / rx;
            let dy = (py as f32 + 0.5 - cy) / ry;
            if dx * dx + dy * dy <= 1.0 {
                let target = image.get_pixel_mut(px, py);
                blend_pixel(target, rgba);
            }
        }
    }
}

/// Ellipse outline: sample the perimeter densely and stamp `width` px
/// squares, so the outline is continuous at any size.
pub fn draw_ellipse_outline(
    image: &mut RgbaImage,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    colour: &Colour,
    width: u32,
) {
    if w == 0 || h == 0 {
        return;
    }
    let (cx, cy) = (x as f32 + w as f32 / 2.0, y as f32 + h as f32 / 2.0);
    let (rx, ry) = (w as f32 / 2.0, h as f32 / 2.0);
    let step = 1.0 / rx.max(ry).max(1.0);
    let mut angle = 0.0f32;
    while angle < std::f32::consts::TAU {
        let px = (cx + rx * angle.cos()).round() as i32;
        let py = (cy + ry * angle.sin()).round() as i32;
        stamp(image, px, py, colour, width);
        angle += step;
    }
}

/// Arrow: a line plus a two-stroke head at `to`. Head length is
/// `max(10, width * 4)` px, opening at ±25° from the shaft.
pub fn draw_arrow(
    image: &mut RgbaImage,
    from: (i32, i32),
    to: (i32, i32),
    colour: &Colour,
    width: u32,
) {
    draw_line(image, from, to, colour, width);
    let dx = (to.0 - from.0) as f32;
    let dy = (to.1 - from.1) as f32;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1.0 {
        return;
    }
    let head_len = (width as f32 * 4.0).max(10.0);
    let head_angle = 25.0f32.to_radians();
    let base_angle = dy.atan2(dx);
    for side in [1.0, -1.0] {
        let a = base_angle + std::f32::consts::PI - side * head_angle;
        let tip = (
            (to.0 as f32 + head_len * a.cos()).round() as i32,
            (to.1 as f32 + head_len * a.sin()).round() as i32,
        );
        draw_line(image, to, tip, colour, width);
    }
}
