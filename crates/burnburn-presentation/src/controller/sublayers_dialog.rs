//! Edit ▸ Sub-layers (C11, LY01, G5-cam): the sub-layers of a layer run right after it, in list
//! order, each with its own mode and settings. The dialog adds a sub-layer (a copy of its layer),
//! edits the chosen one and moves it earlier or later. There is no draft: every valid change is
//! one `SetAdvanced` (one undo step); an invalid one stays in its field with the reason and sends
//! nothing. Tool layers have no sub-layers and are not listed.

use std::collections::BTreeMap;

use burnburn_document::CommandPayload;
use burnburn_document::process::{
    AdvancedOptions, FillSettings, Kerf, OperationMode, Passes, PowerPercent, PowerRange, ProcessCommand,
    ProcessSettings, Speed, SubOperation,
};
use burnburn_types::{DocumentId, Length, OperationId, RequestId};

use crate::cam_dialogs::{
    CamDialog, DialogAction, DialogActionKind, DialogActionState, DialogChoice, DialogError, DialogField, DialogLabel,
    DialogNote, DialogRow, DialogValue, DialogViewModel,
};
use crate::cut_options::FieldState;
use crate::field::{format_mm, parse_number};

use super::{AppUpdate, Session, apply_edit};

/// The open dialog: which layer and sub-layer are chosen, and the text of fields that were
/// refused (so the user sees what they typed next to the reason).
#[derive(Debug, Clone, Default)]
pub(super) struct SublayersState {
    open: bool,
    layer: usize,
    index: usize,
    refused: BTreeMap<DialogField, (String, DialogError)>,
}

fn changed() -> AppUpdate {
    AppUpdate { view_changed: true, ..AppUpdate::default() }
}

/// The layers that are cut, in list order.
fn layers(s: &Session) -> Vec<(OperationId, String, AdvancedOptions)> {
    s.doc
        .snapshot()
        .process()
        .operations()
        .filter(|o| o.tool().is_none())
        .map(|o| (o.id(), o.name().to_owned(), o.advanced().clone()))
        .collect()
}

/// The chosen layer and sub-layer, clamped to what exists.
fn chosen(s: &Session) -> Option<(OperationId, AdvancedOptions, usize)> {
    let list = layers(s);
    let st = &s.cam.sublayers;
    let (id, _, advanced) = list.get(st.layer.min(list.len().saturating_sub(1)))?.clone();
    let index = st.index.min(advanced.sub_operations.len().saturating_sub(1));
    Some((id, advanced, index))
}

fn mode_choice(mode: OperationMode) -> DialogChoice {
    if matches!(mode, OperationMode::Fill(_)) { DialogChoice::SubFill } else { DialogChoice::SubLine }
}

pub(super) fn view(s: &Session) -> Option<DialogViewModel> {
    let st = &s.cam.sublayers;
    if !st.open {
        return None;
    }
    let list = layers(s);
    let layer_labels: Vec<DialogLabel> = list.iter().map(|(_, n, _)| DialogLabel::plain(n.clone())).collect();
    let layer_index = st.layer.min(list.len().saturating_sub(1));
    let current = chosen(s);
    let subs: Vec<SubOperation> = current.as_ref().map(|(_, a, _)| a.sub_operations.clone()).unwrap_or_default();
    let index = current.as_ref().map_or(0, |(_, _, i)| *i);
    let sub = subs.get(index).copied();

    let hidden = sub.is_none();
    let state = |extra_hidden: bool| if hidden || extra_hidden { FieldState::Hidden } else { FieldState::Editable };
    let text_row = |field: DialogField, shown: String, extra_hidden: bool| {
        let refused = st.refused.get(&field);
        DialogRow {
            field,
            value: DialogValue::Text(refused.map_or(shown, |(t, _)| t.clone())),
            state: state(extra_hidden),
            error: refused.map(|(_, e)| *e),
            blocked_choices: Vec::new(),
            reorder: None,
            labels: Vec::new(),
        }
    };
    let choice_row = |field: DialogField, value: usize, labels: Vec<DialogLabel>, hide: bool| DialogRow {
        field,
        value: DialogValue::Choice(value),
        state: if hide {
            FieldState::Hidden
        } else if labels.is_empty() && field != DialogField::SubMode {
            FieldState::Disabled(None)
        } else {
            FieldState::Editable
        },
        error: None,
        blocked_choices: Vec::new(),
        reorder: None,
        labels,
    };
    let (speed, power, passes, interval, mode_index) = match sub {
        Some(sub) => {
            let fill_interval = match sub.mode {
                OperationMode::Fill(f) => format_mm(f.interval().mm()),
                _ => String::new(),
            };
            let mode_index =
                DialogField::SubMode.choices().iter().position(|c| *c == mode_choice(sub.mode)).unwrap_or(0);
            (
                format_mm(sub.settings.speed.get_mm_per_s()),
                format_mm(sub.settings.power.max().get()),
                sub.settings.passes.get().to_string(),
                fill_interval,
                mode_index,
            )
        }
        None => (String::new(), String::new(), String::new(), String::new(), 0),
    };
    let is_line = sub.is_some_and(|s| !matches!(s.mode, OperationMode::Fill(_)));
    let rows = vec![
        choice_row(DialogField::SubLayer, layer_index, layer_labels, false),
        choice_row(
            DialogField::SubIndex,
            index,
            (1..=subs.len()).map(|n| DialogLabel::plain(n.to_string())).collect(),
            false,
        ),
        choice_row(DialogField::SubMode, mode_index, Vec::new(), hidden),
        text_row(DialogField::SubSpeed, speed, false),
        text_row(DialogField::SubPower, power, false),
        text_row(DialogField::SubPasses, passes, false),
        text_row(DialogField::SubInterval, interval, is_line),
    ];
    let mut notes = Vec::new();
    if subs.is_empty() {
        notes.push(DialogNote::SublayersNone);
    }
    let full = subs.len() >= AdvancedOptions::MAX_SUB_OPERATIONS;
    if full {
        notes.push(DialogNote::SublayersFull);
    }
    let has_layer = current.is_some();
    Some(DialogViewModel {
        dialog: CamDialog::Sublayers,
        rows,
        notes,
        actions: vec![
            DialogActionState { action: DialogActionKind::AddRow, enabled: has_layer && !full, reason: None },
            DialogActionState { action: DialogActionKind::RemoveRow, enabled: sub.is_some(), reason: None },
            DialogActionState {
                action: DialogActionKind::MoveRowUp,
                enabled: sub.is_some() && index > 0,
                reason: None,
            },
            DialogActionState {
                action: DialogActionKind::MoveRowDown,
                enabled: sub.is_some() && index + 1 < subs.len(),
                reason: None,
            },
            DialogActionState { action: DialogActionKind::Close, enabled: true, reason: None },
        ],
        capabilities: Vec::new(),
        result: None,
        busy: false,
        lines: Vec::new(),
    })
}

pub(super) fn open(s: &mut Session) -> AppUpdate {
    s.cam.sublayers = SublayersState { open: true, ..SublayersState::default() };
    changed()
}

/// Sends `SetAdvanced` with the sub-layers rewritten by `edit`.
fn rewrite(
    s: &mut Session,
    document: DocumentId,
    request: Option<RequestId>,
    edit: impl FnOnce(&mut Vec<SubOperation>, usize) -> Option<usize>,
) -> AppUpdate {
    let Some((operation, mut advanced, index)) = chosen(s) else { return changed() };
    let Some(new_index) = edit(&mut advanced.sub_operations, index) else { return changed() };
    s.cam.sublayers.index = new_index;
    s.cam.sublayers.refused.clear();
    apply_edit(s, document, CommandPayload::Process(ProcessCommand::SetAdvanced { operation, advanced }), request)
}

pub(super) fn set(
    s: &mut Session,
    document: DocumentId,
    field: DialogField,
    value: DialogValue,
    request: Option<RequestId>,
) -> AppUpdate {
    if !s.cam.sublayers.open {
        return AppUpdate::default();
    }
    match (field, value) {
        (DialogField::SubLayer, DialogValue::Choice(i)) => {
            s.cam.sublayers.layer = i;
            s.cam.sublayers.index = 0;
            s.cam.sublayers.refused.clear();
            changed()
        }
        (DialogField::SubIndex, DialogValue::Choice(i)) => {
            s.cam.sublayers.index = i;
            s.cam.sublayers.refused.clear();
            changed()
        }
        (DialogField::SubMode, DialogValue::Choice(i)) => {
            let to_fill = DialogField::SubMode.choices().get(i) == Some(&DialogChoice::SubFill);
            set_mode(s, document, to_fill, request)
        }
        (
            DialogField::SubSpeed | DialogField::SubPower | DialogField::SubPasses | DialogField::SubInterval,
            DialogValue::Text(t),
        ) => set_number(s, document, field, t, request),
        _ => AppUpdate::default(),
    }
}

fn set_mode(s: &mut Session, document: DocumentId, to_fill: bool, request: Option<RequestId>) -> AppUpdate {
    rewrite(s, document, request, |subs, i| {
        let sub = subs.get_mut(i)?;
        let is_fill = matches!(sub.mode, OperationMode::Fill(_));
        if is_fill == to_fill {
            return None;
        }
        sub.mode = if to_fill {
            OperationMode::Fill(super::layers::default_fill()?)
        } else {
            OperationMode::Line { kerf: Kerf::NONE }
        };
        Some(i)
    })
}

fn set_number(
    s: &mut Session,
    document: DocumentId,
    field: DialogField,
    text: String,
    request: Option<RequestId>,
) -> AppUpdate {
    let parsed = parse(field, &text);
    let value = match parsed {
        Ok(v) => v,
        Err(why) => {
            s.cam.sublayers.refused.insert(field, (text, why));
            return changed();
        }
    };
    rewrite(s, document, request, |subs, i| {
        let sub = subs.get_mut(i)?;
        let before = *sub;
        let settings = sub.settings;
        match (field, value) {
            (DialogField::SubSpeed, Number::Speed(v)) => sub.settings = ProcessSettings { speed: v, ..settings },
            (DialogField::SubPower, Number::Power(p)) => {
                sub.settings = ProcessSettings { power: PowerRange::new(p, p).ok()?, ..settings }
            }
            (DialogField::SubPasses, Number::Passes(n)) => sub.settings = ProcessSettings { passes: n, ..settings },
            (DialogField::SubInterval, Number::Interval(l)) => {
                let OperationMode::Fill(f) = sub.mode else { return None };
                sub.mode = OperationMode::Fill(with_interval(&f, l)?);
            }
            _ => return None,
        }
        (*sub != before).then_some(i)
    })
}

enum Number {
    Speed(Speed),
    Power(PowerPercent),
    Passes(Passes),
    Interval(Length),
}

fn parse(field: DialogField, text: &str) -> Result<Number, DialogError> {
    let num = |suffixes: &[&str]| match parse_number(text, suffixes) {
        Ok(v) if v.is_finite() => Ok(v),
        _ => Err(DialogError::Invalid),
    };
    match field {
        DialogField::SubSpeed => {
            Speed::mm_per_s(num(&["mm/s"])?).map(Number::Speed).map_err(|_| DialogError::OutOfRange)
        }
        DialogField::SubPower => {
            PowerPercent::new(num(&["%"])?).map(Number::Power).map_err(|_| DialogError::OutOfRange)
        }
        DialogField::SubPasses => {
            let v = num(&[])?;
            if v.fract() != 0.0 {
                return Err(DialogError::NotWhole);
            }
            if !(1.0..=f64::from(Passes::MAX)).contains(&v) {
                return Err(DialogError::OutOfRange);
            }
            Passes::new(v as u32).map(Number::Passes).map_err(|_| DialogError::OutOfRange)
        }
        _ => {
            let v = num(&["mm"])?;
            if v < FillSettings::MIN_INTERVAL_MM {
                return Err(DialogError::OutOfRange);
            }
            Length::from_mm(v).map(Number::Interval).map_err(|_| DialogError::OutOfRange)
        }
    }
}

/// The same fill with another line interval.
fn with_interval(f: &FillSettings, interval: Length) -> Option<FillSettings> {
    let mut out = FillSettings::new(interval, f.axis(), f.direction(), f.overscan())
        .ok()?
        .with_cross_hatch(f.cross_hatch())
        .with_grouping(f.grouping())
        .with_flood_fill(f.flood_fill());
    if f.angle_deg() != 0.0 {
        out = out.with_angle(f.angle_deg()).ok()?;
    }
    if f.ramp_mm() != 0.0 {
        out = out.with_ramp(Length::from_mm(f.ramp_mm()).ok()?).ok()?;
    }
    Some(out)
}

pub(super) fn action(
    s: &mut Session,
    document: DocumentId,
    action: DialogAction,
    request: Option<RequestId>,
) -> AppUpdate {
    match action {
        DialogAction::Close => {
            s.cam.sublayers = SublayersState::default();
            AppUpdate::redraw(document)
        }
        DialogAction::AddRow => {
            let Some((operation, _, _)) = chosen(s) else { return changed() };
            let snapshot = s.doc.snapshot();
            let Some(op) = snapshot.process().operation(operation) else { return changed() };
            let copy = SubOperation { mode: op.mode(), settings: op.settings() };
            // A layer that cannot be copied as it is (an image) starts as a plain line.
            let copy = match copy.mode {
                OperationMode::Line { .. } | OperationMode::Fill(_) => copy,
                _ => SubOperation { mode: OperationMode::Line { kerf: Kerf::NONE }, settings: copy.settings },
            };
            rewrite(s, document, request, |subs, _| {
                if subs.len() >= AdvancedOptions::MAX_SUB_OPERATIONS {
                    return None;
                }
                subs.push(copy);
                Some(subs.len() - 1)
            })
        }
        DialogAction::RemoveRow => rewrite(s, document, request, |subs, i| {
            if i >= subs.len() {
                return None;
            }
            subs.remove(i);
            Some(i.min(subs.len().saturating_sub(1)))
        }),
        DialogAction::MoveRowUp => rewrite(s, document, request, |subs, i| {
            let j = i.checked_sub(1)?;
            subs.swap(i, j);
            Some(j)
        }),
        DialogAction::MoveRowDown => rewrite(s, document, request, |subs, i| {
            let j = i + 1;
            (j < subs.len()).then(|| {
                subs.swap(i, j);
                j
            })
        }),
        _ => AppUpdate::default(),
    }
}
