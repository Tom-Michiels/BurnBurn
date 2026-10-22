//! Quick Nest and Nest Selected (W3-UI-schikken, C10): the nest panel, its cancellable task and
//! the application of a finished result. The result belongs to one revision of one set of options:
//! an edit or other options make it stale, and a stale result is never applied.

use std::collections::BTreeSet;
use std::sync::Arc;

use burnburn_document::{ArrangeCommand, CommandPayload, DocumentCommand, GeometryCommand, NestCopy, PlaceCommand};
use burnburn_editor::{Overlay, SelectionChange};
use burnburn_types::{CancellationSource, DocumentId, GeometryId, RequestId, RevisionId, StableId, TaskId};

use crate::field::{parse_length, parse_position};
use crate::intent::CommandKey;
use crate::nest_task::{NestOutput, NestSettings, build_job, classify, failure_of, sheet_of};
use crate::production::OrderId;
use crate::schikken::{
    NestFailure, NestField, NestInput, NestOrderRow, NestPanelViewModel, NestPartRow, NestReportViewModel, NestRun,
    NestScope, SchikkenIntent, SchikkenMessage,
};
use crate::task::{TaskError, TaskInput, TaskKind, TaskOutput, TaskRequest, revision_key};
use crate::view_model::MessageKey;

use super::machine::Machine;
use super::{AppController, AppUpdate, Session, after_change, apply_command, apply_selection, error, submit};

/// Most sheets one nest may use.
const MAX_SHEETS: u32 = 20;
/// Most copies of one part.
const MAX_COPIES: u32 = 1000;

/// The panel while it is open.
#[derive(Debug, Clone)]
struct Panel {
    scope: NestScope,
    /// What the fields show (the last text accepted, or refused text is not kept).
    input: NestInput,
    /// The options in force: the parsed `input`.
    settings: NestSettings,
    /// The task whose answer is awaited; an older one's answer is ignored.
    task: Option<TaskId>,
    error: Option<SchikkenMessage>,
}

/// The finished nest: for which revision it was made, and whether it has been applied.
#[derive(Debug, Clone)]
struct Done {
    revision: RevisionId,
    output: Arc<NestOutput>,
    applied: bool,
}

#[derive(Debug, Clone, Default)]
pub(super) struct NestState {
    panel: Option<Panel>,
    done: Option<Done>,
}

fn nest_pending(s: &Session) -> bool {
    s.pending.values().any(|(kind, _)| *kind == TaskKind::Nest)
}

pub(super) fn cancel_running(s: &mut Session) {
    for (kind, source) in s.pending.values() {
        if *kind == TaskKind::Nest {
            source.cancel();
        }
    }
}

/// Parses the fields of the panel; the first invalid one is named.
fn parse(input: &NestInput) -> Result<NestSettings, NestField> {
    let non_negative = |text: &str, field: NestField| match parse_position(text) {
        Ok(v) if v.mm() >= 0.0 => Ok(v.mm()),
        _ => Err(field),
    };
    let copies =
        |text: &str| text.trim().parse::<u32>().ok().filter(|n| (1..=MAX_COPIES).contains(n)).ok_or(NestField::Copies);
    let part_copies = input
        .part_copies
        .iter()
        .map(|(object, text)| Ok((GeometryId::from_raw(*object), copies(text)?)))
        .collect::<Result<Vec<_>, NestField>>()?;
    Ok(NestSettings {
        part_grain: input.part_grain.iter().map(|(object, rule)| (GeometryId::from_raw(*object), *rule)).collect(),
        copies: copies(&input.copies)?,
        part_copies,
        holes: input.holes,
        tight: input.tight,
        common_line: input.common_line,
        grain_deg: if input.grain.trim().is_empty() {
            None
        } else {
            Some(crate::field::parse_angle(&input.grain).map_err(|_| NestField::Grain)?)
        },
        padding_mm: non_negative(&input.padding, NestField::Padding)?,
        edge_buffer_mm: non_negative(&input.edge_buffer, NestField::EdgeBuffer)?,
        step_mm: parse_length(&input.step).map(|l| l.mm()).map_err(|_| NestField::Step)?,
        rotations: input.rotations,
        mirror: input.mirror,
        sheet_object: input.sheet_object.map(GeometryId::from_raw),
        sheets: input
            .sheets
            .trim()
            .parse::<u32>()
            .ok()
            .filter(|n| (1..=MAX_SHEETS).contains(n))
            .ok_or(NestField::Sheets)?,
    })
}

/// Does a member of the part carry the name an order asks for (compared without case and spaces)?
fn body_is_part(snapshot: &burnburn_document::DocumentSnapshot, body: &[GeometryId], wanted: &str) -> bool {
    let wanted = wanted.trim().to_lowercase();
    !wanted.is_empty()
        && body.iter().filter_map(|id| snapshot.object(*id)).any(|o| o.name().trim().to_lowercase() == wanted)
}

fn failure(f: NestFailure) -> Option<SchikkenMessage> {
    Some(SchikkenMessage::Nest(f))
}

/// The rules of the nest of "distribute and nest" (G9-editor): the defaults of the panel, with
/// `gap_mm` between the parts and no margin at the edge of the plate.
pub(super) fn plate_settings(gap_mm: f64) -> Option<NestSettings> {
    let mut settings = parse(&NestInput::default()).ok()?;
    settings.padding_mm = gap_mm;
    settings.edge_buffer_mm = 0.0;
    Some(settings)
}

/// The state of the run: derived, so a task that was dropped as outdated can never leave it stuck.
fn run_state(s: &Session) -> NestRun {
    if nest_pending(s) {
        return NestRun::Running;
    }
    match &s.nest.done {
        Some(d) if d.applied => NestRun::Applied,
        Some(d) if d.revision == s.doc.revision() => NestRun::Done,
        Some(_) => NestRun::Stale,
        None => NestRun::Idle,
    }
}

impl AppController {
    pub(super) fn nest_intent(&mut self, intent: SchikkenIntent) -> AppUpdate {
        match intent {
            SchikkenIntent::SetNestOptions { document, input } => {
                self.with_session(document, |s| set_options(s, document, *input))
            }
            SchikkenIntent::StartNest { document } => self.start_nest(document),
            SchikkenIntent::TakeOrderQuantities { document, order } => self.take_order_quantities(document, order),
            SchikkenIntent::CancelNest { document } => self.with_session(document, |s| {
                cancel_running(s);
                AppUpdate::redraw(document)
            }),
            SchikkenIntent::ApplyNest { document } => {
                let request = self.next_id::<RequestId>();
                self.with_session(document, |s| apply(s, document, request))
            }
            SchikkenIntent::SelectNestUnplaced { document } => self.with_session(document, |s| {
                let ids: Vec<_> = s
                    .nest
                    .done
                    .as_ref()
                    .map(|d| d.output.unplaced.iter().flat_map(|u| u.objects.iter().copied()).collect())
                    .unwrap_or_default();
                if ids.is_empty() {
                    return AppUpdate::default();
                }
                apply_selection(s, &SelectionChange::Replace(ids));
                AppUpdate::redraw(document)
            }),
            SchikkenIntent::CloseNest { document } => self.with_session(document, |s| {
                cancel_running(s);
                s.nest = NestState::default();
                AppUpdate::redraw(document)
            }),
            _ => AppUpdate::default(),
        }
    }

    /// The pieces of an order still to make: its quantity less the executions with Completed evidence.
    fn order_remaining(&self, order: &super::Order) -> u32 {
        let status = self.machine_port.as_ref().map(|p| p.status());
        let made = self.order_view(order, status.as_ref()).completed;
        order.draft.quantity.saturating_sub(made)
    }

    /// The orders the panel of `s` can take the quantity of, with what is left of each.
    pub(super) fn nest_order_rows(&self, s: &Session) -> Vec<NestOrderRow> {
        let Some(panel) = s.nest.panel.as_ref() else { return Vec::new() };
        let snapshot = s.doc.snapshot();
        let bodies = classify(&snapshot, panel.scope, s.selection.ids(), panel.settings.holes).movable;
        self.orders
            .iter()
            .map(|o| NestOrderRow {
                id: o.id,
                name: o.draft.name.clone(),
                part: o.draft.part.clone(),
                remaining: self.order_remaining(o),
                matching_parts: bodies.iter().filter(|b| body_is_part(&snapshot, b, &o.draft.part)).count(),
            })
            .collect()
    }

    /// The pieces still to make of an order become the number of copies of the parts of the nest that
    /// carry the order's part name; the other parts keep theirs (W07).
    fn take_order_quantities(&mut self, document: DocumentId, order: OrderId) -> AppUpdate {
        let found = self.orders.iter().find(|o| o.id == order).cloned();
        let remaining = found.as_ref().map(|o| self.order_remaining(o));
        self.with_session(document, |s| {
            let Some(panel) = s.nest.panel.as_mut() else { return AppUpdate::default() };
            let (Some(order), Some(remaining)) = (found, remaining) else {
                panel.error = failure(NestFailure::UnknownOrder);
                return AppUpdate::redraw(document);
            };
            let snapshot = s.doc.snapshot();
            let bodies = classify(&snapshot, panel.scope, s.selection.ids(), panel.settings.holes).movable;
            let firsts: Vec<GeometryId> = bodies
                .iter()
                .filter(|b| body_is_part(&snapshot, b, &order.draft.part))
                .filter_map(|b| b.first().copied())
                .collect();
            if firsts.is_empty() {
                panel.error = failure(NestFailure::OrderMatchesNothing);
                return AppUpdate::redraw(document);
            }
            if remaining == 0 {
                panel.error = failure(NestFailure::OrderDone);
                return AppUpdate::redraw(document);
            }
            let text = remaining.min(MAX_COPIES).to_string();
            let mut input = panel.input.clone();
            for first in firsts {
                input.part_copies.retain(|(o, _)| *o != first.raw());
                // The number every part has: the part just follows the panel.
                if text != input.copies.trim() {
                    input.part_copies.push((first.raw(), text.clone()));
                }
            }
            set_options(s, document, input)
        })
    }

    /// Builds the job on the current revision and hands it to the worker.
    fn start_nest(&mut self, document: DocumentId) -> AppUpdate {
        let bed = self.machine.as_ref().map(|m| (m.profile.frame().width_mm(), m.profile.frame().height_mm()));
        let Some(id) = self.next_id::<TaskId>() else { return error(Some(document), MessageKey::CommandRejected) };
        let tasks = Arc::clone(&self.ports.tasks);
        self.with_session(document, |s| {
            let Some(panel) = s.nest.panel.as_ref() else { return AppUpdate::default() };
            // A field the panel shows as wrong is fixed first: nothing starts on options nobody sees.
            if matches!(panel.error, Some(SchikkenMessage::Nest(NestFailure::InvalidField(_)))) {
                return AppUpdate::redraw(document);
            }
            let (scope, settings) = (panel.scope, panel.settings.clone());
            let snapshot = s.doc.snapshot();
            let job = match build_job(&snapshot, scope, s.selection.ids(), &settings, bed) {
                Ok(job) => job,
                Err(e) => {
                    if let Some(p) = s.nest.panel.as_mut() {
                        p.error = failure(e.into());
                    }
                    return AppUpdate::redraw(document);
                }
            };
            cancel_running(s);
            s.nest.done = None;
            if let Some(p) = s.nest.panel.as_mut() {
                p.error = None;
                p.task = Some(id);
            }
            let source = snapshot.revision();
            let cancel = CancellationSource::new();
            let request = TaskRequest {
                id,
                document,
                source,
                key: revision_key(TaskKind::Nest, document, source),
                cancel: cancel.token(),
                input: TaskInput::Nest(Box::new(job)),
            };
            submit(s, &tasks, request, cancel, document)
        })
    }
}

fn set_options(s: &mut Session, document: DocumentId, input: NestInput) -> AppUpdate {
    let Some(panel) = s.nest.panel.as_mut() else { return AppUpdate::default() };
    match parse(&input) {
        Ok(settings) => {
            let changed = panel.input != input;
            panel.input = input;
            panel.settings = settings;
            panel.error = None;
            if changed {
                // A result belongs to the options it was made with.
                s.nest.done = None;
                cancel_running(s);
            }
        }
        Err(field) => panel.error = failure(NestFailure::InvalidField(field)),
    }
    AppUpdate::redraw(document)
}

fn apply(s: &mut Session, document: DocumentId, request: Option<RequestId>) -> AppUpdate {
    let Some(request) = request else { return error(Some(document), MessageKey::CommandRejected) };
    let fresh = run_state(s) == NestRun::Done;
    let Some(done) = s.nest.done.clone().filter(|_| fresh) else {
        if run_state(s) == NestRun::Stale
            && let Some(p) = s.nest.panel.as_mut()
        {
            p.error = failure(NestFailure::Stale);
        }
        return AppUpdate::redraw(document);
    };
    let moves: Vec<_> = done
        .output
        .placements
        .iter()
        .filter(|p| p.copy == 0)
        .flat_map(|p| p.objects.iter().map(|id| (*id, p.transform)))
        .collect();
    // Copy 1 and up are new objects: made from the part as it is now, placed where the nest says.
    let extra: Vec<&crate::nest_task::PartPlacement> = done.output.placements.iter().filter(|p| p.copy > 0).collect();
    if moves.is_empty() && extra.is_empty() {
        return AppUpdate::default();
    }
    let snapshot = s.doc.snapshot();
    if moves.iter().any(|(id, _)| snapshot.is_locked(*id)) {
        return error(Some(document), MessageKey::ObjectsLocked);
    }
    let payload = if extra.is_empty() {
        CommandPayload::Edit(GeometryCommand::Place(PlaceCommand::Nest { moves }))
    } else {
        // Names already in the drawing, and those given to copies on the way.
        let mut taken: BTreeSet<String> = snapshot
            .draw_order()
            .into_iter()
            .filter_map(|id| snapshot.object(id))
            .map(|o| o.name().to_owned())
            .collect();
        let copies =
            extra.iter().map(|p| NestCopy { items: named_copy(s, &mut taken, p), transform: p.transform }).collect();
        CommandPayload::Arrange(ArrangeCommand::NestCopies { moves, copies })
    };
    let cmd = DocumentCommand { document, expected: s.doc.revision(), request, payload };
    match apply_command(&s.fonts, &mut s.doc, cmd) {
        Ok(outcome) => {
            if let Some(d) = s.nest.done.as_mut() {
                d.applied = true;
            }
            // The selection stays what it was; the moved parts are on the sheet now.
            let keep = s.selection.clone();
            let update = after_change(s, document, Ok(outcome));
            s.selection = keep;
            s.selection.retain_existing(&s.doc.snapshot());
            update
        }
        Err(_) => {
            if let Some(p) = s.nest.panel.as_mut() {
                p.error = failure(NestFailure::Refused);
            }
            AppUpdate::redraw(document)
        }
    }
}

pub(super) fn command(
    s: &mut Session,
    document: DocumentId,
    command: CommandKey,
    _request: Option<RequestId>,
) -> Option<AppUpdate> {
    let scope = match command {
        CommandKey::QuickNest => NestScope::Quick,
        CommandKey::NestSelected => NestScope::Selected,
        _ => return None,
    };
    if command_enabled(command, s) != Some(true) {
        return Some(AppUpdate::default());
    }
    let (input, settings) = match s.nest.panel.take() {
        Some(p) => (p.input, p.settings),
        None => {
            let input = NestInput::default();
            // The defaults are valid by construction.
            let settings = parse(&input).ok()?;
            (input, settings)
        }
    };
    cancel_running(s);
    s.nest = NestState { panel: Some(Panel { scope, input, settings, task: None, error: None }), done: None };
    Some(AppUpdate::redraw(document))
}

pub(super) fn command_enabled(command: CommandKey, s: &Session) -> Option<bool> {
    match command {
        CommandKey::QuickNest => Some(!s.doc.snapshot().is_empty()),
        CommandKey::NestSelected => Some(!s.selection.is_empty()),
        _ => None,
    }
}

/// Delivery of the worker's answer. An answer of an older task, or after the panel was closed, is
/// dropped.
pub(super) fn finished(
    s: &mut Session,
    document: DocumentId,
    task: TaskId,
    source: RevisionId,
    result: Result<TaskOutput, TaskError>,
    update: &mut AppUpdate,
) {
    update.scenes_changed.push(document);
    if super::plate_nest::owns(s, task) {
        return super::plate_nest::finished(s, document, result, update);
    }
    let Some(panel) = s.nest.panel.as_mut().filter(|p| p.task == Some(task)) else { return };
    panel.task = None;
    match result {
        Ok(TaskOutput::Nested(output)) => {
            s.nest.done = Some(Done { revision: source, output, applied: false });
        }
        Err(TaskError::Nest(e)) => panel.error = failure(failure_of(&e)),
        _ => {}
    }
}

pub(super) fn view(s: &Session, machine: Option<&Machine>) -> Option<NestPanelViewModel> {
    let panel = s.nest.panel.as_ref()?;
    let snapshot = s.doc.snapshot();
    let mut classified = classify(&snapshot, panel.scope, s.selection.ids(), panel.settings.holes);
    // A chosen sheet is not one of the parts (when it cannot be, Start says why).
    if let Some(sheet) = panel.settings.sheet_object {
        classified.movable.retain(|body| !body.contains(&sheet) || panel.scope == NestScope::Selected);
    }
    let bed = machine.map(|m| (m.profile.frame().width_mm(), m.profile.frame().height_mm()));
    let run = run_state(s);
    let report = s.nest.done.as_ref().map(|d| NestReportViewModel {
        placed: d.output.placements.len(),
        unplaced: d.output.unplaced.clone(),
        skipped: d.output.skipped.clone(),
        utilisation: d.output.utilisation,
        mirrored: d.output.mirrored,
        sheets: d.output.sheets.clone(),
        parts: d.output.parts.clone(),
        complete_sets: d.output.complete_sets,
        shared_mm: d.output.shared_mm,
    });
    Some(NestPanelViewModel {
        scope: panel.scope,
        input: panel.input.clone(),
        error: panel.error,
        run,
        parts: classified.movable.len(),
        sheet: sheet_of(&snapshot, &classified.movable, &panel.settings, bed),
        sheet_choices: crate::nest_task::sheet_choices(&snapshot),
        part_rows: classified
            .movable
            .iter()
            .filter_map(|body| {
                let first = *body.first()?;
                let own = panel.input.part_copies.iter().find(|(id, _)| *id == first.raw()).map(|(_, t)| t.clone());
                Some(NestPartRow {
                    object: first.raw(),
                    name: snapshot.object(first).map(|o| o.name().to_owned()).unwrap_or_default(),
                    copies: own.clone().unwrap_or_else(|| panel.input.copies.clone()),
                    own: own.is_some(),
                    grain: panel.settings.grain_of(first),
                })
            })
            .collect(),
        orders: Vec::new(),
        report,
        can_apply: run == NestRun::Done && s.nest.done.as_ref().is_some_and(|d| !d.output.placements.is_empty()),
    })
}

/// The objects of an extra copy, each named after its source with the number of the copy
/// ("Pad 1 (2)"), and a name that is taken is skipped, so no two objects share a name.
fn named_copy(
    s: &Session,
    taken: &mut BTreeSet<String>,
    placement: &crate::nest_task::PartPlacement,
) -> Vec<burnburn_document::PasteItem> {
    let mut items = super::clipboard::items_of(s, &placement.objects);
    for item in &mut items {
        let base = item.name.clone();
        let mut number = placement.copy + 1;
        let name = loop {
            let candidate = format!("{base} ({number})");
            if !taken.contains(&candidate) {
                break candidate;
            }
            number += 1;
        };
        taken.insert(name.clone());
        item.name = name;
    }
    items
}

/// The finished nest drawn on the canvas before it is applied: every part where it would go.
pub(super) fn scene_overlays(s: &Session) -> Vec<Overlay> {
    if run_state(s) != NestRun::Done {
        return Vec::new();
    }
    let snapshot = s.doc.snapshot();
    s.nest
        .done
        .iter()
        .flat_map(|d| d.output.placements.iter())
        .flat_map(|p| {
            let snapshot = &snapshot;
            p.objects.iter().filter_map(move |id| {
                snapshot
                    .object(*id)
                    .map(|o| Overlay::DragPreview { path: Arc::clone(o.path()), transform: p.transform })
            })
        })
        .collect()
}
