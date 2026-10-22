//! `JobPlan` -> G-code program (GRBL 1.1, laser mode `$32=1`).
//!
//! Output conventions (all verified against an independent interpreter in the tests, none
//! against a real controller):
//! - Header `G21` (mm), `G90` (absolute), `G94` (feed per minute), optional work coordinate
//!   system, `M5`. Footer `M5`, `M9` (when air was on), `M2`.
//! - Positions are absolute and quantised once per coordinate to the profile's decimals.
//! - Laser off motion is `G0`. Burning motion is `G1` with the modal `S` (power as a ratio of
//!   `$30`) and `F` (`mm/s x 60`, rounded to whole mm/min). `M3`/`M4` is emitted once before the
//!   first burning move; the laser state is then governed by laser mode (`G0` and `S0` are off).
//! - Settings blocks use their **maximum** power; the minimum power of a block has no GRBL
//!   meaning (no speed-dependent ramp) and is not used. Scan overscan is written as laser-off
//!   `G1 S0` run-in/run-out moves.
//! - Whatever the matrix ([`crate::grbl_capabilities`]) or the profile cannot express is refused
//!   with a [`Refusal`]; [`lower`] never leaves such an event out or replaces it.

use std::fmt;

use burnburn_job::{Feature, JobPlan, JobPlanId, MachinePoint, PlanEvent, PlanMode};
use burnburn_types::StableId;

use crate::number;
use crate::{GrblProfile, PowerMode, WorkCoordinate};

/// A lowered program for exactly one plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GcodeProgram {
    plan: JobPlanId,
    lines: Vec<String>,
}

impl GcodeProgram {
    /// The plan this program was lowered from (preview, export and upload name the same id).
    pub fn plan(&self) -> JobPlanId {
        self.plan
    }

    /// The program lines, comments included, without line terminators.
    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    /// The program as a file: every line terminated by `\n`.
    pub fn to_text(&self) -> String {
        let mut t = String::with_capacity(self.lines.iter().map(|l| l.len() + 1).sum());
        for l in &self.lines {
            t.push_str(l);
            t.push('\n');
        }
        t
    }
}

/// Why a plan cannot be expressed on the profile.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum RefusalKind {
    /// The profile does not assume `$32=1`; `M3`/`M4` would burn during `G0`.
    LaserModeRequired,
    /// The plan asks for air assist and the machine has no `M8` output.
    AirAssistUnavailable,
    /// A target (or scan overscan end) lies outside the bed frame.
    OutsideBed,
    InvalidProfile(&'static str),
    /// A feature that is not `Supported` in the GRBL capability matrix.
    Feature(Feature),
    /// Rotary plans have no verified GRBL axis mapping.
    Rotary,
    /// A coordinate or speed that cannot be written as a G-code word.
    NotRepresentable(&'static str),
}

impl fmt::Display for RefusalKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RefusalKind::LaserModeRequired => f.write_str("laser mode ($32=1) is not assumed by the profile"),
            RefusalKind::AirAssistUnavailable => f.write_str("air assist requested but the machine has no M8 output"),
            RefusalKind::OutsideBed => f.write_str("motion outside the bed"),
            RefusalKind::InvalidProfile(what) => write!(f, "invalid GRBL profile: {what}"),
            RefusalKind::Feature(feature) => {
                write!(f, "feature {} is not supported by the GRBL backend", feature.tag())
            }
            RefusalKind::Rotary => f.write_str("rotary is not supported by the GRBL backend"),
            RefusalKind::NotRepresentable(what) => write!(f, "{what} cannot be written as G-code"),
        }
    }
}

/// One reason a plan was refused, with the plan event it belongs to (`None`: the plan or profile
/// as a whole).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    pub event: Option<usize>,
    pub kind: RefusalKind,
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.event {
            Some(e) => write!(f, "event {e}: {}", self.kind),
            None => write!(f, "{}", self.kind),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum LowerError {
    /// Every reason the plan cannot be lowered; nothing was written.
    Refused(Vec<Refusal>),
}

impl fmt::Display for LowerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let LowerError::Refused(all) = self;
        f.write_str("G-code lowering refused:")?;
        for r in all {
            write!(f, " [{r}]")?;
        }
        Ok(())
    }
}

impl std::error::Error for LowerError {}

/// Every reason `plan` cannot be lowered for `profile`, in event order; empty means [`lower`]
/// succeeds. Use it for a preflight report that names everything at once.
pub fn check(plan: &JobPlan, profile: &GrblProfile) -> Vec<Refusal> {
    let mut out = Vec::new();
    let mut refuse = |event: Option<usize>, kind: RefusalKind| out.push(Refusal { event, kind });
    if let Err(crate::ProfileError::OutOfRange(what)) = profile.validate() {
        refuse(None, RefusalKind::InvalidProfile(what));
    }
    if !profile.laser_mode {
        refuse(None, RefusalKind::LaserModeRequired);
    }
    if plan.rotary().is_some() {
        refuse(None, RefusalKind::Rotary);
    }
    let caps = profile.capabilities();
    let need = |event: usize, feature: Feature, out: &mut Vec<Refusal>| {
        if !caps.is_supported(feature) {
            out.push(Refusal { event: Some(event), kind: RefusalKind::Feature(feature) });
        }
    };
    let frame = plan.frame();
    let inside = |event: usize, p: MachinePoint, out: &mut Vec<Refusal>| {
        if !frame.contains(p) {
            out.push(Refusal { event: Some(event), kind: RefusalKind::OutsideBed });
        }
    };
    let representable = |event: usize, p: MachinePoint, out: &mut Vec<Refusal>| {
        if number::quantise(p.x_mm(), profile.decimals.max(1)).is_none()
            || number::quantise(p.y_mm(), profile.decimals.max(1)).is_none()
        {
            out.push(Refusal { event: Some(event), kind: RefusalKind::NotRepresentable("coordinate") });
        }
    };
    let mut buf = Vec::new();
    for (i, e) in plan.events().iter().enumerate() {
        match *e {
            PlanEvent::OperationBoundary { settings, .. } => {
                let Some(s) = plan.settings().get(settings.get()) else { continue };
                if s.air_assist && !profile.air_assist {
                    buf.push(Refusal { event: Some(i), kind: RefusalKind::AirAssistUnavailable });
                }
                match s.mode {
                    PlanMode::Dots { .. } => need(i, Feature::DotMode, &mut buf),
                    PlanMode::GrayscaleScan { .. } => need(i, Feature::GrayscaleModulation, &mut buf),
                    PlanMode::AngledScan { .. } => need(i, Feature::ScanAngle, &mut buf),
                    _ => {}
                }
                if !(s.speed_mm_s * 60.0).round().is_finite() || (s.speed_mm_s * 60.0).round() < 1.0 {
                    buf.push(Refusal { event: Some(i), kind: RefusalKind::NotRepresentable("speed") });
                }
            }
            PlanEvent::Travel { to } | PlanEvent::Cut { to, .. } => {
                inside(i, to, &mut buf);
                representable(i, to, &mut buf);
            }
            PlanEvent::ScanRun { line, .. } | PlanEvent::PowerRun { line, .. } => {
                if matches!(*e, PlanEvent::PowerRun { .. }) {
                    need(i, Feature::GrayscaleModulation, &mut buf);
                }
                let (a, z) = line.overscan_ends();
                for p in [a, z] {
                    inside(i, p, &mut buf);
                    representable(i, p, &mut buf);
                }
            }
            PlanEvent::RampRun { .. } => need(i, Feature::RampLength, &mut buf),
            PlanEvent::Dot { .. } => need(i, Feature::DotMode, &mut buf),
            PlanEvent::FocusOffset { .. } => need(i, Feature::ZStep, &mut buf),
            // Feeder (FE01): GRBL has no feeder axis in the profile; refused, never skipped.
            PlanEvent::Feed { .. } | PlanEvent::Pause { .. } => {
                buf.push(Refusal { event: Some(i), kind: RefusalKind::NotRepresentable("feeder") });
            }
            // A newer event kind this backend does not know can only be refused.
            _ => buf.push(Refusal { event: Some(i), kind: RefusalKind::NotRepresentable("event") }),
        }
    }
    out.append(&mut buf);
    out
}

/// Lowers `plan` to a G-code program for `profile`, or names every reason it cannot.
pub fn lower(plan: &JobPlan, profile: &GrblProfile) -> Result<GcodeProgram, LowerError> {
    let refusals = check(plan, profile);
    if !refusals.is_empty() {
        return Err(LowerError::Refused(refusals));
    }
    let mut w = Writer::new(profile);
    w.header(plan);
    w.body(plan);
    w.footer();
    Ok(GcodeProgram { plan: plan.id(), lines: w.lines })
}

/// Modal state of the writer: what the controller will believe after the lines written so far.
struct Writer<'a> {
    profile: &'a GrblProfile,
    lines: Vec<String>,
    /// Physical head position in quantised units.
    at: (i64, i64),
    /// Where the plan thinks the head is (differs from `at` after a scan run's overscan-out).
    logical: (i64, i64),
    spindle: bool,
    s: Option<u32>,
    f: Option<u32>,
    air: bool,
    feed: u32,
    power_s: u32,
}

impl<'a> Writer<'a> {
    fn new(profile: &'a GrblProfile) -> Self {
        Writer {
            profile,
            lines: Vec::new(),
            at: (0, 0),
            logical: (0, 0),
            spindle: false,
            s: None,
            f: None,
            air: false,
            feed: 0,
            power_s: 0,
        }
    }

    fn q(&self, p: MachinePoint) -> (i64, i64) {
        // `check` has already proven every coordinate representable.
        let d = self.profile.decimals;
        (number::quantise(p.x_mm(), d).unwrap_or(0), number::quantise(p.y_mm(), d).unwrap_or(0))
    }

    fn spindle_s(&self, ratio: f64) -> u32 {
        let max = self.profile.max_spindle;
        ((ratio * f64::from(max)).round() as u32).min(max)
    }

    fn header(&mut self, plan: &JobPlan) {
        self.lines.push(format!("; BurnBurn {}", plan.id()));
        for l in ["G21", "G90", "G94"] {
            self.lines.push(l.into());
        }
        if let Some(wc) = self.profile.work_coordinate {
            self.lines.push(
                match wc {
                    WorkCoordinate::G54 => "G54",
                    WorkCoordinate::G55 => "G55",
                    WorkCoordinate::G56 => "G56",
                    WorkCoordinate::G57 => "G57",
                    WorkCoordinate::G58 => "G58",
                    WorkCoordinate::G59 => "G59",
                }
                .into(),
            );
        }
        self.lines.push("M5".into());
        self.at = self.q(plan.start());
        self.logical = self.at;
    }

    fn body(&mut self, plan: &JobPlan) {
        for e in plan.events() {
            match *e {
                PlanEvent::OperationBoundary { operation, settings, pass } => {
                    let Some(s) = plan.settings().get(settings.get()) else { continue };
                    self.lines.push(format!("; operation {} pass {pass}", operation.raw()));
                    if s.air_assist != self.air {
                        self.lines.push(if s.air_assist { "M8" } else { "M9" }.into());
                        self.air = s.air_assist;
                    }
                    self.feed = ((s.speed_mm_s * 60.0).round() as u32).max(1);
                    self.power_s = self.spindle_s(s.max_power.get());
                }
                PlanEvent::Travel { to } => {
                    let to = self.q(to);
                    self.rapid(to);
                    self.logical = to;
                }
                PlanEvent::Cut { to, .. } => {
                    let to = self.q(to);
                    self.sync();
                    self.burn(to, self.power_s);
                    self.logical = to;
                }
                PlanEvent::ScanRun { line, .. } => self.scan(line, self.power_s),
                PlanEvent::PowerRun { line, power, .. } => {
                    let s = self.spindle_s(power.get());
                    self.scan(line, s);
                }
                // Refused by `check`; unreachable for a checked plan.
                _ => {}
            }
        }
    }

    fn footer(&mut self) {
        self.lines.push("M5".into());
        if self.air {
            self.lines.push("M9".into());
        }
        self.lines.push("M2".into());
    }

    /// Brings the physical head to where the plan believes it is (after an overscan-out).
    fn sync(&mut self) {
        let target = self.logical;
        self.rapid(target);
    }

    fn axes(&self, from: (i64, i64), to: (i64, i64)) -> String {
        let d = self.profile.decimals;
        let mut out = String::new();
        if to.0 != from.0 {
            out.push_str(&format!(" X{}", number::format(to.0, d)));
        }
        if to.1 != from.1 {
            out.push_str(&format!(" Y{}", number::format(to.1, d)));
        }
        out
    }

    fn rapid(&mut self, to: (i64, i64)) {
        if to == self.at {
            return;
        }
        self.lines.push(format!("G0{}", self.axes(self.at, to)));
        self.at = to;
    }

    /// A feed move with the laser at spindle value `s` (`0`: off).
    fn feed_move(&mut self, to: (i64, i64), s: u32) {
        if to == self.at {
            return;
        }
        let mut line = format!("G1{}", self.axes(self.at, to));
        if s > 0 && !self.spindle {
            self.lines.push(format!("{} S0", if self.profile.power_mode == PowerMode::Dynamic { "M4" } else { "M3" }));
            self.spindle = true;
            self.s = Some(0);
        }
        // With the spindle off there is no laser to switch; with it on S is explicit.
        if self.spindle && self.s != Some(s) {
            line.push_str(&format!(" S{s}"));
            self.s = Some(s);
        }
        if self.f != Some(self.feed) {
            line.push_str(&format!(" F{}", self.feed));
            self.f = Some(self.feed);
        }
        self.lines.push(line);
        self.at = to;
    }

    fn burn(&mut self, to: (i64, i64), s: u32) {
        self.feed_move(to, s);
    }

    fn scan(&mut self, line: burnburn_job::ScanLine, s: u32) {
        let (a, z) = line.overscan_ends();
        let (from, to) = (self.q(line.from), self.q(line.to));
        let (a, z) = (self.q(a), self.q(z));
        self.sync();
        if a != from {
            self.rapid(a);
            self.feed_move(from, 0);
        }
        self.burn(to, s);
        if z != to {
            self.feed_move(z, 0);
        }
        self.logical = to;
    }
}
