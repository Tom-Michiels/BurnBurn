//! The export dialog (M026, P05): SVG/DXF options and the report of what the file cannot carry.
//!
//! The report is made on a worker before anything is written, for the options as they are now; a
//! report made for older options is dropped. Constraints and modifiers that would be lost need
//! the dialog's acknowledgement, and the file is only written with it.

use std::path::PathBuf;
use std::sync::Arc;

use burnburn_document::{DerivedKey, DerivedOwner, GeometrySource};
use burnburn_io::export::{DxfCurves, DxfLayers, DxfOptions, DxfUnit, ExportLoss, LostSemantics, SvgOptions, SvgUnit};
use burnburn_types::{DocumentId, OperationId};

use super::{AppController, AppUpdate, FilesJob};
use crate::controller::Session;
use crate::controller::layers::slots;
use crate::field::{parse_length, parse_number};
use crate::files::{
    ExportCurves, ExportDialogViewModel, ExportField, ExportFormat, ExportLayers, ExportLossRow, ExportOptionsInput,
    ExportUnit, FilesIntent, FilesMessage,
};
use crate::files_task::{ExportJob, ExportRefusal, FilesError, FilesOutput, FilesTask, OwnedExportItem};
use crate::task::{TaskError, TaskOutcome, TaskOutput};
use crate::view_model::MessageKey;

/// Window state of the dialog.
#[derive(Debug)]
pub(in crate::controller) struct ExportUi {
    format: ExportFormat,
    items: Arc<Vec<OwnedExportItem>>,
    /// The last options that were accepted.
    options: ExportOptionsInput,
    field_error: Option<ExportField>,
    acknowledged: bool,
    /// Counts the reports asked for: only the newest result is used.
    generation: u64,
    checking: bool,
    losses: Vec<ExportLossRow>,
    needs_acknowledgement: bool,
    busy: bool,
    issue: Option<MessageKey>,
    written: Option<PathBuf>,
}

impl ExportUi {
    fn selected(&self) -> impl Iterator<Item = &OwnedExportItem> {
        let only = self.options.selection_only;
        self.items.iter().filter(move |i| !only || i.selected)
    }
}

/// The options of both formats for the dialog's input, or the field that holds a wrong value.
fn parse_options(input: &ExportOptionsInput, acknowledged: bool) -> Result<(SvgOptions, DxfOptions), ExportField> {
    let positive_mm = |text: &str, field: ExportField| match parse_length(text) {
        Ok(l) if l.mm() > 0.0 => Ok(l.mm()),
        _ => Err(field),
    };
    let stroke_mm = positive_mm(&input.stroke_mm, ExportField::Stroke)?;
    let tolerance_mm = positive_mm(&input.tolerance_mm, ExportField::Tolerance)?;
    let decimals = match input.decimals.trim() {
        "" => None,
        text => match parse_number(text, &[]) {
            Ok(n) if n.fract() == 0.0 && (0.0..=12.0).contains(&n) => Some(n as u8),
            _ => return Err(ExportField::Decimals),
        },
    };
    let svg = SvgOptions {
        unit: match input.unit {
            ExportUnit::Millimeter => SvgUnit::Mm,
            ExportUnit::Inch => SvgUnit::Inch,
            ExportUnit::Pixel => SvgUnit::Pixel,
        },
        stroke_mm,
        use_item_colors: input.use_colors,
        group_layers: input.layers == ExportLayers::Layer,
        decimals,
        acknowledge_lost_semantics: acknowledged,
        ..SvgOptions::default()
    };
    let dxf = DxfOptions {
        unit: match input.unit {
            ExportUnit::Inch => DxfUnit::Inch,
            _ => DxfUnit::Mm,
        },
        layers: match input.layers {
            ExportLayers::Object => DxfLayers::ItemName,
            ExportLayers::Layer => DxfLayers::ItemLayerOrName,
            ExportLayers::Single => DxfLayers::Single,
        },
        curves: match input.curves {
            ExportCurves::Splines => DxfCurves::Splines,
            ExportCurves::Polylines => DxfCurves::Polylines { tolerance_mm },
        },
        use_item_colors: input.use_colors,
        acknowledge_lost_semantics: acknowledged,
    };
    Ok((svg, dxf))
}

/// The item an export loss is about.
fn item_of(loss: &ExportLoss) -> Option<usize> {
    match loss {
        ExportLoss::EllipticalArcAsSpline { item, .. }
        | ExportLoss::CircularArcAsSpline { item, .. }
        | ExportLoss::ConstraintsDropped { item, .. }
        | ExportLoss::ModifiersApplied { item, .. }
        | ExportLoss::ParametricFlattened { item }
        | ExportLoss::TextAsOutlines { item }
        | ExportLoss::CurvesFlattened { item, .. }
        | ExportLoss::ColorApproximated { item } => Some(*item),
        _ => None,
    }
}

/// The objects of the document as export items: layer and colour of their operation, and what
/// their file form cannot carry (constraints of the sketch they came from, modifier steps,
/// parametric definition, text).
fn export_items(s: &Session, palette: &[[u8; 3]]) -> Vec<OwnedExportItem> {
    let snapshot = s.doc.snapshot();
    let process = snapshot.process();
    let colours = slots(&s.layers, &snapshot);
    let mut seen: Vec<DerivedOwner> = Vec::new();
    let mut first_of = |owner: DerivedOwner| {
        let first = !seen.contains(&owner);
        if first {
            seen.push(owner);
        }
        first
    };
    snapshot
        .objects()
        .map(|(id, obj)| {
            let operation: Option<OperationId> = process.operations_using(id).first().copied();
            let layer = operation.and_then(|op| process.operation(op)).map(|op| op.name().to_owned());
            let color = operation
                .and_then(|op| colours.get(&op))
                .and_then(|slot| (!palette.is_empty()).then(|| palette[usize::from(*slot) % palette.len()]));
            let mut lost = LostSemantics { parametric: obj.parametric().is_some(), ..LostSemantics::default() };
            if let GeometrySource::Derived(DerivedKey { owner, .. }) = obj.source() {
                match owner {
                    DerivedOwner::Sketch(sketch) if first_of(*owner) => {
                        let count = snapshot.sketch(*sketch).map_or(0, |sk| sk.constraints().len());
                        lost.constraints = u32::try_from(count).unwrap_or(u32::MAX);
                    }
                    DerivedOwner::Modifier(m) if first_of(*owner) => {
                        let steps = snapshot.structure().modifier(*m).map_or(0, |c| c.steps.len());
                        lost.modifiers = u32::try_from(steps).unwrap_or(u32::MAX);
                    }
                    DerivedOwner::Text(_) => lost.text_outlined = true,
                    _ => {}
                }
            }
            OwnedExportItem {
                name: obj.name().to_owned(),
                layer,
                color,
                lost,
                path: Arc::clone(obj.path()),
                selected: s.selection.contains(id),
            }
        })
        .collect()
}

impl AppController {
    /// The colours of the layer palette, slot by slot, for export files (the host owns the palette).
    pub fn set_layer_palette(&mut self, palette: Vec<[u8; 3]>) {
        self.libs.palette = palette;
    }

    pub(super) fn export_intent(&mut self, intent: FilesIntent) -> AppUpdate {
        match intent {
            FilesIntent::OpenExport { document, format } => {
                let palette = self.libs.palette.clone();
                let update = self.with_session(document, |s| {
                    let items = Arc::new(export_items(s, &palette));
                    let options =
                        ExportOptionsInput { selection_only: !s.selection.is_empty(), ..ExportOptionsInput::default() };
                    s.files.export = Some(ExportUi {
                        format,
                        items,
                        options,
                        field_error: None,
                        acknowledged: false,
                        generation: 0,
                        checking: false,
                        losses: Vec::new(),
                        needs_acknowledgement: false,
                        busy: false,
                        issue: None,
                        written: None,
                    });
                    AppUpdate { view_changed: true, ..AppUpdate::default() }
                });
                self.check_export(document);
                update
            }
            FilesIntent::CloseExport { document } => self.with_session(document, |s| {
                s.files.export = None;
                AppUpdate { view_changed: true, ..AppUpdate::default() }
            }),
            FilesIntent::SetExportOptions { document, options } => {
                let Some(ui) = self.sessions.get_mut(&document).and_then(|s| s.files.export.as_mut()) else {
                    return AppUpdate::default();
                };
                match parse_options(&options, ui.acknowledged) {
                    Ok(_) => {
                        ui.options = *options;
                        ui.field_error = None;
                        ui.issue = None;
                        ui.written = None;
                        self.check_export(document);
                    }
                    Err(field) => {
                        ui.field_error = Some(field);
                        ui.issue = Some(MessageKey::Files(FilesMessage::ExportInvalidOption));
                    }
                }
                AppUpdate { view_changed: true, ..AppUpdate::default() }
            }
            FilesIntent::SetExportAcknowledged { document, acknowledged } => self.with_session(document, |s| {
                if let Some(ui) = s.files.export.as_mut() {
                    ui.acknowledged = acknowledged;
                    ui.issue = None;
                }
                AppUpdate { view_changed: true, ..AppUpdate::default() }
            }),
            FilesIntent::ExportDrawing { document, path } => self.export_drawing(document, path),
            _ => AppUpdate::default(),
        }
    }

    /// The job for the dialog as it is now.
    fn export_job(&self, document: DocumentId) -> Option<(ExportJob, u64, bool)> {
        let ui = self.sessions.get(&document)?.files.export.as_ref()?;
        let (svg_options, dxf_options) = parse_options(&ui.options, ui.acknowledged).ok()?;
        let job = ExportJob {
            dxf: ui.format == ExportFormat::Dxf,
            items: Arc::clone(&ui.items),
            selection_only: ui.options.selection_only,
            svg_options,
            dxf_options,
        };
        Some((job, ui.generation, ui.acknowledged))
    }

    /// Asks a worker for the report of the current options; the answer of an older ask is ignored.
    fn check_export(&mut self, document: DocumentId) {
        let Some(ui) = self.sessions.get_mut(&document).and_then(|s| s.files.export.as_mut()) else { return };
        ui.generation += 1;
        ui.checking = true;
        let generation = ui.generation;
        let Some((job, ..)) = self.export_job(document) else { return };
        self.submit_files(FilesTask::ExportPreflight(Box::new(job)), FilesJob::ExportCheck { document, generation });
    }

    pub(super) fn export_checked(&mut self, document: DocumentId, generation: u64, outcome: TaskOutcome) -> AppUpdate {
        let result = outcome.result;
        self.with_session(document, |s| {
            let Some(ui) = s.files.export.as_mut().filter(|ui| ui.generation == generation) else {
                return AppUpdate::default();
            };
            ui.checking = false;
            match result {
                Ok(TaskOutput::Files(FilesOutput::ExportChecked { result })) => {
                    let names: Vec<String> = ui.selected().map(|i| i.name.clone()).collect();
                    let losses = result.unwrap_or_default();
                    ui.needs_acknowledgement = losses.iter().any(ExportLoss::needs_acknowledgement);
                    ui.losses = losses
                        .into_iter()
                        .map(|loss| ExportLossRow {
                            object: item_of(&loss)
                                .and_then(|i| names.get(i))
                                .map(|n| (*n).to_owned())
                                .unwrap_or_default(),
                            loss,
                        })
                        .collect();
                }
                Err(TaskError::Cancelled) => {}
                _ => ui.issue = Some(MessageKey::Files(FilesMessage::ExportInvalidOption)),
            }
            AppUpdate { view_changed: true, ..AppUpdate::default() }
        })
    }

    fn export_drawing(&mut self, document: DocumentId, path: PathBuf) -> AppUpdate {
        let Some((job, ..)) = self.export_job(document) else { return AppUpdate::default() };
        let Some(ui) = self.sessions.get_mut(&document).and_then(|s| s.files.export.as_mut()) else {
            return AppUpdate::default();
        };
        let objects = ui.selected().count();
        if objects == 0 {
            ui.issue = Some(MessageKey::Files(FilesMessage::ExportEmpty));
        } else if ui.needs_acknowledgement && !ui.acknowledged {
            ui.issue = Some(MessageKey::Files(FilesMessage::ExportNeedsAcknowledgement));
        } else if ui.checking {
            // The report of the current options is not there yet: wait for it, never guess.
        } else {
            ui.busy = true;
            ui.issue = None;
            ui.written = None;
            let task = FilesTask::ExportWrite { job: Box::new(job), path, fs: Arc::clone(&self.ports.fs) };
            self.submit_files(task, FilesJob::ExportWrite { document });
        }
        AppUpdate { view_changed: true, ..AppUpdate::default() }
    }

    pub(super) fn export_written(&mut self, document: DocumentId, outcome: TaskOutcome) -> AppUpdate {
        let result = outcome.result;
        self.with_session(document, |s| {
            let Some(ui) = s.files.export.as_mut() else { return AppUpdate::default() };
            ui.busy = false;
            match result {
                Ok(TaskOutput::Files(FilesOutput::ExportWritten { path, .. })) => ui.written = Some(path),
                Err(TaskError::Cancelled) => {}
                Err(TaskError::Files(FilesError::Export(ExportRefusal::Empty))) => {
                    ui.issue = Some(MessageKey::Files(FilesMessage::ExportEmpty));
                }
                Err(TaskError::Files(FilesError::Export(ExportRefusal::LossesNotAcknowledged))) => {
                    ui.issue = Some(MessageKey::Files(FilesMessage::ExportNeedsAcknowledgement));
                }
                _ => ui.issue = Some(MessageKey::Files(FilesMessage::ExportWriteFailed)),
            }
            AppUpdate { view_changed: true, ..AppUpdate::default() }
        })
    }
}

pub(super) fn panel_view(s: &Session) -> Option<ExportDialogViewModel> {
    let ui = s.files.export.as_ref()?;
    Some(ExportDialogViewModel {
        format: ui.format,
        options: ui.options.clone(),
        field_error: ui.field_error,
        objects: ui.selected().count(),
        losses: ui.losses.clone(),
        needs_acknowledgement: ui.needs_acknowledgement,
        acknowledged: ui.acknowledged,
        checking: ui.checking,
        busy: ui.busy,
        issue: ui.issue,
        written: ui.written.clone(),
    })
}
