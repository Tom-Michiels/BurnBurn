//! What the window's panels show, from the existing presentation view models (no AppKit): the
//! toolbar's tool control, the inspector's field order and the JobPlan preview panel.
//!
//! Panels only show and forward: tools and buttons become `UiIntent::Invoke`, field text goes to
//! presentation unparsed (DR-05), also in the operations panel; the bed-camera section only asks
//! for access or starts the camera on an explicit click.

use burnburn_io::import::{SvgUnitBasis, UnitDecision};
use burnburn_presentation::{
    AppViewModel, BedCameraIntent, CameraAuthorization, CameraMode, CameraPixelFormat, CameraViewModel, CommandKey,
    FieldKey, ImportUnit, LAYER_SLOTS, MachineButton, MachineViewModel, OperationField, OperationKind, UiIntent,
    format_mm,
};
use burnburn_types::{DocumentId, OperationId};

use crate::l10n::{HostText, Locale, host_text, machine_status_text, message_or_key};

/// Segments of the toolbar's tool control, left to right.
pub const TOOLBAR_TOOLS: [CommandKey; 6] = [
    CommandKey::ToolSelect,
    CommandKey::ToolRectangle,
    CommandKey::ToolEllipse,
    CommandKey::ToolLine,
    CommandKey::ToolPen,
    CommandKey::ToolNode,
];

/// Inspector fields, top to bottom.
pub const INSPECTOR_FIELDS: [FieldKey; 5] =
    [FieldKey::X, FieldKey::Y, FieldKey::Width, FieldKey::Height, FieldKey::Rotation];

/// The tool command for toolbar segment `index` (AppKit's `selectedSegment`, −1 = none).
pub fn toolbar_tool(index: isize) -> Option<CommandKey> {
    TOOLBAR_TOOLS.get(usize::try_from(index).ok()?).copied()
}

/// The segment of the active tool (from the commands' checked state, like the menu checkmarks).
pub fn toolbar_selected(vm: &AppViewModel) -> Option<usize> {
    TOOLBAR_TOOLS.iter().position(|k| vm.commands.get(k).is_some_and(|c| c.checked))
}

/// Enablement per segment (same source as `validateMenuItem:`).
pub fn toolbar_enabled(vm: &AppViewModel) -> Vec<bool> {
    TOOLBAR_TOOLS.iter().map(|k| vm.commands.get(k).is_some_and(|c| c.enabled)).collect()
}

/// The preview panel's texts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanPanel {
    pub title: String,
    /// Full plan identity (`plan-<hash>`), the same that RD export and upload name; empty without plan.
    pub plan_id: String,
    pub estimate: String,
    /// Current, or out of date — said in words, not only with a colour (X03).
    pub state: String,
    pub findings: String,
}

pub fn plan_panel(vm: &AppViewModel, document: DocumentId, locale: Locale) -> PlanPanel {
    let plan = vm.document(document).and_then(|d| d.plan.as_ref());
    let Some(plan) = plan else {
        return PlanPanel {
            title: host_text(HostText::NoPlan, locale).to_owned(),
            plan_id: String::new(),
            estimate: String::new(),
            state: host_text(HostText::NoPlanHint, locale).to_owned(),
            findings: String::new(),
        };
    };
    let state = if plan.stale { HostText::PlanStale } else { HostText::PlanCurrent };
    PlanPanel {
        title: host_text(HostText::PlanTitle, locale).to_owned(),
        plan_id: plan.id.to_string(),
        estimate: estimate_text(plan.preview.estimated_seconds, locale),
        state: host_text(state, locale).to_owned(),
        findings: findings_text(plan.errors, plan.warnings, locale),
    }
}

fn findings_text(errors: usize, warnings: usize, locale: Locale) -> String {
    match locale {
        Locale::Nl => format!(
            "{errors} {}, {warnings} {}",
            if errors == 1 { "fout" } else { "fouten" },
            if warnings == 1 { "waarschuwing" } else { "waarschuwingen" }
        ),
        Locale::En => format!(
            "{errors} {}, {warnings} {}",
            if errors == 1 { "error" } else { "errors" },
            if warnings == 1 { "warning" } else { "warnings" }
        ),
    }
}

/// The plan's constant-velocity time estimate, always labelled as an estimate (it is not a
/// measured duration, `PlanPreview::estimated_seconds`).
pub fn estimate_text(seconds: f64, locale: Locale) -> String {
    if !(seconds.is_finite() && seconds >= 0.0) {
        return host_text(HostText::PlanEstimateUnknown, locale).to_owned();
    }
    let label = match locale {
        Locale::Nl => "schatting",
        Locale::En => "estimate",
    };
    let body = if seconds < 60.0 {
        let tenths = format!("{seconds:.1}");
        let tenths = if locale.decimal_sign() == ',' { tenths.replace('.', ",") } else { tenths };
        format!("{tenths} s")
    } else {
        let total = seconds.round() as u64;
        if total < 3600 {
            format!("{} min {:02} s", total / 60, total % 60)
        } else {
            let minutes = (seconds / 60.0).round() as u64;
            format!("{} h {:02} min", minutes / 60, minutes % 60)
        }
    };
    format!("≈ {body} ({label})")
}

/// Choices of the import preview's unit pop-up, top to bottom.
pub const IMPORT_UNITS: [ImportUnit; 3] = [ImportUnit::Millimeter, ImportUnit::Centimeter, ImportUnit::Inch];

/// The unit for pop-up item `index` (−1 = nothing chosen).
pub fn import_unit(index: isize) -> Option<ImportUnit> {
    IMPORT_UNITS.get(usize::try_from(index).ok()?).copied()
}

pub fn import_unit_name(unit: ImportUnit, locale: Locale) -> &'static str {
    match (unit, locale) {
        (ImportUnit::Millimeter, _) => "mm",
        (ImportUnit::Centimeter, _) => "cm",
        (ImportUnit::Inch, Locale::Nl) => "inch",
        (ImportUnit::Inch, Locale::En) => "inch",
    }
}

/// The import preview section (P03): file, unit decision, size, report, Importeer enablement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportPanel {
    pub file: String,
    pub unit_prompt: String,
    /// Pop-up item of the chosen unit (`None`: nothing chosen, the file's own unit applies).
    pub selected_unit: Option<usize>,
    pub size: String,
    pub report: String,
    pub commit_enabled: bool,
}

fn unit_label(mm_per_unit: f64) -> Option<&'static str> {
    [(1.0, "mm"), (10.0, "cm"), (25.4, "inch"), (1000.0, "m")]
        .iter()
        .find(|(f, _)| (mm_per_unit - f).abs() < 1e-9)
        .map(|(_, l)| *l)
}

pub fn import_panel(vm: &AppViewModel, document: DocumentId, locale: Locale) -> Option<ImportPanel> {
    let import = vm.document(document)?.import.as_ref()?;
    let nl = locale == Locale::Nl;
    let unit_prompt = if import.needs_unit {
        if nl {
            "Het bestand noemt geen eenheid: kies er een.".to_owned()
        } else {
            "The file names no unit: choose one.".to_owned()
        }
    } else {
        match import.units {
            Some(UnitDecision::DxfDeclared { mm_per_unit, .. }) => {
                let unit =
                    unit_label(mm_per_unit).map_or_else(|| format!("{} mm", format_mm(mm_per_unit)), str::to_owned);
                if nl { format!("Eenheid uit het bestand: {unit}") } else { format!("Unit from the file: {unit}") }
            }
            Some(UnitDecision::DxfExplicit { mm_per_unit }) => {
                let unit =
                    unit_label(mm_per_unit).map_or_else(|| format!("{} mm", format_mm(mm_per_unit)), str::to_owned);
                if nl { format!("Gekozen eenheid: {unit}") } else { format!("Chosen unit: {unit}") }
            }
            Some(UnitDecision::Svg { basis, .. }) => match (basis, nl) {
                (SvgUnitBasis::ViewBoxToPhysicalSize, true) => {
                    "Maat uit het SVG-bestand (viewBox en breedte/hoogte)".to_owned()
                }
                (SvgUnitBasis::ViewBoxToPhysicalSize, false) => {
                    "Size from the SVG file (viewBox and width/height)".to_owned()
                }
                (SvgUnitBasis::CssPixelsBySpec, true) => "SVG-pixels volgens de norm (96 per inch)".to_owned(),
                (SvgUnitBasis::CssPixelsBySpec, false) => "SVG pixels by the specification (96 per inch)".to_owned(),
                (_, true) => "Geen maat in het SVG-bestand: 96 eenheden per inch aangenomen".to_owned(),
                (_, false) => "No size in the SVG file: 96 units per inch assumed".to_owned(),
            },
            _ => String::new(),
        }
    };
    let size = match import.size_mm {
        Some((w, h)) => {
            if nl {
                format!("Maat: {} × {} mm", format_mm(w), format_mm(h))
            } else {
                format!("Size: {} × {} mm", format_mm(w), format_mm(h))
            }
        }
        None => {
            if nl {
                "Maat: onbekend tot je een eenheid kiest".to_owned()
            } else {
                "Size: unknown until you choose a unit".to_owned()
            }
        }
    };
    let skipped = import.skipped.len();
    let mut kinds: Vec<&str> = import.skipped.iter().map(|s| s.kind.as_str()).collect();
    kinds.sort_unstable();
    kinds.dedup();
    let open = import.open_contours;
    let report = if nl {
        let mut r = format!("{} objecten; {skipped} niet geïmporteerd", import.items);
        if !kinds.is_empty() {
            r.push_str(&format!(" ({})", kinds.join(", ")));
        }
        r.push_str(&format!("; {open} open {}", if open == 1 { "contour" } else { "contouren" }));
        r
    } else {
        let mut r = format!("{} objects; {skipped} not imported", import.items);
        if !kinds.is_empty() {
            r.push_str(&format!(" ({})", kinds.join(", ")));
        }
        r.push_str(&format!("; {open} open contour{}", if open == 1 { "" } else { "s" }));
        r
    };
    let commit_enabled = vm.commands.get(&CommandKey::CommitImport).is_some_and(|c| c.enabled) && !import.needs_unit;
    Some(ImportPanel {
        file: import.file.clone(),
        unit_prompt,
        selected_unit: import.unit.and_then(|u| IMPORT_UNITS.iter().position(|x| *x == u)),
        size,
        report,
        commit_enabled,
    })
}

/// The machine section: status in words (never "klaar" before the machine reports it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MachinePanel {
    pub status: String,
    pub message: String,
    pub progress: String,
    pub job: String,
    /// `(label, enabled)` per control, in view-model order.
    pub buttons: Vec<(String, bool)>,
}

pub fn machine_panel(vm: Option<&MachineViewModel>, locale: Locale) -> MachinePanel {
    let Some(m) = vm else {
        return MachinePanel {
            status: host_text(HostText::NoMachine, locale).to_owned(),
            message: String::new(),
            progress: String::new(),
            job: String::new(),
            buttons: Vec::new(),
        };
    };
    let progress = m.progress.map_or_else(String::new, |(done, total)| match locale {
        Locale::Nl => format!("{done} van {total} pakketten ontvangen (transport)"),
        Locale::En => format!("{done} of {total} packets received (transport)"),
    });
    MachinePanel {
        status: machine_status_text(m.status, locale).to_owned(),
        message: m.message.map(|k| message_or_key(k, locale)).unwrap_or_default(),
        progress,
        job: m.job_name.clone().unwrap_or_default(),
        buttons: m.buttons.iter().map(|b| (message_or_key(b.label, locale), b.enabled)).collect(),
    }
}

/// A click on a machine control: `action` and `seen` go to presentation unchanged (idempotency,
/// IM31); a disabled control sends nothing.
pub fn machine_intent(document: Option<DocumentId>, button: &MachineButton) -> Option<UiIntent> {
    button.enabled.then(|| UiIntent::Machine { document, action: button.action, seen: button.seen.clone() })
}

/// Unit shown after an inspector field's caption.
pub fn field_unit(key: FieldKey, locale: Locale) -> &'static str {
    match key {
        FieldKey::Rotation => host_text(HostText::UnitDegrees, locale),
        _ => host_text(HostText::Unit, locale),
    }
}

/// Kinds the operations panel's pop-up offers, top to bottom.
pub const OPERATION_KINDS: [OperationKind; 2] = [OperationKind::Line, OperationKind::Fill];

/// The kind for pop-up item `index` (−1 = nothing chosen).
pub fn operation_kind(index: isize) -> Option<OperationKind> {
    OPERATION_KINDS.get(usize::try_from(index).ok()?).copied()
}

pub fn operation_kind_name(kind: OperationKind, locale: Locale) -> &'static str {
    let text = match kind {
        OperationKind::Line => HostText::OperationKindLine,
        OperationKind::Fill => HostText::OperationKindFill,
        OperationKind::OffsetFill => HostText::OperationKindOffsetFill,
        _ => HostText::OperationKindOther,
    };
    host_text(text, locale)
}

/// One operation as the panel shows it; `speed`/`power` are the view model's texts (the editable
/// field contents), units are in the column captions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationLine {
    /// Palette colour (`C03`); the host draws the swatch in the theme colour of `slot`.
    pub slot: u8,
    pub layer: String,
    pub name: String,
    pub kind: String,
    pub speed: String,
    pub power: String,
    pub passes: String,
    pub targets: String,
    pub output: bool,
    pub visible: bool,
}

/// Columns of the layer (operations) table, left to right.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LayerColumn {
    Layer,
    Name,
    Kind,
    Speed,
    Power,
    Passes,
    Targets,
    /// Checkbox: the operation goes into the job.
    Output,
    /// Checkbox: its objects are drawn on the canvas.
    Visible,
}

pub const LAYER_COLUMNS: [LayerColumn; 9] = [
    LayerColumn::Layer,
    LayerColumn::Name,
    LayerColumn::Kind,
    LayerColumn::Speed,
    LayerColumn::Power,
    LayerColumn::Passes,
    LayerColumn::Targets,
    LayerColumn::Output,
    LayerColumn::Visible,
];

impl LayerColumn {
    /// `NSTableColumn` identifier.
    pub fn id(self) -> &'static str {
        match self {
            LayerColumn::Layer => "layer",
            LayerColumn::Name => "name",
            LayerColumn::Kind => "kind",
            LayerColumn::Speed => "speed",
            LayerColumn::Power => "power",
            LayerColumn::Passes => "passes",
            LayerColumn::Targets => "targets",
            LayerColumn::Output => "output",
            LayerColumn::Visible => "visible",
        }
    }

    pub fn from_id(id: &str) -> Option<LayerColumn> {
        LAYER_COLUMNS.iter().copied().find(|c| c.id() == id)
    }

    /// The raw-text field an edit of this column sets.
    pub fn field(self) -> Option<OperationField> {
        match self {
            LayerColumn::Name => Some(OperationField::Name),
            LayerColumn::Speed => Some(OperationField::Speed),
            LayerColumn::Power => Some(OperationField::PowerMax),
            LayerColumn::Passes => Some(OperationField::Passes),
            _ => None,
        }
    }

    pub fn is_toggle(self) -> bool {
        matches!(self, LayerColumn::Output | LayerColumn::Visible)
    }

    pub fn caption(self, locale: Locale) -> &'static str {
        let text = match self {
            LayerColumn::Layer => HostText::LayerColumn,
            LayerColumn::Name => HostText::OperationName,
            LayerColumn::Kind => HostText::OperationKind,
            LayerColumn::Speed => HostText::OperationSpeed,
            LayerColumn::Power => HostText::OperationPower,
            LayerColumn::Passes => HostText::OperationPasses,
            LayerColumn::Targets => HostText::OperationTargets,
            LayerColumn::Output => HostText::OperationOutput,
            LayerColumn::Visible => HostText::OperationVisible,
        };
        host_text(text, locale)
    }
}

impl OperationLine {
    /// The text a column shows (empty for the checkbox columns).
    pub fn text(&self, column: LayerColumn) -> String {
        match column {
            LayerColumn::Layer => self.layer.clone(),
            LayerColumn::Name => self.name.clone(),
            LayerColumn::Kind => self.kind.clone(),
            LayerColumn::Speed => self.speed.clone(),
            LayerColumn::Power => self.power.clone(),
            LayerColumn::Passes => self.passes.clone(),
            LayerColumn::Targets => self.targets.clone(),
            LayerColumn::Output | LayerColumn::Visible => String::new(),
        }
    }

    /// The state a checkbox column shows.
    pub fn checked(&self, column: LayerColumn) -> Option<bool> {
        match column {
            LayerColumn::Output => Some(self.output),
            LayerColumn::Visible => Some(self.visible),
            _ => None,
        }
    }
}

/// Palette label of a colour slot (`C00` … `C29`, as LightBurn names its layers).
pub fn layer_label(slot: u8) -> String {
    format!("C{slot:02}")
}

/// The colour slot of palette button `tag` (tag = slot).
pub fn palette_slot(tag: isize) -> Option<u8> {
    u8::try_from(tag).ok().filter(|s| *s < LAYER_SLOTS)
}

fn objects_text(n: usize, locale: Locale) -> String {
    match (locale, n) {
        (_, 1) => "1 object".to_owned(),
        (Locale::Nl, n) => format!("{n} objecten"),
        (Locale::En, n) => format!("{n} objects"),
    }
}

/// One palette button.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaletteButton {
    pub slot: u8,
    pub label: String,
    /// Tooltip and accessibility label: the colour and its layer (or that it is still empty).
    pub help: String,
    /// The colour has a layer in this document.
    pub used: bool,
}

pub fn palette_buttons(vm: &AppViewModel, document: DocumentId, locale: Locale) -> Vec<PaletteButton> {
    let ops = vm.document(document).map(|d| d.operations.as_slice()).unwrap_or_default();
    (0..LAYER_SLOTS)
        .map(|slot| {
            let label = layer_label(slot);
            let layer = ops.iter().find(|o| o.slot == slot);
            let help = match layer {
                Some(op) => format!("{label} — {} ({})", op.name, objects_text(op.targets, locale)),
                None => format!("{label} — {}", host_text(HostText::PaletteNewLayer, locale)),
            };
            PaletteButton { slot, label, help, used: layer.is_some() }
        })
        .collect()
}

/// Modes of the Cut Settings pop-up, top to bottom (as presentation lists them).
pub const SHEET_MODES: [OperationKind; 3] = [OperationKind::Line, OperationKind::Fill, OperationKind::OffsetFill];

/// The mode of pop-up item `index` (−1 = nothing chosen).
pub fn sheet_mode(index: isize) -> Option<OperationKind> {
    SHEET_MODES.get(usize::try_from(index).ok()?).copied()
}

/// Whether the kerf field applies to pop-up item `index` (Line only).
pub fn kerf_applies(index: isize) -> bool {
    sheet_mode(index) == Some(OperationKind::Line)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModeItem {
    pub title: String,
    /// False for a mode the machine profile does not support; the title says why.
    pub enabled: bool,
}

/// The Cut Settings sheet's texts and states.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CutSettingsPanel {
    pub operation: OperationId,
    pub slot: u8,
    pub title: String,
    pub modes: Vec<ModeItem>,
    pub mode_index: Option<usize>,
    pub speed: String,
    pub power_min: String,
    pub power_max: String,
    pub passes: String,
    pub kerf: String,
    /// Kerf applies to Line only.
    pub kerf_enabled: bool,
    pub output: bool,
    pub visible: bool,
    pub error: String,
}

pub fn cut_settings_panel(vm: &AppViewModel, document: DocumentId, locale: Locale) -> Option<CutSettingsPanel> {
    let c = vm.document(document)?.cut_settings.as_ref()?;
    let modes = SHEET_MODES
        .iter()
        .map(|kind| {
            let name = operation_kind_name(*kind, locale);
            match c.modes.iter().find(|m| m.kind == *kind).and_then(|m| m.blocked) {
                Some(reason) => {
                    ModeItem { title: format!("{name} — {}", message_or_key(reason, locale)), enabled: false }
                }
                None => ModeItem { title: name.to_owned(), enabled: true },
            }
        })
        .collect();
    Some(CutSettingsPanel {
        operation: c.operation,
        slot: c.slot,
        title: format!("{} — {} {}", host_text(HostText::CutSettingsTitle, locale), layer_label(c.slot), c.name),
        modes,
        mode_index: SHEET_MODES.iter().position(|k| *k == c.mode),
        speed: c.speed.clone(),
        power_min: c.power_min.clone(),
        power_max: c.power_max.clone(),
        passes: c.passes.clone(),
        kerf: c.kerf.clone(),
        kerf_enabled: c.mode == OperationKind::Line,
        output: c.output_enabled,
        visible: c.visible,
        error: c.error.map(|k| message_or_key(k, locale)).unwrap_or_default(),
    })
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OperationsPanel {
    pub rows: Vec<OperationLine>,
    /// Why presentation sent no command for the last input (empty: no problem).
    pub error: String,
    /// Shown instead of rows when there are none.
    pub empty_text: String,
}

pub fn operations_panel(vm: &AppViewModel, document: DocumentId, locale: Locale) -> OperationsPanel {
    let Some(doc) = vm.document(document) else { return OperationsPanel::default() };
    let rows = doc
        .operations
        .iter()
        .map(|op| OperationLine {
            name: op.name.clone(),
            kind: operation_kind_name(op.kind, locale).to_owned(),
            slot: op.slot,
            layer: layer_label(op.slot),
            speed: op.speed.clone(),
            power: op.power.clone(),
            passes: op.passes.clone(),
            output: op.output_enabled,
            visible: op.visible,
            targets: objects_text(op.targets, locale),
        })
        .collect();
    OperationsPanel {
        rows,
        error: doc.operation_error.map(|k| message_or_key(k, locale)).unwrap_or_default(),
        empty_text: host_text(HostText::NoOperations, locale).to_owned(),
    }
}

/// Buttons of the bed-camera section.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CameraControl {
    RequestAccess,
    Start,
    Stop,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CameraPanel {
    pub status: String,
    /// Device names for the pop-up.
    pub devices: Vec<String>,
    /// Pop-up item of the running device.
    pub selected: Option<usize>,
    pub frame: String,
    /// "Visual placement only" (camera pixels never feed CAM).
    pub note: String,
    pub access_enabled: bool,
    pub start_enabled: bool,
    pub stop_enabled: bool,
}

pub fn camera_panel(vm: Option<&CameraViewModel>, locale: Locale) -> CameraPanel {
    let Some(c) = vm else {
        return CameraPanel { status: host_text(HostText::CameraNoPort, locale).to_owned(), ..CameraPanel::default() };
    };
    let authorized = c.authorization == CameraAuthorization::Authorized;
    let status = match c.authorization {
        _ if c.active.is_some() => HostText::CameraRunning,
        _ if authorized && c.devices.is_empty() => HostText::CameraNoDevices,
        CameraAuthorization::Authorized => HostText::CameraAuthorized,
        CameraAuthorization::NotDetermined => HostText::CameraNotDetermined,
        CameraAuthorization::Denied => HostText::CameraDenied,
        CameraAuthorization::Restricted => HostText::CameraRestricted,
    };
    let frame = match (&c.active, c.frame) {
        (Some(_), Some(f)) => match locale {
            Locale::Nl => format!("Beeld {}: {} × {} px", f.seq, f.width, f.height),
            Locale::En => format!("Frame {}: {} × {} px", f.seq, f.width, f.height),
        },
        (Some(_), None) => host_text(HostText::CameraNoFrame, locale).to_owned(),
        (None, _) => String::new(),
    };
    CameraPanel {
        status: host_text(status, locale).to_owned(),
        devices: c.devices.iter().map(|d| d.name.clone()).collect(),
        selected: c.active.as_ref().and_then(|(id, _)| c.devices.iter().position(|d| d.id == *id)),
        frame,
        note: message_or_key(c.note, locale),
        access_enabled: c.authorization == CameraAuthorization::NotDetermined,
        start_enabled: authorized && c.active.is_none() && c.devices.iter().any(|d| bgra_mode(&d.modes).is_some()),
        stop_enabled: c.active.is_some(),
    }
}

/// The capture mode the host's adapters decode: the largest BGRA mode.
fn bgra_mode(modes: &[CameraMode]) -> Option<CameraMode> {
    modes
        .iter()
        .filter(|m| m.pixel_format == CameraPixelFormat::Bgra8)
        .max_by_key(|m| u64::from(m.width) * u64::from(m.height))
        .copied()
}

/// A click on a camera button with pop-up item `device_index` chosen; disabled → nothing.
pub fn camera_intent(vm: Option<&CameraViewModel>, control: CameraControl, device_index: isize) -> Option<UiIntent> {
    let panel = camera_panel(vm, Locale::En);
    let c = vm?;
    let intent = match control {
        CameraControl::RequestAccess if panel.access_enabled => BedCameraIntent::RequestAccess,
        CameraControl::Start if panel.start_enabled => {
            let device = c.devices.get(usize::try_from(device_index).ok()?)?;
            BedCameraIntent::Start { device: device.id.clone(), mode: bgra_mode(&device.modes)? }
        }
        CameraControl::Stop if panel.stop_enabled => BedCameraIntent::Stop,
        _ => return None,
    };
    Some(UiIntent::BedCamera(intent))
}
