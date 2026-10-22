//! Starting places for the touch of two ellipses that lie deep in each other (G9-editor, S01).
//!
//! The solver finds a touch from a state that is already near it. Two ellipses whose outlines cross
//! deeply are far from every touch, so the second one is first slid along the line between the two
//! centres to the places where the crossing stops: just outside the first, or just inside it (or
//! around it). Those places, nearest to the drawn one first, are where a retry starts from.

use burnburn_geometry::Point;

/// An ellipse as plain numbers: centre, unit direction of the semi-axis `a`, semi-axes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Oval {
    pub centre: Point,
    pub u: (f64, f64),
    pub a: f64,
    pub b: f64,
}

/// How two ovals lie to each other.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lie {
    Apart,
    Crossing,
    SecondInside,
    FirstInside,
}

const SAMPLES: usize = 180;
const SCAN_STEPS: usize = 240;

impl Oval {
    fn v(&self) -> (f64, f64) {
        (-self.u.1, self.u.0)
    }

    /// Whether `p` lies strictly inside.
    fn contains(&self, p: Point) -> bool {
        let (dx, dy) = (p.x - self.centre.x, p.y - self.centre.y);
        let v = self.v();
        let (x, y) = ((dx * self.u.0 + dy * self.u.1) / self.a, (dx * v.0 + dy * v.1) / self.b);
        x * x + y * y < 1.0 - 1e-9
    }

    fn samples(&self) -> Vec<Point> {
        let v = self.v();
        (0..SAMPLES)
            .map(|i| {
                let t = std::f64::consts::TAU * i as f64 / SAMPLES as f64;
                let (s, k) = (t.sin(), t.cos());
                Point::new(
                    self.centre.x + self.a * k * self.u.0 + self.b * s * v.0,
                    self.centre.y + self.a * k * self.u.1 + self.b * s * v.1,
                )
            })
            .collect()
    }

    fn at(&self, centre: Point) -> Oval {
        Oval { centre, ..*self }
    }
}

fn lie(first: &Oval, second: &Oval) -> Lie {
    let (own, theirs) = (first.samples(), second.samples());
    let second_in = theirs.iter().filter(|p| first.contains(**p)).count();
    let first_in = own.iter().filter(|p| second.contains(**p)).count();
    if second_in == 0 && first_in == 0 {
        Lie::Apart
    } else if second_in == theirs.len() {
        Lie::SecondInside
    } else if first_in == own.len() {
        Lie::FirstInside
    } else {
        Lie::Crossing
    }
}

/// The distances `D` from the centre of `first`, along the unit direction `dir`, at which the centre
/// of `second` puts the two outlines at the edge of crossing: the places where a touch is near. In
/// increasing order.
pub(crate) fn touch_distances(first: &Oval, second: &Oval, dir: (f64, f64)) -> Vec<f64> {
    if !(first.a > 0.0 && first.b > 0.0 && second.a > 0.0 && second.b > 0.0) {
        return Vec::new();
    }
    let reach = first.a.max(first.b) + second.a.max(second.b);
    let place = |d: f64| second.at(Point::new(first.centre.x + dir.0 * d, first.centre.y + dir.1 * d));
    let class = |d: f64| lie(first, &place(d));
    let mut out = Vec::new();
    let mut previous = (0.0, class(0.0));
    for step in 1..=SCAN_STEPS {
        let d = reach * 1.05 * step as f64 / SCAN_STEPS as f64;
        let now = class(d);
        if (previous.1 == Lie::Crossing) != (now == Lie::Crossing) {
            // The edge of crossing lies between the two: bisect to the side without crossing.
            let (mut crossing, mut clear) = if now == Lie::Crossing { (d, previous.0) } else { (previous.0, d) };
            for _ in 0..40 {
                let mid = 0.5 * (crossing + clear);
                if class(mid) == Lie::Crossing {
                    crossing = mid;
                } else {
                    clear = mid;
                }
            }
            out.push(clear);
        }
        previous = (d, now);
    }
    out.sort_by(f64::total_cmp);
    out
}
