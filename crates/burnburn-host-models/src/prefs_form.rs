//! The preferences window as a [`FormSpec`] (LF6c): one tab per group (general, canvas, nudging),
//! the unit of a number as the suffix of its field, the one notice of the window above the
//! buttons, and "restore defaults" as the destructive button. The window title is "Instellingen".

use burnburn_presentation::PrefKey;

use crate::app_text::{AppText, app_text};
use crate::app_view::{PrefsControl, PrefsPanel};
use crate::form::{
    Control, DialogButtons, FormButton, FormId, FormRow, FormSection, FormSpec, split_caption,
};
use crate::format::Quantity;
use crate::l10n::Locale;

/// The form of the preferences and which preference each of its ids is.
#[derive(Debug, Clone, PartialEq)]
pub struct PrefsForm {
    pub spec: FormSpec,
    pub keys: Vec<(FormId, PrefKey)>,
}

/// The id of the row of `key`.
pub fn pref_form_id(key: PrefKey) -> FormId {
    FormId(PrefKey::ALL.iter().position(|k| *k == key).map_or(0, |i| i as u32 + 1))
}

/// The id of the "restore defaults" button.
pub const RESET_ID: FormId = FormId(100);

/// Which tab a preference lies on.
fn tab_of(key: PrefKey) -> AppText {
    match key {
        PrefKey::Units | PrefKey::Language | PrefKey::AutosaveSeconds | PrefKey::DesktopAutomation => {
            AppText::PrefsTabGeneral
        }
        PrefKey::SnapToObjects
        | PrefKey::SnapToGrid
        | PrefKey::GridSnap
        | PrefKey::ObjectSnapPixels
        | PrefKey::Quality => AppText::PrefsTabCanvas,
        PrefKey::NudgeSmall | PrefKey::NudgeNormal | PrefKey::NudgeLarge => AppText::PrefsTabNudge,
    }
}

/// The unit a suffix text names.
fn unit_of_suffix(suffix: &str) -> Option<Quantity> {
    match suffix {
        "mm" => Some(Quantity::Millimeters),
        "in" => Some(Quantity::Inches),
        "px" => Some(Quantity::Pixels),
        "s" => Some(Quantity::Seconds),
        _ => None,
    }
}

/// The preferences form for `panel`.
pub fn prefs_form(panel: &PrefsPanel, locale: Locale) -> PrefsForm {
    let tabs = [AppText::PrefsTabGeneral, AppText::PrefsTabCanvas, AppText::PrefsTabNudge];
    let mut sections: Vec<FormSection> = Vec::new();
    let mut keys: Vec<(FormId, PrefKey)> = Vec::new();
    for tab in tabs {
        let title = app_text(tab, locale);
        let mut rows: Vec<FormRow> = Vec::new();
        for row in panel.rows.iter().filter(|r| tab_of(r.key) == tab) {
            let id = pref_form_id(row.key);
            keys.push((id, row.key));
            let split = split_caption(&row.caption);
            let form_row = match &row.control {
                PrefsControl::Choice { titles, selected, .. } => {
                    FormRow::new(id.0, &split.label, Control::Choice { options: titles.clone(), selected: *selected })
                }
                PrefsControl::Check(on) => FormRow::new(id.0, &row.caption, Control::Check { on: *on }),
                PrefsControl::Field { text, suffix, invalid } => {
                    let mut r = FormRow::new(
                        id.0,
                        &split.label,
                        Control::Text { value: text.clone(), placeholder: String::new() },
                    )
                    .invalid(*invalid);
                    r.unit = unit_of_suffix(suffix).or(split.unit);
                    r
                }
            };
            let mut form_row = form_row;
            form_row.help = split.hint;
            rows.push(form_row);
        }
        if tab == AppText::PrefsTabGeneral {
            let id = RESET_ID.0 + 1;
            rows.push(FormRow::new(id, "", Control::ReadOnly { value: app_text(AppText::PrefsNote, locale).to_owned() }));
        }
        sections.push(FormSection::new(None, rows).on_tab(title));
    }
    let buttons = DialogButtons {
        destructive: Some(FormButton::new(RESET_ID.0, app_text(AppText::PrefsReset, locale))),
        ..DialogButtons::default()
    };
    PrefsForm {
        spec: FormSpec {
            title: app_text(AppText::PrefsTitle, locale).to_owned(),
            sections,
            notice: panel.banner.clone(),
            notice_slot: true,
            buttons,
        },
        keys,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_view::{PrefsPanelRow, prefs_panel};
    use crate::form::{ButtonRole, Control, Platform};
    use crate::format::Quantity;
    use burnburn_presentation::HeadlessEditorHarness;

    fn panel(locale: Locale) -> PrefsPanel {
        let mut h = HeadlessEditorHarness::new();
        prefs_panel(&h.controller().prefs_view(), locale)
    }

    fn row<'a>(form: &'a PrefsForm, key: PrefKey) -> &'a crate::form::FormRow {
        let id = pref_form_id(key);
        form.spec.sections.iter().flat_map(|s| &s.rows).find(|r| r.id == id).expect("row")
    }

    #[test]
    fn the_groups_are_tabs_and_every_preference_is_on_exactly_one() {
        let form = prefs_form(&panel(Locale::Nl), Locale::Nl);
        assert_eq!(form.spec.title, "Instellingen");
        assert_eq!(form.spec.tabs(), vec!["Algemeen", "Canvas", "Verplaatsen"]);
        for key in PrefKey::ALL {
            let n = form.spec.sections.iter().flat_map(|s| &s.rows).filter(|r| r.id == pref_form_id(key)).count();
            assert_eq!(n, 1, "{key:?}");
        }
        let tab_of = |key: PrefKey| {
            let id = pref_form_id(key);
            form.spec.sections.iter().find(|s| s.rows.iter().any(|r| r.id == id)).and_then(|s| s.tab.clone())
        };
        assert_eq!(tab_of(PrefKey::Language).as_deref(), Some("Algemeen"));
        assert_eq!(tab_of(PrefKey::GridSnap).as_deref(), Some("Canvas"));
        assert_eq!(tab_of(PrefKey::NudgeLarge).as_deref(), Some("Verplaatsen"));
    }

    #[test]
    fn units_are_suffixes_and_a_hint_in_the_caption_becomes_help() {
        let form = prefs_form(&panel(Locale::Nl), Locale::Nl);
        let autosave = row(&form, PrefKey::AutosaveSeconds);
        assert_eq!(autosave.label, "Automatisch bewaren");
        assert_eq!(autosave.unit, Some(Quantity::Seconds));
        assert_eq!(autosave.help.as_deref(), Some("0 = uit"));
        assert_eq!(row(&form, PrefKey::ObjectSnapPixels).unit, Some(Quantity::Pixels));
        assert_eq!(row(&form, PrefKey::ObjectSnapPixels).label, "Vangstraal objecten");
        assert_eq!(row(&form, PrefKey::NudgeNormal).unit, Some(Quantity::Millimeters));
        assert_eq!(form.spec.problems(), Vec::new());
    }

    #[test]
    fn the_notice_is_one_for_the_window_and_reset_is_the_destructive_button() {
        let mut p = panel(Locale::Nl);
        assert_eq!(prefs_form(&p, Locale::Nl).spec.notice, None);
        p.banner = Some("Geen geldig getal.".into());
        if let Some(PrefsPanelRow { control: PrefsControl::Field { invalid, .. }, .. }) =
            p.rows.iter_mut().find(|r| r.key == PrefKey::GridSnap)
        {
            *invalid = true;
        }
        let form = prefs_form(&p, Locale::Nl);
        assert_eq!(form.spec.notice.as_deref(), Some("Geen geldig getal."));
        assert!(row(&form, PrefKey::GridSnap).invalid);
        assert!(row(&form, PrefKey::GridSnap).error.is_none(), "the window says it once, not the row");
        let roles: Vec<(ButtonRole, &str)> =
            form.spec.buttons.order(Platform::Mac).into_iter().map(|(r, b)| (r, b.title.as_str())).collect();
        assert_eq!(roles, vec![(ButtonRole::Destructive, "Standaardwaarden herstellen")]);
    }

    #[test]
    fn a_notice_that_comes_and_goes_does_not_change_the_shape_of_the_form() {
        let quiet = prefs_form(&panel(Locale::Nl), Locale::Nl);
        let mut p = panel(Locale::Nl);
        p.banner = Some("Geen geldig getal.".into());
        let loud = prefs_form(&p, Locale::Nl);
        assert_eq!(quiet.spec.shape(), loud.spec.shape());
        assert!(quiet.spec.notice_slot);
    }

    #[test]
    fn the_controls_keep_their_kind_and_value() {
        let form = prefs_form(&panel(Locale::En), Locale::En);
        assert!(matches!(row(&form, PrefKey::Language).control, Control::Choice { .. }));
        assert!(matches!(row(&form, PrefKey::SnapToGrid).control, Control::Check { .. }));
        assert!(matches!(row(&form, PrefKey::GridSnap).control, Control::Text { .. }));
        assert_eq!(form.spec.tabs(), vec!["General", "Canvas", "Nudging"]);
    }
}
