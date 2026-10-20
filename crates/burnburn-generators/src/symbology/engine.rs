//! Dispatch: parameter checks, payload normalisation and the encoders behind each symbology.
//!
//! Own encoders: Code 128 (`barcode.rs`), QR Code (`qr.rs`), the fiducial families. Everything else
//! is produced by the pure-Rust library `anyd` (no dependencies, `no_std` + `alloc`) behind the typed
//! validation of `payload.rs`; every library-produced family is read back by an independent decoder
//! or compared module-for-module with a second encoder in the tests.

use anyd::codes::aztec::AztecEncoder;
use anyd::codes::codabar::CodabarEncoder;
use anyd::codes::code39::Code39Encoder;
use anyd::codes::code93::Code93Encoder;
use anyd::codes::code128::{Code128Encoder, Code128Input};
use anyd::codes::datamatrix::DataMatrixEncoder;
use anyd::codes::ean::EanEncoder;
use anyd::codes::microqr::{MicroEcLevel, MicroQrEncoder};
use anyd::codes::msi::MsiEncoder;
use anyd::codes::pdf417::{EcLevel as Pdf417Level, MicroPdf417Encoder, Pdf417Encoder};
use anyd::codes::rmqr::{RmqrEcLevel, RmqrEncoder};
use anyd::codes::twoof5::TwoOf5Encoder;
use anyd::output::Encoding;
use anyd::traits::Encode;
use anyd::{Segment, Symbology as Lib};

use super::geometry;
use super::payload::{self, MAX_PAYLOAD_CHARS};
use super::{
    EccStep, EncodedSymbol, SymbolCapabilities, SymbolError, SymbolInputMode, SymbolKind, SymbolLayout, SymbolSource,
    SymbolStatus, Symbology, SymbologyParams, fiducial,
};
use crate::barcode::{code128_patterns, code128_symbols};
use crate::error::{GenError, positive};
use crate::qr::{QrEcc, qr_encode};

const ANYD: SymbolStatus = SymbolStatus::Implemented { source: SymbolSource::Library("anyd") };
const OWN: SymbolStatus = SymbolStatus::Implemented { source: SymbolSource::Own };

pub(super) fn status(symbology: Symbology) -> SymbolStatus {
    use Symbology::*;
    match symbology {
        Code128 | QrCode | AprilTag16h5 | AprilTag25h9 | AprilTag36h10 | AprilTag36h11 | ArucoOpenCv7x7 => OWN,
        TwoOfFiveIata | TwoOfFiveIndustrial | Aztec | DataMatrix | Codabar | Code128Gs1 | Code93 | Ean | Isbn
        | Plessey | UpcA | UpcE | Vin | Pdf417 | Pdf417Compact | Pdf417Micro | QrMicro | QrMicroRect => ANYD,
        TwoOfFive => SymbolStatus::Unsupported {
            reason: "The meaning of LightBurn's plain 2 of 5 is not established (Standard/Matrix or Interleaved), and the two \
                     reference encoders disagree on the start pattern of Matrix 2 of 5, so no verified pattern exists. \
                     2 of 5 Industrial and 2 of 5 IATA are available.",
        },
        ArucoMip36h11 => SymbolStatus::Unsupported {
            reason: "No ArUco MIP 36h11 family is published; the MIP family is 36h12 (OpenCV DICT_ARUCO_MIP_36h12). \
                     The LightBurn name could not be mapped to a verified code table.",
        },
        HanXin => SymbolStatus::Unsupported {
            reason: "Two independent encoders disagree on the function-information modules of Han Xin versions 1-3 and \
                     no decoder is available to arbitrate; larger versions are not implemented in either.",
        },
        GridMatrix => SymbolStatus::Unsupported {
            reason: "Only one encoder exists (mirroring the zint source) and neither a second implementation nor a \
                     decoder is available to verify its modules; it is not shipped unverified.",
        },
    }
}

const QR_STEPS: &[EccStep] = &[
    EccStep { label: "L", recovery_percent: Some(7) },
    EccStep { label: "M", recovery_percent: Some(15) },
    EccStep { label: "Q", recovery_percent: Some(25) },
    EccStep { label: "H", recovery_percent: Some(30) },
];
const MICRO_STEPS: &[EccStep] = &[
    EccStep { label: "Detection only (M1)", recovery_percent: None },
    EccStep { label: "L", recovery_percent: Some(7) },
    EccStep { label: "M", recovery_percent: Some(15) },
    EccStep { label: "Q", recovery_percent: Some(25) },
];
const RMQR_STEPS: &[EccStep] =
    &[EccStep { label: "M", recovery_percent: Some(15) }, EccStep { label: "H", recovery_percent: Some(30) }];
const PDF417_STEPS: &[EccStep] = &[
    EccStep { label: "Level 0", recovery_percent: None },
    EccStep { label: "Level 1", recovery_percent: None },
    EccStep { label: "Level 2", recovery_percent: None },
    EccStep { label: "Level 3", recovery_percent: None },
    EccStep { label: "Level 4", recovery_percent: None },
    EccStep { label: "Level 5", recovery_percent: None },
    EccStep { label: "Level 6", recovery_percent: None },
    EccStep { label: "Level 7", recovery_percent: None },
    EccStep { label: "Level 8", recovery_percent: None },
];

const DATA_ONLY: &[SymbolInputMode] = &[SymbolInputMode::Data];
const GS1_ONLY: &[SymbolInputMode] = &[SymbolInputMode::Gs1];
const DATA_UNICODE: &[SymbolInputMode] = &[SymbolInputMode::Data, SymbolInputMode::Unicode];

pub(super) fn capabilities(symbology: Symbology) -> SymbolCapabilities {
    use Symbology::*;
    let linear = |quiet: u32, hint: &'static str, modes: &'static [SymbolInputMode]| SymbolCapabilities {
        kind: SymbolKind::Linear,
        ecc_steps: &[],
        default_ecc_step: None,
        input_modes: modes,
        min_quiet_modules: quiet,
        uses_columns: false,
        marker_count: None,
        payload_hint: hint,
    };
    let grid = |quiet: u32,
                hint: &'static str,
                steps: &'static [EccStep],
                default: Option<u8>,
                columns: bool,
                modes: &'static [SymbolInputMode]| SymbolCapabilities {
        kind: SymbolKind::Matrix,
        ecc_steps: steps,
        default_ecc_step: default,
        input_modes: modes,
        min_quiet_modules: quiet,
        uses_columns: columns,
        marker_count: None,
        payload_hint: hint,
    };
    let marker = |hint: &'static str, count: Option<u32>| SymbolCapabilities {
        kind: SymbolKind::Marker,
        ecc_steps: &[],
        default_ecc_step: None,
        input_modes: DATA_ONLY,
        min_quiet_modules: 1,
        uses_columns: false,
        marker_count: count,
        payload_hint: hint,
    };
    match symbology {
        TwoOfFive | TwoOfFiveIata | TwoOfFiveIndustrial => linear(10, "digits", DATA_ONLY),
        AprilTag16h5 => marker("marker id 0-29", Some(30)),
        AprilTag25h9 => marker("marker id 0-34", Some(35)),
        AprilTag36h10 => marker("marker id 0-2319", Some(2320)),
        AprilTag36h11 => marker("marker id 0-586", Some(587)),
        ArucoMip36h11 => marker("not available", None),
        ArucoOpenCv7x7 => marker("marker id 0-99", Some(100)),
        Aztec => SymbolCapabilities {
            kind: SymbolKind::Matrix,
            ecc_steps: &[],
            default_ecc_step: None,
            input_modes: DATA_ONLY,
            min_quiet_modules: 0,
            uses_columns: false,
            marker_count: None,
            payload_hint: "text (ISO 8859-1) or bytes; error correction is fixed (about 23 % + 11 bits)",
        },
        HanXin | GridMatrix => grid(3, "not available", &[], None, false, DATA_ONLY),
        DataMatrix => {
            grid(1, "text (ISO 8859-1) or bytes; ECC 200, error correction is fixed", &[], None, false, DATA_ONLY)
        }
        Codabar => linear(10, "start letter A-D, data 0-9 - $ : / . +, stop letter A-D", DATA_ONLY),
        Code128 => linear(10, "ASCII 32-127", DATA_ONLY),
        Code128Gs1 => linear(10, "GS1 element string (01)09501101530003(10)LOT", GS1_ONLY),
        Code93 => linear(10, "0-9 A-Z - . space $ / + %", DATA_ONLY),
        Ean => linear(11, "7 or 12 digits (check digit added), or 8 or 13 with check digit", DATA_ONLY),
        Isbn => linear(11, "ISBN-13 (978/979) or ISBN-10, hyphens allowed", DATA_ONLY),
        Plessey => linear(10, "hexadecimal digits 0-9 A-F", DATA_ONLY),
        UpcA => linear(9, "11 digits (check digit added) or 12 with check digit", DATA_ONLY),
        UpcE => linear(9, "6, 7 or 8 digits (zero-suppressed, number system 0 or 1)", DATA_ONLY),
        Vin => linear(10, "17 characters, no I O Q; check digit verified for North American VINs", DATA_ONLY),
        Pdf417 | Pdf417Compact => grid(2, "text (ISO 8859-1) or bytes", PDF417_STEPS, Some(2), true, DATA_ONLY),
        Pdf417Micro => {
            grid(1, "text (ISO 8859-1) or bytes; error correction is fixed by the size", &[], None, false, DATA_ONLY)
        }
        QrCode => grid(4, "text or bytes, up to version 10", QR_STEPS, Some(0), false, DATA_UNICODE),
        QrMicro => grid(2, "text or bytes, up to 35 digits", MICRO_STEPS, Some(1), false, DATA_UNICODE),
        QrMicroRect => grid(2, "text or bytes", RMQR_STEPS, Some(0), false, DATA_UNICODE),
    }
}

/// What an encoder produced, before the physical parameters are applied.
struct Raw {
    layout: SymbolLayout,
    min_quiet: u32,
    canonical: String,
}

/// Standard minimum quiet zone (modules) of one symbol.
fn standard_quiet(symbology: Symbology, canonical: &str) -> u32 {
    use Symbology::*;
    match symbology {
        TwoOfFive | TwoOfFiveIata | TwoOfFiveIndustrial | Plessey | Codabar | Code128 | Code128Gs1 | Code93 | Vin => 10,
        Ean => {
            if canonical.len() == 8 {
                7
            } else {
                11
            }
        }
        Isbn => 11,
        UpcA | UpcE => 9,
        DataMatrix => 1,
        Aztec => 0,
        Pdf417 | Pdf417Compact => 2,
        Pdf417Micro => 1,
        QrCode => 4,
        QrMicro | QrMicroRect => 2,
        AprilTag16h5 | AprilTag25h9 | AprilTag36h10 | AprilTag36h11 | ArucoMip36h11 | ArucoOpenCv7x7 => 1,
        HanXin | GridMatrix => 3,
    }
}

fn library_error(symbology: Symbology, e: anyd::Error) -> SymbolError {
    match e {
        anyd::Error::Capacity { .. } => SymbolError::DataTooLong { symbology, limit: capacity_text(symbology) },
        other => SymbolError::Engine { symbology, message: other.to_string() },
    }
}

fn capacity_text(symbology: Symbology) -> &'static str {
    match symbology {
        Symbology::QrCode => "the own encoder stops at QR version 10",
        Symbology::QrMicro => "Micro QR M4 holds at most 35 digits, 21 alphanumeric characters or 15 bytes",
        Symbology::QrMicroRect => "the largest rMQR size is 17 x 139 modules",
        Symbology::Pdf417Micro => "the largest MicroPDF417 size has four data columns and 44 rows",
        Symbology::Pdf417 | Symbology::Pdf417Compact => "at most 928 codewords, 30 columns and 90 rows",
        Symbology::DataMatrix => "the largest DataMatrix ECC 200 symbol is 144 x 144 modules",
        Symbology::Aztec => "the largest Aztec symbol has 32 layers",
        _ => "the symbology limit",
    }
}

fn layout_of(symbology: Symbology, encoding: Encoding) -> Result<SymbolLayout, SymbolError> {
    match encoding {
        Encoding::Linear(l) => {
            let mut widths: Vec<u8> = Vec::new();
            let mut bar = true;
            let mut run = 0usize;
            let push = |widths: &mut Vec<u8>, run: usize| -> Result<(), SymbolError> {
                u8::try_from(run).map(|w| widths.push(w)).map_err(|_| SymbolError::Engine {
                    symbology,
                    message: format!("a {run}-module element does not fit the layout"),
                })
            };
            for &m in &l.modules {
                if m == bar {
                    run += 1;
                } else {
                    push(&mut widths, run)?;
                    bar = m;
                    run = 1;
                }
            }
            push(&mut widths, run)?;
            Ok(SymbolLayout::Linear { widths })
        }
        Encoding::Matrix(m) => {
            let (w, h) = (m.width(), m.height());
            let modules = (0..h).flat_map(|y| (0..w).map(move |x| (x, y))).map(|(x, y)| m.get(x, y)).collect();
            Ok(SymbolLayout::Matrix { width: w, height: h, modules })
        }
    }
}

fn from_lib(
    symbology: Symbology,
    built: Result<anyd::Symbol, anyd::Error>,
    encoder: &dyn Encode,
    canonical: String,
) -> Result<Raw, SymbolError> {
    let symbol = built.map_err(|e| library_error(symbology, e))?;
    let encoding = encoder.encode(&symbol).map_err(|e| library_error(symbology, e))?;
    let layout = layout_of(symbology, encoding)?;
    Ok(Raw { min_quiet: standard_quiet(symbology, &canonical), layout, canonical })
}

fn ecc_index(step: Option<u8>, caps: &SymbolCapabilities) -> usize {
    usize::from(step.or(caps.default_ecc_step).unwrap_or(0))
}

/// Compaction for PDF417 / MicroPDF417: numeric for digit-only payloads of at least `numeric_from` digits
/// (13 for PDF417, 9 for MicroPDF417, where the reference encoders switch), text for printable ASCII,
/// otherwise bytes.
fn pdf417_segments(bytes: &[u8], numeric_from: usize) -> Vec<Segment> {
    if bytes.len() >= numeric_from && bytes.iter().all(u8::is_ascii_digit) {
        vec![Segment::numeric(bytes.to_vec())]
    } else if bytes.iter().all(|&b| (0x20..=0x7e).contains(&b) || matches!(b, b'\t' | b'\r' | b'\n')) {
        vec![Segment::alphanumeric(bytes.to_vec())]
    } else {
        vec![Segment::byte(bytes.to_vec())]
    }
}

/// The 18-module stop pattern of PDF417 (ISO/IEC 15438 table).
const PDF417_STOP: [bool; 18] = bits18("111111101000101001");

const fn bits18(s: &str) -> [bool; 18] {
    let b = s.as_bytes();
    let mut out = [false; 18];
    let mut i = 0;
    while i < 18 {
        out[i] = b[i] == b'1';
        i += 1;
    }
    out
}

/// Compact (truncated) PDF417: the right row indicator is left out and the stop pattern is a single bar.
fn compact_pdf417(layout: SymbolLayout) -> Result<SymbolLayout, SymbolError> {
    let engine =
        |message: &str| SymbolError::Engine { symbology: Symbology::Pdf417Compact, message: message.to_string() };
    let SymbolLayout::Matrix { width, height, modules } = layout else {
        return Err(engine("expected a module grid"));
    };
    if width < 17 + 17 + 17 + 17 + 18 {
        return Err(engine("symbol too narrow for a right row indicator"));
    }
    let new_width = width - 18 - 17 + 1;
    let mut out = Vec::with_capacity(new_width * height);
    for r in 0..height {
        let row = &modules[r * width..(r + 1) * width];
        if row[width - 18..] != PDF417_STOP {
            return Err(engine("row does not end in the stop pattern"));
        }
        out.extend_from_slice(&row[..width - 18 - 17]);
        out.push(true);
    }
    Ok(SymbolLayout::Matrix { width: new_width, height, modules: out })
}

fn raw(
    symbology: Symbology,
    payload: &str,
    mode: SymbolInputMode,
    ecc_step: Option<u8>,
    columns: Option<u8>,
    caps: &SymbolCapabilities,
) -> Result<Raw, SymbolError> {
    use Symbology::*;
    let ecc = ecc_index(ecc_step, caps);
    match symbology {
        TwoOfFiveIata | TwoOfFiveIndustrial => {
            let digits = payload::digit_string(symbology, payload)?;
            let lib = if symbology == TwoOfFiveIata { Lib::Iata2of5 } else { Lib::Std2of5 };
            let enc = TwoOf5Encoder::new();
            from_lib(symbology, enc.build(lib, digits.as_bytes()), &enc, digits)
        }
        Codabar => {
            let text = payload::codabar(payload)?;
            let b = text.as_bytes();
            let enc = CodabarEncoder::new();
            from_lib(symbology, enc.build(b[0], &b[1..b.len() - 1], b[b.len() - 1]), &enc, text)
        }
        Code128 => {
            let symbols = code128_symbols(payload).map_err(|e| match e {
                GenError::UnsupportedChar { ch, index } => SymbolError::InvalidChar { symbology, ch, index },
                GenError::Count { min, max, value, .. } => {
                    SymbolError::PayloadLength { symbology, min: min as usize, max: max as usize, len: value as usize }
                }
                other => SymbolError::Param(other),
            })?;
            let table = code128_patterns();
            let widths: Vec<u8> =
                symbols.iter().filter_map(|&v| table.get(usize::from(v))).flat_map(|p| p.iter().copied()).collect();
            Ok(Raw {
                layout: SymbolLayout::Linear { widths },
                min_quiet: standard_quiet(symbology, payload),
                canonical: payload.to_string(),
            })
        }
        Code128Gs1 => {
            let elements = payload::gs1(payload)?;
            let mut input = Vec::new();
            let mut canonical = String::new();
            for (i, e) in elements.iter().enumerate() {
                canonical.push_str(&format!("({}){}", e.ai, e.value));
                input.extend(e.ai.bytes().chain(e.value.bytes()).map(Code128Input::Data));
                if i + 1 < elements.len() && !payload::predefined_length(&e.ai) {
                    input.push(Code128Input::Fnc1);
                }
            }
            let enc = Code128Encoder::new();
            from_lib(symbology, enc.build_gs1(&input), &enc, canonical)
        }
        Code93 => {
            let text = payload::code93(payload)?;
            let enc = Code93Encoder::new();
            from_lib(symbology, enc.build(text.as_bytes(), false), &enc, text)
        }
        Ean | Isbn | UpcA | UpcE => {
            let enc = EanEncoder::new();
            let (digits, built) = match symbology {
                Ean => {
                    let d = payload::ean(payload)?;
                    let b = if d.len() == 8 { enc.build_ean8(&d) } else { enc.build_ean13(&d) };
                    (d, b)
                }
                Isbn => {
                    let d = payload::isbn(payload)?;
                    let b = enc.build_ean13(&d);
                    (d, b)
                }
                UpcA => {
                    let d = payload::upc_a(payload)?;
                    let b = enc.build_upca(&d);
                    (d, b)
                }
                _ => {
                    let d = payload::upc_e(payload)?;
                    let b = enc.build_upce(&d);
                    (d, b)
                }
            };
            from_lib(symbology, built, &enc, digits)
        }
        Plessey => {
            let hex = payload::plessey(payload)?;
            let enc = MsiEncoder::new();
            from_lib(symbology, enc.build_plessey(hex.as_bytes()), &enc, hex)
        }
        Vin => {
            let vin = payload::vin(payload)?;
            let enc = Code39Encoder::new();
            from_lib(symbology, enc.build(vin.as_bytes(), false, false), &enc, vin)
        }
        Aztec => {
            let bytes = payload::payload_bytes(symbology, payload, mode)?;
            let enc = AztecEncoder::new();
            from_lib(symbology, enc.build(vec![Segment::byte(bytes)]), &enc, payload.to_string())
        }
        DataMatrix => {
            let bytes = payload::payload_bytes(symbology, payload, mode)?;
            let enc = DataMatrixEncoder::new();
            from_lib(symbology, enc.build(&bytes), &enc, payload.to_string())
        }
        Pdf417 | Pdf417Compact => {
            let bytes = payload::payload_bytes(symbology, payload, mode)?;
            let level = u8::try_from(ecc)
                .ok()
                .and_then(Pdf417Level::new)
                .ok_or(SymbolError::Engine { symbology, message: "no such security level".to_string() })?;
            let enc = Pdf417Encoder::new();
            let built = enc.build_sized(pdf417_segments(&bytes, 13), level, columns.map(usize::from));
            let mut r = from_lib(symbology, built, &enc, payload.to_string())?;
            if symbology == Pdf417Compact {
                r.layout = compact_pdf417(r.layout)?;
            }
            Ok(r)
        }
        Pdf417Micro => {
            let bytes = payload::payload_bytes(symbology, payload, mode)?;
            let enc = MicroPdf417Encoder::new();
            from_lib(symbology, enc.build(pdf417_segments(&bytes, 9)), &enc, payload.to_string())
        }
        QrCode => {
            let bytes = payload::payload_bytes(symbology, payload, mode)?;
            let level = [QrEcc::L, QrEcc::M, QrEcc::Q, QrEcc::H][ecc.min(3)];
            let code = qr_encode(&bytes, level).map_err(|e| match e {
                GenError::Count { param: "data", .. } => {
                    SymbolError::DataTooLong { symbology, limit: capacity_text(symbology) }
                }
                other => SymbolError::Param(other),
            })?;
            let n = code.size();
            let modules = (0..n).flat_map(|r| (0..n).map(move |c| (r, c))).map(|(r, c)| code.is_dark(r, c)).collect();
            Ok(Raw {
                layout: SymbolLayout::Matrix { width: n, height: n, modules },
                min_quiet: standard_quiet(symbology, payload),
                canonical: payload.to_string(),
            })
        }
        QrMicro => {
            let bytes = payload::payload_bytes(symbology, payload, mode)?;
            let level = [MicroEcLevel::Detection, MicroEcLevel::L, MicroEcLevel::M, MicroEcLevel::Q][ecc.min(3)];
            let enc = MicroQrEncoder::new();
            let built = match std::str::from_utf8(&bytes) {
                Ok(text) => enc.build_text(text, level),
                Err(_) => enc.build(vec![Segment::byte(bytes.clone())], level),
            };
            from_lib(symbology, built, &enc, payload.to_string())
        }
        QrMicroRect => {
            let bytes = payload::payload_bytes(symbology, payload, mode)?;
            let level = [RmqrEcLevel::M, RmqrEcLevel::H][ecc.min(1)];
            let enc = RmqrEncoder::new();
            let built = if mode == SymbolInputMode::Unicode && !bytes.is_ascii() {
                // rMQR can name the character set: ECI 26 = UTF-8.
                enc.build(vec![Segment::eci(26), Segment::byte(bytes.clone())], level)
            } else {
                match std::str::from_utf8(&bytes) {
                    Ok(text) => enc.build_text(text, level),
                    Err(_) => enc.build(vec![Segment::byte(bytes.clone())], level),
                }
            };
            from_lib(symbology, built, &enc, payload.to_string())
        }
        AprilTag16h5 | AprilTag25h9 | AprilTag36h10 | AprilTag36h11 | ArucoOpenCv7x7 => {
            let id = payload::marker_id(symbology, payload)?;
            let (size, modules) = fiducial::marker(symbology, id)?;
            Ok(Raw {
                layout: SymbolLayout::Matrix { width: size, height: size, modules },
                min_quiet: standard_quiet(symbology, payload),
                canonical: id.to_string(),
            })
        }
        TwoOfFive | ArucoMip36h11 | HanXin | GridMatrix => match status(symbology) {
            SymbolStatus::Unsupported { reason } => Err(SymbolError::Unsupported { symbology, reason }),
            SymbolStatus::Implemented { .. } => {
                Err(SymbolError::Engine { symbology, message: "no encoder".to_string() })
            }
        },
    }
}

pub(super) fn encode(
    symbology: Symbology,
    payload: &str,
    params: &SymbologyParams,
) -> Result<EncodedSymbol, SymbolError> {
    if let SymbolStatus::Unsupported { reason } = status(symbology) {
        return Err(SymbolError::Unsupported { symbology, reason });
    }
    let caps = capabilities(symbology);
    let module = positive("module", params.module_mm)?;
    let min_module = positive("min_module", params.min_module_mm)?;
    if module < min_module {
        return Err(GenError::TooSmall { param: "module", min_mm: min_module, value_mm: module }.into());
    }
    let bar_height = if caps.kind == SymbolKind::Linear { positive("height_mm", params.height_mm)? } else { 0.0 };
    let mode = params.input_mode.unwrap_or(caps.input_modes[0]);
    if !caps.input_modes.contains(&mode) {
        return Err(SymbolError::InputModeUnavailable { symbology, mode });
    }
    if let Some(step) = params.ecc_step
        && usize::from(step) >= caps.ecc_steps.len()
    {
        return Err(SymbolError::EccStepUnavailable { symbology, step, steps: caps.ecc_steps.len() as u8 });
    }
    if let Some(columns) = params.columns {
        if !caps.uses_columns {
            return Err(SymbolError::ParamNotApplicable { symbology, param: "columns" });
        }
        if !(1..=30).contains(&columns) {
            return Err(GenError::Count { param: "columns", min: 1, max: 30, value: u64::from(columns) }.into());
        }
    }
    let chars = payload.chars().count();
    if chars == 0 {
        return Err(SymbolError::EmptyPayload { symbology });
    }
    if chars > MAX_PAYLOAD_CHARS {
        return Err(SymbolError::PayloadLength { symbology, min: 1, max: MAX_PAYLOAD_CHARS, len: chars });
    }
    let raw = raw(symbology, payload, mode, params.ecc_step, params.columns, &caps)?;
    let quiet = params.quiet_modules.unwrap_or(raw.min_quiet);
    if quiet < raw.min_quiet {
        return Err(SymbolError::QuietZoneTooSmall { symbology, min_modules: raw.min_quiet, got_modules: quiet });
    }
    let (path, size_mm) = geometry::build(&raw.layout, module, quiet, bar_height)?;
    Ok(EncodedSymbol {
        symbology,
        path,
        layout: raw.layout,
        module_mm: module,
        quiet_modules: quiet,
        min_quiet_modules: raw.min_quiet,
        size_mm,
        canonical_payload: raw.canonical,
    })
}
