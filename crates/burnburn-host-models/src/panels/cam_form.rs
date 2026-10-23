//! The CAM dialogs as a [`FormSpec`] (LF6a): Optimization, Device, Array, Feeder, Preflight,
//! Cutting order, Sub-layers and the three tests. What each dialog shows stays in presentation's
//! [`DialogViewModel`] and in the words of `cam.rs`; this file only arranges it for the form
//! builder: fields in sections with a heading, the unit as a suffix after the field, the hint of a
//! caption as the help under it, the reason a field is greyed said once per section, the dialog
//! buttons by role (Close cancels, Apply or Generate is primary) and every other button as an
//! action inside the form.
//!
//! The shape of a form does not change while the user works in it: a hidden field keeps its row,
//! a field that can show an error always has an (empty) error line, a section that can show a reason
//! always has an (empty) reason. The host updates the widgets in place and builds again only when a
//! dialog really gets other rows.

use burnburn_document::process::OrderKey;
use burnburn_presentation::{
    DialogActionKind, DialogControl, DialogField, DialogValue, DialogViewModel, FieldState,
};

use burnburn_presentation::DialogRow;

use super::{
    cam_action_title, cam_caption, cam_choice_label, cam_error, cam_label, cam_note, cam_result_text,
    cam_support_line, cam_title, order_key_caption,
};
use crate::form::{Control, DialogButtons, FormButton, FormId, FormRow, FormSection, FormSpec, FormTool};
use crate::format::Quantity;
use crate::l10n::{Locale, message_or_key};

/// What a form id stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CamTarget {
    Field(DialogField),
    Action(DialogActionKind),
    /// The arrow beside an `Order by` level; `true` is up.
    Move(OrderKey, bool),
    /// The notes under the form (and the user's own lines).
    Notes,
    /// The generated test, in words.
    Result,
    /// The capability list of Device Settings.
    Capabilities,
}

/// A dialog as a form, and which dialog part each id is.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CamForm {
    pub spec: FormSpec,
    pub targets: Vec<(FormId, CamTarget)>,
    /// Why fields are greyed, each reason once (the sections' reasons, joined).
    pub reasons: String,
}

/// A caption without its unit and remark.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptionParts {
    pub label: String,
    pub unit: Option<Quantity>,
    /// The remark that stood in brackets after the label: a range, what zero means.
    pub hint: Option<String>,
}

/// The unit a bracketed caption word names.
fn unit_of(word: &str) -> Option<Quantity> {
    Some(match word.trim() {
        "mm" => Quantity::Millimeters,
        "%" => Quantity::Percent,
        "mm/s" => Quantity::MillimetersPerSecond,
        "s" => Quantity::Seconds,
        "ms" => Quantity::Milliseconds,
        "°" | "graden" | "degrees" => Quantity::Degrees,
        _ => return None,
    })
}

/// A hint starts with a capital, unless it starts with a unit (`mm of lpi`).
fn as_hint(text: &str) -> String {
    let first = text.split_whitespace().next().unwrap_or("");
    if first.chars().count() < 3 || matches!(first, "mm" | "dpi" | "lpi") {
        return text.to_owned();
    }
    let mut chars = text.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// Splits `Overcut (mm, 0 = uit)` into the label `Overcut`, the unit `mm` and the hint `0 = uit`.
/// A bracket that names no unit (`(1–25)`, `(zoals jij het opgeeft)`) becomes the hint whole.
pub fn split_caption(caption: &str) -> CaptionParts {
    let trimmed = caption.trim_end();
    if let Some(open) = trimmed.rfind(" (")
        && trimmed.ends_with(')')
    {
        let label = trimmed[..open].trim_end().to_owned();
        let inner = &trimmed[open + 2..trimmed.len() - 1];
        let (first, rest) = match inner.split_once(", ") {
            Some((a, b)) => (a, Some(b)),
            None => (inner, None),
        };
        return match unit_of(first) {
            Some(unit) => CaptionParts { label, unit: Some(unit), hint: rest.map(as_hint) },
            None => CaptionParts { label, unit: None, hint: Some(as_hint(inner)) },
        };
    }
    CaptionParts { label: caption.to_owned(), unit: None, hint: None }
}

/// The groups of fields, each a section under its heading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Group {
    General,
    Order,
    Optimize,
    Cells,
    Base,
    Layout,
    Bed,
    Speed,
    Rotary,
    Machine,
    Grbl,
    Grid,
    Circle,
    Path,
    Sublayer,
}

fn group_of(field: DialogField) -> Group {
    use DialogField as F;
    match field {
        F::OrderEnabled(_) => Group::Order,
        F::InnerFirst
        | F::ReduceTravel
        | F::BestStart
        | F::BestDirection
        | F::HideBacklash
        | F::RemoveOverlapping
        | F::OverlapTolerance => Group::Optimize,
        F::CellMode
        | F::ColumnParameter
        | F::ColumnMin
        | F::ColumnMax
        | F::ColumnCount
        | F::RowsEnabled
        | F::RowParameter
        | F::RowMin
        | F::RowMax
        | F::RowCount
        | F::IntervalMin
        | F::IntervalMax
        | F::IntervalCount
        | F::FocusStart
        | F::FocusStep
        | F::FocusCount
        | F::LineLength
        | F::LineSpacing => Group::Cells,
        F::BaseSpeed | F::BasePower | F::BasePasses | F::FillInterval | F::FillOverscan => Group::Base,
        F::CellWidth
        | F::CellHeight
        | F::CellGap
        | F::LabelsEnabled
        | F::LabelHeight
        | F::BorderEnabled
        | F::BorderMargin => Group::Layout,
        F::BedWidth | F::BedHeight | F::BedOrigin => Group::Bed,
        F::MaxSpeed | F::TravelSpeed | F::MinPower | F::MaxPower => Group::Speed,
        F::RotaryEnabled
        | F::RotaryKind
        | F::RotaryDiameter
        | F::StepsPerRotation
        | F::YStepsPerMm
        | F::RotaryMirror => Group::Rotary,
        F::Backend | F::Model | F::Firmware => Group::Machine,
        F::GrblMaxSpindle
        | F::GrblLaserMode
        | F::GrblPowerMode
        | F::GrblAirAssist
        | F::GrblWorkCoordinate
        | F::GrblDecimals => Group::Grbl,
        F::GridColumns | F::GridRows | F::GridPitchX | F::GridPitchY => Group::Grid,
        F::CircularCount | F::CircularCentreX | F::CircularCentreY | F::CircularSweep | F::CircularRotate => {
            Group::Circle
        }
        F::PathCount | F::PathAlong | F::PathOrient => Group::Path,
        F::SubMode | F::SubSpeed | F::SubPower | F::SubPasses | F::SubInterval => Group::Sublayer,
        _ => Group::General,
    }
}

fn pick(pair: (&'static str, &'static str), locale: Locale) -> &'static str {
    match locale {
        Locale::Nl => pair.0,
        Locale::En => pair.1,
    }
}

fn heading(group: Group, locale: Locale) -> Option<&'static str> {
    Some(pick(
        match group {
            Group::General => return None,
            Group::Order => ("Volgorde", "Order"),
            Group::Optimize => ("Optimalisaties", "Optimizations"),
            Group::Cells => ("Vakken van de test", "Cells of the test"),
            Group::Base => ("Basisinstelling", "Base setting"),
            Group::Layout => ("Opmaak", "Layout"),
            Group::Bed => ("Bed", "Bed"),
            Group::Speed => ("Snelheid en vermogen", "Speed and power"),
            Group::Rotary => ("Rotary", "Rotary"),
            Group::Machine => ("Machine", "Machine"),
            Group::Grbl => ("GRBL", "GRBL"),
            Group::Grid => ("Raster", "Grid"),
            Group::Circle => ("Cirkel", "Circle"),
            Group::Path => ("Langs een pad", "Along a path"),
            Group::Sublayer => ("Instellingen van de sublaag", "Sub-layer settings"),
        },
        locale,
    ))
}

/// Buttons the form shows as rows (the others are the dialog's buttons).
const INLINE: [DialogActionKind; 6] = [
    DialogActionKind::ShowSource,
    DialogActionKind::AddRow,
    DialogActionKind::RemoveRow,
    DialogActionKind::MoveRowUp,
    DialogActionKind::MoveRowDown,
    DialogActionKind::ReturnToFlatbed,
];

const ALL_KINDS: [DialogActionKind; 10] = [
    DialogActionKind::ShowSource,
    DialogActionKind::AddRow,
    DialogActionKind::RemoveRow,
    DialogActionKind::MoveRowUp,
    DialogActionKind::MoveRowDown,
    DialogActionKind::ReturnToFlatbed,
    DialogActionKind::Generate,
    DialogActionKind::ExportRd,
    DialogActionKind::Close,
    DialogActionKind::Apply,
];

fn action_id(kind: DialogActionKind) -> FormId {
    FormId(500 + ALL_KINDS.iter().position(|k| *k == kind).unwrap_or(0) as u32)
}

/// A row for a button inside the form.
fn action_row(vm: &DialogViewModel, kind: DialogActionKind, locale: Locale) -> Option<FormRow> {
    let state = vm.action(kind)?;
    let mut row = FormRow::new(
        action_id(kind).0,
        "",
        Control::Action { title: cam_action_title(kind, vm.dialog, locale).to_owned() },
    );
    row.enabled = state.enabled;
    row.tooltip = state.reason.map(|n| cam_note(n, locale)).filter(|t| !t.is_empty());
    Some(row)
}

fn unique(texts: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for t in texts {
        if !out.contains(&t) {
            out.push(t);
        }
    }
    out
}

fn field_row(
    vm: &DialogViewModel,
    index: usize,
    row_vm: &DialogRow,
    locale: Locale,
    targets: &mut Vec<(FormId, CamTarget)>,
) -> FormRow {
    let id = 100 + index as u32;
    let caption = cam_caption(row_vm.field, vm, locale);
    let (enabled, hidden) = match row_vm.state {
        FieldState::Hidden => (false, true),
        FieldState::Editable => (true, false),
        FieldState::Disabled(_) => (false, false),
    };
    let (label, unit, hint, control) = match row_vm.field.control() {
        DialogControl::Flag => (caption, None, None, Control::Check { on: row_vm.value.flag() }),
        DialogControl::Text => {
            let parts = split_caption(&caption);
            let value = row_vm.value.text().to_owned();
            let control = if parts.unit.is_some() {
                Control::Number { value }
            } else {
                Control::Text { value, placeholder: String::new() }
            };
            (parts.label, parts.unit, parts.hint, control)
        }
        DialogControl::Choice => {
            let parts = split_caption(&caption);
            let choices = row_vm.field.choices();
            let options: Vec<String> = if row_vm.labels.is_empty() {
                choices.iter().map(|c| cam_choice_label(*c, locale).to_owned()).collect()
            } else {
                row_vm.labels.iter().map(|l| cam_label(l, locale)).collect()
            };
            let selected = match row_vm.value {
                DialogValue::Choice(i) => i,
                _ => 0,
            };
            (parts.label, None, parts.hint, Control::Choice { options, selected })
        }
    };
    let mut row = FormRow::new(id, &label, control);
    row.unit = unit;
    row.help = hint;
    // Only an error belongs to the row; an empty one keeps the line (and the shape) ready.
    row.error = Some(row_vm.error.map(|e| cam_error(e, locale)).unwrap_or_default());
    row.enabled = enabled;
    row.hidden = hidden;
    for (choice, reason) in &row_vm.blocked_choices {
        if let Some(i) = row_vm.field.choices().iter().position(|c| c == choice) {
            row.disabled_options.push((i, Some(message_or_key(*reason, locale))));
        }
    }
    if let (Some((can_up, can_down)), DialogField::OrderEnabled(key)) = (row_vm.reorder, row_vm.field) {
        for (up, can, glyph) in [(true, can_up, "↑"), (false, can_down, "↓")] {
            let tool_id = 300 + 2 * index as u32 + u32::from(!up);
            let mut tool = FormTool::new(tool_id, glyph, &arrow_label(key, up, locale));
            tool.enabled = can;
            row.tools.push(tool);
            targets.push((FormId(tool_id), CamTarget::Move(key, up)));
        }
    }
    targets.push((FormId(id), CamTarget::Field(row_vm.field)));
    row
}

/// The accessibility label of an arrow beside an `Order by` level.
pub fn arrow_label(key: OrderKey, up: bool, locale: Locale) -> String {
    let word = match (up, locale) {
        (true, Locale::Nl) => "omhoog",
        (false, Locale::Nl) => "omlaag",
        (true, Locale::En) => "up",
        (false, Locale::En) => "down",
    };
    format!("{} {}", order_key_caption(key, locale), word)
}

const NOTES_ID: u32 = 700;
const RESULT_ID: u32 = 701;
const CAPABILITIES_ID: u32 = 702;

/// The form of a dialog.
pub fn cam_form(vm: &DialogViewModel, locale: Locale) -> CamForm {
    let mut targets: Vec<(FormId, CamTarget)> = Vec::new();
    // The fields by group, in the order the groups first appear.
    let mut groups: Vec<(Group, Vec<usize>)> = Vec::new();
    for (i, row) in vm.rows.iter().enumerate() {
        let g = group_of(row.field);
        match groups.iter_mut().find(|(k, _)| *k == g) {
            Some((_, rows)) => rows.push(i),
            None => groups.push((g, vec![i])),
        }
    }
    let single = groups.len() <= 1;
    let mut all_reasons: Vec<String> = Vec::new();
    let mut sections: Vec<FormSection> = Vec::new();
    let mut section_groups: Vec<Group> = Vec::new();
    for (group, indices) in &groups {
        let rows: Vec<FormRow> =
            indices.iter().map(|i| field_row(vm, *i, &vm.rows[*i], locale, &mut targets)).collect();
        // A reason is said once in the dialog: in the first section it applies to.
        let reasons: Vec<String> = unique(
            indices
                .iter()
                .filter_map(|i| match vm.rows[*i].state {
                    FieldState::Disabled(Some(reason)) => Some(message_or_key(reason, locale)),
                    _ => None,
                })
                .collect(),
        )
        .into_iter()
        .filter(|r| !all_reasons.contains(r))
        .collect();
        all_reasons.extend(reasons.iter().cloned());
        let heading = if single { None } else { heading(*group, locale) };
        let mut section = FormSection::new(heading, rows);
        section.reason = Some(reasons.join("\n"));
        sections.push(section);
        section_groups.push(*group);
    }
    if sections.is_empty() {
        sections.push(FormSection::new(None, Vec::new()));
        section_groups.push(Group::General);
    }
    // Buttons that belong to the form: Back to flat bed under the rotary, the others after the fields.
    for kind in INLINE {
        let Some(row) = action_row(vm, kind, locale) else { continue };
        let at = if kind == DialogActionKind::ReturnToFlatbed {
            section_groups.iter().position(|g| *g == Group::Rotary)
        } else {
            None
        }
        .unwrap_or(sections.len() - 1);
        targets.push((row.id, CamTarget::Action(kind)));
        sections[at].rows.push(row);
    }
    // The notes, the capability list and the result.
    let mut notes: Vec<String> = vm.notes.iter().map(|n| cam_note(*n, locale)).filter(|t| !t.is_empty()).collect();
    notes.extend(vm.lines.iter().cloned());
    sections.push(FormSection::new(
        Some(pick(("Opmerkingen", "Notes"), locale)),
        vec![FormRow::new(NOTES_ID, "", Control::ReadOnly { value: notes.join("\n") })],
    ));
    targets.push((FormId(NOTES_ID), CamTarget::Notes));
    if !vm.capabilities.is_empty() {
        let lines: Vec<String> = vm.capabilities.iter().map(|c| cam_support_line(c, locale)).collect();
        sections.push(FormSection::new(
            Some(pick(("Mogelijkheden van dit profiel", "Capabilities of this profile"), locale)),
            vec![FormRow::new(CAPABILITIES_ID, "", Control::ReadOnly { value: lines.join("\n") })],
        ));
        targets.push((FormId(CAPABILITIES_ID), CamTarget::Capabilities));
    }
    if vm.dialog.is_test() {
        let result = match (&vm.result, vm.busy) {
            (Some(r), _) => cam_result_text(r, locale),
            (None, true) => pick(("Bezig met compileren…", "Compiling…"), locale).to_owned(),
            (None, false) => String::new(),
        };
        let mut rows = vec![FormRow::new(RESULT_ID, "", Control::ReadOnly { value: result })];
        targets.push((FormId(RESULT_ID), CamTarget::Result));
        if let Some(row) = action_row(vm, DialogActionKind::ExportRd, locale) {
            targets.push((row.id, CamTarget::Action(DialogActionKind::ExportRd)));
            rows.push(row);
        }
        sections.push(FormSection::new(Some(pick(("Resultaat", "Result"), locale)), rows));
    }
    // The dialog's own buttons: Close cancels, Apply or Generate is the primary one.
    let button = |kind: DialogActionKind| {
        vm.action(kind).map(|state| {
            let mut b = FormButton::new(action_id(kind).0, cam_action_title(kind, vm.dialog, locale));
            b.enabled = state.enabled;
            b.tooltip = state.reason.map(|n| cam_note(n, locale)).filter(|t| !t.is_empty());
            (kind, b)
        })
    };
    let mut buttons = DialogButtons::default();
    if let Some((kind, b)) = button(DialogActionKind::Close) {
        targets.push((b.id, CamTarget::Action(kind)));
        buttons.cancel = Some(b);
    }
    if let Some((kind, b)) = button(DialogActionKind::Generate).or_else(|| button(DialogActionKind::Apply)) {
        targets.push((b.id, CamTarget::Action(kind)));
        buttons.primary = Some(b);
    }
    CamForm {
        spec: FormSpec { title: cam_title(vm.dialog, locale).to_owned(), sections, buttons },
        targets,
        reasons: all_reasons.join("\n"),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;
    use crate::form::{ButtonRole, Platform};
    use burnburn_presentation::CamDialog;
    use burnburn_job::{Feature, MachineCapabilities, MachineFrame, MachineProfile, OriginCorner, PowerRatio, Support};
    use burnburn_presentation::{HeadlessEditorHarness, UiIntent};
    use burnburn_ruida::{Magic, OverscanPolicy, RuidaProfile};
    use burnburn_types::DocumentId;

    const LOCALES: [Locale; 2] = [Locale::Nl, Locale::En];

    fn rig(z: Support) -> (HeadlessEditorHarness, DocumentId) {
        let mut h = HeadlessEditorHarness::new();
        let doc = h.new_document().unwrap();
        let profile = MachineProfile::new(
            "research-1500x900",
            1,
            MachineFrame::new(1500.0, 900.0, OriginCorner::BottomLeft).unwrap(),
            1000.0,
            400.0,
            PowerRatio::new(0.0).unwrap(),
            PowerRatio::new(1.0).unwrap(),
        )
        .unwrap()
        .with_capabilities(MachineCapabilities::conservative().with(Feature::ZStep, z))
        .unwrap();
        h.send(UiIntent::SetMachineProfile {
            profile: Box::new(profile),
            ruida: RuidaProfile { magic: Magic::FIXTURE_0X88, overscan: OverscanPolicy::ControllerManaged },
        });
        (h, doc)
    }

    fn open(h: &mut HeadlessEditorHarness, doc: DocumentId, dialog: CamDialog) -> DialogViewModel {
        h.send(UiIntent::OpenCamDialog { document: doc, dialog });
        h.controller().cam_dialog(doc, dialog).unwrap()
    }

    fn parts(caption: &str) -> (String, Option<Quantity>, Option<String>) {
        let p = split_caption(caption);
        (p.label, p.unit, p.hint)
    }

    #[test]
    fn a_caption_loses_its_unit_and_keeps_its_remark_as_a_hint() {
        let cases: [(&str, &str, Option<Quantity>, Option<&str>); 14] = [
            ("Snelheid (mm/s)", "Snelheid", Some(Quantity::MillimetersPerSecond), None),
            ("Vermogen (%)", "Vermogen", Some(Quantity::Percent), None),
            ("Overcut (mm, 0 = uit)", "Overcut", Some(Quantity::Millimeters), Some("0 = uit")),
            ("Tabvermogen (%, 0 = overslaan)", "Tabvermogen", Some(Quantity::Percent), Some("0 = overslaan")),
            ("Hoek (graden, 360 = rond)", "Hoek", Some(Quantity::Degrees), Some("360 = rond")),
            ("Sweep (degrees, 360 = full circle)", "Sweep", Some(Quantity::Degrees), Some("360 = full circle")),
            ("Aanloop, hoek (°)", "Aanloop, hoek", Some(Quantity::Degrees), None),
            ("Puls (ms)", "Puls", Some(Quantity::Milliseconds), None),
            ("Pauze tussen de runs (s)", "Pauze tussen de runs", Some(Quantity::Seconds), None),
            ("Tolerantie (mm, hoogstens 5)", "Tolerantie", Some(Quantity::Millimeters), Some("Hoogstens 5")),
            ("Aantal vakken (1–25)", "Aantal vakken", None, Some("1–25")),
            ("Kleinste lijnafstand (mm of lpi)", "Kleinste lijnafstand", None, Some("mm of lpi")),
            ("Controllermodel (zoals jij het opgeeft)", "Controllermodel", None, Some("Zoals jij het opgeeft")),
            ("Binnenste vormen eerst snijden", "Binnenste vormen eerst snijden", None, None),
        ];
        for (caption, label, unit, hint) in cases {
            assert_eq!(parts(caption), (label.to_owned(), unit, hint.map(str::to_owned)), "{caption}");
        }
    }

    #[test]
    fn every_dialog_is_a_valid_form_in_both_languages_and_every_row_has_a_target() {
        let (mut h, doc) = rig(Support::Unsupported);
        for dialog in CamDialog::ALL {
            let vm = open(&mut h, doc, dialog);
            for locale in LOCALES {
                let form = cam_form(&vm, locale);
                assert!(form.spec.problems().is_empty(), "{dialog:?} {locale:?}: {:?}", form.spec.problems());
                assert!(!form.spec.sections.is_empty(), "{dialog:?} has sections");
                for row in &vm.rows {
                    assert!(
                        form.targets.iter().any(|(_, t)| *t == CamTarget::Field(row.field)),
                        "{dialog:?}: no form row for {:?}",
                        row.field
                    );
                }
                for action in &vm.actions {
                    assert!(
                        form.targets.iter().any(|(_, t)| *t == CamTarget::Action(action.action)),
                        "{dialog:?}: no button for {:?}",
                        action.action
                    );
                }
                for section in &form.spec.sections {
                    for row in section.rows.iter().filter(|r| !matches!(r.control, Control::Check { .. })) {
                        let tail = row.label.rsplit_once('(').map(|(_, t)| t.to_owned()).unwrap_or_default();
                        assert!(
                            !row.label.ends_with(')') || tail.contains(' ') && !tail.starts_with("mm"),
                            "{dialog:?}: label {:?} still carries a unit or a remark",
                            row.label
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn close_cancels_and_apply_or_generate_is_primary_and_the_rest_are_buttons_inside_the_form() {
        let (mut h, doc) = rig(Support::Supported);
        for (dialog, primary) in [
            (CamDialog::Device, Some("Pas toe")),
            (CamDialog::Optimization, Some("Pas toe")),
            (CamDialog::Array, Some("Reeks maken")),
            (CamDialog::MaterialTest, Some("Maak test")),
            (CamDialog::Preflight, None),
        ] {
            let vm = open(&mut h, doc, dialog);
            let form = cam_form(&vm, Locale::Nl);
            let order: Vec<(ButtonRole, String)> =
                form.spec.buttons.order(Platform::Mac).into_iter().map(|(r, b)| (r, b.title.clone())).collect();
            let roles: Vec<ButtonRole> = order.iter().map(|(r, _)| *r).collect();
            match primary {
                Some(title) => {
                    assert_eq!(roles, vec![ButtonRole::Cancel, ButtonRole::Primary], "{dialog:?}");
                    assert_eq!(order[1].1, title, "{dialog:?}");
                }
                None => assert_eq!(roles, vec![ButtonRole::Cancel], "{dialog:?}"),
            }
            assert_eq!(order[0].1, "Sluiten");
        }
        // Save as RD and the like are rows of the form, not part of the button row.
        let vm = open(&mut h, doc, CamDialog::MaterialTest);
        let form = cam_form(&vm, Locale::Nl);
        let save = form.targets.iter().find(|(_, t)| *t == CamTarget::Action(DialogActionKind::ExportRd)).unwrap().0;
        let row = form.spec.sections.iter().flat_map(|s| &s.rows).find(|r| r.id == save).unwrap();
        assert!(matches!(row.control, Control::Action { .. }));
        assert!(!row.enabled, "nothing to save before a test exists");
    }

    #[test]
    fn a_reason_for_greyed_fields_is_said_once_and_no_field_repeats_it() {
        let (mut h, doc) = rig(Support::Unsupported);
        let vm = open(&mut h, doc, CamDialog::FocusTest);
        let form = cam_form(&vm, Locale::Nl);
        let all: Vec<String> = form.spec.sections.iter().flat_map(|s| s.texts()).collect();
        let reason = all.iter().filter(|t| t.contains("Ontbrekende capability")).count();
        assert_eq!(reason, 1, "{all:?}");
        assert!(form.reasons.contains("Z-stap") && form.reasons.matches("Ontbrekende capability").count() == 1);
        let start = form.targets.iter().find(|(_, t)| *t == CamTarget::Field(DialogField::FocusStart)).unwrap().0;
        let row = form.spec.sections.iter().flat_map(|s| &s.rows).find(|r| r.id == start).unwrap();
        assert!(!row.enabled);
        assert!(row.error.as_deref().unwrap_or("").is_empty(), "no note of its own beside a greyed field");
        let generate = form.spec.buttons.primary.as_ref().unwrap();
        assert!(!generate.enabled);
        assert!(form.spec.problems().is_empty());
    }

    #[test]
    fn the_order_levels_have_arrows_that_follow_what_can_move() {
        let (mut h, doc) = rig(Support::Supported);
        let vm = open(&mut h, doc, CamDialog::Optimization);
        let form = cam_form(&vm, Locale::En);
        let level = |key: OrderKey| {
            let id = form
                .targets
                .iter()
                .find(|(_, t)| *t == CamTarget::Field(DialogField::OrderEnabled(key)))
                .unwrap()
                .0;
            form.spec.sections.iter().flat_map(|s| &s.rows).find(|r| r.id == id).unwrap().clone()
        };
        let first = level(OrderKey::Layer);
        let last = level(OrderKey::Priority);
        assert_eq!(first.tools.len(), 2);
        assert!(!first.tools[0].enabled && first.tools[1].enabled, "the first level cannot move up");
        assert!(last.tools[0].enabled && !last.tools[1].enabled, "the last level cannot move down");
        let (up, down) = (first.tools[0].id, first.tools[1].id);
        assert!(form.targets.contains(&(up, CamTarget::Move(OrderKey::Layer, true))));
        assert!(form.targets.contains(&(down, CamTarget::Move(OrderKey::Layer, false))));
        assert_eq!(first.label, "Order by: Layer");
    }

    #[test]
    fn typing_ticking_and_an_error_do_not_change_the_shape_of_a_dialog() {
        let (mut h, doc) = rig(Support::Supported);
        let vm = open(&mut h, doc, CamDialog::Device);
        let before = cam_form(&vm, Locale::Nl).spec.shape();
        h.send(UiIntent::SetCamField {
            document: doc,
            dialog: CamDialog::Device,
            field: DialogField::RotaryEnabled,
            value: DialogValue::Flag(true),
        });
        h.send(UiIntent::SetCamField {
            document: doc,
            dialog: CamDialog::Device,
            field: DialogField::BedWidth,
            value: DialogValue::Text("abc".into()),
        });
        h.send(UiIntent::CamDialogAction {
            document: doc,
            dialog: CamDialog::Device,
            action: burnburn_presentation::DialogAction::Apply,
        });
        let vm = h.controller().cam_dialog(doc, CamDialog::Device).unwrap();
        let after = cam_form(&vm, Locale::Nl);
        assert_eq!(before, after.spec.shape(), "a tick shows the rotary fields, an error shows its line: same shape");
        let rotary =
            after.targets.iter().find(|(_, t)| *t == CamTarget::Field(DialogField::RotaryDiameter)).unwrap().0;
        let row = after.spec.sections.iter().flat_map(|s| &s.rows).find(|r| r.id == rotary).unwrap();
        assert!(!row.hidden, "the rotary fields are there once the rotary is on");
        let width = after.targets.iter().find(|(_, t)| *t == CamTarget::Field(DialogField::BedWidth)).unwrap().0;
        let row = after.spec.sections.iter().flat_map(|s| &s.rows).find(|r| r.id == width).unwrap();
        assert_eq!(row.error.as_deref(), Some("Geen geldige waarde."), "{:?}", row.error);
        assert_eq!(row.suffix(), Some("mm"));
        assert_eq!(row.label, "Bedbreedte");
    }
}
