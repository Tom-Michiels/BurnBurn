//! Public contract (IM10, P03): PNG/JPEG source images become assets with a physical size.
//! The PNG fixtures are built byte by byte here (stored deflate), independent of any encoder.

#![allow(clippy::unwrap_used)] // test helpers outside #[test] fns

use burnburn_io::AssetStore;
use burnburn_io::import::{RasterError, RasterFormat, RasterLimits, inspect_raster};

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

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let mut body = kind.to_vec();
    body.extend_from_slice(data);
    out.extend_from_slice(&body);
    out.extend_from_slice(&crc32(&body).to_be_bytes());
}

/// A grey 8-bit PNG of `w × h` pixels; `phys` = (pixels per unit x, y, unit byte).
fn png(w: u32, h: u32, phys: Option<(u32, u32, u8)>) -> Vec<u8> {
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 0, 0, 0, 0]); // depth 8, greyscale, deflate, filter 0, no interlace
    chunk(&mut out, b"IHDR", &ihdr);
    if let Some((x, y, unit)) = phys {
        let mut p = Vec::new();
        p.extend_from_slice(&x.to_be_bytes());
        p.extend_from_slice(&y.to_be_bytes());
        p.push(unit);
        chunk(&mut out, b"pHYs", &p);
    }
    // Raw scanlines (filter byte 0 + w grey bytes) in one stored deflate block, zlib-wrapped.
    let mut raw = Vec::new();
    for _ in 0..h {
        raw.push(0);
        raw.extend(std::iter::repeat_n(0x80, w as usize));
    }
    let mut z = vec![0x78, 0x01, 0x01];
    let len = raw.len() as u16;
    z.extend_from_slice(&len.to_le_bytes());
    z.extend_from_slice(&(!len).to_le_bytes());
    z.extend_from_slice(&raw);
    let (mut a, mut b) = (1u32, 0u32);
    for &x in &raw {
        a = (a + u32::from(x)) % 65521;
        b = (b + a) % 65521;
    }
    z.extend_from_slice(&((b << 16) | a).to_be_bytes());
    chunk(&mut out, b"IDAT", &z);
    chunk(&mut out, b"IEND", &[]);
    out
}

fn jpeg(w: u32, h: u32, dpi: u16) -> Vec<u8> {
    use image::codecs::jpeg::{JpegEncoder, PixelDensity, PixelDensityUnit};
    let mut out = Vec::new();
    let mut enc = JpegEncoder::new_with_quality(&mut out, 90);
    enc.set_pixel_density(PixelDensity { density: (dpi, dpi), unit: PixelDensityUnit::Inches });
    enc.encode(&vec![128u8; (w * h) as usize], w, h, image::ExtendedColorType::L8).unwrap();
    out
}

#[test]
fn png_with_phys_metre_density_has_a_physical_size() {
    // 11811 px/m ≈ 300 dpi: 30 px = 30/11811 m.
    let info = inspect_raster(&png(30, 15, Some((11811, 11811, 1))), &RasterLimits::default()).unwrap();
    assert_eq!((info.format, info.width_px, info.height_px), (RasterFormat::Png, 30, 15));
    let (w, h) = info.physical_size(None).unwrap();
    assert!((w.mm() - 30.0 / 11811.0 * 1000.0).abs() < 1e-12, "{}", w.mm());
    assert!((h.mm() - 15.0 / 11811.0 * 1000.0).abs() < 1e-12);
    assert_eq!(info.media_type(), "image/png");
}

#[test]
fn missing_or_aspect_only_density_needs_an_explicit_dpi() {
    for phys in [None, Some((2, 1, 0))] {
        let info = inspect_raster(&png(30, 15, phys), &RasterLimits::default()).unwrap();
        assert_eq!(info.dpi, None);
        assert_eq!(info.physical_size(None), Err(RasterError::DpiRequired));
        let (w, _) = info.physical_size(Some(96.0)).unwrap();
        assert!((w.mm() - 30.0 / 96.0 * 25.4).abs() < 1e-12);
    }
    let info = inspect_raster(&png(4, 4, None), &RasterLimits::default()).unwrap();
    assert_eq!(info.physical_size(Some(0.0)), Err(RasterError::InvalidDpi));
}

#[test]
fn jpeg_jfif_density_gives_the_size() {
    let info = inspect_raster(&jpeg(40, 20, 254), &RasterLimits::default()).unwrap();
    assert_eq!((info.format, info.width_px, info.height_px), (RasterFormat::Jpeg, 40, 20));
    assert_eq!(info.dpi, Some((254.0, 254.0)));
    let (w, h) = info.physical_size(None).unwrap();
    assert!((w.mm() - 4.0).abs() < 1e-12 && (h.mm() - 2.0).abs() < 1e-12, "{w:?} {h:?}");
    assert_eq!(info.media_type(), "image/jpeg");
}

#[test]
fn damaged_unknown_and_oversized_images_are_typed_errors() {
    let good = png(8, 8, None);
    let mut truncated = good.clone();
    truncated.truncate(good.len() - 20);
    assert!(matches!(inspect_raster(&truncated, &RasterLimits::default()), Err(RasterError::Decode(_))));
    assert_eq!(inspect_raster(b"GIF89a....", &RasterLimits::default()), Err(RasterError::UnsupportedFormat));
    let small = RasterLimits { max_pixels: 10, ..RasterLimits::default() };
    assert!(matches!(inspect_raster(&good, &small), Err(RasterError::TooManyPixels { .. })));
    let tiny = RasterLimits { max_bytes: 10, ..RasterLimits::default() };
    assert!(matches!(inspect_raster(&good, &tiny), Err(RasterError::TooLarge { .. })));
}

#[test]
fn source_image_is_stored_as_original_bytes() {
    let bytes = png(3, 3, Some((3780, 3780, 1)));
    let info = inspect_raster(&bytes, &RasterLimits::default()).unwrap();
    let mut store = AssetStore::default();
    let key = store.add(info.media_type(), &bytes);
    assert_eq!(key, burnburn_io::asset_hash(&bytes), "content-addressed original pixels");
}
