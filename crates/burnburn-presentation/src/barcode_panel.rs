//! The barcode dialog (W3-UI-tekst, E10): the type list over all 29 symbologies (unsupported
//! ones visible with their reason), the parameters that belong to the chosen type, the encoded
//! size and canonical payload, field-level errors and the readability warning. Raw field text goes
//! to the controller unparsed (DR-05). The dialog creates the symbol as one ordinary path; a
//! barcode object with a stable key does not exist yet (queue W2-L5d).

use std::sync::Arc;

pub use burnburn_generators::{SymbolInputMode, SymbolKind, Symbology};

/// A field of the barcode dialog, named in [`BarcodeProblem`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BarcodeField {
    Payload,
    /// Module size (mm).
    Module,
    /// Smallest module the material and laser reproduce (mm).
    MinModule,
    /// Quiet zone on each side (modules).
    Quiet,
    /// Bar height of a linear symbol (mm).
    Height,
    Ecc,
    InputMode,
    /// PDF417 data columns.
    Columns,
    /// Total width including the quiet zone (mm).
    Width,
}

/// Why the symbol cannot be made, in terms hosts show in words. Never a substitute pattern.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum BarcodeIssue {
    /// The field does not hold a valid number.
    InvalidNumber,
    /// No verified encoder exists for this type; the reason is on its row of the type list.
    Unsupported,
    EmptyPayload,
    PayloadLength {
        min: usize,
        max: usize,
        len: usize,
    },
    /// A character the type cannot carry; `index` counts characters (the cursor goes there).
    InvalidChar {
        ch: char,
        index: usize,
    },
    /// The check digit is wrong; `expected` is the proposal.
    CheckDigit {
        expected: char,
        found: char,
    },
    InvalidPayload {
        rule: &'static str,
    },
    UnknownAi,
    DataTooLong,
    /// The quiet zone is below the standard minimum of the type.
    QuietZoneTooSmall {
        min_modules: u32,
        got_modules: u32,
    },
    /// The module is below the smallest the material and laser reproduce.
    ModuleTooSmall {
        min_mm: f64,
        value_mm: f64,
    },
    EccStepUnavailable,
    InputModeUnavailable,
    ParamNotApplicable,
    MarkerId {
        id: u32,
        count: u32,
    },
    /// The encoder refused an input nothing above anticipated.
    Engine,
}

/// A refusal with the field it belongs to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BarcodeProblem {
    pub field: BarcodeField,
    pub issue: BarcodeIssue,
}

/// One row of the type list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BarcodeTypeRow {
    pub symbology: Symbology,
    pub name: &'static str,
    /// An encoder exists; otherwise `reason` says why not.
    pub supported: bool,
    pub reason: Option<&'static str>,
    /// Where the encoder comes from (`None` when unsupported).
    pub source: Option<&'static str>,
}

/// One step of the error-correction slider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EccStepViewModel {
    pub label: &'static str,
    pub recovery_percent: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EccViewModel {
    pub steps: Vec<EccStepViewModel>,
    pub selected: u8,
}

/// The module is (or would be) smaller than the smallest the material and laser reproduce.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Readability {
    pub module_mm: f64,
    pub min_module_mm: f64,
}

/// The open barcode dialog.
#[derive(Debug, Clone, PartialEq)]
pub struct BarcodeDialogViewModel {
    pub types: Arc<[BarcodeTypeRow]>,
    pub symbology: Symbology,
    pub kind: SymbolKind,
    pub payload: String,
    /// One line on what the type accepts as payload.
    pub payload_hint: &'static str,
    pub module: String,
    pub min_module: String,
    /// Quiet zone typed (empty: the standard minimum) and that minimum (modules).
    pub quiet: String,
    pub quiet_minimum: u32,
    /// Bar height; only for linear symbols.
    pub height: Option<String>,
    /// Error-correction slider; only for types that have steps.
    pub ecc: Option<EccViewModel>,
    /// Accepted input modes (the first is the default).
    pub input_modes: Vec<SymbolInputMode>,
    pub input_mode: SymbolInputMode,
    /// PDF417 data columns (empty: automatic); only for types that use columns.
    pub columns: Option<String>,
    /// Number of marker ids of a fiducial family (the payload is the id).
    pub marker_count: Option<u32>,
    /// Total width including the quiet zone (mm) as typed or encoded.
    pub width: String,
    /// Width and height of the encoded symbol (mm), when it encodes.
    pub size: Option<(String, String)>,
    /// The payload as encoded: check digit added, ISBN-10 as ISBN-13.
    pub canonical: Option<String>,
    pub problems: Vec<BarcodeProblem>,
    /// Set when the module (or the quiet zone) is below its limit, with the limit.
    pub readability: Option<Readability>,
    pub can_create: bool,
}

/// An inspector-size change of a created barcode that would drop its module below the minimum.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BarcodeScaleWarning {
    /// The module after the scaling, mm.
    pub module_mm: f64,
    pub min_module_mm: f64,
}

/// One edit of the barcode dialog.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum BarcodeEdit {
    /// Another type; the type-dependent parameters go back to their defaults.
    Symbology(Symbology),
    Payload(String),
    Module(String),
    MinModule(String),
    Quiet(String),
    Height(String),
    /// Error-correction step (index into the slider's steps).
    Ecc(u8),
    InputMode(SymbolInputMode),
    Columns(String),
    /// Total width including the quiet zone: the module follows (raw text, mm).
    Width(String),
    /// Make the symbol in the middle of the view: one path, one undo step.
    Create,
    Cancel,
}
