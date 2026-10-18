//! Integration gate B, domain chain without the presentation layer: montageplaat sketch (real
//! solver) → width 150 → JobPlan → preflight → RD bytes → decode → cut geometry in machine mm.
//!
//! Hand-derived expectations: the plate is 150 × 80 after the change; the holes are Ø4 with
//! centres 10 mm from each edge, so at (10, 10), (140, 10), (140, 70), (10, 70). Circles are
//! flattened with 0.001 mm tolerance and RD quantises to 1 µm, so every hole vertex lies on its
//! circle within 0.002 mm.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use burnburn_cam::preflight;
use burnburn_document::CommandPayload;
use burnburn_document::sketch::SketchCommand;
use burnburn_geometry::Point;
use burnburn_sketch::SketchEvaluator;
use burnburn_sketch::solver::SketchSolverAdapter;
use common::*;

const HOLES: [(f64, f64); 4] = [(10.0, 10.0), (140.0, 10.0), (140.0, 70.0), (10.0, 70.0)];
const VERTEX_TOL_MM: f64 = 0.002;

#[test]
fn gate_b_plate_width_150_reaches_the_rd_file_with_holes_in_place() {
    let ev = SketchEvaluator::new(SketchSolverAdapter);
    let mut s = new_session();
    let out = apply(&mut s, CommandPayload::Sketch(SketchCommand::Create { sketch: Box::new(plate()) }), &[&ev]);
    let sketch = out.changes.sketches[0];
    let targets = derived_objects(&s.snapshot());
    assert_eq!(targets.len(), 5, "outline + 4 holes");
    add_cut(&mut s, targets.clone());

    let set = SketchCommand::SetParameter { sketch, name: "width".into(), text: "150 mm".into() };
    apply(&mut s, CommandPayload::Sketch(set), &[&ev]);
    let snap = s.snapshot();
    assert_eq!(derived_objects(&snap), targets, "operation targets survive re-evaluation");

    let plan = compile_plan(&snap);
    let report = preflight(&snap, &plan, &profile());
    assert!(!report.has_errors(), "{:?}", report.findings);
    assert!(report.findings.is_empty(), "every object is cut once, inside the bed: {:?}", report.findings);
    // Preflight size check: the planned outline is the nominal 150 × 80.
    let outline = report.sizes.iter().find(|c| c.nominal_mm.0 > 100.0).expect("outline size check");
    assert!((outline.planned_mm.0 - 150.0).abs() < 1e-9 && (outline.planned_mm.1 - 80.0).abs() < 1e-9, "{outline:?}");

    let rd = decode(&rd_bytes(&plan));
    let (lo, hi) = header_bounds(&rd);
    assert_eq!((lo, hi), (Point::new(0.0, 0.0), Point::new(150.0, 80.0)), "RD header bounds");
    let cuts = cuts(&rd);
    let (clo, chi) = cut_extent(&cuts);
    assert_eq!((clo, chi), (lo, hi), "cut motion spans the header bounds exactly");

    // Every cut vertex is either on the outline or on one hole circle; each hole is one closed
    // loop around its own centre.
    let on_outline = |p: Point| {
        (p.x.abs() < 1e-9 || (p.x - 150.0).abs() < 1e-9 || p.y.abs() < 1e-9 || (p.y - 80.0).abs() < 1e-9)
            && (-1e-9..=150.0 + 1e-9).contains(&p.x)
            && (-1e-9..=80.0 + 1e-9).contains(&p.y)
    };
    let loops = loops(&cuts);
    assert_eq!(loops.len(), 5, "outline + 4 holes as separate closed loops");
    let mut found = Vec::new();
    for l in &loops {
        assert!(l.first().unwrap().distance(*l.last().unwrap()) < 1e-9, "closed loop");
        if l.iter().all(|p| on_outline(*p)) {
            continue;
        }
        let c = centroid(&l[..l.len() - 1]);
        let (k, centre) = HOLES
            .iter()
            .enumerate()
            .map(|(k, h)| (k, Point::new(h.0, h.1)))
            .min_by(|a, b| a.1.distance(c).total_cmp(&b.1.distance(c)))
            .unwrap();
        for p in l {
            let r = p.distance(centre);
            assert!((r - 2.0).abs() <= VERTEX_TOL_MM, "hole {k} vertex {p:?} at r = {r}");
        }
        assert!(l.len() > 16, "a flattened Ø4 circle, not a polygon stand-in: {} vertices", l.len());
        found.push(k);
    }
    found.sort_unstable();
    assert_eq!(found, vec![0, 1, 2, 3], "one loop per hole");
}
