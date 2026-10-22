//! G5-grbl-ui, Device Settings: backend choice Ruida/GRBL with the GRBL fields (`$30`, laser mode,
//! M3/M4, air assist M8, work coordinate G54-G59 = S70). Apply makes a new profile revision whose
//! capability matrix is the GRBL one; the dialog never talks to a machine.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_test_support::host::rig as common;

use burnburn_gcode::{GrblProfile, PowerMode, WorkCoordinate, grbl_capabilities};
use burnburn_job::{Feature, MachineCapabilities, Support};
use burnburn_presentation::{
    CamDialog, DialogAction, DialogChoice, DialogError, DialogField, DialogNote, DialogValue, DialogViewModel,
    FieldState, HeadlessEditorHarness, UiIntent,
};
use burnburn_types::DocumentId;

use common::{RUIDA, profile};

const DEV: CamDialog = CamDialog::Device;

fn rig() -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().expect("window");
    h.send(UiIntent::SetMachineProfile { profile: Box::new(profile(1)), ruida: RUIDA });
    (h, doc)
}

fn open(h: &mut HeadlessEditorHarness, doc: DocumentId) -> DialogViewModel {
    h.send(UiIntent::OpenCamDialog { document: doc, dialog: DEV });
    h.controller().cam_dialog(doc, DEV).expect("dialog open")
}

fn vm(h: &mut HeadlessEditorHarness, doc: DocumentId) -> Option<DialogViewModel> {
    h.controller().cam_dialog(doc, DEV)
}

fn set(h: &mut HeadlessEditorHarness, doc: DocumentId, field: DialogField, value: DialogValue) {
    h.send(UiIntent::SetCamField { document: doc, dialog: DEV, field, value });
}

fn text(h: &mut HeadlessEditorHarness, doc: DocumentId, field: DialogField, value: &str) {
    set(h, doc, field, DialogValue::Text(value.into()));
}

fn choose(h: &mut HeadlessEditorHarness, doc: DocumentId, field: DialogField, choice: DialogChoice) {
    let index = field.choices().iter().position(|c| *c == choice).expect("an entry of the pop-up");
    set(h, doc, field, DialogValue::Choice(index));
}

fn apply(h: &mut HeadlessEditorHarness, doc: DocumentId) {
    h.send(UiIntent::CamDialogAction { document: doc, dialog: DEV, action: DialogAction::Apply });
}

fn grbl_selected(h: &mut HeadlessEditorHarness, doc: DocumentId) {
    open(h, doc);
    choose(h, doc, DialogField::Backend, DialogChoice::BackendGrbl);
}

#[test]
fn a_ruida_profile_opens_with_the_ruida_backend_and_greyed_grbl_fields() {
    let (mut h, doc) = rig();
    let v = open(&mut h, doc);
    assert_eq!(v.choice(DialogField::Backend), Some(DialogChoice::BackendRuida));
    for field in [
        DialogField::GrblMaxSpindle,
        DialogField::GrblLaserMode,
        DialogField::GrblPowerMode,
        DialogField::GrblAirAssist,
        DialogField::GrblWorkCoordinate,
        DialogField::GrblDecimals,
    ] {
        let row = v.row(field).unwrap_or_else(|| panic!("{field:?} is listed"));
        assert!(matches!(row.state, FieldState::Disabled(_)), "{field:?} follows the backend");
    }
    assert!(v.notes.contains(&DialogNote::DialectUnverified));
    assert!(!v.notes.contains(&DialogNote::GrblUnverified));
    assert_eq!(h.controller().grbl_profile(), None);
}

#[test]
fn choosing_grbl_enables_its_fields_with_the_stock_laser_defaults() {
    let (mut h, doc) = rig();
    grbl_selected(&mut h, doc);
    let v = vm(&mut h, doc).unwrap();
    let stock = GrblProfile::grbl_1_1_laser();
    assert_eq!(v.text(DialogField::GrblMaxSpindle), "1000");
    assert!(v.flag(DialogField::GrblLaserMode));
    assert_eq!(v.choice(DialogField::GrblPowerMode), Some(DialogChoice::PowerDynamicM4));
    assert!(!v.flag(DialogField::GrblAirAssist));
    assert_eq!(v.choice(DialogField::GrblWorkCoordinate), Some(DialogChoice::WorkCoordNone));
    assert_eq!(v.text(DialogField::GrblDecimals), stock.decimals.to_string());
    for field in [DialogField::GrblMaxSpindle, DialogField::GrblPowerMode, DialogField::GrblWorkCoordinate] {
        assert_eq!(v.row(field).unwrap().state, FieldState::Editable, "{field:?}");
    }
    assert!(v.notes.contains(&DialogNote::GrblUnverified));
    assert!(!v.notes.contains(&DialogNote::DialectUnverified));
}

#[test]
fn applying_grbl_makes_a_new_revision_with_the_grbl_matrix_and_the_typed_fields() {
    let (mut h, doc) = rig();
    let before = h.controller().machine_profile().unwrap().clone();
    grbl_selected(&mut h, doc);
    text(&mut h, doc, DialogField::GrblMaxSpindle, "255");
    choose(&mut h, doc, DialogField::GrblPowerMode, DialogChoice::PowerConstantM3);
    set(&mut h, doc, DialogField::GrblAirAssist, DialogValue::Flag(true));
    choose(&mut h, doc, DialogField::GrblWorkCoordinate, DialogChoice::WorkCoordG55);
    text(&mut h, doc, DialogField::GrblDecimals, "2");
    apply(&mut h, doc);
    assert!(vm(&mut h, doc).is_none(), "Apply closes the dialog");

    let grbl = h.controller().grbl_profile().expect("a GRBL machine").clone();
    assert_eq!(
        grbl,
        GrblProfile {
            max_spindle: 255,
            laser_mode: true,
            power_mode: PowerMode::Constant,
            air_assist: true,
            work_coordinate: Some(WorkCoordinate::G55),
            decimals: 2,
        }
    );
    let after = h.controller().machine_profile().unwrap().clone();
    assert_eq!(after.revision().revision, before.revision().revision + 1);
    for feature in Feature::ALL {
        assert_eq!(after.capabilities().support(feature), grbl_capabilities(&grbl).support(feature), "{feature:?}");
    }
    assert_eq!(after.capabilities().support(Feature::DotMode), Support::Unsupported);
}

#[test]
fn the_dialog_shows_the_stored_grbl_values_again_and_ruida_can_be_chosen_back() {
    let (mut h, doc) = rig();
    grbl_selected(&mut h, doc);
    text(&mut h, doc, DialogField::GrblMaxSpindle, "800");
    choose(&mut h, doc, DialogField::GrblWorkCoordinate, DialogChoice::WorkCoordG59);
    set(&mut h, doc, DialogField::GrblLaserMode, DialogValue::Flag(false));
    apply(&mut h, doc);

    let v = open(&mut h, doc);
    assert_eq!(v.choice(DialogField::Backend), Some(DialogChoice::BackendGrbl));
    assert_eq!(v.text(DialogField::GrblMaxSpindle), "800");
    assert_eq!(v.choice(DialogField::GrblWorkCoordinate), Some(DialogChoice::WorkCoordG59));
    assert!(!v.flag(DialogField::GrblLaserMode));

    choose(&mut h, doc, DialogField::Backend, DialogChoice::BackendRuida);
    apply(&mut h, doc);
    assert_eq!(h.controller().grbl_profile(), None);
    let p = h.controller().machine_profile().unwrap();
    assert_eq!(
        p.capabilities().support(Feature::ScanAngle),
        MachineCapabilities::conservative().support(Feature::ScanAngle)
    );
}

#[test]
fn unusable_grbl_fields_keep_the_dialog_open_and_the_profile_as_it_was() {
    let (mut h, doc) = rig();
    let before = h.controller().machine_profile().unwrap().clone();
    for (field, value, error) in [
        (DialogField::GrblMaxSpindle, "0", DialogError::OutOfRange),
        (DialogField::GrblMaxSpindle, "abc", DialogError::Invalid),
        (DialogField::GrblMaxSpindle, "12,5", DialogError::NotWhole),
        (DialogField::GrblDecimals, "0", DialogError::OutOfRange),
        (DialogField::GrblDecimals, "5", DialogError::OutOfRange),
    ] {
        grbl_selected(&mut h, doc);
        text(&mut h, doc, field, value);
        apply(&mut h, doc);
        let v = vm(&mut h, doc).unwrap_or_else(|| panic!("{field:?}={value} closed the dialog"));
        assert_eq!(v.row(field).unwrap().error, Some(error), "{field:?}={value}");
        assert_eq!(h.controller().machine_profile().unwrap(), &before);
        assert_eq!(h.controller().grbl_profile(), None);
        h.send(UiIntent::CamDialogAction { document: doc, dialog: DEV, action: DialogAction::Close });
    }
}

#[test]
fn a_grbl_only_change_is_a_new_revision_and_an_untouched_dialog_is_not() {
    let (mut h, doc) = rig();
    grbl_selected(&mut h, doc);
    apply(&mut h, doc);
    let first = h.controller().machine_profile().unwrap().revision().revision;
    open(&mut h, doc);
    apply(&mut h, doc);
    assert_eq!(h.controller().machine_profile().unwrap().revision().revision, first, "nothing changed");

    open(&mut h, doc);
    set(&mut h, doc, DialogField::GrblAirAssist, DialogValue::Flag(true));
    apply(&mut h, doc);
    assert_eq!(
        h.controller().machine_profile().unwrap().revision().revision,
        first + 1,
        "air assist is part of the profile"
    );
    assert!(h.controller().grbl_profile().unwrap().air_assist);
}

#[test]
fn a_loaded_grbl_profile_is_the_active_one_and_a_loaded_ruida_profile_replaces_it() {
    let (mut h, doc) = rig();
    let mut grbl = GrblProfile::grbl_1_1_laser();
    grbl.max_spindle = 500;
    h.send(UiIntent::SetGrblMachineProfile {
        profile: Box::new(profile(1).with_capabilities(grbl_capabilities(&grbl)).unwrap()),
        grbl: Box::new(grbl.clone()),
    });
    assert_eq!(h.controller().grbl_profile(), Some(&grbl));
    assert_eq!(open(&mut h, doc).text(DialogField::GrblMaxSpindle), "500");
    h.send(UiIntent::SetMachineProfile { profile: Box::new(profile(2)), ruida: RUIDA });
    assert_eq!(h.controller().grbl_profile(), None);
}
