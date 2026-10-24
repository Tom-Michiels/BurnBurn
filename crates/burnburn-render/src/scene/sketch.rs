//! Sketch-mode overlays as primitives: relation glyphs, freedom markers, dimensions with their
//! numbers as glyph outlines (`text`), dashed suggestions. Everything is built from strokes and small
//! fills in device pixels; a conflict is drawn with a cross as well as the conflict colour (X03).

use burnburn_editor::{Overlay, PointFreedom, SketchGlyph, SketchMark};
use burnburn_geometry::{Affine, Point};

use super::{Projection, Theme, flatten, text};
use crate::input::Rgba;
use crate::packet::Primitive;

/// Half the side of a glyph's box, in logical points.
const GLYPH_HALF_PT: f64 = 6.0;
const GLYPH_WIDTH_PT: f64 = 1.2;
const ENTITY_WIDTH_PT: f64 = 2.5;
/// Size of dimension labels (the em square), in logical points.
const TEXT_HEIGHT_PT: f64 = 11.0;
/// How far the halo around a label reaches beyond its glyphs, in logical points.
const HALO_PT: f64 = 1.5;

type Stroke = (Vec<[f64; 2]>, bool);

fn ring(r: f64, sides: usize) -> Vec<[f64; 2]> {
    (0..sides)
        .map(|k| {
            let a = std::f64::consts::TAU * k as f64 / sides as f64;
            [r * a.cos(), r * a.sin()]
        })
        .collect()
}

/// The glyph's strokes in a box of ±1 (y up).
fn glyph_strokes(glyph: SketchGlyph) -> Vec<Stroke> {
    let open = |pts: &[[f64; 2]]| (pts.to_vec(), false);
    match glyph {
        SketchGlyph::Horizontal => vec![
            open(&[[-1.0, 0.0], [1.0, 0.0]]),
            open(&[[-1.0, -0.45], [-1.0, 0.45]]),
            open(&[[1.0, -0.45], [1.0, 0.45]]),
        ],
        SketchGlyph::Vertical => vec![
            open(&[[0.0, -1.0], [0.0, 1.0]]),
            open(&[[-0.45, -1.0], [0.45, -1.0]]),
            open(&[[-0.45, 1.0], [0.45, 1.0]]),
        ],
        SketchGlyph::Parallel => vec![open(&[[-0.7, -1.0], [0.1, 1.0]]), open(&[[-0.1, -1.0], [0.7, 1.0]])],
        SketchGlyph::Perpendicular => vec![open(&[[-1.0, -0.9], [1.0, -0.9]]), open(&[[0.0, -0.9], [0.0, 1.0]])],
        SketchGlyph::Equal => vec![open(&[[-1.0, 0.4], [1.0, 0.4]]), open(&[[-1.0, -0.4], [1.0, -0.4]])],
        SketchGlyph::Tangent => {
            let circle = ring(0.6, 12).into_iter().map(|[x, y]| [x, y + 0.4]).collect();
            vec![(circle, true), open(&[[-1.0, -0.2], [1.0, -0.2]])]
        }
        SketchGlyph::Coincident => {
            vec![(ring(0.8, 8), true), open(&[[-0.4, 0.0], [0.4, 0.0]]), open(&[[0.0, -0.4], [0.0, 0.4]])]
        }
        SketchGlyph::PointOn => {
            let diamond = vec![[0.0, 0.8], [0.4, 0.4], [0.0, 0.0], [-0.4, 0.4]];
            vec![(diamond, true), open(&[[-1.0, -0.6], [1.0, -0.6]])]
        }
        SketchGlyph::Midpoint => vec![
            open(&[[-1.0, 0.0], [1.0, 0.0]]),
            open(&[[0.0, -0.6], [0.0, 0.6]]),
            open(&[[-0.55, -0.3], [-0.55, 0.3]]),
            open(&[[0.55, -0.3], [0.55, 0.3]]),
        ],
        SketchGlyph::Symmetric => vec![
            open(&[[0.0, -1.0], [0.0, 1.0]]),
            open(&[[-0.3, 0.5], [-0.9, 0.0], [-0.3, -0.5]]),
            open(&[[0.3, 0.5], [0.9, 0.0], [0.3, -0.5]]),
        ],
        SketchGlyph::Concentric => vec![(ring(0.95, 16), true), (ring(0.5, 16), true)],
        SketchGlyph::Fixed => {
            vec![(vec![[-0.9, -0.5], [0.9, -0.5], [0.0, 0.9]], true), open(&[[-1.0, -0.9], [1.0, -0.9]])]
        }
        _ => vec![(ring(0.8, 4), true)],
    }
}

fn colour_of(mark: SketchMark, normal: Rgba, theme: &Theme) -> Rgba {
    match mark {
        SketchMark::Normal => normal,
        SketchMark::Selected => theme.selection,
        SketchMark::Conflict => theme.issue,
    }
}

fn place(center: [f64; 2], scale: f64, strokes: &[Stroke]) -> Vec<Stroke> {
    strokes
        .iter()
        .map(|(pts, closed)| {
            (pts.iter().map(|[x, y]| [center[0] + x * scale, center[1] - y * scale]).collect(), *closed)
        })
        .collect()
}

fn push(out: &mut Vec<Primitive>, strokes: Vec<Stroke>, width_px: f64, color: Rgba) {
    for (points, closed) in strokes {
        let points_px = points.into_iter().map(|[x, y]| [x as f32, y as f32]).collect();
        out.push(Primitive::Stroke { points_px, closed, width_px: width_px as f32, color });
    }
}

/// A closed frame of `half` pixels around `center`, as dashes.
fn dashed_frame(center: [f64; 2], half: f64) -> Vec<Stroke> {
    const DASHES: usize = 3;
    let corners = [[-half, -half], [half, -half], [half, half], [-half, half]];
    let mut out = Vec::new();
    for side in 0..4 {
        let (a, b) = (corners[side], corners[(side + 1) % 4]);
        for k in 0..DASHES {
            let (t0, t1) = ((2 * k) as f64 / (2 * DASHES) as f64, (2 * k + 1) as f64 / (2 * DASHES) as f64);
            let at = |t: f64| [center[0] + a[0] + (b[0] - a[0]) * t, center[1] + a[1] + (b[1] - a[1]) * t];
            out.push((vec![at(t0), at(t1)], false));
        }
    }
    out
}

fn cross(center: [f64; 2], half: f64) -> Vec<Stroke> {
    vec![
        (vec![[center[0] - half, center[1] - half], [center[0] + half, center[1] + half]], false),
        (vec![[center[0] - half, center[1] + half], [center[0] + half, center[1] - half]], false),
    ]
}

pub(super) fn draw(out: &mut Vec<Primitive>, overlay: &Overlay, theme: &Theme, proj: &Projection) {
    let s = proj.scale;
    match overlay {
        Overlay::SketchGlyph { at, glyph, mark } => {
            let c = proj.px(*at);
            let half = GLYPH_HALF_PT * s;
            let color = colour_of(*mark, theme.sketch, theme);
            push(out, place(c, half, &glyph_strokes(*glyph)), GLYPH_WIDTH_PT * s, color);
            if *mark == SketchMark::Conflict {
                push(out, cross(c, half), GLYPH_WIDTH_PT * s, color);
            }
        }
        Overlay::SketchSuggestion { at, glyph } => {
            let c = proj.px(*at);
            let half = GLYPH_HALF_PT * s;
            push(out, place(c, half, &glyph_strokes(*glyph)), GLYPH_WIDTH_PT * s, theme.preview);
            push(out, dashed_frame(c, half * 1.4), GLYPH_WIDTH_PT * s, theme.preview);
        }
        Overlay::SketchPoint { at, freedom, selected } => {
            let c = proj.px(*at);
            match freedom {
                PointFreedom::Free => {
                    push(out, place(c, 4.0 * s, &[(ring(1.0, 12), true)]), GLYPH_WIDTH_PT * s, theme.sketch)
                }
                PointFreedom::Determined => {
                    let h = 2.5 * s;
                    let dot = [[c[0] - h, c[1] - h], [c[0] + h, c[1] - h], [c[0] + h, c[1] + h], [c[0] - h, c[1] + h]]
                        .map(|[x, y]| [x as f32, y as f32])
                        .to_vec();
                    out.push(Primitive::Fill { contours_px: vec![dot], color: theme.sketch });
                }
                PointFreedom::Fixed => {
                    let h = 5.0 * s;
                    let tri = [[c[0] - h, c[1] + h], [c[0] + h, c[1] + h], [c[0], c[1] - h]]
                        .map(|[x, y]| [x as f32, y as f32])
                        .to_vec();
                    out.push(Primitive::Fill { contours_px: vec![tri], color: theme.sketch });
                }
            }
            if *selected {
                push(out, place(c, 7.0 * s, &[(ring(1.0, 16), true)]), GLYPH_WIDTH_PT * s, theme.selection);
            }
        }
        Overlay::SketchEntity { path, mark } => {
            let color = colour_of(*mark, theme.sketch, theme);
            for (points_px, closed) in flatten(path, &Affine::IDENTITY, proj) {
                out.push(Primitive::Stroke { points_px, closed, width_px: (ENTITY_WIDTH_PT * s) as f32, color });
            }
        }
        Overlay::SketchDimension { lines, label_at, text, driven, mark } => {
            let base = if *driven { Rgba { a: 0.75, ..theme.sketch_dimension } } else { theme.sketch_dimension };
            let color = colour_of(*mark, base, theme);
            for [a, b] in lines {
                let (pa, pb): (Point, Point) = (*a, *b);
                push(out, vec![(vec![proj.px(pa), proj.px(pb)], false)], s, color);
            }
            let c = proj.px(*label_at);
            let style = text::TextStyle {
                em_px: TEXT_HEIGHT_PT * s,
                ink: color,
                halo: proj.halo,
                halo_px: (HALO_PT * s) as f32,
                quarter_turns: 0,
            };
            text::label(out, text, c, text::Anchor::Center, style);
            if *mark == SketchMark::Conflict {
                push(out, cross(c, GLYPH_HALF_PT * s), GLYPH_WIDTH_PT * s, color);
            }
        }
        _ => {}
    }
}
