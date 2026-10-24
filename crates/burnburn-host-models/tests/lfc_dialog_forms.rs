//! LFC: the barcode sheet and the geometry dialog as form descriptions (headless).

use burnburn_host_models::form::{ButtonRole, Control, FormSpec, Platform};
use burnburn_host_models::format::Quantity;
use burnburn_host_models::l10n::Locale;
use burnburn_host_models::panels::GeometryDialogPanel;
use burnburn_host_models::panels::dialog_forms::{barcode, barcode_form, geometry, geometry_dialog_form};
use burnburn_host_models::style::StatusTone;
use burnburn_presentation::GeometryDialogKind;

fn titles(spec: &FormSpec) -> Vec<(ButtonRole, String)> {
    spec.buttons.order(Platform::Mac).into_iter().map(|(r, b)| (r, b.title.clone())).collect()
}

#[test]
fn the_barcode_form_follows_the_form_rules_with_units_as_suffixes() {
    for locale in [Locale::Nl, Locale::En] {
        let spec = barcode_form(locale);
        assert_eq!(spec.problems(), Vec::new(), "{locale:?}");
        for (id, unit) in [
            (barcode::MODULE, Quantity::Millimeters),
            (barcode::MIN_MODULE, Quantity::Millimeters),
            (barcode::HEIGHT, Quantity::Millimeters),
            (barcode::WIDTH, Quantity::Millimeters),
        ] {
            let row = spec.row(id).unwrap_or_else(|| panic!("row {id}"));
            assert_eq!(row.unit, Some(unit), "{} has its unit as the suffix", row.label);
            assert!(!row.label.contains('('), "{}", row.label);
        }
        assert!(matches!(spec.row(barcode::TYPE).map(|r| &r.control), Some(Control::Choice { .. })));
        assert!(matches!(spec.row(barcode::ECC).map(|r| &r.control), Some(Control::Choice { .. })));
        assert!(matches!(spec.row(barcode::INPUT_MODE).map(|r| &r.control), Some(Control::Choice { .. })));
        assert!(matches!(spec.row(barcode::PAYLOAD).map(|r| &r.control), Some(Control::Text { .. })));
    }
}

#[test]
fn the_barcode_messages_are_notes_and_the_problems_are_in_the_error_colour() {
    let spec = barcode_form(Locale::En);
    for id in [barcode::PAYLOAD_HINT, barcode::SIZE_LINE, barcode::CANONICAL, barcode::PROBLEMS, barcode::READABILITY] {
        assert!(matches!(spec.row(id).map(|r| &r.control), Some(Control::Note { .. })), "row {id}");
    }
    for id in [barcode::PROBLEMS, barcode::READABILITY] {
        assert!(
            matches!(spec.row(id).map(|r| &r.control), Some(Control::Note { tone: StatusTone::Error, .. })),
            "row {id}"
        );
    }
}

#[test]
fn the_barcode_buttons_are_cancel_then_create_and_create_answers_return() {
    assert_eq!(
        titles(&barcode_form(Locale::Nl)),
        vec![(ButtonRole::Cancel, "Annuleer".to_owned()), (ButtonRole::Primary, "Maak".to_owned())]
    );
    assert_eq!(
        titles(&barcode_form(Locale::En)),
        vec![(ButtonRole::Cancel, "Cancel".to_owned()), (ButtonRole::Primary, "Create".to_owned())]
    );
    let spec = barcode_form(Locale::En);
    assert_eq!(spec.buttons.primary.as_ref().map(|b| b.id.0), Some(barcode::CREATE));
    assert_eq!(spec.buttons.cancel.as_ref().map(|b| b.id.0), Some(barcode::CANCEL));
}

fn dialog(kind: GeometryDialogKind, title: &str, label: &str, error: &str) -> GeometryDialogPanel {
    GeometryDialogPanel {
        kind,
        title: title.to_owned(),
        label: label.to_owned(),
        text: "2".to_owned(),
        error: error.to_owned(),
    }
}

#[test]
fn the_geometry_dialog_has_the_title_as_heading_the_unit_as_suffix_and_the_refusal_under_the_field() {
    let d = dialog(GeometryDialogKind::RoundCorners, "Hoeken afronden", "Straal (mm)", "Dat is geen getal.");
    let spec = geometry_dialog_form(&d, Locale::Nl);
    assert_eq!(spec.problems(), Vec::new());
    assert_eq!(spec.sections.first().and_then(|s| s.heading.as_deref()), Some("Hoeken afronden"));
    let row = spec.row(geometry::VALUE).expect("the value row");
    assert_eq!((row.label.as_str(), row.unit), ("Straal", Some(Quantity::Millimeters)));
    assert!(matches!(&row.control, Control::Number { value } | Control::Text { value, .. } if value == "2"));
    assert_eq!(row.error.as_deref(), Some("Dat is geen getal."));
    assert_eq!(
        titles(&spec),
        vec![(ButtonRole::Cancel, "Annuleer".to_owned()), (ButtonRole::Primary, "OK".to_owned())]
    );
}

#[test]
fn the_geometry_dialog_keeps_its_shape_for_every_kind_and_with_or_without_a_refusal() {
    let a = geometry_dialog_form(
        &dialog(GeometryDialogKind::RoundCorners, "Hoeken afronden", "Straal (mm)", ""),
        Locale::Nl,
    );
    let b = geometry_dialog_form(
        &dialog(GeometryDialogKind::Join, "Paden verbinden of sluiten", "Grootste opening (mm)", "Geen getal."),
        Locale::Nl,
    );
    assert!(!a.shape().is_empty());
    assert_eq!(a.shape(), b.shape(), "the host updates one built form for every opening");
}
