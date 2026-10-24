//! The golf-5 panel lines as a [`FormSpec`] (LF5, LF6c): the line model stays what the headless
//! tests check, the form is what both hosts build (see [`crate::form_lines`]). The buttons that
//! save, start or close a panel are the dialog buttons (the role table below), every other button
//! an action inside the form.

use super::g5::{G5Control, G5Line};
use crate::form::{ButtonRole, FormId, FormSpec};
use crate::form_lines::{FormLine, lines_form};

/// A form and which panel control each of its ids is.
#[derive(Debug, Clone, PartialEq)]
pub struct G5Form {
    pub spec: FormSpec,
    pub controls: Vec<(FormId, G5Control)>,
}

/// The dialog role of a button, `None` for a button inside the form. A panel with two groups of
/// preview/apply buttons (offset and repair) has no dialog button for them: each button acts on
/// its own section.
pub fn dialog_role(control: G5Control) -> Option<ButtonRole> {
    match control {
        G5Control::NotesSave | G5Control::GeneratorInsert | G5Control::StartConfirm => Some(ButtonRole::Primary),
        G5Control::NotesClose | G5Control::GeneratorCancel | G5Control::StartCancel | G5Control::SearchClose => {
            Some(ButtonRole::Cancel)
        }
        _ => None,
    }
}

fn line_of(line: &G5Line) -> FormLine<G5Control> {
    match line {
        G5Line::Heading(t) => FormLine::Heading(t.clone()),
        G5Line::Text(t) => FormLine::Text(t.clone()),
        G5Line::Problem(t) => FormLine::Problem(t.clone()),
        G5Line::Unavailable(t) => FormLine::Unavailable(t.clone()),
        G5Line::Memo { control, text, .. } => FormLine::Memo { control: *control, text: text.clone() },
        G5Line::Field { control, caption, text } => {
            FormLine::Field { control: *control, caption: caption.clone(), text: text.clone(), invalid: false }
        }
        G5Line::Choice { control, caption, options, selected } => FormLine::Choice {
            control: *control,
            caption: caption.clone(),
            options: options.clone(),
            selected: *selected,
        },
        G5Line::Check { control, caption, on } => {
            FormLine::Check { control: *control, caption: caption.clone(), on: *on }
        }
        G5Line::Button { control, title, enabled } => {
            FormLine::Button { control: *control, title: title.clone(), enabled: *enabled }
        }
        G5Line::Image(image) => FormLine::Image(image.clone()),
        G5Line::EmptyState { symbol, text } => FormLine::Empty { symbol: (*symbol).to_owned(), text: text.clone() },
    }
}

/// The form of a panel's lines under `title`.
pub fn g5_form(title: &str, lines: &[G5Line]) -> G5Form {
    let lines: Vec<FormLine<G5Control>> = lines.iter().map(line_of).collect();
    let form = lines_form(title, &lines, dialog_role);
    G5Form { spec: form.spec, controls: form.controls }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::form::{Control, FormRow};

    fn notes_lines(too_long: bool) -> Vec<G5Line> {
        let mut lines = vec![
            G5Line::Memo { control: G5Control::NotesText, text: "Klant Jansen".into(), max_chars: 20_000 },
            G5Line::Text("12 van 20000 tekens".into()),
        ];
        if too_long {
            lines.push(G5Line::Problem("Te lang".into()));
        }
        lines.push(G5Line::Button {
            control: G5Control::NotesSave,
            title: "Bewaar notities".into(),
            enabled: !too_long,
        });
        lines.push(G5Line::Field {
            control: G5Control::NotesPartName,
            caption: "Onderdeelnaam".into(),
            text: "Deur".into(),
        });
        lines.push(G5Line::Button {
            control: G5Control::NotesPartNameSave,
            title: "Bewaar onderdeelnaam".into(),
            enabled: true,
        });
        lines.push(G5Line::Button { control: G5Control::NotesClose, title: "Sluit".into(), enabled: true });
        lines
    }

    fn control_of(form: &G5Form, c: G5Control) -> Option<FormId> {
        form.controls.iter().find(|(_, k)| *k == c).map(|(id, _)| *id)
    }

    #[test]
    fn the_notes_panel_becomes_a_form_with_close_and_save_as_its_dialog_buttons() {
        let form = g5_form("Notities", &notes_lines(false));
        assert_eq!(form.spec.title, "Notities");
        let titles: Vec<&str> =
            form.spec.buttons.order(crate::form::Platform::Mac).into_iter().map(|(_, b)| b.title.as_str()).collect();
        assert_eq!(titles, vec!["Sluit", "Bewaar notities"], "cancel, then primary");
        assert_eq!(form.spec.problems(), Vec::new());
        let rows: Vec<&FormRow> = form.spec.sections.iter().flat_map(|s| &s.rows).collect();
        assert!(matches!(rows[0].control, Control::Memo { ref text } if text == "Klant Jansen"));
        assert_eq!(rows[0].help.as_deref(), Some("12 van 20000 tekens"));
        assert_eq!(rows[1].label, "Onderdeelnaam");
        assert!(matches!(rows[2].control, Control::Action { ref title } if title == "Bewaar onderdeelnaam"));
        assert_eq!(rows.len(), 3);
    }

    #[test]
    fn every_control_of_the_lines_keeps_its_panel_control() {
        let form = g5_form("Notities", &notes_lines(false));
        for c in [
            G5Control::NotesText,
            G5Control::NotesPartName,
            G5Control::NotesPartNameSave,
            G5Control::NotesSave,
            G5Control::NotesClose,
        ] {
            assert!(control_of(&form, c).is_some(), "{c:?}");
        }
        let mut ids: Vec<u32> = form.controls.iter().map(|(id, _)| id.0).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), form.controls.len(), "ids are unique");
    }

    #[test]
    fn a_problem_line_is_the_error_of_the_row_above_and_switches_save_off() {
        let form = g5_form("Notities", &notes_lines(true));
        let memo = &form.spec.sections[0].rows[0];
        assert_eq!(memo.error.as_deref(), Some("Te lang"));
        assert_eq!(form.spec.buttons.primary.as_ref().map(|b| b.enabled), Some(false));
        assert_eq!(form.spec.buttons.cancel.as_ref().map(|b| b.enabled), Some(true));
    }

    #[test]
    fn the_generator_start_confirmation_and_search_panels_have_dialog_buttons() {
        use crate::form::Platform;
        let button = |c: G5Control, t: &str| G5Line::Button { control: c, title: t.into(), enabled: true };
        for (lines, expected) in [
            (
                vec![button(G5Control::GeneratorInsert, "Voeg in"), button(G5Control::GeneratorCancel, "Annuleer")],
                vec!["Annuleer", "Voeg in"],
            ),
            (
                vec![button(G5Control::StartConfirm, "Start"), button(G5Control::StartCancel, "Niet starten")],
                vec!["Niet starten", "Start"],
            ),
            (
                vec![button(G5Control::SearchOpenProject, "Open project"), button(G5Control::SearchClose, "Sluit")],
                vec!["Sluit"],
            ),
        ] {
            let form = g5_form("T", &lines);
            let titles: Vec<&str> =
                form.spec.buttons.order(Platform::Mac).into_iter().map(|(_, b)| b.title.as_str()).collect();
            assert_eq!(titles, expected);
        }
    }

    #[test]
    fn the_offset_and_repair_buttons_stay_actions_in_their_own_section() {
        let lines = vec![
            G5Line::Heading("Offset".into()),
            G5Line::Field { control: G5Control::OffsetDistance, caption: "Afstand (mm)".into(), text: "1".into() },
            G5Line::Button { control: G5Control::OffsetPreview, title: "Voorbeeld".into(), enabled: true },
            G5Line::Button { control: G5Control::OffsetApply, title: "Pas toe".into(), enabled: true },
            G5Line::Heading("Reparatie".into()),
            G5Line::Button { control: G5Control::RepairApply, title: "Repareer".into(), enabled: false },
        ];
        let form = g5_form("Offset en reparatie", &lines);
        assert_eq!(form.spec.sections.len(), 2);
        assert_eq!(form.spec.sections[0].heading.as_deref(), Some("Offset"));
        assert_eq!(form.spec.sections[0].rows.len(), 3);
        assert_eq!(form.spec.sections[0].rows[0].label, "Afstand");
        assert!(form.spec.buttons.primary.is_none());
        assert!(!form.spec.sections[1].rows[0].enabled, "a disabled button stays disabled");
        assert_eq!(form.spec.problems(), Vec::new());
    }
}
