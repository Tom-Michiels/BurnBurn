//! W4-UI-gaten (P03): BMP and TIFF decode to the same `SourceImage` contract as PNG/JPEG. The
//! fixtures are written byte by byte here (independent of the decoder): known sample values,
//! known resolution, and malformed variants that must be typed errors, never panics.

#![allow(clippy::unwrap_used)]

use burnburn_image::{DecodeError, DecodeLimits, PixelFormat, decode};

fn le16(v: u16) -> [u8; 2] {
    v.to_le_bytes()
}

fn le32(v: u32) -> [u8; 4] {
    v.to_le_bytes()
}

/// A bottom-up 24-bit BMP of 2 × 2 pixels: top row red, green; bottom row blue, white.
/// `ppm` is the horizontal and vertical pixels-per-metre field of the info header.
fn bmp_2x2(ppm: u32) -> Vec<u8> {
    let mut out = b"BM".to_vec();
    let pixel_offset = 14 + 40;
    let row = |a: [u8; 3], b: [u8; 3]| {
        // BGR order, rows padded to four bytes.
        vec![a[2], a[1], a[0], b[2], b[1], b[0], 0, 0]
    };
    let mut data = row([0, 0, 255], [255, 255, 255]); // bottom row first: blue, white
    data.extend(row([255, 0, 0], [0, 255, 0])); // then the top row: red, green
    out.extend(le32(u32::try_from(pixel_offset + data.len()).unwrap()));
    out.extend([0u8; 4]);
    out.extend(le32(pixel_offset as u32));
    out.extend(le32(40)); // BITMAPINFOHEADER
    out.extend(le32(2)); // width
    out.extend(le32(2)); // height (positive: bottom-up)
    out.extend(le16(1)); // planes
    out.extend(le16(24)); // bits per pixel
    out.extend(le32(0)); // BI_RGB
    out.extend(le32(data.len() as u32));
    out.extend(le32(ppm));
    out.extend(le32(ppm));
    out.extend(le32(0));
    out.extend(le32(0));
    out.extend(data);
    out
}

/// An uncompressed 8-bit grayscale TIFF (little endian, one strip) with a resolution in inches.
fn tiff_gray(width: u16, height: u16, samples: &[u8], dpi: Option<u32>) -> Vec<u8> {
    let entries: u16 = if dpi.is_some() { 12 } else { 9 };
    let ifd_at = 16u32; // 8 byte header, strip data padded to 16
    let after_ifd = ifd_at + 2 + u32::from(entries) * 12 + 4;
    let mut out = b"II*\0".to_vec();
    out.extend(le32(ifd_at));
    out.extend(samples);
    out.resize(ifd_at as usize, 0);
    out.extend(le16(entries));
    let entry = |tag: u16, kind: u16, value: u32| -> Vec<u8> {
        let mut e = le16(tag).to_vec();
        e.extend(le16(kind));
        e.extend(le32(1));
        if kind == 3 {
            e.extend(le16(u16::try_from(value).unwrap()));
            e.extend([0, 0]);
        } else {
            e.extend(le32(value));
        }
        e
    };
    out.extend(entry(256, 3, u32::from(width)));
    out.extend(entry(257, 3, u32::from(height)));
    out.extend(entry(258, 3, 8));
    out.extend(entry(259, 3, 1)); // no compression
    out.extend(entry(262, 3, 1)); // BlackIsZero
    out.extend(entry(273, 4, 8)); // strip offset
    out.extend(entry(277, 3, 1));
    out.extend(entry(278, 3, u32::from(height)));
    out.extend(entry(279, 4, samples.len() as u32));
    if dpi.is_some() {
        out.extend(entry(282, 5, after_ifd));
        out.extend(entry(283, 5, after_ifd + 8));
        out.extend(entry(296, 3, 2)); // inches
    }
    out.extend(le32(0)); // no next IFD
    if let Some(dpi) = dpi {
        for _ in 0..2 {
            out.extend(le32(dpi));
            out.extend(le32(1));
        }
    }
    out
}

#[test]
fn a_bmp_decodes_to_the_pixels_it_holds_top_row_first() {
    let img = decode(&bmp_2x2(0), DecodeLimits::default()).unwrap();
    assert_eq!((img.width(), img.height(), img.format()), (2, 2, PixelFormat::Rgb8));
    assert_eq!(img.samples(), &[255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255]);
    assert_eq!(img.metadata().dpi_suggestion, None, "no resolution in the file, none invented");
}

#[test]
fn a_bmp_resolution_in_pixels_per_metre_is_offered_as_dpi() {
    let img = decode(&bmp_2x2(10_000), DecodeLimits::default()).unwrap();
    let (x, y) = img.metadata().dpi_suggestion.expect("254 dpi");
    assert!((x - 254.0).abs() < 1e-9 && (y - 254.0).abs() < 1e-9, "{x} {y}");
}

#[test]
fn a_tiff_decodes_to_the_gray_samples_it_holds() {
    let img = decode(&tiff_gray(3, 2, &[10, 20, 30, 40, 50, 60], None), DecodeLimits::default()).unwrap();
    assert_eq!((img.width(), img.height(), img.format()), (3, 2, PixelFormat::Gray8));
    assert_eq!(img.samples(), &[10, 20, 30, 40, 50, 60]);
    assert_eq!(img.metadata().dpi_suggestion, None);
}

#[test]
fn a_tiff_resolution_in_inches_is_offered_as_dpi() {
    let img = decode(&tiff_gray(3, 2, &[10, 20, 30, 40, 50, 60], Some(300)), DecodeLimits::default()).unwrap();
    let (x, y) = img.metadata().dpi_suggestion.expect("300 dpi");
    assert!((x - 300.0).abs() < 1e-9 && (y - 300.0).abs() < 1e-9, "{x} {y}");
}

#[test]
fn truncated_or_lying_bmp_and_tiff_files_are_typed_errors() {
    let bmp = bmp_2x2(0);
    let tiff = tiff_gray(3, 2, &[10, 20, 30, 40, 50, 60], Some(300));
    for cut in [1, 14, 30, 54, bmp.len() - 1] {
        assert!(decode(&bmp[..cut], DecodeLimits::default()).is_err(), "bmp cut at {cut}");
    }
    for cut in [1, 8, 20, 100, tiff.len() - 20] {
        assert!(decode(&tiff[..cut], DecodeLimits::default()).is_err(), "tiff cut at {cut}");
    }
    // A header that claims far more pixels than the limit allows is refused from the header.
    let mut huge = bmp_2x2(0);
    huge[18..22].copy_from_slice(&le32(20_000));
    huge[22..26].copy_from_slice(&le32(20_000));
    assert!(matches!(decode(&huge, DecodeLimits { max_pixels: 1_000_000 }), Err(DecodeError::TooLarge { .. })));
    let mut wide = tiff_gray(3, 2, &[0; 6], None);
    wide[18..20].copy_from_slice(&le16(60_000)); // ImageWidth value in the first IFD entry
    assert!(decode(&wide, DecodeLimits { max_pixels: 1_000 }).is_err());
}
