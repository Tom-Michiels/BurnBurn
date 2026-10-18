//! IM25 process extensions: the capability matrix gates every feature with a typed error, and
//! each feature has an analytic effect on the plan (C04, C05, C11).

#![allow(clippy::unwrap_used)]

mod common;

use burnburn_cam::{CompileError, compile, compile_with};
use burnburn_document::process::{
    AdvancedOptions, FillSettings, ImageRender, ImageSettings, Lead, LineOptions, OffsetFillOrder, OffsetFillSettings,
    OperationDraft, OperationMode, Overcut, Passes, Perforation, ProcessCommand, ScanAxis as DocAxis, ScanDirection,
    SubOperation, TabSpec, ZStep,
};
use burnburn_document::{CommandPayload, DocumentSession, GeometryCommand};
use burnburn_geometry::{PathBuilder, Point};
use burnburn_image::SourceImage;
use burnburn_job::{Feature, JobPlan, MachineCapabilities, MachineProfile, PlanEvent, PlanMode, Support};
use burnburn_types::{CancellationToken, GeometryId, OperationId};
use common::*;

/// Compiles with the head at (−1, −1), so the first travel to an entry at (0, 0) is visible.
fn plan(s: &DocumentSession, p: &MachineProfile) -> Result<JobPlan, CompileError> {
    let mut o = options();
    o.start = burnburn_cam::StartPoint::Bed(Point::new(-1.0, -1.0));
    compile(&s.snapshot(), p, &o, &CancellationToken::never())
}

fn supporting(features: &[Feature]) -> MachineProfile {
    let caps = features.iter().fold(MachineCapabilities::conservative(), |c, f| c.with(*f, Support::Supported));
    profile().with_capabilities(caps).unwrap()
}

fn set_advanced(s: &mut DocumentSession, operation: OperationId, advanced: AdvancedOptions) {
    run(s, CommandPayload::Process(ProcessCommand::SetAdvanced { operation, advanced }));
}

fn line_with(s: &mut DocumentSession, g: GeometryId, line: LineOptions) -> OperationId {
    let op = add_op(s, line_op("Snijden", 10.0, 50.0), vec![g]);
    set_advanced(s, op, AdvancedOptions { line, ..Default::default() });
    op
}

fn near(a: (f64, f64), b: (f64, f64)) -> bool {
    (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9
}

fn grayscale_doc() -> (DocumentSession, SourceImage, OperationId) {
    let source = SourceImage::from_gray8(2, 1, vec![0, 255]).unwrap();
    let mut s = session();
    let frame = rectangle(&mut s, 10.0, 10.0, 0.2, 0.1);
    let image = ImageSettings::new(source.content_hash(), mm(0.1), mm(0.1), ImageRender::Grayscale).unwrap();
    let draft = OperationDraft {
        name: "Grijs".into(),
        mode: OperationMode::Image(image),
        settings: settings(100.0, 60.0),
        output_enabled: true,
    };
    let op = add_op(&mut s, draft, vec![frame]);
    (s, source, op)
}

#[test]
fn unsupported_modulation_mode_is_rejected_instead_of_replaced() {
    let (s, source, op) = grayscale_doc();
    let sources = vec![source];
    // Default matrix: grayscale modulation is unverified → typed error naming the capability.
    let err = compile_with(&s.snapshot(), &profile(), &options(), &sources, &CancellationToken::never()).unwrap_err();
    assert!(
        matches!(&err, CompileError::UnsupportedFeature { operation, feature: Feature::GrayscaleModulation,
                 support: Support::Unverified { .. } } if *operation == op),
        "{err:?}"
    );
    // A profile that claims support gets power-modulated runs — never dithered on/off runs.
    let p = supporting(&[Feature::GrayscaleModulation]);
    let plan = compile_with(&s.snapshot(), &p, &options(), &sources, &CancellationToken::never()).unwrap();
    assert!(matches!(plan.settings()[0].mode, PlanMode::GrayscaleScan { .. }));
    assert!(!plan.events().iter().any(|e| matches!(e, PlanEvent::ScanRun { .. })), "not replaced by dithering");
    let powers: Vec<f64> = plan
        .events()
        .iter()
        .filter_map(|e| match e {
            PlanEvent::PowerRun { power, .. } => Some(power.get()),
            _ => None,
        })
        .collect();
    // Black pixel at maximum power (60 %); the white pixel is not burned.
    assert_eq!(powers, vec![0.6]);
}

#[test]
fn capability_matrix_gates_each_feature_with_a_typed_error() {
    let mut s = session();
    let g = rectangle(&mut s, 0.0, 0.0, 10.0, 10.0);
    let mut twice = line_op("Snijden", 10.0, 50.0);
    twice.settings.passes = Passes::new(2).unwrap();
    let op = add_op(&mut s, twice, vec![g]);
    set_advanced(&mut s, op, AdvancedOptions { z_step: Some(ZStep::new(mm(0.5)).unwrap()), ..Default::default() });
    let err = plan(&s, &profile()).unwrap_err();
    assert_eq!(
        err,
        CompileError::UnsupportedFeature { operation: op, feature: Feature::ZStep, support: Support::Unsupported }
    );

    let mut t = session();
    let g = rectangle(&mut t, 0.0, 0.0, 10.0, 10.0);
    let fill = FillSettings::new(mm(1.0), DocAxis::X, ScanDirection::Bidirectional, mm(0.0)).unwrap();
    let draft = OperationDraft {
        name: "Schuin".into(),
        mode: OperationMode::Fill(fill.with_angle(45.0).unwrap()),
        settings: settings(100.0, 20.0),
        output_enabled: true,
    };
    let op = add_op(&mut t, draft, vec![g]);
    let err = plan(&t, &profile()).unwrap_err();
    assert!(matches!(
        err,
        CompileError::UnsupportedFeature { operation, feature: Feature::ScanAngle, support: Support::Unverified { .. } }
            if operation == op
    ));
    // An explicit "unsupported" in the profile wins over the default "supported" for tabs.
    let mut u = session();
    let g = rectangle(&mut u, 0.0, 0.0, 10.0, 10.0);
    let tabbed =
        line_with(&mut u, g, LineOptions { tabs: Some(TabSpec::new(2, mm(1.0)).unwrap()), ..Default::default() });
    let no_tabs = profile()
        .with_capabilities(MachineCapabilities::conservative().with(Feature::Tabs, Support::Unsupported))
        .unwrap();
    assert_eq!(
        plan(&u, &no_tabs).unwrap_err(),
        CompileError::UnsupportedFeature { operation: tabbed, feature: Feature::Tabs, support: Support::Unsupported }
    );
    assert_ne!(no_tabs.revision().content, profile().revision().content, "the matrix is profile content");
    // Line-only options on a fill are refused, not ignored.
    let mut v = session();
    let g = rectangle(&mut v, 0.0, 0.0, 10.0, 10.0);
    let op = add_op(&mut v, fill_op("Vullen", 1.0, 0.0), vec![g]);
    set_advanced(
        &mut v,
        op,
        AdvancedOptions {
            line: LineOptions { overcut: Some(Overcut::new(mm(1.0)).unwrap()), ..Default::default() },
            ..Default::default()
        },
    );
    assert_eq!(plan(&v, &profile()).unwrap_err(), CompileError::LineOptionsOnNonLineMode { operation: op });
}

/// `(min x, min y, max x, max y)` of each travel-started chain of cuts.
fn chains(events: &[PlanEvent]) -> Vec<(f64, f64, f64, f64)> {
    let mut out: Vec<Vec<(f64, f64)>> = Vec::new();
    for (kind, x, y) in motion(events) {
        match kind {
            "travel" => out.push(vec![(x, y)]),
            _ => out.last_mut().unwrap().push((x, y)),
        }
    }
    out.iter()
        .map(|c| {
            c.iter().fold((f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY), |b, p| {
                (b.0.min(p.0), b.1.min(p.1), b.2.max(p.0), b.3.max(p.1))
            })
        })
        .collect()
}

fn offset_fill(order: OffsetFillOrder) -> DocumentSession {
    let mut s = session();
    let g = rectangle(&mut s, 0.0, 0.0, 10.0, 10.0);
    let draft = OperationDraft {
        name: "Offset".into(),
        mode: OperationMode::OffsetFill(OffsetFillSettings::new(mm(2.0), order).unwrap()),
        settings: settings(100.0, 20.0),
        output_enabled: true,
    };
    add_op(&mut s, draft, vec![g]);
    s
}

#[test]
fn offset_fill_cuts_concentric_rings_at_half_step_insets() {
    // 10 × 10 square, step 2: rings inset by 1 and 3 mm; an inset of 5 mm collapses the area.
    let plan = plan(&offset_fill(OffsetFillOrder::OutsideIn), &profile()).unwrap();
    assert_eq!(plan.settings()[0].mode, PlanMode::Line);
    let c = chains(plan.events());
    assert_eq!(c.len(), 2, "{c:?}");
    let close = |a: (f64, f64, f64, f64), b: (f64, f64, f64, f64)| {
        (a.0 - b.0).abs() < 1e-4 && (a.1 - b.1).abs() < 1e-4 && (a.2 - b.2).abs() < 1e-4 && (a.3 - b.3).abs() < 1e-4
    };
    assert!(close(c[0], (1.0, 1.0, 9.0, 9.0)) && close(c[1], (3.0, 3.0, 7.0, 7.0)), "{c:?}");
    let inside_out = chains(
        burnburn_cam::compile(
            &offset_fill(OffsetFillOrder::InsideOut).snapshot(),
            &profile(),
            &options(),
            &CancellationToken::never(),
        )
        .unwrap()
        .events(),
    );
    assert!(close(inside_out[0], (3.0, 3.0, 7.0, 7.0)), "{inside_out:?}");
}

#[test]
fn scan_angle_rotates_rows_and_keeps_the_interval() {
    let mut s = session();
    let g = rectangle(&mut s, 0.0, 0.0, 10.0, 10.0);
    let fill = FillSettings::new(mm(1.0), DocAxis::X, ScanDirection::Bidirectional, mm(0.0)).unwrap();
    let draft = OperationDraft {
        name: "Schuin".into(),
        mode: OperationMode::Fill(fill.with_angle(45.0).unwrap()),
        settings: settings(100.0, 20.0),
        output_enabled: true,
    };
    add_op(&mut s, draft, vec![g]);
    let plan = plan(&s, &supporting(&[Feature::ScanAngle])).unwrap();
    assert!(matches!(plan.settings()[0].mode, PlanMode::AngledScan { angle_deg, .. } if angle_deg == 45.0));
    let lines: Vec<_> = plan
        .events()
        .iter()
        .filter_map(|e| match e {
            PlanEvent::ScanRun { line, .. } => Some(*line),
            _ => None,
        })
        .collect();
    // The square spans 10·√2 = 14.14 mm across the 45° rows: 14 rows of 1 mm.
    assert_eq!(lines.len(), 14);
    let h = std::f64::consts::FRAC_1_SQRT_2;
    let mut offsets = Vec::new();
    for l in &lines {
        let (dx, dy) = (l.to.x_mm() - l.from.x_mm(), l.to.y_mm() - l.from.y_mm());
        let len = dx.hypot(dy);
        assert!((dx / len).abs() - h < 1e-9 && (dx / len - dy / len).abs() < 1e-9, "parallel to (1, 1)");
        // Ends lie on the square's boundary.
        for p in [l.from, l.to] {
            let (x, y) = (p.x_mm(), p.y_mm());
            let on = |v: f64| v.abs() < 1e-9 || (v - 10.0).abs() < 1e-9;
            assert!((on(x) && (-1e-9..=10.0 + 1e-9).contains(&y)) || (on(y) && (-1e-9..=10.0 + 1e-9).contains(&x)));
        }
        offsets.push(-h * l.from.x_mm() + h * l.from.y_mm());
    }
    offsets.sort_by(f64::total_cmp);
    for w in offsets.windows(2) {
        assert!((w[1] - w[0] - 1.0).abs() < 1e-9, "rows 1 mm apart perpendicular to the scan direction");
    }
}

#[test]
fn tabs_leave_evenly_spaced_bridges_and_reject_tabs_longer_than_the_contour() {
    // 40 × 10 rectangle, perimeter 100, entered at (0, 0) counter-clockwise. Two 2 mm tabs
    // centred at arc length 25 (bottom edge, x = 25) and 75 (top edge, x = 40 − 25 = 15).
    let mut s = session();
    let g = rectangle(&mut s, 0.0, 0.0, 40.0, 10.0);
    line_with(&mut s, g, LineOptions { tabs: Some(TabSpec::new(2, mm(2.0)).unwrap()), ..Default::default() });
    let m = motion(plan(&s, &profile()).unwrap().events());
    let travels: Vec<(f64, f64)> = m.iter().filter(|e| e.0 == "travel").map(|e| (e.1, e.2)).collect();
    assert_eq!(travels.len(), 3, "{m:?}");
    assert!(near(travels[1], (26.0, 0.0)) && near(travels[2], (14.0, 10.0)), "{travels:?}");
    let i = m.iter().position(|e| e.0 == "travel" && near((e.1, e.2), (26.0, 0.0))).unwrap();
    assert!(near((m[i - 1].1, m[i - 1].2), (24.0, 0.0)), "the cut stops at the tab start");

    let mut t = session();
    let g = rectangle(&mut t, 0.0, 0.0, 40.0, 10.0);
    let op = line_with(&mut t, g, LineOptions { tabs: Some(TabSpec::new(2, mm(50.0)).unwrap()), ..Default::default() });
    assert_eq!(plan(&t, &profile()).unwrap_err(), CompileError::InvalidTabs { operation: op, geometry: g });
}

#[test]
fn leads_start_and_end_on_the_waste_side() {
    let lead = Some(Lead::new(mm(2.0)).unwrap());
    let opts = LineOptions { lead_in: lead, lead_out: lead, ..Default::default() };
    // Outer contour: waste is outside. Entry (0, 0), first edge +x → lead from (0, −2);
    // last edge −y → lead out to (−2, 0).
    let mut s = session();
    let g = rectangle(&mut s, 0.0, 0.0, 10.0, 10.0);
    line_with(&mut s, g, opts);
    let m = motion(plan(&s, &profile()).unwrap().events());
    assert!(near((m[0].1, m[0].2), (0.0, -2.0)) && m[0].0 == "travel", "{m:?}");
    assert!(near((m[1].1, m[1].2), (0.0, 0.0)) && m[1].0 == "cut");
    let last = m.last().unwrap();
    assert!(near((last.1, last.2), (-2.0, 0.0)) && last.0 == "cut", "{m:?}");

    // A hole: waste is inside the hole (80..90). Entry (80, 80): lead from (80, 82), out to (82, 80).
    let mut t = session();
    let ring = PathBuilder::new()
        .move_to(Point::new(10.0, 10.0))
        .line_to(Point::new(110.0, 10.0))
        .line_to(Point::new(110.0, 110.0))
        .line_to(Point::new(10.0, 110.0))
        .close()
        .move_to(Point::new(80.0, 80.0))
        .line_to(Point::new(90.0, 80.0))
        .line_to(Point::new(90.0, 90.0))
        .line_to(Point::new(80.0, 90.0))
        .close()
        .build()
        .unwrap();
    let g = run(&mut t, CommandPayload::Edit(GeometryCommand::CreatePath { path: ring, name: String::new() }))
        .changes
        .created[0];
    line_with(&mut t, g, opts);
    let m = motion(plan(&t, &profile()).unwrap().events());
    assert!(near((m[0].1, m[0].2), (80.0, 82.0)), "hole lead-in inside the hole: {m:?}");
    let k = m.iter().position(|e| e.0 == "travel" && e != &m[0]).unwrap();
    assert!(near((m[k - 1].1, m[k - 1].2), (82.0, 80.0)), "hole lead-out inside the hole: {m:?}");
    let (ox, oy) = (m[k].1, m[k].2);
    assert!(!(10.0..=110.0).contains(&ox) || !(10.0..=110.0).contains(&oy), "outer lead-in outside the plate: {m:?}");
}

#[test]
fn overcut_continues_past_the_entry_along_the_contour() {
    let mut s = session();
    let g = rectangle(&mut s, 0.0, 0.0, 10.0, 10.0);
    line_with(&mut s, g, LineOptions { overcut: Some(Overcut::new(mm(3.0)).unwrap()), ..Default::default() });
    let m = motion(plan(&s, &profile()).unwrap().events());
    let last = m.last().unwrap();
    assert!(near((last.1, last.2), (3.0, 0.0)), "{m:?}");
    assert_eq!(m.iter().filter(|e| e.0 == "cut").count(), 5);
}

#[test]
fn perforation_alternates_on_and_off_from_the_entry() {
    let mut s = session();
    let open = PathBuilder::new().move_to(Point::new(0.0, 0.0)).line_to(Point::new(10.0, 0.0)).build().unwrap();
    let g = run(&mut s, CommandPayload::Edit(GeometryCommand::CreatePath { path: open, name: String::new() }))
        .changes
        .created[0];
    let perforation = Some(Perforation::new(mm(2.0), mm(1.0)).unwrap());
    line_with(&mut s, g, LineOptions { perforation, ..Default::default() });
    let m: Vec<(&str, f64)> = motion(plan(&s, &profile()).unwrap().events()).iter().map(|e| (e.0, e.1)).collect();
    let expected = [
        ("travel", 0.0),
        ("cut", 2.0),
        ("travel", 3.0),
        ("cut", 5.0),
        ("travel", 6.0),
        ("cut", 8.0),
        ("travel", 9.0),
        ("cut", 10.0),
    ];
    assert_eq!(m.len(), expected.len(), "{m:?}");
    for (a, b) in m.iter().zip(expected) {
        assert!(a.0 == b.0 && (a.1 - b.1).abs() < 1e-9, "{m:?}");
    }
}

#[test]
fn passes_with_z_step_emit_focus_offsets_when_supported() {
    let mut s = session();
    let g = rectangle(&mut s, 0.0, 0.0, 10.0, 10.0);
    let mut thrice = line_op("Snijden", 10.0, 50.0);
    thrice.settings.passes = Passes::new(3).unwrap();
    let op = add_op(&mut s, thrice, vec![g]);
    set_advanced(&mut s, op, AdvancedOptions { z_step: Some(ZStep::new(mm(0.5)).unwrap()), ..Default::default() });
    let plan = plan(&s, &supporting(&[Feature::ZStep])).unwrap();
    let marks: Vec<String> = plan
        .events()
        .iter()
        .filter_map(|e| match e {
            PlanEvent::OperationBoundary { pass, .. } => Some(format!("pass {pass}")),
            PlanEvent::FocusOffset { z_mm } => Some(format!("z {z_mm}")),
            _ => None,
        })
        .collect();
    assert_eq!(marks, vec!["pass 0", "pass 1", "z 0.5", "pass 2", "z 1"]);
}

#[test]
fn sub_operations_run_after_the_parent_with_their_own_settings() {
    let mut s = session();
    let g = rectangle(&mut s, 0.0, 0.0, 10.0, 10.0);
    let op = add_op(&mut s, fill_op("Vullen", 1.0, 0.0), vec![g]);
    let sub = SubOperation {
        mode: OperationMode::Line { kerf: burnburn_document::process::Kerf::NONE },
        settings: settings(20.0, 80.0),
    };
    set_advanced(&mut s, op, AdvancedOptions { sub_operations: vec![sub], ..Default::default() });
    let plan = plan(&s, &profile()).unwrap();
    assert_eq!(plan.settings().len(), 2);
    assert!(matches!(plan.settings()[0].mode, PlanMode::Scan { .. }));
    assert_eq!((plan.settings()[1].mode, plan.settings()[1].speed_mm_s), (PlanMode::Line, 20.0));
    let blocks: Vec<(OperationId, usize)> = plan
        .events()
        .iter()
        .filter_map(|e| match e {
            PlanEvent::OperationBoundary { operation, settings, .. } => Some((*operation, settings.get())),
            _ => None,
        })
        .collect();
    assert_eq!(blocks, vec![(op, 0), (op, 1)], "parent first, then its sub-operation, same operation id");
    assert_eq!(motion(plan.events()).iter().filter(|e| e.0 == "cut").count(), 4);
}
