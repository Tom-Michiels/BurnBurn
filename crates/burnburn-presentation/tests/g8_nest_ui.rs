//! G8-cam-werk, nest: a grain rule per part and told-apart names of the copies. Fixtures are the
//! analytic ones of the G7 grain tests: a 40 x 10 part and a bed that only holds it standing up.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

mod edit_rig;

use std::collections::BTreeSet;

use burnburn_document::ObjectKind;
use burnburn_geometry::{Bounds, Path, Point};
use burnburn_job::{MachineFrame, MachineProfile, OriginCorner, PowerRatio};
use burnburn_presentation::{
    CommandKey, NestInput, NestPanelViewModel, NestPartGrain, NestRotations, SchikkenIntent, UiIntent,
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

fn panel(h: &Harness, doc: DocumentId) -> NestPanelViewModel {
    view(h, doc).schikken.nest.expect("the nest panel is open")
}

fn options(h: &mut Harness, doc: DocumentId, input: NestInput) {
    h.send(UiIntent::Schikken(SchikkenIntent::SetNestOptions { document: doc, input: Box::new(input) }));
}

fn nest(h: &mut Harness, doc: DocumentId) -> NestPanelViewModel {
    h.send(UiIntent::Schikken(SchikkenIntent::StartNest { document: doc }));
    h.run_tasks();
    panel(h, doc)
}

fn bounds_of(h: &Harness, doc: DocumentId, id: GeometryId) -> Bounds {
    let scene = h.scene(doc).expect("scene");
    scene.items.iter().find(|i| i.source == id).and_then(|i| i.path.bounds()).expect("object in scene")
}

fn long_part() -> Path {
    Path::polyline(
        &[Point::new(300.0, 300.0), Point::new(340.0, 300.0), Point::new(340.0, 310.0), Point::new(300.0, 310.0)],
        true,
    )
    .unwrap()
}

/// A part of 40 x 10 and a bed of 12 x 45: it only fits standing up.
fn rig() -> (Harness, DocumentId, GeometryId) {
    let (mut h, doc, ids) = opened_with_kinds(vec![(long_part(), ObjectKind::Path)]);
    bed(&mut h, 12.0, 45.0);
    h.invoke(doc, CommandKey::QuickNest);
    (h, doc, ids[0])
}

fn along_x(rule: Option<NestPartGrain>, id: GeometryId) -> NestInput {
    NestInput {
        grain: "0".to_owned(),
        rotations: NestRotations::Quarter,
        padding: "0".into(),
        edge_buffer: "0".into(),
        part_grain: rule.map(|r| vec![(id.raw(), r)]).unwrap_or_default(),
        ..NestInput::default()
    }
}

#[test]
fn a_part_follows_the_grain_of_the_sheet_unless_it_says_otherwise() {
    let (mut h, doc, id) = rig();
    options(&mut h, doc, along_x(None, id));
    assert_eq!(panel(&h, doc).part_rows[0].grain, NestPartGrain::Along);
    let report = nest(&mut h, doc).report.expect("report");
    assert_eq!((report.placed, report.unplaced.len()), (0, 1), "its X axis along X cannot stand up");
}

#[test]
fn a_part_drawn_across_the_grain_stands_up_and_a_free_part_turns_as_it_likes() {
    for rule in [NestPartGrain::Across, NestPartGrain::Free] {
        let (mut h, doc, id) = rig();
        options(&mut h, doc, along_x(Some(rule), id));
        assert_eq!(panel(&h, doc).part_rows[0].grain, rule, "the row shows the rule");
        assert_eq!(nest(&mut h, doc).report.expect("report").placed, 1, "{rule:?}");
        h.send(UiIntent::Schikken(SchikkenIntent::ApplyNest { document: doc }));
        let b = bounds_of(&h, doc, id);
        assert!((b.width() - 10.0).abs() < 1e-6 && (b.height() - 40.0).abs() < 1e-6, "{rule:?}: {b:?}");
    }
}

#[test]
fn a_rule_of_one_part_does_not_change_another_part() {
    // Two long parts: the one that is free stands up on the narrow bed, the other keeps its grain.
    let second = Path::polyline(
        &[Point::new(300.0, 340.0), Point::new(340.0, 340.0), Point::new(340.0, 350.0), Point::new(300.0, 350.0)],
        true,
    )
    .unwrap();
    let (mut h, doc, ids) = opened_with_kinds(vec![(long_part(), ObjectKind::Path), (second, ObjectKind::Path)]);
    bed(&mut h, 12.0, 45.0);
    h.invoke(doc, CommandKey::QuickNest);
    let input = NestInput { part_grain: vec![(ids[1].raw(), NestPartGrain::Free)], ..along_x(None, ids[0]) };
    options(&mut h, doc, input);
    let report = nest(&mut h, doc).report.expect("report");
    assert_eq!((report.placed, report.unplaced.len()), (1, 1));
    let placed: Vec<u32> = report.parts.iter().map(|r| r.placed).collect();
    assert_eq!(placed, vec![0, 1], "the second (free) part is the one that is placed");
}

/// Two squares on a 100 x 100 bed, three copies of each.
fn squares(copies: &str) -> (Harness, DocumentId) {
    let (mut h, doc, _) = opened_with_kinds(vec![
        (square(300.0, 300.0, 20.0), ObjectKind::Path),
        (square(340.0, 300.0, 20.0), ObjectKind::Path),
    ]);
    bed(&mut h, 100.0, 100.0);
    h.invoke(doc, CommandKey::QuickNest);
    options(&mut h, doc, NestInput { copies: copies.to_owned(), ..NestInput::default() });
    nest(&mut h, doc);
    h.send(UiIntent::Schikken(SchikkenIntent::ApplyNest { document: doc }));
    (h, doc)
}

#[test]
fn the_copies_of_a_part_get_names_that_tell_them_apart() {
    let (h, doc) = squares("3");
    let names: Vec<String> = view(&h, doc).objects.iter().map(|o| o.label.clone()).collect();
    assert_eq!(names.len(), 6);
    let unique: BTreeSet<&String> = names.iter().collect();
    assert_eq!(unique.len(), 6, "no two objects share a name: {names:?}");
}

#[test]
fn a_copy_keeps_the_name_of_its_source_as_a_prefix_and_the_original_is_not_renamed() {
    let (h, doc) = squares("2");
    let names: Vec<String> = view(&h, doc).objects.iter().map(|o| o.label.clone()).collect();
    let originals: Vec<&String> = names.iter().filter(|n| !n.contains('(')).collect();
    assert_eq!(originals.len(), 2, "the two originals keep their names: {names:?}");
    for copy in names.iter().filter(|n| n.contains('(')) {
        assert!(originals.iter().any(|o| copy.starts_with(o.as_str())), "{copy} is named after its source: {names:?}");
    }
}
