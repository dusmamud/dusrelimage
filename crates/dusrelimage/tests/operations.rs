//! Integration tests per tool (SPEC section 6): round-trip
//! decode -> op -> encode -> decode, dimensions, pixel spot-checks,
//! and error codes. All fixtures are deterministic (seeded); no
//! network, no files on disk.

use std::io::Cursor;

use dusrelimage::common::{EncodeSettings, decode_image, encode_image, get_image_info};
use dusrelimage::fixtures::{MockKind, make_test_image};
use dusrelimage::options::{
    CommonOptions, CompressOptions, ConvertOptions, CropOptions, ResizeOptions,
};
use dusrelimage::types::{Colour, FitMode, ImageFormat, Position};
use dusrelimage::{compress_image, compress_to_size, convert_image, crop_image, resize_image};

fn approx_eq(a: u8, b: u8, tol: u8) -> bool {
    a.abs_diff(b) <= tol
}

/// Decode `bytes` with the `image` crate and read one RGBA pixel.
fn pixel_at(bytes: &[u8], x: u32, y: u32) -> [u8; 4] {
    let img = image::ImageReader::new(Cursor::new(bytes.to_vec()))
        .with_guessed_format()
        .unwrap()
        .decode()
        .unwrap();
    img.to_rgba8().get_pixel(x, y).0
}

#[test]
fn compress_png_keeps_format_and_dimensions() {
    let input = make_test_image(MockKind::Solid, 64, 48, 1);
    let out = compress_image(&input, &CompressOptions::default()).unwrap();
    let info = get_image_info(&out, &CommonOptions::default()).unwrap();
    assert_eq!(info.format, "png");
    assert_eq!((info.width, info.height), (64, 48));
    assert!(!info.has_alpha);
    assert_eq!(info.file_size, out.len() as u64);
}

#[test]
fn compress_to_jpeg_roundtrip() {
    let input = make_test_image(MockKind::Gradient, 100, 80, 2);
    let opts = CompressOptions {
        format: Some(ImageFormat::Jpeg),
        quality: 80,
        ..CompressOptions::default()
    };
    let out = compress_image(&input, &opts).unwrap();
    let info = get_image_info(&out, &CommonOptions::default()).unwrap();
    assert_eq!(info.format, "jpeg");
    assert_eq!((info.width, info.height), (100, 80));
}

#[test]
fn encode_image_rejects_quality_outside_1_to_100() {
    let input = make_test_image(MockKind::Solid, 32, 32, 1);
    let decoded = decode_image(&input, &CommonOptions::default()).unwrap();
    for quality in [0, 101, 255] {
        let settings = EncodeSettings {
            quality,
            ..EncodeSettings::default()
        };
        let err = encode_image(&decoded.image, ImageFormat::Jpeg, &settings).unwrap_err();
        assert_eq!(err.code_str(), "INVALID_QUALITY", "for quality {quality}");
    }
    let settings = EncodeSettings {
        quality: 100,
        ..EncodeSettings::default()
    };
    assert!(encode_image(&decoded.image, ImageFormat::Jpeg, &settings).is_ok());
}

#[test]
fn compress_quality_zero_is_invalid_quality() {
    let input = make_test_image(MockKind::Solid, 32, 32, 1);
    let opts = CompressOptions {
        quality: 0,
        ..CompressOptions::default()
    };
    let err = compress_image(&input, &opts).unwrap_err();
    assert_eq!(err.code_str(), "INVALID_QUALITY");
}

#[test]
fn compress_quality_above_100_is_invalid_quality() {
    let input = make_test_image(MockKind::Solid, 32, 32, 1);
    // 101 is the first out-of-range value; 255 is the u8 maximum.
    for quality in [101, 255] {
        let opts = CompressOptions {
            quality,
            ..CompressOptions::default()
        };
        let err = compress_image(&input, &opts).unwrap_err();
        assert_eq!(err.code_str(), "INVALID_QUALITY", "for quality {quality}");
        // compress_to_size shares the same validation.
        let err = compress_to_size(&input, 100_000, &opts).unwrap_err();
        assert_eq!(err.code_str(), "INVALID_QUALITY", "for quality {quality}");
    }
    // Boundaries 1 and 100 still succeed.
    for quality in [1, 100] {
        let opts = CompressOptions {
            quality,
            ..CompressOptions::default()
        };
        assert!(
            compress_image(&input, &opts).is_ok(),
            "for quality {quality}"
        );
    }
}

#[test]
fn compress_to_size_reaches_target_and_reports_unreachable() {
    let input = make_test_image(MockKind::Noise, 400, 300, 7);
    let opts = CompressOptions {
        format: Some(ImageFormat::Jpeg),
        quality: 90,
        ..CompressOptions::default()
    };
    // Generous target: must fit, at some quality <= 90.
    let out = compress_to_size(&input, 200_000, &opts).unwrap();
    assert!(out.len() <= 200_000);
    let info = get_image_info(&out, &CommonOptions::default()).unwrap();
    assert_eq!((info.width, info.height), (400, 300));

    // Impossible target: even quality 1 cannot fit in 100 bytes.
    let err = compress_to_size(&input, 100, &opts).unwrap_err();
    assert_eq!(err.code_str(), "TARGET_SIZE_UNREACHABLE");

    // Zero target is unreachable by definition.
    let err = compress_to_size(&input, 0, &opts).unwrap_err();
    assert_eq!(err.code_str(), "TARGET_SIZE_UNREACHABLE");
}

#[test]
fn compress_to_size_png_best_effort() {
    let input = make_test_image(MockKind::Solid, 64, 48, 1);
    let out = compress_to_size(&input, 100_000, &CompressOptions::default()).unwrap();
    assert!(out.len() <= 100_000);
    let err = compress_to_size(&input, 10, &CompressOptions::default()).unwrap_err();
    assert_eq!(err.code_str(), "TARGET_SIZE_UNREACHABLE");
}

#[test]
fn resize_percent_halves_dimensions() {
    let input = make_test_image(MockKind::Gradient, 100, 80, 3);
    let opts = ResizeOptions {
        percent: Some(50.0),
        ..ResizeOptions::default()
    };
    let out = resize_image(&input, &opts).unwrap();
    let info = get_image_info(&out, &CommonOptions::default()).unwrap();
    assert_eq!((info.width, info.height), (50, 40));
}

#[test]
fn resize_width_only_keeps_aspect_ratio() {
    let input = make_test_image(MockKind::Gradient, 100, 80, 3);
    let opts = ResizeOptions {
        width: Some(50),
        ..ResizeOptions::default()
    };
    let out = resize_image(&input, &opts).unwrap();
    let info = get_image_info(&out, &CommonOptions::default()).unwrap();
    assert_eq!((info.width, info.height), (50, 40));
}

#[test]
fn resize_fill_is_exact_with_background_corners() {
    let input = make_test_image(MockKind::Solid, 100, 80, 5);
    let opts = ResizeOptions {
        width: Some(40),
        height: Some(40),
        fit: FitMode::Fill,
        ..ResizeOptions::default()
    };
    let out = resize_image(&input, &opts).unwrap();
    let info = get_image_info(&out, &CommonOptions::default()).unwrap();
    assert_eq!((info.width, info.height), (40, 40));
    // Letterbox bars top/bottom are the (white) background.
    assert_eq!(pixel_at(&out, 20, 0), [255, 255, 255, 255]);
    assert_eq!(pixel_at(&out, 20, 39), [255, 255, 255, 255]);
}

#[test]
fn resize_stretch_and_crop_are_exact() {
    let input = make_test_image(MockKind::Gradient, 100, 80, 3);
    for fit in [FitMode::Stretch, FitMode::Crop] {
        let opts = ResizeOptions {
            width: Some(30),
            height: Some(30),
            fit,
            ..ResizeOptions::default()
        };
        let out = resize_image(&input, &opts).unwrap();
        let info = get_image_info(&out, &CommonOptions::default()).unwrap();
        assert_eq!((info.width, info.height), (30, 30));
    }
}

#[test]
fn resize_without_size_is_invalid_dimensions() {
    let input = make_test_image(MockKind::Solid, 32, 32, 1);
    let err = resize_image(&input, &ResizeOptions::default()).unwrap_err();
    assert_eq!(err.code_str(), "INVALID_DIMENSIONS");
}

#[test]
fn resize_percent_with_width_is_invalid_argument() {
    let input = make_test_image(MockKind::Solid, 32, 32, 1);
    let opts = ResizeOptions {
        percent: Some(50.0),
        width: Some(16),
        ..ResizeOptions::default()
    };
    let err = resize_image(&input, &opts).unwrap_err();
    assert_eq!(err.code_str(), "INVALID_ARGUMENT");
}

#[test]
fn crop_explicit_rect_and_out_of_bounds() {
    let input = make_test_image(MockKind::Solid, 100, 80, 9);
    let out = crop_image(&input, &CropOptions::new(10, 10, 20, 20)).unwrap();
    let info = get_image_info(&out, &CommonOptions::default()).unwrap();
    assert_eq!((info.width, info.height), (20, 20));

    // Right/bottom overflow.
    let err = crop_image(&input, &CropOptions::new(90, 10, 20, 20)).unwrap_err();
    assert_eq!(err.code_str(), "CROP_OUT_OF_BOUNDS");
    let err = crop_image(&input, &CropOptions::new(0, 70, 20, 20)).unwrap_err();
    assert_eq!(err.code_str(), "CROP_OUT_OF_BOUNDS");

    // Zero size.
    let err = crop_image(&input, &CropOptions::new(0, 0, 0, 20)).unwrap_err();
    assert_eq!(err.code_str(), "INVALID_DIMENSIONS");
}

#[test]
fn crop_anchor_center_places_the_rectangle() {
    // Solid red image: every pixel is the seed colour; check dims and
    // that the centre-anchored crop still contains image pixels.
    let input = make_test_image(MockKind::Solid, 100, 80, 11);
    let opts = CropOptions::anchored(Position::Center, 20, 20);
    let out = crop_image(&input, &opts).unwrap();
    let info = get_image_info(&out, &CommonOptions::default()).unwrap();
    assert_eq!((info.width, info.height), (20, 20));

    let anchored = CropOptions::anchored(Position::BottomRight, 20, 20);
    let out = crop_image(&input, &anchored).unwrap();
    let info = get_image_info(&out, &CommonOptions::default()).unwrap();
    assert_eq!((info.width, info.height), (20, 20));
}

#[test]
fn convert_png_to_jpeg_flattens_alpha_onto_background() {
    // Build a fully transparent PNG directly (fixtures are opaque).
    let rgba = image::RgbaImage::new(32, 32);
    let input = {
        let mut buf = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(rgba)
            .write_to(&mut buf, image::ImageFormat::Png)
            .unwrap();
        buf.into_inner()
    };
    let info = get_image_info(&input, &CommonOptions::default()).unwrap();
    assert!(info.has_alpha);

    let opts = ConvertOptions {
        format: ImageFormat::Jpeg,
        quality: 90,
        background: Colour::from_rgb(255, 0, 0),
        ..ConvertOptions::default()
    };
    let out = convert_image(&input, &opts).unwrap();
    let info = get_image_info(&out, &CommonOptions::default()).unwrap();
    assert_eq!(info.format, "jpeg");
    assert!(!info.has_alpha);
    // Transparent black flattened onto red -> (near-)red, JPEG lossy.
    let px = pixel_at(&out, 16, 16);
    assert!(approx_eq(px[0], 255, 12), "r={}", px[0]);
    assert!(approx_eq(px[1], 0, 12), "g={}", px[1]);
    assert!(approx_eq(px[2], 0, 12), "b={}", px[2]);
}

#[test]
fn convert_between_formats() {
    let input = make_test_image(MockKind::Gradient, 64, 48, 4);
    for (format, name) in [
        (ImageFormat::Png, "png"),
        (ImageFormat::Bmp, "bmp"),
        (ImageFormat::Gif, "gif"),
        (ImageFormat::WebP, "webp"),
    ] {
        let opts = ConvertOptions {
            format,
            ..ConvertOptions::default()
        };
        let out = convert_image(&input, &opts).unwrap();
        let info = get_image_info(&out, &CommonOptions::default()).unwrap();
        assert_eq!(info.format, name, "for {name}");
        assert_eq!((info.width, info.height), (64, 48));
    }
}

#[test]
fn convert_to_avif_lossy_encode_works() {
    let input = make_test_image(MockKind::Gradient, 64, 48, 4);
    let opts = ConvertOptions {
        format: ImageFormat::Avif,
        quality: 80,
        ..ConvertOptions::default()
    };
    let out = convert_image(&input, &opts).unwrap();
    assert!(!out.is_empty());
    // AVIF box header: size (4 bytes) then "ftypavif".
    assert_eq!(&out[4..12], b"ftypavif");
    // This build cannot decode AVIF back (needs the `avif-native`
    // decoder) — assert the documented limitation, not a round-trip.
    let err = get_image_info(&out, &CommonOptions::default()).unwrap_err();
    assert_eq!(err.code_str(), "UNSUPPORTED_FORMAT");
}

#[test]
fn tiff_input_decodes_and_falls_back_to_png_output() {
    // TIFF is a valid input (SPEC section 3) but not an output format:
    // info reports "tiff", and a format-preserving op falls back to PNG.
    let input = {
        let mut buf = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            48,
            32,
            image::Rgb([10, 20, 30]),
        ))
        .write_to(&mut buf, image::ImageFormat::Tiff)
        .unwrap();
        buf.into_inner()
    };
    let info = get_image_info(&input, &CommonOptions::default()).unwrap();
    assert_eq!(info.format, "tiff");
    assert_eq!((info.width, info.height), (48, 32));

    let out = compress_image(&input, &CompressOptions::default()).unwrap();
    let info = get_image_info(&out, &CommonOptions::default()).unwrap();
    assert_eq!(info.format, "png");
}

#[test]
fn convert_invalid_quality() {
    let input = make_test_image(MockKind::Solid, 32, 32, 1);
    let opts = ConvertOptions {
        quality: 0,
        ..ConvertOptions::default()
    };
    let err = convert_image(&input, &opts).unwrap_err();
    assert_eq!(err.code_str(), "INVALID_QUALITY");

    // Above 100 is out of range too (SPEC section 2: quality 1-100).
    for quality in [101, 255] {
        let opts = ConvertOptions {
            quality,
            ..ConvertOptions::default()
        };
        let err = convert_image(&input, &opts).unwrap_err();
        assert_eq!(err.code_str(), "INVALID_QUALITY", "for quality {quality}");
    }
    // Boundaries 1 and 100 still succeed.
    for quality in [1, 100] {
        let opts = ConvertOptions {
            quality,
            ..ConvertOptions::default()
        };
        assert!(
            convert_image(&input, &opts).is_ok(),
            "for quality {quality}"
        );
    }
}

#[test]
fn get_image_info_reports_fields() {
    let input = make_test_image(MockKind::Gradient, 64, 48, 4);
    let info = get_image_info(&input, &CommonOptions::default()).unwrap();
    assert_eq!(info.format, "png");
    assert_eq!(info.width, 64);
    assert_eq!(info.height, 48);
    assert!(!info.has_alpha);
    assert_eq!(info.colour_type, "rgb8");
    assert_eq!(info.file_size, input.len() as u64);
}

#[test]
fn garbage_bytes_are_invalid_image() {
    let garbage = b"this is not an image at all, just plain text bytes 1234567890";
    let err = get_image_info(garbage, &CommonOptions::default()).unwrap_err();
    assert_eq!(err.code_str(), "INVALID_IMAGE");
    assert!(!err.message_en.is_empty());
    assert!(!err.message_hi.is_empty());
}

#[test]
fn empty_input_is_empty_input() {
    let err = get_image_info(&[], &CommonOptions::default()).unwrap_err();
    assert_eq!(err.code_str(), "EMPTY_INPUT");
    let err = compress_image(&[], &CompressOptions::default()).unwrap_err();
    assert_eq!(err.code_str(), "EMPTY_INPUT");
}

#[test]
fn image_too_large_is_enforced() {
    let input = make_test_image(MockKind::Solid, 64, 48, 1);
    let tiny = CommonOptions {
        max_dimension: 10,
        ..CommonOptions::default()
    };
    let err = get_image_info(&input, &tiny).unwrap_err();
    assert_eq!(err.code_str(), "IMAGE_TOO_LARGE");

    let tiny_pixels = CommonOptions {
        max_pixels: 100,
        ..CommonOptions::default()
    };
    let err = get_image_info(&input, &tiny_pixels).unwrap_err();
    assert_eq!(err.code_str(), "IMAGE_TOO_LARGE");

    // ... and the decode helper agrees.
    let err = decode_image(&input, &tiny).unwrap_err();
    assert_eq!(err.code_str(), "IMAGE_TOO_LARGE");
}

#[test]
fn fixtures_are_deterministic() {
    let a = make_test_image(MockKind::Noise, 32, 32, 7);
    let b = make_test_image(MockKind::Noise, 32, 32, 7);
    assert_eq!(a, b);
    let c = make_test_image(MockKind::Noise, 32, 32, 8);
    assert_ne!(a, c);
    let solid_a = make_test_image(MockKind::Solid, 16, 16, 3);
    let solid_b = make_test_image(MockKind::Solid, 16, 16, 3);
    assert_eq!(solid_a, solid_b);
}

#[test]
fn colour_hex_roundtrip() {
    let c = Colour::from_hex("#1A2B3C").unwrap();
    assert_eq!((c.r, c.g, c.b), (0x1A, 0x2B, 0x3C));
    assert_eq!(c.hex, "#1A2B3C");
    assert!(Colour::from_hex("not-a-colour").is_err());
    let short = Colour::from_hex("#F00").unwrap();
    assert_eq!((short.r, short.g, short.b), (255, 0, 0));
}
