//! Machine calibration (IM29, W09): measured values per machine and the test patterns that
//! produce them. Patterns are ordinary frozen [`JobPlan`]s; the user measures the burned result
//! and enters the value. Every calibration change yields a new setup [`ProfileRevision`], so
//! jobs prepared for the old revision become stale.

use std::collections::BTreeMap;
use std::fmt;

use burnburn_job::{
    JobPlan, JobPlanBuilder, MachinePoint, MachineProfile, OperationSettings, PlanError, PlanEvent, PlanMode,
    PlanSource, PowerRatio, ProfileRevision, ScanAxis, ScanLine,
};
use burnburn_types::{CanonicalWriter, ContentHash, HashDomain, OperationId, RevisionId, StableId};

/// Recorded as the compiler of every calibration pattern (part of the plan id).
pub const CALIBRATION_GENERATOR_VERSION: &str = concat!("burnburn-machine calibration ", env!("CARGO_PKG_VERSION"));

/// Material identity for kerf values (e.g. "plywood-3mm").
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MaterialKey(pub String);

/// One scan-offset measurement: at `speed_mm_s` the return rows are shifted by `offset_mm`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScanOffsetPoint {
    pub speed_mm_s: f64,
    pub offset_mm: f64,
}

/// Measured values. Separate scopes: kerf per material, scan offset per speed, pointer offset
/// per machine (W09).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CalibrationSet {
    pub kerf_mm: BTreeMap<MaterialKey, f64>,
    /// Sorted by speed; at most one point per speed.
    pub scan_offset: Vec<ScanOffsetPoint>,
    /// Red-dot pointer position relative to the beam (x, y in mm).
    pub pointer_offset_mm: Option<[f64; 2]>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CalibrationError {
    NotFinite,
    /// Values out of their meaningful range (negative kerf, non-positive speed …).
    OutOfRange(&'static str),
    DuplicateSpeed(f64),
    /// A pattern point lies outside the bed.
    OutsideBed {
        x_mm: f64,
        y_mm: f64,
    },
    Plan(PlanError),
}

impl From<PlanError> for CalibrationError {
    fn from(e: PlanError) -> Self {
        CalibrationError::Plan(e)
    }
}

impl fmt::Display for CalibrationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for CalibrationError {}

/// A machine profile together with its calibration: the "setup" a job is prepared for.
#[derive(Debug, Clone, PartialEq)]
pub struct CalibratedProfile {
    profile: MachineProfile,
    calibration: CalibrationSet,
    revision: ProfileRevision,
}

impl CalibratedProfile {
    pub fn new(profile: MachineProfile, mut calibration: CalibrationSet) -> Result<Self, CalibrationError> {
        let finite = |v: f64| if v.is_finite() { Ok(v) } else { Err(CalibrationError::NotFinite) };
        for &k in calibration.kerf_mm.values() {
            if finite(k)? < 0.0 {
                return Err(CalibrationError::OutOfRange("kerf"));
            }
        }
        for p in &calibration.scan_offset {
            finite(p.offset_mm)?;
            if !(finite(p.speed_mm_s)? > 0.0) {
                return Err(CalibrationError::OutOfRange("scan offset speed"));
            }
        }
        calibration.scan_offset.sort_by(|a, b| a.speed_mm_s.total_cmp(&b.speed_mm_s));
        if let Some(w) = calibration.scan_offset.windows(2).find(|w| w[0].speed_mm_s == w[1].speed_mm_s) {
            return Err(CalibrationError::DuplicateSpeed(w[0].speed_mm_s));
        }
        if let Some([x, y]) = calibration.pointer_offset_mm {
            finite(x)?;
            finite(y)?;
        }
        // Canonical setup (domain `profile`): `tag("calibrated-profile")`, `bytes` profile content,
        // `u64` kerf count + (`str` material, `f64` mm) in key order, `u64` scan count +
        // (`f64` speed, `f64` offset) by speed, `bool` pointer present + (`f64` x, `f64` y).
        let mut w = CanonicalWriter::new();
        w.tag("calibrated-profile");
        w.bytes(profile.revision().content.as_bytes());
        w.u64(calibration.kerf_mm.len() as u64);
        for (m, k) in &calibration.kerf_mm {
            w.str(&m.0);
            w.f64(*k);
        }
        w.u64(calibration.scan_offset.len() as u64);
        for p in &calibration.scan_offset {
            w.f64(p.speed_mm_s);
            w.f64(p.offset_mm);
        }
        w.bool(calibration.pointer_offset_mm.is_some());
        if let Some([x, y]) = calibration.pointer_offset_mm {
            w.f64(x);
            w.f64(y);
        }
        let content = ContentHash::of(HashDomain::Profile, &w.finish().map_err(|_| CalibrationError::NotFinite)?);
        let base = profile.revision();
        let revision = ProfileRevision { id: base.id.clone(), revision: base.revision, content };
        Ok(CalibratedProfile { profile, calibration, revision })
    }

    pub fn profile(&self) -> &MachineProfile {
        &self.profile
    }

    pub fn calibration(&self) -> &CalibrationSet {
        &self.calibration
    }

    /// Setup revision: profile content plus every calibration value.
    pub fn revision(&self) -> &ProfileRevision {
        &self.revision
    }

    /// A new setup with one changed calibration (the old one stays valid for old jobs).
    pub fn with_calibration(&self, calibration: CalibrationSet) -> Result<Self, CalibrationError> {
        CalibratedProfile::new(self.profile.clone(), calibration)
    }

    /// True when a job prepared for `prepared` may run on this setup unchanged.
    pub fn is_current_for(&self, prepared: &ProfileRevision) -> bool {
        prepared.id == self.revision.id && prepared.content == self.revision.content
    }
}

/// Laser settings of a test pattern.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PatternLaser {
    pub speed_mm_s: f64,
    pub power: PowerRatio,
}

/// Pointer-offset test: a cross cut centred on `at` (bed = machine frame, mm).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PointerOffsetTest {
    pub at: [f64; 2],
    pub arm_mm: f64,
    pub laser: PatternLaser,
}

/// Scan-offset test: per speed one block of `rows` bidirectional scan rows of `length_mm`.
#[derive(Debug, Clone, PartialEq)]
pub struct ScanOffsetTest {
    pub origin: [f64; 2],
    pub speeds_mm_s: Vec<f64>,
    pub power: PowerRatio,
    pub length_mm: f64,
    pub rows: u32,
    pub interval_mm: f64,
    /// Vertical gap between speed blocks.
    pub gap_mm: f64,
}

/// Kerf test: `count` squares of nominal `size_mm` in a row, `gap_mm` apart.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KerfTest {
    pub origin: [f64; 2],
    pub size_mm: f64,
    pub count: u32,
    pub gap_mm: f64,
    pub laser: PatternLaser,
}

pub fn pointer_offset_plan(profile: &MachineProfile, test: &PointerOffsetTest) -> Result<JobPlan, CalibrationError> {
    let [x, y] = test.at;
    let a = test.arm_mm;
    positive(a, "arm")?;
    let mut p = Pattern::new(profile, "pointer-offset", &[x, y, a, test.laser.speed_mm_s, test.laser.power.get()])?;
    p.block(line_settings(test.laser))?;
    p.travel(x - a, y)?;
    p.cut(x + a, y)?;
    p.travel(x, y - a)?;
    p.cut(x, y + a)?;
    p.finish()
}

pub fn scan_offset_plan(profile: &MachineProfile, test: &ScanOffsetTest) -> Result<JobPlan, CalibrationError> {
    let [x0, y0] = test.origin;
    positive(test.length_mm, "length")?;
    positive(test.interval_mm, "interval")?;
    if test.rows == 0 || test.speeds_mm_s.is_empty() || !(test.gap_mm.is_finite() && test.gap_mm >= 0.0) {
        return Err(CalibrationError::OutOfRange("scan pattern"));
    }
    let mut params =
        vec![x0, y0, test.power.get(), test.length_mm, f64::from(test.rows), test.interval_mm, test.gap_mm];
    params.extend(&test.speeds_mm_s);
    let mut p = Pattern::new(profile, "scan-offset", &params)?;
    let block_height = f64::from(test.rows) * test.interval_mm + test.gap_mm;
    for (b, &speed) in test.speeds_mm_s.iter().enumerate() {
        positive(speed, "speed")?;
        p.block(OperationSettings {
            mode: PlanMode::Scan { interval_mm: test.interval_mm, axis: ScanAxis::X, bidirectional: true },
            speed_mm_s: speed,
            min_power: test.power,
            max_power: test.power,
            air_assist: false,
        })?;
        for r in 0..test.rows {
            let y = y0 + b as f64 * block_height + f64::from(r) * test.interval_mm;
            let (from, to) = if r % 2 == 0 { (x0, x0 + test.length_mm) } else { (x0 + test.length_mm, x0) };
            p.travel(from, y)?;
            p.scan(to, y)?;
        }
    }
    p.finish()
}

pub fn kerf_test_plan(profile: &MachineProfile, test: &KerfTest) -> Result<JobPlan, CalibrationError> {
    let [x0, y0] = test.origin;
    let s = test.size_mm;
    positive(s, "size")?;
    if test.count == 0 || !(test.gap_mm.is_finite() && test.gap_mm >= 0.0) {
        return Err(CalibrationError::OutOfRange("kerf pattern"));
    }
    let params = [x0, y0, s, f64::from(test.count), test.gap_mm, test.laser.speed_mm_s, test.laser.power.get()];
    let mut p = Pattern::new(profile, "kerf", &params)?;
    p.block(line_settings(test.laser))?;
    for i in 0..test.count {
        let x = x0 + f64::from(i) * (s + test.gap_mm);
        p.travel(x, y0)?;
        for (cx, cy) in [(x + s, y0), (x + s, y0 + s), (x, y0 + s), (x, y0)] {
            p.cut(cx, cy)?;
        }
    }
    p.finish()
}

fn positive(v: f64, what: &'static str) -> Result<f64, CalibrationError> {
    if v.is_finite() && v > 0.0 { Ok(v) } else { Err(CalibrationError::OutOfRange(what)) }
}

fn line_settings(laser: PatternLaser) -> OperationSettings {
    OperationSettings {
        mode: PlanMode::Line,
        speed_mm_s: laser.speed_mm_s,
        min_power: laser.power,
        max_power: laser.power,
        air_assist: false,
    }
}

/// Small builder that checks every point against the bed.
struct Pattern<'a> {
    profile: &'a MachineProfile,
    b: JobPlanBuilder,
    blocks: u64,
}

impl<'a> Pattern<'a> {
    fn new(profile: &'a MachineProfile, kind: &str, params: &[f64]) -> Result<Self, CalibrationError> {
        let mut w = CanonicalWriter::new();
        w.tag("calibration-pattern");
        w.tag(kind);
        w.u64(params.len() as u64);
        for &v in params {
            w.f64(v);
        }
        let source = PlanSource {
            document_revision: RevisionId::INITIAL,
            source_hash: ContentHash::of(HashDomain::Plan, &w.finish().map_err(|_| CalibrationError::NotFinite)?),
            machine_profile: profile.revision().clone(),
            compiler_version: CALIBRATION_GENERATOR_VERSION.to_owned(),
        };
        let b = JobPlanBuilder::new(source, *profile.frame(), MachinePoint::new(0.0, 0.0)?);
        Ok(Pattern { profile, b, blocks: 0 })
    }

    fn point(&self, x: f64, y: f64) -> Result<MachinePoint, CalibrationError> {
        let p = MachinePoint::new(x, y).map_err(|_| CalibrationError::NotFinite)?;
        if self.profile.frame().contains(p) { Ok(p) } else { Err(CalibrationError::OutsideBed { x_mm: x, y_mm: y }) }
    }

    fn block(&mut self, s: OperationSettings) -> Result<(), CalibrationError> {
        let settings = self.b.add_settings(s)?;
        self.blocks += 1;
        // Pattern blocks are not document operations; they are numbered from 1.
        let operation = OperationId::from_raw(self.blocks);
        self.b.push(PlanEvent::OperationBoundary { operation, settings, pass: 0 })?;
        Ok(())
    }

    fn travel(&mut self, x: f64, y: f64) -> Result<(), CalibrationError> {
        let to = self.point(x, y)?;
        if to != self.b.position() {
            self.b.push(PlanEvent::Travel { to })?;
        }
        Ok(())
    }

    fn cut(&mut self, x: f64, y: f64) -> Result<(), CalibrationError> {
        let to = self.point(x, y)?;
        self.b.push(PlanEvent::Cut { to, origin: None })?;
        Ok(())
    }

    fn scan(&mut self, x: f64, y: f64) -> Result<(), CalibrationError> {
        let to = self.point(x, y)?;
        let from = self.b.position();
        self.b.push(PlanEvent::ScanRun { line: ScanLine { from, to, overscan_mm: 0.0 }, origin: None })?;
        Ok(())
    }

    fn finish(self) -> Result<JobPlan, CalibrationError> {
        Ok(self.b.finish()?)
    }
}
