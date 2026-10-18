use std::fmt;

use burnburn_geometry::{Bounds, Point};
use burnburn_types::{CanonicalWriter, ContentHash, HashDomain, RevisionId, StableId};

use crate::{
    MachineFrame, MachinePoint, OperationSettings, PlanError, PlanEvent, PlanMode, ProfileRevision, ScanAxis,
    SettingsIndex,
};

/// Content address of a plan: hash of its canonical content (no addresses, no log time,
/// no session revision). Same compiler input → same id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct JobPlanId(ContentHash);

impl JobPlanId {
    pub fn hash(&self) -> ContentHash {
        self.0
    }
}

impl fmt::Display for JobPlanId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "plan-{}", self.0)
    }
}

/// What the plan was compiled from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanSource {
    /// Session revision of the document snapshot (provenance and staleness check; not part
    /// of the plan id, because revisions are session-local).
    pub document_revision: RevisionId,
    /// Canonical hash of the compiled document content (geometry and process layer).
    pub source_hash: ContentHash,
    pub machine_profile: ProfileRevision,
    pub compiler_version: String,
}

/// The fixed units of every plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanUnits {
    /// Positions in mm, speeds in mm/s, power as a ratio of rated power.
    MillimetreSecond,
}

/// Immutable compiled plan. See the crate documentation.
///
/// There is no public mutation after [`JobPlanBuilder::finish`]: fields are private and every
/// accessor takes `&self` (`plan_has_no_public_mutation_after_finish`):
///
/// ```compile_fail,E0616
/// fn tamper(plan: &mut burnburn_job::JobPlan) {
///     plan.events.clear(); // error[E0616]: field `events` is private
/// }
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct JobPlan {
    id: JobPlanId,
    source: PlanSource,
    frame: MachineFrame,
    start: MachinePoint,
    settings: Vec<OperationSettings>,
    events: Vec<PlanEvent>,
}

impl JobPlan {
    pub fn id(&self) -> JobPlanId {
        self.id
    }
    pub fn source(&self) -> &PlanSource {
        &self.source
    }
    pub fn frame(&self) -> &MachineFrame {
        &self.frame
    }
    pub fn units(&self) -> PlanUnits {
        PlanUnits::MillimetreSecond
    }
    /// Head position the plan starts from (machine frame).
    pub fn start(&self) -> MachinePoint {
        self.start
    }
    pub fn settings(&self) -> &[OperationSettings] {
        &self.settings
    }
    pub fn events(&self) -> &[PlanEvent] {
        &self.events
    }

    /// Bounds of every position the head reaches (travel targets, both ends of every cut, scan
    /// runs and their overscan). The start position counts only when a cut begins there.
    /// `None` for a plan without motion.
    pub fn bounds(&self) -> Option<Bounds> {
        let mut b: Option<Bounds> = None;
        let mut add = |p: MachinePoint| {
            let p = Point::new(p.x_mm(), p.y_mm());
            match b.as_mut() {
                Some(b) => b.include(p),
                None => b = Some(Bounds::from_point(p)),
            }
        };
        let mut at = self.start;
        for e in &self.events {
            match *e {
                PlanEvent::Travel { to } => {
                    add(to);
                    at = to;
                }
                // A cut burns from the current position: include it (it may be the start).
                PlanEvent::Cut { to, .. } => {
                    add(at);
                    add(to);
                    at = to;
                }
                PlanEvent::ScanRun { line, .. }
                | PlanEvent::PowerRun { line, .. }
                | PlanEvent::RampRun { line, .. } => {
                    let (a, z) = line.overscan_ends();
                    add(a);
                    add(z);
                    at = line.to;
                }
                // A pulse burns where the head is: that position is part of the bounds.
                PlanEvent::Dot { .. } => add(at),
                PlanEvent::OperationBoundary { .. } | PlanEvent::FocusOffset { .. } => {}
            }
        }
        b
    }
}

/// Builds and validates a plan; [`JobPlanBuilder::finish`] freezes it.
#[derive(Debug)]
pub struct JobPlanBuilder {
    source: PlanSource,
    frame: MachineFrame,
    start: MachinePoint,
    settings: Vec<OperationSettings>,
    events: Vec<PlanEvent>,
    current: Option<SettingsIndex>,
    position: MachinePoint,
}

impl JobPlanBuilder {
    pub fn new(source: PlanSource, frame: MachineFrame, start: MachinePoint) -> Self {
        JobPlanBuilder {
            source,
            frame,
            start,
            settings: Vec::new(),
            events: Vec::new(),
            current: None,
            position: start,
        }
    }

    /// Registers a settings block and returns its index.
    pub fn add_settings(&mut self, s: OperationSettings) -> Result<SettingsIndex, PlanError> {
        s.validate()?;
        let i = u32::try_from(self.settings.len()).map_err(|_| PlanError::TooManySettings)?;
        self.settings.push(s);
        Ok(SettingsIndex(i))
    }

    /// Current head position (after the last pushed event).
    pub fn position(&self) -> MachinePoint {
        self.position
    }

    /// Appends one event after checking it against the settings it runs under.
    pub fn push(&mut self, event: PlanEvent) -> Result<(), PlanError> {
        let mode = |b: &Self| -> Result<PlanMode, PlanError> {
            let i = b.current.ok_or(PlanError::MotionOutsideOperation)?;
            b.settings.get(i.get()).map(|s| s.mode).ok_or(PlanError::UnknownSettings(i))
        };
        match event {
            PlanEvent::OperationBoundary { settings, .. } => {
                if self.settings.get(settings.get()).is_none() {
                    return Err(PlanError::UnknownSettings(settings));
                }
                self.current = Some(settings);
            }
            PlanEvent::Travel { to } => self.position = to,
            PlanEvent::Cut { to, .. } => {
                if mode(self)? != PlanMode::Line {
                    return Err(PlanError::EventDoesNotMatchMode);
                }
                self.position = to;
            }
            PlanEvent::ScanRun { line, .. } | PlanEvent::PowerRun { line, .. } | PlanEvent::RampRun { line, .. } => {
                let m = mode(self)?;
                let fits = match event {
                    PlanEvent::ScanRun { .. } | PlanEvent::RampRun { .. } => m.is_binary_scan(),
                    _ => matches!(m, PlanMode::GrayscaleScan { .. }),
                };
                if !fits {
                    return Err(PlanError::EventDoesNotMatchMode);
                }
                if let PlanEvent::RampRun { ramp_mm, .. } = event
                    && !(ramp_mm.is_finite() && ramp_mm >= 0.0)
                {
                    return Err(PlanError::OutOfRange("ramp"));
                }
                if !(line.overscan_mm.is_finite() && line.overscan_mm >= 0.0) {
                    return Err(PlanError::OutOfRange("overscan"));
                }
                if line.from != self.position {
                    return Err(PlanError::ScanRunNotAtCurrentPosition);
                }
                self.position = line.to;
            }
            PlanEvent::FocusOffset { z_mm } => {
                mode(self)?;
                if !z_mm.is_finite() {
                    return Err(PlanError::NotFinite);
                }
            }
            PlanEvent::Dot { .. } => {
                if !matches!(mode(self)?, PlanMode::Dots { .. }) {
                    return Err(PlanError::EventDoesNotMatchMode);
                }
            }
        }
        self.events.push(event);
        Ok(())
    }

    pub fn finish(self) -> Result<JobPlan, PlanError> {
        let id = JobPlanId(plan_hash(&canonical_bytes(&self)?));
        Ok(JobPlan {
            id,
            source: self.source,
            frame: self.frame,
            start: self.start,
            settings: self.settings,
            events: self.events,
        })
    }
}

fn point(w: &mut CanonicalWriter, p: MachinePoint) {
    w.f64(p.x_mm());
    w.f64(p.y_mm());
}

/// Canonical plan content. Layout: `tag("units-mm-s")`; frame (`f64` width, `f64` height,
/// origin tag); source (`bytes` source hash, `str` profile id, `u64` profile revision, `bytes`
/// profile content hash, `str`
/// compiler version — not the session revision); start point; `u64` settings count, each
/// settings block; `u64` event count, each event with a tag.
fn canonical_bytes(b: &JobPlanBuilder) -> Result<Vec<u8>, PlanError> {
    let mut w = CanonicalWriter::new();
    w.tag("units-mm-s");
    w.f64(b.frame.width_mm());
    w.f64(b.frame.height_mm());
    w.tag(crate::profile::origin_tag(b.frame.origin()));
    w.bytes(b.source.source_hash.as_bytes());
    w.str(&b.source.machine_profile.id);
    w.u64(u64::from(b.source.machine_profile.revision));
    w.bytes(b.source.machine_profile.content.as_bytes());
    w.str(&b.source.compiler_version);
    point(&mut w, b.start);
    w.u64(b.settings.len() as u64);
    for s in &b.settings {
        match s.mode {
            PlanMode::Line => w.tag("line"),
            PlanMode::Scan { interval_mm, axis, bidirectional } => {
                w.tag("scan");
                w.f64(interval_mm);
                w.tag(match axis {
                    ScanAxis::X => "x",
                    ScanAxis::Y => "y",
                });
                w.bool(bidirectional);
            }
            PlanMode::AngledScan { interval_mm, angle_deg, bidirectional } => {
                w.tag("angled-scan");
                w.f64(interval_mm);
                w.f64(angle_deg);
                w.bool(bidirectional);
            }
            PlanMode::GrayscaleScan { interval_mm, bidirectional } => {
                w.tag("grayscale-scan");
                w.f64(interval_mm);
                w.bool(bidirectional);
            }
            PlanMode::Dots { dwell_ms } => {
                w.tag("dots");
                w.f64(dwell_ms);
            }
        }
        w.f64(s.speed_mm_s);
        w.f64(s.min_power.get());
        w.f64(s.max_power.get());
        w.bool(s.air_assist);
    }
    w.u64(b.events.len() as u64);
    for e in &b.events {
        let origin = |w: &mut CanonicalWriter, o: Option<crate::SourceRef>| match o {
            None => w.u64(0),
            Some(o) => {
                w.u64(1);
                w.u64(o.geometry.raw());
                w.u64(o.operation.raw());
            }
        };
        match *e {
            PlanEvent::OperationBoundary { operation, settings, pass } => {
                w.tag("operation");
                w.u64(operation.raw());
                w.u64(u64::from(settings.0));
                w.u64(u64::from(pass));
            }
            PlanEvent::Travel { to } => {
                w.tag("travel");
                point(&mut w, to);
            }
            PlanEvent::Cut { to, origin: o } => {
                w.tag("cut");
                point(&mut w, to);
                origin(&mut w, o);
            }
            PlanEvent::ScanRun { line, origin: o } => {
                w.tag("scan-run");
                point(&mut w, line.from);
                point(&mut w, line.to);
                w.f64(line.overscan_mm);
                origin(&mut w, o);
            }
            PlanEvent::PowerRun { line, power, origin: o } => {
                w.tag("power-run");
                point(&mut w, line.from);
                point(&mut w, line.to);
                w.f64(line.overscan_mm);
                w.f64(power.get());
                origin(&mut w, o);
            }
            PlanEvent::FocusOffset { z_mm } => {
                w.tag("focus-offset");
                w.f64(z_mm);
            }
            PlanEvent::Dot { origin: o } => {
                w.tag("dot");
                origin(&mut w, o);
            }
            PlanEvent::RampRun { line, ramp_mm, origin: o } => {
                w.tag("ramp-run");
                point(&mut w, line.from);
                point(&mut w, line.to);
                w.f64(line.overscan_mm);
                w.f64(ramp_mm);
                origin(&mut w, o);
            }
        }
    }
    w.finish().map_err(|_| PlanError::NotFinite)
}

pub(crate) fn plan_hash(payload: &[u8]) -> ContentHash {
    ContentHash::of(HashDomain::Plan, payload)
}
