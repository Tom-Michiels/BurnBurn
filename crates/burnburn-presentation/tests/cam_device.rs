//! Device Settings and Rotary Setup (W3-UI-cam; M01, M02, M08): the dialog edits a draft of the
//! machine profile, Apply makes a new profile revision (compiled plans go stale, the document
//! and its undo history are untouched), rotary is a profile setting and "back to the flat bed"
//! restores the profile exactly. The capability list is the matrix the compiler reads.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use burnburn_job::{Feature, MachineCapabilities, MachineProfile, OriginCorner, RotaryKind, Support};
use burnburn_presentation::{
    CamDialog, CommandKey, CutField, DialogAction, DialogActionKind, DialogChoice, DialogError, DialogField,
    DialogNote, DialogValue, DialogViewModel, FieldState, HeadlessEditorHarness, HostEffect, MessageKey, OperationKind,
    SelectMode, SupportView, UiIntent,
};
use burnburn_types::{DocumentId, GeometryId};

use common::{RUIDA, profile};

const DEV: CamDialog = CamDialog::Device;

fn window(h: &mut HeadlessEditorHarness) -> DocumentId {
    h.new_document().expect("window")
}

fn with_profile(h: &mut HeadlessEditorHarness, p: MachineProfile) {
    h.send(UiIntent::SetMachineProfile { profile: Box::new(p), ruida: RUIDA });
}

fn open(h: &mut HeadlessEditorHarness, doc: DocumentId) -> DialogViewModel {
    h.send(UiIntent::OpenCamDialog { document: doc, dialog: DEV });
    h.controller().cam_dialog(doc, DEV).expect("dialog open")
}

fn dialog(h: &mut HeadlessEditorHarness, doc: DocumentId) -> Option<DialogViewModel> {
    h.controller().cam_dialog(doc, DEV)
}

fn text(h: &mut HeadlessEditorHarness, doc: DocumentId, field: DialogField, value: &str) {
    h.send(UiIntent::SetCamField { document: doc, dialog: DEV, field, value: DialogValue::Text(value.into()) });
}

fn flag(h: &mut HeadlessEditorHarness, doc: DocumentId, field: DialogField, on: bool) {
    h.send(UiIntent::SetCamField { document: doc, dialog: DEV, field, value: DialogValue::Flag(on) });
}

fn choose(h: &mut HeadlessEditorHarness, doc: DocumentId, field: DialogField, choice: DialogChoice) {
    let index = field.choices().iter().position(|c| *c == choice).expect("an entry of the pop-up");
    h.send(UiIntent::SetCamField { document: doc, dialog: DEV, field, value: DialogValue::Choice(index) });
}

fn press(h: &mut HeadlessEditorHarness, doc: DocumentId, action: DialogAction) -> burnburn_presentation::AppUpdate {
    h.send(UiIntent::CamDialogAction { document: doc, dialog: DEV, action })
}

fn active(h: &mut HeadlessEditorHarness) -> MachineProfile {
    h.controller().machine_profile().expect("an active profile").clone()
}

fn compile(h: &mut HeadlessEditorHarness, doc: DocumentId) {
    h.invoke(doc, CommandKey::Compile);
    h.run_tasks();
}

fn rectangle(h: &mut HeadlessEditorHarness, doc: DocumentId) -> GeometryId {
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (0.0, 0.0), (40.0, 20.0));
    h.invoke(doc, CommandKey::ToolSelect);
    let id = h.document(doc).unwrap().objects[0].id;
    h.send(UiIntent::Select { document: doc, ids: vec![id], mode: SelectMode::Replace });
    id
}

#[test]
fn opening_shows_the_active_profile_and_its_capability_matrix() {
    let mut h = HeadlessEditorHarness::new();
    let doc = window(&mut h);
    with_profile(&mut h, profile(1));
    let vm = open(&mut h, doc);
    assert_eq!(vm.text(DialogField::BedWidth), "1500");
    assert_eq!(vm.text(DialogField::BedHeight), "900");
    assert_eq!(vm.choice(DialogField::BedOrigin), Some(DialogChoice::OriginBottomLeft));
    assert_eq!(vm.text(DialogField::MaxSpeed), "1000");
    assert_eq!(vm.text(DialogField::TravelSpeed), "400");
    assert_eq!((vm.text(DialogField::MinPower), vm.text(DialogField::MaxPower)), ("0", "100"));
    assert!(!vm.flag(DialogField::RotaryEnabled));
    assert!(vm.action(DialogActionKind::Apply).is_some_and(|a| a.enabled));
    assert!(vm.action(DialogActionKind::ReturnToFlatbed).is_some_and(|a| !a.enabled), "there is no rotary to leave");
    let features: Vec<Feature> = vm.capabilities.iter().map(|c| c.feature).collect();
    assert_eq!(features, Feature::ALL, "every feature is listed, in the canonical order");
    assert!(vm.notes.contains(&DialogNote::ProfileChangeMakesPlansStale));
    assert!(vm.notes.contains(&DialogNote::DialectUnverified));
}

/// M01: a profile change makes the compiled plan stale; the document is not touched and nothing
/// enters the document's undo history.
#[test]
fn profile_change_marks_compiled_plan_stale() {
    let mut h = HeadlessEditorHarness::new();
    let doc = window(&mut h);
    rectangle(&mut h, doc);
    with_profile(&mut h, profile(1));
    h.send(UiIntent::AssignLayer { document: doc, slot: 1, add: false });
    compile(&mut h, doc);
    let before = h.document(doc).unwrap();
    assert!(!before.plan.as_ref().expect("plan").stale);

    open(&mut h, doc);
    text(&mut h, doc, DialogField::MaxSpeed, "800");
    press(&mut h, doc, DialogAction::Apply);
    assert!(dialog(&mut h, doc).is_none(), "Apply closes the dialog");

    let after = h.document(doc).unwrap();
    let plan = after.plan.expect("the old plan is kept");
    assert!(plan.stale, "a profile change makes the compiled plan stale");
    assert_eq!(plan.id, before.plan.unwrap().id, "the plan itself is the same immutable plan");
    assert_eq!((after.revision, after.dirty, after.undo), (before.revision, before.dirty, before.undo));
    assert_eq!(active(&mut h).max_speed_mm_s(), 800.0);
    assert_eq!(active(&mut h).revision().revision, 2, "a new revision of the profile");

    // Compiling again makes a plan for the new profile.
    compile(&mut h, doc);
    assert!(!h.document(doc).unwrap().plan.expect("plan").stale);
}

#[test]
fn an_unchanged_dialog_keeps_the_profile_and_its_plans() {
    let mut h = HeadlessEditorHarness::new();
    let doc = window(&mut h);
    rectangle(&mut h, doc);
    with_profile(&mut h, profile(1));
    h.send(UiIntent::AssignLayer { document: doc, slot: 1, add: false });
    compile(&mut h, doc);
    let before = active(&mut h);
    open(&mut h, doc);
    press(&mut h, doc, DialogAction::Apply);
    assert!(dialog(&mut h, doc).is_none());
    assert_eq!(active(&mut h), before, "nothing changed, not even the revision counter");
    assert!(!h.document(doc).unwrap().plan.unwrap().stale);
}

#[test]
fn bed_origin_and_power_reach_the_profile() {
    let mut h = HeadlessEditorHarness::new();
    let doc = window(&mut h);
    with_profile(&mut h, profile(1));
    open(&mut h, doc);
    text(&mut h, doc, DialogField::BedWidth, "600 mm");
    text(&mut h, doc, DialogField::BedHeight, "400,5");
    choose(&mut h, doc, DialogField::BedOrigin, DialogChoice::OriginTopRight);
    text(&mut h, doc, DialogField::TravelSpeed, "300");
    text(&mut h, doc, DialogField::MinPower, "5");
    text(&mut h, doc, DialogField::MaxPower, "80 %");
    press(&mut h, doc, DialogAction::Apply);
    let p = active(&mut h);
    assert_eq!(
        (p.frame().width_mm(), p.frame().height_mm(), p.frame().origin()),
        (600.0, 400.5, OriginCorner::TopRight)
    );
    assert_eq!(p.travel_speed_mm_s(), 300.0);
    let (min, max) = p.power_range();
    assert_eq!((min.get(), max.get()), (0.05, 0.8));
    assert_eq!(p.revision().id, "research-1500x900", "the machine keeps its identity");
}

#[test]
fn invalid_fields_are_refused_where_they_are_and_nothing_changes() {
    let mut h = HeadlessEditorHarness::new();
    let doc = window(&mut h);
    with_profile(&mut h, profile(1));
    let before = active(&mut h);
    open(&mut h, doc);
    for (field, value, why) in [
        (DialogField::BedWidth, "0", DialogError::Invalid),
        (DialogField::BedHeight, "abc", DialogError::Invalid),
        (DialogField::MaxSpeed, "-5", DialogError::Invalid),
        (DialogField::TravelSpeed, "", DialogError::Invalid),
        (DialogField::MinPower, "101", DialogError::OutOfRange),
        (DialogField::MaxPower, "-1", DialogError::OutOfRange),
    ] {
        text(&mut h, doc, field, value);
        press(&mut h, doc, DialogAction::Apply);
        let vm = dialog(&mut h, doc).unwrap_or_else(|| panic!("{field:?} {value:?}: a refused Apply keeps the dialog"));
        assert_eq!(vm.row(field).unwrap().error, Some(why), "{field:?} {value:?}");
        assert_eq!(vm.text(field), value, "what was typed stays");
        assert_eq!(active(&mut h), before, "{field:?} {value:?} changes nothing");
        // Back to the stored text: the error goes with the edit.
        let stored = open(&mut h, doc).text(field).to_owned();
        text(&mut h, doc, field, &stored);
    }
    // Minimum above maximum power.
    text(&mut h, doc, DialogField::MinPower, "90");
    text(&mut h, doc, DialogField::MaxPower, "10");
    press(&mut h, doc, DialogAction::Apply);
    let vm = dialog(&mut h, doc).expect("still open");
    assert_eq!(vm.row(DialogField::MinPower).unwrap().error, Some(DialogError::OutOfRange));
    assert_eq!(active(&mut h), before);
}

#[test]
fn without_a_profile_the_first_apply_creates_one_and_compiling_works() {
    let mut h = HeadlessEditorHarness::new();
    let doc = window(&mut h);
    rectangle(&mut h, doc);
    h.send(UiIntent::AssignLayer { document: doc, slot: 1, add: false });
    assert!(h.controller().machine_profile().is_none());
    let vm = open(&mut h, doc);
    assert!(vm.notes.contains(&DialogNote::NewProfile));
    assert!(!vm.text(DialogField::BedWidth).is_empty(), "start values are offered");
    press(&mut h, doc, DialogAction::Apply);
    assert!(active(&mut h).frame().width_mm() > 0.0);
    compile(&mut h, doc);
    assert!(h.document(doc).unwrap().plan.is_some(), "the new profile compiles the document");
}

#[test]
fn a_chuck_rotary_makes_the_bed_one_wrap_high_and_says_it_is_unconfirmed() {
    let mut h = HeadlessEditorHarness::new();
    let doc = window(&mut h);
    with_profile(&mut h, profile(1));
    open(&mut h, doc);
    flag(&mut h, doc, DialogField::RotaryEnabled, true);
    choose(&mut h, doc, DialogField::RotaryKind, DialogChoice::RotaryChuck);
    text(&mut h, doc, DialogField::RotaryDiameter, "80");
    text(&mut h, doc, DialogField::StepsPerRotation, "16000");
    text(&mut h, doc, DialogField::YStepsPerMm, "40");
    flag(&mut h, doc, DialogField::RotaryMirror, true);
    let vm = dialog(&mut h, doc).unwrap();
    assert!(vm.notes.contains(&DialogNote::RotaryUnconfirmed));
    assert!(vm.notes.contains(&DialogNote::RotaryNotInCliProfile));
    press(&mut h, doc, DialogAction::Apply);
    let p = active(&mut h);
    let rotary = p.rotary().expect("rotary attached");
    assert_eq!(rotary.kind(), RotaryKind::Chuck { object_diameter_mm: 80.0 });
    assert_eq!((rotary.steps_per_rotation(), rotary.y_steps_per_mm(), rotary.mirror()), (16000, 40.0, true));
    assert!((p.frame().height_mm() - std::f64::consts::PI * 80.0).abs() < 1e-9, "one wrap high");
    assert_eq!(p.frame().width_mm(), 1500.0);
    assert_eq!(p.flatbed_frame().height_mm(), 900.0);

    // Reopened, the dialog shows the rotary, and the bed fields still describe the flat bed.
    let vm = open(&mut h, doc);
    assert!(vm.flag(DialogField::RotaryEnabled));
    assert_eq!(vm.text(DialogField::BedHeight), "900");
    assert_eq!(vm.text(DialogField::RotaryDiameter), "80");
    assert!(vm.action(DialogActionKind::ReturnToFlatbed).is_some_and(|a| a.enabled));
}

#[test]
fn a_roller_rotary_keeps_the_bed() {
    let mut h = HeadlessEditorHarness::new();
    let doc = window(&mut h);
    with_profile(&mut h, profile(1));
    open(&mut h, doc);
    flag(&mut h, doc, DialogField::RotaryEnabled, true);
    choose(&mut h, doc, DialogField::RotaryKind, DialogChoice::RotaryRoller);
    text(&mut h, doc, DialogField::RotaryDiameter, "25");
    text(&mut h, doc, DialogField::StepsPerRotation, "8000");
    text(&mut h, doc, DialogField::YStepsPerMm, "40");
    press(&mut h, doc, DialogAction::Apply);
    let p = active(&mut h);
    assert_eq!(p.rotary().unwrap().kind(), RotaryKind::Roller { roller_diameter_mm: 25.0 });
    assert_eq!(p.frame(), p.flatbed_frame());
}

#[test]
fn rotary_fields_are_checked_only_while_the_rotary_is_on() {
    let mut h = HeadlessEditorHarness::new();
    let doc = window(&mut h);
    with_profile(&mut h, profile(1));
    let before = active(&mut h);
    let vm = open(&mut h, doc);
    assert!(matches!(vm.row(DialogField::RotaryDiameter).unwrap().state, FieldState::Disabled(None)));
    // Junk in a switched-off rotary does not matter.
    text(&mut h, doc, DialogField::StepsPerRotation, "x");
    press(&mut h, doc, DialogAction::Apply);
    assert!(dialog(&mut h, doc).is_none());
    assert_eq!(active(&mut h), before);

    open(&mut h, doc);
    flag(&mut h, doc, DialogField::RotaryEnabled, true);
    for (field, value, why) in [
        (DialogField::RotaryDiameter, "0", DialogError::Invalid),
        (DialogField::RotaryDiameter, "2001", DialogError::OutOfRange),
        (DialogField::StepsPerRotation, "1,5", DialogError::NotWhole),
        (DialogField::StepsPerRotation, "0", DialogError::OutOfRange),
        (DialogField::YStepsPerMm, "", DialogError::Invalid),
    ] {
        text(&mut h, doc, DialogField::RotaryDiameter, "80");
        text(&mut h, doc, DialogField::StepsPerRotation, "16000");
        text(&mut h, doc, DialogField::YStepsPerMm, "40");
        text(&mut h, doc, field, value);
        press(&mut h, doc, DialogAction::Apply);
        let vm = dialog(&mut h, doc).unwrap_or_else(|| panic!("{field:?} {value:?}: kept open"));
        assert_eq!(vm.row(field).unwrap().error, Some(why), "{field:?} {value:?}");
        assert_eq!(active(&mut h), before, "{field:?} {value:?} changes nothing");
    }
}

/// M08: back to the flat bed restores the profile exactly (same content, frame, capabilities).
#[test]
fn returning_to_flatbed_restores_profile() {
    let mut h = HeadlessEditorHarness::new();
    let doc = window(&mut h);
    let caps = MachineCapabilities::conservative().with(Feature::Tabs, Support::Unsupported);
    with_profile(&mut h, profile(1).with_capabilities(caps).unwrap());
    let original = active(&mut h);

    open(&mut h, doc);
    flag(&mut h, doc, DialogField::RotaryEnabled, true);
    text(&mut h, doc, DialogField::RotaryDiameter, "60");
    text(&mut h, doc, DialogField::StepsPerRotation, "12000");
    text(&mut h, doc, DialogField::YStepsPerMm, "40");
    press(&mut h, doc, DialogAction::Apply);
    assert!(active(&mut h).rotary().is_some());
    assert_ne!(active(&mut h).revision().content, original.revision().content);

    open(&mut h, doc);
    press(&mut h, doc, DialogAction::ReturnToFlatbed);
    assert!(dialog(&mut h, doc).is_none(), "returning closes the dialog");
    let restored = active(&mut h);
    assert!(restored.rotary().is_none());
    assert_eq!(restored.frame(), original.frame());
    assert_eq!(restored.capabilities(), original.capabilities());
    assert_eq!(restored.revision().content, original.revision().content, "the same profile content");
    assert_eq!(restored.revision().id, original.revision().id);
    assert!(restored.revision().revision > 1, "each change is a revision of its own");
}

/// M02: the Device Settings list, the Cut Settings sheet and the compiler read one matrix.
#[test]
fn ui_and_compiler_read_same_capability_matrix() {
    let matrices = [
        MachineCapabilities::conservative(),
        MachineCapabilities::conservative()
            .with(Feature::ZStep, Support::Supported)
            .with(Feature::Tabs, Support::Unsupported)
            .with(Feature::Leads, Support::Unverified { reason: "not proven here".into() }),
    ];
    for caps in matrices {
        let mut h = HeadlessEditorHarness::new();
        let doc = window(&mut h);
        let id = rectangle(&mut h, doc);
        with_profile(&mut h, profile(1).with_capabilities(caps.clone()).unwrap());
        h.send(UiIntent::AssignLayer { document: doc, slot: 1, add: false });
        let vm = open(&mut h, doc);
        for row in &vm.capabilities {
            let expected = match caps.support(row.feature) {
                Support::Supported => SupportView::Supported,
                Support::Unsupported => SupportView::Unsupported,
                Support::Unverified { reason } => SupportView::Unverified(reason),
            };
            assert_eq!(row.support, expected, "{:?}", row.feature);
        }
        // The Cut Settings sheet greys exactly the fields whose feature is not Supported.
        let op = h.document(doc).unwrap().operations[0].id;
        h.send(UiIntent::OpenCutSettings { document: doc, operation: op });
        let sheet = h.document(doc).unwrap().cut_settings.unwrap();
        for (field, feature) in [
            (CutField::TabsEnabled, Feature::Tabs),
            (CutField::LeadInEnabled, Feature::Leads),
            (CutField::Overcut, Feature::Overcut),
            (CutField::PerforationEnabled, Feature::Perforation),
            (CutField::ZOffset, Feature::ZStep),
        ] {
            let state = sheet.extras.state(field, OperationKind::Line, &sheet.gates);
            if caps.is_supported(feature) {
                assert!(!matches!(state, FieldState::Disabled(Some(_))), "{field:?} is allowed by {feature:?}");
            } else {
                assert_eq!(state, FieldState::Disabled(Some(MessageKey::MissingCapability(feature))), "{field:?}");
            }
        }
        h.send(UiIntent::CancelCutSettings { document: doc });
        let _ = id;
    }
}

#[test]
fn the_dialog_is_closed_by_close_and_an_operator_cannot_open_it() {
    let mut h = HeadlessEditorHarness::new();
    let doc = window(&mut h);
    with_profile(&mut h, profile(1));
    open(&mut h, doc);
    text(&mut h, doc, DialogField::MaxSpeed, "123");
    press(&mut h, doc, DialogAction::Close);
    assert!(dialog(&mut h, doc).is_none());
    assert_eq!(active(&mut h).max_speed_mm_s(), 1000.0, "Close drops the draft");

    h.controller().set_role(burnburn_presentation::Role::Operator);
    let update = h.send(UiIntent::OpenCamDialog { document: doc, dialog: DEV });
    assert!(
        update
            .effects
            .iter()
            .any(|e| matches!(e, HostEffect::ShowError { message: MessageKey::NotAllowedForRole, .. }))
    );
    assert!(dialog(&mut h, doc).is_none());
}
