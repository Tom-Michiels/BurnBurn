//! C12 (golf 6): production nesting in the nest panel. Mirroring is a choice (off by default), the
//! report says per sheet how much is used and left, which orders are complete and how many complete
//! sets were placed. Every number has an analytic oracle (areas of squares, counts of parts).

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

mod edit_rig;

use burnburn_document::ObjectKind;
use burnburn_geometry::{Bounds, Path};
use burnburn_job::{MachineFrame, MachineProfile, OriginCorner, PowerRatio};
use burnburn_presentation::{
    CommandKey, NestFailure, NestField, NestInput, NestMirror, NestPanelViewModel, NestRotations, NestRun,
    SchikkenIntent, SchikkenMessage, UiIntent,
};
use burnburn_ruida::{Magic, OverscanPolicy, RuidaProfile};
use burnburn_types::{DocumentId, GeometryId};
use edit_rig::{opened_with_kinds, square, view};

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

fn options(h: &mut Harness, doc: DocumentId, input: NestInput) {
    send(h, SchikkenIntent::SetNestOptions { document: doc, input: Box::new(input) });
}

fn nest(h: &mut Harness, doc: DocumentId) -> NestPanelViewModel {
    send(h, SchikkenIntent::StartNest { document: doc });
    h.run_tasks();
    panel(h, doc)
}

fn bounds_of(h: &Harness, doc: DocumentId, id: GeometryId) -> Bounds {
    let scene = h.scene(doc).expect("scene");
    scene.items.iter().find(|i| i.source == id).and_then(|i| i.path.bounds()).expect("object in scene")
}

fn three_squares_on_a_bed() -> (Harness, DocumentId, Vec<GeometryId>) {
    let (mut h, doc, ids) =
        doc_with(vec![square(300.0, 300.0, 30.0), square(340.0, 300.0, 30.0), square(380.0, 300.0, 30.0)]);
    bed(&mut h, 100.0, 100.0);
    h.invoke(doc, CommandKey::QuickNest);
    (h, doc, ids)
}

#[test]
fn mirroring_is_off_by_default_and_a_nest_without_it_reports_no_mirrored_part() {
    let (mut h, doc, _) = three_squares_on_a_bed();
    assert_eq!(panel(&h, doc).input.mirror, NestMirror::Forbidden);
    let report = nest(&mut h, doc).report.expect("report");
    assert_eq!((report.placed, report.mirrored), (3, 0));
}

#[test]
fn choosing_to_allow_mirroring_drops_the_old_result_and_is_kept() {
    let (mut h, doc, _) = three_squares_on_a_bed();
    assert_eq!(nest(&mut h, doc).run, NestRun::Done);
    options(&mut h, doc, NestInput { mirror: NestMirror::Allowed, ..NestInput::default() });
    let p = panel(&h, doc);
    assert_eq!((p.input.mirror, p.run), (NestMirror::Allowed, NestRun::Idle), "a result belongs to its options");
    assert!(p.report.is_none());
}

#[test]
fn the_report_says_per_sheet_how_much_is_used_and_how_much_is_left() {
    let (mut h, doc, _) = three_squares_on_a_bed();
    let report = nest(&mut h, doc).report.expect("report");
    assert_eq!(report.sheets.len(), 1);
    let s = report.sheets[0];
    // Three 30 x 30 squares: 2700 mm² placed. The sheet is the 100 x 100 bed, less at most its margin.
    assert!((s.placed_mm2 - 2700.0).abs() < 1.0, "{s:?}");
    assert!(s.net_mm2 > 9500.0 && s.net_mm2 <= 10_000.0 + 1e-6, "{s:?}");
    assert!((s.free_mm2 - (s.net_mm2 - s.placed_mm2)).abs() < 1e-6, "{s:?}");
}

#[test]
fn a_second_sheet_takes_what_the_first_cannot_hold_and_lies_beside_the_first() {
    // 60 x 60 parts: one per 100 x 100 sheet.
    let (mut h, doc, ids) =
        doc_with(vec![square(300.0, 300.0, 60.0), square(400.0, 300.0, 60.0), square(500.0, 300.0, 60.0)]);
    bed(&mut h, 100.0, 100.0);
    h.invoke(doc, CommandKey::QuickNest);
    options(&mut h, doc, NestInput { sheets: "2".to_owned(), ..NestInput::default() });
    let report = nest(&mut h, doc).report.expect("report");
    assert_eq!((report.placed, report.unplaced.len(), report.sheets.len()), (2, 1, 2));
    send(&mut h, SchikkenIntent::ApplyNest { document: doc });
    let mut starts: Vec<f64> = ids.iter().map(|id| bounds_of(&h, doc, *id).min().x).filter(|x| *x < 250.0).collect();
    starts.sort_by(f64::total_cmp);
    assert_eq!(starts.len(), 2, "two parts moved onto sheets: {starts:?}");
    assert!(starts[0] >= 1.0 && starts[0] + 60.0 <= 99.0 + 1e-9, "first sheet x 0..100: {starts:?}");
    assert!(starts[1] >= 100.0, "second sheet starts beyond the first: {starts:?}");
}

#[test]
fn the_sheet_count_must_be_a_whole_number_of_at_least_one() {
    let (mut h, doc, _) = three_squares_on_a_bed();
    for bad in ["0", "-1", "1.5", "", "abc", "21"] {
        options(&mut h, doc, NestInput { sheets: bad.to_owned(), ..NestInput::default() });
        assert_eq!(
            panel(&h, doc).error,
            Some(SchikkenMessage::Nest(NestFailure::InvalidField(NestField::Sheets))),
            "{bad:?}"
        );
    }
}

#[test]
fn order_status_counts_ordered_and_placed_per_part_and_complete_sets_follow() {
    // Two 60 x 60 parts on one 100 x 100 sheet: only one fits.
    let (mut h, doc, _) = doc_with(vec![square(300.0, 300.0, 60.0), square(400.0, 300.0, 60.0)]);
    bed(&mut h, 100.0, 100.0);
    h.invoke(doc, CommandKey::QuickNest);
    let report = nest(&mut h, doc).report.expect("report");
    let mut counts: Vec<(u32, u32)> = report.parts.iter().map(|r| (r.ordered, r.placed)).collect();
    counts.sort_unstable();
    assert_eq!(counts, vec![(1, 0), (1, 1)]);
    assert_eq!(report.complete_sets, 0, "one part is missing: no complete set");
    let missing = report.parts.iter().find(|r| r.placed == 0).map(|r| r.objects.clone());
    assert_eq!(missing, report.unplaced.first().map(|u| u.objects.clone()), "the unplaced row is the missing part");

    // On two sheets both are placed: one complete set.
    options(&mut h, doc, NestInput { sheets: "2".to_owned(), ..NestInput::default() });
    let report = nest(&mut h, doc).report.expect("report");
    assert!(report.parts.iter().all(|r| r.ordered == 1 && r.placed == 1));
    assert_eq!(report.complete_sets, 1);
}

#[test]
fn rotations_and_mirror_are_independent_options() {
    let (mut h, doc, _) = three_squares_on_a_bed();
    options(
        &mut h,
        doc,
        NestInput { rotations: NestRotations::Fixed, mirror: NestMirror::Allowed, ..NestInput::default() },
    );
    let p = panel(&h, doc);
    assert_eq!((p.input.rotations, p.input.mirror), (NestRotations::Fixed, NestMirror::Allowed));
}
