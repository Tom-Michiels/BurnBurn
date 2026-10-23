//! One platform-free description of a form (LF5): sections with a heading, rows with a label, a
//! control and the unit as a suffix, help or an error per row, the reason a section is not
//! available said once per section, and a button row whose buttons have a role. The macOS host
//! builds an `NSGridView` from it, the Windows host a dialog-unit layout; neither decides what the
//! form says. Units come from [`crate::format::Quantity`], so a label never carries one.

use crate::format::{Quantity, unit_label};

/// Identifies a row or a button to the panel model that owns the form.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FormId(pub u32);

/// What a row edits.
#[derive(Debug, Clone, PartialEq)]
pub enum Control {
    Text {
        value: String,
        placeholder: String,
    },
    Number {
        value: String,
    },
    Choice {
        options: Vec<String>,
        selected: usize,
    },
    Check {
        on: bool,
    },
    /// A multi-line text.
    Memo {
        text: String,
    },
    /// A button inside the form (not one of the dialog buttons).
    Action {
        title: String,
    },
    /// A value the user only reads.
    ReadOnly {
        value: String,
    },
}

impl Control {
    fn kind(&self) -> &'static str {
        match self {
            Control::Text { .. } => "text",
            Control::Number { .. } => "number",
            Control::Choice { .. } => "choice",
            Control::Check { .. } => "check",
            Control::Memo { .. } => "memo",
            Control::Action { .. } => "action",
            Control::ReadOnly { .. } => "readonly",
        }
    }
}

/// One labelled row.
#[derive(Debug, Clone, PartialEq)]
pub struct FormRow {
    pub id: FormId,
    /// The caption. It never holds the unit.
    pub label: String,
    pub control: Control,
    /// Shown as a suffix after the control.
    pub unit: Option<Quantity>,
    pub help: Option<String>,
    pub error: Option<String>,
    pub enabled: bool,
    /// Not part of the form now (another mode): the row keeps its place so the form keeps its shape.
    pub hidden: bool,
    /// Said when the pointer rests on the control.
    pub tooltip: Option<String>,
    /// Small buttons after the control (the arrows beside an `Order by` level).
    pub tools: Vec<FormTool>,
    /// Entries of a choice that cannot be picked, with the reason if there is one.
    pub disabled_options: Vec<(usize, Option<String>)>,
}

/// A small button beside the control of a row.
#[derive(Debug, Clone, PartialEq)]
pub struct FormTool {
    pub id: FormId,
    pub title: String,
    /// What a screen reader says.
    pub label: String,
    pub enabled: bool,
}

impl FormTool {
    pub fn new(id: u32, title: &str, label: &str) -> Self {
        FormTool { id: FormId(id), title: title.to_owned(), label: label.to_owned(), enabled: true }
    }
}

impl FormRow {
    pub fn new(id: u32, label: &str, control: Control) -> Self {
        FormRow {
            id: FormId(id),
            label: label.to_owned(),
            control,
            unit: None,
            help: None,
            error: None,
            enabled: true,
            hidden: false,
            tooltip: None,
            tools: Vec::new(),
            disabled_options: Vec::new(),
        }
    }

    pub fn hidden(mut self, hidden: bool) -> Self {
        self.hidden = hidden;
        self
    }

    pub fn tooltip(mut self, text: &str) -> Self {
        self.tooltip = Some(text.to_owned());
        self
    }

    pub fn tool(mut self, tool: FormTool) -> Self {
        self.tools.push(tool);
        self
    }

    pub fn disabled_option(mut self, index: usize, reason: Option<&str>) -> Self {
        self.disabled_options.push((index, reason.map(str::to_owned)));
        self
    }

    pub fn unit(mut self, unit: Quantity) -> Self {
        self.unit = Some(unit);
        self
    }

    pub fn help(mut self, text: &str) -> Self {
        self.help = Some(text.to_owned());
        self
    }

    pub fn error(mut self, text: &str) -> Self {
        self.error = Some(text.to_owned());
        self
    }

    /// The suffix text after the control (`mm`, `°`, `%`).
    pub fn suffix(&self) -> Option<&'static str> {
        self.unit.map(unit_label)
    }
}

/// A group of rows under a heading.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FormSection {
    pub heading: Option<String>,
    /// Why the section cannot be used now; said once for the section, never per row.
    pub unavailable: Option<String>,
    /// Why some rows of the section are greyed; said once under the heading. Unlike
    /// `unavailable` it does not switch the rows off: each row says whether it is enabled.
    /// An empty text is not shown but keeps the shape (the reason may come and go).
    pub reason: Option<String>,
    pub rows: Vec<FormRow>,
}

impl FormSection {
    pub fn new(heading: Option<&str>, rows: Vec<FormRow>) -> Self {
        FormSection { heading: heading.map(str::to_owned), unavailable: None, reason: None, rows }
    }

    pub fn reason(mut self, text: &str) -> Self {
        self.reason = Some(text.to_owned());
        self
    }

    pub fn unavailable(mut self, reason: &str) -> Self {
        self.unavailable = Some(reason.to_owned());
        self
    }

    /// Whether the controls of `row` can be used: the row says so and the section is available.
    pub fn row_enabled(&self, row: &FormRow) -> bool {
        row.enabled && self.unavailable.is_none()
    }

    /// Every text the section shows, in reading order (heading, reason, then the rows).
    pub fn texts(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        out.extend(self.heading.clone());
        out.extend(self.unavailable.clone());
        out.extend(self.reason.clone().filter(|r| !r.is_empty()));
        for row in self.rows.iter().filter(|r| !r.hidden) {
            if !row.label.is_empty() {
                out.push(row.label.clone());
            }
            out.extend(row.help.clone().filter(|t| !t.is_empty()));
            out.extend(row.error.clone().filter(|t| !t.is_empty()));
        }
        out
    }
}

/// The role of a dialog button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ButtonRole {
    Cancel,
    Primary,
    Destructive,
    Help,
}

impl ButtonRole {
    /// The key equivalent of the role: Return for the primary button, Escape for cancel.
    pub fn key(self) -> Option<Key> {
        match self {
            ButtonRole::Primary => Some(Key::Return),
            ButtonRole::Cancel => Some(Key::Escape),
            ButtonRole::Destructive | ButtonRole::Help => None,
        }
    }
}

/// The key that answers a button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Return,
    Escape,
}

/// One button of the button row.
#[derive(Debug, Clone, PartialEq)]
pub struct FormButton {
    pub id: FormId,
    pub title: String,
    pub enabled: bool,
    /// Why the button is greyed, or what it does.
    pub tooltip: Option<String>,
}

impl FormButton {
    pub fn new(id: u32, title: &str) -> Self {
        FormButton { id: FormId(id), title: title.to_owned(), enabled: true, tooltip: None }
    }

    pub fn tooltip(mut self, text: &str) -> Self {
        self.tooltip = Some(text.to_owned());
        self
    }
}

/// Which host orders the buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Mac,
    Windows,
}

/// The buttons of a dialog by role.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DialogButtons {
    pub cancel: Option<FormButton>,
    pub primary: Option<FormButton>,
    pub destructive: Option<FormButton>,
    pub help: Option<FormButton>,
}

impl DialogButtons {
    /// The buttons from left to right in the order `platform` uses: on the Mac help, destructive,
    /// cancel, primary (the primary button is rightmost); on Windows primary, cancel, destructive,
    /// help.
    pub fn order(&self, platform: Platform) -> Vec<(ButtonRole, &FormButton)> {
        let all = [
            (ButtonRole::Help, &self.help),
            (ButtonRole::Destructive, &self.destructive),
            (ButtonRole::Cancel, &self.cancel),
            (ButtonRole::Primary, &self.primary),
        ];
        let mut out: Vec<(ButtonRole, &FormButton)> =
            all.iter().filter_map(|(r, b)| b.as_ref().map(|b| (*r, b))).collect();
        if platform == Platform::Windows {
            let rank = |r: ButtonRole| match r {
                ButtonRole::Primary => 0,
                ButtonRole::Cancel => 1,
                ButtonRole::Destructive => 2,
                ButtonRole::Help => 3,
            };
            out.sort_by_key(|(r, _)| rank(*r));
        }
        out
    }
}

/// What is wrong with a form description.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormProblem {
    /// A label carries the unit that belongs after the control.
    UnitInLabel(FormId),
    /// A row repeats the reason its section is unavailable.
    ReasonRepeated(FormId),
    /// Two rows or buttons share an id.
    DuplicateId(FormId),
}

/// A whole form.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FormSpec {
    pub title: String,
    pub sections: Vec<FormSection>,
    pub buttons: DialogButtons,
}

/// Whether `label` names a unit in brackets or as a trailing word: `Snelheid (mm/s)`, `Hoek [°]`,
/// `Vermogen %`, `Offset in mm`.
fn label_has_unit(label: &str, unit: Quantity) -> bool {
    let u = unit_label(unit);
    let trimmed = label.trim_end();
    let bracketed =
        ['(', '['].iter().any(|open| label.contains(&format!("{open}{u}")) || label.contains(&format!("{open} {u}")));
    let trailing =
        trimmed.strip_suffix(u).is_some_and(|rest| rest.is_empty() || rest.ends_with(' ') || rest.ends_with('('));
    bracketed || trailing
}

impl FormSpec {
    /// Structural faults; an empty list means the description follows the rules.
    pub fn problems(&self) -> Vec<FormProblem> {
        let mut out = Vec::new();
        let mut seen: Vec<FormId> = Vec::new();
        let mut note = |id: FormId, out: &mut Vec<FormProblem>| {
            if seen.contains(&id) {
                if !out.contains(&FormProblem::DuplicateId(id)) {
                    out.push(FormProblem::DuplicateId(id));
                }
            } else {
                seen.push(id);
            }
        };
        for section in &self.sections {
            for row in &section.rows {
                note(row.id, &mut out);
                for tool in &row.tools {
                    note(tool.id, &mut out);
                }
                if row.unit.is_some_and(|u| label_has_unit(&row.label, u)) {
                    out.push(FormProblem::UnitInLabel(row.id));
                }
                if let Some(reason) = section.unavailable.as_ref().or(section.reason.as_ref().filter(|r| !r.is_empty()))
                    && (row.help.as_ref() == Some(reason) || row.error.as_ref() == Some(reason))
                {
                    out.push(FormProblem::ReasonRepeated(row.id));
                }
            }
        }
        for (_, button) in self.buttons.order(Platform::Mac) {
            note(button.id, &mut out);
        }
        out
    }

    /// What the form looks like without its values: the host rebuilds only when this changes.
    pub fn shape(&self) -> Vec<String> {
        let mut out = Vec::new();
        for (i, section) in self.sections.iter().enumerate() {
            out.push(format!(
                "section {i} heading={} unavailable={} reason={}",
                section.heading.is_some(),
                section.unavailable.is_some(),
                section.reason.is_some()
            ));
            for row in &section.rows {
                let options = match &row.control {
                    Control::Choice { options, .. } => options.len(),
                    _ => 0,
                };
                out.push(format!(
                    "row {} {} options={options} unit={} help={} error={} tools={}",
                    row.id.0,
                    row.control.kind(),
                    row.unit.is_some(),
                    row.help.is_some(),
                    row.error.is_some(),
                    row.tools.len()
                ));
            }
        }
        for (role, button) in self.buttons.order(Platform::Mac) {
            out.push(format!("button {} {role:?}", button.id.0));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(id: u32, label: &str) -> FormRow {
        FormRow::new(id, label, Control::Text { value: String::new(), placeholder: String::new() })
    }

    #[test]
    fn a_unit_is_a_suffix_after_the_control_and_not_part_of_the_label() {
        let row = text(1, "Snelheid").unit(Quantity::MillimetersPerSecond);
        assert_eq!(row.suffix(), Some("mm/s"));
        assert_eq!(text(2, "Naam").suffix(), None);
        assert_eq!(text(3, "Hoek").unit(Quantity::Degrees).suffix(), Some("°"));
        assert_eq!(text(4, "Vermogen").unit(Quantity::Percent).suffix(), Some("%"));
    }

    #[test]
    fn a_label_with_a_unit_is_a_problem() {
        let cases = [
            ("Snelheid (mm/s)", Quantity::MillimetersPerSecond),
            ("Breedte [mm]", Quantity::Millimeters),
            ("Hoek (°)", Quantity::Degrees),
            ("Vermogen %", Quantity::Percent),
            ("Offset in mm", Quantity::Millimeters),
            ("Duur (s)", Quantity::Seconds),
        ];
        for (bad, unit) in cases {
            let spec = FormSpec {
                sections: vec![FormSection::new(None, vec![text(7, bad).unit(unit)])],
                ..FormSpec::default()
            };
            assert_eq!(spec.problems(), vec![FormProblem::UnitInLabel(FormId(7))], "{bad}");
        }
        for good in ["Snelheid", "Breedte", "Inmaken", "Min. vermogen", "Pasta", "Seconde-teller"] {
            let spec = FormSpec {
                sections: vec![FormSection::new(None, vec![text(7, good).unit(Quantity::Millimeters)])],
                ..FormSpec::default()
            };
            assert_eq!(spec.problems(), Vec::new(), "{good}");
        }
    }

    #[test]
    fn the_buttons_run_cancel_then_primary_on_the_mac_and_primary_then_cancel_on_windows() {
        let buttons = DialogButtons {
            cancel: Some(FormButton::new(1, "Annuleer")),
            primary: Some(FormButton::new(2, "Pas toe")),
            destructive: Some(FormButton::new(3, "Verwijder")),
            help: Some(FormButton::new(4, "?")),
        };
        let roles = |p| buttons.order(p).into_iter().map(|(r, _)| r).collect::<Vec<_>>();
        use ButtonRole::{Cancel, Destructive, Help, Primary};
        assert_eq!(roles(Platform::Mac), vec![Help, Destructive, Cancel, Primary]);
        assert_eq!(roles(Platform::Windows), vec![Primary, Cancel, Destructive, Help]);
        let only = DialogButtons {
            cancel: Some(FormButton::new(1, "Annuleer")),
            primary: Some(FormButton::new(2, "OK")),
            ..DialogButtons::default()
        };
        let titles: Vec<&str> = only.order(Platform::Mac).into_iter().map(|(_, b)| b.title.as_str()).collect();
        assert_eq!(titles, vec!["Annuleer", "OK"]);
        assert_eq!(only.order(Platform::Mac).last().map(|(r, _)| *r), Some(Primary), "primary is rightmost on the Mac");
    }

    #[test]
    fn return_answers_the_primary_button_and_escape_cancel() {
        assert_eq!(ButtonRole::Primary.key(), Some(Key::Return));
        assert_eq!(ButtonRole::Cancel.key(), Some(Key::Escape));
        assert_eq!(ButtonRole::Destructive.key(), None);
        assert_eq!(ButtonRole::Help.key(), None);
    }

    #[test]
    fn the_reason_a_section_is_unavailable_is_shown_once_and_all_its_rows_are_off() {
        let section =
            FormSection::new(Some("Machine"), vec![text(1, "Snelheid"), text(2, "Vermogen"), text(3, "Doorgangen")])
                .unavailable("Kies eerst een machine");
        let texts = section.texts();
        assert_eq!(texts.iter().filter(|t| *t == "Kies eerst een machine").count(), 1, "{texts:?}");
        assert_eq!(texts.first().map(String::as_str), Some("Machine"));
        assert!(section.rows.iter().all(|r| !section.row_enabled(r)));
        let available = FormSection::new(None, vec![text(1, "Naam")]);
        assert!(available.rows.iter().all(|r| available.row_enabled(r)));
    }

    #[test]
    fn a_row_that_repeats_the_section_reason_is_a_problem() {
        let section = FormSection::new(None, vec![text(1, "Snelheid").help("Kies eerst een machine")])
            .unavailable("Kies eerst een machine");
        let spec = FormSpec { sections: vec![section], ..FormSpec::default() };
        assert_eq!(spec.problems(), vec![FormProblem::ReasonRepeated(FormId(1))]);
    }

    #[test]
    fn duplicate_ids_are_a_problem() {
        let spec = FormSpec {
            sections: vec![FormSection::new(None, vec![text(1, "A"), text(1, "B")])],
            buttons: DialogButtons { primary: Some(FormButton::new(1, "OK")), ..DialogButtons::default() },
            ..FormSpec::default()
        };
        assert!(spec.problems().contains(&FormProblem::DuplicateId(FormId(1))));
    }

    #[test]
    fn the_shape_ignores_values_but_sees_notes_appear() {
        let make = |value: &str, error: Option<&str>| {
            let mut row = FormRow::new(1, "Naam", Control::Text { value: value.into(), placeholder: String::new() });
            row.error = error.map(str::to_owned);
            FormSpec { sections: vec![FormSection::new(None, vec![row])], ..FormSpec::default() }
        };
        assert_eq!(make("a", None).shape(), make("b", None).shape());
        assert_ne!(make("a", None).shape(), make("a", Some("Te lang")).shape());
        assert!(!make("a", None).shape().is_empty());
    }

    #[test]
    fn a_hidden_row_says_nothing_but_keeps_the_shape_of_the_form() {
        let make = |hidden: bool| {
            let section =
                FormSection::new(Some("Tabs"), vec![text(1, "Aantal").help("Per rand"), text(2, "Breedte")]).reason("");
            let mut section = section;
            section.rows[0].hidden = hidden;
            FormSpec { sections: vec![section], ..FormSpec::default() }
        };
        let shown = make(false);
        let hidden = make(true);
        assert_eq!(shown.shape(), hidden.shape(), "hiding a row does not make the host build again");
        assert!(shown.sections[0].texts().contains(&"Aantal".to_owned()));
        let texts = hidden.sections[0].texts();
        assert!(!texts.contains(&"Aantal".to_owned()) && !texts.contains(&"Per rand".to_owned()), "{texts:?}");
        assert!(texts.contains(&"Breedte".to_owned()));
    }

    #[test]
    fn a_section_reason_is_said_once_and_leaves_the_rows_to_say_if_they_are_enabled() {
        let mut off = text(2, "Rotary-diameter");
        off.enabled = false;
        let section =
            FormSection::new(Some("Rotary"), vec![text(1, "Naam"), off]).reason("Zet eerst Rotary gebruiken aan");
        let texts = section.texts();
        assert_eq!(texts.iter().filter(|t| *t == "Zet eerst Rotary gebruiken aan").count(), 1, "{texts:?}");
        assert!(section.row_enabled(&section.rows[0]), "a reason does not switch the other rows off");
        assert!(!section.row_enabled(&section.rows[1]));
        assert!(FormSection::new(None, vec![text(3, "x")]).reason("").texts().iter().all(|t| !t.is_empty()));
        let repeated =
            FormSection::new(None, vec![text(1, "Snelheid").help("Kies een machine")]).reason("Kies een machine");
        let spec = FormSpec { sections: vec![repeated], ..FormSpec::default() };
        assert_eq!(spec.problems(), vec![FormProblem::ReasonRepeated(FormId(1))]);
    }

    #[test]
    fn tools_and_a_reason_belong_to_the_shape_and_tool_ids_may_not_repeat() {
        let plain = FormSpec { sections: vec![FormSection::new(None, vec![text(1, "Laag")])], ..FormSpec::default() };
        let with_tools = FormSpec {
            sections: vec![FormSection::new(
                None,
                vec![text(1, "Laag").tool(FormTool::new(2, "↑", "Omhoog")).tool(FormTool::new(3, "↓", "Omlaag"))],
            )],
            ..FormSpec::default()
        };
        assert_ne!(plain.shape(), with_tools.shape());
        let clash = FormSpec {
            sections: vec![FormSection::new(
                None,
                vec![text(1, "Laag").tool(FormTool::new(2, "↑", "Omhoog")), text(2, "Groep")],
            )],
            ..FormSpec::default()
        };
        assert!(clash.problems().contains(&FormProblem::DuplicateId(FormId(2))));
        let reasoned = FormSpec {
            sections: vec![FormSection::new(None, vec![text(1, "Laag")]).reason("")],
            ..FormSpec::default()
        };
        assert_ne!(plain.shape(), reasoned.shape(), "a section that can show a reason has another shape");
    }
}
