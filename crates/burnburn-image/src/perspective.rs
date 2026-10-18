//! Planar perspective correction: a homography from point correspondences (direct linear
//! transform, least squares for more than four pairs) with its reprojection residual.

use burnburn_types::{CancellationToken, CanonicalEncode, CanonicalWriter};

use crate::error::ImageError;
use crate::gray::GrayImage;

/// Maps adjusted (cropped, rotated) pixel coordinates of the photo to the rectified working image
/// of `width × height` pixels. `points` are `(photo, rectified)` pairs, at least four, no three of
/// the first four collinear. With a perspective, `ImageRecipe::scale` is the scale of *rectified*
/// pixels. Only valid for a flat object; lens distortion and curved surfaces need other models.
#[derive(Debug, Clone, PartialEq)]
pub struct Perspective {
    pub points: Vec<([f64; 2], [f64; 2])>,
    pub width: u32,
    pub height: u32,
}

/// Row-major 3×3 matrix `m` with `m[8] = 1`: `(x', y') = ((m0 x + m1 y + m2) / w, (m3 x + m4 y + m5) / w)`,
/// `w = m6 x + m7 y + 1`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Homography {
    pub m: [f64; 9],
}

/// The plane transform shown to the user, with its fit quality.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PerspectiveReport {
    /// Photo → rectified.
    pub homography: Homography,
    /// Largest reprojection error of the given pairs, in rectified pixels (≈ 0 for exactly four).
    pub residual_px: f64,
}

impl Homography {
    pub fn apply(&self, p: [f64; 2]) -> Option<[f64; 2]> {
        let m = &self.m;
        let w = m[6] * p[0] + m[7] * p[1] + m[8];
        if w.abs() < 1e-12 {
            return None;
        }
        let x = (m[0] * p[0] + m[1] * p[1] + m[2]) / w;
        let y = (m[3] * p[0] + m[4] * p[1] + m[5]) / w;
        (x.is_finite() && y.is_finite()).then_some([x, y])
    }

    /// `(homography photo → rectified, max reprojection error px)`: direct linear transform with
    /// Hartley normalisation, least squares for more than four pairs.
    pub fn from_points(pairs: &[([f64; 2], [f64; 2])]) -> Result<(Homography, f64), ImageError> {
        if pairs.len() < 4 || pairs.iter().any(|(a, b)| !(a.iter().chain(b).all(|v| v.is_finite()))) {
            return Err(ImageError::DegeneratePerspective);
        }
        let src: Vec<[f64; 2]> = pairs.iter().map(|p| p.0).collect();
        let dst: Vec<[f64; 2]> = pairs.iter().map(|p| p.1).collect();
        for set in [&src, &dst] {
            if has_collinear_triple(&set[..4]) {
                return Err(ImageError::DegeneratePerspective);
            }
        }
        let (ts, ns) = normalise(&src).ok_or(ImageError::DegeneratePerspective)?;
        let (td, nd) = normalise(&dst).ok_or(ImageError::DegeneratePerspective)?;
        // Normal equations of the 2n × 8 system.
        let mut m = [[0.0f64; 9]; 8];
        for (s, d) in ns.iter().zip(&nd) {
            let (x, y, u, v) = (s[0], s[1], d[0], d[1]);
            let rows =
                [([x, y, 1.0, 0.0, 0.0, 0.0, -x * u, -y * u], u), ([0.0, 0.0, 0.0, x, y, 1.0, -x * v, -y * v], v)];
            for (row, rhs) in rows {
                for i in 0..8 {
                    for j in 0..8 {
                        m[i][j] += row[i] * row[j];
                    }
                    m[i][8] += row[i] * rhs;
                }
            }
        }
        let h = solve8(m).ok_or(ImageError::DegeneratePerspective)?;
        let hn = [h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7], 1.0];
        let full = mul(&invert(&td).ok_or(ImageError::DegeneratePerspective)?, &mul(&hn, &ts));
        if full[8].abs() < 1e-15 {
            return Err(ImageError::DegeneratePerspective);
        }
        let hom = Homography { m: full.map(|v| v / full[8]) };
        let mut residual = 0.0f64;
        for (s, d) in &pairs.iter().map(|p| (p.0, p.1)).collect::<Vec<_>>() {
            let p = hom.apply(*s).ok_or(ImageError::DegeneratePerspective)?;
            residual = residual.max((p[0] - d[0]).hypot(p[1] - d[1]));
        }
        Ok((hom, residual))
    }

    pub fn inverse(&self) -> Option<Homography> {
        invert(&self.m).map(|m| Homography { m })
    }
}

fn has_collinear_triple(p: &[[f64; 2]]) -> bool {
    let scale = p.iter().flat_map(|a| p.iter().map(move |b| (a[0] - b[0]).hypot(a[1] - b[1]))).fold(0.0, f64::max);
    let eps = 1e-9 * scale * scale;
    for i in 0..p.len() {
        for j in i + 1..p.len() {
            for k in j + 1..p.len() {
                let area = (p[j][0] - p[i][0]) * (p[k][1] - p[i][1]) - (p[j][1] - p[i][1]) * (p[k][0] - p[i][0]);
                if area.abs() <= eps.max(1e-12) {
                    return true;
                }
            }
        }
    }
    false
}

/// Similarity moving the centroid to 0 and the mean distance to √2, and the transformed points.
fn normalise(p: &[[f64; 2]]) -> Option<([f64; 9], Vec<[f64; 2]>)> {
    let n = p.len() as f64;
    let (cx, cy) = (p.iter().map(|q| q[0]).sum::<f64>() / n, p.iter().map(|q| q[1]).sum::<f64>() / n);
    let mean = p.iter().map(|q| (q[0] - cx).hypot(q[1] - cy)).sum::<f64>() / n;
    if !(mean > 0.0) {
        return None;
    }
    let s = std::f64::consts::SQRT_2 / mean;
    let t = [s, 0.0, -s * cx, 0.0, s, -s * cy, 0.0, 0.0, 1.0];
    Some((t, p.iter().map(|q| [s * (q[0] - cx), s * (q[1] - cy)]).collect()))
}

fn mul(a: &[f64; 9], b: &[f64; 9]) -> [f64; 9] {
    let mut r = [0.0; 9];
    for i in 0..3 {
        for j in 0..3 {
            r[i * 3 + j] = (0..3).map(|k| a[i * 3 + k] * b[k * 3 + j]).sum();
        }
    }
    r
}

fn invert(m: &[f64; 9]) -> Option<[f64; 9]> {
    let [a, b, c, d, e, f, g, h, i] = *m;
    let det = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g);
    if !(det.abs() > 1e-300) || !det.is_finite() {
        return None;
    }
    Some([
        (e * i - f * h) / det,
        (c * h - b * i) / det,
        (b * f - c * e) / det,
        (f * g - d * i) / det,
        (a * i - c * g) / det,
        (c * d - a * f) / det,
        (d * h - e * g) / det,
        (b * g - a * h) / det,
        (a * e - b * d) / det,
    ])
}

/// Gaussian elimination with partial pivoting on an augmented 8×9 matrix.
fn solve8(mut m: [[f64; 9]; 8]) -> Option<[f64; 8]> {
    let scale = m.iter().flat_map(|r| r[..8].iter()).map(|v| v.abs()).fold(0.0, f64::max);
    for col in 0..8 {
        let pivot = (col..8).max_by(|&a, &b| m[a][col].abs().total_cmp(&m[b][col].abs()))?;
        if m[pivot][col].abs() <= 1e-12 * scale.max(1e-300) {
            return None;
        }
        m.swap(col, pivot);
        for row in 0..8 {
            if row != col {
                let f = m[row][col] / m[col][col];
                let pivot_row = m[col];
                for (v, p) in m[row].iter_mut().zip(pivot_row).skip(col) {
                    *v -= f * p;
                }
            }
        }
    }
    let mut out = [0.0; 8];
    for (i, o) in out.iter_mut().enumerate() {
        *o = m[i][8] / m[i][i];
    }
    out.iter().all(|v| v.is_finite()).then_some(out)
}

/// Warps `gray` (photo, `w × h`) into the rectified image with bilinear sampling at pixel centres;
/// coordinates outside the photo take the nearest edge value.
pub(crate) fn warp(
    gray: &GrayImage,
    p: &Perspective,
    cancel: &CancellationToken,
) -> Result<(GrayImage, PerspectiveReport), ImageError> {
    if p.width == 0 || p.height == 0 {
        return Err(ImageError::DegeneratePerspective);
    }
    let (homography, residual_px) = Homography::from_points(&p.points)?;
    let back = homography.inverse().ok_or(ImageError::DegeneratePerspective)?;
    let (w, h) = (gray.width as f64, gray.height as f64);
    let mut values = Vec::with_capacity(p.width as usize * p.height as usize);
    for v in 0..p.height {
        if v % 64 == 0 {
            cancel.check()?;
        }
        for u in 0..p.width {
            let q = back.apply([f64::from(u) + 0.5, f64::from(v) + 0.5]).ok_or(ImageError::DegeneratePerspective)?;
            let x = (q[0] - 0.5).clamp(0.0, w - 1.0);
            let y = (q[1] - 0.5).clamp(0.0, h - 1.0);
            let (x0, y0) = (x.floor(), y.floor());
            let (fx, fy) = ((x - x0) as f32, (y - y0) as f32);
            let (x0, y0) = (x0 as u32, y0 as u32);
            let (x1, y1) = ((x0 + 1).min(gray.width - 1), (y0 + 1).min(gray.height - 1));
            let top = gray.get(x0, y0) * (1.0 - fx) + gray.get(x1, y0) * fx;
            let bottom = gray.get(x0, y1) * (1.0 - fx) + gray.get(x1, y1) * fx;
            values.push(top * (1.0 - fy) + bottom * fy);
        }
    }
    Ok((GrayImage { width: p.width, height: p.height, values }, PerspectiveReport { homography, residual_px }))
}

impl CanonicalEncode for Perspective {
    fn encode(&self, out: &mut CanonicalWriter) {
        out.tag("perspective-v1");
        out.u64(self.points.len() as u64);
        for (a, b) in &self.points {
            for v in [a[0], a[1], b[0], b[1]] {
                out.f64(v);
            }
        }
        out.u64(u64::from(self.width));
        out.u64(u64::from(self.height));
    }
}
