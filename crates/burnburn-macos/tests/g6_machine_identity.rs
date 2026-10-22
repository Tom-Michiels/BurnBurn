//! G6-machine, M01 on the Mac: the declared model and firmware are said next to the device in
//! the hub, as declared by the user, and Device Settings names its two new rows in both languages.
//! Nothing here reads a controller.

#![allow(clippy::unwrap_used, clippy::panic)]

use burnburn_macos::hub_panel::hub_panel;
use burnburn_macos::l10n::Locale;
use burnburn_macos::panels::cam_caption;
use burnburn_presentation::{CamDialog, DeviceRow, DialogField, HeadlessEditorHarness, MachineHubViewModel, UiIntent};

fn row(id: &str, model: Option<&str>, firmware: Option<&str>) -> DeviceRow {
    DeviceRow {
        id: id.into(),
        revision: 2,
        width_mm: 600.0,
        height_mm: 400.0,
        active: true,
        model: model.map(str::to_owned),
        firmware: firmware.map(str::to_owned),
    }
}

fn devices(rows: Vec<DeviceRow>, locale: Locale) -> Vec<String> {
    let mut h = HeadlessEditorHarness::new();
    let vm = MachineHubViewModel { devices: rows, ..h.controller().machine_hub_view() };
    hub_panel(&vm, false, locale).devices
}

#[test]
fn a_declared_model_and_firmware_are_said_as_declared() {
    let nl = devices(vec![row("graveur", Some("RDC6445G"), Some("v8.02"))], Locale::Nl);
    assert_eq!(nl[0], "graveur (revisie 2, 600 × 400 mm) — model RDC6445G, firmware v8.02 (door jou opgegeven)");
    let en = devices(vec![row("graveur", Some("RDC6445G"), Some("v8.02"))], Locale::En);
    assert_eq!(en[0], "graveur (revision 2, 600 × 400 mm) — model RDC6445G, firmware v8.02 (declared by you)");
}

#[test]
fn only_what_was_declared_is_named() {
    let nl = devices(vec![row("a", Some("RDC6445G"), None), row("b", None, Some("v8.02"))], Locale::Nl);
    assert_eq!(nl[0], "a (revisie 2, 600 × 400 mm) — model RDC6445G (door jou opgegeven)");
    assert_eq!(nl[1], "b (revisie 2, 600 × 400 mm) — firmware v8.02 (door jou opgegeven)");
}

#[test]
fn a_device_without_a_declaration_names_no_model_at_all() {
    let nl = devices(vec![row("a", None, None)], Locale::Nl);
    assert_eq!(nl[0], "a (revisie 2, 600 × 400 mm)", "no model is guessed or filled in");
}

#[test]
fn device_settings_names_the_model_and_firmware_rows_in_both_languages() {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.send(UiIntent::OpenCamDialog { document: doc, dialog: CamDialog::Device });
    let vm = h.controller().cam_dialog(doc, CamDialog::Device).unwrap();
    for locale in [Locale::Nl, Locale::En] {
        for field in [DialogField::Model, DialogField::Firmware] {
            assert!(!cam_caption(field, &vm, locale).is_empty(), "{field:?} {locale:?}");
        }
    }
    assert_eq!(cam_caption(DialogField::Model, &vm, Locale::Nl), "Controllermodel (zoals jij het opgeeft)");
    assert_eq!(cam_caption(DialogField::Firmware, &vm, Locale::En), "Controller firmware (as you declare it)");
}
