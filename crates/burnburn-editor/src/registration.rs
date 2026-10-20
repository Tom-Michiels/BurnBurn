//! Print and Cut registration and Center Finder flows (W2-L2, IM27): who has been measured, which
//! transform is active. No transform survives a reset, a changed target or a failed computation.

use burnburn_geometry::{
    Affine, CircleFit, CircleFitError, Point, Registration, RegistrationError, RegistrationOptions, fit_circle,
    register,
};

#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum RegistrationFlowError {
    /// No design targets have been set.
    NoDesignTargets,
    NoSuchTarget(usize),
    /// Target `n` has not been measured yet.
    MissingMeasurement(usize),
    Registration(RegistrationError),
}

/// Registers design targets against measured machine positions.
#[derive(Debug, Clone, PartialEq)]
pub struct RegistrationSession {
    options: RegistrationOptions,
    design: Vec<Point>,
    measured: Vec<Option<Point>>,
    active: Option<Registration>,
}

impl RegistrationSession {
    pub fn new(options: RegistrationOptions) -> Self {
        RegistrationSession { options, design: Vec::new(), measured: Vec::new(), active: None }
    }

    /// Replaces the design targets; measurements and any active transform are dropped.
    pub fn set_design_targets(&mut self, targets: Vec<Point>) {
        self.measured = vec![None; targets.len()];
        self.design = targets;
        self.active = None;
    }

    /// Records where target `index` was found on the machine; an active transform is dropped.
    pub fn set_measured(&mut self, index: usize, at: Point) -> Result<(), RegistrationFlowError> {
        let slot = self.measured.get_mut(index).ok_or(RegistrationFlowError::NoSuchTarget(index))?;
        *slot = Some(at);
        self.active = None;
        Ok(())
    }

    pub fn design_targets(&self) -> &[Point] {
        &self.design
    }

    /// Fits the transform once every target is measured. On failure no transform stays active.
    pub fn compute(&mut self) -> Result<&Registration, RegistrationFlowError> {
        self.active = None;
        if self.design.is_empty() {
            return Err(RegistrationFlowError::NoDesignTargets);
        }
        let mut measured = Vec::with_capacity(self.design.len());
        for (i, m) in self.measured.iter().enumerate() {
            measured.push(m.ok_or(RegistrationFlowError::MissingMeasurement(i))?);
        }
        let registration =
            register(&self.design, &measured, &self.options).map_err(RegistrationFlowError::Registration)?;
        Ok(self.active.insert(registration))
    }

    /// The transform in force, if any.
    pub fn transform(&self) -> Option<&Affine> {
        self.active.as_ref().map(|r| &r.transform)
    }

    pub fn registration(&self) -> Option<&Registration> {
        self.active.as_ref()
    }

    /// Forgets targets, measurements and the transform.
    pub fn reset(&mut self) {
        self.design.clear();
        self.measured.clear();
        self.active = None;
    }
}

/// Collects measured edge points for the Center Finder.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CenterFinderSession {
    points: Vec<Point>,
}

impl CenterFinderSession {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_point(&mut self, p: Point) {
        self.points.push(p);
    }

    /// Drops the most recent point (a mis-measurement).
    pub fn undo_last(&mut self) {
        self.points.pop();
    }

    pub fn clear(&mut self) {
        self.points.clear();
    }

    pub fn points(&self) -> &[Point] {
        &self.points
    }

    /// How many more points are needed before a circle can be fitted.
    pub fn points_needed(&self) -> usize {
        3usize.saturating_sub(self.points.len())
    }

    pub fn fit(&self) -> Result<CircleFit, CircleFitError> {
        fit_circle(&self.points)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use burnburn_geometry::ScaleMode;

    fn p(x: f64, y: f64) -> Point {
        Point::new(x, y)
    }

    fn options() -> RegistrationOptions {
        RegistrationOptions { scale: ScaleMode::Fixed, min_separation: 1.0 }
    }

    fn ready() -> RegistrationSession {
        let mut s = RegistrationSession::new(options());
        s.set_design_targets(vec![p(0.0, 0.0), p(100.0, 0.0)]);
        s.set_measured(0, p(10.0, 5.0)).unwrap();
        s.set_measured(1, p(110.0, 5.0)).unwrap();
        s
    }

    #[test]
    fn a_registration_needs_every_target_measured_and_then_places_the_design() {
        let mut s = RegistrationSession::new(options());
        assert_eq!(s.compute().err(), Some(RegistrationFlowError::NoDesignTargets));
        s.set_design_targets(vec![p(0.0, 0.0), p(100.0, 0.0)]);
        s.set_measured(0, p(10.0, 5.0)).unwrap();
        assert_eq!(s.compute().err(), Some(RegistrationFlowError::MissingMeasurement(1)));
        assert!(s.transform().is_none(), "nothing is active before the registration succeeds");
        s.set_measured(1, p(110.0, 5.0)).unwrap();
        let reg = s.compute().unwrap();
        assert!(reg.max_residual() < 1e-9);
        let t = s.transform().unwrap();
        assert!(t.apply(p(50.0, 0.0)).distance(p(60.0, 5.0)) < 1e-9);
        assert_eq!(s.set_measured(2, p(0.0, 0.0)), Err(RegistrationFlowError::NoSuchTarget(2)));
    }

    #[test]
    fn reset_leaves_no_transform_and_no_targets() {
        let mut s = ready();
        s.compute().unwrap();
        assert!(s.transform().is_some());
        s.reset();
        assert!(s.transform().is_none() && s.registration().is_none());
        assert!(s.design_targets().is_empty());
        assert_eq!(s.compute().err(), Some(RegistrationFlowError::NoDesignTargets));
        assert!(s.transform().is_none());
    }

    #[test]
    fn changing_a_target_or_a_measurement_drops_the_transform_it_was_computed_from() {
        let mut s = ready();
        s.compute().unwrap();
        s.set_measured(1, p(111.0, 5.0)).unwrap();
        assert!(s.transform().is_none(), "a new measurement makes the old transform stale");
        s.compute().unwrap();
        s.set_design_targets(vec![p(0.0, 0.0), p(80.0, 0.0)]);
        assert!(s.transform().is_none());
        assert_eq!(
            s.compute().err(),
            Some(RegistrationFlowError::MissingMeasurement(0)),
            "measurements are dropped too"
        );
    }

    #[test]
    fn a_failed_registration_leaves_no_transform_behind() {
        let mut s = ready();
        s.compute().unwrap();
        // The operator measured the same spot twice: the old transform must not survive.
        s.set_measured(1, p(10.2, 5.0)).unwrap();
        assert_eq!(
            s.compute().err(),
            Some(RegistrationFlowError::Registration(RegistrationError::MeasuredTargetsTooClose))
        );
        assert!(s.transform().is_none());
    }

    #[test]
    fn the_center_finder_asks_for_three_points_and_reports_the_fit() {
        let mut c = CenterFinderSession::new();
        assert_eq!(c.points_needed(), 3);
        assert_eq!(c.fit().err(), Some(CircleFitError::TooFew));
        c.add_point(p(15.0, 5.0));
        c.add_point(p(5.0, 15.0));
        assert_eq!(c.points_needed(), 1);
        c.add_point(p(-5.0, 5.0));
        assert_eq!(c.points_needed(), 0);
        let fit = c.fit().unwrap();
        assert!(fit.circle.center.distance(p(5.0, 5.0)) < 1e-9 && (fit.circle.radius - 10.0).abs() < 1e-9);
        // A fourth point off the circle shows up as a residual.
        c.add_point(p(5.0, -4.5));
        assert!(c.fit().unwrap().max_abs_residual() > 0.1);
        c.undo_last();
        assert!(c.fit().unwrap().max_abs_residual() < 1e-9);
        c.clear();
        assert!(c.points().is_empty());
        assert_eq!(c.points_needed(), 3);
    }

    #[test]
    fn collinear_marks_give_no_valid_registration() {
        // Story W05 at the flow level: three marks on one line never produce a centre.
        let mut c = CenterFinderSession::new();
        for q in [p(0.0, 0.0), p(5.0, 5.0), p(10.0, 10.0)] {
            c.add_point(q);
        }
        assert_eq!(c.fit().err(), Some(CircleFitError::Collinear));
        c.undo_last();
        c.add_point(p(10.0, 0.0));
        assert!(c.fit().is_ok(), "moving one mark off the line gives a circle");
    }
}
