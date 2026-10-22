//! `JobPlan` → RD command stream (DR-09; owner CAM/job role).
//!
//! The lowering is the only place where plan millimetres become controller micrometres. It
//! uses only RD-confirmed motion opcodes (`88` absolute travel, `A8` absolute cut, `8B`
//! relative Y travel, `AA` relative X cut; see `02-binaire-specificatie.md` §4) and the typed
//! header/part records that occur in the LightBurn fixtures. Records whose meaning is not
//! established (`F0`, `F1 xx`, `F2 xx`, `E7 04/05/06/08/23/24/37/38/54/55/60`, `DA 01 06 20`,
//! through-power, laser-2 powers) are **not** written: a controller accepting this reduced
//! envelope is unverified on hardware.
//!
//! A rotary plan (profile setting, M08) holds surface millimetres; the Y axis is scaled to the
//! controller's flat-bed millimetres (and mirrored when configured) just before quantisation.
//!
//! Quantisation: round-half-away-from-zero to µm (`f64::round`) on absolute positions;
//! relative moves are differences of already quantised absolute points, so rounding never
//! accumulates. Power: `round(ratio × 16383)` (matches 1 %, 20 % and 60 % in the fixtures; the
//! general LightBurn formula is not identified). Speed: `round(mm/s × 1000)` µm/s.

use std::fmt;

use burnburn_job::{JobPlan, JobPlanId, PlanEvent, PlanMode, ScanAxis};

use crate::command::{BoundsKind, Command, Laser, ModeFlag, PartBoundsKind, PowerBound, ProcessAction, RefPoint};
use crate::encode::EncodeError;
use crate::field::{AbsCoord, Byte7, FieldError, Power, RelCoord, Speed, U35};
use crate::file::encode_rd_file;
use crate::scramble::Magic;

/// What the controller does with scan overscan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverscanPolicy {
    /// The controller performs run-up/run-out itself in native scan mode (configured on the
    /// controller); the plan's overscan is not written to the file (LightBurn Fill fixture
    /// contains none). The physical motion is firmware-dependent.
    ControllerManaged,
    /// A plan with overscan > 0 is rejected.
    Reject,
}

/// Backend capabilities of one Ruida configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuidaProfile {
    pub magic: Magic,
    pub overscan: OverscanPolicy,
}

/// A lowered program: the RD body for exactly one plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RdProgram {
    plan: JobPlanId,
    body: Vec<Command>,
}

impl RdProgram {
    /// The plan this program was lowered from (preview, export and upload name the same id).
    pub fn plan(&self) -> JobPlanId {
        self.plan
    }

    /// Body records, without file sum and end record.
    pub fn body(&self) -> &[Command] {
        &self.body
    }

    /// The scrambled `.rd` file bytes (body + computed file sum + end record).
    pub fn to_rd_file(&self, magic: Magic) -> Result<Vec<u8>, EncodeError> {
        encode_rd_file(&self.body, magic)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum LowerError {
    /// A coordinate below zero: negative absolute coordinates are not confirmed (OPEN).
    NegativeCoordinate { event: usize },
    /// A value does not fit its field.
    Field { event: usize, error: FieldError },
    /// More settings blocks than the 7-bit part index allows.
    TooManyParts,
    /// The plan contains an event or setting this backend cannot represent.
    Unsupported { event: usize, what: &'static str },
}

impl fmt::Display for LowerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LowerError::NegativeCoordinate { event } => write!(f, "event {event}: negative machine coordinate"),
            LowerError::Field { event, error } => write!(f, "event {event}: {error}"),
            LowerError::TooManyParts => f.write_str("more than 128 settings blocks"),
            LowerError::Unsupported { event, what } => write!(f, "event {event}: {what} is not supported"),
        }
    }
}

impl std::error::Error for LowerError {}

/// Lowers `plan` to an RD command stream for `profile`.
///
/// Layout (typed records only, in the order the fixtures use): reference/absolute/init;
/// process and document bounds; per part (= settings block) speed, laser-1 powers, colour,
/// work mode and part bounds; max part; array start and bounds; one body block per
/// `OperationBoundary` (mode, part, laser device, air, speed, delays 0, powers, laser select,
/// motion); array end, block end. File sum and end record are added by [`RdProgram::to_rd_file`].
pub fn lower(plan: &JobPlan, profile: &RuidaProfile) -> Result<RdProgram, LowerError> {
    if plan.settings().len() > 128 {
        return Err(LowerError::TooManyParts);
    }
    let mut blocks: Vec<Block> = Vec::new();
    let mut at: Option<(u64, u64)> = None;
    // A rotary plan is in surface millimetres: Y becomes controller millimetres here, once.
    let y_mm = |v: f64| plan.rotary().map_or(v, |r| r.to_controller_y(v, plan.frame().height_mm()));
    for (event, e) in plan.events().iter().enumerate() {
        let field = |error| LowerError::Field { event, error };
        match *e {
            PlanEvent::OperationBoundary { settings, .. } => {
                let part = u8::try_from(settings.get()).map_err(|_| LowerError::TooManyParts)?;
                let s =
                    plan.settings().get(settings.get()).ok_or(LowerError::Unsupported { event, what: "settings" })?;
                let scan = match s.mode {
                    PlanMode::Line => false,
                    PlanMode::Scan { axis: ScanAxis::X, .. } => true,
                    PlanMode::Scan { axis: ScanAxis::Y, .. } => {
                        return Err(LowerError::Unsupported { event, what: "scan axis Y" });
                    }
                    // Modulation records (C7/C2 in the Grayscale fixture) have no established
                    // formula; never lowered as constant-power or dithered runs instead.
                    PlanMode::GrayscaleScan { .. } => {
                        return Err(LowerError::Unsupported { event, what: "grayscale modulation" });
                    }
                    PlanMode::AngledScan { .. } => return Err(LowerError::Unsupported { event, what: "scan angle" }),
                    // No fixture holds a pulse record (`docs/hardware/evidence-cam.md`): never
                    // written as a tiny cut or a delay instead.
                    PlanMode::Dots { .. } => return Err(LowerError::Unsupported { event, what: "dot mode" }),
                    _ => return Err(LowerError::Unsupported { event, what: "mode" }),
                };
                let speed = Speed::from_um_per_s(quantise_positive(s.speed_mm_s * 1000.0, event)?).map_err(field)?;
                let power = |r: f64| Power::from_raw((r * f64::from(Power::MAX_RAW)).round() as u16).map_err(field);
                blocks.push(Block {
                    part,
                    scan,
                    air: s.air_assist,
                    speed,
                    min: power(s.min_power.get())?,
                    max: power(s.max_power.get())?,
                    motion: Vec::new(),
                    points: Vec::new(),
                });
            }
            PlanEvent::Travel { to } => {
                let block =
                    blocks.last_mut().ok_or(LowerError::Unsupported { event, what: "travel before operation" })?;
                let p = (
                    quantise_positive(to.x_mm() * 1000.0, event)?,
                    quantise_positive(y_mm(to.y_mm()) * 1000.0, event)?,
                );
                if at == Some(p) {
                    continue;
                }
                let cmd = match at {
                    Some(prev) if prev.0 == p.0 => match relative(prev.1, p.1) {
                        Some(d) => Command::MoveRelY(d),
                        None => abs_move(p, false).map_err(field)?,
                    },
                    _ => abs_move(p, false).map_err(field)?,
                };
                block.motion.push(cmd);
                block.points.push(p);
                at = Some(p);
            }
            PlanEvent::Cut { to, .. } | PlanEvent::ScanRun { line: burnburn_job::ScanLine { to, .. }, .. } => {
                if let PlanEvent::ScanRun { line, .. } = *e
                    && line.overscan_mm > 0.0
                    && profile.overscan == OverscanPolicy::Reject
                {
                    return Err(LowerError::Unsupported { event, what: "overscan" });
                }
                let block = blocks.last_mut().ok_or(LowerError::Unsupported { event, what: "cut before operation" })?;
                let p = (
                    quantise_positive(to.x_mm() * 1000.0, event)?,
                    quantise_positive(y_mm(to.y_mm()) * 1000.0, event)?,
                );
                // The burn starts at the current head position, which is the plan start when no
                // travel came first: it belongs to the header/part bounds too.
                let from = match at {
                    Some(prev) => prev,
                    None => (
                        quantise_positive(plan.start().x_mm() * 1000.0, event)?,
                        quantise_positive(y_mm(plan.start().y_mm()) * 1000.0, event)?,
                    ),
                };
                block.points.push(from);
                let cmd = match at {
                    Some(prev) if prev.1 == p.1 => match relative(prev.0, p.0) {
                        Some(d) => Command::CutRelX(d),
                        None => abs_move(p, true).map_err(field)?,
                    },
                    _ => abs_move(p, true).map_err(field)?,
                };
                block.motion.push(cmd);
                block.points.push(p);
                at = Some(p);
            }
            PlanEvent::FocusOffset { .. } => return Err(LowerError::Unsupported { event, what: "focus offset" }),
            // A ramp has no established power record; a plain scan run would silently change the
            // burn, so the plan is refused.
            PlanEvent::RampRun { .. } => return Err(LowerError::Unsupported { event, what: "ramp length" }),
            PlanEvent::Dot { .. } => return Err(LowerError::Unsupported { event, what: "dot mode" }),
            // Feeder (FE01): no proven RD form for the feed or the pause; a job without them
            // would be a different job.
            PlanEvent::Feed { .. } | PlanEvent::Pause { .. } => {
                return Err(LowerError::Unsupported { event, what: "feeder" });
            }
            PlanEvent::PowerRun { .. } => return Err(LowerError::Unsupported { event, what: "grayscale modulation" }),
            // C04: no proven RD form for a power per movement; writing the tab at full power or
            // skipping it would be a different job.
            PlanEvent::CutPower { .. } => return Err(LowerError::Unsupported { event, what: "tab cut power" }),
            _ => return Err(LowerError::Unsupported { event, what: "event" }),
        }
    }
    let end = plan.events().len();
    let field = |error| LowerError::Field { event: end, error };
    let byte = |v: u8| Byte7::new(v).map_err(field);
    let xy = |p: (u64, u64)| -> Result<(AbsCoord, AbsCoord), LowerError> {
        Ok((AbsCoord::from_um(p.0).map_err(field)?, AbsCoord::from_um(p.1).map_err(field)?))
    };
    let all = extent(blocks.iter().flat_map(|b| b.points.iter().copied()));

    let mut body =
        vec![Command::Reference(RefPoint::RefPoint2), Command::SetAbsolute, Command::Process(ProcessAction::Start)];
    if let Some((lo, hi)) = all {
        let ((x0, y0), (x1, y1)) = (xy(lo)?, xy(hi)?);
        body.push(Command::Bounds { kind: BoundsKind::ProcessTopLeft, x: x0, y: y0 });
        body.push(Command::Bounds { kind: BoundsKind::ProcessBottomRight, x: x1, y: y1 });
        body.push(Command::Bounds { kind: BoundsKind::DocumentMin, x: x0, y: y0 });
        body.push(Command::Bounds { kind: BoundsKind::DocumentMax, x: x1, y: y1 });
    }
    // Part metadata, once per settings block that is used, in part order.
    let mut parts: Vec<&Block> = Vec::new();
    for b in &blocks {
        if !parts.iter().any(|p| p.part == b.part) {
            parts.push(b);
        }
    }
    parts.sort_by_key(|b| b.part);
    for b in &parts {
        let part = byte(b.part)?;
        body.push(Command::PartSpeed { part, speed: b.speed });
        body.push(Command::PartPower { laser: Laser::L1, bound: PowerBound::Min, part, value: b.min });
        body.push(Command::PartPower { laser: Laser::L1, bound: PowerBound::Max, part, value: b.max });
        body.push(Command::LayerColorPart { part, color: U35::new(0).map_err(field)? });
        // Fixture values: 0 for Line, 2 for Fill.
        body.push(Command::WorkModePart { part, mode: byte(if b.scan { 2 } else { 0 })? });
        let points = blocks.iter().filter(|x| x.part == b.part).flat_map(|x| x.points.iter().copied());
        if let Some((lo, hi)) = extent(points) {
            let ((x0, y0), (x1, y1)) = (xy(lo)?, xy(hi)?);
            for (kind, x, y) in [
                (PartBoundsKind::Min, x0, y0),
                (PartBoundsKind::Max, x1, y1),
                (PartBoundsKind::MinEx, x0, y0),
                (PartBoundsKind::MaxEx, x1, y1),
            ] {
                body.push(Command::PartBounds { kind, part, x, y });
            }
        }
    }
    if let Some(last) = parts.last() {
        body.push(Command::MaxLayerPart(byte(last.part)?));
    }
    body.push(Command::ArrayStart(byte(0)?));
    if let Some((lo, hi)) = all {
        let ((x0, y0), (x1, y1)) = (xy(lo)?, xy(hi)?);
        body.push(Command::Bounds { kind: BoundsKind::ArrayMin, x: x0, y: y0 });
        body.push(Command::Bounds { kind: BoundsKind::ArrayMax, x: x1, y: y1 });
    }
    for b in blocks {
        let zero = U35::new(0).map_err(field)?;
        body.push(Command::Mode(if b.scan { ModeFlag::WorkMode1 } else { ModeFlag::LayerEnd }));
        body.push(Command::LayerNumberPart(byte(b.part)?));
        body.push(Command::Mode(ModeFlag::LaserDevice0));
        body.push(Command::Mode(if b.air { ModeFlag::AirAssistOn } else { ModeFlag::AirAssistOff }));
        body.push(Command::Speed(b.speed));
        body.push(Command::LaserDelay { edge: crate::command::DelayEdge::On, raw: zero });
        body.push(Command::LaserDelay { edge: crate::command::DelayEdge::Off, raw: zero });
        body.push(Command::LaserPower { laser: Laser::L1, bound: PowerBound::Min, value: b.min });
        body.push(Command::LaserPower { laser: Laser::L1, bound: PowerBound::Max, value: b.max });
        body.push(Command::LaserSelect(byte(1)?));
        body.extend(b.motion);
        if b.scan {
            body.push(Command::BlockEnd);
        }
    }
    body.push(Command::ArrayEnd);
    body.push(Command::BlockEnd);
    Ok(RdProgram { plan: plan.id(), body })
}

/// One body block: the motion of one `OperationBoundary`.
struct Block {
    part: u8,
    scan: bool,
    air: bool,
    speed: Speed,
    min: Power,
    max: Power,
    motion: Vec<Command>,
    points: Vec<(u64, u64)>,
}

/// Round half away from zero to an integer; negative results are rejected, never clamped.
/// Values beyond a field's range are rejected by that field's constructor.
fn quantise_positive(v: f64, event: usize) -> Result<u64, LowerError> {
    let r = v.round();
    if !(r >= 0.0) {
        return Err(LowerError::NegativeCoordinate { event });
    }
    // `as` saturates for huge values; the field constructors then report the overflow.
    Ok(r as u64)
}

/// A single-axis delta between quantised positions, when it fits signed14.
fn relative(from: u64, to: u64) -> Option<RelCoord> {
    let d = i64::try_from(to).ok()? - i64::try_from(from).ok()?;
    let d = i32::try_from(d).ok()?;
    if d == 0 { None } else { RelCoord::from_um(d).ok() }
}

fn abs_move(p: (u64, u64), cut: bool) -> Result<Command, FieldError> {
    let (x, y) = (AbsCoord::from_um(p.0)?, AbsCoord::from_um(p.1)?);
    Ok(if cut { Command::CutAbs { x, y } } else { Command::MoveAbs { x, y } })
}

fn extent(points: impl Iterator<Item = (u64, u64)>) -> Option<((u64, u64), (u64, u64))> {
    points.fold(None, |acc, p| match acc {
        None => Some((p, p)),
        Some((lo, hi)) => Some(((lo.0.min(p.0), lo.1.min(p.1)), (hi.0.max(p.0), hi.1.max(p.1)))),
    })
}
