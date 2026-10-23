//! The Schikken panel lines as a [`FormSpec`] (LF6c), the way [`super::g5_form`] does for the
//! golf-5 panels. The flows that end in an answer (slots, nest, print and cut, camera
//! registration) have a primary button that does it and a cancel button that closes; every other
//! button acts on its own section.

use super::schikken::{SchikkenControl, SchikkenLine};
use crate::form::{ButtonRole, FormId, FormSpec};
use crate::form_lines::{FormLine, lines_form};

/// A form and which panel control each of its ids is.
#[derive(Debug, Clone, PartialEq)]
pub struct SchikkenForm {
    pub spec: FormSpec,
    pub controls: Vec<(FormId, SchikkenControl)>,
}

/// The dialog role of a button, `None` for a button inside the form.
pub fn schikken_dialog_role(control: SchikkenControl) -> Option<ButtonRole> {
    match control {
        SchikkenControl::SlotsOk | SchikkenControl::NestApply | SchikkenControl::PcRegister => {
            Some(ButtonRole::Primary)
        }
        SchikkenControl::SlotsCancel
        | SchikkenControl::NestClose
        | SchikkenControl::PcClose
        | SchikkenControl::CfClose => Some(ButtonRole::Cancel),
        _ => None,
    }
}

fn line_of(line: &SchikkenLine) -> FormLine<SchikkenControl> {
    match line {
        SchikkenLine::Heading(t) => FormLine::Heading(t.clone()),
        SchikkenLine::Text(t) => FormLine::Text(t.clone()),
        SchikkenLine::Problem(t) => FormLine::Problem(t.clone()),
        SchikkenLine::Field { control, caption, text, invalid } => FormLine::Field {
            control: *control,
            caption: caption.clone(),
            text: text.clone(),
            invalid: *invalid,
        },
        SchikkenLine::Choice { control, caption, options, selected } => FormLine::Choice {
            control: *control,
            caption: caption.clone(),
            options: options.clone(),
            selected: *selected,
        },
        SchikkenLine::Check { control, caption, on } => {
            FormLine::Check { control: *control, caption: caption.clone(), on: *on }
        }
        SchikkenLine::Button { control, title, enabled } => {
            FormLine::Button { control: *control, title: title.clone(), enabled: *enabled }
        }
    }
}

/// The form of the panel's lines under `title`.
pub fn schikken_form(title: &str, lines: &[SchikkenLine]) -> SchikkenForm {
    let lines: Vec<FormLine<SchikkenControl>> = lines.iter().map(line_of).collect();
    let form = lines_form(title, &lines, schikken_dialog_role);
    SchikkenForm { spec: form.spec, controls: form.controls }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::form::Platform;
    use crate::format::Quantity;

    fn button(c: SchikkenControl, t: &str) -> SchikkenLine {
        SchikkenLine::Button { control: c, title: t.into(), enabled: true }
    }

    fn titles(form: &SchikkenForm) -> Vec<&str> {
        form.spec.buttons.order(Platform::Mac).into_iter().map(|(_, b)| b.title.as_str()).collect()
    }

    #[test]
    fn the_ending_flows_have_cancel_left_and_the_answer_right() {
        for (lines, expected) in [
            (vec![button(SchikkenControl::SlotsOk, "Pas toe"), button(SchikkenControl::SlotsCancel, "Annuleer")], vec!["Annuleer", "Pas toe"]),
            (vec![button(SchikkenControl::NestApply, "Plaats"), button(SchikkenControl::NestClose, "Sluit")], vec!["Sluit", "Plaats"]),
            (vec![button(SchikkenControl::PcRegister, "Registreer"), button(SchikkenControl::PcClose, "Sluit")], vec!["Sluit", "Registreer"]),
            (vec![button(SchikkenControl::CfAdd, "Voeg toe"), button(SchikkenControl::CfClose, "Sluit")], vec!["Sluit"]),
        ] {
            let form = schikken_form("Schikken", &lines);
            assert_eq!(titles(&form), expected);
        }
    }

    #[test]
    fn nest_start_and_stop_act_in_the_form_next_to_their_fields() {
        let lines = vec![
            SchikkenLine::Heading("Nesten".into()),
            SchikkenLine::Field { control: SchikkenControl::NestPadding, caption: "Tussenruimte (mm)".into(), text: "2".into(), invalid: true },
            button(SchikkenControl::NestStart, "Start"),
            button(SchikkenControl::NestStop, "Stop"),
        ];
        let form = schikken_form("Schikken", &lines);
        assert!(form.spec.buttons.primary.is_none() && form.spec.buttons.cancel.is_none());
        let rows = &form.spec.sections[0].rows;
        assert_eq!((rows[0].label.as_str(), rows[0].unit, rows[0].invalid), ("Tussenruimte", Some(Quantity::Millimeters), true));
        assert_eq!(rows.len(), 3);
        assert_eq!(form.spec.problems(), Vec::new());
    }
}
