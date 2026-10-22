//! Edit ▸ Cutting order (C06, G5-cam): hard "this layer completely before that one" constraints.
//! They beat the list order and the travel optimisation in the compiled plan. A constraint that
//! would close a cycle is refused before anything is sent, with the layers on the cycle named
//! under the form; each add or remove is one document command and one undo step.

use burnburn_document::CommandPayload;
use burnburn_document::process::{OrderConstraint, ProcessCommand};
use burnburn_types::{DocumentId, OperationId, RequestId};

use crate::cam_dialogs::{
    CamDialog, DialogAction, DialogActionKind, DialogActionState, DialogField, DialogLabel, DialogNote, DialogRow,
    DialogValue, DialogViewModel,
};
use crate::cut_options::FieldState;

use super::{AppUpdate, Session, apply_edit};

/// The open dialog: the chosen entries and why the last Add did nothing.
#[derive(Debug, Clone, Default)]
pub(super) struct OrderState {
    open: bool,
    before: usize,
    after: usize,
    existing: usize,
    refused: Option<(DialogNote, Vec<OperationId>)>,
}

fn changed() -> AppUpdate {
    AppUpdate { view_changed: true, ..AppUpdate::default() }
}

/// The layers in list order and the stored constraints.
fn layers(s: &Session) -> (Vec<(OperationId, String)>, Vec<OrderConstraint>) {
    let snapshot = s.doc.snapshot();
    let ops = snapshot.process().operations().map(|o| (o.id(), o.name().to_owned())).collect();
    (ops, snapshot.process().constraints().to_vec())
}

fn name_of(ops: &[(OperationId, String)], id: OperationId) -> String {
    ops.iter().find(|(o, _)| *o == id).map(|(_, n)| n.clone()).unwrap_or_default()
}

/// A path of constraints from `from` to `to`, if there is one (`from` first, `to` last).
fn path_between(constraints: &[OrderConstraint], from: OperationId, to: OperationId) -> Option<Vec<OperationId>> {
    let mut stack = vec![vec![from]];
    let mut seen = vec![from];
    while let Some(path) = stack.pop() {
        let Some(&last) = path.last() else { continue };
        if last == to {
            return Some(path);
        }
        for c in constraints.iter().filter(|c| c.before == last) {
            if !seen.contains(&c.after) {
                seen.push(c.after);
                let mut next = path.clone();
                next.push(c.after);
                stack.push(next);
            }
        }
    }
    None
}

pub(super) fn view(s: &Session) -> Option<DialogViewModel> {
    let st = &s.cam.order;
    if !st.open {
        return None;
    }
    let (ops, constraints) = layers(s);
    let (before, after) = (st.before.min(ops.len().saturating_sub(1)), st.after.min(ops.len().saturating_sub(1)));
    let existing = st.existing.min(constraints.len().saturating_sub(1));
    let names: Vec<DialogLabel> = ops.iter().map(|(_, n)| DialogLabel::plain(n.clone())).collect();
    let stored: Vec<DialogLabel> = constraints
        .iter()
        .map(|c| DialogLabel::plain(format!("{} \u{2192} {}", name_of(&ops, c.before), name_of(&ops, c.after))))
        .collect();
    let pop = |field, value: usize, labels: Vec<DialogLabel>| DialogRow {
        field,
        value: DialogValue::Choice(value),
        state: if labels.is_empty() { FieldState::Disabled(None) } else { FieldState::Editable },
        error: None,
        blocked_choices: Vec::new(),
        reorder: None,
        labels,
    };
    let mut notes = Vec::new();
    let same = ops.len() >= 2 && before == after;
    if same {
        notes.push(DialogNote::OrderSameLayer);
    }
    let mut lines = Vec::new();
    if let Some((note, on_cycle)) = &st.refused {
        notes.push(*note);
        lines = on_cycle.iter().map(|id| name_of(&ops, *id)).collect();
    }
    Some(DialogViewModel {
        dialog: CamDialog::Order,
        rows: vec![
            pop(DialogField::OrderBefore, before, names.clone()),
            pop(DialogField::OrderAfter, after, names),
            pop(DialogField::OrderExisting, existing, stored),
        ],
        notes,
        actions: vec![
            DialogActionState { action: DialogActionKind::AddRow, enabled: ops.len() >= 2 && !same, reason: None },
            DialogActionState { action: DialogActionKind::RemoveRow, enabled: !constraints.is_empty(), reason: None },
            DialogActionState { action: DialogActionKind::Close, enabled: true, reason: None },
        ],
        capabilities: Vec::new(),
        result: None,
        busy: false,
        lines,
    })
}

pub(super) fn open(s: &mut Session) -> AppUpdate {
    s.cam.order = OrderState { open: true, after: 1, ..OrderState::default() };
    changed()
}

pub(super) fn set(s: &mut Session, field: DialogField, value: DialogValue) -> AppUpdate {
    let st = &mut s.cam.order;
    let DialogValue::Choice(i) = value else { return AppUpdate::default() };
    if !st.open {
        return AppUpdate::default();
    }
    match field {
        DialogField::OrderBefore => st.before = i,
        DialogField::OrderAfter => st.after = i,
        DialogField::OrderExisting => st.existing = i,
        _ => return AppUpdate::default(),
    }
    st.refused = None;
    changed()
}

pub(super) fn action(
    s: &mut Session,
    document: DocumentId,
    action: DialogAction,
    request: Option<RequestId>,
) -> AppUpdate {
    let (ops, constraints) = layers(s);
    match action {
        DialogAction::Close => {
            s.cam.order = OrderState::default();
            AppUpdate::redraw(document)
        }
        DialogAction::AddRow => {
            let st = &s.cam.order;
            let (Some((b, _)), Some((a, _))) = (ops.get(st.before), ops.get(st.after)) else { return changed() };
            let c = OrderConstraint { before: *b, after: *a };
            if c.before == c.after {
                return changed();
            }
            if constraints.contains(&c) {
                s.cam.order.refused = Some((DialogNote::OrderDuplicate, Vec::new()));
                return changed();
            }
            // `after` already has to come before `before` somewhere: the new one closes the loop.
            if let Some(path) = path_between(&constraints, c.after, c.before) {
                s.cam.order.refused = Some((DialogNote::OrderCycle, path));
                return changed();
            }
            s.cam.order.refused = None;
            apply_edit(
                s,
                document,
                CommandPayload::Process(ProcessCommand::AddOrderConstraint { before: c.before, after: c.after }),
                request,
            )
        }
        DialogAction::RemoveRow => {
            let Some(c) = constraints.get(s.cam.order.existing.min(constraints.len().saturating_sub(1))) else {
                return changed();
            };
            s.cam.order.refused = None;
            apply_edit(
                s,
                document,
                CommandPayload::Process(ProcessCommand::RemoveOrderConstraint { before: c.before, after: c.after }),
                request,
            )
        }
        _ => AppUpdate::default(),
    }
}
