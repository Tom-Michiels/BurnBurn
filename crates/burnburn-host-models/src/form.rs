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
    /// Read-only text that scrolls (a queue, a list of support lines).
    Log {
        text: String,
    },
    /// A small picture (the preview of a found project).
    Picture {
        image: crate::panels_image::BitmapImage,
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
            Control::Log { .. } => "log",
            Control::Picture { .. } => "picture",
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
    /// The value is refused: the caption is drawn in the error colour.
    pub invalid: bool,
    pub enabled: bool,
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
            invalid: false,
            enabled: true,
        }
    }

    pub fn invalid(mut self, invalid: bool) -> Self {
        self.invalid = invalid;
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
    /// The tab the section lies on; `None` for a form without tabs. Consecutive sections with the
    /// same title share a tab.
    pub tab: Option<String>,
    pub rows: Vec<FormRow>,
}

impl FormSection {
    pub fn new(heading: Option<&str>, rows: Vec<FormRow>) -> Self {
        FormSection { heading: heading.map(str::to_owned), unavailable: None, tab: None, rows }
    }

    /// Puts the section on the tab `title`.
    pub fn on_tab(mut self, title: &str) -> Self {
        self.tab = Some(title.to_owned());
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
        for row in &self.rows {
            if !row.label.is_empty() {
                out.push(row.label.clone());
            }
            out.extend(row.help.clone());
            out.extend(row.error.clone());
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
}

impl FormButton {
    pub fn new(id: u32, title: &str) -> Self {
        FormButton { id: FormId(id), title: title.to_owned(), enabled: true }
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
    /// One notice for the whole form, shown above the buttons whatever tab is open (a refused
    /// value, a read problem).
    pub notice: Option<String>,
    /// The room for the notice is kept even while there is none, so a notice that comes and goes
    /// does not change the shape of the form (the host would build it again under the user's hands).
    pub notice_slot: bool,
    pub buttons: DialogButtons,
}

/// A caption split into the words that name the field, the unit that belongs after the control and
/// what is left of the brackets (a hint such as `0 = uit`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplitCaption {
    pub label: String,
    pub unit: Option<Quantity>,
    pub hint: Option<String>,
}

/// The unit a bracket word names, in the words of both languages.
fn unit_of_word(word: &str) -> Option<Quantity> {
    Some(match word.trim() {
        "mm" => Quantity::Millimeters,
        "inch" => Quantity::Inches,
        "°" | "graden" | "degrees" => Quantity::Degrees,
        "%" => Quantity::Percent,
        "s" | "seconden" | "seconds" => Quantity::Seconds,
        "mm/s" => Quantity::MillimetersPerSecond,
        "px" | "pixels" => Quantity::Pixels,
        _ => return None,
    })
}

/// Takes the unit out of a caption: `Tussenruimte (mm)` is `Tussenruimte` with `mm`,
/// `Automatisch bewaren (s, 0 = uit)` is `Automatisch bewaren` with `s` and the hint `0 = uit`.
/// Brackets that do not start with a unit stay in the label.
pub fn split_caption(caption: &str) -> SplitCaption {
    let whole = |label: &str| SplitCaption { label: label.to_owned(), unit: None, hint: None };
    let trimmed = caption.trim_end();
    let Some(body) = trimmed.strip_suffix(')') else { return whole(caption) };
    let Some(open) = body.rfind('(') else { return whole(caption) };
    let (head, inside) = (body[..open].trim_end(), &body[open + 1..]);
    let (word, rest) = match inside.split_once(',') {
        Some((w, r)) => (w, Some(r.trim().to_owned())),
        None => (inside, None),
    };
    match unit_of_word(word) {
        Some(unit) if !head.is_empty() => SplitCaption { label: head.to_owned(), unit: Some(unit), hint: rest },
        _ => whole(caption),
    }
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
    /// The tab titles in order of appearance; empty for a form without tabs.
    pub fn tabs(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for tab in self.sections.iter().filter_map(|s| s.tab.as_ref()) {
            if !out.contains(tab) {
                out.push(tab.clone());
            }
        }
        out
    }

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
                if row.unit.is_some_and(|u| label_has_unit(&row.label, u)) {
                    out.push(FormProblem::UnitInLabel(row.id));
                }
                if let Some(reason) = &section.unavailable
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
        let mut out = vec![format!("notice={}", self.notice_slot || self.notice.is_some())];
        for (i, section) in self.sections.iter().enumerate() {
            out.push(format!(
                "section {i} heading={} unavailable={} tab={}",
                section.heading.is_some(),
                section.unavailable.is_some(),
                section.tab.as_deref().unwrap_or("-")
            ));
            for row in &section.rows {
                let options = match &row.control {
                    Control::Choice { options, .. } => options.len(),
                    _ => 0,
                };
                out.push(format!(
                    "row {} {} options={options} unit={} help={} error={}",
                    row.id.0,
                    row.control.kind(),
                    row.unit.is_some(),
                    row.help.is_some(),
                    row.error.is_some()
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

    #[test]
    fn a_unit_in_brackets_leaves_the_caption_and_becomes_the_suffix() {
        let cases = [
            ("Tussenruimte (mm)", "Tussenruimte", Some(Quantity::Millimeters), None),
            ("Snelheid (mm/s)", "Snelheid", Some(Quantity::MillimetersPerSecond), None),
            ("Vermogen (%)", "Vermogen", Some(Quantity::Percent), None),
            ("Cirkel: hoek (graden)", "Cirkel: hoek", Some(Quantity::Degrees), None),
            ("Vangstraal objecten (px)", "Vangstraal objecten", Some(Quantity::Pixels), None),
            ("Automatisch bewaren (s, 0 = uit)", "Automatisch bewaren", Some(Quantity::Seconds), Some("0 = uit")),
            ("Afstand (mm, positief vergroot)", "Afstand", Some(Quantity::Millimeters), Some("positief vergroot")),
            ("Sheet grain (°, blank = none)", "Sheet grain", Some(Quantity::Degrees), Some("blank = none")),
        ];
        for (caption, label, unit, hint) in cases {
            let split = split_caption(caption);
            assert_eq!(split.label, label, "{caption}");
            assert_eq!(split.unit, unit, "{caption}");
            assert_eq!(split.hint.as_deref(), hint, "{caption}");
        }
    }

    #[test]
    fn brackets_that_are_not_a_unit_stay_in_the_caption() {
        for caption in [
            "Afstand (negatief = naar binnen)",
            "Datum (JJJJ-MM-DD, leeg is vandaag)",
            "Kleinste leesbare module (mm of lpi)",
            "Naam",
            "",
            "(mm)",
        ] {
            let split = split_caption(caption);
            assert_eq!(split, SplitCaption { label: caption.to_owned(), unit: None, hint: None }, "{caption}");
        }
    }

    #[test]
    fn consecutive_sections_with_one_title_make_one_tab_in_order() {
        let sec = |tab: Option<&str>| {
            let s = FormSection::new(None, vec![text(1, "A")]);
            match tab {
                Some(t) => s.on_tab(t),
                None => s,
            }
        };
        let spec = FormSpec {
            sections: vec![sec(Some("Algemeen")), sec(Some("Algemeen")), sec(Some("Canvas")), sec(Some("Algemeen"))],
            ..FormSpec::default()
        };
        assert_eq!(spec.tabs(), vec!["Algemeen".to_owned(), "Canvas".to_owned()]);
        assert!(FormSpec { sections: vec![sec(None)], ..FormSpec::default() }.tabs().is_empty());
    }

    #[test]
    fn the_shape_sees_a_section_move_to_another_tab() {
        let make = |tab: &str| FormSpec {
            sections: vec![FormSection::new(None, vec![text(1, "A")]).on_tab(tab)],
            ..FormSpec::default()
        };
        assert_ne!(make("Algemeen").shape(), make("Canvas").shape());
    }

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
}
