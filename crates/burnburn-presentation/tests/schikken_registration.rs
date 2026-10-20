//! W3-UI-schikken: Print and Cut (two design targets, measured with the laser, a registration
//! that moves the next compile) and the Center Finder (edge points → centre and residuals). The
//! laser is the real execution actor over a scripted fake controller; an unknown head position
//! is never taken as zero.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

mod common;

use burnburn_job::MoveKind;
use burnburn_machine::{LaserAction, LiveRequest, Reply, Request, StartFrom};
use burnburn_presentation::{
    CenterFinderIntent, CenterFinderViewModel, CenterProblem, CommandKey, HeadlessEditorHarness, LaserCommand,
    LaserControlKind, LaserSetup, PrintAndCutIntent, PrintAndCutViewModel, PrintCutScale, RegistrationFailure,
    RegistrationField, SchikkenIntent, SchikkenMessage, UiIntent,
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

/// One window with the 120 × 80 rectangle (10,10)–(130,90) and a cut operation on a 1500 × 900 bed.
fn win() -> Win {
    let lab = lab();
    lab.service.set_capabilities(all_supported());
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

/// The head is at `(x, y)` mm: the controller answers the position read.
fn head_at(w: &mut Win, x_mm: f64, y_mm: f64) {
    let (x_um, y_um) = ((x_mm * 1000.0) as i64, (y_mm * 1000.0) as i64);
    w.lab.peer.script(
        Step::expect(Request::Live(LiveRequest::ReadPosition))
            .reply(Reply::Ack)
            .reply(Reply::Position { x_um: Some(x_um), y_um: Some(y_um) }),
    );
    let vm = w.h.controller().laser_view().expect("laser window");
    let control = vm.control(&LaserControlKind::ShowPosition).expect("show position");
    w.h.send(UiIntent::Laser {
        document: Some(w.doc),
        action: control.action,
        seen: vm.seen.clone(),
        command: LaserCommand::Action(LaserAction::ShowPosition),
    });
    settle(w);
}

fn pc(w: &mut Win, intent: PrintAndCutIntent) {
    w.h.send(UiIntent::Schikken(SchikkenIntent::PrintAndCut(intent)));
}

fn cf(w: &mut Win, intent: CenterFinderIntent) {
    w.h.send(UiIntent::Schikken(SchikkenIntent::CenterFinder(intent)));
}

fn print_vm(w: &Win) -> PrintAndCutViewModel {
    w.h.document(w.doc).expect("view").schikken.print_and_cut.expect("the Print and Cut panel is open")
}

fn center_vm(w: &Win) -> CenterFinderViewModel {
    w.h.document(w.doc).expect("view").schikken.center_finder.expect("the Center Finder panel is open")
}

fn set_target(w: &mut Win, index: usize, x: &str, y: &str) {
    let document = w.doc;
    pc(w, PrintAndCutIntent::SetTarget { document, index, x: x.into(), y: y.into() });
}

fn set_measured(w: &mut Win, index: usize, x: &str, y: &str) {
    let document = w.doc;
    pc(w, PrintAndCutIntent::SetMeasured { document, index, x: x.into(), y: y.into() });
}

fn register(w: &mut Win) {
    let document = w.doc;
    pc(w, PrintAndCutIntent::Register { document });
}

fn error(w: &Win) -> Option<SchikkenMessage> {
    print_vm(w).error
}

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

fn close(a: (f64, f64), b: (f64, f64)) -> bool {
    (a.0 - b.0).abs() < 1e-6 && (a.1 - b.1).abs() < 1e-6
}

/// The two printed marks at the lower corners of the rectangle: (10,10) and (130,10), 120 apart.
fn open_with_corner_targets(w: &mut Win) {
    w.h.invoke(w.doc, CommandKey::PrintAndCut);
    set_target(w, 0, "10", "10");
    set_target(w, 1, "130", "10");
}

#[test]
fn two_measured_targets_move_the_next_compile_by_the_exact_rotation_and_translation() {
    let mut w = win();
    open_with_corner_targets(&mut w);
    // The sheet was put on the bed turned a quarter: the marks are found 120 apart, upwards.
    head_at(&mut w, 200.0, 40.0);
    let document = w.doc;
    pc(&mut w, PrintAndCutIntent::MeasureTarget { document, index: 0 });
    head_at(&mut w, 200.0, 160.0);
    pc(&mut w, PrintAndCutIntent::MeasureTarget { document, index: 1 });
    let vm = print_vm(&w);
    assert_eq!(vm.measured, [Some((200.0, 40.0)), Some((200.0, 160.0))]);
    register(&mut w);
    let vm = print_vm(&w);
    let result = vm.result.expect("registered");
    assert!((result.rotation_deg - 90.0).abs() < 1e-9 && (result.scale - 1.0).abs() < 1e-12, "{result:?}");
    assert!(result.rms_mm < 1e-9, "two exact marks leave no residual: {result:?}");
    compile(&mut w);
    let (min, max) = burn_bounds(&w);
    assert!(close(min, (120.0, 40.0)) && close(max, (200.0, 160.0)), "{min:?} {max:?}");
}

#[test]
fn reset_leaves_no_registration_and_the_next_compile_is_the_plain_one() {
    let mut w = win();
    open_with_corner_targets(&mut w);
    set_measured(&mut w, 0, "200", "40");
    set_measured(&mut w, 1, "200", "160");
    register(&mut w);
    compile(&mut w);
    assert!(close(burn_bounds(&w).0, (120.0, 40.0)));
    let document = w.doc;
    pc(&mut w, PrintAndCutIntent::Reset { document });
    let vm = print_vm(&w);
    assert!(vm.result.is_none() && vm.targets == [None, None] && vm.measured == [None, None]);
    assert!(w.h.document(w.doc).unwrap().plan.is_none(), "a plan made with the registration is dropped");
    compile(&mut w);
    let (min, max) = burn_bounds(&w);
    assert!(close(min, (10.0, 10.0)) && close(max, (130.0, 90.0)), "{min:?} {max:?}");
}

#[test]
fn closing_the_panel_forgets_the_registration_too() {
    let mut w = win();
    open_with_corner_targets(&mut w);
    set_measured(&mut w, 0, "200", "40");
    set_measured(&mut w, 1, "200", "160");
    register(&mut w);
    let document = w.doc;
    pc(&mut w, PrintAndCutIntent::Close { document });
    assert!(w.h.document(w.doc).unwrap().schikken.print_and_cut.is_none());
    compile(&mut w);
    assert!(close(burn_bounds(&w).0, (10.0, 10.0)));
}

#[test]
fn no_scaling_is_the_default_and_a_scale_is_only_taken_when_allowed() {
    let mut w = win();
    open_with_corner_targets(&mut w);
    // The print came out 5 % too big: the marks are found 126 apart.
    set_measured(&mut w, 0, "200", "40");
    set_measured(&mut w, 1, "200", "166");
    register(&mut w);
    let result = print_vm(&w).result.expect("registered without scaling");
    assert!((result.scale - 1.0).abs() < 1e-12, "no scaling by default");
    assert!(
        result.residuals_mm.iter().all(|r| (r - 3.0).abs() < 1e-6),
        "the difference shows as residuals: {result:?}"
    );
    let document = w.doc;
    pc(&mut w, PrintAndCutIntent::SetScale { document, scale: PrintCutScale::Uniform, max_deviation: "0,1".into() });
    assert!(print_vm(&w).result.is_none(), "another option drops the registration");
    register(&mut w);
    let result = print_vm(&w).result.expect("registered with scaling");
    assert!((result.scale - 1.05).abs() < 1e-9 && result.rms_mm < 1e-9, "{result:?}");
    pc(&mut w, PrintAndCutIntent::SetScale { document, scale: PrintCutScale::Uniform, max_deviation: "0,01".into() });
    register(&mut w);
    assert_eq!(error(&w), Some(SchikkenMessage::Registration(RegistrationFailure::ScaleOutOfRange)));
    assert!(print_vm(&w).result.is_none());
    pc(&mut w, PrintAndCutIntent::SetScale { document, scale: PrintCutScale::Uniform, max_deviation: "abc".into() });
    assert_eq!(
        error(&w),
        Some(SchikkenMessage::Registration(RegistrationFailure::InvalidField(RegistrationField::MaxDeviation)))
    );
}

#[test]
fn an_unknown_head_position_is_never_taken_as_zero() {
    let mut w = win();
    open_with_corner_targets(&mut w);
    let document = w.doc;
    pc(&mut w, PrintAndCutIntent::MeasureTarget { document, index: 0 });
    assert_eq!(error(&w), Some(SchikkenMessage::Registration(RegistrationFailure::PositionUnknown)));
    assert_eq!(print_vm(&w).measured, [None, None]);
}

#[test]
fn a_changed_target_or_measurement_drops_the_registration_it_was_computed_from() {
    let mut w = win();
    open_with_corner_targets(&mut w);
    set_measured(&mut w, 0, "200", "40");
    set_measured(&mut w, 1, "200", "160");
    register(&mut w);
    assert!(print_vm(&w).result.is_some());
    set_measured(&mut w, 1, "200", "161");
    assert!(print_vm(&w).result.is_none());
    register(&mut w);
    set_target(&mut w, 0, "11", "10");
    assert!(print_vm(&w).result.is_none());
    assert_eq!(print_vm(&w).measured, [None, None], "new design targets need new measurements");
}

#[test]
fn registering_needs_both_targets_measured_and_not_on_top_of_each_other() {
    let mut w = win();
    w.h.invoke(w.doc, CommandKey::PrintAndCut);
    register(&mut w);
    assert_eq!(error(&w), Some(SchikkenMessage::Registration(RegistrationFailure::TargetNotSet)));
    set_target(&mut w, 0, "10", "10");
    set_target(&mut w, 1, "130", "10");
    register(&mut w);
    assert_eq!(error(&w), Some(SchikkenMessage::Registration(RegistrationFailure::NotMeasured)));
    set_measured(&mut w, 0, "200", "40");
    set_measured(&mut w, 1, "200", "40");
    register(&mut w);
    assert_eq!(error(&w), Some(SchikkenMessage::Registration(RegistrationFailure::MeasuredTargetsTooClose)));
    set_target(&mut w, 1, "10", "10");
    set_measured(&mut w, 0, "200", "40");
    set_measured(&mut w, 1, "300", "40");
    register(&mut w);
    assert_eq!(error(&w), Some(SchikkenMessage::Registration(RegistrationFailure::DesignTargetsTooClose)));
    assert!(print_vm(&w).result.is_none());
}

#[test]
fn typed_coordinates_must_be_numbers() {
    let mut w = win();
    w.h.invoke(w.doc, CommandKey::PrintAndCut);
    set_target(&mut w, 0, "abc", "10");
    assert_eq!(error(&w), Some(SchikkenMessage::Registration(RegistrationFailure::InvalidField(RegistrationField::X))));
    set_measured(&mut w, 1, "10", "");
    assert_eq!(error(&w), Some(SchikkenMessage::Registration(RegistrationFailure::InvalidField(RegistrationField::Y))));
    assert_eq!(print_vm(&w).targets, [None, None]);
}

#[test]
fn the_design_targets_can_be_picked_on_the_canvas_snapped_to_the_geometry() {
    let mut w = win();
    w.h.invoke(w.doc, CommandKey::PrintAndCut);
    let document = w.doc;
    let selection_before = w.h.document(w.doc).unwrap().selection;
    pc(&mut w, PrintAndCutIntent::PickTarget { document, index: 1 });
    assert_eq!(print_vm(&w).picking, Some(1));
    // Near the right-hand lower corner of the rectangle: the click snaps to it.
    w.h.click(w.doc, (129.8, 10.2), burnburn_editor::Modifiers::default());
    let vm = print_vm(&w);
    assert_eq!(vm.picking, None);
    let target = vm.targets[1].expect("picked");
    assert!(close(target, (130.0, 10.0)), "{target:?}");
    assert_eq!(w.h.document(w.doc).unwrap().selection, selection_before, "the pick is not a selection");
    // Escape ends a pick without choosing.
    pc(&mut w, PrintAndCutIntent::PickTarget { document, index: 0 });
    w.h.escape(w.doc);
    assert_eq!(print_vm(&w).picking, None);
    assert_eq!(print_vm(&w).targets[0], None);
}

#[test]
fn print_and_cut_works_with_absolute_coordinates_only() {
    let mut w = win();
    open_with_corner_targets(&mut w);
    set_measured(&mut w, 0, "200", "40");
    set_measured(&mut w, 1, "200", "160");
    w.h.send(UiIntent::LaserSetup { setup: LaserSetup::StartFrom(StartFrom::UserOrigin) });
    register(&mut w);
    assert_eq!(error(&w), Some(SchikkenMessage::Registration(RegistrationFailure::NeedsAbsoluteCoordinates)));
    assert!(print_vm(&w).result.is_none());
    w.h.send(UiIntent::LaserSetup { setup: LaserSetup::StartFrom(StartFrom::AbsoluteCoordinates) });
    register(&mut w);
    assert!(print_vm(&w).result.is_some());
}

// -- Center Finder -------------------------------------------------------------------------------

fn add_point(w: &mut Win, x: f64, y: f64) {
    let document = w.doc;
    cf(w, CenterFinderIntent::AddPoint { document, x: format!("{x}"), y: format!("{y}") });
}

fn on_circle(c: (f64, f64), r: f64, degrees: f64) -> (f64, f64) {
    let a = degrees.to_radians();
    (c.0 + r * a.cos(), c.1 + r * a.sin())
}

#[test]
fn three_edge_points_give_the_centre_the_radius_and_the_residuals() {
    let mut w = win();
    w.h.invoke(w.doc, CommandKey::CenterFinder);
    assert_eq!(center_vm(&w).needed, 3);
    for deg in [0.0, 120.0, 240.0] {
        let p = on_circle((50.0, 40.0), 20.0, deg);
        add_point(&mut w, p.0, p.1);
    }
    let vm = center_vm(&w);
    assert_eq!((vm.needed, vm.points.len()), (0, 3));
    let fit = vm.fit.expect("a fit");
    assert!(close(fit.center, (50.0, 40.0)) && (fit.radius_mm - 20.0).abs() < 1e-9, "{fit:?}");
    assert!(fit.max_residual_mm < 1e-9 && !fit.poorly_determined, "{fit:?}");
    assert_eq!(fit.center_machine, Some(fit.center), "bottom-left origin: the same numbers");
}

#[test]
fn fewer_than_three_points_collinear_ones_and_coincident_ones_give_no_circle() {
    let mut w = win();
    w.h.invoke(w.doc, CommandKey::CenterFinder);
    add_point(&mut w, 0.0, 0.0);
    add_point(&mut w, 10.0, 0.0);
    let vm = center_vm(&w);
    assert_eq!((vm.needed, vm.fit.is_none(), vm.problem), (1, true, None));
    add_point(&mut w, 20.0, 0.0);
    let vm = center_vm(&w);
    assert!(vm.fit.is_none());
    assert_eq!(vm.problem, Some(CenterProblem::Collinear));
    let document = w.doc;
    cf(&mut w, CenterFinderIntent::Clear { document });
    for _ in 0..3 {
        add_point(&mut w, 5.0, 5.0);
    }
    assert_eq!(center_vm(&w).problem, Some(CenterProblem::Coincident));
}

#[test]
fn points_on_a_short_arc_are_flagged_as_badly_determined() {
    let mut w = win();
    w.h.invoke(w.doc, CommandKey::CenterFinder);
    for deg in [0.0, 10.0, 20.0] {
        let p = on_circle((100.0, 100.0), 50.0, deg);
        add_point(&mut w, p.0, p.1);
    }
    let fit = center_vm(&w).fit.expect("a fit");
    assert!(fit.poorly_determined, "{fit:?}");
    assert!(fit.arc_span_rad < 0.5);
    assert!(close(fit.center, (100.0, 100.0)));
}

#[test]
fn the_laser_position_is_an_edge_point_unless_it_is_unknown() {
    let mut w = win();
    w.h.invoke(w.doc, CommandKey::CenterFinder);
    let document = w.doc;
    cf(&mut w, CenterFinderIntent::MeasurePoint { document });
    assert_eq!(center_vm(&w).error, Some(SchikkenMessage::Registration(RegistrationFailure::PositionUnknown)));
    assert!(center_vm(&w).points.is_empty(), "nothing was added at 0,0");
    for deg in [0.0, 120.0, 240.0] {
        let p = on_circle((300.0, 200.0), 25.0, deg);
        head_at(&mut w, p.0, p.1);
        cf(&mut w, CenterFinderIntent::MeasurePoint { document });
    }
    let vm = center_vm(&w);
    assert_eq!(vm.points.len(), 3);
    assert!(vm.error.is_none());
    let center = vm.fit.expect("fit").center;
    // The controller reports whole micrometres.
    assert!((center.0 - 300.0).abs() < 1e-2 && (center.1 - 200.0).abs() < 1e-2, "{center:?}");
}

#[test]
fn points_can_be_picked_on_the_canvas_and_the_last_one_undone() {
    let mut w = win();
    w.h.invoke(w.doc, CommandKey::CenterFinder);
    let document = w.doc;
    cf(&mut w, CenterFinderIntent::PickPoints { document });
    assert!(center_vm(&w).picking);
    for p in [(500.0, 500.0), (540.0, 520.0), (520.0, 480.0)] {
        w.h.click(w.doc, p, burnburn_editor::Modifiers::default());
    }
    assert_eq!(center_vm(&w).points.len(), 3);
    assert!(center_vm(&w).picking, "picking continues until it is ended");
    cf(&mut w, CenterFinderIntent::UndoLast { document });
    assert_eq!(center_vm(&w).points.len(), 2);
    w.h.escape(w.doc);
    assert!(!center_vm(&w).picking);
    cf(&mut w, CenterFinderIntent::Close { document });
    assert!(w.h.document(w.doc).unwrap().schikken.center_finder.is_none());
}

#[test]
fn the_panels_open_only_with_a_document() {
    let w = win();
    let vm = w.h.view();
    assert!(vm.commands[&CommandKey::PrintAndCut].enabled && vm.commands[&CommandKey::CenterFinder].enabled);
}
