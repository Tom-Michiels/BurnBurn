//! Panel lines as a [`FormSpec`] (LF6c): the golf-5 panels and the Schikken panel are lists of
//! lines (heading, text, problem, field, choice, check, button). This turns such a list into the
//! form both hosts build. A heading starts a section; a Text line after a row is its help, a
//! Problem line its error; a unit in brackets at the end of a caption becomes the suffix of the
//! row; the buttons that answer or close a panel are the dialog buttons (one per role), every other
//! button an action inside the form. Pictures are rows of their own.

use crate::form::{
    ButtonRole, Control, DialogButtons, FormButton, FormId, FormRow, FormSection, FormSpec, split_caption,
};
use crate::panels_image::BitmapImage;

/// One line of a panel, with the panel's own control type `C`.
#[derive(Debug, Clone, PartialEq)]
pub enum FormLine<C> {
    Heading(String),
    Text(String),
    Problem(String),
    Memo { control: C, text: String },
    Field { control: C, caption: String, text: String, invalid: bool },
    Choice { control: C, caption: String, options: Vec<String>, selected: usize },
    Check { control: C, caption: String, on: bool },
    Button { control: C, title: String, enabled: bool },
    Image(BitmapImage),
}

/// A form and which panel control each of its ids is.
#[derive(Debug, Clone, PartialEq)]
pub struct LinesForm<C> {
    pub spec: FormSpec,
    pub controls: Vec<(FormId, C)>,
}

/// The form of `lines` under `title`; `role` says which controls are dialog buttons.
pub fn lines_form<C: Copy>(title: &str, lines: &[FormLine<C>], role: impl Fn(C) -> Option<ButtonRole>) -> LinesForm<C> {
    let mut sections: Vec<FormSection> = Vec::new();
    let mut current = FormSection::new(None, Vec::new());
    let mut controls: Vec<(FormId, C)> = Vec::new();
    let mut buttons = DialogButtons::default();
    let mut next = 0_u32;
    let mut take_id = |control: Option<C>, controls: &mut Vec<(FormId, C)>| {
        next += 1;
        if let Some(c) = control {
            controls.push((FormId(next), c));
        }
        next
    };
    for line in lines {
        match line {
            FormLine::Heading(text) => {
                if current.heading.is_some() || !current.rows.is_empty() {
                    sections.push(std::mem::take(&mut current));
                }
                current.heading = Some(text.clone());
            }
            FormLine::Text(text) => match current.rows.last_mut() {
                Some(row) if row.help.is_none() && !matches!(row.control, Control::ReadOnly { .. }) => {
                    row.help = Some(text.clone());
                }
                _ => {
                    let id = take_id(None, &mut controls);
                    current.rows.push(FormRow::new(id, "", Control::ReadOnly { value: text.clone() }));
                }
            },
            FormLine::Problem(text) => match current.rows.last_mut() {
                Some(row) if row.error.is_none() => row.error = Some(text.clone()),
                _ => {
                    let id = take_id(None, &mut controls);
                    let mut row = FormRow::new(id, "", Control::ReadOnly { value: String::new() });
                    row.error = Some(text.clone());
                    current.rows.push(row);
                }
            },
            FormLine::Memo { control, text } => {
                let id = take_id(Some(*control), &mut controls);
                current.rows.push(FormRow::new(id, "", Control::Memo { text: text.clone() }));
            }
            FormLine::Field { control, caption, text, invalid } => {
                let id = take_id(Some(*control), &mut controls);
                let split = split_caption(caption);
                let mut row =
                    FormRow::new(id, &split.label, Control::Text { value: text.clone(), placeholder: String::new() })
                        .invalid(*invalid);
                row.unit = split.unit;
                row.help = split.hint;
                current.rows.push(row);
            }
            FormLine::Choice { control, caption, options, selected } => {
                let id = take_id(Some(*control), &mut controls);
                let split = split_caption(caption);
                let mut row =
                    FormRow::new(id, &split.label, Control::Choice { options: options.clone(), selected: *selected });
                row.unit = split.unit;
                row.help = split.hint;
                current.rows.push(row);
            }
            FormLine::Check { control, caption, on } => {
                let id = take_id(Some(*control), &mut controls);
                current.rows.push(FormRow::new(id, caption, Control::Check { on: *on }));
            }
            FormLine::Button { control, title, enabled } => {
                let id = take_id(Some(*control), &mut controls);
                let slot = role(*control).and_then(|r| {
                    let slot = match r {
                        ButtonRole::Primary => &mut buttons.primary,
                        ButtonRole::Cancel => &mut buttons.cancel,
                        ButtonRole::Destructive => &mut buttons.destructive,
                        ButtonRole::Help => &mut buttons.help,
                    };
                    slot.is_none().then_some(slot)
                });
                match slot {
                    Some(slot) => {
                        let mut button = FormButton::new(id, title);
                        button.enabled = *enabled;
                        *slot = Some(button);
                    }
                    None => {
                        let mut row = FormRow::new(id, "", Control::Action { title: title.clone() });
                        row.enabled = *enabled;
                        current.rows.push(row);
                    }
                }
            }
            FormLine::Image(image) => {
                let id = take_id(None, &mut controls);
                current.rows.push(FormRow::new(id, "", Control::Picture { image: image.clone() }));
            }
        }
    }
    if current.heading.is_some() || !current.rows.is_empty() {
        sections.push(current);
    }
    if sections.is_empty() {
        sections.push(FormSection::default());
    }
    LinesForm { spec: FormSpec { title: title.to_owned(), sections, buttons, ..FormSpec::default() }, controls }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::Quantity;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum C {
        Gap,
        Kind,
        Mirror,
        Ok,
        Cancel,
        Other,
        Again,
    }

    fn role(c: C) -> Option<ButtonRole> {
        match c {
            C::Ok => Some(ButtonRole::Primary),
            C::Cancel => Some(ButtonRole::Cancel),
            _ => None,
        }
    }

    fn field(c: C, caption: &str) -> FormLine<C> {
        FormLine::Field { control: c, caption: caption.into(), text: "1".into(), invalid: false }
    }

    fn button(c: C, title: &str) -> FormLine<C> {
        FormLine::Button { control: c, title: title.into(), enabled: true }
    }

    fn rows(form: &LinesForm<C>) -> Vec<&FormRow> {
        form.spec.sections.iter().flat_map(|s| &s.rows).collect()
    }

    #[test]
    fn a_heading_starts_a_section_and_lines_before_it_form_a_section_without_a_heading() {
        let lines = vec![
            field(C::Gap, "Tussenruimte"),
            FormLine::Heading("Plaat".into()),
            field(C::Kind, "Soort"),
            FormLine::Heading("Uitvoer".into()),
            FormLine::Check { control: C::Mirror, caption: "Spiegelen".into(), on: true },
        ];
        let form = lines_form("Schikken", &lines, role);
        let headings: Vec<Option<&str>> = form.spec.sections.iter().map(|s| s.heading.as_deref()).collect();
        assert_eq!(headings, vec![None, Some("Plaat"), Some("Uitvoer")]);
        assert_eq!(form.spec.sections.iter().map(|s| s.rows.len()).collect::<Vec<_>>(), vec![1, 1, 1]);
        assert_eq!(form.spec.title, "Schikken");
    }

    #[test]
    fn a_unit_in_the_caption_is_the_suffix_and_the_form_follows_the_rules() {
        let lines = vec![field(C::Gap, "Tussenruimte (mm)"), field(C::Kind, "Hoek (°, leeg = geen)")];
        let form = lines_form("Schikken", &lines, role);
        let rows = rows(&form);
        assert_eq!((rows[0].label.as_str(), rows[0].unit), ("Tussenruimte", Some(Quantity::Millimeters)));
        assert_eq!((rows[1].label.as_str(), rows[1].unit), ("Hoek", Some(Quantity::Degrees)));
        assert_eq!(rows[1].help.as_deref(), Some("Leeg = geen"));
        assert_eq!(form.spec.problems(), Vec::new());
    }

    #[test]
    fn text_is_the_help_of_the_row_above_and_a_problem_its_error() {
        let lines = vec![
            field(C::Gap, "Tussenruimte"),
            FormLine::Text("Tussen twee delen".into()),
            FormLine::Problem("Geen getal".into()),
        ];
        let form = lines_form("T", &lines, role);
        let row = &form.spec.sections[0].rows[0];
        assert_eq!(row.help.as_deref(), Some("Tussen twee delen"));
        assert_eq!(row.error.as_deref(), Some("Geen getal"));
        assert_eq!(form.spec.sections[0].rows.len(), 1);
    }

    #[test]
    fn a_refused_field_marks_its_row_invalid() {
        let lines = vec![FormLine::Field { control: C::Gap, caption: "Gap".into(), text: "x".into(), invalid: true }];
        let form = lines_form("T", &lines, role);
        assert!(form.spec.sections[0].rows[0].invalid);
    }

    #[test]
    fn buttons_with_a_role_are_dialog_buttons_and_the_rest_actions_in_the_form() {
        let lines = vec![button(C::Other, "Voorbeeld"), button(C::Cancel, "Sluit"), button(C::Ok, "Pas toe")];
        let form = lines_form("T", &lines, role);
        let titles: Vec<&str> =
            form.spec.buttons.order(crate::form::Platform::Mac).into_iter().map(|(_, b)| b.title.as_str()).collect();
        assert_eq!(titles, vec!["Sluit", "Pas toe"]);
        let rows = rows(&form);
        assert_eq!(rows.len(), 1);
        assert!(matches!(&rows[0].control, Control::Action { title } if title == "Voorbeeld"));
        for c in [C::Other, C::Cancel, C::Ok] {
            assert!(form.controls.iter().any(|(_, k)| *k == c), "{c:?}");
        }
    }

    #[test]
    fn a_second_button_for_a_taken_role_becomes_an_action_instead_of_vanishing() {
        let lines = vec![button(C::Ok, "Eerste"), button(C::Ok, "Tweede")];
        let form = lines_form("T", &lines, role);
        assert_eq!(form.spec.buttons.primary.as_ref().map(|b| b.title.as_str()), Some("Eerste"));
        let rows = rows(&form);
        assert_eq!(rows.len(), 1);
        assert!(matches!(&rows[0].control, Control::Action { title } if title == "Tweede"));
        assert_eq!(form.controls.len(), 2);
    }

    #[test]
    fn a_picture_is_a_row_of_its_own_and_ids_stay_unique() {
        let image = BitmapImage { width: 1, height: 1, rgba: vec![0, 0, 0, 255] };
        let lines = vec![
            FormLine::Heading("Gevonden".into()),
            FormLine::Image(image.clone()),
            button(C::Again, "Nog eens"),
            button(C::Ok, "OK"),
        ];
        let form = lines_form("T", &lines, role);
        assert!(matches!(&form.spec.sections[0].rows[0].control, Control::Picture { image: i } if *i == image));
        assert_eq!(form.spec.problems(), Vec::new());
        let mut ids: Vec<u32> = form.controls.iter().map(|(id, _)| id.0).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), form.controls.len());
    }
}
