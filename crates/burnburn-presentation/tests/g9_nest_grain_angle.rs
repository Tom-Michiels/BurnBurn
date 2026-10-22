//! G9-cam-werk, nest: the grain of a part as a free angle (next to follows / across / free). The
//! fixture is analytic: a 40 x 10 strip drawn turned 30 degrees whose grain runs along its length
//! (30 degrees in its own coordinates), and a bed of 45 x 12 that only holds it lying flat.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

mod edit_rig;

use burnburn_document::ObjectKind;
use burnburn_geometry::{Bounds, Path, Point};
use burnburn_job::{MachineFrame, MachineProfile, OriginCorner, PowerRatio};
use burnburn_presentation::{
    CommandKey, NestInput, NestPanelViewModel, NestPartGrain, NestRotations, SchikkenIntent, UiIntent,
};
use burnburn_ruida::{Magic, OverscanPolicy, RuidaProfile};
use burnburn_types::{DocumentId, GeometryId, StableId};
use edit_rig::{opened_with_kinds, view};

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

/// A 40 x 10 rectangle turned `deg` degrees about (300, 300), anywhere on the page.
fn strip(deg: f64) -> Path {
    let (s, c) = deg.to_radians().sin_cos();
    let corner = |x: f64, y: f64| Point::new(300.0 + x * c - y * s, 300.0 + x * s + y * c);
    Path::polyline(&[corner(0.0, 0.0), corner(40.0, 0.0), corner(40.0, 10.0), corner(0.0, 10.0)], true).unwrap()
}

fn rig(deg: f64) -> (Harness, DocumentId, GeometryId) {
    let (mut h, doc, ids) = opened_with_kinds(vec![(strip(deg), ObjectKind::Path)]);
    bed(&mut h, 45.0, 12.0);
    h.invoke(doc, CommandKey::QuickNest);
    (h, doc, ids[0])
}

fn input(rule: Option<NestPartGrain>, id: GeometryId) -> NestInput {
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
fn a_strip_drawn_at_thirty_degrees_does_not_fit_when_it_follows_the_sheet() {
    let (mut h, doc, id) = rig(30.0);
    options(&mut h, doc, input(None, id));
    let report = nest(&mut h, doc).report.expect("report");
    assert_eq!((report.placed, report.unplaced.len()), (0, 1), "no quarter turn lays a 30 degree strip flat");
}

#[test]
fn a_part_with_its_grain_at_a_free_angle_turns_until_that_grain_lies_along_the_sheet() {
    let (mut h, doc, id) = rig(30.0);
    options(&mut h, doc, input(Some(NestPartGrain::at_degrees(30.0)), id));
    assert_eq!(panel(&h, doc).part_rows[0].grain, NestPartGrain::Angle(300), "the row shows the angle");
    assert_eq!(nest(&mut h, doc).report.expect("report").placed, 1);
    h.send(UiIntent::Schikken(SchikkenIntent::ApplyNest { document: doc }));
    let b = bounds_of(&h, doc, id);
    assert!((b.width() - 40.0).abs() < 1e-6 && (b.height() - 10.0).abs() < 1e-6, "lies flat: {b:?}");
}

#[test]
fn the_angle_is_a_direction_not_a_sense_and_wraps_at_a_half_turn() {
    // 210 degrees is the same grain as 30; the strip still lies flat (turned by 150 or 330 degrees).
    assert_eq!(NestPartGrain::at_degrees(210.0), NestPartGrain::at_degrees(30.0));
    assert_eq!(NestPartGrain::at_degrees(-150.0), NestPartGrain::Angle(300));
    let (mut h, doc, id) = rig(30.0);
    options(&mut h, doc, input(Some(NestPartGrain::at_degrees(210.0)), id));
    assert_eq!(nest(&mut h, doc).report.expect("report").placed, 1);
}

#[test]
fn a_wrong_angle_keeps_the_part_off_a_bed_that_needs_the_right_one() {
    // The grain is said to run at 60 degrees, but the strip lies at 30: lined up, it stands 30 degrees
    // off the bed and does not fit the 12 mm height.
    let (mut h, doc, id) = rig(30.0);
    options(&mut h, doc, input(Some(NestPartGrain::at_degrees(60.0)), id));
    let report = nest(&mut h, doc).report.expect("report");
    assert_eq!((report.placed, report.unplaced.len()), (0, 1));
}

#[test]
fn without_a_sheet_grain_the_angle_rule_changes_nothing() {
    let (mut h, doc, id) = rig(0.0);
    let mut free = input(Some(NestPartGrain::at_degrees(60.0)), id);
    free.grain = String::new();
    options(&mut h, doc, free);
    assert_eq!(nest(&mut h, doc).report.expect("report").placed, 1, "a flat strip fits a bed of 45 x 12 as is");
}
