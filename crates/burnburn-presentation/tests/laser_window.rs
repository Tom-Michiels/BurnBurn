//! W2-L1-laser, window level (M05, M06, M07): the laser window through the real `AppController`,
//! the machine service over the real execution actor, a scripted fake controller and a fake clock.
//! Start from / 3×3 job origin / cut selected / selection origin feed the compile; frame, jog,
//! move, position laser and the controller files go to the machine only through the service.
//! No socket, no real laser.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

use burnburn_test_support::host::rig as common;

use burnburn_io::fs::FileSystem;
use burnburn_job::MoveKind;
use burnburn_machine::{
    ControllerFileName, FileEntry, FrameShape, HomeTarget, JobAnchor, JogKey, LaserAction, LiveFeature, LiveRequest,
    Reply, Request, StartFrom,
};
use burnburn_presentation::{
    CommandKey, HeadlessEditorHarness, LaserCommand, LaserControlKind, LaserSetup, LaserViewModel, LaserWhy,
    MachineAction, MachinePort, MachineStatusKey, SelectMode, UiIntent,
};
use burnburn_test_support::Step;
use burnburn_types::DocumentId;

use common::laser::{Lab, all_supported, lab};
use common::*;

struct Win {
    h: HeadlessEditorHarness,
    doc: DocumentId,
    lab: Lab,
}

/// One window with a 120 × 80 rectangle (10,10)–(130,90) and a cut operation. `supported`: the
/// controller's live features are switched on (otherwise every one is unverified).
fn win(supported: bool) -> Win {
    let lab = lab();
    if supported {
        lab.service.set_capabilities(all_supported());
    }
    let mut h = HeadlessEditorHarness::new();
    h.controller().attach_machine(lab.service.clone());
    let doc = h.new_document().expect("window");
    design(&mut h, doc);
    Win { h, doc, lab }
}

fn settle(w: &Win) {
    for _ in 0..8 {
        w.lab.service.poll().unwrap();
    }
}

fn view(w: &mut Win) -> LaserViewModel {
    w.h.controller().laser_view().expect("laser window")
}

/// Uses the control of `kind` exactly as a host does: id and seen state copied from the view model.
fn use_control(w: &mut Win, kind: LaserControlKind, command: LaserCommand) {
    let vm = view(w);
    let control = vm.control(&kind).unwrap_or_else(|| panic!("no {kind:?} control"));
    let intent = UiIntent::Laser { document: Some(w.doc), action: control.action, seen: vm.seen.clone(), command };
    w.h.send(intent);
    settle(w);
}

fn setup(w: &mut Win, setup: LaserSetup) {
    w.h.send(UiIntent::LaserSetup { setup });
}

fn live_requests(w: &Win) -> Vec<LiveRequest> {
    w.lab
        .peer
        .sent()
        .into_iter()
        .filter_map(|r| match r {
            Request::Live(l) => Some(l),
            _ => None,
        })
        .collect()
}

/// The head is at `(x, y)` mm: the controller answers the position read.
fn head_at(w: &mut Win, x_mm: f64, y_mm: f64) {
    let (x_um, y_um) = ((x_mm * 1000.0) as i64, (y_mm * 1000.0) as i64);
    w.lab.peer.script(
        Step::expect(Request::Live(LiveRequest::ReadPosition))
            .reply(Reply::Ack)
            .reply(Reply::Position { x_um: Some(x_um), y_um: Some(y_um) }),
    );
    use_control(w, LaserControlKind::ShowPosition, LaserCommand::Action(LaserAction::ShowPosition));
}

/// Bounds (min, max) of the burning moves of the compiled plan the window shows.
fn burn_bounds(w: &Win) -> ((f64, f64), (f64, f64)) {
    let plan = w.h.document(w.doc).unwrap().plan.expect("a compiled plan");
    let mut points = Vec::new();
    for m in plan.preview.moves.iter().filter(|m| matches!(m.kind, MoveKind::Cut)) {
        points.push((m.from.x_mm(), m.from.y_mm()));
        points.push((m.to.x_mm(), m.to.y_mm()));
    }
    let min = points.iter().fold((f64::MAX, f64::MAX), |a, p| (a.0.min(p.0), a.1.min(p.1)));
    let max = points.iter().fold((f64::MIN, f64::MIN), |a, p| (a.0.max(p.0), a.1.max(p.1)));
    (min, max)
}

fn compile(w: &mut Win) {
    w.h.invoke(w.doc, CommandKey::Compile);
    w.h.run_tasks();
}

fn assert_close(got: (f64, f64), want: (f64, f64)) {
    assert!((got.0 - want.0).abs() < 1e-6 && (got.1 - want.1).abs() < 1e-6, "{got:?} != {want:?}");
}

/// Adds a second 60 × 40 rectangle at (200,10) with its own cut operation; the first stays as is.
fn add_second_rectangle(w: &mut Win) -> burnburn_types::GeometryId {
    w.h.invoke(w.doc, CommandKey::ToolRectangle);
    w.h.drag(w.doc, (200.0, 10.0), (260.0, 50.0));
    let second = w.h.document(w.doc).unwrap().objects.last().unwrap().id;
    w.h.send(UiIntent::Select { document: w.doc, ids: vec![second], mode: SelectMode::Replace });
    w.h.send(UiIntent::AddOperation { document: w.doc, draft: cut() });
    w.h.invoke(w.doc, CommandKey::ToolSelect);
    second
}

// --- the controls follow the state of the machine -------------------------------------------------

#[test]
fn nothing_is_offered_that_has_no_wire_evidence_and_the_reason_is_typed() {
    let mut w = win(false);
    let vm = view(&mut w);
    for kind in [
        LaserControlKind::Jog(JogKey::XLeft),
        LaserControlKind::Home,
        LaserControlKind::MoveTo,
        LaserControlKind::Pause,
        LaserControlKind::ShowPosition,
        LaserControlKind::ListFiles,
    ] {
        let c = vm.control(&kind).unwrap();
        assert!(!c.enabled, "{kind:?} must be off without evidence");
        assert!(matches!(c.why_not, Some(LaserWhy::NotVerified { .. })), "{kind:?}: {:?}", c.why_not);
    }
    let Some(LaserWhy::NotVerified { feature, needs }) = vm.control(&LaserControlKind::Home).unwrap().why_not else {
        panic!()
    };
    assert_eq!(feature, LiveFeature::Home);
    assert!(needs.contains("HOME_XY"), "the missing evidence is named: {needs}");

    // A forged intent for a disabled control is refused and leaves nothing: no wire, no journal.
    let home = vm.control(&LaserControlKind::Home).unwrap().clone();
    w.h.send(UiIntent::Laser {
        document: Some(w.doc),
        action: home.action,
        seen: None,
        command: LaserCommand::Action(LaserAction::Home(HomeTarget::Xy)),
    });
    assert!(matches!(view(&mut w).last_refusal, Some(LaserWhy::NotVerified { .. })));
    assert!(w.lab.peer.sent().is_empty());
    assert!(w.lab.history.intents.lock().unwrap().is_empty());
}

#[test]
fn the_position_reads_unknown_never_zero_until_the_controller_answers() {
    let mut w = win(true);
    let vm = view(&mut w);
    assert_eq!((vm.position.x_mm, vm.position.y_mm), (None, None));
    assert!(!vm.offers(&LaserControlKind::SetOrigin), "an origin needs a known position");
    assert!(!vm.offers(&LaserControlKind::Step(burnburn_presentation::StepDirection::Left)));
    assert_eq!(vm.control(&LaserControlKind::SetOrigin).unwrap().why_not, Some(LaserWhy::PositionUnknown));

    head_at(&mut w, 120.5, 80.0);
    let vm = view(&mut w);
    assert_eq!((vm.position.x_mm, vm.position.y_mm), (Some(120.5), Some(80.0)));
    assert!(vm.offers(&LaserControlKind::SetOrigin));

    // A motion command makes it unknown again until it is read.
    w.lab.peer.script(Step::expect(Request::Live(LiveRequest::Home(HomeTarget::Xy))).reply(Reply::Ack));
    use_control(&mut w, LaserControlKind::Home, LaserCommand::Action(LaserAction::Home(HomeTarget::Xy)));
    let vm = view(&mut w);
    assert_eq!((vm.position.x_mm, vm.position.y_mm), (None, None));
}

#[test]
fn a_job_in_progress_switches_off_the_motion_but_not_pause() {
    let mut w = win(true);
    let job = prepare(&mut w.h, w.doc);
    script_upload_acks(&w.lab.peer, &job);
    w.h.press(Some(w.doc), job.upload.as_ref().expect("upload"));
    settle(&w);
    let vm = view(&mut w);
    assert_eq!(vm.control(&LaserControlKind::Home).unwrap().why_not, Some(LaserWhy::JobMayBeActive));
    assert_eq!(vm.control(&LaserControlKind::MoveTo).unwrap().why_not, Some(LaserWhy::JobMayBeActive));
    assert!(vm.offers(&LaserControlKind::Pause));
    assert_eq!(
        vm.control(&LaserControlKind::Frame(FrameShape::RubberBand)).unwrap().why_not,
        Some(LaserWhy::JobMayBeActive)
    );
}

// --- jog, move, origin, position laser -------------------------------------------------------------

#[test]
fn holding_a_key_jogs_and_losing_focus_releases_it() {
    let mut w = win(true);
    w.lab.peer.script(Step::expect(Request::Live(LiveRequest::JogStart(JogKey::XLeft))).reply(Reply::Ack));
    use_control(
        &mut w,
        LaserControlKind::Jog(JogKey::XLeft),
        LaserCommand::Action(LaserAction::JogPress(JogKey::XLeft)),
    );
    let vm = view(&mut w);
    assert_eq!(vm.jog_held, Some(JogKey::XLeft));
    assert!(vm.jog_release.is_some(), "the key-up has its own idempotency key");

    // Focus lost, window closed or panel hidden: the host sends this, nothing else.
    w.lab.peer.script(Step::expect(Request::Live(LiveRequest::JogStop(JogKey::XLeft))).reply(Reply::Ack));
    w.h.send(UiIntent::LaserReleaseJog);
    settle(&w);
    assert_eq!(view(&mut w).jog_held, None);
    assert_eq!(live_requests(&w), vec![LiveRequest::JogStart(JogKey::XLeft), LiveRequest::JogStop(JogKey::XLeft)]);

    // Releasing again sends nothing.
    w.h.send(UiIntent::LaserReleaseJog);
    settle(&w);
    assert_eq!(live_requests(&w).len(), 2);
}

#[test]
fn a_numeric_move_is_parsed_here_and_goes_to_the_machine_frame_in_micrometres() {
    let mut w = win(true);
    w.lab.peer.script(
        Step::expect(Request::Live(LiveRequest::MoveAbsolute { x_um: 150_250, y_um: 40_000 })).reply(Reply::Ack),
    );
    use_control(&mut w, LaserControlKind::MoveTo, LaserCommand::MoveToText { x: "150,25 mm".into(), y: "40".into() });
    assert_eq!(live_requests(&w), vec![LiveRequest::MoveAbsolute { x_um: 150_250, y_um: 40_000 }]);

    use_control(&mut w, LaserControlKind::MoveTo, LaserCommand::MoveToText { x: "abc".into(), y: "1".into() });
    assert_eq!(view(&mut w).last_refusal, Some(LaserWhy::InvalidNumber));
    use_control(&mut w, LaserControlKind::MoveTo, LaserCommand::MoveToText { x: "5000".into(), y: "1".into() });
    assert_eq!(view(&mut w).last_refusal, Some(LaserWhy::OutsideBed));
    assert_eq!(live_requests(&w).len(), 1, "refused moves never reach the controller");
}

#[test]
fn the_user_origin_is_set_from_a_known_position_and_go_to_origin_uses_it() {
    let mut w = win(true);
    setup(&mut w, LaserSetup::SetOriginHere);
    assert_eq!(view(&mut w).setup.user_origin_mm, None, "an unknown position sets no origin, not (0, 0)");
    assert_eq!(view(&mut w).last_refusal, Some(LaserWhy::PositionUnknown));
    assert_eq!(
        view(&mut w).control(&LaserControlKind::GoToUserOrigin).unwrap().why_not,
        Some(LaserWhy::UserOriginNotSet)
    );

    head_at(&mut w, 300.0, 200.0);
    setup(&mut w, LaserSetup::SetOriginHere);
    assert_eq!(view(&mut w).setup.user_origin_mm, Some((300.0, 200.0)));

    w.lab.peer.script(
        Step::expect(Request::Live(LiveRequest::MoveAbsolute { x_um: 300_000, y_um: 200_000 })).reply(Reply::Ack),
    );
    use_control(
        &mut w,
        LaserControlKind::GoToUserOrigin,
        LaserCommand::Action(LaserAction::GoToOrigin(burnburn_machine::OriginTarget::User)),
    );
    assert!(live_requests(&w).contains(&LiveRequest::MoveAbsolute { x_um: 300_000, y_um: 200_000 }));
}

#[test]
fn position_laser_turns_a_canvas_click_into_a_move_and_selects_nothing() {
    let mut w = win(true);
    w.h.send(UiIntent::Select { document: w.doc, ids: vec![], mode: SelectMode::Replace });
    setup(&mut w, LaserSetup::PositionLaserMode(true));
    w.lab.peer.script(
        Step::expect(Request::Live(LiveRequest::MoveAbsolute { x_um: 50_000, y_um: 40_000 })).reply(Reply::Ack),
    );
    // (50, 40) lies inside the rectangle: without the mode this click would select it.
    w.h.click(w.doc, (50.0, 40.0), Default::default());
    settle(&w);
    assert_eq!(live_requests(&w), vec![LiveRequest::MoveAbsolute { x_um: 50_000, y_um: 40_000 }]);
    assert!(w.h.document(w.doc).unwrap().objects.iter().all(|o| !o.selected), "the click moved the laser only");

    setup(&mut w, LaserSetup::PositionLaserMode(false));
    w.h.click(w.doc, (50.0, 40.0), Default::default());
    assert!(w.h.document(w.doc).unwrap().objects.iter().any(|o| o.selected), "mode off: a click selects again");
}

// --- start from, 3x3 job origin, cut selected, selection origin ------------------------------------

#[test]
fn start_from_current_position_puts_the_chosen_anchor_of_the_job_on_the_head() {
    let mut w = win(true);
    head_at(&mut w, 500.0, 400.0);
    setup(&mut w, LaserSetup::StartFrom(StartFrom::CurrentPosition));
    setup(&mut w, LaserSetup::Anchor(JobAnchor::TopLeft));
    compile(&mut w);
    // The job's top-left corner (10, 90) lands on (500, 400); the job is 120 × 80.
    let (min, max) = burn_bounds(&w);
    assert_close(min, (500.0, 320.0));
    assert_close(max, (620.0, 400.0));
    let placement = view(&mut w).placement.expect("placement shown");
    assert_close(placement.delta_mm, (490.0, 310.0));
    assert_eq!(placement.from_position_mm, Some((500.0, 400.0)));
    assert!(!placement.setup_changed);

    setup(&mut w, LaserSetup::Anchor(JobAnchor::Center));
    compile(&mut w);
    let (min, max) = burn_bounds(&w);
    assert_close(min, (440.0, 360.0));
    assert_close(max, (560.0, 440.0));
}

#[test]
fn an_unknown_head_position_refuses_the_compile_and_keeps_the_old_plan() {
    let mut w = win(true);
    compile(&mut w);
    let before = w.h.document(w.doc).unwrap().plan.unwrap().id;

    setup(&mut w, LaserSetup::StartFrom(StartFrom::CurrentPosition));
    let update = w.h.invoke(w.doc, CommandKey::Compile);
    w.h.run_tasks();
    assert!(!update.effects.is_empty(), "the refusal is reported");
    assert_eq!(view(&mut w).last_refusal, Some(LaserWhy::PositionUnknown));
    assert_eq!(w.h.document(w.doc).unwrap().plan.unwrap().id, before, "no plan was invented at (0, 0)");
}

#[test]
fn a_user_origin_that_is_not_set_refuses_the_compile() {
    let mut w = win(true);
    setup(&mut w, LaserSetup::StartFrom(StartFrom::UserOrigin));
    w.h.invoke(w.doc, CommandKey::Compile);
    w.h.run_tasks();
    assert_eq!(view(&mut w).last_refusal, Some(LaserWhy::UserOriginNotSet));
    assert!(w.h.document(w.doc).unwrap().plan.is_none());
}

#[test]
fn cut_selected_compiles_only_the_selected_objects() {
    let mut w = win(true);
    let _second = add_second_rectangle(&mut w);
    compile(&mut w);
    assert_close(burn_bounds(&w).1, (260.0, 90.0));

    let first = w.h.document(w.doc).unwrap().objects[0].id;
    w.h.send(UiIntent::Select { document: w.doc, ids: vec![first], mode: SelectMode::Replace });
    setup(&mut w, LaserSetup::CutSelected(true));
    compile(&mut w);
    let (min, max) = burn_bounds(&w);
    assert_close(min, (10.0, 10.0));
    assert_close(max, (130.0, 90.0));

    // Nothing selected: refused, the plan stays.
    w.h.send(UiIntent::Select { document: w.doc, ids: vec![], mode: SelectMode::Replace });
    let before = w.h.document(w.doc).unwrap().plan.unwrap().id;
    w.h.invoke(w.doc, CommandKey::Compile);
    w.h.run_tasks();
    assert_eq!(view(&mut w).last_refusal, Some(LaserWhy::NothingSelected));
    assert_eq!(w.h.document(w.doc).unwrap().plan.unwrap().id, before);
}

#[test]
fn use_selection_origin_places_by_the_selection_while_everything_is_still_cut() {
    let mut w = win(true);
    let second = add_second_rectangle(&mut w);
    w.h.send(UiIntent::Select { document: w.doc, ids: vec![second], mode: SelectMode::Replace });
    head_at(&mut w, 700.0, 500.0);
    setup(&mut w, LaserSetup::StartFrom(StartFrom::CurrentPosition));
    setup(&mut w, LaserSetup::Anchor(JobAnchor::BottomLeft));
    setup(&mut w, LaserSetup::UseSelectionOrigin(true));
    compile(&mut w);
    // The selection's bottom-left (200, 10) lands on (700, 500): shift (+500, +490). Both are cut.
    let (min, max) = burn_bounds(&w);
    assert_close(min, (510.0, 500.0));
    assert_close(max, (760.0, 580.0));
}

#[test]
fn a_changed_setup_makes_the_compiled_plan_stale_until_it_is_compiled_again() {
    let mut w = win(true);
    head_at(&mut w, 500.0, 400.0);
    setup(&mut w, LaserSetup::StartFrom(StartFrom::CurrentPosition));
    compile(&mut w);
    assert!(!view(&mut w).placement.unwrap().setup_changed);

    setup(&mut w, LaserSetup::Anchor(JobAnchor::Center));
    assert!(view(&mut w).placement.unwrap().setup_changed);
    w.h.invoke(w.doc, CommandKey::PrepareJob);
    w.h.run_tasks();
    assert!(w.h.document(w.doc).unwrap().job.is_none(), "a plan of another setup is not frozen");
    assert_eq!(
        view(&mut w).control(&LaserControlKind::Frame(FrameShape::BoundingRectangle)).unwrap().why_not,
        Some(LaserWhy::PlacementStale)
    );
}

#[test]
fn a_job_placed_on_the_head_is_not_uploaded_once_the_head_has_moved() {
    let mut w = win(true);
    head_at(&mut w, 500.0, 400.0);
    setup(&mut w, LaserSetup::StartFrom(StartFrom::CurrentPosition));
    compile(&mut w);
    w.h.invoke(w.doc, CommandKey::PrepareJob);
    w.h.run_tasks();
    let job = w.h.document(w.doc).unwrap().job.expect("prepared job");

    w.lab.peer.script(Step::expect(Request::Live(LiveRequest::Home(HomeTarget::Xy))).reply(Reply::Ack));
    use_control(&mut w, LaserControlKind::Home, LaserCommand::Action(LaserAction::Home(HomeTarget::Xy)));
    let before = w.lab.peer.sent().len();
    let update = w.h.press(Some(w.doc), job.upload.as_ref().expect("upload control"));
    settle(&w);
    assert!(!update.effects.is_empty(), "the refusal is reported");
    assert_eq!(w.lab.peer.sent().len(), before, "nothing of the stale job reached the controller");
}

// --- frame, save RD, files, diagnostics -------------------------------------------------------------

#[test]
fn frame_is_off_until_a_plan_is_reviewed_and_never_replaces_the_prepared_job() {
    let mut w = win(true);
    let kind = LaserControlKind::Frame(FrameShape::RubberBand);
    assert_eq!(view(&mut w).control(&kind).unwrap().why_not, Some(LaserWhy::NoReviewedPlan));

    let job = prepare(&mut w.h, w.doc);
    assert!(view(&mut w).offers(&kind));

    script_identity(&w.lab.peer, MODEL);
    for i in 0..8 {
        w.lab.peer.script(
            Step::expect_matching(format!("frame chunk {i}"), |r| matches!(r, Request::UploadChunk { .. }))
                .reply(Reply::Ack),
        );
    }
    use_control(&mut w, kind, LaserCommand::Frame(FrameShape::RubberBand));
    let status = w.lab.service.status();
    assert!(status.current_is_frame, "the execution is a frame run");
    assert_eq!(w.h.machine_view().unwrap().status, MachineStatusKey::UploadedNotConfirmed);
    assert_ne!(status.current.unwrap().execution.plan, job.plan, "the frame has its own plan");
    assert_eq!(w.h.document(w.doc).unwrap().job.unwrap().plan, job.plan, "the prepared job is untouched");
    let records = w.lab.history.inner.records.lock().unwrap();
    assert!(
        records.iter().any(|(r, _)| r.action.kind == MachineAction::Frame),
        "the upload is recorded as a frame, so it never counts as a produced piece"
    );
    assert!(!w.lab.peer.sent().contains(&Request::Start), "framing does not start anything");
}

#[test]
fn save_rd_to_the_controller_is_a_refusal_in_words_and_never_a_start() {
    for supported in [false, true] {
        let mut w = win(supported);
        prepare(&mut w.h, w.doc);
        let before = w.lab.peer.sent().len();
        use_control(&mut w, LaserControlKind::SaveRd, LaserCommand::SaveRd { name: "PLAAT1".into() });
        let why = view(&mut w).last_refusal;
        if supported {
            assert_eq!(why, Some(LaserWhy::NoWireForm(LiveFeature::FileUpload)));
        } else {
            assert!(matches!(why, Some(LaserWhy::NotVerified { feature: LiveFeature::FileUpload, .. })), "{why:?}");
        }
        assert_eq!(w.lab.peer.sent().len(), before);
        assert!(!w.lab.peer.sent().contains(&Request::Start));
    }
    let mut w = win(true);
    prepare(&mut w.h, w.doc);
    use_control(&mut w, LaserControlKind::SaveRd, LaserCommand::SaveRd { name: String::new() });
    assert_eq!(view(&mut w).last_refusal, Some(LaserWhy::InvalidFileName));
}

#[test]
fn the_controller_file_list_is_kept_and_a_missing_size_reads_unknown() {
    let mut w = win(true);
    let files = vec![
        FileEntry { name: ControllerFileName::new("PLAAT1").unwrap(), size_bytes: Some(2048) },
        FileEntry { name: ControllerFileName::new("PLAAT2").unwrap(), size_bytes: None },
    ];
    w.lab.peer.script(Step::expect(Request::Live(LiveRequest::ListFiles)).reply(Reply::Ack).reply(Reply::Files(files)));
    use_control(&mut w, LaserControlKind::ListFiles, LaserCommand::Action(LaserAction::ListFiles));
    let vm = view(&mut w);
    let rows = vm.files.clone().expect("list");
    assert_eq!((rows[0].size_bytes, rows[1].size_bytes), (Some(2048), None), "no size is not 0 bytes");

    // Run is its own request: no execution, no job start.
    let name = ControllerFileName::new("PLAAT1").unwrap();
    w.lab.peer.script(Step::expect(Request::Live(LiveRequest::RunFile(name.clone()))).reply(Reply::Ack));
    use_control(&mut w, LaserControlKind::RunFile(name.clone()), LaserCommand::Action(LaserAction::RunFile(name)));
    assert!(w.lab.service.status().current.is_none());
    assert!(!w.lab.peer.sent().contains(&Request::Start));
}

#[test]
fn operators_do_not_get_the_laser_window() {
    let mut w = win(true);
    w.h.controller().set_role(burnburn_presentation::Role::Operator);
    let update = w.h.send(UiIntent::LaserSetup { setup: LaserSetup::CutSelected(true) });
    assert!(!update.effects.is_empty(), "refused for the role");
    assert!(w.lab.peer.sent().is_empty());
}

// --- diagnostics export (M07) -----------------------------------------------------------------------

#[test]
fn exporting_diagnostics_writes_the_rendered_replay_and_touches_no_machine() {
    let mut w = win(true);
    w.h.controller().attach_history(w.lab.history.clone()).unwrap();
    let path = std::path::PathBuf::from("/exports/diagnose.txt");

    // Private data is off unless it is ticked.
    w.h.send(UiIntent::ExportDiagnostics { options: Default::default(), path: path.clone() });
    w.h.run_tasks();
    let text = String::from_utf8(w.h.fs().read(&path, 1 << 20).unwrap()).unwrap();
    assert_eq!(text, "diagnostic replay: paths=false ids=false");

    let ticked = burnburn_presentation::DiagnosticOptions { include_project_paths: true, include_machine_ids: false };
    w.h.send(UiIntent::ExportDiagnostics { options: ticked, path: path.clone() });
    w.h.run_tasks();
    let text = String::from_utf8(w.h.fs().read(&path, 1 << 20).unwrap()).unwrap();
    assert_eq!(text, "diagnostic replay: paths=true ids=false");
    assert!(w.lab.peer.sent().is_empty(), "a diagnostic export has no machine contact");
}

#[test]
fn diagnostics_without_a_history_are_refused_in_words() {
    let mut w = win(true);
    let update = w.h.send(UiIntent::ExportDiagnostics { options: Default::default(), path: "/x.txt".into() });
    assert!(!update.effects.is_empty());
    assert!(!w.h.fs().exists(std::path::Path::new("/x.txt")).unwrap());
}
