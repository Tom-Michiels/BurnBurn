//! C10 (G7-cam): parts in the holes of other parts, and tight packing, in the nest panel. An
//! object inside another one is a hole of that part (it moves with it, and the space is free for
//! other parts) unless the panel says they are separate. Tight packing places against the
//! neighbours with exact geometry, where the raster alone leaves up to a cell.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

mod edit_rig;

use burnburn_document::ObjectKind;
use burnburn_geometry::{Bounds, Path, Point};
use burnburn_job::{MachineFrame, MachineProfile, OriginCorner, PowerRatio};
use burnburn_presentation::{CommandKey, NestHoles, NestInput, NestPanelViewModel, SchikkenIntent, UiIntent};
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
    send(h, SchikkenIntent::SetNestOptions { document: doc, input });
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

fn rectangle(x: f64, y: f64, w: f64, h: f64) -> Path {
    Path::polyline(&[Point::new(x, y), Point::new(x + w, y), Point::new(x + w, y + h), Point::new(x, y + h)], true)
        .unwrap()
}

/// A 100 x 100 plate with a 60 x 60 hole (two objects) and four 25 x 25 parts, far from a 110 x 110 bed.
fn plate_with_small_parts() -> (Harness, DocumentId, Vec<GeometryId>) {
    let (mut h, doc, ids) = doc_with(vec![
        square(300.0, 300.0, 100.0),
        square(320.0, 320.0, 60.0),
        square(500.0, 300.0, 25.0),
        square(540.0, 300.0, 25.0),
        square(580.0, 300.0, 25.0),
        square(620.0, 300.0, 25.0),
    ]);
    bed(&mut h, 110.0, 110.0);
    h.invoke(doc, CommandKey::QuickNest);
    (h, doc, ids)
}

#[test]
fn an_object_inside_another_is_a_hole_of_that_part_unless_the_panel_says_otherwise() {
    let (mut h, doc, _) = plate_with_small_parts();
    let p = panel(&h, doc);
    assert_eq!(p.input.holes, NestHoles::Join, "the default: an inner contour is a hole");
    assert_eq!(p.parts, 5, "the plate and its hole are one part, plus four small ones");
    assert_eq!(p.part_rows.len(), 5);

    options(&mut h, doc, NestInput { holes: NestHoles::Separate, ..NestInput::default() });
    assert_eq!(panel(&h, doc).parts, 6, "separate: the hole is a part of its own");
}

#[test]
fn small_parts_go_into_the_hole_of_the_plate_and_the_hole_moves_with_the_plate() {
    let (mut h, doc, ids) = plate_with_small_parts();
    options(&mut h, doc, NestInput { padding: "1".into(), edge_buffer: "1".into(), ..NestInput::default() });
    let report = nest(&mut h, doc).report.expect("report");
    assert_eq!(report.placed, 5, "{:?}", report.unplaced);
    send(&mut h, SchikkenIntent::ApplyNest { document: doc });
    let (plate, hole) = (bounds_of(&h, doc, ids[0]), bounds_of(&h, doc, ids[1]));
    assert!((hole.min().x - plate.min().x - 20.0).abs() < 1e-6 && (hole.min().y - plate.min().y - 20.0).abs() < 1e-6);
    assert!((hole.width() - 60.0).abs() < 1e-6, "the hole keeps its place in the plate and its size");
    for small in &ids[2..] {
        let b = bounds_of(&h, doc, *small);
        assert!(
            b.min().x >= hole.min().x - 1e-6
                && b.min().y >= hole.min().y - 1e-6
                && b.max().x <= hole.max().x + 1e-6
                && b.max().y <= hole.max().y + 1e-6,
            "a small part lies in the hole of the plate: {b:?} vs {hole:?}"
        );
    }
}

#[test]
fn tight_packing_places_what_the_raster_alone_cannot() {
    // Two 10.4 x 10 parts on a bed whose usable width (22.8 less 1 mm at both edges) is exactly 20.8.
    let make = || {
        let (mut h, doc, ids) = doc_with(vec![rectangle(300.0, 300.0, 10.4, 10.0), rectangle(340.0, 300.0, 10.4, 10.0)]);
        bed(&mut h, 22.8, 12.0);
        h.invoke(doc, CommandKey::QuickNest);
        (h, doc, ids)
    };
    let base = NestInput { padding: "0".into(), edge_buffer: "1".into(), step: "1".into(), ..NestInput::default() };

    let (mut h, doc, _) = make();
    assert!(!panel(&h, doc).input.tight, "tight packing is an option, off by default");
    options(&mut h, doc, base.clone());
    assert_eq!(nest(&mut h, doc).report.expect("report").placed, 1, "the premise: the raster places one");

    let (mut h, doc, ids) = make();
    options(&mut h, doc, NestInput { tight: true, ..base });
    let report = nest(&mut h, doc).report.expect("report");
    assert_eq!(report.placed, 2, "{:?}", report.unplaced);
    send(&mut h, SchikkenIntent::ApplyNest { document: doc });
    let mut xs: Vec<f64> = ids.iter().map(|id| bounds_of(&h, doc, *id).min().x).collect();
    xs.sort_by(f64::total_cmp);
    assert!((xs[0] - 1.0).abs() < 5e-3 && (xs[1] - 11.4).abs() < 5e-3, "{xs:?}");
}
