//! Material Test, Interval Test and Focus Test dialogs (W3-UI-cam, W2-L3): each makes a private
//! test job beside the document, compiled by the ordinary compiler as a cancellable background
//! task; only the newest job counts, a changed field or profile drops/stales the result, and RD is
//! written only on request (never into the project).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use burnburn_job::{Feature, MachineCapabilities, Support};
use burnburn_presentation::{
    CamDialog, DialogAction, DialogActionKind, DialogChoice, DialogError, DialogField, DialogNote, DialogValue,
    DialogViewModel, FieldState, HeadlessEditorHarness, MessageKey, RdRefusal, UiIntent,
};
use burnburn_types::DocumentId;

use common::{RUIDA, profile};

const MATERIAL: CamDialog = CamDialog::MaterialTest;
const INTERVAL: CamDialog = CamDialog::IntervalTest;
const FOCUS: CamDialog = CamDialog::FocusTest;

fn rig(z: Support) -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().expect("window");
    let caps = MachineCapabilities::conservative().with(Feature::ZStep, z);
    h.send(UiIntent::SetMachineProfile {
        profile: Box::new(profile(1).with_capabilities(caps).unwrap()),
        ruida: RUIDA,
    });
    (h, doc)
}

fn open(h: &mut HeadlessEditorHarness, doc: DocumentId, d: CamDialog) -> DialogViewModel {
    h.send(UiIntent::OpenCamDialog { document: doc, dialog: d });
    h.controller().cam_dialog(doc, d).expect("dialog open")
}

fn view(h: &mut HeadlessEditorHarness, doc: DocumentId, d: CamDialog) -> Option<DialogViewModel> {
    h.controller().cam_dialog(doc, d)
}

fn text(h: &mut HeadlessEditorHarness, doc: DocumentId, d: CamDialog, field: DialogField, value: &str) {
    h.send(UiIntent::SetCamField { document: doc, dialog: d, field, value: DialogValue::Text(value.into()) });
}

fn flag(h: &mut HeadlessEditorHarness, doc: DocumentId, d: CamDialog, field: DialogField, on: bool) {
    h.send(UiIntent::SetCamField { document: doc, dialog: d, field, value: DialogValue::Flag(on) });
}

fn choose(h: &mut HeadlessEditorHarness, doc: DocumentId, d: CamDialog, field: DialogField, c: DialogChoice) {
    let index = field.choices().iter().position(|x| *x == c).expect("an entry of the pop-up");
    h.send(UiIntent::SetCamField { document: doc, dialog: d, field, value: DialogValue::Choice(index) });
}

fn press(h: &mut HeadlessEditorHarness, doc: DocumentId, d: CamDialog, action: DialogAction) {
    h.send(UiIntent::CamDialogAction { document: doc, dialog: d, action });
}

/// Generate and run the compile task; returns the dialog.
fn generate(h: &mut HeadlessEditorHarness, doc: DocumentId, d: CamDialog) -> DialogViewModel {
    press(h, doc, d, DialogAction::Generate);
    h.run_tasks();
    view(h, doc, d).expect("still open")
}

fn error_of(vm: &DialogViewModel, field: DialogField) -> Option<DialogError> {
    vm.row(field).and_then(|r| r.error)
}

#[test]
fn a_material_test_makes_a_grid_plan_and_leaves_the_document_alone() {
    let (mut h, doc) = rig(Support::Unsupported);
    let before = h.document(doc).unwrap();
    let vm = open(&mut h, doc, MATERIAL);
    assert!(vm.notes.contains(&DialogNote::TestIsSeparate));
    assert!(vm.result.is_none());
    let vm = generate(&mut h, doc, MATERIAL);
    let result = vm.result.expect("a generated test");
    assert_eq!(result.cells, 25, "the default grid is 5 speeds × 5 powers");
    assert_eq!((result.errors, result.warnings), (0, 0));
    assert!(result.preview.estimated_seconds > 0.0);
    assert!(!result.stale);

    let after = h.document(doc).unwrap();
    assert_eq!((after.revision, after.dirty, after.objects.len()), (before.revision, before.dirty, 0));
    assert!(after.plan.is_none(), "the test is not the document's plan");
    assert!(after.operations.is_empty(), "no operation was added to the project");
}

#[test]
fn one_axis_and_the_cell_mode_decide_the_cell_count() {
    let (mut h, doc) = rig(Support::Unsupported);
    open(&mut h, doc, MATERIAL);
    flag(&mut h, doc, MATERIAL, DialogField::RowsEnabled, false);
    text(&mut h, doc, MATERIAL, DialogField::ColumnCount, "3");
    let vm = view(&mut h, doc, MATERIAL).unwrap();
    assert!(matches!(vm.row(DialogField::RowCount).unwrap().state, FieldState::Disabled(None)));
    assert_eq!(generate(&mut h, doc, MATERIAL).result.unwrap().cells, 3);

    // Fill cells: the interval and overscan fields are live, and the interval can be the axis.
    choose(&mut h, doc, MATERIAL, DialogField::CellMode, DialogChoice::CellFill);
    let vm = view(&mut h, doc, MATERIAL).unwrap();
    assert!(vm.result.is_none(), "a changed field drops the earlier result");
    assert_eq!(vm.row(DialogField::FillInterval).unwrap().state, FieldState::Editable);
    choose(&mut h, doc, MATERIAL, DialogField::ColumnParameter, DialogChoice::ParamInterval);
    text(&mut h, doc, MATERIAL, DialogField::ColumnMin, "0,05");
    text(&mut h, doc, MATERIAL, DialogField::ColumnMax, "0,2");
    assert_eq!(generate(&mut h, doc, MATERIAL).result.unwrap().cells, 3);
}

#[test]
fn bad_fields_are_said_on_their_row_and_no_task_is_started() {
    let (mut h, doc) = rig(Support::Unsupported);
    open(&mut h, doc, MATERIAL);
    let refuse = |h: &mut HeadlessEditorHarness, edits: &[(DialogField, &str)], row: DialogField, why: DialogError| {
        for (field, value) in edits {
            text(h, doc, MATERIAL, *field, value);
        }
        press(h, doc, MATERIAL, DialogAction::Generate);
        assert!(h.take_task_requests().is_empty(), "{row:?}: no job is made for a refused spec");
        let vm = view(h, doc, MATERIAL).unwrap();
        assert_eq!(error_of(&vm, row), Some(why), "{row:?}");
        assert!(vm.result.is_none());
        // Reopen to start from the defaults again.
        press(h, doc, MATERIAL, DialogAction::Close);
        open(h, doc, MATERIAL);
    };
    refuse(&mut h, &[(DialogField::ColumnCount, "26")], DialogField::ColumnCount, DialogError::Count);
    refuse(&mut h, &[(DialogField::ColumnCount, "0")], DialogField::ColumnCount, DialogError::Count);
    refuse(&mut h, &[(DialogField::ColumnCount, "x")], DialogField::ColumnCount, DialogError::Invalid);
    refuse(&mut h, &[(DialogField::RowCount, "2,5")], DialogField::RowCount, DialogError::NotWhole);
    refuse(&mut h, &[(DialogField::ColumnMin, "60")], DialogField::ColumnMin, DialogError::OutOfRange);
    refuse(&mut h, &[(DialogField::BaseSpeed, "-1")], DialogField::BaseSpeed, DialogError::Invalid);
    refuse(&mut h, &[(DialogField::BasePower, "101")], DialogField::BasePower, DialogError::OutOfRange);
    refuse(&mut h, &[(DialogField::BasePasses, "0")], DialogField::BasePasses, DialogError::OutOfRange);
    refuse(&mut h, &[(DialogField::CellWidth, "0")], DialogField::CellWidth, DialogError::Size);
    refuse(&mut h, &[(DialogField::CellGap, "")], DialogField::CellGap, DialogError::Invalid);
    // 11 × 10 cells is more than one test may have.
    refuse(
        &mut h,
        &[(DialogField::ColumnCount, "11"), (DialogField::RowCount, "10")],
        DialogField::ColumnCount,
        DialogError::TooManyCells,
    );
}

#[test]
fn parameters_that_do_not_fit_are_refused_in_words() {
    let (mut h, doc) = rig(Support::Unsupported);
    let vm = open(&mut h, doc, MATERIAL);
    // Interval on a line cell; the same parameter twice; a focus offset without a Z axis.
    choose(&mut h, doc, MATERIAL, DialogField::ColumnParameter, DialogChoice::ParamInterval);
    press(&mut h, doc, MATERIAL, DialogAction::Generate);
    let vm2 = view(&mut h, doc, MATERIAL).unwrap();
    assert_eq!(error_of(&vm2, DialogField::ColumnParameter), Some(DialogError::ParameterNotForMode));

    choose(&mut h, doc, MATERIAL, DialogField::ColumnParameter, DialogChoice::ParamSpeed);
    choose(&mut h, doc, MATERIAL, DialogField::RowParameter, DialogChoice::ParamSpeed);
    press(&mut h, doc, MATERIAL, DialogAction::Generate);
    let vm3 = view(&mut h, doc, MATERIAL).unwrap();
    assert_eq!(error_of(&vm3, DialogField::RowParameter), Some(DialogError::SameParameter));

    choose(&mut h, doc, MATERIAL, DialogField::RowParameter, DialogChoice::ParamZOffset);
    press(&mut h, doc, MATERIAL, DialogAction::Generate);
    let vm4 = view(&mut h, doc, MATERIAL).unwrap();
    assert_eq!(error_of(&vm4, DialogField::RowParameter), Some(DialogError::Unsupported(Feature::ZStep)));
    assert!(
        vm.row(DialogField::RowParameter)
            .unwrap()
            .blocked_choices
            .contains(&(DialogChoice::ParamZOffset, MessageKey::MissingCapability(Feature::ZStep))),
        "the pop-up shows why the entry cannot be chosen"
    );
    assert!(h.take_task_requests().is_empty());
}

#[test]
fn a_changed_field_cancels_the_job_in_flight_and_its_late_result_is_ignored() {
    let (mut h, doc) = rig(Support::Unsupported);
    open(&mut h, doc, MATERIAL);
    press(&mut h, doc, MATERIAL, DialogAction::Generate);
    assert!(view(&mut h, doc, MATERIAL).unwrap().busy);
    let requests = h.take_task_requests();
    assert_eq!(requests.len(), 1);
    text(&mut h, doc, MATERIAL, DialogField::BaseSpeed, "30");
    assert!(requests[0].cancel.is_cancelled(), "the old job is cancelled");
    assert!(!view(&mut h, doc, MATERIAL).unwrap().busy);
    // The worker finished before it saw the cancellation: the controller drops the result.
    for r in requests {
        h.complete_despite_cancel(r);
    }
    assert!(view(&mut h, doc, MATERIAL).unwrap().result.is_none());
}

#[test]
fn only_the_newest_generate_counts() {
    let (mut h, doc) = rig(Support::Unsupported);
    open(&mut h, doc, MATERIAL);
    flag(&mut h, doc, MATERIAL, DialogField::RowsEnabled, false);
    text(&mut h, doc, MATERIAL, DialogField::ColumnCount, "2");
    press(&mut h, doc, MATERIAL, DialogAction::Generate);
    let first = h.take_task_requests();
    // A second Generate for another count, without a field edit in between (same draft again).
    text(&mut h, doc, MATERIAL, DialogField::ColumnCount, "4");
    press(&mut h, doc, MATERIAL, DialogAction::Generate);
    let second = h.take_task_requests();
    assert!(first[0].cancel.is_cancelled());
    for r in second {
        h.complete(r);
    }
    for r in first {
        h.complete_despite_cancel(r);
    }
    assert_eq!(view(&mut h, doc, MATERIAL).unwrap().result.unwrap().cells, 4, "the older job did not replace it");
}

#[test]
fn an_interval_test_takes_millimetres_or_lines_per_inch() {
    let (mut h, doc) = rig(Support::Unsupported);
    open(&mut h, doc, INTERVAL);
    text(&mut h, doc, INTERVAL, DialogField::IntervalMin, "254 lpi");
    text(&mut h, doc, INTERVAL, DialogField::IntervalMax, "0,25 mm");
    text(&mut h, doc, INTERVAL, DialogField::IntervalCount, "4");
    let vm = generate(&mut h, doc, INTERVAL);
    assert_eq!(vm.result.expect("generated").cells, 4);

    for (field, value, why) in [
        (DialogField::IntervalMin, "0", DialogError::Invalid),
        (DialogField::IntervalMax, "0,05 mm", DialogError::OutOfRange),
        (DialogField::IntervalCount, "30", DialogError::Count),
    ] {
        press(&mut h, doc, INTERVAL, DialogAction::Close);
        open(&mut h, doc, INTERVAL);
        text(&mut h, doc, INTERVAL, field, value);
        press(&mut h, doc, INTERVAL, DialogAction::Generate);
        assert_eq!(error_of(&view(&mut h, doc, INTERVAL).unwrap(), field), Some(why), "{field:?}");
    }
}

#[test]
fn a_focus_test_is_greyed_with_its_reason_without_a_z_axis() {
    let (mut h, doc) = rig(Support::Unsupported);
    let vm = open(&mut h, doc, FOCUS);
    assert!(vm.notes.contains(&DialogNote::FocusNeedsZ));
    let generate_button = vm.action(DialogActionKind::Generate).unwrap();
    assert!(!generate_button.enabled);
    assert_eq!(generate_button.reason, Some(DialogNote::FocusNeedsZ));
    assert_eq!(
        vm.row(DialogField::FocusStart).unwrap().state,
        FieldState::Disabled(Some(MessageKey::MissingCapability(Feature::ZStep)))
    );
    press(&mut h, doc, FOCUS, DialogAction::Generate);
    assert!(h.take_task_requests().is_empty(), "no job without Z");
    assert!(view(&mut h, doc, FOCUS).unwrap().result.is_none());
}

#[test]
fn a_focus_test_with_z_cuts_one_line_per_offset() {
    let (mut h, doc) = rig(Support::Supported);
    let vm = open(&mut h, doc, FOCUS);
    assert!(!vm.notes.contains(&DialogNote::FocusNeedsZ));
    text(&mut h, doc, FOCUS, DialogField::FocusStart, "-1");
    text(&mut h, doc, FOCUS, DialogField::FocusStep, "0,5");
    text(&mut h, doc, FOCUS, DialogField::FocusCount, "5");
    let vm = generate(&mut h, doc, FOCUS);
    let result = vm.result.expect("generated");
    assert_eq!(result.cells, 5);
    assert_eq!(result.errors, 0);

    for (field, value, why) in [
        (DialogField::FocusStep, "0", DialogError::OutOfRange),
        (DialogField::FocusCount, "26", DialogError::Count),
        (DialogField::FocusStart, "-60", DialogError::OutOfRange),
        (DialogField::LineLength, "0", DialogError::Size),
    ] {
        press(&mut h, doc, FOCUS, DialogAction::Close);
        open(&mut h, doc, FOCUS);
        text(&mut h, doc, FOCUS, field, value);
        press(&mut h, doc, FOCUS, DialogAction::Generate);
        assert_eq!(error_of(&view(&mut h, doc, FOCUS).unwrap(), field), Some(why), "{field:?}");
    }
}

#[test]
fn a_test_is_written_as_rd_only_on_request_and_never_into_the_project() {
    let (mut h, doc) = rig(Support::Unsupported);
    open(&mut h, doc, MATERIAL);
    choose(&mut h, doc, MATERIAL, DialogField::CellMode, DialogChoice::CellFill);
    flag(&mut h, doc, MATERIAL, DialogField::RowsEnabled, false);
    text(&mut h, doc, MATERIAL, DialogField::ColumnCount, "3");
    assert!(!view(&mut h, doc, MATERIAL).unwrap().action(DialogActionKind::ExportRd).unwrap().enabled);
    generate(&mut h, doc, MATERIAL);
    assert!(view(&mut h, doc, MATERIAL).unwrap().action(DialogActionKind::ExportRd).unwrap().enabled);

    let path = std::path::PathBuf::from("/out/material-test.rd");
    press(&mut h, doc, MATERIAL, DialogAction::ExportRd(path.clone()));
    h.run_tasks();
    let bytes = burnburn_io::fs::FileSystem::read(h.fs(), &path, 1 << 24).expect("the RD file was written");
    assert!(!bytes.is_empty());
    let vm = view(&mut h, doc, MATERIAL).unwrap();
    assert_eq!(vm.result.as_ref().unwrap().saved_to.as_deref(), Some(path.as_path()));
    assert!(vm.notes.contains(&DialogNote::TestSaved));
    let after = h.document(doc).unwrap();
    assert!(after.export.is_none() && !after.dirty, "the project knows nothing about the test file");
}

#[test]
fn a_focus_test_has_no_rd_form_yet_and_says_so() {
    let (mut h, doc) = rig(Support::Supported);
    open(&mut h, doc, FOCUS);
    generate(&mut h, doc, FOCUS);
    let path = std::path::PathBuf::from("/out/focus-test.rd");
    press(&mut h, doc, FOCUS, DialogAction::ExportRd(path.clone()));
    h.run_tasks();
    assert!(burnburn_io::fs::FileSystem::read(h.fs(), &path, 1 << 20).is_err(), "no file was written");
    let vm = view(&mut h, doc, FOCUS).unwrap();
    assert!(vm.notes.contains(&DialogNote::RdRefused(RdRefusal::FocusOffset)));
    assert!(vm.result.is_some(), "the plan and its preview remain");
}

#[test]
fn a_profile_change_makes_the_test_stale_and_without_a_profile_nothing_is_generated() {
    let (mut h, doc) = rig(Support::Unsupported);
    open(&mut h, doc, MATERIAL);
    assert!(!generate(&mut h, doc, MATERIAL).result.unwrap().stale);
    h.send(UiIntent::SetMachineProfile { profile: Box::new(profile(2)), ruida: RUIDA });
    let vm = view(&mut h, doc, MATERIAL).unwrap();
    assert!(vm.result.as_ref().unwrap().stale);
    assert!(!vm.action(DialogActionKind::ExportRd).unwrap().enabled, "a stale test is not written");

    let mut bare = HeadlessEditorHarness::new();
    let doc = bare.new_document().unwrap();
    let vm = open(&mut bare, doc, MATERIAL);
    assert!(vm.notes.contains(&DialogNote::NoMachineProfile));
    assert!(!vm.action(DialogActionKind::Generate).unwrap().enabled);
    press(&mut bare, doc, MATERIAL, DialogAction::Generate);
    assert!(bare.take_task_requests().is_empty());
}

#[test]
fn close_drops_the_draft_and_the_result() {
    let (mut h, doc) = rig(Support::Unsupported);
    open(&mut h, doc, MATERIAL);
    text(&mut h, doc, MATERIAL, DialogField::BaseSpeed, "33");
    generate(&mut h, doc, MATERIAL);
    press(&mut h, doc, MATERIAL, DialogAction::Close);
    assert!(view(&mut h, doc, MATERIAL).is_none());
    let vm = open(&mut h, doc, MATERIAL);
    assert_eq!(vm.text(DialogField::BaseSpeed), "20", "a reopened dialog starts from the defaults");
    assert!(vm.result.is_none());
}
