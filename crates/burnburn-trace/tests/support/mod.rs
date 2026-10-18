//! Test support for trace contracts: an independent supersampling rasterizer for analytic vector
//! references and a bidirectional Hausdorff oracle. Nothing here calls the tracer's own geometry code
//! except `burnburn_geometry::Path::flatten` (IM02, tested separately) to sample the traced output.
//!
//! Fixed rasterisation (recorded in docs/dev-notes/IM12.md): pixel `(x, y)` (y-down, top-left origin)
//! covers millimetres `[x·s, (x+1)·s] × [(h−y−1)·s, (h−y)·s]` (y-up), `s` = mm per pixel. Its gray value
//! is `255 · (1 − coverage)` rounded to nearest, coverage estimated with `SS × SS` stratified samples at
//! sub-cell centres. Ink (inside the shape) is black.

#![allow(dead_code, clippy::unwrap_used)]

use burnburn_geometry::{Path, Point, ProductionTolerance};
use burnburn_image::SourceImage;
use burnburn_types::Length;

pub const SS: u32 = 8;

pub fn rasterize(w: u32, h: u32, mm_per_px: f64, inside: impl Fn(f64, f64) -> bool) -> SourceImage {
    let mut px = Vec::with_capacity((w * h) as usize);
    for y in 0..h {
        for x in 0..w {
            let mut hits = 0u32;
            for sy in 0..SS {
                for sx in 0..SS {
                    let fx = (f64::from(x) + (f64::from(sx) + 0.5) / f64::from(SS)) * mm_per_px;
                    let fy = (f64::from(h - y - 1) + (f64::from(sy) + 0.5) / f64::from(SS)) * mm_per_px;
                    hits += u32::from(inside(fx, fy));
                }
            }
            let coverage = f64::from(hits) / f64::from(SS * SS);
            px.push((255.0 * (1.0 - coverage)).round() as u8);
        }
    }
    SourceImage::from_gray8(w, h, px).unwrap()
}

pub fn disc(cx: f64, cy: f64, r: f64) -> impl Fn(f64, f64) -> bool {
    move |x, y| (x - cx).powi(2) + (y - cy).powi(2) <= r * r
}

/// Convex polygon (counter-clockwise vertices) containment.
pub fn convex(poly: Vec<(f64, f64)>) -> impl Fn(f64, f64) -> bool {
    move |x, y| {
        (0..poly.len()).all(|i| {
            let (ax, ay) = poly[i];
            let (bx, by) = poly[(i + 1) % poly.len()];
            (bx - ax) * (y - ay) - (by - ay) * (x - ax) >= 0.0
        })
    }
}

/// Densely sampled points of a path: flattened at 1 µm, then chords subdivided to ≤ `step` mm.
pub fn sample_path(path: &Path, step: f64) -> Vec<Point> {
    let tol = ProductionTolerance::new(Length::from_mm(0.001).unwrap()).unwrap();
    let mut out = Vec::new();
    for poly in path.flatten(&tol) {
        let n = poly.points.len();
        let count = if poly.closed { n } else { n.saturating_sub(1) };
        for i in 0..count {
            let (a, b) = (poly.points[i], poly.points[(i + 1) % n]);
            let k = (a.distance(b) / step).ceil().max(1.0) as usize;
            for j in 0..k {
                out.push(a.lerp(b, j as f64 / k as f64));
            }
        }
    }
    out
}

pub fn polygon_samples(poly: &[(f64, f64)], step: f64) -> Vec<Point> {
    let mut out = Vec::new();
    for i in 0..poly.len() {
        let a = Point::new(poly[i].0, poly[i].1);
        let b = Point::new(poly[(i + 1) % poly.len()].0, poly[(i + 1) % poly.len()].1);
        let k = (a.distance(b) / step).ceil().max(1.0) as usize;
        for j in 0..k {
            out.push(a.lerp(b, j as f64 / k as f64));
        }
    }
    out
}

pub fn circle_samples(cx: f64, cy: f64, r: f64, step: f64) -> Vec<Point> {
    let n = ((std::f64::consts::TAU * r) / step).ceil() as usize;
    (0..n)
        .map(|i| {
            let t = std::f64::consts::TAU * i as f64 / n as f64;
            Point::new(cx + r * t.cos(), cy + r * t.sin())
        })
        .collect()
}

/// One-sided distance: max over `a` of the distance to the nearest point of `b`.
pub fn directed(a: &[Point], b: &[Point]) -> f64 {
    a.iter().map(|p| b.iter().map(|q| p.distance(*q)).fold(f64::INFINITY, f64::min)).fold(0.0, f64::max)
}

/// Bidirectional Hausdorff distance between two dense point samplings.
pub fn hausdorff(a: &[Point], b: &[Point]) -> f64 {
    directed(a, b).max(directed(b, a))
}

/// Like [`rasterize`] but with gray levels: each sample takes the level of the first shape that
/// contains it (paper 255 otherwise); the pixel is the rounded mean of its samples.
pub fn rasterize_levels(w: u32, h: u32, mm_per_px: f64, shapes: &[(&dyn Fn(f64, f64) -> bool, u8)]) -> SourceImage {
    let mut px = Vec::with_capacity((w * h) as usize);
    for y in 0..h {
        for x in 0..w {
            let mut sum = 0u32;
            for sy in 0..SS {
                for sx in 0..SS {
                    let fx = (f64::from(x) + (f64::from(sx) + 0.5) / f64::from(SS)) * mm_per_px;
                    let fy = (f64::from(h - y - 1) + (f64::from(sy) + 0.5) / f64::from(SS)) * mm_per_px;
                    sum += shapes.iter().find(|(f, _)| f(fx, fy)).map_or(255, |&(_, g)| u32::from(g));
                }
            }
            px.push((f64::from(sum) / f64::from(SS * SS)).round() as u8);
        }
    }
    SourceImage::from_gray8(w, h, px).unwrap()
}

/// Round-capped stroke of half-width `r` along segment a–b.
pub fn capsule(ax: f64, ay: f64, bx: f64, by: f64, r: f64) -> impl Fn(f64, f64) -> bool {
    move |x, y| {
        let (dx, dy) = (bx - ax, by - ay);
        let len2 = dx * dx + dy * dy;
        let t = if len2 > 0.0 { (((x - ax) * dx + (y - ay) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
        (x - ax - t * dx).powi(2) + (y - ay - t * dy).powi(2) <= r * r
    }
}

/// Round-capped arc stroke: centre, radius `big_r`, from angle `a0` sweeping `sweep` (radians, ccw).
pub fn arc_stroke(cx: f64, cy: f64, big_r: f64, a0: f64, sweep: f64, r: f64) -> impl Fn(f64, f64) -> bool {
    move |x, y| {
        let d = (x - cx).hypot(y - cy);
        let mut a = (y - cy).atan2(x - cx) - a0;
        a = a.rem_euclid(std::f64::consts::TAU);
        if a <= sweep && (d - big_r).abs() <= r {
            return true;
        }
        let end = |t: f64| (cx + big_r * t.cos(), cy + big_r * t.sin());
        let (sx, sy) = end(a0);
        let (ex, ey) = end(a0 + sweep);
        (x - sx).hypot(y - sy) <= r || (x - ex).hypot(y - ey) <= r
    }
}
