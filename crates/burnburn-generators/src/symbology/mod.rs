//! One API for the 29 LightBurn bar-code types (W2-L5d): `Symbology`, validated payloads in,
//! closed module rectangles in millimetres out.
//!
//! Rules of this module:
//! - Module sizes are real millimetres; quiet zones are enforced per symbology (a request below
//!   the standard minimum is refused, never widened silently).
//! - Payload checks are typed (`SymbolError`): EAN/UPC lengths and check digits, ISBN-13, UPC-E
//!   zero suppression, VIN, Codabar start/stop letters, Plessey hex, GS1 application identifiers.
//! - A symbology without a verified encoder returns `SymbolError::Unsupported` — never a pattern
//!   that merely looks like the code. `catalog()` lists the status of all 29.
//! - Geometry is nominal (no kerf; CAM applies it once). Origin is the bottom-left corner of the
//!   quiet zone, y up, like `Barcode` and `QrSymbol`.

mod engine;
mod error;
mod fiducial;
mod fiducial_tables;
mod geometry;
mod payload;

pub use error::SymbolError;

use burnburn_geometry::Path;

/// The 29 bar-code types of the LightBurn "Bar Code Type" list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum Symbology {
    TwoOfFive,
    TwoOfFiveIata,
    TwoOfFiveIndustrial,
    AprilTag16h5,
    AprilTag25h9,
    AprilTag36h10,
    AprilTag36h11,
    ArucoMip36h11,
    ArucoOpenCv7x7,
    Aztec,
    HanXin,
    DataMatrix,
    GridMatrix,
    Codabar,
    Code128,
    Code128Gs1,
    Code93,
    Ean,
    Isbn,
    Plessey,
    UpcA,
    UpcE,
    Vin,
    Pdf417,
    Pdf417Compact,
    Pdf417Micro,
    QrCode,
    QrMicro,
    QrMicroRect,
}

impl Symbology {
    /// All 29, in the order of the LightBurn list.
    pub const ALL: [Symbology; 29] = [
        Symbology::TwoOfFive,
        Symbology::TwoOfFiveIata,
        Symbology::TwoOfFiveIndustrial,
        Symbology::AprilTag16h5,
        Symbology::AprilTag25h9,
        Symbology::AprilTag36h10,
        Symbology::AprilTag36h11,
        Symbology::ArucoMip36h11,
        Symbology::ArucoOpenCv7x7,
        Symbology::Aztec,
        Symbology::HanXin,
        Symbology::DataMatrix,
        Symbology::GridMatrix,
        Symbology::Codabar,
        Symbology::Code128,
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
        Symbology::QrCode,
        Symbology::QrMicro,
        Symbology::QrMicroRect,
    ];

    /// Stable machine name (document files, UI state). Never changes once published.
    pub fn stable_name(self) -> &'static str {
        match self {
            Symbology::TwoOfFive => "two_of_five",
            Symbology::TwoOfFiveIata => "two_of_five_iata",
            Symbology::TwoOfFiveIndustrial => "two_of_five_industrial",
            Symbology::AprilTag16h5 => "apriltag_16h5",
            Symbology::AprilTag25h9 => "apriltag_25h9",
            Symbology::AprilTag36h10 => "apriltag_36h10",
            Symbology::AprilTag36h11 => "apriltag_36h11",
            Symbology::ArucoMip36h11 => "aruco_mip_36h11",
            Symbology::ArucoOpenCv7x7 => "aruco_opencv_7x7",
            Symbology::Aztec => "aztec",
            Symbology::HanXin => "han_xin",
            Symbology::DataMatrix => "data_matrix",
            Symbology::GridMatrix => "grid_matrix",
            Symbology::Codabar => "codabar",
            Symbology::Code128 => "code_128",
            Symbology::Code128Gs1 => "code_128_gs1",
            Symbology::Code93 => "code_93",
            Symbology::Ean => "ean",
            Symbology::Isbn => "isbn",
            Symbology::Plessey => "plessey",
            Symbology::UpcA => "upc_a",
            Symbology::UpcE => "upc_e",
            Symbology::Vin => "vin",
            Symbology::Pdf417 => "pdf417",
            Symbology::Pdf417Compact => "pdf417_compact",
            Symbology::Pdf417Micro => "pdf417_micro",
            Symbology::QrCode => "qr_code",
            Symbology::QrMicro => "qr_micro",
            Symbology::QrMicroRect => "qr_micro_rect",
        }
    }

    /// The symbology for a stable name.
    pub fn from_stable_name(name: &str) -> Option<Symbology> {
        Symbology::ALL.into_iter().find(|s| s.stable_name() == name)
    }

    /// The name as the LightBurn list shows it.
    pub fn display_name(self) -> &'static str {
        match self {
            Symbology::TwoOfFive => "2 of 5",
            Symbology::TwoOfFiveIata => "2 of 5 IATA",
            Symbology::TwoOfFiveIndustrial => "2 of 5 Industrial",
            Symbology::AprilTag16h5 => "AprilTag 16h5",
            Symbology::AprilTag25h9 => "AprilTag 25h9",
            Symbology::AprilTag36h10 => "AprilTag 36h10",
            Symbology::AprilTag36h11 => "AprilTag 36h11",
            Symbology::ArucoMip36h11 => "ArUco MIP 36h11",
            Symbology::ArucoOpenCv7x7 => "ArUco OpenCV 7x7",
            Symbology::Aztec => "Aztec",
            Symbology::HanXin => "Chinese Sensible Code",
            Symbology::DataMatrix => "DataMatrix",
            Symbology::GridMatrix => "Grid Matrix",
            Symbology::Codabar => "CodaBar",
            Symbology::Code128 => "Code 128",
            Symbology::Code128Gs1 => "Code 128 GS1",
            Symbology::Code93 => "Code 93",
            Symbology::Ean => "EAN",
            Symbology::Isbn => "ISBN",
            Symbology::Plessey => "Plessey",
            Symbology::UpcA => "UPC A",
            Symbology::UpcE => "UPC E",
            Symbology::Vin => "VIN",
            Symbology::Pdf417 => "PDF417",
            Symbology::Pdf417Compact => "PDF417 Compact",
            Symbology::Pdf417Micro => "PDF417 Micro",
            Symbology::QrCode => "QR Code",
            Symbology::QrMicro => "QR Micro",
            Symbology::QrMicroRect => "QR Micro Rectangular",
        }
    }

    /// Whether and how this symbology can be produced.
    pub fn status(self) -> SymbolStatus {
        engine::status(self)
    }

    /// What a UI needs to offer for this symbology.
    pub fn capabilities(self) -> SymbolCapabilities {
        engine::capabilities(self)
    }
}

/// Where the encoder of an implemented symbology comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolSource {
    /// Written in this crate.
    Own,
    /// A pure-Rust library (name as in the dependency register).
    Library(&'static str),
}

/// Production status of a symbology.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolStatus {
    Implemented {
        source: SymbolSource,
    },
    /// No verified encoder; `reason` is shown to the user and returned by `encode_symbol`.
    Unsupported {
        reason: &'static str,
    },
}

/// One row of `catalog()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SymbolCatalogEntry {
    pub symbology: Symbology,
    pub status: SymbolStatus,
}

/// All 29 symbologies with their status.
pub fn catalog() -> Vec<SymbolCatalogEntry> {
    Symbology::ALL.iter().map(|&symbology| SymbolCatalogEntry { symbology, status: symbology.status() }).collect()
}

/// How the payload text is turned into bytes (the LightBurn "Input Mode").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SymbolInputMode {
    /// Every character is one byte (U+0000..=U+00FF); anything above is refused.
    #[default]
    Data,
    /// The payload is UTF-8.
    Unicode,
    /// GS1 element string with parenthesised application identifiers: `(01)09501101530003(10)AB`.
    Gs1,
}

/// One step of the error-correction slider of a symbology.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EccStep {
    pub label: &'static str,
    /// Share of the symbol that can be restored, where the standard names one.
    pub recovery_percent: Option<u8>,
}

/// Shape of the symbol, for UI and sizing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    /// Bars of one height; `SymbologyParams::height_mm` applies.
    Linear,
    /// A module grid (matrix or stacked).
    Matrix,
    /// A fiducial marker family; the payload is the marker id.
    Marker,
}

/// What a UI offers for one symbology.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SymbolCapabilities {
    pub kind: SymbolKind,
    /// Error-correction steps, weakest first; empty when the symbology has no choice.
    pub ecc_steps: &'static [EccStep],
    /// Index into `ecc_steps` used when the parameters leave it open.
    pub default_ecc_step: Option<u8>,
    /// Accepted input modes; the first is the default.
    pub input_modes: &'static [SymbolInputMode],
    /// Largest standard minimum quiet zone over all variants of the symbology (modules).
    pub min_quiet_modules: u32,
    /// Whether `SymbologyParams::columns` (PDF417 data columns) applies.
    pub uses_columns: bool,
    /// Number of marker ids (`0..count`) of a fiducial family.
    pub marker_count: Option<u32>,
    /// One-line description of the accepted payload.
    pub payload_hint: &'static str,
}

/// Physical and encoding parameters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SymbologyParams {
    /// Narrowest bar / one module, mm.
    pub module_mm: f64,
    /// Smallest module the material/laser reproduces (from a named read test), mm.
    pub min_module_mm: f64,
    /// Quiet zone on each side in modules; `None` = the standard minimum of the symbology.
    pub quiet_modules: Option<u32>,
    /// Bar height of linear symbologies, mm (ignored by 2D symbologies).
    pub height_mm: f64,
    /// Index into `SymbolCapabilities::ecc_steps`; `None` = the default step.
    pub ecc_step: Option<u8>,
    /// `None` = the first mode of `SymbolCapabilities::input_modes`.
    pub input_mode: Option<SymbolInputMode>,
    /// PDF417 data columns (1..=30); `None` = automatic.
    pub columns: Option<u8>,
}

impl SymbologyParams {
    /// Parameters with every optional choice left to the symbology.
    pub fn new(module_mm: f64, min_module_mm: f64) -> Self {
        SymbologyParams {
            module_mm,
            min_module_mm,
            quiet_modules: None,
            height_mm: 10.0,
            ecc_step: None,
            input_mode: None,
            columns: None,
        }
    }
}

/// Module data of an encoded symbol, independent of its physical size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SymbolLayout {
    /// Bar/space widths in modules, bar first, without quiet zones.
    Linear { widths: Vec<u8> },
    /// Row-major modules (row 0 at the top), `true` = dark, without quiet zones.
    Matrix { width: usize, height: usize, modules: Vec<bool> },
}

impl SymbolLayout {
    /// Width in modules without quiet zones.
    pub fn width_modules(&self) -> usize {
        match self {
            SymbolLayout::Linear { widths } => widths.iter().map(|&w| usize::from(w)).sum(),
            SymbolLayout::Matrix { width, .. } => *width,
        }
    }
}

/// An encoded symbol as geometry plus the module data it came from.
#[derive(Debug, Clone, PartialEq)]
pub struct EncodedSymbol {
    pub symbology: Symbology,
    /// Closed rectangles (one per run of dark modules); y up, origin at the bottom-left corner of
    /// the quiet zone.
    pub path: Path,
    pub layout: SymbolLayout,
    pub module_mm: f64,
    /// Quiet zone applied on each side, modules.
    pub quiet_modules: u32,
    /// Standard minimum quiet zone of this symbol, modules.
    pub min_quiet_modules: u32,
    /// Width and height including the quiet zone, mm.
    pub size_mm: (f64, f64),
    /// The payload as encoded: digits with their check digit, ISBN as EAN-13, normalised GS1.
    pub canonical_payload: String,
}

/// Encodes `payload` as `symbology`.
pub fn encode_symbol(
    symbology: Symbology,
    payload: &str,
    params: &SymbologyParams,
) -> Result<EncodedSymbol, SymbolError> {
    engine::encode(symbology, payload, params)
}
