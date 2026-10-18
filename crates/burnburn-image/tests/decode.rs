//! Public contract: only PNG/JPEG, bounded dimensions checked before allocation, typed errors,
//! exact preservation of decoded samples, and file DPI as a suggestion only.

#![allow(clippy::unwrap_used)]

use std::io::Cursor;

use burnburn_image::{DecodeError, DecodeLimits, ImageError, ImageRecipe, PixelFormat, ScaleError, decode, prepare};
use burnburn_types::CancellationToken;
use image::{ImageEncoder, codecs::png::PngEncoder};

fn png_gray(w: u32, h: u32, px: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    PngEncoder::new(Cursor::new(&mut out)).write_image(px, w, h, image::ExtendedColorType::L8).unwrap();
    out
}

/// CRC-32 (ISO-HDLC) as PNG requires, written independently of the decoder.
fn crc32(data: &[u8]) -> u32 {
    let mut c = 0xffff_ffffu32;
    for &b in data {
        c ^= u32::from(b);
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 };
        }
    }
    !c
}

fn chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut out = (data.len() as u32).to_be_bytes().to_vec();
    let mut body = kind.to_vec();
    body.extend_from_slice(data);
    out.extend_from_slice(&body);
    out.extend_from_slice(&crc32(&body).to_be_bytes());
    out
}

/// Inserts a chunk right after IHDR (signature 8 bytes + IHDR 25 bytes).
fn insert_after_ihdr(png: &[u8], extra: &[u8]) -> Vec<u8> {
    let mut out = png[..33].to_vec();
    out.extend_from_slice(extra);
    out.extend_from_slice(&png[33..]);
    out
}

#[test]
fn png_samples_are_preserved_exactly() {
    let px: Vec<u8> = (0..12).map(|i| i * 20).collect();
    let img = decode(&png_gray(4, 3, &px), DecodeLimits::default()).unwrap();
    assert_eq!((img.width(), img.height(), img.format()), (4, 3, PixelFormat::Gray8));
    assert_eq!(img.samples(), &px[..]);
    assert_eq!(img.metadata().dpi_suggestion, None);
}

#[test]
fn jpeg_decodes_to_rgb_with_close_values() {
    let mut out = Vec::new();
    let px = vec![200u8; 16 * 16 * 3];
    image::codecs::jpeg::JpegEncoder::new_with_quality(Cursor::new(&mut out), 95)
        .write_image(&px, 16, 16, image::ExtendedColorType::Rgb8)
        .unwrap();
    let img = decode(&out, DecodeLimits::default()).unwrap();
    assert_eq!((img.width(), img.height(), img.format()), (16, 16, PixelFormat::Rgb8));
    let [r, g, b, a] = img.rgba_at(7, 7);
    assert!((r - 200.0).abs() <= 3.0 && (g - 200.0).abs() <= 3.0 && (b - 200.0).abs() <= 3.0 && a == 255.0);
    // JFIF written by the encoder carries a density; whatever it is, it is only a suggestion.
    let recipe = ImageRecipe::default();
    assert_eq!(
        prepare(&img, &recipe, &CancellationToken::never()).unwrap_err(),
        ImageError::Scale(ScaleError::NeedsScale)
    );
}

#[test]
fn png_phys_is_reported_as_dpi_suggestion() {
    // pHYs: 3937 px per metre on both axes, unit 1 (metre) → 99.9998 DPI.
    let mut phys = 3937u32.to_be_bytes().to_vec();
    phys.extend_from_slice(&3937u32.to_be_bytes());
    phys.push(1);
    let png = insert_after_ihdr(&png_gray(2, 2, &[0, 1, 2, 3]), &chunk(b"pHYs", &phys));
    let img = decode(&png, DecodeLimits::default()).unwrap();
    let (x, y) = img.metadata().dpi_suggestion.unwrap();
    assert!((x - 3937.0 * 0.0254).abs() < 1e-9 && (y - x).abs() < 1e-12, "{x} {y}");
}

#[test]
fn oversized_dimensions_are_rejected_from_the_header() {
    // A valid 2×2 PNG whose IHDR is patched to 5000×5000 (and re-CRC'd); there is no pixel data
    // for that size, so only a header check can produce `TooLarge` instead of `Malformed`.
    let mut png = png_gray(2, 2, &[0, 1, 2, 3]);
    png[16..20].copy_from_slice(&5000u32.to_be_bytes());
    png[20..24].copy_from_slice(&5000u32.to_be_bytes());
    let crc = crc32(&png[12..29]);
    png[29..33].copy_from_slice(&crc.to_be_bytes());
    let err = decode(&png, DecodeLimits { max_pixels: 1_000_000 }).unwrap_err();
    assert_eq!(err, DecodeError::TooLarge { width: 5000, height: 5000, max_pixels: 1_000_000 });
}

#[test]
fn other_formats_and_garbage_are_typed_errors() {
    let gif = b"GIF89a\x01\x00\x01\x00\x00\x00\x00;";
    assert_eq!(decode(gif, DecodeLimits::default()).unwrap_err(), DecodeError::UnsupportedFormat);
    assert_eq!(decode(b"not an image", DecodeLimits::default()).unwrap_err(), DecodeError::UnsupportedFormat);
    let png = png_gray(4, 4, &[7; 16]);
    let truncated = &png[..png.len() - 20];
    assert!(matches!(decode(truncated, DecodeLimits::default()), Err(DecodeError::Malformed(_))));
}
