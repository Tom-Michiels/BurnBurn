//! C12 (G7-cam): the number of copies per part in the nest panel. Every part gets the number of
//! the panel or its own; the originals move and the other copies become new objects on the same
//! layers, all in one undo entry. A copy that does not fit is reported, never made.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

mod edit_rig;

use burnburn_document::process::{
    Kerf, OperationDraft, OperationMode, Passes, PowerPercent, PowerRange, ProcessSettings, Speed,
};
use burnburn_document::{ArrangeLabel, ObjectKind, UndoLabel};
use burnburn_geometry::{Bounds, Path};
use burnburn_job::{MachineFrame, MachineProfile, OriginCorner, PowerRatio};
use burnburn_presentation::{
    CommandKey, NestFailure, NestField, NestInput, NestPanelViewModel, SchikkenIntent, SchikkenMessage, UiIntent,
};
use burnburn_ruida::{Magic, OverscanPolicy, RuidaProfile};
use burnburn_types::{DocumentId, GeometryId, StableId};
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

/// Two 20 x 20 parts far from a 100 x 100 bed.
fn two_squares() -> (Harness, DocumentId, Vec<GeometryId>) {
    let (mut h, doc, ids) = doc_with(vec![square(300.0, 300.0, 20.0), square(340.0, 300.0, 20.0)]);
    bed(&mut h, 100.0, 100.0);
    h.invoke(doc, CommandKey::QuickNest);
    (h, doc, ids)
}

fn copies(n: &str) -> NestInput {
    NestInput { copies: n.to_owned(), ..NestInput::default() }
}

#[test]
fn every_part_is_one_copy_unless_the_panel_says_more() {
    let (mut h, doc, ids) = two_squares();
    let p = panel(&h, doc);
    assert_eq!(p.input.copies, "1");
    assert_eq!(p.part_rows.len(), 2);
    assert_eq!(
        p.part_rows.iter().map(|r| r.object).collect::<Vec<_>>(),
        ids.iter().map(|i| i.raw()).collect::<Vec<_>>()
    );
    assert!(p.part_rows.iter().all(|r| r.copies == "1" && !r.own));

    options(&mut h, doc, copies("3"));
    let p = panel(&h, doc);
    assert!(p.part_rows.iter().all(|r| r.copies == "3" && !r.own), "the number of the panel is what every part shows");
    let report = nest(&mut h, doc).report.expect("report");
    assert_eq!(report.placed, 6, "three copies of two parts");
    assert!(report.parts.iter().all(|r| (r.ordered, r.placed) == (3, 3)));
    assert_eq!(report.complete_sets, 3);
}

#[test]
fn a_part_can_have_its_own_number() {
    let (mut h, doc, ids) = two_squares();
    let input =
        NestInput { copies: "2".to_owned(), part_copies: vec![(ids[1].raw(), "4".to_owned())], ..NestInput::default() };
    options(&mut h, doc, input);
    let p = panel(&h, doc);
    let shown: Vec<(&str, bool)> = p.part_rows.iter().map(|r| (r.copies.as_str(), r.own)).collect();
    assert_eq!(shown, vec![("2", false), ("4", true)]);
    let report = nest(&mut h, doc).report.expect("report");
    let mut counts: Vec<(u32, u32)> = report.parts.iter().map(|r| (r.ordered, r.placed)).collect();
    counts.sort_unstable();
    assert_eq!(counts, vec![(2, 2), (4, 4)]);
    assert_eq!(report.complete_sets, 2, "a set is one of each: the part with two copies limits it");
}

#[test]
fn a_number_that_is_not_a_whole_count_in_range_is_refused_by_name() {
    let (mut h, doc, ids) = two_squares();
    let refused = Some(SchikkenMessage::Nest(NestFailure::InvalidField(NestField::Copies)));
    for bad in ["0", "-1", "1.5", "", "abc", "1001"] {
        options(&mut h, doc, copies(bad));
        assert_eq!(panel(&h, doc).error, refused, "{bad:?}");
        let own = NestInput { part_copies: vec![(ids[0].raw(), bad.to_owned())], ..NestInput::default() };
        options(&mut h, doc, own);
        assert_eq!(panel(&h, doc).error, refused, "own number {bad:?}");
    }
    options(&mut h, doc, copies("1000"));
    assert_eq!(panel(&h, doc).error, None, "1000 is the largest");
}

#[test]
fn applying_moves_the_originals_and_makes_the_other_copies_in_one_undo_entry() {
    let (mut h, doc, ids) = two_squares();
    options(&mut h, doc, copies("3"));
    nest(&mut h, doc);
    let before = view(&h, doc).objects.len();
    send(&mut h, SchikkenIntent::ApplyNest { document: doc });
    let after = view(&h, doc);
    assert_eq!(before, 2);
    assert_eq!(after.objects.len(), 6, "the 4 extra copies are new objects");
    assert_eq!(after.undo, Some(UndoLabel::Arrange(ArrangeLabel::NestCopies)));
    // All six lie on the bed (0..100) and the originals are among them.
    for o in &after.objects {
        let b = bounds_of(&h, doc, o.id);
        assert!(b.min().x >= 0.0 && b.min().y >= 0.0 && b.max().x <= 100.0 && b.max().y <= 100.0, "{b:?}");
        assert!((b.width() - 20.0).abs() < 1e-6 && (b.height() - 20.0).abs() < 1e-6, "size is kept: {b:?}");
    }
    assert!(ids.iter().all(|id| bounds_of(&h, doc, *id).max().x <= 100.0), "the originals moved onto the bed");
    // No two overlap (squares: boxes are the parts).
    let boxes: Vec<Bounds> = after.objects.iter().map(|o| bounds_of(&h, doc, o.id)).collect();
    for (i, a) in boxes.iter().enumerate() {
        for b in &boxes[i + 1..] {
            let apart = a.max().x <= b.min().x + 1e-9
                || b.max().x <= a.min().x + 1e-9
                || a.max().y <= b.min().y + 1e-9
                || b.max().y <= a.min().y + 1e-9;
            assert!(apart, "{a:?} overlaps {b:?}");
        }
    }
    h.invoke(doc, CommandKey::Undo);
    let undone = view(&h, doc);
    assert_eq!(undone.objects.len(), 2, "one undo takes all the copies away");
    assert!(bounds_of(&h, doc, ids[0]).min().x >= 250.0, "and puts the originals back");
}

#[test]
fn the_copies_are_cut_on_the_layer_of_their_source() {
    let (mut h, doc, ids) = two_squares();
    let p = PowerPercent::new(20.0).unwrap();
    let draft = OperationDraft {
        name: "Snijden".into(),
        mode: OperationMode::Line { kerf: Kerf::NONE },
        settings: ProcessSettings {
            speed: Speed::mm_per_s(100.0).unwrap(),
            power: PowerRange::new(p, p).unwrap(),
            passes: Passes::new(1).unwrap(),
            air_assist: true,
        },
        output_enabled: true,
    };
    edit_rig::select(&mut h, doc, &ids);
    h.send(UiIntent::AddOperation { document: doc, draft });
    options(&mut h, doc, copies("2"));
    nest(&mut h, doc);
    send(&mut h, SchikkenIntent::ApplyNest { document: doc });
    let v = view(&h, doc);
    assert_eq!(v.operations[0].targets, 4, "the layer cuts the two originals and the two copies");
}

#[test]
fn a_copy_that_does_not_fit_is_reported_and_not_made() {
    // 40 x 40 parts, 5 copies of one on a 100 x 100 bed: four fit.
    let (mut h, doc, _) = doc_with(vec![square(300.0, 300.0, 40.0)]);
    bed(&mut h, 100.0, 100.0);
    h.invoke(doc, CommandKey::QuickNest);
    options(&mut h, doc, copies("5"));
    let report = nest(&mut h, doc).report.expect("report");
    assert_eq!(report.placed, 4);
    assert_eq!(report.unplaced.len(), 1);
    assert_eq!(report.unplaced[0].count, 1, "one copy is missing");
    assert_eq!((report.parts[0].ordered, report.parts[0].placed), (5, 4));
    send(&mut h, SchikkenIntent::ApplyNest { document: doc });
    assert_eq!(view(&h, doc).objects.len(), 4, "only the placed copies exist");
}
