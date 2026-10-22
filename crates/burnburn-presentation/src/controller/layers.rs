//! LightBurn-like layers over the process layer (C01–C05): every operation has a palette colour
//! and a canvas visibility, both window state of the session (never document data, never undo);
//! the colour palette assigns the selection with `AssignGeometry`/`UnassignGeometry`, and the Cut
//! Settings sheet sends raw text that becomes one `UpdateOperation`.
//!
//! Colours are not stored in the project yet: an operation without a chosen colour (from the
//! operations panel, or after reopening a file) gets the lowest free colour in list order.

use std::collections::{BTreeMap, BTreeSet};

use burnburn_document::process::{
    AdvancedOptions, FillSettings, Kerf, OffsetFillOrder, OffsetFillSettings, Operation, OperationDraft, OperationMode,
    Passes, PowerPercent, PowerRange, ProcessCommand, ProcessError, ProcessSettings, ScanAxis, ScanDirection, Speed,
    ToolLayer,
};
use burnburn_document::{CommandPayload, DocumentSnapshot};
use burnburn_editor::{SceneSnapshot, StyleRole};
use burnburn_job::{Feature, MachineCapabilities, Support};
use burnburn_types::{DocumentId, GeometryId, Length, OperationId, RequestId};

use super::cut_options;
use super::process::{parse_speed, refused};
use super::{AppUpdate, DEFAULT_FILL_INTERVAL_MM, Session, apply_edit};
use crate::field::{format_mm, parse_number, parse_position};
use crate::intent::{CutSettingsInput, OperationField, OperationKind};
use crate::view_model::{
    CutSettingsViewModel, LAYER_SLOTS, MessageKey, ModeChoice, OperationRow, PALETTE_SLOTS, StatusMessage,
    TOOL_SLOT_T1, TOOL_SLOT_T2,
};

/// Settings of a layer created by a palette click (LightBurn-like defaults).
const NEW_LAYER_SPEED_MM_S: f64 = 100.0;
const NEW_LAYER_POWER_PCT: f64 = 20.0;
/// Ring spacing when a layer switches to Offset fill (mm).
const DEFAULT_OFFSET_STEP_MM: f64 = 0.5;

/// Modes the Cut Settings sheet offers, top to bottom.
pub(crate) const SHEET_MODES: [OperationKind; 3] =
    [OperationKind::Line, OperationKind::Fill, OperationKind::OffsetFill];

#[derive(Debug, Clone, Default)]
pub(super) struct LayerState {
    /// Chosen (or once given) colour per operation. Entries of removed operations stay, so an
    /// undo/redo of the removal brings the same colour back.
    slots: BTreeMap<OperationId, u8>,
    hidden: BTreeSet<OperationId>,
    /// The open Cut Settings sheet and why its last OK was refused.
    sheet: Option<(OperationId, Option<MessageKey>)>,
}

/// Colour of every current operation: the stored one, else the lowest colour no other current
/// operation has, in list order.
pub(in crate::controller) fn slots(state: &LayerState, snapshot: &DocumentSnapshot) -> BTreeMap<OperationId, u8> {
    let ops: Vec<OperationId> = snapshot.process().operations().map(|o| o.id()).collect();
    let mut used: BTreeSet<u8> =
        ops.iter().filter_map(|id| state.slots.get(id).copied()).filter(|s| *s < LAYER_SLOTS).collect();
    let mut out = BTreeMap::new();
    for id in ops {
        // A tool layer sits on its own button at the end of the palette.
        if let Some(tool) = snapshot.process().operation(id).and_then(|o| o.tool()) {
            out.insert(id, tool_slot(tool));
            continue;
        }
        let slot = match state.slots.get(&id) {
            Some(slot) => *slot,
            None => {
                let free = (0..LAYER_SLOTS).find(|n| !used.contains(n)).unwrap_or(0);
                used.insert(free);
                free
            }
        };
        out.insert(id, slot);
    }
    out
}

/// Store the colours of operations that only had an implicit one, so later operations cannot
/// shift them (called after every change).
pub(super) fn sync(s: &mut Session) {
    let current = slots(&s.layers, &s.doc.snapshot());
    s.layers.slots.extend(current);
    if let Some((op, _)) = s.layers.sheet
        && s.doc.snapshot().process().operation(op).is_none()
    {
        s.layers.sheet = None;
    }
}

/// The palette button of a tool layer.
fn tool_slot(tool: ToolLayer) -> u8 {
    match tool {
        ToolLayer::T1 => TOOL_SLOT_T1,
        ToolLayer::T2 => TOOL_SLOT_T2,
    }
}

/// The tool layer behind a palette button, if it is one.
fn tool_of_slot(slot: u8) -> Option<ToolLayer> {
    match slot {
        TOOL_SLOT_T1 => Some(ToolLayer::T1),
        TOOL_SLOT_T2 => Some(ToolLayer::T2),
        _ => None,
    }
}

pub(in crate::controller) fn kind_of(mode: OperationMode) -> OperationKind {
    match mode {
        OperationMode::Line { .. } => OperationKind::Line,
        OperationMode::Fill(_) => OperationKind::Fill,
        OperationMode::OffsetFill(_) => OperationKind::OffsetFill,
        OperationMode::Image(_) => OperationKind::Image,
        _ => OperationKind::Other,
    }
}

fn kerf_text(mode: OperationMode) -> String {
    match mode {
        OperationMode::Line { kerf } => format_mm(kerf.width_mm()),
        _ => "0".to_owned(),
    }
}

pub(super) fn operation_row(s: &Session, op: &Operation) -> OperationRow {
    let settings = op.settings();
    let slot = slots(&s.layers, &s.doc.snapshot()).get(&op.id()).copied().unwrap_or(0);
    OperationRow {
        id: op.id(),
        name: op.name().to_owned(),
        kind: kind_of(op.mode()),
        slot,
        tool: op.tool().is_some(),
        speed: format_mm(settings.speed.get_mm_per_s()),
        power: format_mm(settings.power.max().get()),
        power_min: format_mm(settings.power.min().get()),
        passes: settings.passes.get().to_string(),
        kerf: kerf_text(op.mode()),
        visible: !s.layers.hidden.contains(&op.id()),
        targets: op.targets().len(),
        output_enabled: op.output_enabled(),
    }
}

/// The modes the sheet of an operation of `kind` offers: an image keeps its mode, the rest switch
/// between Line, Fill and Offset fill.
fn sheet_modes(kind: OperationKind, caps: &MachineCapabilities) -> Vec<ModeChoice> {
    match kind {
        OperationKind::Image => vec![ModeChoice { kind, blocked: None }],
        _ => SHEET_MODES.iter().map(|k| ModeChoice { kind: *k, blocked: mode_blocked(*k, caps) }).collect(),
    }
}

/// Why `kind` cannot be chosen with this machine profile (conservative matrix without one).
fn mode_blocked(kind: OperationKind, caps: &MachineCapabilities) -> Option<MessageKey> {
    let feature = match kind {
        OperationKind::OffsetFill => Feature::OffsetFill,
        _ => return None,
    };
    missing(feature, caps)
}

fn missing(feature: Feature, caps: &MachineCapabilities) -> Option<MessageKey> {
    (caps.support(feature) != Support::Supported).then_some(MessageKey::MissingCapability(feature))
}

pub(super) fn cut_settings_view(s: &Session, caps: Option<&MachineCapabilities>) -> Option<CutSettingsViewModel> {
    let (id, error) = s.layers.sheet?;
    let snapshot = s.doc.snapshot();
    let op = snapshot.process().operation(id)?;
    let conservative = MachineCapabilities::conservative();
    let caps = caps.unwrap_or(&conservative);
    let row = operation_row(s, op);
    Some(CutSettingsViewModel {
        operation: id,
        name: row.name,
        slot: row.slot,
        mode: row.kind,
        modes: sheet_modes(row.kind, caps),
        speed: row.speed,
        power_min: row.power_min,
        power_max: row.power,
        passes: row.passes,
        kerf: row.kerf,
        output_enabled: row.output_enabled,
        visible: row.visible,
        error,
        extras: cut_options::extras_of(op),
        gates: cut_options::gates_of(row.kind, caps),
    })
}

/// A fresh request id per document command (several commands per click are possible).
pub(super) type NextRequest<'a> = &'a mut dyn FnMut() -> Option<RequestId>;

fn new_layer_draft(slot: u8) -> Option<OperationDraft> {
    let power = PowerPercent::new(NEW_LAYER_POWER_PCT).ok()?;
    Some(OperationDraft {
        name: format!("C{slot:02}"),
        mode: OperationMode::Line { kerf: Kerf::NONE },
        settings: ProcessSettings {
            speed: Speed::mm_per_s(NEW_LAYER_SPEED_MM_S).ok()?,
            power: PowerRange::new(power, power).ok()?,
            passes: Passes::new(1).ok()?,
            air_assist: true,
        },
        output_enabled: true,
    })
}

/// Palette click: the selection goes to the operation of colour `slot` (created when missing).
/// Without `add` the objects leave their other operations first.
pub(super) fn assign_layer(
    s: &mut Session,
    document: DocumentId,
    slot: u8,
    add: bool,
    next_request: NextRequest<'_>,
) -> AppUpdate {
    if slot >= PALETTE_SLOTS {
        return AppUpdate::default();
    }
    let selection: Vec<GeometryId> = s.selection.ids().to_vec();
    if selection.is_empty() {
        return refused(s, MessageKey::SelectObjectsFirst);
    }
    sync(s);
    s.operation_error = None;
    let snapshot = s.doc.snapshot();
    let target = snapshot.process().operations().find(|o| s.layers.slots.get(&o.id()) == Some(&slot)).map(|o| o.id());
    let mut edits = Vec::new();
    if !add {
        for op in snapshot.process().operations().filter(|o| Some(o.id()) != target) {
            let leaving: Vec<_> = selection.iter().copied().filter(|g| op.targets().contains(g)).collect();
            if !leaving.is_empty() {
                edits.push(ProcessCommand::UnassignGeometry { operation: op.id(), targets: leaving });
            }
        }
    }
    let before: BTreeSet<OperationId> = snapshot.process().operations().map(|o| o.id()).collect();
    match target.and_then(|t| snapshot.process().operation(t)) {
        Some(op) => {
            let joining: Vec<_> = selection.iter().copied().filter(|g| !op.targets().contains(g)).collect();
            if !joining.is_empty() {
                edits.push(ProcessCommand::AssignGeometry { operation: op.id(), targets: joining });
            }
        }
        None => match tool_of_slot(slot) {
            Some(tool) => edits.push(ProcessCommand::AddToolLayer { tool, targets: selection.clone() }),
            None => match new_layer_draft(slot) {
                Some(draft) => edits.push(ProcessCommand::AddOperation { draft, targets: selection.clone() }),
                None => return refused(s, MessageKey::CommandRejected),
            },
        },
    }
    let mut update = AppUpdate { view_changed: true, ..AppUpdate::default() };
    for edit in edits {
        update = apply_edit(s, document, CommandPayload::Process(edit), next_request());
        if !update.effects.is_empty() {
            break;
        }
    }
    if target.is_none()
        && let Some(new) = s.doc.snapshot().process().operations().map(|o| o.id()).find(|id| !before.contains(id))
    {
        s.layers.slots.insert(new, slot);
    }
    // The palette keeps the clicked objects selected (not the objects the commands touched).
    s.selection.apply(&burnburn_editor::SelectionChange::Replace(selection));
    s.selection.retain_existing(&s.doc.snapshot());
    update
}

pub(super) fn set_visible(s: &mut Session, document: DocumentId, operation: OperationId, visible: bool) -> AppUpdate {
    if s.doc.snapshot().process().operation(operation).is_none() {
        return AppUpdate::default();
    }
    let changed = if visible { s.layers.hidden.remove(&operation) } else { s.layers.hidden.insert(operation) };
    if changed { AppUpdate::redraw(document) } else { AppUpdate::default() }
}

pub(super) fn set_output(
    s: &mut Session,
    document: DocumentId,
    operation: OperationId,
    enabled: bool,
    request: Option<RequestId>,
) -> AppUpdate {
    let snapshot = s.doc.snapshot();
    let Some(op) = snapshot.process().operation(operation) else { return AppUpdate::default() };
    if op.output_enabled() == enabled {
        return AppUpdate::default();
    }
    // A tool layer is never output; saying so beats a refused command.
    if op.tool().is_some() {
        s.messages.push(StatusMessage { key: MessageKey::ToolLayerNoOutput });
        return AppUpdate { view_changed: true, ..AppUpdate::default() };
    }
    let draft = OperationDraft { output_enabled: enabled, ..op.draft() };
    apply_edit(s, document, CommandPayload::Process(ProcessCommand::UpdateOperation { operation, draft }), request)
}

pub(super) fn open_cut_settings(s: &mut Session, operation: OperationId) -> AppUpdate {
    if s.doc.snapshot().process().operation(operation).is_none() {
        return AppUpdate::default();
    }
    s.layers.sheet = Some((operation, None));
    AppUpdate { view_changed: true, ..AppUpdate::default() }
}

pub(super) fn cancel_cut_settings(s: &mut Session) -> AppUpdate {
    match s.layers.sheet.take() {
        Some(_) => AppUpdate { view_changed: true, ..AppUpdate::default() },
        None => AppUpdate::default(),
    }
}

fn invalid(field: OperationField) -> MessageKey {
    MessageKey::InvalidNumber(field)
}

/// A percentage 0–100 (`%` optional, decimal comma or point).
pub(super) fn parse_percent(text: &str, field: OperationField) -> Result<PowerPercent, MessageKey> {
    parse_number(text, &["%"]).ok().and_then(|v| PowerPercent::new(v).ok()).ok_or(invalid(field))
}

/// A whole number of passes (1–1000).
pub(super) fn parse_passes(text: &str) -> Result<Passes, MessageKey> {
    let v = parse_number(text, &[]).map_err(|_| invalid(OperationField::Passes))?;
    if v.fract() != 0.0 || !(1.0..=f64::from(Passes::MAX)).contains(&v) {
        return Err(invalid(OperationField::Passes));
    }
    Passes::new(v as u32).map_err(|_| invalid(OperationField::Passes))
}

/// Kerf width with units (`0,15 mm`); empty means none, negative is refused.
pub(super) fn parse_kerf(text: &str) -> Result<Kerf, MessageKey> {
    if text.trim().is_empty() {
        return Ok(Kerf::NONE);
    }
    let length = parse_position(text).map_err(|_| invalid(OperationField::Kerf))?;
    Kerf::width(length).map_err(|_| invalid(OperationField::Kerf))
}

pub(super) fn default_fill() -> Option<FillSettings> {
    let interval = Length::from_mm(DEFAULT_FILL_INTERVAL_MM).ok()?;
    let overscan = Length::from_mm(0.0).ok()?;
    FillSettings::new(interval, ScanAxis::X, ScanDirection::Bidirectional, overscan).ok()
}

fn default_offset_fill() -> Option<OffsetFillSettings> {
    OffsetFillSettings::new(Length::from_mm(DEFAULT_OFFSET_STEP_MM).ok()?, OffsetFillOrder::OutsideIn).ok()
}

/// The mode for `kind`, keeping the current mode's own settings where they fit. An image keeps
/// its mode and nothing becomes an image here (its source is not part of the sheet).
fn mode_for(kind: OperationKind, current: OperationMode, kerf: Kerf) -> Option<OperationMode> {
    let image = matches!(current, OperationMode::Image(_));
    Some(match (kind, current) {
        (OperationKind::Image, OperationMode::Image(_)) => current,
        (OperationKind::Image, _) => return None,
        _ if image => return None,
        (OperationKind::Line, _) => OperationMode::Line { kerf },
        (OperationKind::Fill, OperationMode::Fill(f)) => OperationMode::Fill(f),
        (OperationKind::Fill, _) => OperationMode::Fill(default_fill()?),
        (OperationKind::OffsetFill, OperationMode::OffsetFill(o)) => OperationMode::OffsetFill(o),
        (OperationKind::OffsetFill, _) => OperationMode::OffsetFill(default_offset_fill()?),
        // Other modes are not edited here; their own settings stay.
        _ => current,
    })
}

/// What OK in the sheet turns into: the operation draft and, when the line options change, the
/// advanced options.
struct SheetEdit {
    draft: OperationDraft,
    advanced: AdvancedOptions,
}

fn sheet_edit(op: &Operation, input: &CutSettingsInput, caps: &MachineCapabilities) -> Result<SheetEdit, MessageKey> {
    let speed: Speed = parse_speed(&input.speed)?;
    let min = parse_percent(&input.power_min, OperationField::PowerMin)?;
    let max = parse_percent(&input.power_max, OperationField::PowerMax)?;
    let power = PowerRange::new(min, max).map_err(|_| invalid(OperationField::PowerMin))?;
    let passes = parse_passes(&input.passes)?;
    let line = input.mode == OperationKind::Line;
    let kerf = if line { parse_kerf(&input.kerf)? } else { Kerf::NONE };
    let mode = mode_for(input.mode, op.mode(), kerf).ok_or(MessageKey::CommandRejected)?;
    let (mode, advanced) = cut_options::apply(op, mode, &input.extras)?;
    let current = op.draft();
    let draft = OperationDraft {
        mode,
        settings: ProcessSettings { speed, power, passes, ..current.settings },
        output_enabled: input.output_enabled,
        ..current
    };
    // Options the operation did not use before need the profile's capability; options it
    // already has stay as they are (the compiler reports them) and never block other edits.
    let candidate = Operation::new(op.id(), draft.clone(), op.targets().to_vec(), op.recipe())
        .and_then(|o| o.with_advanced(advanced.clone()))
        .map_err(|e| match e {
            ProcessError::ConflictingOptions { .. } => MessageKey::CutOptionsConflict,
            _ => MessageKey::CommandRejected,
        })?;
    let before = burnburn_cam::required_features(op);
    if let Some(blocked) = burnburn_cam::required_features(&candidate)
        .into_iter()
        .filter(|f| !before.contains(f))
        .find_map(|f| missing(f, caps))
    {
        return Err(blocked);
    }
    Ok(SheetEdit { draft, advanced })
}

/// OK in the Cut Settings sheet: everything valid is applied and closes the sheet; the first
/// invalid field keeps it open with its message. Mode settings are one `UpdateOperation`, line
/// options one `SetAdvanced` (the document has no combined command, so an OK that changes both
/// is two undo steps); nothing is sent for what did not change.
pub(super) fn apply_cut_settings(
    s: &mut Session,
    document: DocumentId,
    operation: OperationId,
    input: &CutSettingsInput,
    caps: Option<&MachineCapabilities>,
    next_request: NextRequest<'_>,
) -> AppUpdate {
    let snapshot = s.doc.snapshot();
    let Some(op) = snapshot.process().operation(operation) else { return AppUpdate::default() };
    let conservative = MachineCapabilities::conservative();
    let edit = match sheet_edit(op, input, caps.unwrap_or(&conservative)) {
        Ok(d) => d,
        Err(key) => {
            s.layers.sheet = Some((operation, Some(key)));
            return AppUpdate { view_changed: true, ..AppUpdate::default() };
        }
    };
    s.layers.sheet = None;
    let mut update = set_visible(s, document, operation, input.visible);
    update.view_changed = true;
    let draft_changed = edit.draft != op.draft();
    let advanced_changed = edit.advanced != *op.advanced();
    let update_draft = ProcessCommand::UpdateOperation { operation, draft: edit.draft };
    let set_advanced = ProcessCommand::SetAdvanced { operation, advanced: edit.advanced };
    // Line options are only legal on a Line operation: leaving Line clears them first, entering
    // it sets the mode first, so no step in between is an operation the compiler would refuse.
    let leaving_line = matches!(op.mode(), OperationMode::Line { .. })
        && !matches!(update_draft_mode(&update_draft), Some(OperationMode::Line { .. }));
    let mut commands = Vec::new();
    if draft_changed {
        commands.push(update_draft);
    }
    if advanced_changed {
        if leaving_line {
            commands.insert(0, set_advanced);
        } else {
            commands.push(set_advanced);
        }
    }
    for command in commands {
        let applied = apply_edit(s, document, CommandPayload::Process(command), next_request());
        let failed = !applied.effects.is_empty();
        update.effects.extend(applied.effects);
        update.scenes_changed.extend(applied.scenes_changed);
        if failed {
            break;
        }
    }
    update
}

fn update_draft_mode(command: &ProcessCommand) -> Option<OperationMode> {
    match command {
        ProcessCommand::UpdateOperation { draft, .. } => Some(draft.mode),
        _ => None,
    }
}

/// Canvas colours and visibility (C01): an object is drawn in the colour of its first operation
/// in list order and left out when every operation it is in is hidden. Selection outranks colour.
pub(super) fn style_scene(s: &Session, snapshot: &DocumentSnapshot, scene: &mut SceneSnapshot) {
    let colours = slots(&s.layers, snapshot);
    let process = snapshot.process();
    scene.items.retain_mut(|item| {
        let ops = process.operations_using(item.source);
        if !ops.is_empty() && ops.iter().all(|op| s.layers.hidden.contains(op)) {
            return false;
        }
        if item.role == StyleRole::Design
            && let Some(slot) = ops.iter().find(|op| !s.layers.hidden.contains(op)).and_then(|op| colours.get(op))
        {
            item.role = StyleRole::ProcessLayer(u16::from(*slot));
        }
        true
    });
}
