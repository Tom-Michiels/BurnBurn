//! C12 (G7-cam): common-line cutting in the nest panel. With it the parts are packed without
//! space between them, so they share edges; the report says how much shared edge there is. Whether
//! the machine may cut such a shared line once is the `CommonLine` capability of the profile.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

mod edit_rig;

use burnburn_document::ObjectKind;
use burnburn_geometry::{Path, Point};
use burnburn_job::{MachineFrame, MachineProfile, OriginCorner, PowerRatio};
use burnburn_presentation::{CommandKey, NestInput, NestPanelViewModel, SchikkenIntent, UiIntent};
use burnburn_ruida::{Magic, OverscanPolicy, RuidaProfile};
use burnburn_types::DocumentId;
use edit_rig::{opened_with_kinds, view};

type Harness = burnburn_presentation::HeadlessEditorHarness;

fn rectangle(x: f64, y: f64, w: f64, h: f64) -> Path {
    Path::polyline(&[Point::new(x, y), Point::new(x + w, y), Point::new(x + w, y + h), Point::new(x, y + h)], true)
        .unwrap()
}

/// Two 20 x 10 parts far from a 40 x 10 bed.
fn rig() -> (Harness, DocumentId) {
    let (mut h, doc, _) = opened_with_kinds(vec![
        (rectangle(300.0, 300.0, 20.0, 10.0), ObjectKind::Path),
        (rectangle(340.0, 300.0, 20.0, 10.0), ObjectKind::Path),
    ]);
    let profile = MachineProfile::new(
        "nest-bed",
        1,
        MachineFrame::new(40.0, 10.0, OriginCorner::BottomLeft).unwrap(),
        1000.0,
        400.0,
        PowerRatio::new(0.0).unwrap(),
        PowerRatio::new(1.0).unwrap(),
    )
    .unwrap();
    let ruida = RuidaProfile { magic: Magic::FIXTURE_0X88, overscan: OverscanPolicy::ControllerManaged };
    h.send(UiIntent::SetMachineProfile { profile: Box::new(profile), ruida });
    h.invoke(doc, CommandKey::QuickNest);
    (h, doc)
}

fn panel(h: &Harness, doc: DocumentId) -> NestPanelViewModel {
    view(h, doc).schikken.nest.expect("the nest panel is open")
}

fn nest_with(h: &mut Harness, doc: DocumentId, input: NestInput) -> NestPanelViewModel {
    h.send(UiIntent::Schikken(SchikkenIntent::SetNestOptions { document: doc, input: Box::new(input) }));
    h.send(UiIntent::Schikken(SchikkenIntent::StartNest { document: doc }));
    h.run_tasks();
    panel(h, doc)
}

fn base() -> NestInput {
    NestInput { padding: "5".into(), edge_buffer: "0".into(), ..NestInput::default() }
}

#[test]
fn with_common_line_the_parts_touch_and_the_report_counts_the_shared_edge() {
    let (mut h, doc) = rig();
    assert!(!panel(&h, doc).input.common_line, "off unless chosen");

    let without = nest_with(&mut h, doc, base()).report.expect("report");
    assert_eq!(without.placed, 1, "5 mm of padding leaves room for one part only");
    assert_eq!(without.shared_mm, None, "nothing is said about shared edges without the option");

    let with = nest_with(&mut h, doc, NestInput { common_line: true, ..base() }).report.expect("report");
    assert_eq!(with.placed, 2, "{:?}", with.unplaced);
    let shared = with.shared_mm.expect("the shared edge is counted");
    assert!((shared - 10.0).abs() < 0.05, "the 10 mm side the two parts share: {shared}");
}
