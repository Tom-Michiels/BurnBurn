//! Tessellation of packet primitives into coloured triangles, with `lyon` (ADR 0003).
//!
//! Fills are general paths (concave contours and holes, even-odd rule); strokes have square caps
//! and mitred joins, so closed outlines have no gaps and translucent strokes do not double-blend
//! at corners. Positions are viewport-relative device pixels; only finite input is tessellated.

use lyon_tessellation::geometry_builder::{BuffersBuilder, VertexBuffers};
use lyon_tessellation::math::point;
use lyon_tessellation::path::Path as LyonPath;
use lyon_tessellation::{
    FillOptions, FillTessellator, FillVertex, LineCap, LineJoin, StrokeOptions, StrokeTessellator, StrokeVertex,
};

use crate::input::Rgba;
use crate::packet::{Primitive, RenderPacket};

/// One vertex: device-pixel position (viewport-relative, y down) and straight-alpha colour.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct Vertex {
    pub position_px: [f32; 2],
    pub color: [f32; 4],
}

/// Curve flattening tolerance inside lyon, in pixels. Our input is already polylines, so this only
/// affects round joins/caps (none today).
const TOLERANCE_PX: f32 = 0.05;

/// Triangles (three vertices each) for every primitive, in painting order.
pub fn tessellate(packet: &RenderPacket) -> Vec<Vertex> {
    let mut out = Vec::new();
    let mut fill = FillTessellator::new();
    let mut stroke = StrokeTessellator::new();
    for primitive in &packet.primitives {
        match primitive {
            Primitive::Fill { contours_px, color } => {
                let contours: Vec<Vec<[f32; 2]>> =
                    contours_px.iter().map(|c| dedup(c)).filter(|c| c.len() >= 3).collect();
                if contours.is_empty() || contours.iter().flatten().any(|p| !finite(*p)) {
                    continue;
                }
                let path = lyon_path(&contours, true);
                let mut buffers: VertexBuffers<[f32; 2], u32> = VertexBuffers::new();
                let options = FillOptions::even_odd().with_tolerance(TOLERANCE_PX);
                let ok = fill
                    .tessellate_path(
                        &path,
                        &options,
                        &mut BuffersBuilder::new(&mut buffers, |v: FillVertex| v.position().to_array()),
                    )
                    .is_ok();
                if ok {
                    push(&mut out, &buffers, *color);
                }
            }
            Primitive::Stroke { points_px, closed, width_px, color } => {
                if !(width_px.is_finite() && *width_px > 0.0) || points_px.iter().any(|p| !finite(*p)) {
                    continue;
                }
                let points = dedup(points_px);
                if points.len() < 2 {
                    continue;
                }
                let path = lyon_path(&[points], *closed);
                let mut buffers: VertexBuffers<[f32; 2], u32> = VertexBuffers::new();
                let options = StrokeOptions::default()
                    .with_line_width(*width_px)
                    .with_line_cap(LineCap::Square)
                    .with_line_join(LineJoin::Miter)
                    .with_tolerance(TOLERANCE_PX);
                let ok = stroke
                    .tessellate_path(
                        &path,
                        &options,
                        &mut BuffersBuilder::new(&mut buffers, |v: StrokeVertex| v.position().to_array()),
                    )
                    .is_ok();
                if ok {
                    push(&mut out, &buffers, *color);
                }
            }
        }
    }
    out
}

fn finite(p: [f32; 2]) -> bool {
    p[0].is_finite() && p[1].is_finite()
}

/// Drops consecutive repeated points (and a closing repeat of the first point).
fn dedup(points: &[[f32; 2]]) -> Vec<[f32; 2]> {
    let mut out: Vec<[f32; 2]> = Vec::with_capacity(points.len());
    for p in points {
        if out.last() != Some(p) {
            out.push(*p);
        }
    }
    if out.len() > 1 && out.first() == out.last() {
        out.pop();
    }
    out
}

fn lyon_path(contours: &[Vec<[f32; 2]>], closed: bool) -> LyonPath {
    let mut builder = LyonPath::builder();
    for contour in contours {
        let mut points = contour.iter();
        let Some(first) = points.next() else { continue };
        builder.begin(point(first[0], first[1]));
        for p in points {
            builder.line_to(point(p[0], p[1]));
        }
        builder.end(closed);
    }
    builder.build()
}

/// Expands indexed triangles into the flat vertex list the pipeline draws.
fn push(out: &mut Vec<Vertex>, buffers: &VertexBuffers<[f32; 2], u32>, color: Rgba) {
    let color = [color.r, color.g, color.b, color.a];
    for index in &buffers.indices {
        if let Some(position_px) = buffers.vertices.get(*index as usize) {
            out.push(Vertex { position_px: *position_px, color });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RED: Rgba = Rgba::new(1.0, 0.0, 0.0, 1.0);

    fn packet(primitives: Vec<Primitive>) -> RenderPacket {
        RenderPacket { size_px: [100, 100], clear: Rgba::TRANSPARENT, primitives }
    }

    /// Absolute triangle areas summed: the covered area when triangles do not overlap.
    fn area(v: &[Vertex]) -> f32 {
        v.chunks(3)
            .map(|t| {
                let [a, b, c] = [t[0].position_px, t[1].position_px, t[2].position_px];
                (((b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1])) / 2.0).abs()
            })
            .sum()
    }

    fn within(v: &[Vertex], x: (f32, f32), y: (f32, f32)) -> bool {
        v.iter().all(|p| (x.0..=x.1).contains(&p.position_px[0]) && (y.0..=y.1).contains(&p.position_px[1]))
    }

    fn fill(contours_px: Vec<Vec<[f32; 2]>>) -> Primitive {
        Primitive::Fill { contours_px, color: RED }
    }

    #[test]
    fn fill_rect_triangles_cover_exactly_its_area() {
        let v = tessellate(&packet(vec![fill(vec![vec![[10.0, 10.0], [30.0, 10.0], [30.0, 20.0], [10.0, 20.0]]])]));
        assert_eq!(v.len() % 3, 0);
        assert!(!v.is_empty());
        assert_eq!(area(&v), 200.0);
        assert!(within(&v, (10.0, 30.0), (10.0, 20.0)));
        assert!(v.iter().all(|p| p.color == [1.0, 0.0, 0.0, 1.0]));
    }

    /// A concave L (general path, not a rectangle): 20×20 minus a 10×10 corner = 300 px².
    #[test]
    fn concave_fill_covers_exact_area() {
        let l = vec![[0.0, 0.0], [20.0, 0.0], [20.0, 10.0], [10.0, 10.0], [10.0, 20.0], [0.0, 20.0]];
        let v = tessellate(&packet(vec![fill(vec![l])]));
        assert_eq!(area(&v), 300.0);
        assert!(within(&v, (0.0, 20.0), (0.0, 20.0)));
    }

    /// A ring: the inner contour is a hole (even-odd), whatever its orientation.
    #[test]
    fn inner_contour_is_a_hole() {
        let outer = vec![[0.0, 0.0], [20.0, 0.0], [20.0, 20.0], [0.0, 20.0]];
        let same = vec![[5.0, 5.0], [15.0, 5.0], [15.0, 15.0], [5.0, 15.0]];
        let reversed: Vec<_> = same.iter().rev().copied().collect();
        for hole in [same, reversed] {
            let v = tessellate(&packet(vec![fill(vec![outer.clone(), hole])]));
            assert_eq!(area(&v), 300.0);
            // No triangle has its centroid inside the hole.
            assert!(v.chunks(3).all(|t| {
                let cx = (t[0].position_px[0] + t[1].position_px[0] + t[2].position_px[0]) / 3.0;
                let cy = (t[0].position_px[1] + t[1].position_px[1] + t[2].position_px[1]) / 3.0;
                !((5.0..15.0).contains(&cx) && (5.0..15.0).contains(&cy))
            }));
        }
    }

    /// A 1 px horizontal line on y = 40.5 covers exactly y 40..41 and x 4.5..60.5 (square caps).
    #[test]
    fn stroke_segment_is_line_width_with_square_caps() {
        let stroke =
            Primitive::Stroke { points_px: vec![[5.0, 40.5], [60.0, 40.5]], closed: false, width_px: 1.0, color: RED };
        let v = tessellate(&packet(vec![stroke]));
        assert_eq!(area(&v), 56.0);
        assert!(within(&v, (4.5, 60.5), (40.0, 41.0)));
    }

    /// A closed 10×10 square stroked 2 px wide with mitred corners covers 12² − 8² = 80 px²; open,
    /// the missing closing side and its corners make it smaller.
    #[test]
    fn closed_stroke_has_mitred_corners_and_closing_side() {
        let square = vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]];
        let closed = Primitive::Stroke { points_px: square.clone(), closed: true, width_px: 2.0, color: RED };
        let v = tessellate(&packet(vec![closed]));
        assert!((area(&v) - 80.0).abs() < 1e-3, "{}", area(&v));
        assert!(within(&v, (-1.0, 11.0), (-1.0, 11.0)));
        let open = Primitive::Stroke { points_px: square, closed: false, width_px: 2.0, color: RED };
        assert!(area(&tessellate(&packet(vec![open]))) < 80.0);
    }

    /// Zero-length segments, non-finite points and non-positive widths produce no triangles; so do
    /// fills without area or with non-finite points.
    #[test]
    fn degenerate_primitives_produce_nothing() {
        for (points_px, width_px) in [
            (vec![[5.0, 5.0], [5.0, 5.0]], 1.0),
            (vec![[5.0, 5.0], [f32::NAN, 5.0]], 1.0),
            (vec![[5.0, 5.0], [9.0, 5.0]], 0.0),
            (vec![[5.0, 5.0], [9.0, 5.0]], f32::INFINITY),
            (vec![[5.0, 5.0]], 1.0),
        ] {
            let s = Primitive::Stroke { points_px: points_px.clone(), closed: false, width_px, color: RED };
            assert!(tessellate(&packet(vec![s])).is_empty(), "{points_px:?} {width_px}");
        }
        for contours in
            [vec![], vec![vec![[0.0, 0.0], [5.0, 5.0]]], vec![vec![[0.0, 0.0], [f32::NAN, 0.0], [5.0, 5.0]]]]
        {
            assert!(tessellate(&packet(vec![fill(contours.clone())])).is_empty(), "{contours:?}");
        }
    }
}
