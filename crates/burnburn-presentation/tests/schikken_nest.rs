//! C10 (W3-UI-schikken): Quick Nest and Nest Selected as a panel with a cancellable task. A
//! result belongs to one revision and one set of options; the report says what was not placed.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

mod edit_rig;

use burnburn_document::{EditLabel, ObjectKind, UndoLabel};
use burnburn_geometry::{Bounds, Path, Point};
use burnburn_job::{MachineFrame, MachineProfile, OriginCorner, PowerRatio};
use burnburn_presentation::{
    CommandKey, NestFailure, NestField, NestInput, NestPanelViewModel, NestRotations, NestRun, NestSkipReason,
    NestUnplacedReason, SchikkenIntent, SchikkenMessage, TaskKind, UiIntent,
};
use burnburn_ruida::{Magic, OverscanPolicy, RuidaProfile};
use burnburn_types::{DocumentId, GeometryId};
use edit_rig::{enabled, opened_with_kinds, select, selected, square, view};

type Harness = burnburn_presentation::HeadlessEditorHarness;

fn bed(h: &mut Harness, w: f64, hgt: f64) {
    let profile = MachineProfile::new(
        "nest-bed",
        1,
        MachineFrame::new(w, hgt, OriginCorner::BottomLeft).unwrap(),
        1000.0,
        400.0,
        PowerRatio::new(0.0).unwrap(),
        PowerRatio::new(1.0).unwrap(),
    )
    .unwrap();
    let ruida = RuidaProfile { magic: Magic::FIXTURE_0X88, overscan: OverscanPolicy::ControllerManaged };
    h.send(UiIntent::SetMachineProfile { profile: Box::new(profile), ruida });
}

fn doc_with(paths: Vec<Path>) -> (Harness, DocumentId, Vec<GeometryId>) {
    opened_with_kinds(paths.into_iter().map(|p| (p, ObjectKind::Path)).collect())
}

fn panel(h: &Harness, doc: DocumentId) -> NestPanelViewModel {
    view(h, doc).schikken.nest.expect("the nest panel is open")
}

fn send(h: &mut Harness, intent: SchikkenIntent) {
    h.send(UiIntent::Schikken(intent));
}

/// Opens the panel with `command`, starts the nest and runs the task.
fn nest(h: &mut Harness, doc: DocumentId, command: CommandKey) -> NestPanelViewModel {
    h.invoke(doc, command);
    send(h, SchikkenIntent::StartNest { document: doc });
    h.run_tasks();
    panel(h, doc)
}

fn bounds_of(h: &Harness, doc: DocumentId, id: GeometryId) -> Bounds {
    let scene = h.scene(doc).expect("scene");
    scene.items.iter().find(|i| i.source == id).and_then(|i| i.path.bounds()).expect("object in scene")
}

fn gap(a: Bounds, b: Bounds) -> f64 {
    let dx = (b.min().x - a.max().x).max(a.min().x - b.max().x);
    let dy = (b.min().y - a.max().y).max(a.min().y - b.max().y);
    dx.max(dy)
}

fn rect_path(x: f64, y: f64, w: f64, h: f64) -> Path {
    Path::polyline(&[Point::new(x, y), Point::new(x + w, y), Point::new(x + w, y + h), Point::new(x, y + h)], true)
        .unwrap()
}

#[test]
fn quick_nest_places_every_part_inside_the_bed_with_the_padding_in_one_undo_entry() {
    let (mut h, doc, ids) =
        doc_with(vec![square(300.0, 300.0, 30.0), square(340.0, 300.0, 30.0), square(380.0, 300.0, 30.0)]);
    bed(&mut h, 100.0, 100.0);
    assert!(enabled(&h, CommandKey::QuickNest));
    let p = nest(&mut h, doc, CommandKey::QuickNest);
    assert_eq!(p.run, NestRun::Done);
    assert!(p.can_apply);
    let report = p.report.expect("report");
    assert_eq!((report.placed, report.unplaced.len(), report.skipped.len()), (3, 0, 0));
    assert!(report.utilisation > 0.2 && report.utilisation < 1.0, "{}", report.utilisation);
    assert!(p.sheet.expect("sheet").is_bed);
    // Nothing reaches the document before Apply.
    assert_eq!(view(&h, doc).undo, None);
    send(&mut h, SchikkenIntent::ApplyNest { document: doc });
    let vm = view(&h, doc);
    assert_eq!(vm.undo, Some(UndoLabel::Edit(EditLabel::Nest)));
    let boxes: Vec<Bounds> = ids.iter().map(|id| bounds_of(&h, doc, *id)).collect();
    for b in &boxes {
        assert!(b.min().x >= 1.0 - 1e-9 && b.min().y >= 1.0 - 1e-9, "edge buffer: {b:?}");
        assert!(b.max().x <= 99.0 + 1e-9 && b.max().y <= 99.0 + 1e-9, "inside the bed: {b:?}");
        assert!((b.width() - 30.0).abs() < 1e-6 && (b.height() - 30.0).abs() < 1e-6, "size kept: {b:?}");
    }
    for i in 0..3 {
        for j in i + 1..3 {
            assert!(
                gap(boxes[i], boxes[j]) >= 1.0 - 1e-6,
                "padding between {i} and {j}: {:?} {:?}",
                boxes[i],
                boxes[j]
            );
        }
    }
    assert_eq!(panel(&h, doc).run, NestRun::Applied);
    h.invoke(doc, CommandKey::Undo);
    assert!((bounds_of(&h, doc, ids[0]).min().x - 300.0).abs() < 1e-9, "one undo restores every part");
    assert!((bounds_of(&h, doc, ids[2]).min().x - 380.0).abs() < 1e-9);
}

#[test]
fn nest_selected_moves_only_the_selection_and_stays_clear_of_the_rest() {
    let (mut h, doc, ids) = doc_with(vec![
        square(200.0, 0.0, 30.0),
        square(240.0, 0.0, 30.0),
        square(0.0, 0.0, 60.0), // stays: occupies the lower left of the bed
    ]);
    bed(&mut h, 100.0, 100.0);
    select(&mut h, doc, &ids[..2]);
    assert!(enabled(&h, CommandKey::NestSelected));
    let p = nest(&mut h, doc, CommandKey::NestSelected);
    assert_eq!(p.report.expect("report").placed, 2);
    send(&mut h, SchikkenIntent::ApplyNest { document: doc });
    let stay = bounds_of(&h, doc, ids[2]);
    assert_eq!((stay.min(), stay.max()), (Point::new(0.0, 0.0), Point::new(60.0, 60.0)), "the others stay");
    for id in &ids[..2] {
        let b = bounds_of(&h, doc, *id);
        assert!(gap(b, stay) >= 1.0 - 1e-6, "clear of the part that stays: {b:?}");
        assert!(b.max().x <= 99.0 + 1e-9 && b.max().y <= 99.0 + 1e-9);
    }
    assert_eq!(selected(&h, doc), ids[..2].to_vec(), "the selection stays");
}

#[test]
fn what_does_not_fit_is_reported_and_left_where_it_is() {
    let (mut h, doc, ids) = doc_with(vec![square(300.0, 0.0, 150.0), square(500.0, 0.0, 30.0)]);
    bed(&mut h, 100.0, 100.0);
    let p = nest(&mut h, doc, CommandKey::QuickNest);
    let report = p.report.expect("report");
    assert_eq!(report.placed, 1);
    assert_eq!(report.unplaced.len(), 1);
    assert_eq!(report.unplaced[0].objects, vec![ids[0]]);
    assert_eq!(report.unplaced[0].reason, NestUnplacedReason::NoFit);
    send(&mut h, SchikkenIntent::ApplyNest { document: doc });
    assert!((bounds_of(&h, doc, ids[0]).min().x - 300.0).abs() < 1e-9, "the part that did not fit stays");
    assert!(bounds_of(&h, doc, ids[1]).max().x <= 100.0);
    send(&mut h, SchikkenIntent::SelectNestUnplaced { document: doc });
    assert_eq!(selected(&h, doc), vec![ids[0]]);
}

#[test]
fn rotations_decide_whether_a_long_part_fits() {
    // 110 × 10 does not fit a 100 mm bed flat or on its side; turned 45° its box is about 85 mm.
    let (mut h, doc, ids) = doc_with(vec![rect_path(300.0, 0.0, 110.0, 10.0)]);
    bed(&mut h, 100.0, 100.0);
    h.invoke(doc, CommandKey::QuickNest);
    send(
        &mut h,
        SchikkenIntent::SetNestOptions {
            document: doc,
            input: NestInput { rotations: NestRotations::Quarter, ..NestInput::default() },
        },
    );
    send(&mut h, SchikkenIntent::StartNest { document: doc });
    h.run_tasks();
    assert_eq!(panel(&h, doc).report.expect("report").unplaced.len(), 1, "quarter turns are not enough");
    send(
        &mut h,
        SchikkenIntent::SetNestOptions {
            document: doc,
            input: NestInput { rotations: NestRotations::Eighth, ..NestInput::default() },
        },
    );
    assert!(panel(&h, doc).report.is_none(), "other options drop the old result");
    send(&mut h, SchikkenIntent::StartNest { document: doc });
    h.run_tasks();
    let p = panel(&h, doc);
    assert_eq!(p.report.expect("report").placed, 1);
    send(&mut h, SchikkenIntent::ApplyNest { document: doc });
    let b = bounds_of(&h, doc, ids[0]);
    assert!(b.width() < 100.0 && b.height() < 100.0 && b.width() > 80.0, "turned 45°: {b:?}");
}

#[test]
fn open_paths_are_reported_as_skipped_and_groups_move_as_one_body() {
    let open = Path::line(Point::new(500.0, 0.0), Point::new(520.0, 0.0)).unwrap();
    let (mut h, doc, ids) = doc_with(vec![square(300.0, 0.0, 20.0), square(330.0, 0.0, 20.0), open]);
    bed(&mut h, 100.0, 100.0);
    select(&mut h, doc, &ids[..2]);
    h.invoke(doc, CommandKey::Group);
    let p = nest(&mut h, doc, CommandKey::QuickNest);
    let report = p.report.expect("report");
    assert_eq!(report.placed, 1, "the group is one part");
    assert_eq!(report.skipped.len(), 1);
    assert_eq!((report.skipped[0].object, report.skipped[0].reason), (ids[2], NestSkipReason::OpenContour));
    send(&mut h, SchikkenIntent::ApplyNest { document: doc });
    let (a, b) = (bounds_of(&h, doc, ids[0]), bounds_of(&h, doc, ids[1]));
    let together = a.union(b);
    assert!(
        (together.width() - 50.0).abs() < 1e-6 || (together.height() - 50.0).abs() < 1e-6,
        "relative places kept: {together:?}"
    );
    assert!((bounds_of(&h, doc, ids[2]).min().x - 500.0).abs() < 1e-9);
}

#[test]
fn invalid_fields_are_named_and_no_task_starts() {
    let (mut h, doc, _) = doc_with(vec![square(300.0, 0.0, 20.0)]);
    bed(&mut h, 100.0, 100.0);
    h.invoke(doc, CommandKey::QuickNest);
    for (input, field) in [
        (NestInput { padding: "abc".to_owned(), ..NestInput::default() }, NestField::Padding),
        (NestInput { edge_buffer: "-2".to_owned(), ..NestInput::default() }, NestField::EdgeBuffer),
        (NestInput { step: "0".to_owned(), ..NestInput::default() }, NestField::Step),
    ] {
        send(&mut h, SchikkenIntent::SetNestOptions { document: doc, input });
        assert_eq!(panel(&h, doc).error, Some(SchikkenMessage::Nest(NestFailure::InvalidField(field))));
    }
    send(&mut h, SchikkenIntent::StartNest { document: doc });
    assert!(h.take_task_requests().is_empty(), "no task for invalid options");
}

#[test]
fn cancelling_stops_the_task_and_no_result_is_delivered() {
    let (mut h, doc, _) = doc_with(vec![square(300.0, 0.0, 20.0), square(340.0, 0.0, 20.0)]);
    bed(&mut h, 100.0, 100.0);
    h.invoke(doc, CommandKey::QuickNest);
    send(&mut h, SchikkenIntent::StartNest { document: doc });
    assert_eq!(panel(&h, doc).run, NestRun::Running);
    assert!(view(&h, doc).tasks.iter().any(|t| t.kind == TaskKind::Nest));
    let requests = h.take_task_requests();
    assert_eq!(requests.len(), 1);
    send(&mut h, SchikkenIntent::CancelNest { document: doc });
    for r in requests {
        h.complete(r);
    }
    let p = panel(&h, doc);
    assert_eq!(p.run, NestRun::Idle);
    assert!(p.report.is_none() && !p.can_apply);
}

#[test]
fn an_edit_while_nesting_drops_the_result_and_an_edit_after_it_makes_it_stale() {
    let (mut h, doc, ids) = doc_with(vec![square(300.0, 0.0, 20.0), square(340.0, 0.0, 20.0)]);
    bed(&mut h, 100.0, 100.0);
    h.invoke(doc, CommandKey::QuickNest);
    send(&mut h, SchikkenIntent::StartNest { document: doc });
    let requests = h.take_task_requests();
    select(&mut h, doc, &ids[..1]);
    h.invoke(doc, CommandKey::FlipHorizontal); // an edit while the worker runs
    for r in requests {
        h.complete_despite_cancel(r);
    }
    let p = panel(&h, doc);
    assert!(p.report.is_none(), "a result of an older revision is never taken");
    assert_ne!(p.run, NestRun::Running);
    // A result that arrived in time goes stale with the next edit.
    send(&mut h, SchikkenIntent::StartNest { document: doc });
    h.run_tasks();
    assert_eq!(panel(&h, doc).run, NestRun::Done);
    h.invoke(doc, CommandKey::FlipVertical);
    let p = panel(&h, doc);
    assert_eq!(p.run, NestRun::Stale);
    assert!(!p.can_apply);
    let revision = view(&h, doc).revision;
    send(&mut h, SchikkenIntent::ApplyNest { document: doc });
    assert_eq!(view(&h, doc).revision, revision, "a stale result is not applied");
}

#[test]
fn without_a_machine_the_sheet_is_the_bounds_of_the_parts_and_says_so() {
    let (mut h, doc, ids) = doc_with(vec![square(0.0, 0.0, 20.0), square(100.0, 0.0, 20.0)]);
    let p = nest(&mut h, doc, CommandKey::QuickNest);
    let sheet = p.sheet.expect("sheet");
    assert!(!sheet.is_bed);
    // The area of the parts, grown by edge buffer + padding + one step (1 + 1 + 1 mm per side).
    assert!((sheet.width_mm - 126.0).abs() < 1e-9 && (sheet.height_mm - 26.0).abs() < 1e-9, "{sheet:?}");
    assert!(p.report.is_some());
    let _ = ids;
}

#[test]
fn nothing_to_nest_keeps_the_commands_off() {
    let (mut h, doc, ids) = doc_with(vec![square(0.0, 0.0, 20.0)]);
    select(&mut h, doc, &[]);
    assert!(!enabled(&h, CommandKey::NestSelected));
    assert!(enabled(&h, CommandKey::QuickNest));
    // Only an open path: the panel opens, and Start says there is nothing to nest.
    let open = Path::line(Point::new(0.0, 0.0), Point::new(10.0, 0.0)).unwrap();
    let (mut h2, doc2, _) = doc_with(vec![open]);
    h2.invoke(doc2, CommandKey::QuickNest);
    send(&mut h2, SchikkenIntent::StartNest { document: doc2 });
    assert_eq!(panel(&h2, doc2).error, Some(SchikkenMessage::Nest(NestFailure::NothingToNest)));
    let _ = (h.view(), ids);
    let _ = &mut h;
}

#[test]
fn closing_the_panel_cancels_a_running_nest() {
    let (mut h, doc, _) = doc_with(vec![square(300.0, 0.0, 20.0)]);
    bed(&mut h, 100.0, 100.0);
    h.invoke(doc, CommandKey::QuickNest);
    send(&mut h, SchikkenIntent::StartNest { document: doc });
    let requests = h.take_task_requests();
    send(&mut h, SchikkenIntent::CloseNest { document: doc });
    assert!(view(&h, doc).schikken.nest.is_none());
    for r in requests {
        h.complete_despite_cancel(r);
    }
    assert!(view(&h, doc).schikken.nest.is_none(), "a late answer does not reopen the panel");
}
