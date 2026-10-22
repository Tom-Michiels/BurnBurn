//! C10/C12 (G7-cam): the grain direction of the sheet in the nest panel. With a grain the parts
//! keep their own X axis along it (they turn only to the orientations that follow it); without
//! one they turn freely. The domain rule is `burnburn_cam::nest`; the panel names the direction.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

mod edit_rig;

use burnburn_document::ObjectKind;
use burnburn_geometry::{Bounds, Path, Point};
use burnburn_job::{MachineFrame, MachineProfile, OriginCorner, PowerRatio};
use burnburn_presentation::{
    CommandKey, NestFailure, NestField, NestInput, NestPanelViewModel, NestRotations, SchikkenIntent, SchikkenMessage,
    UiIntent,
};
use burnburn_ruida::{Magic, OverscanPolicy, RuidaProfile};
use burnburn_types::{DocumentId, GeometryId};
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

fn long_part() -> Path {
    // 40 mm along X, 10 mm along Y, far from the bed.
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

fn input(grain: &str) -> NestInput {
    NestInput {
        grain: grain.to_owned(),
        rotations: NestRotations::Quarter,
        padding: "0".into(),
        edge_buffer: "0".into(),
        ..NestInput::default()
    }
}

fn bounds_of(h: &Harness, doc: DocumentId, id: GeometryId) -> Bounds {
    let scene = h.scene(doc).expect("scene");
    scene.items.iter().find(|i| i.source == id).and_then(|i| i.path.bounds()).expect("object in scene")
}

#[test]
fn without_a_grain_the_part_turns_to_fit_and_with_one_along_x_it_cannot() {
    let (mut h, doc, id) = rig();
    assert_eq!(panel(&h, doc).input.grain, "", "no grain unless the operator names one");
    options(&mut h, doc, input(""));
    assert_eq!(nest(&mut h, doc).report.expect("report").placed, 1);
    h.send(UiIntent::Schikken(SchikkenIntent::ApplyNest { document: doc }));
    let b = bounds_of(&h, doc, id);
    assert!((b.width() - 10.0).abs() < 1e-6 && (b.height() - 40.0).abs() < 1e-6, "it stands up: {b:?}");

    let (mut h, doc, _) = rig();
    options(&mut h, doc, input("0"));
    let report = nest(&mut h, doc).report.expect("report");
    assert_eq!((report.placed, report.unplaced.len()), (0, 1), "a grain along X leaves no orientation that fits");
}

#[test]
fn a_grain_across_the_sheet_makes_the_part_stand_and_a_text_with_the_unit_is_read() {
    let (mut h, doc, id) = rig();
    options(&mut h, doc, input("90 °"));
    assert_eq!(nest(&mut h, doc).report.expect("report").placed, 1);
    h.send(UiIntent::Schikken(SchikkenIntent::ApplyNest { document: doc }));
    let b = bounds_of(&h, doc, id);
    assert!((b.width() - 10.0).abs() < 1e-6 && (b.height() - 40.0).abs() < 1e-6, "{b:?}");
}

#[test]
fn a_grain_that_is_not_an_angle_is_refused_by_name() {
    let (mut h, doc, _) = rig();
    for bad in ["abc", "9x", "90 90"] {
        options(&mut h, doc, input(bad));
        assert_eq!(
            panel(&h, doc).error,
            Some(SchikkenMessage::Nest(NestFailure::InvalidField(NestField::Grain))),
            "{bad:?}"
        );
    }
    options(&mut h, doc, input("  "));
    assert_eq!(panel(&h, doc).error, None, "blank means no grain");
}
