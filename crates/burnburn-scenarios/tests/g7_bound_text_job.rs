//! G7-editor E08, the production side of a text bound to a guide: the bent outline is exactly what
//! reaches the RD cut, and a compiled job is frozen. Editing the guide afterwards makes a new
//! revision; the job compiled from the earlier snapshot stays byte-identical and still describes
//! the old position, while compiling the new snapshot gives the moved text.
//!
//! Three independent extents are compared, as in TX01: the bent outline from `text_on_path`, the
//! document's derived object, and the cut motion decoded from the RD file.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test code

mod common;

use std::sync::Arc;

use burnburn_document::text::{PathBinding, TextCommand, TextObject};
use burnburn_document::{CommandPayload, DerivedOwner, DocumentSession, GeometryCommand, GeometrySource};
use burnburn_generators::{BoundTextEvaluator, Distort, PathTextParams, text_on_path};
use burnburn_geometry::{Affine, Path, Point, ProductionTolerance, Vector};
use burnburn_test_support::fixtures::read_fixture;
use burnburn_text::{FontData, FontSize, FontStyle, MemoryFontSource};
use burnburn_types::{GeometryId, Length};
use common::*;

fn fonts() -> (MemoryFontSource, burnburn_text::FontRef) {
    let mut src = MemoryFontSource::new();
    let data = Arc::new(FontData::from_bytes(read_fixture("fonts", "LiberationSans-Regular.ttf").unwrap()).unwrap());
    let r = src.insert("Liberation Sans", FontStyle::Regular, data);
    (src, r)
}

fn bounds_xy(p: &Path) -> (Point, Point) {
    let b = p.bounds().unwrap();
    (b.min(), b.max())
}

fn same(a: (Point, Point), b: (Point, Point), tol: f64) -> bool {
    a.0.distance(b.0) <= tol && a.1.distance(b.1) <= tol
}

struct Job {
    session: DocumentSession,
    guide: GeometryId,
    output: GeometryId,
    text: TextObject,
}

/// A text on a line guide (y = 60, from x = 20), with a cut on the text outline.
fn job() -> (Job, BoundTextEvaluator<MemoryFontSource>) {
    let (src, font) = fonts();
    let mut text = TextObject::new("Naam 7", font, FontSize::from_mm(Length::from_mm(10.0).unwrap()).unwrap());
    let ev = BoundTextEvaluator::new(src);
    let mut s = new_session();
    let guide_path = Path::line(Point::new(20.0, 60.0), Point::new(220.0, 60.0)).unwrap();
    let guide = apply(
        &mut s,
        CommandPayload::Edit(GeometryCommand::CreatePath { path: guide_path, name: "Pad".into() }),
        &[&ev],
    )
    .changes
    .created[0];
    text.path = Some(PathBinding { guide, start_mm: 0.0, offset_mm: 0.0, reverse: false, warp: true });
    apply(&mut s, CommandPayload::Text(TextCommand::Create { text: Box::new(text.clone()) }), &[&ev]);
    let output = s
        .snapshot()
        .objects()
        .find(|(_, o)| matches!(o.source(), GeometrySource::Derived(k) if matches!(k.owner, DerivedOwner::Text(_))))
        .map(|(id, _)| id)
        .expect("the bound text has an outline");
    add_cut(&mut s, vec![output]);
    (Job { session: s, guide, output, text }, ev)
}

fn rd_cut_extent(snapshot: &burnburn_document::DocumentSnapshot) -> (Point, Point) {
    cut_extent(&cuts(&decode(&rd_bytes(&compile_plan(snapshot)))))
}

#[test]
fn the_cut_is_the_bent_outline_in_preview_document_and_rd() {
    let (job, _) = job();
    let (src, _) = fonts();
    let tolerance = ProductionTolerance::new(Length::from_mm(0.01).unwrap()).unwrap();
    let params = PathTextParams { distort: Distort::Warp, start: 0.0, offset: 0.0, reverse: false, tolerance };
    let guide = Path::line(Point::new(20.0, 60.0), Point::new(220.0, 60.0)).unwrap();
    let bent = text_on_path(&job.text, &src, &guide, &params).unwrap().path;
    let snap = job.session.snapshot();
    let document = snap.object(job.output).unwrap().bounds().unwrap();
    let preview = bounds_xy(&bent);
    assert!(same(preview, (document.min(), document.max()), 1e-9), "{preview:?} vs {document:?}");
    // The RD flattens curves within 0.001 mm and rounds to micrometres.
    assert!(same(preview, rd_cut_extent(&snap), 0.004), "{preview:?} vs the RD");
}

#[test]
fn a_compiled_job_is_frozen_while_the_guide_moves_on() {
    let (mut job, ev) = job();
    let frozen_snapshot = job.session.snapshot();
    let frozen_rd = rd_bytes(&compile_plan(&frozen_snapshot));
    let frozen_extent = rd_cut_extent(&frozen_snapshot);

    // The guide moves 40 mm right and 15 mm up: a new revision, the text follows it.
    let shift = Affine::translate(Vector::new(40.0, 15.0)).unwrap();
    apply(&mut job.session, CommandPayload::Transform { targets: vec![job.guide], transform: shift }, &[&ev]);
    assert!(job.session.revision() > frozen_snapshot.revision());

    // The job compiled earlier is untouched: same bytes, same place, however often it is made again.
    assert_eq!(rd_bytes(&compile_plan(&frozen_snapshot)), frozen_rd);
    assert!(same(rd_cut_extent(&frozen_snapshot), frozen_extent, 0.0));

    // The new revision compiles to the moved text.
    let now = job.session.snapshot();
    let moved = rd_cut_extent(&now);
    let expect = (
        Point::new(frozen_extent.0.x + 40.0, frozen_extent.0.y + 15.0),
        Point::new(frozen_extent.1.x + 40.0, frozen_extent.1.y + 15.0),
    );
    assert!(same(moved, expect, 0.004), "{moved:?} vs {expect:?}");
    assert_ne!(rd_bytes(&compile_plan(&now)), frozen_rd);
}

#[test]
fn a_text_that_lost_its_guide_is_flagged_blocked_and_the_old_job_is_what_it_was() {
    let (mut job, ev) = job();
    let before = job.session.snapshot();
    let old_rd = rd_bytes(&compile_plan(&before));
    apply(&mut job.session, CommandPayload::Delete { targets: vec![job.guide] }, &[&ev]);
    let after = job.session.snapshot();
    assert!(after.production_blocked(job.output), "the outline is stale: production is blocked");
    // Known gap, reported to the CAM lane in the integration queue: `compile` and `preflight` do not
    // look at `production_blocked` yet, so the stale outline would still be cut. Until they refuse
    // it, the guard is the flag the document gives and the toolbar shows.
    assert_eq!(rd_bytes(&compile_plan(&before)), old_rd, "the job of the earlier revision is what it was");
}
