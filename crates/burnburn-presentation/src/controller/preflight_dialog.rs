//! Laser ▸ Preflight (C09, G5-cam): the findings of the plan on screen, errors first, each with
//! the object or layer it is about. "Show source" selects the cause and centres the view on it,
//! so a geometric finding always leads to its place. The list describes the plan, not the
//! document: when an edit made the plan stale, that is the first entry.

use burnburn_cam::{Finding, Severity};
use burnburn_document::DocumentSnapshot;
use burnburn_editor::SelectionChange;
use burnburn_types::{DocumentId, GeometryId, OperationId};

use crate::cam_dialogs::{
    CamDialog, DialogAction, DialogActionKind, DialogActionState, DialogField, DialogLabel, DialogNote, DialogRow,
    DialogValue, DialogViewModel, FindingKind,
};
use crate::cut_options::FieldState;

use super::machine::Machine;
use super::{AppUpdate, Session, apply_selection};

/// The open dialog and which entry is chosen.
#[derive(Debug, Clone, Default)]
pub(super) struct PreflightState {
    open: bool,
    pick: usize,
}

/// One entry of the list.
struct Item {
    kind: FindingKind,
    /// Name of the object or layer the finding is about.
    subject: String,
    /// What "show source" selects; empty: nothing to show.
    targets: Vec<GeometryId>,
    error: bool,
}

fn geometry_name(snapshot: &DocumentSnapshot, g: GeometryId) -> String {
    snapshot.object(g).map(|o| o.name().to_owned()).unwrap_or_default()
}

fn operation_name(snapshot: &DocumentSnapshot, op: OperationId) -> String {
    snapshot.process().operation(op).map(|o| o.name().to_owned()).unwrap_or_default()
}

fn targets_of(snapshot: &DocumentSnapshot, op: OperationId) -> Vec<GeometryId> {
    snapshot.process().operation(op).map(|o| o.targets().to_vec()).unwrap_or_default()
}

fn item(snapshot: &DocumentSnapshot, f: &Finding) -> Option<Item> {
    let error = f.severity() == Severity::Error;
    let on_geometry = |kind, g: GeometryId| Item { kind, subject: geometry_name(snapshot, g), targets: vec![g], error };
    let on_operation = |kind, op: OperationId| Item {
        kind,
        subject: operation_name(snapshot, op),
        targets: targets_of(snapshot, op),
        error,
    };
    Some(match f {
        Finding::OutsideBed { source } => on_geometry(FindingKind::OutsideBed, source.geometry),
        Finding::TravelOutsideBed { operation } => match operation {
            Some(op) => on_operation(FindingKind::TravelOutsideBed, *op),
            None => Item { kind: FindingKind::TravelOutsideBed, subject: String::new(), targets: Vec::new(), error },
        },
        Finding::OpenFillContour { geometry, .. } => on_geometry(FindingKind::OpenFillContour, *geometry),
        Finding::SpeedAboveMachineMax { operation, .. } => on_operation(FindingKind::SpeedAboveMachineMax, *operation),
        Finding::PowerOutsideMachineRange { operation } => {
            on_operation(FindingKind::PowerOutsideMachineRange, *operation)
        }
        Finding::DuplicateCut { geometry, .. } => on_geometry(FindingKind::DuplicateCut, *geometry),
        Finding::EmptyOperation { operation } => on_operation(FindingKind::EmptyOperation, *operation),
        Finding::UnassignedGeometry { geometry } => on_geometry(FindingKind::UnassignedGeometry, *geometry),
        Finding::StalePlan => Item { kind: FindingKind::StalePlan, subject: String::new(), targets: Vec::new(), error },
        Finding::UnsupportedFeature { operation, .. } => on_operation(FindingKind::UnsupportedFeature, *operation),
        Finding::InvalidTabs { geometry, .. } => on_geometry(FindingKind::InvalidTabs, *geometry),
        Finding::SharedBoundaryCutOnce { operation, geometries } => Item {
            kind: FindingKind::SharedBoundaryCutOnce,
            subject: operation_name(snapshot, *operation),
            targets: geometries.clone(),
            error,
        },
        Finding::PassThroughResampled { geometry, .. } => on_geometry(FindingKind::PassThroughResampled, *geometry),
        Finding::BlockedGeometry { geometry, .. } => on_geometry(FindingKind::BlockedGeometry, *geometry),
        _ => return None,
    })
}

/// The list for the plan on screen: errors first, a stale plan before everything. `None`
/// without a plan.
fn items(s: &Session, machine: Option<&Machine>) -> Option<(Vec<Item>, bool)> {
    let compiled = s.compiled.as_ref()?;
    let snapshot = s.doc.snapshot();
    let stale = super::process::plan_view(compiled, s, machine).stale;
    let mut list: Vec<Item> = compiled.report.findings.iter().filter_map(|f| item(&snapshot, f)).collect();
    // Errors before warnings; the order inside each group is the report's.
    list.sort_by_key(|i| !i.error);
    if stale && !list.iter().any(|i| i.kind == FindingKind::StalePlan) {
        list.insert(0, Item { kind: FindingKind::StalePlan, subject: String::new(), targets: Vec::new(), error: true });
    }
    Some((list, stale))
}

fn changed() -> AppUpdate {
    AppUpdate { view_changed: true, ..AppUpdate::default() }
}

pub(super) fn view(s: &Session, machine: Option<&Machine>) -> Option<DialogViewModel> {
    if !s.cam.preflight.open {
        return None;
    }
    let found = items(s, machine);
    let list = found.as_ref().map(|(l, _)| l.as_slice()).unwrap_or_default();
    let pick = s.cam.preflight.pick.min(list.len().saturating_sub(1));
    let mut notes = Vec::new();
    match &found {
        None => notes.push(DialogNote::PreflightNoPlan),
        Some((list, stale)) => {
            if *stale {
                notes.push(DialogNote::PreflightStale);
            }
            let errors = list.iter().filter(|i| i.error && i.kind != FindingKind::StalePlan).count() as u32;
            let warnings = list.iter().filter(|i| !i.error).count() as u32;
            if errors + warnings == 0 && !*stale {
                notes.push(DialogNote::PreflightClean);
            } else {
                notes.push(DialogNote::PreflightCounts { errors, warnings });
            }
        }
    }
    let can_show = list.get(pick).is_some_and(|i| !i.targets.is_empty());
    Some(DialogViewModel {
        dialog: CamDialog::Preflight,
        rows: vec![DialogRow {
            field: DialogField::PreflightPick,
            value: DialogValue::Choice(pick),
            state: if list.is_empty() { FieldState::Disabled(None) } else { FieldState::Editable },
            error: None,
            blocked_choices: Vec::new(),
            reorder: None,
            labels: list
                .iter()
                .map(|i| DialogLabel { note: Some(DialogNote::Finding(i.kind)), text: i.subject.clone() })
                .collect(),
        }],
        notes,
        actions: vec![
            DialogActionState { action: DialogActionKind::ShowSource, enabled: can_show, reason: None },
            DialogActionState { action: DialogActionKind::Close, enabled: true, reason: None },
        ],
        capabilities: Vec::new(),
        result: None,
        busy: false,
        lines: Vec::new(),
    })
}

pub(super) fn open(s: &mut Session) -> AppUpdate {
    s.cam.preflight = PreflightState { open: true, pick: 0 };
    changed()
}

pub(super) fn set(s: &mut Session, field: DialogField, value: DialogValue) -> AppUpdate {
    if let (true, DialogField::PreflightPick, DialogValue::Choice(i)) = (s.cam.preflight.open, field, value) {
        s.cam.preflight.pick = i;
        return changed();
    }
    AppUpdate::default()
}

pub(super) fn action(
    s: &mut Session,
    document: DocumentId,
    action: DialogAction,
    machine: Option<&Machine>,
) -> AppUpdate {
    match action {
        DialogAction::Close => {
            s.cam.preflight = PreflightState::default();
            AppUpdate::redraw(document)
        }
        DialogAction::ShowSource => {
            let Some((list, _)) = items(s, machine) else { return AppUpdate::default() };
            let pick = s.cam.preflight.pick.min(list.len().saturating_sub(1));
            let Some(item) = list.get(pick).filter(|i| !i.targets.is_empty()) else { return AppUpdate::default() };
            apply_selection(s, &SelectionChange::Replace(item.targets.clone()));
            s.cycle = None;
            let snapshot = s.doc.snapshot();
            match s.selection.bounds(&snapshot) {
                Some(bounds) => super::arrange::fit_to(s, document, bounds),
                None => AppUpdate::redraw(document),
            }
        }
        _ => AppUpdate::default(),
    }
}
