//! Thumbnails of library drawings: the outline as 64 × 64 coverage, made without any GPU or
//! native drawing so every host shows the same picture.

use burnburn_geometry::{Bounds, DisplayTolerance, Path};
use burnburn_types::Length;

use crate::files::ArtThumbnail;

/// Width and height of a thumbnail in pixels.
pub const SIZE: u32 = 64;
/// Empty border around the drawing, in pixels.
const MARGIN: f64 = 3.0;

/// The outline of `paths` fitted into the thumbnail with its proportions kept. `None` when there
/// is no geometry.
pub(crate) fn render<'a>(paths: impl IntoIterator<Item = &'a Path>) -> Option<ArtThumbnail> {
    let paths: Vec<&Path> = paths.into_iter().collect();
    let bounds = paths.iter().filter_map(|p| p.bounds()).reduce(Bounds::union)?;
    let extent = bounds.width().max(bounds.height());
    let inner = f64::from(SIZE) - 2.0 * MARGIN;
    // A drawing of zero extent (a point) is drawn as a dot in the middle.
    let scale = if extent > 0.0 { inner / extent } else { 1.0 };
    let (cx, cy) = ((bounds.min().x + bounds.max().x) / 2.0, (bounds.min().y + bounds.max().y) / 2.0);
    let half = f64::from(SIZE) / 2.0;
    let to_px = |x: f64, y: f64| (half + (x - cx) * scale, half - (y - cy) * scale);
    let tolerance = DisplayTolerance::new(Length::from_mm((extent / 400.0).max(1e-9)).ok()?).ok()?;
    let mut coverage = vec![0u8; (SIZE * SIZE) as usize];
    for path in paths {
        for line in path.flatten(&tolerance) {
            let mut points: Vec<(f64, f64)> = line.points.iter().map(|p| to_px(p.x, p.y)).collect();
            if line.closed
                && let Some(first) = points.first().copied()
            {
                points.push(first);
            }
            if points.len() == 1 {
                plot(&mut coverage, points[0].0, points[0].1);
            }
            for pair in points.windows(2) {
                segment(&mut coverage, pair[0], pair[1]);
            }
        }
    }
    Some(ArtThumbnail { width: SIZE, height: SIZE, coverage })
}

fn plot(coverage: &mut [u8], x: f64, y: f64) {
    let (px, py) = (x.floor(), y.floor());
    if px >= 0.0 && py >= 0.0 && px < f64::from(SIZE) && py < f64::from(SIZE) {
        coverage[(py as u32 * SIZE + px as u32) as usize] = 255;
    }
}

/// A one-pixel line sampled at sub-pixel steps (no gaps, no anti-aliasing: it is a thumbnail).
fn segment(coverage: &mut [u8], a: (f64, f64), b: (f64, f64)) {
    let steps = ((b.0 - a.0).abs().max((b.1 - a.1).abs()) * 2.0).ceil().max(1.0) as u32;
    for i in 0..=steps {
        let t = f64::from(i) / f64::from(steps);
        plot(coverage, a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
    }
}
