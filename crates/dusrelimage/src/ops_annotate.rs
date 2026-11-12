//! Tool 6 — annotator (SPEC section 1).
//!
//! Raster shapes drawn directly onto the image: arrows, rectangles,
//! ellipses, lines and text (bundled DejaVu Sans, see
//! [`crate::draw`]). All coordinates are absolute pixels. Shapes
//! partly outside the image are clipped, never an error. The output
//! keeps the input format (TIFF input falls back to PNG) and is
//! re-encoded at quality 80.

use crate::common::{EncodeSettings, decode_image, encode_image};
use crate::draw::{
    blend_image_onto, draw_arrow, draw_ellipse_outline, draw_line, draw_rect_outline, fill_ellipse,
    fill_rect, render_text_layer,
};
use crate::error::{DusrelError, Result};
use crate::options::CommonOptions;
use crate::types::Annotation;

/// Draw `annotations` onto `input`, in order.
pub fn annotate_image(input: &[u8], annotations: &[Annotation]) -> Result<Vec<u8>> {
    if annotations.is_empty() {
        return Err(DusrelError::invalid_argument(
            "No annotations given: pass at least one annotation.",
            "Koi annotation nahi diya: kam se kam ek annotation dein.",
        ));
    }
    for annotation in annotations {
        validate_annotation(annotation)?;
    }
    let common = CommonOptions::default();
    let decoded = decode_image(input, &common)?;
    let mut rgba = decoded.image.to_rgba8();

    for annotation in annotations {
        match annotation {
            Annotation::Arrow {
                from,
                to,
                colour,
                width,
            } => draw_arrow(
                &mut rgba,
                (from.0 as i32, from.1 as i32),
                (to.0 as i32, to.1 as i32),
                colour,
                *width,
            ),
            Annotation::Rect {
                x,
                y,
                w,
                h,
                colour,
                width,
                fill,
            } => {
                if let Some(fill_colour) = fill {
                    fill_rect(&mut rgba, *x, *y, *w, *h, fill_colour);
                }
                draw_rect_outline(&mut rgba, *x, *y, *w, *h, colour, *width);
            }
            Annotation::Ellipse {
                x,
                y,
                w,
                h,
                colour,
                width,
                fill,
            } => {
                if let Some(fill_colour) = fill {
                    fill_ellipse(&mut rgba, *x, *y, *w, *h, fill_colour);
                }
                draw_ellipse_outline(&mut rgba, *x, *y, *w, *h, colour, *width);
            }
            Annotation::Line {
                from,
                to,
                colour,
                width,
            } => draw_line(
                &mut rgba,
                (from.0 as i32, from.1 as i32),
                (to.0 as i32, to.1 as i32),
                colour,
                *width,
            ),
            Annotation::Text {
                x,
                y,
                text,
                size,
                colour,
            } => {
                let layer = render_text_layer(text, *size, colour, 1.0)?;
                blend_image_onto(&mut rgba, &layer, *x as i32, *y as i32);
            }
        }
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

fn validate_annotation(annotation: &Annotation) -> Result<()> {
    match annotation {
        Annotation::Rect { w, h, .. } | Annotation::Ellipse { w, h, .. } => {
            if *w == 0 || *h == 0 {
                return Err(DusrelError::invalid_argument(
                    "Annotation rectangle width and height must be greater than 0.",
                    "Annotation rectangle ki width aur height 0 se badi honi chahiye.",
                ));
            }
            Ok(())
        }
        Annotation::Text { text, size, .. } => {
            if text.is_empty() {
                return Err(DusrelError::invalid_argument(
                    "Annotation text is empty.",
                    "Annotation text khaali hai.",
                ));
            }
            if !size.is_finite() || *size <= 0.0 {
                return Err(DusrelError::invalid_argument(
                    &format!(
                        "Annotation font size {size} is invalid. Font size must be greater than 0."
                    ),
                    "Annotation font size galat hai. Font size 0 se badi honi chahiye.",
                ));
            }
            Ok(())
        }
        Annotation::Arrow { .. } | Annotation::Line { .. } => Ok(()),
    }
}
