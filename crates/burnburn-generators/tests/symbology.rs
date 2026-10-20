//! The 29 LightBurn bar-code types behind one API (W2-L5d). Oracles are independent of the encoders:
//! rasterised module rectangles are read back by `rxing` (a ZXing / zxing-cpp port), the EAN/UPC/Code 128
//! patterns are rebuilt from the published symbology tables inside this file, and check digits are
//! computed by hand-written reference functions. Module-level references from other encoders live in
//! `symbology_reference.rs`.

#![allow(clippy::unwrap_used, clippy::panic)]

use burnburn_generators::{
    EncodedSymbol, GenError, QrEcc, SymbolError, SymbolInputMode, SymbolKind, SymbolLayout, SymbolSource, SymbolStatus,
    Symbology, SymbologyParams, code128_patterns, encode_symbol, qr_encode, symbology_catalog,
};
use burnburn_geometry::Segment;
use rxing::{BarcodeFormat, DecodeHints};

// ---------------------------------------------------------------------------------------------
// helpers: rasteriser and decoder (they only look at the public path / layout, never at an encoder)
// ---------------------------------------------------------------------------------------------

fn params(module_mm: f64) -> SymbologyParams {
    SymbologyParams { height_mm: 12.0, ..SymbologyParams::new(module_mm, 0.1) }
}

fn encode(sym: Symbology, payload: &str) -> EncodedSymbol {
    encode_symbol(sym, payload, &sample_params(sym)).unwrap_or_else(|e| panic!("{sym:?} {payload:?}: {e}"))
}

fn sample_params(sym: Symbology) -> SymbologyParams {
    let mut p = params(0.4);
    p.input_mode = match sym {
        Symbology::Code128Gs1 => Some(SymbolInputMode::Gs1),
        _ => None,
    };
    p
}

/// One closed axis-aligned rectangle per subpath: (min x, min y, max x, max y) in mm.
fn rectangles(sym: &EncodedSymbol) -> Vec<(f64, f64, f64, f64)> {
    sym.path
        .subpaths()
        .iter()
        .map(|s| {
            assert!(s.is_closed(), "every subpath is closed");
            let mut pts = vec![s.start()];
            for seg in s.segments() {
                match seg {
                    Segment::Line { to } => pts.push(*to),
                    other => panic!("not a straight edge: {other:?}"),
                }
            }
            let (x0, x1) = pts.iter().fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p.x), b.max(p.x)));
            let (y0, y1) = pts.iter().fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p.y), b.max(p.y)));
            for p in &pts {
                assert!((p.x == x0 || p.x == x1) && (p.y == y0 || p.y == y1), "corner of an axis-aligned rectangle");
            }
            (x0, y0, x1, y1)
        })
        .collect()
}

/// Luma image of the symbol: `px_per_module` pixels per module, plus a white margin of `margin` pixels.
fn rasterise(sym: &EncodedSymbol, px_per_module: f64, margin: usize) -> (Vec<u8>, u32, u32) {
    let scale = px_per_module / sym.module_mm;
    let w = (sym.size_mm.0 * scale).round() as usize;
    let h = (sym.size_mm.1 * scale).round() as usize;
    let (full_w, full_h) = (w + 2 * margin, h + 2 * margin);
    let mut img = vec![255u8; full_w * full_h];
    for (x0, y0, x1, y1) in rectangles(sym) {
        let (c0, c1) = ((x0 * scale).round() as usize, (x1 * scale).round() as usize);
        // y is up in the symbol, down in the image.
        let (r0, r1) = (h - (y1 * scale).round() as usize, h - (y0 * scale).round() as usize);
        for r in r0..r1 {
            for c in c0..c1 {
                img[(r + margin) * full_w + c + margin] = 0;
            }
        }
    }
    (img, full_w as u32, full_h as u32)
}

fn format_of(sym: Symbology, canonical: &str) -> BarcodeFormat {
    match sym {
        Symbology::Codabar => BarcodeFormat::CODABAR,
        Symbology::Code128 | Symbology::Code128Gs1 => BarcodeFormat::CODE_128,
        Symbology::Code93 => BarcodeFormat::CODE_93,
        Symbology::Vin => BarcodeFormat::CODE_39,
        Symbology::Ean | Symbology::Isbn => {
            if canonical.len() == 8 {
                BarcodeFormat::EAN_8
            } else {
                BarcodeFormat::EAN_13
            }
        }
        Symbology::UpcA => BarcodeFormat::UPC_A,
        Symbology::UpcE => BarcodeFormat::UPC_E,
        Symbology::Aztec => BarcodeFormat::AZTEC,
        Symbology::DataMatrix => BarcodeFormat::DATA_MATRIX,
        Symbology::Pdf417 | Symbology::Pdf417Compact => BarcodeFormat::PDF_417,
        Symbology::QrCode => BarcodeFormat::QR_CODE,
        Symbology::QrMicro => BarcodeFormat::MICRO_QR_CODE,
        Symbology::QrMicroRect => BarcodeFormat::RECTANGULAR_MICRO_QR_CODE,
        other => panic!("no independent decoder for {other:?}"),
    }
}

/// Reads the symbol back with rxing at 8 px per module.
fn read_back(sym: &EncodedSymbol) -> Result<String, String> {
    let (img, w, h) = rasterise(sym, 8.0, 24);
    let mut hints = DecodeHints::default();
    if sym.symbology == Symbology::Codabar {
        hints.ReturnCodabarStartEnd = Some(true);
    }
    if sym.symbology == Symbology::Code128Gs1 {
        // Without this hint the decoder drops every FNC1; with it the leading FNC1 shows as the GS1 symbology
        // identifier ]C1 and every later FNC1 as GS (0x1d).
        hints.AssumeGs1 = Some(true);
    }
    rxing::helpers::detect_in_luma_with_hints(
        img,
        w,
        h,
        Some(format_of(sym.symbology, &sym.canonical_payload)),
        &mut hints,
    )
    .map(|r| {
        let text = r.getText().to_string();
        if sym.symbology == Symbology::Codabar {
            // rxing 0.9.3 returns the *indices* into ZXing's alphabet when it is asked to keep the
            // start/stop letters; map them back with that alphabet.
            const ALPHABET: &[u8] = b"0123456789-$:/.+ABCD";
            return text.chars().map(|c| ALPHABET.get(c as usize).map_or(c, |&b| b as char)).collect();
        }
        text
    })
    .map_err(|e| format!("{e:?}"))
}

fn matrix(sym: &EncodedSymbol) -> (usize, usize, &Vec<bool>) {
    match &sym.layout {
        SymbolLayout::Matrix { width, height, modules } => (*width, *height, modules),
        other => panic!("not a matrix: {other:?}"),
    }
}

fn widths(sym: &EncodedSymbol) -> &Vec<u8> {
    match &sym.layout {
        SymbolLayout::Linear { widths } => widths,
        other => panic!("not linear: {other:?}"),
    }
}

/// Bar/space widths expanded to modules (true = bar).
fn linear_modules(sym: &EncodedSymbol) -> Vec<bool> {
    widths(sym).iter().enumerate().flat_map(|(i, &w)| std::iter::repeat_n(i % 2 == 0, usize::from(w))).collect()
}

fn bits(s: &str) -> Vec<bool> {
    s.chars().map(|c| c == '1').collect()
}

/// Every implemented symbology with a payload it accepts.
const SAMPLES: &[(Symbology, &str)] = &[
    (Symbology::TwoOfFiveIata, "0123456789"),
    (Symbology::TwoOfFiveIndustrial, "0123456789"),
    (Symbology::AprilTag16h5, "7"),
    (Symbology::AprilTag25h9, "7"),
    (Symbology::AprilTag36h10, "7"),
    (Symbology::AprilTag36h11, "7"),
    (Symbology::ArucoOpenCv7x7, "7"),
    (Symbology::Aztec, "Aztec 123"),
    (Symbology::DataMatrix, "DataMatrix 123"),
    (Symbology::Codabar, "A40156B"),
    (Symbology::Code128, "Hello-128 123"),
    (Symbology::Code128Gs1, "(01)09501101530003(10)ABC123(17)201231"),
    (Symbology::Code93, "CODE93 TEST"),
    (Symbology::Ean, "4006381333931"),
    (Symbology::Isbn, "978-3-16-148410-0"),
    (Symbology::Plessey, "1A2B3C"),
    (Symbology::UpcA, "036000291452"),
    (Symbology::UpcE, "04252614"),
    (Symbology::Vin, "1M8GDM9AXKP042788"),
    (Symbology::Pdf417, "PDF417 test 12345"),
    (Symbology::Pdf417Compact, "PDF417 test 12345"),
    (Symbology::Pdf417Micro, "PDF417 micro 123"),
    (Symbology::QrCode, "https://example.com/burnburn"),
    (Symbology::QrMicro, "12345678"),
    (Symbology::QrMicroRect, "https://example.com"),
];

// ---------------------------------------------------------------------------------------------
// catalogue
// ---------------------------------------------------------------------------------------------

#[test]
fn catalog_lists_the_29_lightburn_types_with_unique_stable_names() {
    let entries = symbology_catalog();
    assert_eq!(entries.len(), 29);
    assert_eq!(Symbology::ALL.len(), 29);
    let mut names: Vec<&str> = entries.iter().map(|e| e.symbology.stable_name()).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), 29, "stable names are unique");
    for e in &entries {
        assert_eq!(Symbology::from_stable_name(e.symbology.stable_name()), Some(e.symbology));
        assert!(!e.symbology.display_name().is_empty());
    }
    let display: Vec<&str> = entries.iter().map(|e| e.symbology.display_name()).collect();
    for lightburn in [
        "2 of 5",
        "2 of 5 IATA",
        "2 of 5 Industrial",
        "AprilTag 16h5",
        "AprilTag 25h9",
        "AprilTag 36h10",
        "AprilTag 36h11",
        "ArUco MIP 36h11",
        "ArUco OpenCV 7x7",
        "Aztec",
        "Chinese Sensible Code",
        "DataMatrix",
        "Grid Matrix",
        "CodaBar",
        "Code 128",
        "Code 128 GS1",
        "Code 93",
        "EAN",
        "ISBN",
        "Plessey",
        "UPC A",
        "UPC E",
        "VIN",
        "PDF417",
        "PDF417 Compact",
        "PDF417 Micro",
        "QR Code",
        "QR Micro",
        "QR Micro Rectangular",
    ] {
        assert!(display.contains(&lightburn), "{lightburn} is in the catalogue");
    }
    // The stable names are persisted in documents: pin a few.
    assert_eq!(Symbology::QrMicroRect.stable_name(), "qr_micro_rect");
    assert_eq!(Symbology::Code128Gs1.stable_name(), "code_128_gs1");
    assert_eq!(Symbology::from_stable_name("no_such_code"), None);
}

#[test]
fn catalog_status_names_a_source_or_an_honest_reason() {
    let status = |s: Symbology| s.status();
    let own = SymbolStatus::Implemented { source: SymbolSource::Own };
    let anyd = SymbolStatus::Implemented { source: SymbolSource::Library("anyd") };
    assert_eq!(status(Symbology::Code128), own);
    assert_eq!(status(Symbology::QrCode), own);
    assert_eq!(status(Symbology::AprilTag36h11), own);
    assert_eq!(status(Symbology::ArucoOpenCv7x7), own);
    for s in [
        Symbology::TwoOfFiveIata,
        Symbology::TwoOfFiveIndustrial,
        Symbology::Aztec,
        Symbology::DataMatrix,
        Symbology::Codabar,
        Symbology::Code128Gs1,
        Symbology::Code93,
        Symbology::Ean,
        Symbology::Isbn,
        Symbology::Plessey,
        Symbology::UpcA,
        Symbology::UpcE,
        Symbology::Vin,
        Symbology::Pdf417,
        Symbology::Pdf417Compact,
        Symbology::Pdf417Micro,
        Symbology::QrMicro,
        Symbology::QrMicroRect,
    ] {
        assert_eq!(status(s), anyd, "{s:?}");
    }
    for s in [Symbology::TwoOfFive, Symbology::ArucoMip36h11, Symbology::HanXin, Symbology::GridMatrix] {
        match status(s) {
            SymbolStatus::Unsupported { reason } => {
                assert!(reason.len() > 30, "{s:?}: reason is a sentence, not a stub")
            }
            other => panic!("{s:?} must be unsupported, got {other:?}"),
        }
    }
    let implemented =
        symbology_catalog().iter().filter(|e| matches!(e.status, SymbolStatus::Implemented { .. })).count();
    assert_eq!(implemented, 25);
    assert_eq!(SAMPLES.len(), implemented, "every implemented symbology has a sample");
    for (s, _) in SAMPLES {
        assert!(matches!(s.status(), SymbolStatus::Implemented { .. }), "{s:?}");
    }
}

#[test]
fn unsupported_symbologies_return_a_typed_error_and_never_a_pattern() {
    for s in [Symbology::TwoOfFive, Symbology::ArucoMip36h11, Symbology::HanXin, Symbology::GridMatrix] {
        let SymbolStatus::Unsupported { reason } = s.status() else { panic!("{s:?} should be unsupported") };
        for payload in ["12345", "A", ""] {
            let r = encode_symbol(s, payload, &params(0.4));
            assert_eq!(r, Err(SymbolError::Unsupported { symbology: s, reason }), "{s:?} {payload:?}");
        }
    }
}

#[test]
fn capabilities_describe_what_a_ui_may_offer() {
    use SymbolKind::{Linear, Marker, Matrix};
    for (s, _) in SAMPLES {
        let c = s.capabilities();
        let kind = match s {
            Symbology::TwoOfFiveIata
            | Symbology::TwoOfFiveIndustrial
            | Symbology::Codabar
            | Symbology::Code128
            | Symbology::Code128Gs1
            | Symbology::Code93
            | Symbology::Ean
            | Symbology::Isbn
            | Symbology::Plessey
            | Symbology::UpcA
            | Symbology::UpcE
            | Symbology::Vin => Linear,
            Symbology::AprilTag16h5
            | Symbology::AprilTag25h9
            | Symbology::AprilTag36h10
            | Symbology::AprilTag36h11
            | Symbology::ArucoOpenCv7x7 => Marker,
            _ => Matrix,
        };
        assert_eq!(c.kind, kind, "{s:?}");
        assert!(!c.input_modes.is_empty(), "{s:?}");
        assert!(!c.payload_hint.is_empty(), "{s:?}");
        if let Some(d) = c.default_ecc_step {
            assert!(usize::from(d) < c.ecc_steps.len(), "{s:?}: default step exists");
        } else {
            assert!(c.ecc_steps.is_empty(), "{s:?}: a ladder needs a default");
        }
        assert_eq!(c.uses_columns, matches!(s, Symbology::Pdf417 | Symbology::Pdf417Compact), "{s:?}");
    }
    assert_eq!(Symbology::QrCode.capabilities().marker_count, None);
    assert_eq!(Symbology::QrCode.capabilities().ecc_steps.len(), 4);
    assert_eq!(Symbology::QrCode.capabilities().ecc_steps[3].recovery_percent, Some(30));
    assert_eq!(Symbology::QrMicro.capabilities().ecc_steps.len(), 4);
    assert_eq!(Symbology::QrMicroRect.capabilities().ecc_steps.len(), 2);
    assert_eq!(Symbology::Pdf417.capabilities().ecc_steps.len(), 9);
    assert!(Symbology::Aztec.capabilities().ecc_steps.is_empty());
    assert_eq!(Symbology::Code128Gs1.capabilities().input_modes, &[SymbolInputMode::Gs1]);
    assert_eq!(Symbology::QrCode.capabilities().input_modes, &[SymbolInputMode::Data, SymbolInputMode::Unicode]);
    // The advertised quiet zone is what the encoder reports as the minimum (largest over variants).
    for (s, payload) in SAMPLES {
        let sym = encode(*s, payload);
        assert!(sym.min_quiet_modules <= s.capabilities().min_quiet_modules, "{s:?}");
    }
    assert_eq!(Symbology::Ean.capabilities().min_quiet_modules, 11);
}

// ---------------------------------------------------------------------------------------------
// linear symbologies: known vectors, independent tables, decoder round trips
// ---------------------------------------------------------------------------------------------

/// EAN/UPC element tables of ISO/IEC 15420 / GS1 General Specifications 5.2: L-code, and the parity
/// patterns that encode the first digit of an EAN-13.
const L_CODE: [&str; 10] =
    ["0001101", "0011001", "0010011", "0111101", "0100011", "0110001", "0101111", "0111011", "0110111", "0001011"];
const FIRST_DIGIT_PARITY: [&str; 10] =
    ["LLLLLL", "LLGLGG", "LLGGLG", "LLGGGL", "LGLLGG", "LGGLLG", "LGGGLL", "LGLGLG", "LGLGGL", "LGGLGL"];

fn l_code(d: usize) -> String {
    L_CODE[d].to_string()
}
fn r_code(d: usize) -> String {
    L_CODE[d].chars().map(|c| if c == '0' { '1' } else { '0' }).collect()
}
fn g_code(d: usize) -> String {
    r_code(d).chars().rev().collect()
}

fn expected_ean13(digits: &str) -> Vec<bool> {
    let d: Vec<usize> = digits.bytes().map(|b| usize::from(b - b'0')).collect();
    let mut s = String::from("101");
    for (i, &x) in d[1..7].iter().enumerate() {
        s += &if FIRST_DIGIT_PARITY[d[0]].as_bytes()[i] == b'L' { l_code(x) } else { g_code(x) };
    }
    s += "01010";
    for &x in &d[7..13] {
        s += &r_code(x);
    }
    s += "101";
    bits(&s)
}

#[test]
fn ean13_known_vector_has_the_standard_module_pattern_and_decodes() {
    let sym = encode(Symbology::Ean, "4006381333931");
    assert_eq!(sym.canonical_payload, "4006381333931");
    assert_eq!(linear_modules(&sym), expected_ean13("4006381333931"));
    assert_eq!(sym.layout.width_modules(), 95);
    assert_eq!(sym.min_quiet_modules, 11);
    assert_eq!(read_back(&sym).unwrap(), "4006381333931");
    // 12 digits: the check digit is computed.
    let short = encode(Symbology::Ean, "400638133393");
    assert_eq!(short.canonical_payload, "4006381333931");
    assert_eq!(short.layout, sym.layout);
}

#[test]
fn ean8_known_vector_decodes() {
    // GS1 example 96385074.
    let sym = encode(Symbology::Ean, "96385074");
    assert_eq!(sym.layout.width_modules(), 67);
    let mut want = String::from("101");
    for d in "9638".bytes() {
        want += &l_code(usize::from(d - b'0'));
    }
    want += "01010";
    for d in "5074".bytes() {
        want += &r_code(usize::from(d - b'0'));
    }
    want += "101";
    assert_eq!(linear_modules(&sym), bits(&want));
    assert_eq!(sym.min_quiet_modules, 7);
    assert_eq!(read_back(&sym).unwrap(), "96385074");
    assert_eq!(encode(Symbology::Ean, "9638507").canonical_payload, "96385074");
}

#[test]
fn upc_a_known_vector_is_an_ean13_with_a_leading_zero_and_decodes() {
    let sym = encode(Symbology::UpcA, "036000291452");
    assert_eq!(linear_modules(&sym), expected_ean13("0036000291452"));
    assert_eq!(sym.min_quiet_modules, 9);
    assert_eq!(read_back(&sym).unwrap(), "036000291452");
    assert_eq!(encode(Symbology::UpcA, "03600029145").canonical_payload, "036000291452");
}

#[test]
fn upc_e_zero_suppression_matches_the_published_expansion() {
    // Wikipedia / GS1: UPC-A 042100005264 is carried as UPC-E 04252614 (payload 425261, check 4).
    for input in ["425261", "0425261", "04252614"] {
        let sym = encode(Symbology::UpcE, input);
        assert_eq!(sym.canonical_payload, "04252614", "{input}");
        assert_eq!(sym.layout.width_modules(), 51);
        assert_eq!(read_back(&sym).unwrap(), "04252614");
    }
    // Number system 1 and every suppression pattern (last payload digit 0..9).
    let a = encode(Symbology::UpcE, "123456");
    assert_eq!(a.canonical_payload.len(), 8);
    assert_eq!(&a.canonical_payload[..7], "0123456");
}

#[test]
fn isbn_is_an_ean13_with_the_bookland_prefix() {
    let sym = encode(Symbology::Isbn, "978-3-16-148410-0");
    assert_eq!(sym.canonical_payload, "9783161484100");
    assert_eq!(linear_modules(&sym), expected_ean13("9783161484100"));
    assert_eq!(read_back(&sym).unwrap(), "9783161484100");
    // ISBN-10 is converted: 0-306-40615-2 -> 978-0-306-40615-7; X as check digit.
    assert_eq!(encode(Symbology::Isbn, "0-306-40615-2").canonical_payload, "9780306406157");
    assert_eq!(encode(Symbology::Isbn, "080442957X").canonical_payload, "9780804429573");
    assert_eq!(encode(Symbology::Isbn, "979-10-90636-07-1").canonical_payload, "9791090636071");
}

#[test]
fn vin_is_code39_with_the_north_american_check_digit() {
    let sym = encode(Symbology::Vin, "1M8GDM9AXKP042788");
    assert_eq!(sym.canonical_payload, "1M8GDM9AXKP042788");
    assert_eq!(read_back(&sym).unwrap(), "1M8GDM9AXKP042788");
    // All-ones VIN: weights sum to 89, 89 mod 11 = 1.
    assert_eq!(encode(Symbology::Vin, "11111111111111111").canonical_payload, "11111111111111111");
    // A European VIN has no mandatory check digit: accepted as is.
    assert_eq!(encode(Symbology::Vin, "WVWZZZ1JZXW000001").canonical_payload, "WVWZZZ1JZXW000001");
}

/// Decodes a Code 128 module run into symbol values with the pattern table of the standard.
fn code128_values(modules: &[bool]) -> Vec<usize> {
    let table = code128_patterns();
    let mut runs: Vec<u8> = Vec::new();
    let (mut cur, mut n) = (modules[0], 0u8);
    for &m in modules {
        if m == cur {
            n += 1;
        } else {
            runs.push(n);
            cur = m;
            n = 1;
        }
    }
    runs.push(n);
    let mut values = Vec::new();
    let mut i = 0;
    while i < runs.len() {
        // The stop pattern has seven elements and ends the symbol; every other symbol has six.
        let len = if runs.len() - i == 7 { 7 } else { 6 };
        let v = table
            .iter()
            .position(|p| **p == runs[i..i + len])
            .unwrap_or_else(|| panic!("unknown pattern {:?}", &runs[i..i + len]));
        values.push(v);
        i += len;
    }
    values
}

#[test]
fn code128_gs1_starts_with_fnc1_separates_variable_ais_and_has_a_valid_check_symbol() {
    let payload = "(01)09501101530003(10)ABC123(17)201231";
    let sym = encode(Symbology::Code128Gs1, payload);
    assert_eq!(sym.canonical_payload, "(01)09501101530003(10)ABC123(17)201231");
    let v = code128_values(&linear_modules(&sym));
    assert!((103..=105).contains(&v[0]), "start code {}", v[0]);
    assert_eq!(v[1], 102, "FNC1 right after the start code marks GS1");
    assert_eq!(*v.last().unwrap(), 106, "stop");
    let check = v[v.len() - 2];
    let weighted: usize = v[..v.len() - 2].iter().enumerate().map(|(i, &x)| i.max(1) * x).sum();
    assert_eq!(check, weighted % 103, "mod-103 check symbol");
    assert_eq!(
        v.iter().filter(|&&x| x == 102).count(),
        2,
        "one leading FNC1 and one separator after the variable AI (10)"
    );
    assert_eq!(read_back(&sym).unwrap(), "]C1010950110153000310ABC123\u{1d}17201231");
    // Fixed-length AIs only: no separator at all.
    let fixed = encode(Symbology::Code128Gs1, "(01)09501101530003(17)201231");
    let fv = code128_values(&linear_modules(&fixed));
    assert_eq!(fv.iter().filter(|&&x| x == 102).count(), 1);
    assert_eq!(read_back(&fixed).unwrap(), "]C1010950110153000317201231");
}

#[test]
fn code128_matches_the_existing_hand_checked_symbol_values() {
    // tests/personalisation.rs: "AB" -> [104, 33, 34, 102, 106]; digits in pairs use subset C.
    let sym = encode(Symbology::Code128, "AB");
    assert_eq!(code128_values(&linear_modules(&sym)), [104, 33, 34, 102, 106]);
    let digits = encode(Symbology::Code128, "1234");
    assert_eq!(code128_values(&linear_modules(&digits)), [105, 12, 34, 82, 106]);
    assert_eq!(read_back(&digits).unwrap(), "1234");
    assert_eq!(read_back(&encode(Symbology::Code128, "Hello-128 123")).unwrap(), "Hello-128 123");
    assert_eq!(sym.min_quiet_modules, 10);
}

#[test]
fn code93_and_codabar_decode_to_their_payload() {
    assert_eq!(read_back(&encode(Symbology::Code93, "CODE93 TEST")).unwrap(), "CODE93 TEST");
    assert_eq!(read_back(&encode(Symbology::Code93, "-$%./+ 123")).unwrap(), "-$%./+ 123");
    // Codabar keeps its start/stop letters (A40156B reads back with them when asked to).
    assert_eq!(read_back(&encode(Symbology::Codabar, "A40156B")).unwrap(), "A40156B");
    assert_eq!(read_back(&encode(Symbology::Codabar, "c-12.5$d")).unwrap(), "C-12.5$D");
}

/// Reference tables of the 2-of-5 family: digit patterns (wide = 3 modules, narrow = 1; positions weighted
/// 1-2-4-7-0, exactly two wide) and the Industrial / IATA start-stop of ISO/IEC 16390 style charts.
const TWO_OF_FIVE: [&str; 10] =
    ["NNWWN", "WNNNW", "NWNNW", "WWNNN", "NNWNW", "WNWNN", "NWWNN", "NNNWW", "WNNWN", "NWNWN"];

fn run(c: char) -> &'static str {
    if c == 'W' { "111" } else { "1" }
}

#[test]
fn two_of_five_industrial_and_iata_follow_the_digit_and_frame_tables() {
    let digits = "1234";
    let body = |digits: &str| {
        let mut s = String::new();
        for d in digits.bytes() {
            for c in TWO_OF_FIVE[usize::from(d - b'0')].chars() {
                s += run(c);
                s += "0"; // every bar is followed by a narrow space
            }
        }
        s
    };
    // Industrial: start = wide bar, narrow space, wide bar, narrow space, narrow bar, narrow space;
    // stop = wide bar, narrow space, narrow bar, narrow space, wide bar.
    let industrial = encode(Symbology::TwoOfFiveIndustrial, digits);
    assert_eq!(industrial.min_quiet_modules, 10);
    assert_eq!(linear_modules(&industrial), bits(&format!("1110111010{}111010111", body(digits))));
    // IATA: start = four narrow elements; stop = wide bar, narrow space, narrow bar.
    let iata = encode(Symbology::TwoOfFiveIata, digits);
    assert_eq!(linear_modules(&iata), bits(&format!("1010{}11101", body(digits))));
}

#[test]
fn plessey_draws_start_data_check_and_terminator_elements() {
    let sym = encode(Symbology::Plessey, "1A2B3C");
    assert_eq!(sym.canonical_payload, "1A2B3C");
    // 8 start elements + 8 per hex digit (4 bits, a bar and a space each) + 16 for the two CRC digits + 9 terminator
    // elements; the element-by-element pattern is checked against the BWIPP reference in symbology_reference.rs.
    assert_eq!(widths(&sym).len(), 8 + 6 * 8 + 16 + 9);
    assert_eq!(sym.min_quiet_modules, 10);
    // Different data, different CRC: only the data and CRC elements change, never the frame.
    let other = encode(Symbology::Plessey, "1A2B3D");
    assert_eq!(widths(&other).len(), widths(&sym).len());
    assert_eq!(widths(&other)[..8], widths(&sym)[..8], "same start");
    assert_ne!(widths(&other), widths(&sym));
}

// ---------------------------------------------------------------------------------------------
// validation of payloads
// ---------------------------------------------------------------------------------------------

fn err(sym: Symbology, payload: &str) -> SymbolError {
    encode_symbol(sym, payload, &sample_params(sym)).unwrap_err()
}

#[test]
fn ean_upc_and_isbn_payloads_are_checked_by_length_character_and_check_digit() {
    use Symbology::{Ean, Isbn, UpcA, UpcE};
    assert_eq!(err(Ean, ""), SymbolError::EmptyPayload { symbology: Ean });
    assert!(matches!(err(Ean, "12345"), SymbolError::InvalidPayload { symbology: Ean, .. }));
    assert!(matches!(err(Ean, "40063813339"), SymbolError::InvalidPayload { symbology: Ean, .. }), "11 digits");
    assert_eq!(err(Ean, "40063813339A1"), SymbolError::InvalidChar { symbology: Ean, ch: 'A', index: 11 });
    assert_eq!(err(Ean, "4006381333932"), SymbolError::CheckDigit { symbology: Ean, expected: '1', found: '2' });
    assert_eq!(err(Ean, "96385075"), SymbolError::CheckDigit { symbology: Ean, expected: '4', found: '5' });
    assert!(matches!(err(UpcA, "0360002914"), SymbolError::InvalidPayload { symbology: UpcA, .. }));
    assert_eq!(err(UpcA, "036000291453"), SymbolError::CheckDigit { symbology: UpcA, expected: '2', found: '3' });
    assert_eq!(err(UpcE, "04252615"), SymbolError::CheckDigit { symbology: UpcE, expected: '4', found: '5' });
    assert!(matches!(err(UpcE, "2425261"), SymbolError::InvalidPayload { symbology: UpcE, .. }), "number system 2");
    assert!(matches!(err(UpcE, "42526"), SymbolError::InvalidPayload { symbology: UpcE, .. }));
    assert_eq!(err(Isbn, "9783161484101"), SymbolError::CheckDigit { symbology: Isbn, expected: '0', found: '1' });
    assert_eq!(err(Isbn, "0-306-40615-3"), SymbolError::CheckDigit { symbology: Isbn, expected: '2', found: '3' });
    assert!(
        matches!(err(Isbn, "9773161484100"), SymbolError::InvalidPayload { symbology: Isbn, .. }),
        "977 is not Bookland"
    );
    assert!(matches!(err(Isbn, "12345"), SymbolError::InvalidPayload { symbology: Isbn, .. }));
}

#[test]
fn vin_codabar_plessey_and_two_of_five_payloads_are_checked() {
    use Symbology::{Codabar, Plessey, TwoOfFiveIata, TwoOfFiveIndustrial, Vin};
    assert!(matches!(
        err(Vin, "1M8GDM9AXKP04278"),
        SymbolError::PayloadLength { symbology: Vin, min: 17, max: 17, len: 16 }
    ));
    assert_eq!(err(Vin, "1M8GDM9AXKP04278O"), SymbolError::InvalidChar { symbology: Vin, ch: 'O', index: 16 });
    assert_eq!(err(Vin, "1M8GDM9AXKP04278I"), SymbolError::InvalidChar { symbology: Vin, ch: 'I', index: 16 });
    assert_eq!(err(Vin, "1M8GDM9AXKP04278Q"), SymbolError::InvalidChar { symbology: Vin, ch: 'Q', index: 16 });
    assert_eq!(err(Vin, "1M8GDM9AYKP042788"), SymbolError::CheckDigit { symbology: Vin, expected: 'X', found: 'Y' });
    assert!(
        matches!(err(Codabar, "40156"), SymbolError::InvalidPayload { symbology: Codabar, .. }),
        "no start/stop letters"
    );
    assert!(
        matches!(err(Codabar, "A40156E"), SymbolError::InvalidPayload { symbology: Codabar, .. }),
        "E is not a guard"
    );
    assert_eq!(err(Codabar, "A40x56B"), SymbolError::InvalidChar { symbology: Codabar, ch: 'x', index: 3 });
    assert_eq!(err(Plessey, "12G4"), SymbolError::InvalidChar { symbology: Plessey, ch: 'G', index: 2 });
    assert_eq!(err(Plessey, ""), SymbolError::EmptyPayload { symbology: Plessey });
    assert_eq!(
        err(TwoOfFiveIndustrial, "12a4"),
        SymbolError::InvalidChar { symbology: TwoOfFiveIndustrial, ch: 'a', index: 2 }
    );
    assert_eq!(err(TwoOfFiveIata, ""), SymbolError::EmptyPayload { symbology: TwoOfFiveIata });
    assert_eq!(encode(Plessey, "abcd").canonical_payload, "ABCD");
}

#[test]
fn gs1_element_strings_are_parsed_and_their_check_digits_verified() {
    let g = Symbology::Code128Gs1;
    assert!(matches!(err(g, "09501101530003"), SymbolError::InvalidPayload { symbology: Symbology::Code128Gs1, .. }));
    assert!(
        matches!(err(g, "(01)0950110153000"), SymbolError::InvalidPayload { symbology: Symbology::Code128Gs1, .. }),
        "13 digits"
    );
    assert_eq!(err(g, "(01)09501101530004"), SymbolError::CheckDigit { symbology: g, expected: '3', found: '4' });
    assert_eq!(err(g, "(1234)x"), SymbolError::UnknownAi { ai: "1234".to_string() });
    assert!(
        matches!(err(g, "(10)"), SymbolError::InvalidPayload { symbology: Symbology::Code128Gs1, .. }),
        "empty value"
    );
    assert!(
        matches!(err(g, "(17)20123"), SymbolError::InvalidPayload { symbology: Symbology::Code128Gs1, .. }),
        "date needs 6 digits"
    );
    assert!(matches!(err(g, "(10)AB(C"), SymbolError::InvalidPayload { symbology: Symbology::Code128Gs1, .. }));
    assert_eq!(err(g, "(10)ABé"), SymbolError::InvalidChar { symbology: g, ch: 'é', index: 6 });
    // Data mode is not GS1: the type needs the GS1 input mode.
    let mut p = params(0.4);
    p.input_mode = Some(SymbolInputMode::Data);
    assert_eq!(
        encode_symbol(g, "(01)09501101530003", &p),
        Err(SymbolError::InputModeUnavailable { symbology: g, mode: SymbolInputMode::Data })
    );
    // Unknown-length AI list must not be silently accepted; a known fixed + variable mix is.
    assert_eq!(encode(g, "(10)LOT7(21)SN9").canonical_payload, "(10)LOT7(21)SN9");
}

#[test]
fn code128_code93_and_text_codes_refuse_what_they_cannot_carry() {
    assert_eq!(err(Symbology::Code128, ""), SymbolError::EmptyPayload { symbology: Symbology::Code128 });
    assert_eq!(
        err(Symbology::Code128, "Aé"),
        SymbolError::InvalidChar { symbology: Symbology::Code128, ch: 'é', index: 1 }
    );
    assert_eq!(
        err(Symbology::Code93, "abc"),
        SymbolError::InvalidChar { symbology: Symbology::Code93, ch: 'a', index: 0 }
    );
    for s in [
        Symbology::Aztec,
        Symbology::DataMatrix,
        Symbology::Pdf417,
        Symbology::QrCode,
        Symbology::QrMicro,
        Symbology::QrMicroRect,
    ] {
        assert_eq!(err(s, ""), SymbolError::EmptyPayload { symbology: s }, "{s:?}");
    }
}

// ---------------------------------------------------------------------------------------------
// 2D symbologies: decoder round trips
// ---------------------------------------------------------------------------------------------

#[test]
fn matrix_and_stacked_symbols_decode_to_their_payload() {
    for (s, payload) in [
        (Symbology::DataMatrix, "DataMatrix 123"),
        (Symbology::Aztec, "Aztec 123"),
        (Symbology::Pdf417, "PDF417 test 12345"),
        (Symbology::Pdf417, "1234567890123456789"),
        (Symbology::QrCode, "https://example.com/burnburn"),
        (Symbology::QrMicro, "12345678"),
        (Symbology::QrMicro, "HELLO"),
        (Symbology::QrMicroRect, "https://example.com"),
        (Symbology::QrMicroRect, "0123456789"),
    ] {
        let sym = encode(s, payload);
        assert_eq!(read_back(&sym).as_deref(), Ok(payload), "{s:?}");
    }
}

#[test]
fn input_mode_selects_latin1_bytes_or_utf8() {
    // Unicode: UTF-8 bytes (rMQR names it with ECI 26), read back by the decoder.
    let mut p = params(0.4);
    p.input_mode = Some(SymbolInputMode::Unicode);
    for s in [Symbology::QrCode, Symbology::QrMicro, Symbology::QrMicroRect] {
        let sym = encode_symbol(s, "Grüße €", &p).unwrap_or_else(|e| panic!("{s:?}: {e}"));
        assert_eq!(read_back(&sym).as_deref(), Ok("Grüße €"), "{s:?}");
    }
    // The other 2D codes have no way to name UTF-8 (or default to ISO 8859-1): Unicode is not offered.
    for s in
        [Symbology::DataMatrix, Symbology::Aztec, Symbology::Pdf417, Symbology::Pdf417Compact, Symbology::Pdf417Micro]
    {
        assert_eq!(s.capabilities().input_modes, &[SymbolInputMode::Data], "{s:?}");
        assert_eq!(
            encode_symbol(s, "Gr\u{fc}", &p),
            Err(SymbolError::InputModeUnavailable { symbology: s, mode: SymbolInputMode::Unicode }),
            "{s:?}"
        );
    }
    // Data: one byte per character, which is what an ISO 8859-1 reader (the default of DataMatrix, Aztec) expects.
    for s in [Symbology::DataMatrix, Symbology::Aztec] {
        let sym = encode_symbol(s, "Gr\u{fc}\u{df}e 42", &params(0.4)).unwrap();
        assert_eq!(read_back(&sym).as_deref(), Ok("Gr\u{fc}\u{df}e 42"), "{s:?}");
    }
    // Data: the QR layout is exactly the byte-mode QR of those bytes.
    let data = encode_symbol(Symbology::QrCode, "Gr\u{fc}", &params(0.4)).unwrap();
    let want = qr_encode(&[b'G', b'r', 0xfc], QrEcc::L).unwrap();
    let (n, _, modules) = matrix(&data);
    assert_eq!(n, want.size());
    for r in 0..n {
        for c in 0..n {
            assert_eq!(modules[r * n + c], want.is_dark(r, c), "module {r},{c}");
        }
    }
    // A character that is not one byte is refused in Data mode, not truncated.
    assert_eq!(
        encode_symbol(Symbology::QrCode, "a€", &params(0.4)),
        Err(SymbolError::InvalidChar { symbology: Symbology::QrCode, ch: '€', index: 1 })
    );
    // GS1 input mode on a symbology that has no FNC1 marker.
    let mut g = params(0.4);
    g.input_mode = Some(SymbolInputMode::Gs1);
    assert_eq!(
        encode_symbol(Symbology::QrCode, "(01)09501101530003", &g),
        Err(SymbolError::InputModeUnavailable { symbology: Symbology::QrCode, mode: SymbolInputMode::Gs1 })
    );
}

#[test]
fn error_correction_steps_map_to_the_symbology_ladder() {
    // QR: step 0..3 = L, M, Q, H; the module matrix equals the existing own encoder at that level.
    for (step, ecc) in [(0u8, QrEcc::L), (1, QrEcc::M), (2, QrEcc::Q), (3, QrEcc::H)] {
        let mut p = params(0.4);
        p.ecc_step = Some(step);
        let sym = encode_symbol(Symbology::QrCode, "step test 12345", &p).unwrap();
        let want = qr_encode(b"step test 12345", ecc).unwrap();
        let (n, _, modules) = matrix(&sym);
        assert_eq!(n, want.size(), "step {step}");
        assert!((0..n).all(|r| (0..n).all(|c| modules[r * n + c] == want.is_dark(r, c))), "step {step}");
    }
    // Default is Low.
    let default = encode(Symbology::QrCode, "step test 12345");
    let mut p = params(0.4);
    p.ecc_step = Some(0);
    assert_eq!(default.layout, encode_symbol(Symbology::QrCode, "step test 12345", &p).unwrap().layout);
    // Micro QR: step 0 is detection-only M1 (11 x 11), step 1 (L) needs at least M2.
    let mut p = params(0.4);
    p.ecc_step = Some(0);
    assert_eq!(matrix(&encode_symbol(Symbology::QrMicro, "12345", &p).unwrap()).0, 11);
    p.ecc_step = Some(1);
    assert!(matrix(&encode_symbol(Symbology::QrMicro, "12345", &p).unwrap()).0 >= 13);
    // rMQR: H needs more area than M for the same text.
    let (mut m, mut h) = (params(0.4), params(0.4));
    m.ecc_step = Some(0);
    h.ecc_step = Some(1);
    let area = |s: &EncodedSymbol| matrix(s).0 * matrix(s).1;
    let text = "https://example.com/abc";
    assert!(
        area(&encode_symbol(Symbology::QrMicroRect, text, &h).unwrap())
            > area(&encode_symbol(Symbology::QrMicroRect, text, &m).unwrap())
    );
    // PDF417: more security levels, more rows at the same column count.
    let (mut lo, mut hi) = (params(0.4), params(0.4));
    lo.columns = Some(20);
    hi.columns = Some(20);
    lo.ecc_step = Some(0);
    hi.ecc_step = Some(8);
    let rows = |s: &EncodedSymbol| matrix(s).1;
    let text = "PDF417 security levels 1234567890";
    assert!(
        rows(&encode_symbol(Symbology::Pdf417, text, &hi).unwrap())
            > rows(&encode_symbol(Symbology::Pdf417, text, &lo).unwrap())
    );
    let high = encode_symbol(Symbology::Pdf417, text, &hi).unwrap();
    assert_eq!(read_back(&high).as_deref(), Ok(text));
    // Steps that do not exist are refused with the ladder length.
    let mut bad = params(0.4);
    bad.ecc_step = Some(4);
    assert_eq!(
        encode_symbol(Symbology::QrCode, "x", &bad),
        Err(SymbolError::EccStepUnavailable { symbology: Symbology::QrCode, step: 4, steps: 4 })
    );
    bad.ecc_step = Some(0);
    assert_eq!(
        encode_symbol(Symbology::Aztec, "x", &bad),
        Err(SymbolError::EccStepUnavailable { symbology: Symbology::Aztec, step: 0, steps: 0 })
    );
}

#[test]
fn pdf417_columns_set_the_symbol_width_and_other_symbologies_have_none() {
    let mut p = params(0.4);
    p.columns = Some(2);
    let narrow = encode_symbol(Symbology::Pdf417, "PDF417 column test 1234567890", &p).unwrap();
    p.columns = Some(6);
    let wide = encode_symbol(Symbology::Pdf417, "PDF417 column test 1234567890", &p).unwrap();
    // start 17 + left indicator 17 + 17 per data column + right indicator 17 + stop 18
    assert_eq!(matrix(&narrow).0, 17 + 17 + 17 * 2 + 17 + 18);
    assert_eq!(matrix(&wide).0, 17 + 17 + 17 * 6 + 17 + 18);
    assert!(matrix(&narrow).1 > matrix(&wide).1);
    // The compact form drops the right indicator and shrinks the stop pattern to one bar.
    p.columns = Some(4);
    let compact = encode_symbol(Symbology::Pdf417Compact, "PDF417 column test 1234567890", &p).unwrap();
    assert_eq!(matrix(&compact).0, 17 + 17 + 17 * 4 + 1);
    p.columns = Some(31);
    assert!(matches!(
        encode_symbol(Symbology::Pdf417, "x", &p),
        Err(SymbolError::Param(GenError::Count { param: "columns", min: 1, max: 30, value: 31 }))
    ));
    p.columns = Some(3);
    assert_eq!(
        encode_symbol(Symbology::QrCode, "x", &p),
        Err(SymbolError::ParamNotApplicable { symbology: Symbology::QrCode, param: "columns" })
    );
}

#[test]
fn payloads_beyond_the_largest_symbol_are_refused_with_a_reason() {
    let long = "9".repeat(5000);
    for s in [Symbology::QrMicro, Symbology::QrMicroRect, Symbology::Pdf417Micro] {
        assert!(
            matches!(encode_symbol(s, &long, &params(0.4)), Err(SymbolError::DataTooLong { symbology, .. }) if symbology == s),
            "{s:?}"
        );
    }
    // Micro QR holds 35 digits at most (M4-L).
    assert!(encode_symbol(Symbology::QrMicro, &"1".repeat(35), &params(0.4)).is_ok());
    assert!(matches!(
        encode_symbol(Symbology::QrMicro, &"1".repeat(36), &params(0.4)),
        Err(SymbolError::DataTooLong { .. })
    ));
    // The own QR encoder stops at version 10.
    assert!(matches!(
        encode_symbol(Symbology::QrCode, &"x".repeat(300), &params(0.4)),
        Err(SymbolError::DataTooLong { .. })
    ));
    // Absurd input is refused up front.
    assert!(matches!(
        encode_symbol(Symbology::Code128, &"a".repeat(100_000), &params(0.4)),
        Err(SymbolError::PayloadLength { symbology: Symbology::Code128, .. })
    ));
    for s in [Symbology::Ean, Symbology::UpcA] {
        assert!(
            matches!(
                encode_symbol(s, &"1".repeat(40), &params(0.4)),
                Err(SymbolError::InvalidPayload { .. } | SymbolError::PayloadLength { .. })
            ),
            "{s:?}"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// physical geometry and parameter boundaries
// ---------------------------------------------------------------------------------------------

#[test]
fn geometry_has_real_millimetre_extents_and_exact_dark_area() {
    for (s, payload) in SAMPLES {
        let mut p = sample_params(*s);
        p.module_mm = 0.25;
        p.height_mm = 9.0;
        let sym = encode_symbol(*s, payload, &p).unwrap();
        let q = f64::from(sym.quiet_modules);
        let rects = rectangles(&sym);
        assert!(!rects.is_empty(), "{s:?}");
        let (wm, hm, dark_modules) = match &sym.layout {
            SymbolLayout::Linear { widths } => {
                let bars: usize = widths.iter().step_by(2).map(|&w| usize::from(w)).sum();
                (sym.layout.width_modules(), None, bars)
            }
            SymbolLayout::Matrix { width, height, modules } => {
                (*width, Some(*height), modules.iter().filter(|&&d| d).count())
            }
        };
        let want_w = (wm as f64 + 2.0 * q) * 0.25;
        let want_h = match hm {
            Some(h) => (h as f64 + 2.0 * q) * 0.25,
            None => 9.0,
        };
        assert!((sym.size_mm.0 - want_w).abs() < 1e-9, "{s:?} width {} vs {want_w}", sym.size_mm.0);
        assert!((sym.size_mm.1 - want_h).abs() < 1e-9, "{s:?} height {} vs {want_h}", sym.size_mm.1);
        let b = sym.path.bounds().unwrap();
        assert!(
            b.min().x >= q * 0.25 - 1e-9 && b.max().x <= sym.size_mm.0 - q * 0.25 + 1e-9,
            "{s:?}: inside the quiet zone in x"
        );
        let (low, high) = if hm.is_some() { (q * 0.25, sym.size_mm.1 - q * 0.25) } else { (0.0, 9.0) };
        assert!(b.min().y >= low - 1e-9 && b.max().y <= high + 1e-9, "{s:?}: inside the quiet zone in y");
        let area: f64 = sym.path.subpaths().iter().map(|sp| sp.signed_area().unwrap().abs()).sum();
        let module_area = if hm.is_some() { 0.25 * 0.25 } else { 0.25 * 9.0 };
        assert!(
            (area - dark_modules as f64 * module_area).abs() < 1e-6,
            "{s:?}: area {area} vs {} dark modules",
            dark_modules
        );
        assert_eq!(sym.module_mm, 0.25);
    }
}

#[test]
fn every_sample_reads_back_through_the_independent_decoder() {
    let no_decoder = [
        Symbology::TwoOfFiveIata,
        Symbology::TwoOfFiveIndustrial,
        Symbology::AprilTag16h5,
        Symbology::AprilTag25h9,
        Symbology::AprilTag36h10,
        Symbology::AprilTag36h11,
        Symbology::ArucoOpenCv7x7,
        Symbology::Plessey,
        Symbology::Pdf417Micro,
    ];
    for (s, payload) in SAMPLES {
        if no_decoder.contains(s) {
            continue;
        }
        let sym = encode(*s, payload);
        let got = read_back(&sym).unwrap_or_else(|e| panic!("{s:?} did not decode: {e}"));
        let want = match s {
            Symbology::Isbn => "9783161484100".to_string(),
            Symbology::Code128Gs1 => "]C1010950110153000310ABC123\u{1d}17201231".to_string(),
            Symbology::UpcE => "04252614".to_string(),
            _ => (*payload).to_string(),
        };
        assert_eq!(got, want, "{s:?}");
    }
}

#[test]
fn quiet_zone_below_the_standard_minimum_is_refused_and_the_minimum_is_the_default() {
    for (s, payload) in SAMPLES {
        let base = encode(*s, payload);
        assert_eq!(base.quiet_modules, base.min_quiet_modules, "{s:?}: default = standard minimum");
        let mut p = sample_params(*s);
        if base.min_quiet_modules > 0 {
            p.quiet_modules = Some(base.min_quiet_modules - 1);
            assert_eq!(
                encode_symbol(*s, payload, &p),
                Err(SymbolError::QuietZoneTooSmall {
                    symbology: *s,
                    min_modules: base.min_quiet_modules,
                    got_modules: base.min_quiet_modules - 1
                }),
                "{s:?}"
            );
        }
        p.quiet_modules = Some(base.min_quiet_modules + 3);
        let wider = encode_symbol(*s, payload, &p).unwrap();
        assert_eq!(wider.quiet_modules, base.min_quiet_modules + 3);
        assert_eq!(wider.layout, base.layout, "{s:?}: the quiet zone does not touch the modules");
        assert!((wider.size_mm.0 - base.size_mm.0 - 6.0 * 0.4).abs() < 1e-9, "{s:?}");
    }
    // Standard values.
    assert_eq!(encode(Symbology::QrCode, "x").min_quiet_modules, 4);
    assert_eq!(encode(Symbology::QrMicro, "1").min_quiet_modules, 2);
    assert_eq!(encode(Symbology::QrMicroRect, "1").min_quiet_modules, 2);
    assert_eq!(encode(Symbology::Code128, "x").min_quiet_modules, 10);
    assert_eq!(encode(Symbology::DataMatrix, "x").min_quiet_modules, 1);
    assert_eq!(encode(Symbology::Pdf417, "x").min_quiet_modules, 2);
    assert_eq!(encode(Symbology::AprilTag36h11, "0").min_quiet_modules, 1);
}

#[test]
fn physical_parameters_are_validated_not_clamped() {
    let ok = |s: Symbology, p: &SymbologyParams| encode_symbol(s, "12345678", p);
    for s in [Symbology::Code128, Symbology::QrCode, Symbology::DataMatrix, Symbology::Ean] {
        let payload = if s == Symbology::Ean { "96385074" } else { "12345678" };
        let mut p = sample_params(s);
        p.module_mm = 0.05;
        p.min_module_mm = 0.2;
        assert_eq!(
            encode_symbol(s, payload, &p),
            Err(SymbolError::Param(GenError::TooSmall { param: "module", min_mm: 0.2, value_mm: 0.05 })),
            "{s:?}: below the read-test minimum"
        );
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut p = sample_params(s);
            p.module_mm = bad;
            assert_eq!(
                encode_symbol(s, payload, &p),
                Err(SymbolError::Param(GenError::NotFinite { param: "module" })),
                "{s:?}"
            );
            let mut p = sample_params(s);
            p.min_module_mm = bad;
            assert_eq!(
                encode_symbol(s, payload, &p),
                Err(SymbolError::Param(GenError::NotFinite { param: "min_module" })),
                "{s:?}"
            );
        }
        for bad in [0.0, -0.4] {
            let mut p = sample_params(s);
            p.module_mm = bad;
            assert_eq!(
                encode_symbol(s, payload, &p),
                Err(SymbolError::Param(GenError::NotPositive { param: "module" })),
                "{s:?}"
            );
        }
    }
    // Bar height matters for linear codes only.
    let mut p = params(0.4);
    p.height_mm = f64::NAN;
    assert_eq!(ok(Symbology::Code128, &p), Err(SymbolError::Param(GenError::NotFinite { param: "height_mm" })));
    p.height_mm = 0.0;
    assert_eq!(ok(Symbology::Code128, &p), Err(SymbolError::Param(GenError::NotPositive { param: "height_mm" })));
    p.height_mm = f64::NAN;
    assert!(ok(Symbology::QrCode, &p).is_ok(), "2D codes ignore the bar height");
    // A module exactly at the minimum is allowed.
    let mut p = params(0.2);
    p.min_module_mm = 0.2;
    assert!(ok(Symbology::QrCode, &p).is_ok());
}

#[test]
fn encoding_is_deterministic_and_independent_of_the_input_mode_default() {
    for (s, payload) in SAMPLES {
        let a = encode(*s, payload);
        let b = encode(*s, payload);
        assert_eq!(a, b, "{s:?}");
    }
}

// ---------------------------------------------------------------------------------------------
// fiducial markers (structure; module-level references are in symbology_reference.rs)
// ---------------------------------------------------------------------------------------------

#[test]
fn markers_take_a_decimal_id_inside_the_family_and_draw_a_black_border() {
    let counts = [
        (Symbology::AprilTag16h5, 30u32, 4usize),
        (Symbology::AprilTag25h9, 35, 5),
        (Symbology::AprilTag36h10, 2320, 6),
        (Symbology::AprilTag36h11, 587, 6),
        (Symbology::ArucoOpenCv7x7, 100, 7),
    ];
    for (s, count, n) in counts {
        let first = encode(s, "0");
        let last = encode(s, &(count - 1).to_string());
        for sym in [&first, &last] {
            let (w, h, m) = matrix(sym);
            assert_eq!((w, h), (n + 2, n + 2), "{s:?}");
            for i in 0..n + 2 {
                assert!(
                    m[i] && m[(n + 1) * w + i] && m[i * w] && m[i * w + n + 1],
                    "{s:?}: border module {i} is black"
                );
            }
        }
        assert_ne!(first.layout, last.layout, "{s:?}");
        assert_eq!(encode(s, "007").canonical_payload, "7");
        assert_eq!(
            s.capabilities().marker_count,
            Some(count),
            "{s:?}: the UI reads the id range from the capabilities"
        );
        assert_eq!(err(s, &count.to_string()), SymbolError::MarkerId { symbology: s, id: count, count });
        assert_eq!(err(s, "1x"), SymbolError::InvalidChar { symbology: s, ch: 'x', index: 1 });
        assert_eq!(err(s, ""), SymbolError::EmptyPayload { symbology: s });
        assert_eq!(err(s, "-1"), SymbolError::InvalidChar { symbology: s, ch: '-', index: 0 });
        assert!(matches!(err(s, "99999999999"), SymbolError::InvalidPayload { .. } | SymbolError::MarkerId { .. }));
    }
}

#[test]
fn the_decoder_oracle_discriminates_damaged_symbols() {
    // Negative control: erase a vertical strip through the middle of a symbol; the independent decoder must not return the payload.
    for (s, payload) in [
        (Symbology::Code128, "Hello-128 123"),
        (Symbology::QrCode, "https://example.com/burnburn"),
        (Symbology::DataMatrix, "DataMatrix 123"),
    ] {
        let sym = encode(s, payload);
        let (mut img, w, h) = rasterise(&sym, 8.0, 24);
        let (w_us, h_us) = (w as usize, h as usize);
        for y in 0..h_us {
            for x in w_us * 4 / 10..w_us * 6 / 10 {
                img[y * w_us + x] = 255;
            }
        }
        let got = rxing::helpers::detect_in_luma(img, w, h, Some(format_of(s, &sym.canonical_payload)))
            .map(|r| r.getText().to_string());
        assert_ne!(got.as_deref().ok(), Some(payload), "{s:?}: a damaged symbol must not read back as the payload");
    }
}

// ---------------------------------------------------------------------------------------------
// properties
// ---------------------------------------------------------------------------------------------

mod props {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig { cases: 48, ..ProptestConfig::default() })]

        /// Any printable ASCII text survives encode -> raster -> independent decode.
        #[test]
        fn printable_ascii_round_trips_through_the_decoder(text in "[ -~]{1,40}") {
            for s in [Symbology::Code128, Symbology::QrCode, Symbology::DataMatrix, Symbology::Aztec, Symbology::Pdf417, Symbology::QrMicroRect] {
                // Code 128 text with trailing spaces is still exact; Micro/rMQR/QR capacity covers 40 characters.
                let sym = encode_symbol(s, &text, &sample_params(s)).unwrap();
                prop_assert_eq!(read_back(&sym).unwrap(), text.clone(), "{:?}", s);
            }
        }

        /// A 12-digit EAN payload gets the right check digit: the decoder verifies it and returns all 13 digits.
        #[test]
        fn ean13_check_digits_are_accepted_by_the_decoder(digits in "[0-9]{12}") {
            let sym = encode_symbol(Symbology::Ean, &digits, &sample_params(Symbology::Ean)).unwrap();
            prop_assert_eq!(sym.canonical_payload.len(), 13);
            prop_assert!(sym.canonical_payload.starts_with(&digits));
            prop_assert_eq!(read_back(&sym).unwrap(), sym.canonical_payload.clone());
            let upc = encode_symbol(Symbology::UpcA, &digits[..11], &sample_params(Symbology::UpcA)).unwrap();
            prop_assert_eq!(read_back(&upc).unwrap(), upc.canonical_payload.clone());
        }

        /// UPC-E: every 6-digit payload expands and re-checks; the decoder validates the check digit.
        #[test]
        fn upc_e_payloads_are_accepted_by_the_decoder(digits in "[01][0-9]{6}") {
            let sym = encode_symbol(Symbology::UpcE, &digits, &sample_params(Symbology::UpcE)).unwrap();
            prop_assert_eq!(read_back(&sym).unwrap(), sym.canonical_payload.clone());
        }

        /// No input, however odd, panics or produces geometry without data: every outcome is a symbol or a typed error.
        #[test]
        fn arbitrary_text_never_panics_and_never_yields_an_empty_symbol(text in "\\PC{0,24}") {
            for s in Symbology::ALL {
                let mut p = sample_params(s);
                p.input_mode = Some(s.capabilities().input_modes[0]);
                if let Ok(sym) = encode_symbol(s, &text, &p) {
                    prop_assert!(!sym.path.is_empty(), "{:?}", s);
                    prop_assert!(sym.size_mm.0 > 0.0 && sym.size_mm.1 > 0.0);
                }
            }
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 64, ..ProptestConfig::default() })]

        /// Digit/alphanumeric-heavy input up to the capacity limits: typed errors or symbols, never a panic.
        #[test]
        fn capacity_edges_never_panic(text in "[0-9A-F()$%*+./: -]{0,400}") {
            for s in Symbology::ALL {
                let mut p = sample_params(s);
                p.input_mode = Some(s.capabilities().input_modes[0]);
                if s.capabilities().uses_columns {
                    p.columns = Some(1 + (text.len() % 30) as u8);
                }
                let _ = encode_symbol(s, &text, &p);
            }
        }
    }
}
