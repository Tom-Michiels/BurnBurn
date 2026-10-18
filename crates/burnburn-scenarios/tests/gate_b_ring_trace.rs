//! Integration gate B: the corpus ring (`test-data/trace/synthetic/ring.png`) traced at its
//! declared 0.1 mm/px → document import → JobPlan → RD → decode. The hole must survive the
//! whole chain as its own closed cut loop.
//!
//! Hand-derived expectation from the corpus vector reference (`synthetic/ring.json`): a ring
//! centred at (32, 32) px with outer radius 24 px and inner radius 12 px on a 64 px image, so at
//! 0.1 mm/px (y up) the centre is (3.2, 3.2) mm, r_outer = 2.4 mm, r_inner = 1.2 mm. Tracing is
//! an approximation (corpus criterion ≤ 0.05 mm), so radii are checked within 0.06 mm.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::sync::Arc;

use burnburn_cam::preflight;
use burnburn_document::{CommandPayload, GeometryCommand, ImportItem};
use burnburn_geometry::Point;
use burnburn_image::{DecodeLimits, ImageRecipe, PixelScale, Threshold, decode as decode_png};
use burnburn_test_support::fixtures::read_fixture;
use burnburn_trace::{TraceRecipe, TraceRequest, trace};
use burnburn_types::{CancellationToken, ContentHash, RevisionId, StableId, TaskId};
use common::*;

const CENTRE: (f64, f64) = (3.2, 3.2);
const R_OUTER: f64 = 2.4;
const R_INNER: f64 = 1.2;
const RADIUS_TOL: f64 = 0.06;

#[test]
fn gate_b_ring_trace_keeps_its_hole_in_the_rd_file() {
    // The reference numbers above are re-read from the corpus so a corpus change cannot drift silently.
    let reference: serde_json::Value =
        serde_json::from_slice(&read_fixture("trace", "synthetic/ring.json").unwrap()).unwrap();
    let ring = &reference["union_of"][0]["rings"];
    assert_eq!((ring[0]["circle"]["r"].as_f64(), ring[1]["circle"]["r"].as_f64()), (Some(24.0), Some(12.0)));

    let png = read_fixture("trace", "synthetic/ring.png").unwrap();
    let image = decode_png(&png, DecodeLimits::default()).unwrap();
    let req = TraceRequest {
        task: TaskId::from_raw(1),
        source_revision: RevisionId::INITIAL,
        source: Arc::new(image),
        recipe: TraceRecipe {
            image: ImageRecipe {
                scale: Some(PixelScale::uniform_mm_per_pixel(0.1).unwrap()),
                threshold: Threshold::Otsu,
                ..ImageRecipe::default()
            },
            ..TraceRecipe::default()
        },
    };
    let result = trace(&req, &CancellationToken::never()).unwrap();
    assert_eq!(result.components.len(), 1);
    let path = result.component_path(0).unwrap();
    assert_eq!(path.subpaths().len(), 2, "outer contour and hole");

    let mut s = new_session();
    let items = vec![ImportItem { name: "Ring".into(), entity: Some("trace/component/0".into()), path }];
    let asset = ContentHash::from_bytes([0x21; 32]);
    let ring = apply(&mut s, CommandPayload::Edit(GeometryCommand::Import { asset, items }), &[]).changes.created[0];
    add_cut(&mut s, vec![ring]);
    let snap = s.snapshot();
    let plan = compile_plan(&snap);
    assert!(!preflight(&snap, &plan, &profile()).has_errors());

    let rd = decode(&rd_bytes(&plan));
    let loops = loops(&cuts(&rd));
    assert_eq!(loops.len(), 2, "outer and hole are two closed cut loops");
    let centre = Point::new(CENTRE.0, CENTRE.1);
    let mut radii: Vec<(f64, f64)> = loops
        .iter()
        .map(|l| {
            assert!(l.first().unwrap().distance(*l.last().unwrap()) < 1e-9, "closed loop");
            let r: Vec<f64> = l.iter().map(|p| p.distance(centre)).collect();
            (r.iter().copied().fold(f64::INFINITY, f64::min), r.iter().copied().fold(0.0, f64::max))
        })
        .collect();
    radii.sort_by(|a, b| a.0.total_cmp(&b.0));
    let (hole, outer) = (radii[0], radii[1]);
    assert!((hole.0 - R_INNER).abs() <= RADIUS_TOL && (hole.1 - R_INNER).abs() <= RADIUS_TOL, "hole radii {hole:?}");
    assert!(
        (outer.0 - R_OUTER).abs() <= RADIUS_TOL && (outer.1 - R_OUTER).abs() <= RADIUS_TOL,
        "outer radii {outer:?}"
    );
}
