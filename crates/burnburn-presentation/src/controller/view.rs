//! The view models and scene a window draws.

use crate::catalog::{ActiveTool, ToolCatalog, ToolState, ToolbarViewModel};
use std::sync::Arc;

use burnburn_editor::{SceneRequest, SceneSnapshot, boolean_operands, build_scene};
use burnburn_types::{DocumentId, Unit};

use crate::intent::{CommandKey, FieldKey};
use crate::production::Role;
use crate::view_model::{
    AppViewModel, CommandState, DocumentViewModel, FieldViewModel, InspectorViewModel, JobCardViewModel, MessageKey,
    ObjectRow, StatusMessage, TaskBadge,
};

use super::commands::tool_for;
use super::geometry::current_boolean_preview;
use super::import::import_view;
use super::machine::Machine;
use super::process::{plan_view, preparable};
use super::shapes;
use super::sketch;
use super::sketch::sketch_rows;
use super::trace::tracer_view;
use super::{AppController, Session, layers};

impl AppController {
    pub fn view(&self) -> Arc<AppViewModel> {
        let status = self.machine_port.as_ref().map(|p| p.status());
        let operator =
            Some(self.operator_view()).filter(|o| o.machine.is_some() || !o.jobs.is_empty() || !o.orders.is_empty());
        let start_confirm = self.start_confirm_view();
        let remnants = self.remnants_view();
        let recognition = self.recognition_view();
        let documents = self
            .sessions
            .iter()
            .map(|(id, s)| {
                let mut vm = document_view(*id, s, self.machine.as_ref(), self.app.prefs.units);
                vm.variable_panel = super::variable::panel_view(s, self.clock.as_ref().and_then(|c| c()));
                vm.operator.clone_from(&operator);
                vm.trace_recipes = super::trace_recipes::view(s, &self.app);
                vm.start_confirm.clone_from(&start_confirm);
                vm.remnants.clone_from(&remnants);
                vm.recognition.clone_from(&recognition);
                vm.files = super::library::files_view(s, self.machine.as_ref(), &self.libs, &self.recent_templates);
                vm.job = s.prepared.as_ref().map(|p| JobCardViewModel {
                    plan: p.job.plan,
                    plan_revision: p.job.plan_revision,
                    rd_hash: p.job.rd_hash,
                    bytes: p.job.rd.len(),
                    approved: p.approved,
                    design_changed: p.job.plan_revision != s.doc.revision(),
                    upload: status.as_ref().map(|st| self.upload_button(st, p)),
                });
                if vm.job.as_ref().is_some_and(|j| j.design_changed) {
                    vm.messages.push(StatusMessage { key: MessageKey::DesignChangedSinceJob });
                }
                vm
            })
            .collect();
        let active = self.active.and_then(|id| self.sessions.get(&id));
        let commands = CommandKey::ALL
            .iter()
            .map(|k| {
                let mut state = command_state(*k, active, self.machine.as_ref());
                if self.role == Role::Operator && *k != CommandKey::Close {
                    state.enabled = false;
                }
                (*k, state)
            })
            .collect();
        let toolbar = ToolbarViewModel::build(
            ToolCatalog::standard(),
            ToolState {
                document: active.is_some(),
                operator: self.role == Role::Operator,
                in_sketch: active.is_some_and(|s| s.sketch.active.is_some()),
            },
            ActiveTool {
                tool: active.map(|s| s.editor.tool()),
                trim: active.is_some_and(|s| s.trim_mode),
                polygon: active
                    .filter(|s| s.editor.tool() == burnburn_editor::ToolKind::Polygon)
                    .map(|s| s.editor.shape_defaults().polygon_construction),
            },
            &self.toolbar_variants,
        );
        // Commands that cannot act on the current selection say why on their button.
        let mut toolbar = toolbar;
        if let Some(s) = active {
            toolbar.refine(ToolCatalog::standard(), |key| shapes::disabled_reason(key, s));
        }
        Arc::new(AppViewModel { documents, active: self.active, commands, toolbar })
    }

    /// The scene one window draws (contract §5): document revision, selection, gesture preview,
    /// snap target, issues and boolean preview, with the window's camera.
    pub fn scene(&self, document: DocumentId) -> Option<Arc<SceneSnapshot>> {
        let s = self.sessions.get(&document)?;
        let snapshot = s.doc.snapshot();
        let preview = s.editor.preview();
        let result_preview = super::offset_preview::current_result(s)
            .or_else(|| super::repair_preview::current_result(s))
            .or_else(|| {
                s.boolean_preview.as_ref().filter(|b| b.revision == snapshot.revision()).map(|b| Arc::clone(&b.result))
            });
        let mut sketch_overlays = sketch::scene_overlays(s, preview.as_ref());
        sketch_overlays.extend(super::nest::scene_overlays(s));
        sketch_overlays.extend(super::array_dialog::scene_overlays(s));
        sketch_overlays.extend(super::kerf_view::scene_overlays(s));
        sketch_overlays.extend(super::generator::scene_overlays(s));
        let mut scene = build_scene(&SceneRequest {
            document,
            snapshot: &snapshot,
            selection: &s.selection,
            tool: s.editor.tool(),
            preview: preview.as_ref(),
            snap: s.editor.snap(),
            issues: &s.issues,
            extra: &sketch_overlays,
            result_preview,
            selected_nodes: s.editor.node_selection(),
            camera: s.camera,
            viewport: s.viewport,
        });
        layers::style_scene(s, &snapshot, &mut scene);
        scene.overlays.extend(self.camera_overlays());
        scene.overlays.extend(self.recognition_overlays());
        scene.overlays.extend(super::image::preview_overlay(s));
        scene.overlays.extend(super::trace_overlay::overlays(s));
        Some(Arc::new(scene))
    }
}

pub(super) fn document_view(
    id: DocumentId,
    s: &Session,
    machine: Option<&Machine>,
    display: super::prefs::LengthUnit,
) -> DocumentViewModel {
    let snapshot = s.doc.snapshot();
    // One pass over the layers: how many of them use each object.
    let mut linked: std::collections::BTreeMap<burnburn_types::GeometryId, usize> = std::collections::BTreeMap::new();
    for op in snapshot.process().operations() {
        for g in op.targets() {
            *linked.entry(*g).or_default() += 1;
        }
    }
    let objects = snapshot
        .objects()
        .map(|(gid, obj)| ObjectRow {
            linked: linked.get(&gid).copied().unwrap_or(0),
            id: gid,
            label: obj.name().to_owned(),
            depth: 0,
            selected: s.selection.contains(gid),
            visible: !snapshot.is_hidden(gid),
            locked: snapshot.is_locked(gid),
            groups: obj.groups().to_vec(),
        })
        .collect();
    let inspector = s.selection.reference().and_then(|gid| {
        let b = s.selection.bounds(&snapshot)?;
        let at = super::arrange::reference_in(&b, s.arrange.reference);
        // Lengths are shown in the display unit; the document stays in millimetres.
        let field = |key: FieldKey, mm: f64| FieldViewModel {
            key,
            label: MessageKey::Field(key),
            text: super::prefs::format_length(mm, display),
            unit: match display {
                super::prefs::LengthUnit::Millimeters => Unit::Millimeter,
                super::prefs::LengthUnit::Inches => Unit::Inch,
            },
            editable: true,
            error: s.field_errors.get(&key).copied(),
        };
        // Action fields show the neutral value; a commit applies the typed amount to the selection.
        let action = |key: FieldKey, neutral: &str| FieldViewModel { text: neutral.into(), ..field(key, 0.0) };
        Some(InspectorViewModel {
            object: gid,
            fields: vec![
                field(FieldKey::X, at.x),
                field(FieldKey::Y, at.y),
                field(FieldKey::Width, b.width()),
                field(FieldKey::Height, b.height()),
                action(FieldKey::Rotation, "0"),
                action(FieldKey::ScaleX, "100"),
                action(FieldKey::ScaleY, "100"),
                action(FieldKey::ShearX, "0"),
                action(FieldKey::ShearY, "0"),
                super::process::priority_field(s).unwrap_or_else(|| action(FieldKey::Priority, "0")),
            ],
        })
    });
    DocumentViewModel {
        id,
        revision: s.doc.revision(),
        title: s.title.clone(),
        dirty: s.unsaved(),
        tool: s.editor.tool(),
        preview: s.editor.preview(),
        objects,
        selection: s.selection.ids().to_vec(),
        inspector,
        undo: s.doc.undo_label(),
        redo: s.doc.redo_label(),
        messages: s.messages.clone(),
        tasks: s.pending.iter().map(|(tid, (kind, _))| TaskBadge { id: *tid, kind: *kind }).collect(),
        measure: s.measure.clone(),
        tool_measurement: s.tool_measurement.clone(),
        camera: s.camera,
        viewport: s.viewport,
        snap: s.editor.snap(),
        snap_settings: s.snap,
        selection_cycle: s.cycle,
        align_reference: (s.selection.len() >= 2).then(|| s.selection.reference()).flatten(),
        reference_point: s.arrange.reference,
        aspect_locked: s.arrange.aspect_locked,
        wireframe: s.arrange.wireframe,
        issues: s.issues.clone(),
        tool_size_error: s.tool_size_error,
        boolean_preview: current_boolean_preview(s).cloned(),
        sketches: sketch_rows(&snapshot),
        plan: s.compiled.as_ref().map(|c| plan_view(c, s, machine)),
        export: s.export.clone(),
        gcode_export: s.gcode_export.clone(),
        tracer: s.tracer.as_ref().map(|t| tracer_view(t, s)),
        job: None,
        problem: s.problem.clone(),
        operations: snapshot.process().operations().map(|op| layers::operation_row(s, op)).collect(),
        operation_error: s.operation_error,
        cut_settings: layers::cut_settings_view(s, machine.map(|m| m.profile.capabilities())),
        import: s.import.as_ref().map(|i| import_view(i, machine.map(|m| m.profile.revision().id.as_str()))),
        sketch_mode: sketch::mode_view(s, &snapshot),
        geometry_dialog: shapes::dialog_view(s),
        shape_panel: shapes::shape_panel(s),
        tool_options: shapes::option_rows(s),
        node_selection: s.editor.node_selection().to_vec(),
        image: super::image::image_view(s),
        schikken: crate::schikken::SchikkenViewModel {
            place: super::arrange::place_view(s),
            reshape: super::reshape::view(s),
            slots: super::arrange::slots_view(s),
            nest: super::nest::view(s, machine),
            tangent: super::reshape::tangent_view(s),
            print_and_cut: super::registration::print_and_cut_view(s),
            center_finder: super::registration::center_finder_view(s, machine),
        },
        text_toolbar: super::text::toolbar_view(s),
        text_on_path: super::text::path_view(s),
        notes: super::notes::view(s),
        structure: super::structure::view(s),
        centerline: super::trace::centerline_panel(s),
        trace_prep: super::trace_prep::view(s),
        trace_recipes: None,
        operator: None,
        start_confirm: None,
        remnants: crate::remnants::RemnantsViewModel::default(),
        recognition: crate::recognition::RecognitionViewModel::default(),
        offset: super::offset_preview::view(s),
        repair_preview: super::repair_preview::view(s),
        variable_panel: None,
        barcode_dialog: super::barcode::dialog_view(s),
        generator_dialog: super::generator::dialog_view(s),
        files: crate::files::FilesViewModel::default(),
    }
}

pub(super) fn command_state(key: CommandKey, s: Option<&Session>, machine: Option<&Machine>) -> CommandState {
    let has_machine = machine.is_some();
    let is_grbl = machine.is_some_and(|m| m.grbl.is_some());
    let label = match (key, s) {
        (CommandKey::Undo, Some(s)) => s.doc.undo_label().map_or(MessageKey::Command(key), MessageKey::UndoAction),
        (CommandKey::Redo, Some(s)) => s.doc.redo_label().map_or(MessageKey::Command(key), MessageKey::RedoAction),
        _ => MessageKey::Command(key),
    };
    let (enabled, checked) = match (key, s) {
        (CommandKey::New | CommandKey::NewFromTemplate | CommandKey::OpenMachineProfile, _) => (true, false),
        (_, None) => (false, false),
        (CommandKey::Undo, Some(s)) => (s.doc.can_undo(), false),
        (CommandKey::Redo, Some(s)) => (s.doc.can_redo(), false),
        (CommandKey::Delete, Some(s)) => (!s.selection.is_empty(), false),
        (CommandKey::SelectAll, Some(s)) => (!s.doc.snapshot().is_empty(), false),
        (CommandKey::ViewWireframe, Some(s)) => (true, s.arrange.wireframe),
        (CommandKey::ShowMaterialLibrary, Some(s)) => (true, s.files.material.is_some()),
        (CommandKey::ShowArtLibrary, Some(s)) => (true, s.files.art.is_some()),
        (CommandKey::SearchProjects, Some(s)) => (true, s.files.search.is_some()),
        (CommandKey::ShowNotes, Some(s)) => (true, s.notes_open),
        (CommandKey::SnapToObjects, Some(s)) => (true, s.snap.objects),
        (CommandKey::SnapToGrid, Some(s)) => (true, s.snap.grid_mm.is_some()),
        (
            CommandKey::AlignLeft
            | CommandKey::AlignRight
            | CommandKey::AlignTop
            | CommandKey::AlignBottom
            | CommandKey::AlignCenterX
            | CommandKey::AlignCenterY,
            Some(s),
        ) => (s.selection.len() >= 2, false),
        (CommandKey::DistributeHorizontal | CommandKey::DistributeVertical, Some(s)) => (s.selection.len() >= 3, false),
        (CommandKey::BooleanUnion | CommandKey::BooleanDifference | CommandKey::BooleanIntersection, Some(s)) => {
            (boolean_operands(&s.doc.snapshot(), &s.selection).is_ok(), false)
        }
        (CommandKey::ApplyPreview, Some(s)) => (current_boolean_preview(s).is_some(), false),
        (CommandKey::Compile, Some(s)) => (has_machine && !s.doc.snapshot().process().is_empty(), false),
        (CommandKey::ExportRd, Some(s)) => (has_machine && !is_grbl && s.compiled.is_some(), false),
        // Only a plan compiled for this GRBL machine (a Ruida plan carries no GRBL findings).
        (CommandKey::ExportGcode, Some(s)) => (s.compiled.as_ref().is_some_and(|c| c.grbl.is_some()), false),
        (CommandKey::ApplyTrace, Some(s)) => (s.tracer.as_ref().is_some_and(|t| t.has_result()), false),
        (CommandKey::ApplyTraceLinked, Some(s)) => (super::trace_link::apply_enabled(s), false),
        (CommandKey::TraceEdit, Some(s)) => (super::trace_link::selected_trace(s).is_some(), false),
        (CommandKey::CloseTracer, Some(s)) => (s.tracer.is_some(), false),
        (CommandKey::PrepareJob, Some(s)) => (!is_grbl && preparable(s, machine), false),
        (CommandKey::LockPathStart | CommandKey::LockPathDirection, Some(s)) => {
            (!s.selection.is_empty(), super::path_lock::checked(s, key))
        }
        (CommandKey::ApproveJob, Some(s)) => (s.prepared.as_ref().is_some_and(|p| !p.approved), false),
        (CommandKey::ToolTrim, Some(s)) => (true, s.trim_mode),
        (
            CommandKey::SketchNew
            | CommandKey::SketchFromSelection
            | CommandKey::SketchEdit
            | CommandKey::SketchFinish
            | CommandKey::SketchCoincident
            | CommandKey::SketchHorizontal
            | CommandKey::SketchVertical
            | CommandKey::SketchParallel
            | CommandKey::SketchPerpendicular
            | CommandKey::SketchEqual
            | CommandKey::SketchTangent
            | CommandKey::SketchFix
            | CommandKey::SketchDimension
            | CommandKey::SketchRemoveConstraint,
            Some(s),
        ) => (sketch::command_enabled(key, s), false),
        (
            CommandKey::ShapesJoin
            | CommandKey::ShapesCut
            | CommandKey::ShapesRoundCorners
            | CommandKey::ShapesOptimize
            | CommandKey::ShapesReverse
            | CommandKey::ShapesClockwise
            | CommandKey::ShapesCounterClockwise
            | CommandKey::ShapesConvertToPath
            | CommandKey::NodeDelete
            | CommandKey::NodeBreak
            | CommandKey::NodeSmooth
            | CommandKey::NodeSymmetric
            | CommandKey::NodeCorner
            | CommandKey::NodeToLine
            | CommandKey::NodeToCurve
            | CommandKey::NodeClosePath
            | CommandKey::NodeJoinEnds
            | CommandKey::NodeStartPoint,
            Some(s),
        ) => (shapes::command_enabled(key, s).unwrap_or(false), false),
        (CommandKey::TextOnPath, Some(s)) => (super::text::path_ready(s), false),
        (CommandKey::BarcodeNew | CommandKey::GeneratorNew, Some(_)) => (true, false),
        (CommandKey::GeneratorEdit, Some(s)) => (super::generator::edit_enabled(s), false),
        (CommandKey::CommitImport, Some(s)) => (
            s.import.as_ref().is_some_and(super::import::PendingImport::ready) || super::image::commit_enabled(s),
            false,
        ),
        (CommandKey::CancelImport, Some(s)) => (s.import.is_some() || s.image.pending.is_some(), false),
        (
            CommandKey::AdjustImage
            | CommandKey::ApplyImageAdjust
            | CommandKey::CancelImageAdjust
            | CommandKey::MaskImage
            | CommandKey::ReplaceImage
            | CommandKey::ReplaceImageToFit
            | CommandKey::RefreshImage
            | CommandKey::SaveProcessedBitmap,
            Some(s),
        ) => (super::image::command_enabled(key, s), false),
        (CommandKey::PrintAndCut | CommandKey::CenterFinder, Some(_)) => (true, false),
        (other, Some(s)) if super::nest::command_enabled(other, s).is_some() => {
            (super::nest::command_enabled(other, s).unwrap_or(false), false)
        }
        (other, Some(s)) if super::reshape::command_enabled(other, s).is_some() => {
            (super::reshape::command_enabled(other, s).unwrap_or(false), false)
        }
        (other, Some(s)) if super::arrange::command_enabled(other, s).is_some() => {
            (super::arrange::command_enabled(other, s).unwrap_or(false), false)
        }
        (other, Some(s)) => match tool_for(other) {
            Some(tool) => (true, !s.trim_mode && s.editor.tool() == tool),
            None => (true, false),
        },
    };
    CommandState { enabled, checked, label }
}
