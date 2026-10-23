//! The saved document window: frame and pane widths as one defaults string (LF7). Platform
//! free, so the Windows host can keep the same state; the AppKit host stores it in
//! `NSUserDefaults` (tests use a temporary domain).

/// Frame (points, bottom-left origin as AppKit reports it) and the widths of the object list and
/// the inspector.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SavedWindow {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub sidebar: f64,
    pub inspector: f64,
}

/// Larger than any display or pane: such a value is a corrupted file, not a choice.
const LIMIT: f64 = 100_000.0;
/// Least visible part (points, both ways) for a saved frame to count as reachable.
const REACHABLE: f64 = 64.0;

impl SavedWindow {
    /// `x y width height sidebar inspector`.
    pub fn to_text(&self) -> String {
        format!("{} {} {} {} {} {}", self.x, self.y, self.width, self.height, self.sidebar, self.inspector)
    }

    /// The saved window of `text`; `None` for anything that is not six finite, sensible numbers.
    pub fn parse(text: &str) -> Option<SavedWindow> {
        let numbers: Vec<f64> = text.split_whitespace().map(|t| t.parse::<f64>().ok()).collect::<Option<_>>()?;
        let [x, y, width, height, sidebar, inspector] = <[f64; 6]>::try_from(numbers).ok()?;
        let sane = [x, y, width, height, sidebar, inspector].iter().all(|v| v.is_finite() && v.abs() <= LIMIT);
        let sized = width > 0.0 && height > 0.0 && sidebar > 0.0 && inspector > 0.0;
        (sane && sized).then_some(SavedWindow { x, y, width, height, sidebar, inspector })
    }

    /// Whether enough of the frame lies on one of the `screens` (x, y, width, height of their
    /// visible areas) to grab it: a display that was unplugged leaves a window nobody can reach.
    pub fn is_visible_on(&self, screens: &[(f64, f64, f64, f64)]) -> bool {
        screens.iter().any(|&(sx, sy, sw, sh)| {
            let overlap_x = (self.x + self.width).min(sx + sw) - self.x.max(sx);
            let overlap_y = (self.y + self.height).min(sy + sh) - self.y.max(sy);
            overlap_x >= REACHABLE && overlap_y >= REACHABLE
        })
    }
}
