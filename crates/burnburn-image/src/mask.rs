use burnburn_types::{CancellationToken, CanonicalEncode, CanonicalWriter};

use crate::adjust::PixelRect;
use crate::error::ImageError;

/// A binary foreground mask in adjusted pixel coordinates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinaryMask {
    width: u32,
    height: u32,
    bits: Vec<bool>,
}

impl BinaryMask {
    pub fn new(width: u32, height: u32) -> Self {
        BinaryMask { width, height, bits: vec![false; width as usize * height as usize] }
    }

    pub fn from_fn(width: u32, height: u32, mut f: impl FnMut(u32, u32) -> bool) -> Self {
        let mut bits = Vec::with_capacity(width as usize * height as usize);
        for y in 0..height {
            for x in 0..width {
                bits.push(f(x, y));
            }
        }
        BinaryMask { width, height, bits }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// Out-of-range coordinates are background.
    pub fn get(&self, x: i64, y: i64) -> bool {
        if x < 0 || y < 0 || x >= i64::from(self.width) || y >= i64::from(self.height) {
            return false;
        }
        self.bits.get(y as usize * self.width as usize + x as usize).copied().unwrap_or(false)
    }

    pub fn set(&mut self, x: u32, y: u32, v: bool) {
        if let Some(b) = self.bits.get_mut(y as usize * self.width as usize + x as usize) {
            *b = v;
        }
    }

    pub fn count(&self) -> usize {
        self.bits.iter().filter(|&&b| b).count()
    }

    pub(crate) fn bits(&self) -> &[bool] {
        &self.bits
    }
}

/// Region mask mode. Include regions restrict the trace to their union; exclude regions force
/// background. Neither modifies the source image.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RegionMode {
    Include,
    Exclude,
}

/// Region shapes in adjusted pixel coordinates; a pixel belongs when its centre is inside.
#[derive(Debug, Clone, PartialEq)]
pub enum RegionShape {
    Rect(PixelRect),
    /// Closed polygon, even-odd rule, vertices `[x, y]` in pixels.
    Polygon(Vec<[f64; 2]>),
    /// A pointer stroke (brush): every pixel whose centre is within `radius` pixels of the polyline
    /// through `points`. A single point is a disc.
    Stroke {
        points: Vec<[f64; 2]>,
        radius: f64,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct RegionMask {
    pub mode: RegionMode,
    pub shape: RegionShape,
}

/// Binary morphology with a `(2r+1)²` square structuring element; outside the image is background.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Morphology {
    /// Erode then dilate: removes specks narrower than the element.
    Open(u32),
    /// Dilate then erode: fills pinholes and gaps narrower than the element.
    Close(u32),
}

pub const MAX_MORPHOLOGY_RADIUS: u32 = 16;

impl RegionShape {
    /// A polygon needs at least three finite vertices.
    pub fn validate(&self) -> Result<(), ImageError> {
        match self {
            RegionShape::Rect(_) => Ok(()),
            RegionShape::Polygon(pts) => {
                if pts.len() < 3 || pts.iter().any(|p| !(p[0].is_finite() && p[1].is_finite())) {
                    Err(ImageError::BadRegion)
                } else {
                    Ok(())
                }
            }
            RegionShape::Stroke { points, radius } => {
                let finite = points.iter().all(|p| p[0].is_finite() && p[1].is_finite());
                if points.is_empty() || !finite || !(radius.is_finite() && *radius > 0.0) {
                    Err(ImageError::BadRegion)
                } else {
                    Ok(())
                }
            }
        }
    }

    /// Whether the centre of pixel `(x, y)` lies inside the shape (rect half-open, polygon even-odd).
    pub fn contains_pixel(&self, x: u32, y: u32) -> bool {
        let (px, py) = (f64::from(x) + 0.5, f64::from(y) + 0.5);
        match self {
            RegionShape::Rect(r) => {
                let (x0, y0) = (f64::from(r.x), f64::from(r.y));
                px >= x0 && px < x0 + f64::from(r.width) && py >= y0 && py < y0 + f64::from(r.height)
            }
            RegionShape::Polygon(pts) => {
                // Even-odd crossing test along +x.
                let mut inside = false;
                let mut j = pts.len() - 1;
                for i in 0..pts.len() {
                    let (a, b) = (pts[i], pts[j]);
                    if (a[1] > py) != (b[1] > py) {
                        let cross_x = a[0] + (py - a[1]) / (b[1] - a[1]) * (b[0] - a[0]);
                        if px < cross_x {
                            inside = !inside;
                        }
                    }
                    j = i;
                }
                inside
            }
            RegionShape::Stroke { points, radius } => {
                let limit = radius * radius;
                match points.as_slice() {
                    [] => false,
                    [only] => dist2_to_segment((px, py), *only, *only) <= limit,
                    _ => points.windows(2).any(|w| dist2_to_segment((px, py), w[0], w[1]) <= limit),
                }
            }
        }
    }
}

/// Squared distance from `p` to the segment `a`–`b`.
fn dist2_to_segment(p: (f64, f64), a: [f64; 2], b: [f64; 2]) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len2 = dx * dx + dy * dy;
    let t = if len2 > 0.0 { (((p.0 - a[0]) * dx + (p.1 - a[1]) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
    let (qx, qy) = (a[0] + t * dx, a[1] + t * dy);
    (p.0 - qx).powi(2) + (p.1 - qy).powi(2)
}

/// Applies region masks in order: includes (union) first, then excludes.
pub(crate) fn apply_regions(mask: &mut BinaryMask, regions: &[RegionMask]) -> Result<(), ImageError> {
    for r in regions {
        r.shape.validate()?;
    }
    let includes: Vec<&RegionShape> =
        regions.iter().filter(|r| r.mode == RegionMode::Include).map(|r| &r.shape).collect();
    let excludes: Vec<&RegionShape> =
        regions.iter().filter(|r| r.mode == RegionMode::Exclude).map(|r| &r.shape).collect();
    if includes.is_empty() && excludes.is_empty() {
        return Ok(());
    }
    for y in 0..mask.height {
        for x in 0..mask.width {
            let included = includes.is_empty() || includes.iter().any(|s| s.contains_pixel(x, y));
            let excluded = excludes.iter().any(|s| s.contains_pixel(x, y));
            if !included || excluded {
                mask.set(x, y, false);
            }
        }
    }
    Ok(())
}

/// One pass of erosion (`keep_if_all`) or dilation with a square element of radius `r`.
/// Separable: a horizontal then a vertical run test, each O(1) per pixel via prefix counts.
fn square_pass(mask: &BinaryMask, r: u32, erode: bool, cancel: &CancellationToken) -> Result<BinaryMask, ImageError> {
    let (w, h) = (mask.width as usize, mask.height as usize);
    let r = r as usize;
    let horizontal = run_pass(&mask.bits, w, h, r, erode, true, cancel)?;
    let bits = run_pass(&horizontal, w, h, r, erode, false, cancel)?;
    Ok(BinaryMask { width: mask.width, height: mask.height, bits })
}

fn run_pass(
    bits: &[bool],
    w: usize,
    h: usize,
    r: usize,
    erode: bool,
    along_x: bool,
    cancel: &CancellationToken,
) -> Result<Vec<bool>, ImageError> {
    let (lines, len) = if along_x { (h, w) } else { (w, h) };
    let at = |line: usize, i: usize| if along_x { line * w + i } else { i * w + line };
    let mut out = vec![false; bits.len()];
    let mut prefix = vec![0usize; len + 1];
    for line in 0..lines {
        if line % 64 == 0 {
            cancel.check()?;
        }
        for i in 0..len {
            prefix[i + 1] = prefix[i] + usize::from(bits.get(at(line, i)).copied().unwrap_or(false));
        }
        for i in 0..len {
            // Window [i-r, i+r]; outside the image counts as background.
            let lo = i.saturating_sub(r);
            let hi = (i + r + 1).min(len);
            let ones = prefix[hi] - prefix[lo];
            let v = if erode { i >= r && i + r < len && ones == 2 * r + 1 } else { ones > 0 };
            if let Some(o) = out.get_mut(at(line, i)) {
                *o = v;
            }
        }
    }
    Ok(out)
}

pub(crate) fn apply_morphology(
    mask: &BinaryMask,
    op: Morphology,
    cancel: &CancellationToken,
) -> Result<BinaryMask, ImageError> {
    let (r, first_erode) = match op {
        Morphology::Open(r) => (r, true),
        Morphology::Close(r) => (r, false),
    };
    if r > MAX_MORPHOLOGY_RADIUS {
        return Err(ImageError::RadiusTooLarge { radius: r, max: MAX_MORPHOLOGY_RADIUS });
    }
    if r == 0 {
        return Ok(mask.clone());
    }
    let once = square_pass(mask, r, first_erode, cancel)?;
    square_pass(&once, r, !first_erode, cancel)
}

impl CanonicalEncode for RegionMask {
    fn encode(&self, out: &mut CanonicalWriter) {
        out.tag(match self.mode {
            RegionMode::Include => "region-include",
            RegionMode::Exclude => "region-exclude",
        });
        match &self.shape {
            RegionShape::Rect(r) => r.encode(out),
            RegionShape::Polygon(pts) => {
                out.tag("polygon");
                out.u64(pts.len() as u64);
                for p in pts {
                    out.f64(p[0]);
                    out.f64(p[1]);
                }
            }
            RegionShape::Stroke { points, radius } => {
                out.tag("stroke");
                out.f64(*radius);
                out.u64(points.len() as u64);
                for p in points {
                    out.f64(p[0]);
                    out.f64(p[1]);
                }
            }
        }
    }
}

impl CanonicalEncode for Morphology {
    fn encode(&self, out: &mut CanonicalWriter) {
        match *self {
            Morphology::Open(r) => {
                out.tag("open");
                out.u64(u64::from(r));
            }
            Morphology::Close(r) => {
                out.tag("close");
                out.u64(u64::from(r));
            }
        }
    }
}
