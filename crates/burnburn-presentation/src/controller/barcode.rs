//! The barcode dialog (W3-UI-tekst, E10). Every edit re-encodes with the domain's
//! `encode_symbol`; whatever the domain refuses is shown at its field and nothing is rounded or
//! substituted. Create makes the dark modules as one ordinary path in the middle of the view; the
//! window remembers the module size of what it made, so scaling it through the inspector can warn
//! when the module would fall below the minimum (window state, not part of the project file).

use std::collections::BTreeMap;
use std::sync::Arc;

use burnburn_document::{CommandPayload, GeometryCommand};
use burnburn_generators::{
    EncodedSymbol, GenError, SymbolError, SymbolInputMode, SymbolKind, SymbolSource, SymbolStatus, Symbology,
    SymbologyParams, encode_symbol, symbology_catalog,
};
use burnburn_geometry::{Affine, Vector};
use burnburn_types::{DocumentId, GeometryId, RequestId};

use crate::barcode_panel::{
    BarcodeDialogViewModel, BarcodeEdit, BarcodeField, BarcodeIssue, BarcodeProblem, BarcodeScaleWarning,
    BarcodeTypeRow, EccStepViewModel, EccViewModel, Readability,
};
use crate::field::{format_mm, parse_length};

use super::{AppUpdate, Session, apply_edit};

/// Smallest module a new dialog starts with; the user names the real limit of the material.
const DEFAULT_MODULE_MM: &str = "0.5";
const DEFAULT_MIN_MODULE_MM: &str = "0.2";
const DEFAULT_HEIGHT_MM: &str = "10";

/// What this window made: the module and its minimum, and the width the symbol had.
#[derive(Debug, Clone, Copy)]
struct Made {
    module_mm: f64,
    min_module_mm: f64,
    width_mm: f64,
}

#[derive(Debug, Default)]
pub(super) struct BarcodeState {
    dialog: Option<Dialog>,
    made: BTreeMap<GeometryId, Made>,
}

#[derive(Debug)]
struct Dialog {
    symbology: Symbology,
    payload: String,
    module: String,
    min_module: String,
    quiet: String,
    height: String,
    ecc: Option<u8>,
    input_mode: Option<SymbolInputMode>,
    columns: String,
    /// The width last typed (the module followed from it).
    width: Option<String>,
    problems: Vec<BarcodeProblem>,
    encoded: Option<EncodedSymbol>,
}

impl Dialog {
    fn new() -> Self {
        let mut d = Dialog {
            symbology: Symbology::QrCode,
            payload: String::new(),
            module: DEFAULT_MODULE_MM.into(),
            min_module: DEFAULT_MIN_MODULE_MM.into(),
            quiet: String::new(),
            height: DEFAULT_HEIGHT_MM.into(),
            ecc: None,
            input_mode: None,
            columns: String::new(),
            width: None,
            problems: Vec::new(),
            encoded: None,
        };
        d.recompute();
        d
    }

    fn problem(&mut self, field: BarcodeField, issue: BarcodeIssue) {
        self.problems.push(BarcodeProblem { field, issue });
    }

    /// The parameters as typed, or the fields that are not numbers.
    fn params(&mut self) -> Option<SymbologyParams> {
        let caps = self.symbology.capabilities();
        let mut ok = true;
        let mut length = |d: &mut Dialog, field, text: &str| match parse_length(text) {
            Ok(l) => Some(l.mm()),
            Err(_) => {
                d.problem(field, BarcodeIssue::InvalidNumber);
                ok = false;
                None
            }
        };
        let module = length(self, BarcodeField::Module, &self.module.clone());
        let min_module = length(self, BarcodeField::MinModule, &self.min_module.clone());
        let height = if caps.kind == SymbolKind::Linear {
            length(self, BarcodeField::Height, &self.height.clone())
        } else {
            Some(10.0)
        };
        let quiet = if self.quiet.trim().is_empty() {
            None
        } else if let Ok(q) = self.quiet.trim().parse::<u32>() {
            Some(q)
        } else {
            self.problem(BarcodeField::Quiet, BarcodeIssue::InvalidNumber);
            ok = false;
            None
        };
        let columns = if caps.uses_columns && !self.columns.trim().is_empty() {
            match self.columns.trim().parse::<u8>() {
                Ok(c) => Some(c),
                Err(_) => {
                    self.problem(BarcodeField::Columns, BarcodeIssue::InvalidNumber);
                    ok = false;
                    None
                }
            }
        } else {
            None
        };
        if !ok {
            return None;
        }
        let mut params = SymbologyParams::new(module?, min_module?);
        params.height_mm = height?;
        params.quiet_modules = quiet;
        params.ecc_step = self.ecc;
        params.input_mode = self.input_mode;
        params.columns = columns;
        Some(params)
    }

    fn recompute(&mut self) {
        self.problems.clear();
        self.encoded = None;
        let Some(params) = self.params() else { return };
        match encode_symbol(self.symbology, &self.payload, &params) {
            Ok(e) => self.encoded = Some(e),
            Err(e) => {
                let p = problem_of(&e);
                self.problems.push(p);
            }
        }
    }
}

fn problem_of(e: &SymbolError) -> BarcodeProblem {
    use BarcodeField as F;
    use BarcodeIssue as I;
    let (field, issue) = match e {
        SymbolError::Unsupported { .. } => (F::Payload, I::Unsupported),
        SymbolError::EmptyPayload { .. } => (F::Payload, I::EmptyPayload),
        SymbolError::PayloadLength { min, max, len, .. } => {
            (F::Payload, I::PayloadLength { min: *min, max: *max, len: *len })
        }
        SymbolError::InvalidChar { ch, index, .. } => (F::Payload, I::InvalidChar { ch: *ch, index: *index }),
        SymbolError::CheckDigit { expected, found, .. } => {
            (F::Payload, I::CheckDigit { expected: *expected, found: *found })
        }
        SymbolError::InvalidPayload { rule, .. } => (F::Payload, I::InvalidPayload { rule }),
        SymbolError::UnknownAi { .. } => (F::Payload, I::UnknownAi),
        SymbolError::DataTooLong { .. } => (F::Payload, I::DataTooLong),
        SymbolError::QuietZoneTooSmall { min_modules, got_modules, .. } => {
            (F::Quiet, I::QuietZoneTooSmall { min_modules: *min_modules, got_modules: *got_modules })
        }
        SymbolError::EccStepUnavailable { .. } => (F::Ecc, I::EccStepUnavailable),
        SymbolError::InputModeUnavailable { .. } => (F::InputMode, I::InputModeUnavailable),
        SymbolError::ParamNotApplicable { .. } => (F::Columns, I::ParamNotApplicable),
        SymbolError::MarkerId { id, count, .. } => (F::Payload, I::MarkerId { id: *id, count: *count }),
        SymbolError::Param(GenError::TooSmall { param: "module", min_mm, value_mm }) => {
            (F::Module, I::ModuleTooSmall { min_mm: *min_mm, value_mm: *value_mm })
        }
        SymbolError::Param(GenError::NotPositive { param } | GenError::NotFinite { param }) => {
            let field = match *param {
                "module" => F::Module,
                "min_module" => F::MinModule,
                "height_mm" => F::Height,
                _ => F::Payload,
            };
            (field, I::InvalidNumber)
        }
        SymbolError::Param(GenError::Count { param: "columns", .. }) => (F::Columns, I::InvalidNumber),
        SymbolError::Param(_) | SymbolError::Engine { .. } => (F::Payload, I::Engine),
        _ => (F::Payload, I::Engine),
    };
    BarcodeProblem { field, issue }
}

/// A module value with enough digits that the symbol is as wide as asked (to the micrometre).
fn precise(mm: f64) -> String {
    let s = format!("{mm:.6}");
    s.trim_end_matches('0').trim_end_matches('.').to_owned()
}

pub(super) fn open(s: &mut Session, document: DocumentId) -> AppUpdate {
    s.barcode.dialog = Some(Dialog::new());
    AppUpdate::redraw(document)
}

fn changed() -> AppUpdate {
    AppUpdate { view_changed: true, ..AppUpdate::default() }
}

pub(super) fn edit(s: &mut Session, document: DocumentId, edit: BarcodeEdit, request: Option<RequestId>) -> AppUpdate {
    let Some(d) = s.barcode.dialog.as_mut() else { return AppUpdate::default() };
    match edit {
        BarcodeEdit::Cancel => {
            s.barcode.dialog = None;
            return AppUpdate::redraw(document);
        }
        BarcodeEdit::Create => return create(s, document, request),
        BarcodeEdit::Symbology(symbology) => {
            let (old, new) = (d.symbology.capabilities().kind, symbology.capabilities().kind);
            d.symbology = symbology;
            d.ecc = None;
            d.input_mode = None;
            d.columns.clear();
            d.quiet.clear();
            if new == SymbolKind::Marker {
                d.payload = "0".into();
            } else if old == SymbolKind::Marker {
                d.payload.clear();
            }
        }
        BarcodeEdit::Payload(text) => d.payload = text,
        BarcodeEdit::Module(text) => {
            d.module = text;
            d.width = None;
        }
        BarcodeEdit::MinModule(text) => d.min_module = text,
        BarcodeEdit::Quiet(text) => d.quiet = text,
        BarcodeEdit::Height(text) => d.height = text,
        BarcodeEdit::Ecc(step) => d.ecc = Some(step),
        BarcodeEdit::InputMode(mode) => d.input_mode = Some(mode),
        BarcodeEdit::Columns(text) => d.columns = text,
        BarcodeEdit::Width(text) => {
            d.width = Some(text.clone());
            match parse_length(&text) {
                Err(_) => {
                    d.recompute();
                    d.problem(BarcodeField::Width, BarcodeIssue::InvalidNumber);
                    return changed();
                }
                Ok(width) => {
                    // At a module of 1 mm the width in mm is the width in modules.
                    d.module = "1".into();
                    d.min_module = "1".into();
                    let modules = d.params().and_then(|p| encode_symbol(d.symbology, &d.payload, &p).ok());
                    d.min_module = DEFAULT_MIN_MODULE_MM.into();
                    if let Some(m) = modules {
                        d.module = precise(width.mm() / m.size_mm.0);
                    }
                }
            }
        }
    }
    d.recompute();
    changed()
}

/// The symbol, centred on the view centre, as one `CreatePath`.
fn create(s: &mut Session, document: DocumentId, request: Option<RequestId>) -> AppUpdate {
    let Some(d) = s.barcode.dialog.as_ref() else { return AppUpdate::default() };
    let Some(symbol) = d.encoded.clone() else { return AppUpdate::default() };
    let (module_mm, min_module_mm) = (symbol.module_mm, parse_length(&d.min_module).map_or(0.0, |l| l.mm()));
    let Some(bounds) = symbol.path.bounds() else { return AppUpdate::default() };
    let centre = s.camera.center_mm();
    let shift = Vector { x: centre.x - bounds.center().x, y: centre.y - bounds.center().y };
    let Ok(place) = Affine::translate(shift) else { return AppUpdate::default() };
    let canonical: String = symbol.canonical_payload.chars().take(40).collect();
    let name = format!("{}: {canonical}", symbol.symbology.display_name());
    let path = symbol.path.transform(&place);
    let before = s.doc.revision();
    let update = apply_edit(s, document, CommandPayload::Edit(GeometryCommand::CreatePath { path, name }), request);
    if s.doc.revision() == before {
        return update;
    }
    s.barcode.dialog = None;
    if let [id] = s.selection.ids() {
        s.barcode.made.insert(*id, Made { module_mm, min_module_mm, width_mm: bounds.width() });
    }
    update
}

fn type_rows() -> Arc<[BarcodeTypeRow]> {
    symbology_catalog()
        .into_iter()
        .map(|e| {
            let (supported, reason, source) = match e.status {
                SymbolStatus::Implemented { source } => (
                    true,
                    None,
                    Some(match source {
                        SymbolSource::Own => "BurnBurn",
                        SymbolSource::Library(name) => name,
                    }),
                ),
                SymbolStatus::Unsupported { reason } => (false, Some(reason), None),
            };
            BarcodeTypeRow { symbology: e.symbology, name: e.symbology.display_name(), supported, reason, source }
        })
        .collect()
}

pub(super) fn dialog_view(s: &Session) -> Option<BarcodeDialogViewModel> {
    let d = s.barcode.dialog.as_ref()?;
    let caps = d.symbology.capabilities();
    let default_ecc = caps.default_ecc_step.unwrap_or(0);
    let selected = d.ecc.filter(|e| usize::from(*e) < caps.ecc_steps.len()).unwrap_or(default_ecc);
    let readability = d.problems.iter().find_map(|p| match p.issue {
        BarcodeIssue::ModuleTooSmall { min_mm, value_mm } => {
            Some(Readability { module_mm: value_mm, min_module_mm: min_mm })
        }
        _ => None,
    });
    Some(BarcodeDialogViewModel {
        types: type_rows(),
        symbology: d.symbology,
        kind: caps.kind,
        payload: d.payload.clone(),
        payload_hint: caps.payload_hint,
        module: d.module.clone(),
        min_module: d.min_module.clone(),
        quiet: d.quiet.clone(),
        quiet_minimum: caps.min_quiet_modules,
        height: (caps.kind == SymbolKind::Linear).then(|| d.height.clone()),
        ecc: (!caps.ecc_steps.is_empty()).then(|| EccViewModel {
            steps: caps
                .ecc_steps
                .iter()
                .map(|e| EccStepViewModel { label: e.label, recovery_percent: e.recovery_percent })
                .collect(),
            selected,
        }),
        input_modes: caps.input_modes.to_vec(),
        input_mode: d.input_mode.unwrap_or(caps.input_modes[0]),
        columns: caps.uses_columns.then(|| d.columns.clone()),
        marker_count: caps.marker_count,
        width: d.width.clone().or_else(|| d.encoded.as_ref().map(|e| format_mm(e.size_mm.0))).unwrap_or_default(),
        size: d.encoded.as_ref().map(|e| (format_mm(e.size_mm.0), format_mm(e.size_mm.1))),
        canonical: d.encoded.as_ref().map(|e| e.canonical_payload.clone()),
        problems: d.problems.clone(),
        readability,
        can_create: d.encoded.is_some(),
    })
}

/// The selected barcode of this window would have a module below its minimum at its current width.
pub(super) fn scale_warning(s: &Session) -> Option<BarcodeScaleWarning> {
    let [id] = s.selection.ids() else { return None };
    let made = s.barcode.made.get(id)?;
    let width = s.doc.snapshot().object(*id)?.bounds()?.width();
    let module_mm = made.module_mm * width / made.width_mm;
    (module_mm < made.min_module_mm - 1e-9)
        .then_some(BarcodeScaleWarning { module_mm, min_module_mm: made.min_module_mm })
}
