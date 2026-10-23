//! The golf-5 panel lines as a [`FormSpec`] (LF5): the line model stays what the headless tests
//! check, the form is what both hosts build. A Text line after a row is its help, a Problem line
//! its error; the buttons that save or close a panel are the dialog buttons, every other button an
//! action inside the form. The role table below grows panel by panel (LF6).

use super::g5::{G5Control, G5Line};
use crate::form::{ButtonRole, Control, DialogButtons, FormButton, FormId, FormRow, FormSection, FormSpec};

/// A form and which panel control each of its ids is.
#[derive(Debug, Clone, PartialEq)]
pub struct G5Form {
    pub spec: FormSpec,
    pub controls: Vec<(FormId, G5Control)>,
}

/// The dialog role of a button, `None` for a button inside the form.
pub fn dialog_role(control: G5Control) -> Option<ButtonRole> {
    match control {
        G5Control::NotesSave => Some(ButtonRole::Primary),
        G5Control::NotesClose => Some(ButtonRole::Cancel),
        _ => None,
    }
}

/// The form of a panel's lines under `title`. Pictures have no place in a form and are left out.
pub fn g5_form(title: &str, lines: &[G5Line]) -> G5Form {
    let mut rows: Vec<FormRow> = Vec::new();
    let mut controls: Vec<(FormId, G5Control)> = Vec::new();
    let mut buttons = DialogButtons::default();
    let mut next = 0_u32;
    let mut take_id = |control: Option<G5Control>, controls: &mut Vec<(FormId, G5Control)>| {
        next += 1;
        if let Some(c) = control {
            controls.push((FormId(next), c));
        }
        next
    };
    for line in lines {
        match line {
            G5Line::Heading(text) => {
                // A heading is a row without a control: a read-only line in bold is the host's job.
                let id = take_id(None, &mut controls);
                rows.push(FormRow::new(id, "", Control::ReadOnly { value: text.clone() }));
            }
            G5Line::Text(text) => match rows.last_mut() {
                Some(row) if row.help.is_none() && !matches!(row.control, Control::ReadOnly { .. }) => {
                    row.help = Some(text.clone());
                }
                _ => {
                    let id = take_id(None, &mut controls);
                    rows.push(FormRow::new(id, "", Control::ReadOnly { value: text.clone() }));
                }
            },
            G5Line::Problem(text) => match rows.last_mut() {
                Some(row) if row.error.is_none() => row.error = Some(text.clone()),
                _ => {
                    let id = take_id(None, &mut controls);
                    let mut row = FormRow::new(id, "", Control::ReadOnly { value: String::new() });
                    row.error = Some(text.clone());
                    rows.push(row);
                }
            },
            G5Line::Memo { control, text, .. } => {
                let id = take_id(Some(*control), &mut controls);
                rows.push(FormRow::new(id, "", Control::Memo { text: text.clone() }));
            }
            G5Line::Field { control, caption, text } => {
                let id = take_id(Some(*control), &mut controls);
                rows.push(FormRow::new(id, caption, Control::Text { value: text.clone(), placeholder: String::new() }));
            }
            G5Line::Choice { control, caption, options, selected } => {
                let id = take_id(Some(*control), &mut controls);
                rows.push(FormRow::new(id, caption, Control::Choice { options: options.clone(), selected: *selected }));
            }
            G5Line::Check { control, caption, on } => {
                let id = take_id(Some(*control), &mut controls);
                rows.push(FormRow::new(id, caption, Control::Check { on: *on }));
            }
            G5Line::Button { control, title, enabled } => {
                let id = take_id(Some(*control), &mut controls);
                match dialog_role(*control) {
                    Some(role) => {
                        let mut button = FormButton::new(id, title);
                        button.enabled = *enabled;
                        match role {
                            ButtonRole::Primary => buttons.primary = Some(button),
                            ButtonRole::Cancel => buttons.cancel = Some(button),
                            ButtonRole::Destructive => buttons.destructive = Some(button),
                            ButtonRole::Help => buttons.help = Some(button),
                        }
                    }
                    None => {
                        let mut row = FormRow::new(id, "", Control::Action { title: title.clone() });
                        row.enabled = *enabled;
                        rows.push(row);
                    }
                }
            }
            G5Line::Image(_) => {}
        }
    }
    G5Form {
        spec: FormSpec { title: title.to_owned(), sections: vec![FormSection::new(None, rows)], buttons },
        controls,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
