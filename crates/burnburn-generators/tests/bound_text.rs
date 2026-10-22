//! G7-editor E08: text on a path as a live binding. The evaluator bends the text along the guide
//! object in the document; when the guide moves the text moves with it. Oracles are relations
//! between independent renderings: plain text shifted by the start distance on a straight guide,
//! translation invariance, and the radius rule on a circle.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use burnburn_document::text::{PathBinding, TextCommand, TextId};
use burnburn_document::{
    CommandPayload, DerivedOwner, DocumentCommand, DocumentSession, EvalStatus, GeometryCommand, GeometrySource,
};
use burnburn_generators::BoundTextEvaluator;
use burnburn_geometry::{Affine, Path, Point, Vector};
use burnburn_test_support::fixtures::read_fixture;
use burnburn_text::{
    FontData, FontSize, FontStyle, MemoryFontSource, TextEvaluator, TextMode, TextObject, inputs_hash, shape,
};
use burnburn_types::{DocumentId, GeometryId, Length, RequestId, StableId};

fn fonts() -> (MemoryFontSource, burnburn_text::FontRef) {
    let mut src = MemoryFontSource::new();
    let data = Arc::new(FontData::from_bytes(read_fixture("fonts", "LiberationSans-Regular.ttf").unwrap()).unwrap());
    let r = src.insert("Liberation Sans", FontStyle::Regular, data);
    (src, r)
}

fn text(content: &str) -> (MemoryFontSource, TextObject) {
    let (src, r) = fonts();
    (src, TextObject::new(content, r, FontSize::from_mm(Length::from_mm(10.0).unwrap()).unwrap()))
}

fn run(
    s: &mut DocumentSession,
    ev: &BoundTextEvaluator<MemoryFontSource>,
    payload: CommandPayload,
) -> burnburn_document::CommandOutcome {
    let cmd = DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload };
    s.apply_with(cmd, &[ev]).unwrap()
}

fn guide(s: &mut DocumentSession, ev: &BoundTextEvaluator<MemoryFontSource>, path: Path) -> GeometryId {
    run(s, ev, CommandPayload::Edit(GeometryCommand::CreatePath { path, name: "Pad".into() })).changes.created[0]
}

fn bound(guide: GeometryId, start: f64, offset: f64, warp: bool) -> PathBinding {
    PathBinding { guide, start_mm: start, offset_mm: offset, reverse: false, warp }
}

fn create(
    s: &mut DocumentSession,
    ev: &BoundTextEvaluator<MemoryFontSource>,
    mut obj: TextObject,
    binding: Option<PathBinding>,
) -> TextId {
    obj.path = binding;
    run(s, ev, CommandPayload::Text(TextCommand::Create { text: Box::new(obj) })).changes.texts[0]
}

fn outputs(s: &DocumentSession, text: TextId) -> Vec<(GeometryId, Path)> {
    s.snapshot()
        .objects()
        .filter(|(_, o)| matches!(o.source(), GeometrySource::Derived(k) if k.owner == DerivedOwner::Text(text)))
        .map(|(id, o)| (id, (**o.path()).clone()))
        .collect()
}

fn bounds(p: &Path) -> (Point, Point) {
    let b = p.bounds().unwrap();
    (b.min(), b.max())
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

#[test]
fn a_bound_text_sits_on_a_straight_guide_where_plain_text_shifted_would_be() {
    let (src, obj) = text("Hi");
    let plain = bounds(&shape(&obj, &src).unwrap().outlines);
    let ev = BoundTextEvaluator::new(src);
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let g = guide(&mut s, &ev, Path::line(Point::new(0.0, 20.0), Point::new(200.0, 20.0)).unwrap());
    let t = create(&mut s, &ev, obj, Some(bound(g, 8.0, 0.0, true)));
    let out = outputs(&s, t);
    assert_eq!(out.len(), 1, "one outline for the whole bent line");
    let b = bounds(&out[0].1);
    assert!(near(b.0.x, plain.0.x + 8.0) && near(b.1.x, plain.1.x + 8.0), "{b:?} vs {plain:?}");
    assert!(near(b.0.y, plain.0.y + 20.0) && near(b.1.y, plain.1.y + 20.0));
    assert_eq!(s.snapshot().derived_record(DerivedOwner::Text(t)).unwrap().status, EvalStatus::Valid);
}

#[test]
fn the_offset_moves_the_baseline_along_the_left_normal() {
    let (src, obj) = text("Hi");
    let ev = BoundTextEvaluator::new(src);
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let g = guide(&mut s, &ev, Path::line(Point::new(0.0, 0.0), Point::new(200.0, 0.0)).unwrap());
    let a = create(&mut s, &ev, obj.clone(), Some(bound(g, 0.0, 0.0, false)));
    let b = create(&mut s, &ev, obj, Some(bound(g, 0.0, 3.5, false)));
    let (ya, yb) = (bounds(&outputs(&s, a)[0].1).0.y, bounds(&outputs(&s, b)[0].1).0.y);
    assert!(near(yb - ya, 3.5), "{ya} -> {yb}");
}

#[test]
fn moving_the_guide_moves_the_text_by_the_same_vector() {
    let (src, obj) = text("Hallo");
    let ev = BoundTextEvaluator::new(src);
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let g = guide(&mut s, &ev, Path::line(Point::new(0.0, 0.0), Point::new(200.0, 0.0)).unwrap());
    let t = create(&mut s, &ev, obj, Some(bound(g, 5.0, 0.0, true)));
    let before = bounds(&outputs(&s, t)[0].1);
    let shift = Affine::translate(Vector::new(13.0, -7.5)).unwrap();
    run(&mut s, &ev, CommandPayload::Transform { targets: vec![g], transform: shift });
    let after = bounds(&outputs(&s, t)[0].1);
    assert!(near(after.0.x - before.0.x, 13.0) && near(after.0.y - before.0.y, -7.5), "{before:?} -> {after:?}");
    assert!(near(after.1.x - before.1.x, 13.0) && near(after.1.y - before.1.y, -7.5));
    s.undo(s.revision()).unwrap();
    assert_eq!(bounds(&outputs(&s, t)[0].1), before, "one undo takes both back");
}

#[test]
fn text_on_a_circle_follows_the_radius_rule() {
    let (src, obj) = text("HOH");
    let plain = shape(&obj, &src).unwrap().outlines.bounds().unwrap();
    let ev = BoundTextEvaluator::new(src);
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let (centre, r, offset) = (Point::new(100.0, 80.0), 50.0, 1.5);
    let g = guide(&mut s, &ev, Path::circle(centre, r).unwrap());
    let t = create(&mut s, &ev, obj, Some(bound(g, 8.0, offset, true)));
    let tol = burnburn_geometry::ProductionTolerance::new(Length::from_mm(0.001).unwrap()).unwrap();
    let radii: Vec<f64> = outputs(&s, t)[0]
        .1
        .flatten(&tol)
        .iter()
        .flat_map(|p| p.points.iter().map(|q| q.distance(centre)).collect::<Vec<_>>())
        .collect();
    let (lo, hi) = (radii.iter().copied().fold(f64::MAX, f64::min), radii.iter().copied().fold(0.0, f64::max));
    // The evaluator flattens the guide within 0.01 mm: the radii agree to that chord tolerance.
    assert!((hi - (r - offset - plain.min().y)).abs() < 1.2e-2, "{hi}");
    assert!((lo - (r - offset - plain.max().y)).abs() < 1.2e-2, "{lo}");
}

#[test]
fn a_text_without_a_binding_is_evaluated_exactly_as_before() {
    let (src, obj) = text("Naam");
    let (src2, _) = text("Naam");
    let plain = TextEvaluator::new(src2);
    let ev = BoundTextEvaluator::new(src);
    let mut a = DocumentSession::new(DocumentId::from_raw(1));
    let mut b = DocumentSession::new(DocumentId::from_raw(1));
    let cmd = |s: &DocumentSession| DocumentCommand {
        document: s.id(),
        expected: s.revision(),
        request: RequestId::from_raw(1),
        payload: CommandPayload::Text(TextCommand::Create { text: Box::new(obj.clone()) }),
    };
    let ca = cmd(&a);
    a.apply_with(ca, &[&plain]).unwrap();
    let cb = cmd(&b);
    b.apply_with(cb, &[&ev]).unwrap();
    let id = a.snapshot().text_layer().texts().next().unwrap().0;
    assert_eq!(outputs(&a, id), outputs(&b, id));
    assert_eq!(
        a.snapshot().derived_record(DerivedOwner::Text(id)),
        b.snapshot().derived_record(DerivedOwner::Text(id))
    );
}

#[test]
fn a_guide_that_is_too_short_fails_the_text_and_keeps_its_last_outline() {
    let (src, obj) = text("Een lange tekst");
    let ev = BoundTextEvaluator::new(src);
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let g = guide(&mut s, &ev, Path::line(Point::new(0.0, 0.0), Point::new(300.0, 0.0)).unwrap());
    let t = create(&mut s, &ev, obj, Some(bound(g, 0.0, 0.0, false)));
    let (id, last) = outputs(&s, t).remove(0);
    // The guide is replaced by a stub too short for the text.
    let short = Path::line(Point::new(0.0, 0.0), Point::new(5.0, 0.0)).unwrap();
    let edit = burnburn_geometry::NodeEdit::MoveNode {
        node: burnburn_geometry::NodeRef { subpath: 0, node: 1 },
        to: Point::new(5.0, 0.0),
    };
    let _ = short;
    run(&mut s, &ev, CommandPayload::Edit(GeometryCommand::EditNodes { target: g, edit }));
    let record = s.snapshot().derived_record(DerivedOwner::Text(t)).unwrap().clone();
    assert!(matches!(record.status, EvalStatus::Failed { .. }), "{:?}", record.status);
    assert_eq!(outputs(&s, t)[0], (id, last), "the last valid outline stays");
    assert!(s.snapshot().production_blocked(id));
}

#[test]
fn several_lines_and_a_missing_font_are_failures_not_panics() {
    let (src, obj) = text("een\ntwee");
    let ev = BoundTextEvaluator::new(src);
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let g = guide(&mut s, &ev, Path::line(Point::new(0.0, 0.0), Point::new(300.0, 0.0)).unwrap());
    let t = create(&mut s, &ev, obj, Some(bound(g, 0.0, 0.0, false)));
    assert!(matches!(s.snapshot().derived_record(DerivedOwner::Text(t)).unwrap().status, EvalStatus::Failed { .. }));
    assert!(outputs(&s, t).is_empty(), "nothing was ever valid");

    let (src, mut obj) = text("x");
    obj.font = burnburn_text::FontRef::new("Bestaat Niet", FontStyle::Regular);
    let ev = BoundTextEvaluator::new(src);
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let g = guide(&mut s, &ev, Path::line(Point::new(0.0, 0.0), Point::new(300.0, 0.0)).unwrap());
    let t = create(&mut s, &ev, obj, Some(bound(g, 0.0, 0.0, false)));
    assert!(matches!(
        s.snapshot().derived_record(DerivedOwner::Text(t)).unwrap().status,
        EvalStatus::Failed { diagnosis: burnburn_document::EvalDiagnosis::FontMissing { .. } }
    ));
}

#[test]
fn single_line_text_on_a_path_stays_open_strokes() {
    let (src, mut obj) = text("AB 12");
    obj.mode = TextMode::SingleLine;
    let ev = BoundTextEvaluator::new(src);
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let g = guide(&mut s, &ev, Path::circle(Point::ORIGIN, 60.0).unwrap());
    let t = create(&mut s, &ev, obj, Some(bound(g, 0.0, 0.0, true)));
    let out = outputs(&s, t);
    assert_eq!(out.len(), 1);
    assert!(out[0].1.subpaths().iter().all(|sp| !sp.is_closed()), "strokes, no fill");
    assert!(!out[0].1.subpaths().is_empty());
    // And they lie on the circle, not where the plain text would be.
    let tol = burnburn_geometry::ProductionTolerance::new(Length::from_mm(0.01).unwrap()).unwrap();
    let radii: Vec<f64> =
        out[0].1.flatten(&tol).iter().flat_map(|p| p.points.iter().map(|q| q.distance(Point::ORIGIN))).collect();
    assert!(radii.iter().all(|r| *r > 45.0 && *r < 62.0), "{radii:?}");
}

#[test]
fn the_evaluation_record_sees_the_guide() {
    let (src, obj) = text("Hi");
    let own = inputs_hash(&obj, &src);
    let ev = BoundTextEvaluator::new(src);
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let g = guide(&mut s, &ev, Path::line(Point::new(0.0, 0.0), Point::new(200.0, 0.0)).unwrap());
    let t = create(&mut s, &ev, obj, Some(bound(g, 0.0, 0.0, true)));
    let first = s.snapshot().derived_record(DerivedOwner::Text(t)).unwrap().inputs;
    assert!(first.is_some() && first != own, "bound text hashes differently from the same text unbound");
    run(
        &mut s,
        &ev,
        CommandPayload::Transform { targets: vec![g], transform: Affine::translate(Vector::new(1.0, 0.0)).unwrap() },
    );
    let second = s.snapshot().derived_record(DerivedOwner::Text(t)).unwrap().inputs;
    assert_ne!(first, second, "the guide's geometry is part of the inputs");
}
