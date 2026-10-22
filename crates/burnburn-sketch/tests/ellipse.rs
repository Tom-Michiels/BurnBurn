//! S01: an ellipse is a sketch entity (centre point, major-axis end point, semi-minor radius).
//! The oracle is analytical: bounding boxes of axis-aligned and rotated ellipses.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test helpers outside #[test] fns

use burnburn_document::sketch::{
    Along, DimensionKind, DimensionRole, Entity, EntityId, Relation, Sketch, SketchEntity, SketchError,
};
use burnburn_geometry::{Path, Point};
use burnburn_sketch::solver::SketchSolverAdapter;
use burnburn_sketch::{Health, evaluate_sketch};
use burnburn_types::CancellationToken;

fn eval(s: &Sketch) -> burnburn_sketch::Evaluation {
    evaluate_sketch(s, &SketchSolverAdapter, &CancellationToken::never()).unwrap()
}

fn bounds(p: &Path) -> (f64, f64, f64, f64) {
    let b = p.bounds().unwrap();
    (b.min().x, b.min().y, b.max().x, b.max().y)
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

/// Ellipse around (10, 5), major end at `major`, semi-minor 2.
fn ellipse_at(major: (f64, f64)) -> (Sketch, EntityId, EntityId, EntityId) {
    let mut s = Sketch::new("e");
    let c = s.point(10.0, 5.0).unwrap();
    let m = s.point(major.0, major.1).unwrap();
    let e = s.ellipse(c, m, 2.0).unwrap();
    (s, c, m, e)
}

#[test]
fn an_axis_aligned_ellipse_is_one_closed_output_with_its_analytical_bounds() {
    let (s, ..) = ellipse_at((14.0, 5.0));
    let ev = eval(&s);
    assert!(ev.is_valid(), "{:?}", ev.health);
    let outputs = ev.solved.unwrap().outputs().unwrap();
    assert_eq!(outputs.len(), 1, "one contour");
    let (x0, y0, x1, y1) = bounds(&outputs[0].1);
    assert!(near(x0, 6.0) && near(x1, 14.0) && near(y0, 3.0) && near(y1, 7.0), "{:?}", (x0, y0, x1, y1));
    assert!(outputs[0].1.subpaths()[0].is_closed());
}

#[test]
fn the_major_axis_end_sets_the_rotation() {
    // Major end straight above the centre: a vertical ellipse of 2 x 8.
    let (s, ..) = ellipse_at((10.0, 9.0));
    let outputs = eval(&s).solved.unwrap().outputs().unwrap();
    let (x0, y0, x1, y1) = bounds(&outputs[0].1);
    assert!(near(x0, 8.0) && near(x1, 12.0) && near(y0, 1.0) && near(y1, 9.0), "{:?}", (x0, y0, x1, y1));
}

#[test]
fn a_construction_ellipse_is_no_output() {
    let (mut s, ..) = ellipse_at((14.0, 5.0));
    let id = s.entities().last().map(|(i, _)| i).unwrap();
    s.set_construction(id, true).unwrap();
    assert!(eval(&s).solved.unwrap().outputs().unwrap().is_empty());
}

#[test]
fn horizontal_on_an_ellipse_levels_its_major_axis() {
    let (mut s, c, m, e) = ellipse_at((13.0, 8.0));
    s.relate(Relation::Fix { point: c, at: Point::new(10.0, 5.0) }).unwrap();
    s.relate(Relation::Horizontal(e)).unwrap();
    let ev = eval(&s);
    assert!(matches!(ev.health, Health::UnderConstrained { .. }), "{:?}", ev.health);
    let solved = ev.solved.unwrap();
    assert!(near(solved.point(m).unwrap().y, 5.0), "{:?}", solved.point(m));
}

#[test]
fn the_semi_major_radius_is_a_driving_dimension() {
    let (mut s, c, m, e) = ellipse_at((13.0, 5.0));
    s.relate(Relation::Fix { point: c, at: Point::new(10.0, 5.0) }).unwrap();
    s.relate(Relation::Horizontal(e)).unwrap();
    s.dimension(DimensionKind::Radius { curve: e }, "6 mm", DimensionRole::Driving).unwrap();
    let ev = eval(&s);
    assert!(ev.is_valid(), "{:?}", ev.health);
    assert!(near(ev.solved.unwrap().point(m).unwrap().x, 16.0));
}

#[test]
fn an_ellipse_is_concentric_with_a_circle() {
    let (mut s, c, _m, e) = ellipse_at((14.0, 5.0));
    let cc = s.point(20.0, 20.0).unwrap();
    let circle = s.circle(cc, 3.0).unwrap();
    s.relate(Relation::Fix { point: cc, at: Point::new(20.0, 20.0) }).unwrap();
    s.relate(Relation::Concentric(e, circle)).unwrap();
    let ev = eval(&s);
    assert!(ev.is_valid(), "{:?}", ev.health);
    let p = ev.solved.unwrap().point(c).unwrap();
    assert!(near(p.x, 20.0) && near(p.y, 20.0), "{p:?}");
}

#[test]
fn an_ellipse_is_never_equal_to_a_circle_and_only_a_line_is_tangent_to_it() {
    // Changed in G7-editor (S01): a point can lie on an ellipse and a line can touch it
    // (`g7_ellipse_relations.rs`); since G8 a circle or another ellipse can touch it too
    // (`g8_ellipse_swap_tangent.rs`); a circle can still not be equal to it.
    let (mut s, _c, m, e) = ellipse_at((14.0, 5.0));
    let q = s.point(0.0, 0.0).unwrap();
    let circle = s.circle(q, 1.0).unwrap();
    assert!(s.relate(Relation::Tangent(circle, e)).is_ok());
    assert!(matches!(s.relate(Relation::Equal(circle, e)), Err(SketchError::WrongEntityKind(_))));
    let line = s.line(q, m).unwrap();
    assert!(s.relate(Relation::Tangent(line, e)).is_ok());
}

#[test]
fn a_zero_or_negative_minor_radius_is_refused() {
    let mut s = Sketch::new("e");
    let c = s.point(0.0, 0.0).unwrap();
    let m = s.point(5.0, 0.0).unwrap();
    for bad in [0.0, -1.0, f64::NAN] {
        let r = s.add_entity(SketchEntity {
            entity: Entity::Ellipse { center: c, major: m, minor: bad },
            construction: false,
        });
        assert_eq!(r, Err(SketchError::InvalidValue), "{bad}");
    }
}

#[test]
fn a_distance_dimension_between_centre_and_major_end_is_the_semi_major_radius() {
    let (mut s, c, m, _e) = ellipse_at((13.0, 5.0));
    s.dimension(DimensionKind::Distance { a: c, b: m, along: Along::Direct }, "7 mm", DimensionRole::Driving).unwrap();
    let ev = eval(&s);
    assert!(ev.is_valid());
    let solved = ev.solved.unwrap();
    let d = solved.point(c).unwrap().distance(solved.point(m).unwrap());
    assert!(near(d, 7.0), "{d}");
}

#[test]
fn a_diameter_dimension_on_an_ellipse_is_the_full_major_axis_and_a_driven_one_measures_it() {
    let (mut s, c, m, e) = ellipse_at((13.0, 5.0));
    s.relate(Relation::Fix { point: c, at: Point::new(10.0, 5.0) }).unwrap();
    s.relate(Relation::Horizontal(e)).unwrap();
    s.dimension(DimensionKind::Diameter { curve: e }, "12 mm", DimensionRole::Driving).unwrap();
    let driven = s.dimension(DimensionKind::Radius { curve: e }, "0 mm", DimensionRole::Driven).unwrap();
    let ev = eval(&s);
    assert!(ev.is_valid(), "{:?}", ev.health);
    assert!(near(ev.solved.as_ref().unwrap().point(m).unwrap().x, 16.0));
    match ev.diagnostics.get(&driven) {
        Some(burnburn_sketch::Diagnosis::Measured(burnburn_sketch::param::Value::Length(l))) => {
            assert!(near(l.mm(), 6.0))
        }
        other => panic!("{other:?}"),
    }
}
