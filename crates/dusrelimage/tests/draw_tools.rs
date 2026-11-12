//! Integration tests for tools 5-10 + bulk (SPEC section 6):
//! round-trip decode -> op -> encode -> decode, dimensions, pixel
//! spot-checks, and error codes. All fixtures are deterministic
//! (seeded); no network, no files on disk.

use std::io::Cursor;

use dusrelimage::bulk::{BulkOp, BulkOutput};
use dusrelimage::common::get_image_info;
use dusrelimage::fixtures::{MockKind, make_test_image, solid_png};
use dusrelimage::options::{CollageOptions, CommonOptions, PixelateOptions, WatermarkOptions};
use dusrelimage::types::{
    Annotation, CollageLayout, Colour, FitMode, ImageWatermark, Position, TextWatermark,
};
use dusrelimage::{
    add_watermark, annotate_image, bulk_process, extract_palette, make_collage, pick_colour,
    pixelate_image,
};

/// Decode `bytes` with the `image` crate and read one RGBA pixel.
fn pixel_at(bytes: &[u8], x: u32, y: u32) -> [u8; 4] {
    let img = image::ImageReader::new(Cursor::new(bytes.to_vec()))
        .with_guessed_format()
        .unwrap()
        .decode()
        .unwrap();
    img.to_rgba8().get_pixel(x, y).0
}

/// True when any pixel in the rectangle is dark (any channel < 128).
fn region_has_dark(bytes: &[u8], x0: u32, y0: u32, x1: u32, y1: u32) -> bool {
    let img = image::ImageReader::new(Cursor::new(bytes.to_vec()))
        .with_guessed_format()
        .unwrap()
        .decode()
        .unwrap()
        .to_rgba8();
    for y in y0..y1 {
        for x in x0..x1 {
            let px = img.get_pixel(x, y);
            if px[0] < 128 || px[1] < 128 || px[2] < 128 {
                return true;
            }
        }
    }
    false
}

/// True when any pixel in the rectangle differs from white.
fn region_has_nonwhite(bytes: &[u8], x0: u32, y0: u32, x1: u32, y1: u32) -> bool {
    let img = image::ImageReader::new(Cursor::new(bytes.to_vec()))
        .with_guessed_format()
        .unwrap()
        .decode()
        .unwrap()
        .to_rgba8();
    for y in y0..y1 {
        for x in x0..x1 {
            let px = img.get_pixel(x, y);
            if px[0] != 255 || px[1] != 255 || px[2] != 255 {
                return true;
            }
        }
    }
    false
}

fn white_png(width: u32, height: u32) -> Vec<u8> {
    solid_png(width, height, Colour::white())
}

fn text_watermark_opts() -> WatermarkOptions {
    WatermarkOptions {
        text: Some(TextWatermark {
            text: "Hi".to_string(),
            size: 48.0,
            colour: Colour::black(),
        }),
        opacity: 1.0,
        position: Position::BottomRight,
        ..WatermarkOptions::default()
    }
}

// ---- Tool 5: watermark ----

#[test]
fn watermark_text_draws_dark_pixels_and_keeps_dimensions() {
    let input = white_png(200, 100);
    let out = add_watermark(&input, &text_watermark_opts()).unwrap();
    let info = get_image_info(&out, &CommonOptions::default()).unwrap();
    assert_eq!(info.format, "png");
    assert_eq!((info.width, info.height), (200, 100));
    // Bottom-right quadrant must contain the black text.
    assert!(region_has_dark(&out, 100, 40, 200, 100));
    // Top-left stays white.
    assert!(!region_has_nonwhite(&out, 0, 0, 60, 30));
}

#[test]
fn watermark_text_top_left_anchor() {
    let input = white_png(200, 100);
    let opts = WatermarkOptions {
        position: Position::TopLeft,
        ..text_watermark_opts()
    };
    let out = add_watermark(&input, &opts).unwrap();
    assert!(region_has_dark(&out, 0, 0, 100, 60));
    assert!(!region_has_nonwhite(&out, 140, 60, 200, 100));
}

#[test]
fn watermark_tiled_covers_whole_image() {
    let input = white_png(200, 200);
    let opts = WatermarkOptions {
        position: Position::Tiled,
        ..text_watermark_opts()
    };
    let out = add_watermark(&input, &opts).unwrap();
    assert!(region_has_dark(&out, 0, 0, 60, 60));
    assert!(region_has_dark(&out, 140, 140, 200, 200));
}

#[test]
fn watermark_rotated_still_draws_and_keeps_dimensions() {
    let input = white_png(200, 200);
    let opts = WatermarkOptions {
        rotation_deg: 45.0,
        position: Position::Center,
        ..text_watermark_opts()
    };
    let out = add_watermark(&input, &opts).unwrap();
    let info = get_image_info(&out, &CommonOptions::default()).unwrap();
    assert_eq!((info.width, info.height), (200, 200));
    assert!(region_has_dark(&out, 50, 50, 150, 150));
}

#[test]
fn watermark_logo_centred_and_opaque() {
    let input = white_png(100, 100);
    let logo = solid_png(20, 20, Colour::from_rgb(255, 0, 0));
    let opts = WatermarkOptions {
        text: None,
        logo: Some(ImageWatermark {
            image: logo,
            scale: 1.0,
        }),
        opacity: 1.0,
        position: Position::Center,
        ..WatermarkOptions::default()
    };
    let out = add_watermark(&input, &opts).unwrap();
    let px = pixel_at(&out, 50, 50);
    assert_eq!(px[0], 255);
    assert_eq!(px[1], 0);
    assert_eq!(px[2], 0);
    // Outside the logo stays white.
    assert_eq!(pixel_at(&out, 5, 5), [255, 255, 255, 255]);
}

#[test]
fn watermark_logo_scaled_half() {
    let input = white_png(100, 100);
    let logo = solid_png(40, 40, Colour::from_rgb(0, 0, 255));
    let opts = WatermarkOptions {
        text: None,
        logo: Some(ImageWatermark {
            image: logo,
            scale: 0.5,
        }),
        opacity: 1.0,
        position: Position::Center,
        ..WatermarkOptions::default()
    };
    let out = add_watermark(&input, &opts).unwrap();
    // 20x20 logo centred on 100x100: covers 40..60.
    assert_eq!(pixel_at(&out, 50, 50)[2], 255);
    assert_eq!(pixel_at(&out, 30, 30), [255, 255, 255, 255]);
}

#[test]
fn watermark_validation_errors() {
    let input = white_png(100, 100);
    // Neither text nor logo.
    let err = add_watermark(&input, &WatermarkOptions::default()).unwrap_err();
    assert_eq!(err.code_str(), "INVALID_ARGUMENT");
    // Opacity out of range.
    let opts = WatermarkOptions {
        opacity: 1.5,
        ..text_watermark_opts()
    };
    let err = add_watermark(&input, &opts).unwrap_err();
    assert_eq!(err.code_str(), "INVALID_ARGUMENT");
    // Empty text.
    let opts = WatermarkOptions {
        text: Some(TextWatermark {
            text: String::new(),
            size: 48.0,
            colour: Colour::black(),
        }),
        ..WatermarkOptions::default()
    };
    let err = add_watermark(&input, &opts).unwrap_err();
    assert_eq!(err.code_str(), "INVALID_ARGUMENT");
    // Garbage input bytes.
    let err = add_watermark(&[0, 1, 2, 3], &text_watermark_opts()).unwrap_err();
    assert_eq!(err.code_str(), "INVALID_IMAGE");
}

// ---- Tool 6: annotator ----

#[test]
fn annotate_filled_rect_and_outline_rect() {
    let input = white_png(100, 100);
    let annotations = vec![
        Annotation::Rect {
            x: 10,
            y: 10,
            w: 40,
            h: 30,
            colour: Colour::from_rgb(255, 0, 0),
            width: 2,
            fill: Some(Colour::from_rgb(255, 0, 0)),
        },
        Annotation::Rect {
            x: 60,
            y: 10,
            w: 30,
            h: 30,
            colour: Colour::from_rgb(0, 0, 255),
            width: 3,
            fill: None,
        },
    ];
    let out = annotate_image(&input, &annotations).unwrap();
    let info = get_image_info(&out, &CommonOptions::default()).unwrap();
    assert_eq!((info.width, info.height), (100, 100));
    // Filled rect centre is red.
    assert_eq!(pixel_at(&out, 30, 25), [255, 0, 0, 255]);
    // Outline-only rect: border blue, interior still white.
    assert_eq!(pixel_at(&out, 60, 10)[2], 255);
    assert_eq!(pixel_at(&out, 75, 25), [255, 255, 255, 255]);
}

#[test]
fn annotate_line_and_arrow() {
    let input = white_png(100, 100);
    let annotations = vec![
        Annotation::Line {
            from: (0, 0),
            to: (99, 99),
            colour: Colour::black(),
            width: 1,
        },
        Annotation::Arrow {
            from: (10, 80),
            to: (90, 80),
            colour: Colour::from_rgb(255, 0, 0),
            width: 2,
        },
    ];
    let out = annotate_image(&input, &annotations).unwrap();
    assert!(pixel_at(&out, 50, 50)[0] < 128);
    // Arrow shaft and tip are red.
    assert_eq!(pixel_at(&out, 50, 80)[0], 255);
    assert_eq!(pixel_at(&out, 50, 80)[1], 0);
    assert_eq!(pixel_at(&out, 90, 80)[0], 255);
}

#[test]
fn annotate_filled_ellipse() {
    let input = white_png(100, 100);
    let annotations = vec![Annotation::Ellipse {
        x: 10,
        y: 10,
        w: 40,
        h: 40,
        colour: Colour::from_rgb(0, 0, 255),
        width: 2,
        fill: Some(Colour::from_rgb(0, 0, 255)),
    }];
    let out = annotate_image(&input, &annotations).unwrap();
    assert_eq!(pixel_at(&out, 30, 30), [0, 0, 255, 255]);
    // Corner of the bounding box stays white.
    assert_eq!(pixel_at(&out, 11, 11), [255, 255, 255, 255]);
}

#[test]
fn annotate_text_draws() {
    let input = white_png(200, 100);
    let annotations = vec![Annotation::Text {
        x: 10,
        y: 10,
        text: "A".to_string(),
        size: 40.0,
        colour: Colour::black(),
    }];
    let out = annotate_image(&input, &annotations).unwrap();
    assert!(region_has_dark(&out, 10, 10, 60, 60));
}

#[test]
fn annotate_validation_errors() {
    let input = white_png(100, 100);
    let err = annotate_image(&input, &[]).unwrap_err();
    assert_eq!(err.code_str(), "INVALID_ARGUMENT");
    let err = annotate_image(
        &input,
        &[Annotation::Rect {
            x: 0,
            y: 0,
            w: 0,
            h: 10,
            colour: Colour::black(),
            width: 1,
            fill: None,
        }],
    )
    .unwrap_err();
    assert_eq!(err.code_str(), "INVALID_ARGUMENT");
    let err = annotate_image(
        &[9, 9, 9],
        &[Annotation::Line {
            from: (0, 0),
            to: (10, 10),
            colour: Colour::black(),
            width: 1,
        }],
    )
    .unwrap_err();
    assert_eq!(err.code_str(), "INVALID_IMAGE");
}

// ---- Tool 7: colour picker ----

#[test]
fn pick_colour_known_pixel() {
    let input = solid_png(64, 48, Colour::from_rgb(10, 20, 30));
    let colour = pick_colour(&input, 5, 5).unwrap();
    assert_eq!(colour.r, 10);
    assert_eq!(colour.g, 20);
    assert_eq!(colour.b, 30);
    assert_eq!(colour.a, 255);
    assert_eq!(colour.hex, "#0A141E");
}

#[test]
fn pick_colour_gradient_position_matters() {
    let input = make_test_image(MockKind::Gradient, 100, 10, 3);
    let left = pick_colour(&input, 0, 5).unwrap();
    let right = pick_colour(&input, 99, 5).unwrap();
    assert_ne!(left.hex, right.hex);
}

#[test]
fn pick_colour_out_of_bounds() {
    let input = solid_png(64, 48, Colour::white());
    let err = pick_colour(&input, 64, 0).unwrap_err();
    assert_eq!(err.code_str(), "PIXEL_OUT_OF_BOUNDS");
    let err = pick_colour(&input, 0, 48).unwrap_err();
    assert_eq!(err.code_str(), "PIXEL_OUT_OF_BOUNDS");
}

// ---- Tool 8: palette ----

fn two_colour_png() -> Vec<u8> {
    let mut img = image::RgbImage::new(100, 10);
    for y in 0..10 {
        for x in 0..100 {
            let px = if x < 50 {
                image::Rgb([255, 0, 0])
            } else {
                image::Rgb([0, 0, 255])
            };
            img.put_pixel(x, y, px);
        }
    }
    let mut buf = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(img)
        .write_to(&mut buf, image::ImageFormat::Png)
        .unwrap();
    buf.into_inner()
}

#[test]
fn palette_two_colours_most_frequent_first() {
    let input = two_colour_png();
    let palette = extract_palette(&input, 2).unwrap();
    assert_eq!(palette.len(), 2);
    let hexes: Vec<&str> = palette.iter().map(|p| p.colour.hex.as_str()).collect();
    assert!(hexes.contains(&"#FF0000"));
    assert!(hexes.contains(&"#0000FF"));
    for entry in &palette {
        assert!((entry.share - 0.5).abs() < 0.01);
    }
    // Shares sum to 1.
    let total: f32 = palette.iter().map(|p| p.share).sum();
    assert!((total - 1.0).abs() < 0.001);
}

#[test]
fn palette_solid_image_is_one_entry_share_one() {
    let input = solid_png(50, 50, Colour::from_rgb(1, 2, 3));
    let palette = extract_palette(&input, 4).unwrap();
    assert_eq!(palette.len(), 1);
    assert_eq!(palette[0].colour.hex, "#010203");
    assert!((palette[0].share - 1.0).abs() < 0.001);
}

#[test]
fn palette_count_validation() {
    let input = two_colour_png();
    assert_eq!(
        extract_palette(&input, 0).unwrap_err().code_str(),
        "INVALID_ARGUMENT"
    );
    assert_eq!(
        extract_palette(&input, 17).unwrap_err().code_str(),
        "INVALID_ARGUMENT"
    );
    assert_eq!(
        extract_palette(&[1, 2, 3], 4).unwrap_err().code_str(),
        "INVALID_IMAGE"
    );
}

// ---- Tool 9: collage ----

#[test]
fn collage_grid_1x2_cells_gap_and_background() {
    let red = solid_png(90, 80, Colour::from_rgb(255, 0, 0));
    let blue = solid_png(90, 80, Colour::from_rgb(0, 0, 255));
    let opts = CollageOptions {
        layout: CollageLayout::Grid { rows: 1, cols: 2 },
        gap: 10,
        background: Colour::white(),
        width: 210,
        height: 100,
        ..CollageOptions::default()
    };
    let out = make_collage(&[red, blue], &opts).unwrap();
    let info = get_image_info(&out, &CommonOptions::default()).unwrap();
    assert_eq!(info.format, "png");
    assert_eq!((info.width, info.height), (210, 100));
    // Cell 0 centre red, cell 1 centre blue, gap pixel white.
    assert_eq!(pixel_at(&out, 55, 50), [255, 0, 0, 255]);
    assert_eq!(pixel_at(&out, 155, 50), [0, 0, 255, 255]);
    assert_eq!(pixel_at(&out, 105, 50), [255, 255, 255, 255]);
}

#[test]
fn collage_row_layout_and_empty_cell() {
    let red = solid_png(40, 40, Colour::from_rgb(255, 0, 0));
    let opts = CollageOptions {
        layout: CollageLayout::Row,
        gap: 0,
        background: Colour::from_rgb(0, 255, 0),
        width: 200,
        height: 100,
        cell_fit: FitMode::Stretch,
        ..CollageOptions::default()
    };
    // Row with 1 input: 1 row x 1 col — the single cell fills the canvas.
    let out = make_collage(&[red], &opts).unwrap();
    let info = get_image_info(&out, &CommonOptions::default()).unwrap();
    assert_eq!((info.width, info.height), (200, 100));
    assert_eq!(pixel_at(&out, 100, 50), [255, 0, 0, 255]);
}

#[test]
fn collage_validation_errors() {
    let red = solid_png(40, 40, Colour::from_rgb(255, 0, 0));
    // Empty input list.
    let err = make_collage(&[], &CollageOptions::default()).unwrap_err();
    assert_eq!(err.code_str(), "EMPTY_INPUT");
    // More images than cells in a 1x1 grid.
    let opts = CollageOptions {
        layout: CollageLayout::Grid { rows: 1, cols: 1 },
        ..CollageOptions::default()
    };
    let err = make_collage(&[red.clone(), red.clone()], &opts).unwrap_err();
    assert_eq!(err.code_str(), "INVALID_ARGUMENT");
    // Grid out of range.
    let opts = CollageOptions {
        layout: CollageLayout::Grid { rows: 5, cols: 1 },
        ..CollageOptions::default()
    };
    let err = make_collage(std::slice::from_ref(&red), &opts).unwrap_err();
    assert_eq!(err.code_str(), "INVALID_ARGUMENT");
    // Zero output size.
    let opts = CollageOptions {
        width: 0,
        ..CollageOptions::default()
    };
    let err = make_collage(&[red], &opts).unwrap_err();
    assert_eq!(err.code_str(), "INVALID_DIMENSIONS");
}

// ---- Tool 10: pixelator ----

#[test]
fn pixelate_rect_blocks_become_flat() {
    let input = make_test_image(MockKind::Gradient, 100, 100, 3);
    let opts = PixelateOptions {
        x: 0,
        y: 0,
        width: 100,
        height: 100,
        block_size: 20,
        ..PixelateOptions::default()
    };
    let out = pixelate_image(&input, &opts).unwrap();
    let info = get_image_info(&out, &CommonOptions::default()).unwrap();
    assert_eq!((info.width, info.height), (100, 100));
    // Nearest downscale+upscale: every pixel inside one 20x20 block is
    // the same colour (the block is flat) ...
    assert_eq!(pixel_at(&out, 5, 5), pixel_at(&out, 19, 19));
    assert_eq!(pixel_at(&out, 25, 25), pixel_at(&out, 39, 39));
    // ... blocks differ from each other on a gradient ...
    assert_ne!(pixel_at(&out, 5, 5), pixel_at(&out, 25, 25));
    // ... and the block value differs from the gradient's own value there.
    assert_ne!(pixel_at(&out, 5, 5), pixel_at(&input, 5, 5));
}

#[test]
fn pixelate_rect_only_leaves_rest_untouched() {
    let input = make_test_image(MockKind::Gradient, 100, 100, 3);
    let opts = PixelateOptions {
        x: 0,
        y: 0,
        width: 50,
        height: 50,
        block_size: 10,
        ..PixelateOptions::default()
    };
    let out = pixelate_image(&input, &opts).unwrap();
    // Bottom-right (outside the rect) is byte-identical to the input.
    assert_eq!(pixel_at(&out, 90, 90), pixel_at(&input, 90, 90));
}

#[test]
fn pixelate_validation_errors() {
    let input = make_test_image(MockKind::Gradient, 100, 100, 3);
    let opts = PixelateOptions {
        block_size: 1,
        ..PixelateOptions::default()
    };
    assert_eq!(
        pixelate_image(&input, &opts).unwrap_err().code_str(),
        "INVALID_ARGUMENT"
    );
    let opts = PixelateOptions {
        block_size: 101,
        ..PixelateOptions::default()
    };
    assert_eq!(
        pixelate_image(&input, &opts).unwrap_err().code_str(),
        "INVALID_ARGUMENT"
    );
    let opts = PixelateOptions {
        x: 90,
        y: 0,
        width: 20,
        height: 10,
        block_size: 10,
        ..PixelateOptions::default()
    };
    assert_eq!(
        pixelate_image(&input, &opts).unwrap_err().code_str(),
        "INVALID_ARGUMENT"
    );
}

// ---- Bulk ----

#[test]
fn bulk_preserves_order_and_reports_partial_failures() {
    let good = make_test_image(MockKind::Solid, 32, 32, 1);
    let garbage = vec![0u8, 1, 2, 3];
    let inputs = vec![good.clone(), garbage, good.clone()];
    let results = bulk_process(&inputs, &BulkOp::Pixelate(PixelateOptions::default()));
    assert_eq!(results.len(), 3);
    assert_eq!(results[0].index, 0);
    assert_eq!(results[1].index, 1);
    assert_eq!(results[2].index, 2);
    assert!(results[0].is_ok());
    assert!(results[1].is_err());
    assert_eq!(
        results[1].result.as_ref().unwrap_err().code_str(),
        "INVALID_IMAGE"
    );
    assert!(results[2].is_ok());
    assert!(matches!(results[0].result, Ok(BulkOutput::Image(_))));
}

#[test]
fn bulk_pick_colour_and_palette_outputs() {
    let input = solid_png(20, 20, Colour::from_rgb(9, 8, 7));
    let results = bulk_process(
        std::slice::from_ref(&input),
        &BulkOp::PickColour { x: 3, y: 3 },
    );
    assert_eq!(results.len(), 1);
    match &results[0].result {
        Ok(BulkOutput::Colour(c)) => assert_eq!(c.hex, "#090807"),
        other => panic!("expected Colour, got {other:?}"),
    }
    let results = bulk_process(&[input], &BulkOp::Palette { count: 2 });
    match &results[0].result {
        Ok(BulkOutput::Palette(p)) => {
            assert_eq!(p.len(), 1);
            assert_eq!(p[0].colour.hex, "#090807");
        }
        other => panic!("expected Palette, got {other:?}"),
    }
}

#[test]
fn bulk_collage_is_per_item_invalid_argument() {
    let input = make_test_image(MockKind::Solid, 32, 32, 1);
    let results = bulk_process(&[input], &BulkOp::Collage(CollageOptions::default()));
    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0].result.as_ref().unwrap_err().code_str(),
        "INVALID_ARGUMENT"
    );
}

#[test]
fn bulk_empty_input_list_is_empty_output() {
    let results = bulk_process(&[], &BulkOp::Pixelate(PixelateOptions::default()));
    assert!(results.is_empty());
}

#[test]
fn bulk_watermark_and_annotate_roundtrip() {
    let input = white_png(100, 100);
    let results = bulk_process(
        std::slice::from_ref(&input),
        &BulkOp::Watermark(text_watermark_opts()),
    );
    assert!(results[0].is_ok());
    let results = bulk_process(
        &[input],
        &BulkOp::Annotate(vec![Annotation::Line {
            from: (0, 0),
            to: (99, 99),
            colour: Colour::black(),
            width: 1,
        }]),
    );
    assert!(results[0].is_ok());
    // The decoded output keeps its dimensions.
    if let Ok(BulkOutput::Image(bytes)) = &results[0].result {
        let info = get_image_info(bytes, &CommonOptions::default()).unwrap();
        assert_eq!((info.width, info.height), (100, 100));
    } else {
        panic!("expected image output");
    }
}
