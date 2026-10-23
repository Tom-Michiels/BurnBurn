//! The forms LF6c added to the shared models (preferences with tabs, machine hub with tabs, the
//! Schikken and golf-5 panels as sections) compile in the Windows host and keep the rules the
//! Windows layout (LF13) will rely on: tabs in order, a unit never inside a caption, the Windows
//! button order. Written after the models (they test existing behaviour); the red phases are in
//! `docs/dev-notes/LF6c.md`.

use burnburn_host_models::app_view::prefs_panel;
use burnburn_host_models::form::{ButtonRole, Platform};
use burnburn_host_models::hub_form::hub_form;
use burnburn_host_models::hub_panel::HubPanel;
use burnburn_host_models::l10n::Locale;
use burnburn_host_models::panels::{G5Control, G5Line, SchikkenControl, SchikkenLine, g5_form, schikken_form};
use burnburn_host_models::prefs_form::prefs_form;
use burnburn_presentation::HeadlessEditorHarness;

#[test]
fn the_preferences_form_has_tabs_and_only_a_destructive_button() {
    let mut h = HeadlessEditorHarness::new();
    let form = prefs_form(&prefs_panel(&h.controller().prefs_view(), Locale::Nl), Locale::Nl);
    assert_eq!(form.spec.tabs().len(), 3);
    assert_eq!(form.spec.problems(), Vec::new());
    let roles: Vec<ButtonRole> = form.spec.buttons.order(Platform::Windows).into_iter().map(|(r, _)| r).collect();
    assert_eq!(roles, vec![ButtonRole::Destructive]);
}

#[test]
fn the_hub_form_has_four_tabs_and_no_problems() {
    let form = hub_form(&HubPanel::default(), Locale::En);
    assert_eq!(form.tabs(), vec!["Device", "Production", "History", "Calibration"]);
    assert_eq!(form.problems(), Vec::new());
}

#[test]
fn the_schikken_and_g5_forms_put_the_answer_first_on_windows() {
    let lines = vec![
        SchikkenLine::Heading("Sleuven".into()),
        SchikkenLine::Field {
            control: SchikkenControl::SlotsTolerance,
            caption: "Tolerantie (mm)".into(),
            text: "0,1".into(),
            invalid: false,
        },
        SchikkenLine::Button { control: SchikkenControl::SlotsCancel, title: "Annuleer".into(), enabled: true },
        SchikkenLine::Button { control: SchikkenControl::SlotsOk, title: "Pas toe".into(), enabled: true },
    ];
    let form = schikken_form("Schikken", &lines);
    let roles = |p| form.spec.buttons.order(p).into_iter().map(|(r, _)| r).collect::<Vec<_>>();
    assert_eq!(roles(Platform::Windows), vec![ButtonRole::Primary, ButtonRole::Cancel]);
    assert_eq!(form.spec.problems(), Vec::new());

    let g5 = g5_form(
        "Generator",
        &[
            G5Line::Button { control: G5Control::GeneratorCancel, title: "Annuleer".into(), enabled: true },
            G5Line::Button { control: G5Control::GeneratorInsert, title: "Voeg in".into(), enabled: true },
        ],
    );
    let roles: Vec<ButtonRole> = g5.spec.buttons.order(Platform::Windows).into_iter().map(|(r, _)| r).collect();
    assert_eq!(roles, vec![ButtonRole::Primary, ButtonRole::Cancel]);
}
