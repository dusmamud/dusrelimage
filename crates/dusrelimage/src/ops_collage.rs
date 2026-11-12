//! Tool 9 — collage (SPEC section 1).
//!
//! Grid layouts `1x1..4x4` plus named `Row` / `Column` layouts
//! (`masonry` is out of scope for v0.1.0). Cells are separated by
//! `gap` px of `background` colour, with an outer `gap` border of the
//! same colour. Each input is fitted into its cell per `cell_fit`
//! (`Fit` and `Fill` both letterbox onto the background colour;
//! `Stretch` distorts to the cell; `Crop` covers then centre-crops).
//! Cells with no input stay background. The output is always PNG
//! (lossless — a collage is a new composition, not a re-encode of one
//! input). More inputs than cells is `INVALID_ARGUMENT`.

use image::{GenericImageView, RgbaImage};

use crate::common::{EncodeSettings, check_limits, decode_image, encode_image};
use crate::draw::blend_image_onto;
use crate::error::{DusrelError, Result};
use crate::options::CollageOptions;
use crate::types::{FitMode, ImageFormat};

/// Fit one decoded image into a `cell_width` x `cell_height` cell,
/// returning an RGBA cell of exactly that size.
fn fit_cell(
    image: &image::DynamicImage,
    cell_width: u32,
    cell_height: u32,
    fit: FitMode,
    background: &crate::types::Colour,
) -> RgbaImage {
    use image::imageops::FilterType;
    match fit {
        FitMode::Fit | FitMode::Fill => {
            let resized = image.resize(cell_width, cell_height, FilterType::Lanczos3);
            let mut cell = RgbaImage::new(cell_width, cell_height);
            for px in cell.pixels_mut() {
                *px = image::Rgba(background.to_rgba());
            }
            let (rw, rh) = resized.dimensions();
            let x = (cell_width.saturating_sub(rw)) / 2;
            let y = (cell_height.saturating_sub(rh)) / 2;
            blend_image_onto(&mut cell, &resized.to_rgba8(), x as i32, y as i32);
            cell
        }
        FitMode::Stretch => image
            .resize_exact(cell_width, cell_height, FilterType::Lanczos3)
            .to_rgba8(),
        FitMode::Crop => image
            .resize_to_fill(cell_width, cell_height, FilterType::Lanczos3)
            .to_rgba8(),
    }
}

/// Combine `inputs` into one collage image per `options`.
pub fn make_collage(inputs: &[Vec<u8>], options: &CollageOptions) -> Result<Vec<u8>> {
    if inputs.is_empty() {
        return Err(DusrelError::empty_input("collage images"));
    }
    options.layout.validate()?;
    if options.width == 0 || options.height == 0 {
        return Err(DusrelError::invalid_dimensions(
            "collage width and height must be greater than 0",
        ));
    }
    check_limits(options.width, options.height, &options.common)?;
    let (rows, cols) = options.layout.grid_dimensions(inputs.len());
    let cells = rows * cols;
    if inputs.len() as u32 > cells {
        return Err(DusrelError::invalid_argument(
            &format!(
                "Collage layout {rows}x{cols} has {cells} cells but {} images were given.",
                inputs.len()
            ),
            &format!(
                "Collage layout {rows}x{cols} me {cells} cells hain, par {} images di gayi.",
                inputs.len()
            ),
        ));
    }
    let gap = options.gap as i64;
    let avail_width = options.width as i64 - gap * (cols as i64 + 1);
    let avail_height = options.height as i64 - gap * (rows as i64 + 1);
    if avail_width < cols as i64 || avail_height < rows as i64 {
        return Err(DusrelError::invalid_dimensions(
            "collage gap is too large for the collage size",
        ));
    }
    let cell_width = (avail_width / cols as i64) as u32;
    let cell_height = (avail_height / rows as i64) as u32;

    let mut canvas = RgbaImage::new(options.width, options.height);
    for px in canvas.pixels_mut() {
        *px = image::Rgba(options.background.to_rgba());
    }
    for (i, bytes) in inputs.iter().enumerate() {
        let row = i as u32 / cols;
        let col = i as u32 % cols;
        let cell_x = options.gap + col * (cell_width + options.gap);
        let cell_y = options.gap + row * (cell_height + options.gap);
        let decoded = decode_image(bytes, &options.common)?;
        let cell = fit_cell(
            &decoded.image,
            cell_width,
            cell_height,
            options.cell_fit,
            &options.background,
        );
        blend_image_onto(&mut canvas, &cell, cell_x as i32, cell_y as i32);
    }

    let out = image::DynamicImage::ImageRgba8(canvas);
    encode_image(&out, ImageFormat::Png, &EncodeSettings::default())
}
