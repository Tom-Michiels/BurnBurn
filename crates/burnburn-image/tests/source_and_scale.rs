//! Public contract: original pixels are preserved under a content hash, and a physical scale is
//! explicit (DPI, mm per pixel or a calibration distance) or reported as missing.

#![allow(clippy::unwrap_used)]

use burnburn_image::{ImageError, ImageRecipe, PixelScale, ScaleError, SourceImage, Threshold, prepare};
use burnburn_types::{CancellationToken, Length};

fn checker(w: u32, h: u32) -> Vec<u8> {
    (0..w * h).map(|i| if (i % w + i / w).is_multiple_of(2) { 0 } else { 255 }).collect()
}

#[test]
fn content_hash_identifies_pixels_and_survives_processing() {
    let a = SourceImage::from_gray8(8, 4, checker(8, 4)).unwrap();
    let same = SourceImage::from_gray8(8, 4, checker(8, 4)).unwrap();
    let mut changed_px = checker(8, 4);
    changed_px[5] ^= 1;
    let changed = SourceImage::from_gray8(8, 4, changed_px).unwrap();
    let reshaped = SourceImage::from_gray8(4, 8, checker(8, 4)).unwrap();

    assert_eq!(a.content_hash(), same.content_hash());
    assert_ne!(a.content_hash(), changed.content_hash(), "one changed bit changes the hash");
    assert_ne!(a.content_hash(), reshaped.content_hash(), "same bytes, other dimensions");

    let before = a.samples().to_vec();
    let recipe = ImageRecipe {
        scale: Some(PixelScale::uniform_mm_per_pixel(0.1).unwrap()),
        threshold: Threshold::Fixed(127),
        median_radius: 1,
        ..ImageRecipe::default()
    };
    let prepared = prepare(&a, &recipe, &CancellationToken::never()).unwrap();
    assert_eq!(a.samples(), &before[..], "processing never touches the original samples");
    assert_eq!(prepared.source_hash, a.content_hash());
}

#[test]
fn bad_buffers_are_typed_errors() {
    assert_eq!(SourceImage::from_gray8(3, 3, vec![0; 8]).err(), Some(ImageError::BadDimensions));
    assert_eq!(SourceImage::from_gray8(0, 3, vec![]).err(), Some(ImageError::BadDimensions));
    assert_eq!(SourceImage::from_rgba8(2, 2, vec![0; 12]).err(), Some(ImageError::BadDimensions));
}

#[test]
fn missing_scale_is_a_typed_needs_scale_error() {
    let img = SourceImage::from_gray8(4, 4, vec![0; 16]).unwrap();
    let err = prepare(&img, &ImageRecipe::default(), &CancellationToken::never()).unwrap_err();
    assert_eq!(err, ImageError::Scale(ScaleError::NeedsScale));
}

#[test]
fn scale_from_dpi_mm_per_pixel_and_calibration_agree() {
    // 254 DPI is exactly 0.1 mm per pixel (25.4 mm per inch).
    let dpi = PixelScale::from_dpi(254.0, 127.0).unwrap();
    assert!((dpi.mm_per_pixel_x() - 0.1).abs() < 1e-15);
    assert!((dpi.mm_per_pixel_y() - 0.2).abs() < 1e-15);
    // 1000 px spanning 100 mm is 0.1 mm per pixel (tracer plan example).
    let cal = PixelScale::from_known_distance(1000.0, Length::from_mm(100.0).unwrap()).unwrap();
    assert!((cal.mm_per_pixel_x() - 0.1).abs() < 1e-15);
    assert_eq!(cal.mm_per_pixel_x(), cal.mm_per_pixel_y());

    for bad in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert_eq!(PixelScale::from_dpi(bad, 300.0), Err(ScaleError::NotPositive));
        assert_eq!(PixelScale::uniform_mm_per_pixel(bad), Err(ScaleError::NotPositive));
    }
    assert_eq!(PixelScale::from_known_distance(0.0, Length::from_mm(10.0).unwrap()), Err(ScaleError::NotPositive));
}
