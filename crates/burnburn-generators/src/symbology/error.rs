use std::fmt;

use super::{SymbolInputMode, Symbology};
use crate::error::GenError;

/// Why a symbol could not be produced. Every case names the symbology and the offending input,
/// so the inspector can point at the field (E10: no silent clamping, no look-alike output).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum SymbolError {
    /// The symbology is in the catalogue but no verified encoder exists. The reason is the same
    /// text `catalog()` shows; no pattern is ever produced for it.
    Unsupported { symbology: Symbology, reason: &'static str },
    /// The payload has no characters.
    EmptyPayload { symbology: Symbology },
    /// The payload has `len` characters; `min..=max` are accepted.
    PayloadLength { symbology: Symbology, min: usize, max: usize, len: usize },
    /// A character the symbology cannot carry (`index` counts characters).
    InvalidChar { symbology: Symbology, ch: char, index: usize },
    /// A check digit/character that does not match the rest of the payload.
    CheckDigit { symbology: Symbology, expected: char, found: char },
    /// A structural rule of the symbology is violated (for example an ISBN prefix, UPC-E number
    /// system, Codabar start/stop letters or GS1 syntax).
    InvalidPayload { symbology: Symbology, rule: &'static str },
    /// A GS1 application identifier that is not in the verified table.
    UnknownAi { ai: String },
    /// The payload does not fit the largest symbol this encoder produces at the chosen settings.
    DataTooLong { symbology: Symbology, limit: &'static str },
    /// The requested quiet zone is below the standard minimum of the symbology.
    QuietZoneTooSmall { symbology: Symbology, min_modules: u32, got_modules: u32 },
    /// The symbology has no error-correction step `step` (it has `steps` steps; 0 = none).
    EccStepUnavailable { symbology: Symbology, step: u8, steps: u8 },
    /// The symbology cannot take this input mode.
    InputModeUnavailable { symbology: Symbology, mode: SymbolInputMode },
    /// A parameter that this symbology does not have (for example PDF417 columns on a QR code).
    ParamNotApplicable { symbology: Symbology, param: &'static str },
    /// A marker id outside the family (`0..count`).
    MarkerId { symbology: Symbology, id: u32, count: u32 },
    /// A physical parameter is not usable (non-finite, not positive, module below the minimum).
    Param(GenError),
    /// The underlying encoder refused an input the checks above did not anticipate.
    Engine { symbology: Symbology, message: String },
}

impl fmt::Display for SymbolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SymbolError::Unsupported { symbology, reason } => {
                write!(f, "{} is not supported: {reason}", symbology.display_name())
            }
            SymbolError::EmptyPayload { symbology } => write!(f, "{} needs a payload", symbology.display_name()),
            SymbolError::PayloadLength { symbology, min, max, len } => {
                write!(f, "{} payload has {len} characters; {min}..={max} are accepted", symbology.display_name())
            }
            SymbolError::InvalidChar { symbology, ch, index } => {
                write!(f, "{} cannot encode character {ch:?} at {index}", symbology.display_name())
            }
            SymbolError::CheckDigit { symbology, expected, found } => {
                write!(f, "{} check character is {found:?}; {expected:?} expected", symbology.display_name())
            }
            SymbolError::InvalidPayload { symbology, rule } => write!(f, "{}: {rule}", symbology.display_name()),
            SymbolError::UnknownAi { ai } => {
                write!(f, "GS1 application identifier ({ai}) is not in the verified table")
            }
            SymbolError::DataTooLong { symbology, limit } => {
                write!(f, "{} payload does not fit ({limit})", symbology.display_name())
            }
            SymbolError::QuietZoneTooSmall { symbology, min_modules, got_modules } => write!(
                f,
                "{} needs a quiet zone of at least {min_modules} modules; {got_modules} requested",
                symbology.display_name()
            ),
            SymbolError::EccStepUnavailable { symbology, step, steps } => {
                write!(f, "{} has {steps} error-correction steps; step {step} does not exist", symbology.display_name())
            }
            SymbolError::InputModeUnavailable { symbology, mode } => {
                write!(f, "{} does not support the {mode:?} input mode", symbology.display_name())
            }
            SymbolError::ParamNotApplicable { symbology, param } => {
                write!(f, "{} has no {param} setting", symbology.display_name())
            }
            SymbolError::MarkerId { symbology, id, count } => {
                write!(f, "{} has {count} markers; id {id} does not exist", symbology.display_name())
            }
            SymbolError::Param(e) => write!(f, "{e}"),
            SymbolError::Engine { symbology, message } => write!(f, "{}: {message}", symbology.display_name()),
        }
    }
}

impl std::error::Error for SymbolError {}

impl From<GenError> for SymbolError {
    fn from(e: GenError) -> Self {
        SymbolError::Param(e)
    }
}
