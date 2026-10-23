//! The Cut Settings sheet as a [`FormSpec`] (LF6a): the mode, the five numbers and the two switches
//! of the layer in a section under the layer's name, and in a second section every option field of
//! every mode. The rows of the modes that are not chosen are hidden, not left out, so the form keeps
//! its shape while the user changes the mode, a switch or a pop-up and the host never has to build
//! it again: the widgets the window controller reads on OK stay the same.
//!
//! A note (why a row is greyed, the advice that applies to it) stands once under the first row of a
//! run of rows that have the same note, and as the tooltip of every row of the run.

use burnburn_presentation::{CutControl, CutExtras, CutField, CutSettingsViewModel, OperationKind};

use super::cut_settings::row_control;
use super::{
    CaptionParts, CutSettingsPanel, ModeItem, OptionRow, RowControl, SHEET_MODES, choice_label, operation_kind_name,
    option_caption, option_rows, split_caption,
};
use crate::form::{Control, DialogButtons, FormButton, FormId, FormRow, FormSection, FormSpec};
use crate::l10n::{HostText, Locale, host_text};

/// What a form id stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CutTarget {
    Mode,
    Speed,
    PowerMin,
    PowerMax,
    Passes,
    Kerf,
    Output,
    Visible,
    /// The sheet's own error (a refused OK), under the rows of the layer.
    Error,
    Option(CutField),
    Ok,
    Cancel,
}

/// A sheet as a form, and which part of the sheet each id is.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CutForm {
    pub spec: FormSpec,
    pub targets: Vec<(FormId, CutTarget)>,
}

impl CutForm {
    /// The id of a part of the sheet.
    pub fn id(&self, target: CutTarget) -> Option<FormId> {
        self.targets.iter().find(|(_, t)| *t == target).map(|(id, _)| *id)
    }
}

/// The values of the layer's own rows, as the sheet opens with them or as the user has them now.
#[derive(Debug, Clone, PartialEq)]
pub struct CutTop {
    pub title: String,
    pub modes: Vec<ModeItem>,
    pub mode_index: Option<usize>,
    pub speed: String,
    pub power_min: String,
    pub power_max: String,
    pub passes: String,
    pub kerf: String,
    pub output: bool,
    pub visible: bool,
    pub error: String,
}

impl CutTop {
    /// What the sheet opens with.
    pub fn opening(panel: &CutSettingsPanel) -> CutTop {
        CutTop {
            title: panel.title.clone(),
            modes: panel.modes.clone(),
            mode_index: panel.mode_index,
            speed: panel.speed.clone(),
            power_min: panel.power_min.clone(),
            power_max: panel.power_max.clone(),
            passes: panel.passes.clone(),
            kerf: panel.kerf.clone(),
            output: panel.output,
            visible: panel.visible,
            error: panel.error.clone(),
        }
    }

    /// A sheet that is not open: the modes are named, nothing is filled in.
    pub fn blank(locale: Locale) -> CutTop {
        CutTop {
            title: host_text(HostText::CutSettingsTitle, locale).to_owned(),
            modes: SHEET_MODES
                .iter()
                .map(|k| ModeItem { title: operation_kind_name(*k, locale).to_owned(), enabled: true })
                .collect(),
            mode_index: Some(0),
            speed: String::new(),
            power_min: String::new(),
            power_max: String::new(),
            passes: String::new(),
            kerf: String::new(),
            output: false,
            visible: false,
            error: String::new(),
        }
    }
}

const MODE_ID: u32 = 1;
const SPEED_ID: u32 = 2;
const POWER_MIN_ID: u32 = 3;
const POWER_MAX_ID: u32 = 4;
const PASSES_ID: u32 = 5;
const KERF_ID: u32 = 6;
const OUTPUT_ID: u32 = 7;
const VISIBLE_ID: u32 = 8;
const ERROR_ID: u32 = 9;
const OK_ID: u32 = 20;
const CANCEL_ID: u32 = 21;
const OPTION_BASE: u32 = 100;

/// A number of the layer with its caption split into label and unit.
fn number(id: u32, caption: &str, value: &str) -> FormRow {
    let parts = split_caption(caption);
    let mut row = FormRow::new(id, &parts.label, Control::Number { value: value.to_owned() });
    row.unit = parts.unit;
    row.help = parts.hint;
    row
}

/// The form of the sheet for `kind` (the mode chosen now) and the option values `live` on it. Without
/// a sheet every option row is hidden and empty.
pub fn cut_form(
    sheet: Option<&CutSettingsViewModel>,
    top: &CutTop,
    live: &CutExtras,
    kind: OperationKind,
    locale: Locale,
) -> CutForm {
    let t = |text: HostText| host_text(text, locale);
    let mut targets: Vec<(FormId, CutTarget)> = Vec::new();
    let mut note = |id: u32, target: CutTarget| targets.push((FormId(id), target));

    let mut mode = FormRow::new(
        MODE_ID,
        t(HostText::CutSettingsMode),
        Control::Choice {
            options: top.modes.iter().map(|m| m.title.clone()).collect(),
            selected: top.mode_index.unwrap_or(0),
        },
    );
    for (i, m) in top.modes.iter().enumerate() {
        if !m.enabled {
            mode = mode.disabled_option(i, None);
        }
    }
    let mut kerf = number(KERF_ID, t(HostText::CutSettingsKerf), &top.kerf);
    kerf.enabled = kind == OperationKind::Line;
    let mut error = FormRow::new(ERROR_ID, "", Control::ReadOnly { value: String::new() });
    error.error = Some(top.error.clone());
    let layer = vec![
        mode,
        number(SPEED_ID, t(HostText::CutSettingsSpeed), &top.speed),
        number(POWER_MIN_ID, t(HostText::CutSettingsPowerMin), &top.power_min),
        number(POWER_MAX_ID, t(HostText::CutSettingsPowerMax), &top.power_max),
        number(PASSES_ID, t(HostText::CutSettingsPasses), &top.passes),
        kerf,
        FormRow::new(OUTPUT_ID, t(HostText::CutSettingsOutput), Control::Check { on: top.output }),
        FormRow::new(VISIBLE_ID, t(HostText::CutSettingsVisible), Control::Check { on: top.visible }),
        error,
    ];
    for (id, target) in [
        (MODE_ID, CutTarget::Mode),
        (SPEED_ID, CutTarget::Speed),
        (POWER_MIN_ID, CutTarget::PowerMin),
        (POWER_MAX_ID, CutTarget::PowerMax),
        (PASSES_ID, CutTarget::Passes),
        (KERF_ID, CutTarget::Kerf),
        (OUTPUT_ID, CutTarget::Output),
        (VISIBLE_ID, CutTarget::Visible),
        (ERROR_ID, CutTarget::Error),
    ] {
        note(id, target);
    }

    // The option rows of the mode chosen now, and which of them say their note: the first of a run
    // of rows with the same note.
    let shown: Vec<OptionRow> = sheet.map(|s| option_rows(s, kind, live, locale)).unwrap_or_default();
    let mut noted: Vec<CutField> = Vec::new();
    let mut previous: Option<&str> = None;
    for r in &shown {
        if !r.note.is_empty() && previous != Some(r.note.as_str()) {
            noted.push(r.field);
        }
        previous = Some(r.note.as_str());
    }
    let mut options: Vec<FormRow> = Vec::new();
    for (i, field) in CutField::ALL.into_iter().enumerate() {
        let id = OPTION_BASE + i as u32;
        let caption = option_caption(field, locale);
        let visible = shown.iter().find(|r| r.field == field);
        let flag = matches!(field.control(), CutControl::Flag);
        let parts = if flag {
            CaptionParts { label: caption.to_owned(), unit: None, hint: None }
        } else {
            split_caption(caption)
        };
        let from_sheet = || sheet.map(|s| row_control(field, s, live, locale));
        let control = match visible.map(|r| r.control.clone()).or_else(from_sheet) {
            Some(RowControl::Text(value)) => Control::Number { value },
            Some(RowControl::Check(on)) => Control::Check { on },
            Some(RowControl::Popup { items, selected }) => {
                Control::Choice { options: items.into_iter().map(|m| m.title).collect(), selected }
            }
            None => match field.control() {
                CutControl::Text => Control::Number { value: String::new() },
                CutControl::Flag => Control::Check { on: false },
                CutControl::Choice => Control::Choice {
                    options: field.choices().iter().map(|c| choice_label(*c, locale).to_owned()).collect(),
                    selected: 0,
                },
            },
        };
        let mut row = FormRow::new(id, &parts.label, control);
        row.unit = parts.unit;
        let note_here = visible.filter(|r| noted.contains(&r.field)).map_or("", |r| r.note.as_str());
        let help: Vec<&str> =
            [parts.hint.as_deref().unwrap_or(""), note_here].into_iter().filter(|t| !t.is_empty()).collect();
        row.help = Some(help.join("\n"));
        row.tooltip = visible.map(|r| r.note.clone()).filter(|n| !n.is_empty());
        row.enabled = visible.is_some_and(|r| r.enabled);
        row.hidden = visible.is_none();
        if let Control::Choice { options, .. } = &row.control
            && let Some(r) = visible
            && let RowControl::Popup { items, .. } = &r.control
        {
            for (k, item) in items.iter().enumerate() {
                if !item.enabled && k < options.len() {
                    row.disabled_options.push((k, None));
                }
            }
        }
        targets.push((FormId(id), CutTarget::Option(field)));
        options.push(row);
    }
    targets.push((FormId(OK_ID), CutTarget::Ok));
    targets.push((FormId(CANCEL_ID), CutTarget::Cancel));
    CutForm {
        spec: FormSpec {
            title: t(HostText::CutSettingsTitle).to_owned(),
            sections: vec![
                FormSection::new(Some(&top.title), layer),
                FormSection::new(Some(t(HostText::CutSettingsOptions)), options),
            ],
            buttons: DialogButtons {
                cancel: Some(FormButton::new(CANCEL_ID, t(HostText::CutSettingsCancel))),
                primary: Some(FormButton::new(OK_ID, t(HostText::CutSettingsOk))),
                ..DialogButtons::default()
            },
            ..FormSpec::default()
        },
        targets,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;
    use crate::panels::cut_settings_panel;
    use burnburn_job::{Feature, MachineCapabilities, MachineFrame, MachineProfile, OriginCorner, PowerRatio, Support};
    use burnburn_presentation::{CommandKey, HeadlessEditorHarness, SelectMode, UiIntent};
    use burnburn_ruida::{Magic, OverscanPolicy, RuidaProfile};
    use burnburn_types::DocumentId;

    const LOCALES: [Locale; 2] = [Locale::Nl, Locale::En];

    fn open(caps: MachineCapabilities) -> (HeadlessEditorHarness, DocumentId, CutSettingsViewModel) {
        let mut h = HeadlessEditorHarness::new();
        let doc = h.new_document().unwrap();
        h.invoke(doc, CommandKey::ToolRectangle);
        h.drag(doc, (0.0, 0.0), (40.0, 20.0));
        h.invoke(doc, CommandKey::ToolSelect);
        let id = h.document(doc).unwrap().objects[0].id;
        h.send(UiIntent::Select { document: doc, ids: vec![id], mode: SelectMode::Replace });
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
        .with_capabilities(caps)
        .unwrap();
        h.send(UiIntent::SetMachineProfile {
            profile: Box::new(profile),
            ruida: RuidaProfile { magic: Magic::FIXTURE_0X88, overscan: OverscanPolicy::ControllerManaged },
        });
        h.send(UiIntent::AssignLayer { document: doc, slot: 1, add: false });
        let op = h.document(doc).unwrap().operations[0].id;
        h.send(UiIntent::OpenCutSettings { document: doc, operation: op });
        let sheet = h.document(doc).unwrap().cut_settings.unwrap();
        (h, doc, sheet)
    }

    fn form_for(
        h: &HeadlessEditorHarness,
        doc: DocumentId,
        sheet: &CutSettingsViewModel,
        kind: OperationKind,
        locale: Locale,
    ) -> CutForm {
        let panel = cut_settings_panel(&h.view(), doc, locale).unwrap();
        cut_form(Some(sheet), &CutTop::opening(&panel), &sheet.extras, kind, locale)
    }

    fn row(form: &CutForm, target: CutTarget) -> &FormRow {
        let id = form.id(target).unwrap_or_else(|| panic!("no id for {target:?}"));
        form.spec.sections.iter().flat_map(|s| &s.rows).find(|r| r.id == id).unwrap()
    }

    #[test]
    fn the_shape_is_the_same_for_every_mode_and_for_a_sheet_that_is_not_open() {
        let (h, doc, sheet) = open(MachineCapabilities::conservative());
        for locale in LOCALES {
            let line = form_for(&h, doc, &sheet, OperationKind::Line, locale);
            let fill = form_for(&h, doc, &sheet, OperationKind::Fill, locale);
            let blank = cut_form(None, &CutTop::blank(locale), &CutExtras::new(), OperationKind::Line, locale);
            assert!(!line.spec.shape().is_empty());
            assert_eq!(line.spec.shape(), fill.spec.shape(), "{locale:?}: a mode swaps rows, not the form");
            assert_eq!(
                line.spec.shape(),
                blank.spec.shape(),
                "{locale:?}: the form that is built before the sheet opens"
            );
            for form in [&line, &fill, &blank] {
                assert!(form.spec.problems().is_empty(), "{:?}", form.spec.problems());
            }
            for field in CutField::ALL {
                assert!(line.id(CutTarget::Option(field)).is_some(), "{field:?} has a row");
            }
        }
    }

    #[test]
    fn the_rows_of_the_chosen_mode_are_shown_and_the_rows_of_the_other_modes_hidden() {
        let (h, doc, sheet) = open(MachineCapabilities::conservative());
        let line = form_for(&h, doc, &sheet, OperationKind::Line, Locale::Nl);
        let fill = form_for(&h, doc, &sheet, OperationKind::Fill, Locale::Nl);
        assert!(!row(&line, CutTarget::Option(CutField::TabsEnabled)).hidden);
        assert!(row(&line, CutTarget::Option(CutField::FillAngle)).hidden);
        assert!(row(&fill, CutTarget::Option(CutField::TabsEnabled)).hidden);
        assert!(!row(&fill, CutTarget::Option(CutField::FillAngle)).hidden);
        // Switch off: the values are greyed, not hidden.
        let value = row(&line, CutTarget::Option(CutField::TabsValue));
        assert!(!value.hidden && !value.enabled);
        // Kerf is for Line only.
        assert!(row(&line, CutTarget::Kerf).enabled);
        assert!(!row(&fill, CutTarget::Kerf).enabled);
        // A hidden row says nothing.
        let texts = fill.spec.sections[1].texts();
        assert!(!texts.iter().any(|t| t.starts_with("Tabs")), "{texts:?}");
    }

    #[test]
    fn units_are_suffixes_and_remarks_are_help() {
        let (h, doc, sheet) = open(MachineCapabilities::conservative());
        let form = form_for(&h, doc, &sheet, OperationKind::Line, Locale::Nl);
        let speed = row(&form, CutTarget::Speed);
        assert_eq!((speed.label.as_str(), speed.suffix()), ("Snelheid", Some("mm/s")));
        let min = row(&form, CutTarget::PowerMin);
        assert_eq!((min.label.as_str(), min.suffix()), ("Min. vermogen", Some("%")));
        let kerf = row(&form, CutTarget::Kerf);
        assert_eq!((kerf.label.as_str(), kerf.suffix()), ("Kerfcompensatie", Some("mm")));
        let tabs = row(&form, CutTarget::Option(CutField::TabsValue));
        assert_eq!((tabs.label.as_str(), tabs.suffix()), ("Aantal of afstand", Some("mm")));
        let power = row(&form, CutTarget::Option(CutField::TabsPower));
        assert_eq!(power.label, "Tabvermogen");
        assert_eq!(power.suffix(), Some("%"));
        assert!(power.help.as_deref().is_some_and(|h| h.contains("0 = overslaan")), "{:?}", power.help);
        // A switch keeps its whole caption.
        assert_eq!(row(&form, CutTarget::Option(CutField::DotEnabled)).label, "Dot-modus (puls per afstand)");
    }

    #[test]
    fn a_run_of_rows_with_the_same_note_says_it_once_under_the_first_and_the_others_keep_the_tooltip() {
        let (h, doc, sheet) = open(MachineCapabilities::conservative());
        let form = form_for(&h, doc, &sheet, OperationKind::Line, Locale::Nl);
        let dot = row(&form, CutTarget::Option(CutField::DotEnabled));
        let note = dot.help.clone().unwrap_or_default();
        assert!(note.to_lowercase().contains("ontbrekende capability"), "{note:?}");
        assert!(!dot.enabled);
        let spacing = row(&form, CutTarget::Option(CutField::DotSpacing));
        assert!(spacing.help.as_deref().unwrap_or("").is_empty(), "said once, under the first row of the run");
        assert_eq!(spacing.tooltip.as_deref(), Some(note.as_str()), "the others have it as their tooltip");
        let all: Vec<String> = form.spec.sections.iter().flat_map(|s| s.texts()).collect();
        assert_eq!(all.iter().filter(|t| **t == note).count(), 1, "{all:?}");
    }

    #[test]
    fn ok_is_primary_cancel_cancels_and_a_blocked_mode_is_greyed_with_its_reason() {
        let caps = MachineCapabilities::conservative().with(Feature::OffsetFill, Support::Unsupported);
        let (h, doc, sheet) = open(caps);
        let form = form_for(&h, doc, &sheet, OperationKind::Line, Locale::Nl);
        assert_eq!(form.spec.buttons.primary.as_ref().map(|b| b.title.as_str()), Some("OK"));
        assert_eq!(form.spec.buttons.cancel.as_ref().map(|b| b.title.as_str()), Some("Annuleer"));
        assert_eq!(form.id(CutTarget::Ok), form.spec.buttons.primary.as_ref().map(|b| b.id));
        assert_eq!(form.id(CutTarget::Cancel), form.spec.buttons.cancel.as_ref().map(|b| b.id));
        let mode = row(&form, CutTarget::Mode);
        let Control::Choice { options, selected } = &mode.control else { panic!("{:?}", mode.control) };
        assert_eq!(options.len(), 3);
        assert_eq!(*selected, 0);
        assert!(options[2].to_lowercase().contains("ontbrekende capability"), "{options:?}");
        assert!(
            mode.disabled_options.iter().any(|(i, _)| *i == 2) && mode.disabled_options.iter().all(|(i, _)| *i == 2)
        );
        // The title names the layer; the error of a refused OK is the error of its own row.
        assert!(form.spec.sections[0].heading.as_deref().unwrap_or("").contains("C01"));
        let panel = cut_settings_panel(&h.view(), doc, Locale::Nl).unwrap();
        let mut top = CutTop::opening(&panel);
        top.error = "Doorgangen moet een geheel getal zijn.".into();
        let refused = cut_form(Some(&sheet), &top, &sheet.extras, OperationKind::Line, Locale::Nl);
        assert_eq!(row(&refused, CutTarget::Error).error.as_deref(), Some("Doorgangen moet een geheel getal zijn."));
        assert_eq!(form.spec.shape(), refused.spec.shape(), "an error appearing does not change the shape");
    }
}
