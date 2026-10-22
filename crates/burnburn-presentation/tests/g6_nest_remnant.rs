//! G6-werk, W07: nesting on a remnant. A drawn closed contour is the sheet (a leftover plate with
//! its own shape); the other objects that stay in the drawing are its exclusion zones; the parts go
//! inside the contour and the report says what did not fit.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

mod edit_rig;

use burnburn_document::ObjectKind;
use burnburn_geometry::{Bounds, Path, Point};
use burnburn_job::{MachineFrame, MachineProfile, OriginCorner, PowerRatio};
use burnburn_presentation::{
    CommandKey, NestFailure, NestInput, NestPanelViewModel, NestRun, SchikkenIntent, SchikkenMessage, UiIntent,
};
use burnburn_ruida::{Magic, OverscanPolicy, RuidaProfile};
use burnburn_types::{DocumentId, GeometryId, StableId};
use edit_rig::{opened_with_kinds, select, square, view};

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

/// An L-shaped remnant: 100 x 100 with the upper right 50 x 50 corner gone (area 7 500 mm²).
fn l_shape() -> Path {
    Path::polyline(
        &[
            Point::new(0.0, 0.0),
            Point::new(100.0, 0.0),
            Point::new(100.0, 50.0),
            Point::new(50.0, 50.0),
            Point::new(50.0, 100.0),
            Point::new(0.0, 100.0),
        ],
        true,
    )
    .unwrap()
}

fn panel(h: &Harness, doc: DocumentId) -> NestPanelViewModel {
    view(h, doc).schikken.nest.expect("the nest panel is open")
}

fn send(h: &mut Harness, intent: SchikkenIntent) {
    h.send(UiIntent::Schikken(intent));
}

fn nest(h: &mut Harness, doc: DocumentId, input: NestInput) -> NestPanelViewModel {
    send(h, SchikkenIntent::SetNestOptions { document: doc, input });
    send(h, SchikkenIntent::StartNest { document: doc });
    h.run_tasks();
    panel(h, doc)
}

fn bounds_of(h: &Harness, doc: DocumentId, id: GeometryId) -> Bounds {
    let scene = h.scene(doc).expect("scene");
    scene.items.iter().find(|i| i.source == id).and_then(|i| i.path.bounds()).expect("object in scene")
}

/// The contour first, then three 30 x 30 parts and a 60 x 60 one far away from it.
fn remnant_and_parts() -> (Harness, DocumentId, Vec<GeometryId>) {
    let (mut h, doc, ids) = opened_with_kinds(vec![
        (l_shape(), ObjectKind::Path),
        (square(300.0, 300.0, 30.0), ObjectKind::Path),
        (square(340.0, 300.0, 30.0), ObjectKind::Path),
        (square(380.0, 300.0, 30.0), ObjectKind::Path),
        (square(420.0, 300.0, 60.0), ObjectKind::Path),
    ]);
    bed(&mut h, 1000.0, 1000.0);
    (h, doc, ids)
}

#[test]
fn the_panel_offers_the_closed_objects_as_a_sheet_and_the_machine_bed_stays_the_default() {
    let (mut h, doc, ids) = remnant_and_parts();
    h.invoke(doc, CommandKey::QuickNest);
    let p = panel(&h, doc);
    assert_eq!(p.input.sheet_object, None, "the bed unless a contour is chosen");
    assert!(p.sheet.unwrap().is_bed);
    assert_eq!(p.sheet_choices.len(), 5, "every closed object may be the sheet");
    assert_eq!(p.sheet_choices[0].object, ids[0].raw());
    assert_eq!(p.sheet_choices[0].name, "Pad 1");
}

#[test]
fn a_chosen_contour_is_the_sheet_and_the_parts_go_inside_it_with_what_does_not_fit_reported() {
    let (mut h, doc, ids) = remnant_and_parts();
    select(&mut h, doc, &ids[1..]);
    h.invoke(doc, CommandKey::NestSelected);
    let p = nest(&mut h, doc, NestInput { sheet_object: Some(ids[0].raw()), ..NestInput::default() });
    assert_eq!(p.run, NestRun::Done);
    let sheet = p.sheet.expect("sheet");
    assert!(!sheet.is_bed);
    assert_eq!(sheet.contour, Some(ids[0].raw()));
    assert!((sheet.width_mm - 100.0).abs() < 1e-6 && (sheet.height_mm - 100.0).abs() < 1e-6);
    let report = p.report.expect("report");
    // The 60 x 60 part does not fit either arm (50 mm wide); the three 30 x 30 do.
    assert_eq!(report.placed, 3);
    assert_eq!(report.unplaced.len(), 1);
    assert_eq!(report.unplaced[0].objects, vec![ids[4]]);
    let sheet_use = report.sheets[0];
    assert!((sheet_use.net_mm2 - 7500.0).abs() < 100.0, "the contour's own area, not its bounding box: {sheet_use:?}");
    assert!((sheet_use.placed_mm2 - 2700.0).abs() < 1.0);

    send(&mut h, SchikkenIntent::ApplyNest { document: doc });
    for id in &ids[1..4] {
        let b = bounds_of(&h, doc, *id);
        assert!(
            b.min().x >= 1.0 - 1e-9 && b.min().y >= 1.0 - 1e-9 && b.max().x <= 99.0 + 1e-9 && b.max().y <= 99.0 + 1e-9,
            "{b:?}"
        );
        // Not in the missing corner: x > 50 and y > 50 at the same time.
        assert!(!(b.max().x > 50.0 + 1e-9 && b.max().y > 50.0 + 1e-9), "inside the corner that is gone: {b:?}");
    }
    let far = bounds_of(&h, doc, ids[4]);
    assert!((far.min().x - 420.0).abs() < 1e-9, "the part that did not fit stays where it was");
    assert!((bounds_of(&h, doc, ids[0]).width() - 100.0).abs() < 1e-9, "the contour itself does not move");
}

#[test]
fn another_object_that_stays_is_an_exclusion_zone_of_the_remnant() {
    let (mut h, doc, ids) = opened_with_kinds(vec![
        (l_shape(), ObjectKind::Path),
        (square(10.0, 10.0, 30.0), ObjectKind::Path), // a damaged spot on the remnant that stays
        (square(300.0, 300.0, 30.0), ObjectKind::Path),
        (square(340.0, 300.0, 30.0), ObjectKind::Path),
    ]);
    bed(&mut h, 1000.0, 1000.0);
    select(&mut h, doc, &ids[2..]);
    h.invoke(doc, CommandKey::NestSelected);
    let p = nest(&mut h, doc, NestInput { sheet_object: Some(ids[0].raw()), ..NestInput::default() });
    assert_eq!(p.report.expect("report").placed, 2);
    send(&mut h, SchikkenIntent::ApplyNest { document: doc });
    let zone = bounds_of(&h, doc, ids[1]);
    for id in &ids[2..] {
        let b = bounds_of(&h, doc, *id);
        assert!(b.max().x <= 99.0 + 1e-9 && b.max().y <= 99.0 + 1e-9, "on the remnant: {b:?}");
        let separated = b.min().x >= zone.max().x + 1.0 - 1e-6
            || b.max().x <= zone.min().x - 1.0 + 1e-6
            || b.min().y >= zone.max().y + 1.0 - 1e-6
            || b.max().y <= zone.min().y - 1.0 + 1e-6;
        assert!(separated, "clear of the exclusion zone: {b:?} vs {zone:?}");
    }
}

#[test]
fn the_sheet_cannot_be_one_of_the_parts_and_a_deleted_sheet_is_said() {
    let (mut h, doc, ids) = remnant_and_parts();
    select(&mut h, doc, &ids[..2]);
    h.invoke(doc, CommandKey::NestSelected);
    send(
        &mut h,
        SchikkenIntent::SetNestOptions {
            document: doc,
            input: NestInput { sheet_object: Some(ids[0].raw()), ..NestInput::default() },
        },
    );
    send(&mut h, SchikkenIntent::StartNest { document: doc });
    h.run_tasks();
    assert_eq!(panel(&h, doc).error, Some(SchikkenMessage::Nest(NestFailure::SheetIsPart)));
    assert_eq!(panel(&h, doc).run, NestRun::Idle, "nothing ran");

    // A sheet that is not there any more.
    send(
        &mut h,
        SchikkenIntent::SetNestOptions {
            document: doc,
            input: NestInput { sheet_object: Some(9999), ..NestInput::default() },
        },
    );
    send(&mut h, SchikkenIntent::StartNest { document: doc });
    assert_eq!(panel(&h, doc).error, Some(SchikkenMessage::Nest(NestFailure::SheetGone)));
}

#[test]
fn in_quick_nest_the_chosen_contour_is_not_nested_itself() {
    let (mut h, doc, ids) = remnant_and_parts();
    h.invoke(doc, CommandKey::QuickNest);
    let p = nest(&mut h, doc, NestInput { sheet_object: Some(ids[0].raw()), ..NestInput::default() });
    assert_eq!(p.parts, 4, "the four parts, not the contour");
    send(&mut h, SchikkenIntent::ApplyNest { document: doc });
    assert!((bounds_of(&h, doc, ids[0]).min().x).abs() < 1e-9, "the contour stays at the origin");
    let _ = ids.iter().map(|i| i.raw()).sum::<u64>();
}
