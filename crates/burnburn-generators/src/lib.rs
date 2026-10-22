//! Laser generators as pure functions (IM22): validated parameters in, geometry in millimetres
//! out, with stable output keys so a result becomes a `DocumentPatch` (contract §11).
//!
//! Generators never apply kerf: kerf compensation is CAM's job, exactly once
//! (`OperationSettings`). A generator's `fit` is design clearance (pasvorm), a separate meaning.

mod barcode;
mod bound_text;
mod boxes;
mod error;
mod evaluator;
mod hinge;
mod joints;
mod output;
mod path_text;
mod production;
mod qr;
mod rows;
mod stencil;
mod symbology;
mod variable;

pub use barcode::{Barcode, Code128Params, code128, code128_patterns, code128_symbols};
pub use bound_text::BoundTextEvaluator;
pub use boxes::{BoxParams, Lid, Panel, PanelRole, finger_box};
pub use error::GenError;
pub use evaluator::{GENERATOR_ENGINE, GeneratorEvaluator};
pub use hinge::{LivingHinge, living_hinge};
pub use joints::{
    EdgeProfile, EdgeStart, FingerJoint, JointSegment, SegmentKind, TabSlot, TabSlotResult, tab_and_slot,
};
pub use output::{GeneratedPart, to_outputs};
pub use path_text::{
    Distort, GlyphFrame, PathText, PathTextParams, guide_frame_at, guide_tangent_at, text_on_path, text_room,
};
pub use production::{
    AdvancePolicy, Commit, JobEvent, ProductionCounters, ProductionRecord, RecordBuilder, bake_command, resolve_text,
};
pub use qr::{
    QR_MAX_VERSION, QrCode, QrEcc, QrParams, QrSymbol, qr, qr_encode, qr_encode_with, qr_format_bits, qr_reed_solomon,
    qr_version_bits,
};
pub use rows::{CsvError, DataTable, FrozenRows, ResolvedRow, RowBinding, RowError, resolve, resolve_bound};
pub use stencil::{Stencil, stencil_bridges};
pub use symbology::{
    EccStep, EncodedSymbol, SymbolCapabilities, SymbolCatalogEntry, SymbolError, SymbolInputMode, SymbolKind,
    SymbolLayout, SymbolSource, SymbolStatus, Symbology, SymbologyParams, catalog as symbology_catalog, encode_symbol,
};
pub use variable::{CutField, JobClock, OnEnd, SerialCounter, SerialSpec, Template, Token, VariableError};
