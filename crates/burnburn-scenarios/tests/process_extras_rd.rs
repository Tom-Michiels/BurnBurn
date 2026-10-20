//! W1-L3: the line and fill extras reach the Ruida file exactly as the preview shows them.
//! Plan → preview and plan → RD → decode are two readings of one plan; their cut/scan lengths
//! must agree, and extras without a proven RD form must stay typed lowering errors.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use burnburn_document::process::{
    AdvancedOptions, FillGrouping, FillSettings, Lead, LeadStyle, LineOptions, OperationDraft, OperationMode, Overcut,
    Passes, Perforation, ProcessCommand, ScanAxis, ScanDirection, TabSpec,
};
use burnburn_document::{CommandPayload, GeometryCommand};
use burnburn_geometry::{Path, PathBuilder, Point, Rect};
use burnburn_job::{Feature, JobPlan, MachineCapabilities, MachineProfile, Support, preview};
use burnburn_ruida::{LowerError, lower};
use burnburn_types::{CancellationToken, GeometryId, OperationId};
use common::*;

fn supported(features: &[Feature]) -> MachineProfile {
    let caps = features.iter().fold(MachineCapabilities::conservative(), |c, f| c.with(*f, Support::Supported));
    profile().with_capabilities(caps).unwrap()
}

fn compile_for(s: &burnburn_document::DocumentSession, p: &MachineProfile) -> JobPlan {
    burnburn_cam::compile(&s.snapshot(), p, &options(), &CancellationToken::never()).unwrap()
}

fn square(s: &mut burnburn_document::DocumentSession, x: f64, w: f64) -> GeometryId {
    // 10 mm up and right from the bed corner: leads on the waste side stay at positive coordinates.
    let rect = Rect::new(Point::new(x, 10.0), mm(w), mm(w)).unwrap();
    apply(s, CommandPayload::CreateRectangle { rect, name: String::new() }, &[]).changes.created[0]
}

fn set_line(s: &mut burnburn_document::DocumentSession, op: OperationId, line: LineOptions) {
    let advanced = AdvancedOptions { line, ..Default::default() };
    apply(s, CommandPayload::Process(ProcessCommand::SetAdvanced { operation: op, advanced }), &[]);
}

fn add_line(s: &mut burnburn_document::DocumentSession, g: GeometryId, passes: u32) -> OperationId {
    let mut draft = line_cut("Snijden");
    draft.settings.passes = Passes::new(passes).unwrap();
    apply(s, CommandPayload::Process(ProcessCommand::AddOperation { draft, targets: vec![g] }), &[]).changes.operations
        [0]
}

fn total(cuts: &[Cut]) -> f64 {
    cuts.iter().map(|c| c.from.distance(c.to)).sum()
}

#[test]
fn line_extras_cut_the_same_length_in_the_preview_and_in_the_decoded_rd() {
    // 20 × 20 square (perimeter 80), two passes. Tabs every 20 mm (4 tabs) of 2 mm remove 8 mm
    // per pass; the 3 mm overcut is cut on the last pass only: 2 · 72 + 3 = 147 mm.
    let mut s = new_session();
    let g = square(&mut s, 10.0, 20.0);
    let op = add_line(&mut s, g, 2);
    set_line(
        &mut s,
        op,
        LineOptions {
            tabs: Some(TabSpec::spaced(mm(20.0), mm(2.0)).unwrap()),
            overcut: Some(Overcut::new(mm(3.0)).unwrap()),
            ..Default::default()
        },
    );
    let plan = compile_for(&s, &profile());
    let pv = preview(&plan, &profile());
    assert!((pv.cut_length_mm - 147.0).abs() < 1e-6, "{}", pv.cut_length_mm);
    let file = decode(&rd_bytes(&plan));
    let rd = total(&cuts(&file));
    assert!((rd - 147.0).abs() < 0.01, "decoded RD cuts {rd} mm, preview {} mm", pv.cut_length_mm);
}

#[test]
fn leads_perforation_and_arc_leads_agree_between_preview_and_rd() {
    // Perforation 3 on / 2 off along the 40 mm perimeter of a 10 × 10 square: 24 mm of cut.
    let mut s = new_session();
    let g = square(&mut s, 10.0, 10.0);
    let op = add_line(&mut s, g, 1);
    set_line(
        &mut s,
        op,
        LineOptions { perforation: Some(Perforation::new(mm(3.0), mm(2.0)).unwrap()), ..Default::default() },
    );
    let plan = compile_for(&s, &profile());
    let rd = total(&cuts(&decode(&rd_bytes(&plan))));
    assert!((rd - 24.0).abs() < 0.01, "{rd}");
    assert!((preview(&plan, &profile()).cut_length_mm - rd).abs() < 0.01);

    // Tilted line lead-in (2 mm, 45°) and arc lead-out (radius 2): both are ordinary cuts, so
    // the RD carries them: 40 mm contour + 2 mm lead + the quarter circle (π).
    let mut t = new_session();
    let g = square(&mut t, 10.0, 10.0);
    let op = add_line(&mut t, g, 1);
    let tilted = Lead::new(mm(2.0)).unwrap().with_angle(45.0).unwrap();
    let arc = Lead::new(mm(2.0)).unwrap().with_style(LeadStyle::Arc);
    set_line(&mut t, op, LineOptions { lead_in: Some(tilted), lead_out: Some(arc), ..Default::default() });
    let plan = compile_for(&t, &profile());
    let pv = preview(&plan, &profile());
    let rd = total(&cuts(&decode(&rd_bytes(&plan))));
    let arc_len = std::f64::consts::PI;
    assert!((rd - pv.cut_length_mm).abs() < 0.05, "RD {rd} vs preview {}", pv.cut_length_mm);
    // The flattened arc is slightly shorter than the exact quarter circle (chord error ≤ 0.001).
    assert!((rd - (40.0 + 2.0 + arc_len)).abs() < 0.02, "{rd}");
}

/// Two disjoint 10 × 10 squares at x = 0 and x = 20 as one geometry.
fn two_squares() -> Path {
    let square = |b: PathBuilder, x: f64| {
        b.move_to(Point::new(x, 0.0))
            .line_to(Point::new(x + 10.0, 0.0))
            .line_to(Point::new(x + 10.0, 10.0))
            .line_to(Point::new(x, 10.0))
            .close()
    };
    square(square(PathBuilder::new(), 0.0), 20.0).build().unwrap()
}

fn fill_draft(f: FillSettings) -> OperationDraft {
    OperationDraft { mode: OperationMode::Fill(f), ..line_cut("Vullen") }
}

#[test]
fn flood_fill_order_and_grouping_reach_the_rd_scan_rows() {
    let fill = FillSettings::new(mm(2.5), ScanAxis::X, ScanDirection::Bidirectional, mm(0.0)).unwrap();
    let build = |f: FillSettings| {
        let mut s = new_session();
        let path = two_squares();
        let g = apply(&mut s, CommandPayload::Edit(GeometryCommand::CreatePath { path, name: String::new() }), &[])
            .changes
            .created[0];
        apply(
            &mut s,
            CommandPayload::Process(ProcessCommand::AddOperation { draft: fill_draft(f), targets: vec![g] }),
            &[],
        );
        compile_for(&s, &profile())
    };
    let scan_cuts = |plan: &JobPlan| cuts(&decode(&rd_bytes(plan)));
    let normal = scan_cuts(&build(fill));
    let flood = scan_cuts(&build(fill.with_flood_fill(true)));
    assert_eq!((normal.len(), flood.len()), (8, 8));
    let in_left = |c: &Cut| c.from.x <= 10.001 && c.to.x <= 10.001;
    assert!(!normal[..4].iter().all(in_left), "row by row alternates between the squares");
    assert!(flood[..4].iter().all(in_left), "flood: the left square first, in the file as well");
    assert!((total(&normal) - total(&flood)).abs() < 0.01, "the same 80 mm are burned");

    // Individual grouping of one geometry changes nothing for a single shape set.
    let individual = scan_cuts(&build(fill.with_grouping(FillGrouping::Individual)));
    assert_eq!(individual.len(), 8);
}

#[test]
fn extras_without_a_proven_rd_form_are_typed_lowering_errors() {
    let unsupported = |plan: &JobPlan| lower(plan, &RUIDA).unwrap_err();

    // Cross-hatch needs a vertical scan block, which the Ruida lowering does not claim yet.
    let mut s = new_session();
    let g = square(&mut s, 0.0, 10.0);
    let fill = FillSettings::new(mm(1.0), ScanAxis::X, ScanDirection::Bidirectional, mm(0.0)).unwrap();
    apply(
        &mut s,
        CommandPayload::Process(ProcessCommand::AddOperation {
            draft: fill_draft(fill.with_cross_hatch(true)),
            targets: vec![g],
        }),
        &[],
    );
    assert!(matches!(unsupported(&compile_for(&s, &profile())), LowerError::Unsupported { what: "scan axis Y", .. }));

    // A ramped run has no RD form.
    let mut t = new_session();
    let g = square(&mut t, 0.0, 10.0);
    apply(
        &mut t,
        CommandPayload::Process(ProcessCommand::AddOperation {
            draft: fill_draft(fill.with_ramp(mm(1.0)).unwrap()),
            targets: vec![g],
        }),
        &[],
    );
    let plan = compile_for(&t, &supported(&[Feature::RampLength]));
    assert!(matches!(unsupported(&plan), LowerError::Unsupported { what: "ramp length", .. }));

    // Neither has a dot pulse.
    let mut u = new_session();
    let g = square(&mut u, 0.0, 10.0);
    let op = add_line(&mut u, g, 1);
    let dot = burnburn_document::process::DotMode::new(mm(5.0), 2.0).unwrap();
    set_line(&mut u, op, LineOptions { dot: Some(dot), ..Default::default() });
    let plan = compile_for(&u, &supported(&[Feature::DotMode]));
    assert!(matches!(unsupported(&plan), LowerError::Unsupported { what: "dot mode", .. }));
}
