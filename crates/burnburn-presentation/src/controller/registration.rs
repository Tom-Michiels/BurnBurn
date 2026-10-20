//! Print and Cut and Center Finder (W3-UI-schikken). Positions are document (bed view)
//! coordinates; the laser's position is converted from the machine frame, and an unknown position
//! is refused, never taken as 0. A registration is machine-side state of the window: it moves what
//! the next compile cuts and never changes the document. It is dropped by anything that makes it
//! stale (another target, another measurement, another option, Reset, closing the panel).

use std::sync::Arc;

use burnburn_document::DocumentSnapshot;
use burnburn_editor::{CenterFinderSession, PointerPhase, RegistrationFlowError, RegistrationSession, snap_point};
use burnburn_geometry::{Affine, CircleFitError, Point, RegistrationError, RegistrationOptions, ScaleMode};
use burnburn_machine::StartFrom;
use burnburn_types::{DocumentId, RequestId};

use crate::field::parse_position;
use crate::intent::{CommandKey, ViewPointerEvent};
use crate::schikken::{
    CenterFinderIntent, CenterFinderViewModel, CenterFitView, CenterProblem, POOR_ARC_SPAN_RAD, PointMm,
    PrintAndCutIntent, PrintAndCutViewModel, PrintCutScale, RegistrationFailure, RegistrationField,
    RegistrationResultView, SchikkenMessage,
};
use crate::view_model::MessageKey;

use super::machine::Machine;
use super::{AppController, AppUpdate, Session};

/// The targets must span at least this much (mm), in the design and on the bed.
const MIN_TARGET_SEPARATION_MM: f64 = 1.0;
/// The allowed scale deviation the panel starts with (5 %).
const DEFAULT_MAX_DEVIATION: &str = "0,05";

#[derive(Debug, Clone)]
struct PrintCut {
    targets: [Option<Point>; 2],
    measured: [Option<Point>; 2],
    picking: Option<usize>,
    scale: PrintCutScale,
    /// What the field shows, and its parsed value when valid.
    max_deviation: String,
    deviation: f64,
    /// The registration in force, with the session it was computed in.
    session: Option<RegistrationSession>,
    error: Option<SchikkenMessage>,
}

impl PrintCut {
    fn new() -> Self {
        PrintCut {
            targets: [None, None],
            measured: [None, None],
            picking: None,
            scale: PrintCutScale::Fixed,
            max_deviation: DEFAULT_MAX_DEVIATION.to_owned(),
            deviation: 0.05,
            session: None,
            error: None,
        }
    }

    fn transform(&self) -> Option<&Affine> {
        self.session.as_ref().and_then(RegistrationSession::transform)
    }
}

#[derive(Debug, Clone, Default)]
struct Center {
    session: CenterFinderSession,
    picking: bool,
    error: Option<SchikkenMessage>,
}

#[derive(Debug, Clone, Default)]
pub(super) struct RegState {
    print: Option<PrintCut>,
    center: Option<Center>,
}

fn failure(f: RegistrationFailure) -> Option<SchikkenMessage> {
    Some(SchikkenMessage::Registration(f))
}

fn point_mm(p: Point) -> PointMm {
    (p.x, p.y)
}

/// Typed coordinates → a point; the first wrong field is named.
fn parse_point(x: &str, y: &str) -> Result<Point, RegistrationFailure> {
    let x = parse_position(x).map_err(|_| RegistrationFailure::InvalidField(RegistrationField::X))?;
    let y = parse_position(y).map_err(|_| RegistrationFailure::InvalidField(RegistrationField::Y))?;
    Ok(Point::new(x.mm(), y.mm()))
}

impl AppController {
    /// The head position in the document's (bed view) coordinates.
    fn laser_point(&self) -> Result<Point, RegistrationFailure> {
        let port = self.machine_port.as_ref().ok_or(RegistrationFailure::NoMachine)?;
        let machine = self.machine.as_ref().ok_or(RegistrationFailure::NoMachineProfile)?;
        let at = port.status().live.position.machine_point().ok_or(RegistrationFailure::PositionUnknown)?;
        Ok(machine.profile.frame().to_bed(at))
    }

    /// The registration the next compile of `document` is made with: `Ok(None)` without one. It is
    /// only valid with Absolute Coordinates, as another start would move the registered job.
    pub(super) fn registration_for_compile(&self, document: DocumentId) -> Result<Option<Affine>, MessageKey> {
        let Some(m) = self.sessions.get(&document).and_then(|s| s.registration.print.as_ref()?.transform().copied())
        else {
            return Ok(None);
        };
        if self.laser.start_from != StartFrom::AbsoluteCoordinates {
            return Err(MessageKey::Schikken(SchikkenMessage::Registration(
                RegistrationFailure::NeedsAbsoluteCoordinates,
            )));
        }
        Ok(Some(m))
    }

    pub(super) fn print_and_cut_intent(&mut self, intent: PrintAndCutIntent) -> AppUpdate {
        match intent {
            PrintAndCutIntent::MeasureTarget { document, index } => {
                let at = self.laser_point();
                self.with_session(document, |s| {
                    let Some(p) = s.registration.print.as_mut() else { return AppUpdate::default() };
                    match at {
                        Ok(at) => set_measured(s, index, at),
                        Err(f) => p.error = failure(f),
                    }
                    AppUpdate::redraw(document)
                })
            }
            PrintAndCutIntent::Register { document } => {
                let absolute = self.laser.start_from == StartFrom::AbsoluteCoordinates;
                self.with_session(document, |s| register(s, document, absolute))
            }
            other => self.with_session(other_document(&other), |s| print_and_cut_local(s, other)),
        }
    }

    pub(super) fn center_finder_intent(&mut self, intent: CenterFinderIntent) -> AppUpdate {
        match intent {
            CenterFinderIntent::MeasurePoint { document } => {
                let at = self.laser_point();
                self.with_session(document, |s| {
                    let Some(c) = s.registration.center.as_mut() else { return AppUpdate::default() };
                    match at {
                        Ok(at) => {
                            c.session.add_point(at);
                            c.error = None;
                        }
                        Err(f) => c.error = failure(f),
                    }
                    AppUpdate::redraw(document)
                })
            }
            CenterFinderIntent::PickPoints { document } => self.with_session(document, |s| {
                let Some(c) = s.registration.center.as_mut() else { return AppUpdate::default() };
                c.picking = true;
                AppUpdate::redraw(document)
            }),
            CenterFinderIntent::AddPoint { document, x, y } => self.with_session(document, |s| {
                let Some(c) = s.registration.center.as_mut() else { return AppUpdate::default() };
                match parse_point(&x, &y) {
                    Ok(p) => {
                        c.session.add_point(p);
                        c.error = None;
                    }
                    Err(f) => c.error = failure(f),
                }
                AppUpdate::redraw(document)
            }),
            CenterFinderIntent::UndoLast { document } => self.with_session(document, |s| {
                if let Some(c) = s.registration.center.as_mut() {
                    c.session.undo_last();
                }
                AppUpdate::redraw(document)
            }),
            CenterFinderIntent::Clear { document } => self.with_session(document, |s| {
                if let Some(c) = s.registration.center.as_mut() {
                    c.session.clear();
                    c.error = None;
                }
                AppUpdate::redraw(document)
            }),
            CenterFinderIntent::Close { document } => self.with_session(document, |s| {
                s.registration.center = None;
                AppUpdate::redraw(document)
            }),
        }
    }
}

fn other_document(intent: &PrintAndCutIntent) -> DocumentId {
    crate::schikken::SchikkenIntent::PrintAndCut(intent.clone()).document()
}

/// Intents that need nothing of the machine.
fn print_and_cut_local(s: &mut Session, intent: PrintAndCutIntent) -> AppUpdate {
    let document = other_document(&intent);
    let reset_plan = |s: &mut Session| {
        // A plan made with another registration is not the plan of this one.
        s.compiled = None;
    };
    let Some(p) = s.registration.print.as_mut() else { return AppUpdate::default() };
    match intent {
        PrintAndCutIntent::PickTarget { index, .. } if index < 2 => p.picking = Some(index),
        PrintAndCutIntent::SetTarget { index, x, y, .. } if index < 2 => match parse_point(&x, &y) {
            Ok(at) => set_target(s, index, at),
            Err(f) => p.error = failure(f),
        },
        PrintAndCutIntent::SetMeasured { index, x, y, .. } if index < 2 => match parse_point(&x, &y) {
            Ok(at) => set_measured(s, index, at),
            Err(f) => p.error = failure(f),
        },
        PrintAndCutIntent::SetScale { scale, max_deviation, .. } => match parse_position(&max_deviation) {
            Ok(d) if d.mm() >= 0.0 && d.mm().is_finite() => {
                let changed = p.scale != scale || p.max_deviation != max_deviation;
                p.scale = scale;
                p.max_deviation = max_deviation;
                p.deviation = d.mm();
                p.error = None;
                if changed {
                    p.session = None;
                    reset_plan(s);
                }
            }
            _ => p.error = failure(RegistrationFailure::InvalidField(RegistrationField::MaxDeviation)),
        },
        PrintAndCutIntent::Reset { .. } => {
            *p = PrintCut::new();
            reset_plan(s);
        }
        PrintAndCutIntent::Close { .. } => {
            s.registration.print = None;
            reset_plan(s);
        }
        _ => {}
    }
    AppUpdate::redraw(document)
}

fn set_target(s: &mut Session, index: usize, at: Point) {
    let Some(p) = s.registration.print.as_mut() else { return };
    p.targets[index] = Some(at);
    // New design targets need new measurements.
    p.measured = [None, None];
    p.picking = None;
    p.error = None;
    if p.session.take().is_some() {
        s.compiled = None;
    }
}

fn set_measured(s: &mut Session, index: usize, at: Point) {
    let Some(p) = s.registration.print.as_mut() else { return };
    p.measured[index] = Some(at);
    p.error = None;
    if p.session.take().is_some() {
        s.compiled = None;
    }
}

fn register(s: &mut Session, document: DocumentId, absolute: bool) -> AppUpdate {
    let Some(p) = s.registration.print.as_mut() else { return AppUpdate::default() };
    p.session = None;
    p.error = None;
    let outcome = (|| {
        let (Some(t0), Some(t1)) = (p.targets[0], p.targets[1]) else { return Err(RegistrationFailure::TargetNotSet) };
        let (Some(m0), Some(m1)) = (p.measured[0], p.measured[1]) else { return Err(RegistrationFailure::NotMeasured) };
        if !absolute {
            return Err(RegistrationFailure::NeedsAbsoluteCoordinates);
        }
        let scale = match p.scale {
            PrintCutScale::Fixed => ScaleMode::Fixed,
            PrintCutScale::Uniform => ScaleMode::Uniform { max_deviation: p.deviation },
        };
        let mut session =
            RegistrationSession::new(RegistrationOptions { scale, min_separation: MIN_TARGET_SEPARATION_MM });
        session.set_design_targets(vec![t0, t1]);
        session.set_measured(0, m0).map_err(|_| RegistrationFailure::Refused)?;
        session.set_measured(1, m1).map_err(|_| RegistrationFailure::Refused)?;
        session.compute().map_err(|e| match e {
            RegistrationFlowError::Registration(RegistrationError::DesignTargetsTooClose) => {
                RegistrationFailure::DesignTargetsTooClose
            }
            RegistrationFlowError::Registration(RegistrationError::MeasuredTargetsTooClose) => {
                RegistrationFailure::MeasuredTargetsTooClose
            }
            RegistrationFlowError::Registration(RegistrationError::ScaleOutOfRange { .. }) => {
                RegistrationFailure::ScaleOutOfRange
            }
            _ => RegistrationFailure::Refused,
        })?;
        Ok(session)
    })();
    match outcome {
        Ok(session) => p.session = Some(session),
        Err(f) => p.error = failure(f),
    }
    // The registration is part of what a compile makes: an older plan is not this one's.
    s.compiled = None;
    AppUpdate::redraw(document)
}

/// Opens a panel; `None` when `command` opens none.
pub(super) fn command(
    s: &mut Session,
    document: DocumentId,
    command: CommandKey,
    _request: Option<RequestId>,
) -> Option<AppUpdate> {
    match command {
        CommandKey::PrintAndCut => {
            s.registration.print.get_or_insert_with(PrintCut::new);
        }
        CommandKey::CenterFinder => {
            s.registration.center.get_or_insert_with(Center::default);
        }
        _ => return None,
    }
    Some(AppUpdate::redraw(document))
}

/// A canvas click while a target or edge point is being picked. `None`: nothing is being picked.
pub(super) fn pick(s: &mut Session, document: DocumentId, at: Point, event: &ViewPointerEvent) -> Option<AppUpdate> {
    let picking_target = s.registration.print.as_ref().and_then(|p| p.picking);
    let picking_point = s.registration.center.as_ref().is_some_and(|c| c.picking);
    if picking_target.is_none() && !picking_point {
        return None;
    }
    if event.phase != PointerPhase::Down {
        return Some(AppUpdate::default());
    }
    let snapshot: Arc<DocumentSnapshot> = s.doc.snapshot();
    let at = snap_point(&snapshot, &s.snap, s.camera.mm_per_point(), at, false, &[]).map_or(at, |c| c.at);
    if let Some(index) = picking_target {
        set_target(s, index, at);
    } else if let Some(c) = s.registration.center.as_mut() {
        c.session.add_point(at);
        c.error = None;
    }
    Some(AppUpdate::redraw(document))
}

/// Escape ends a pick. `true` when one was running.
pub(super) fn end_pick(s: &mut Session) -> bool {
    let mut ended = false;
    if let Some(p) = s.registration.print.as_mut()
        && p.picking.take().is_some()
    {
        ended = true;
    }
    if let Some(c) = s.registration.center.as_mut()
        && std::mem::take(&mut c.picking)
    {
        ended = true;
    }
    ended
}

pub(super) fn print_and_cut_view(s: &Session) -> Option<PrintAndCutViewModel> {
    let p = s.registration.print.as_ref()?;
    let result = p.session.as_ref().and_then(|session| session.registration()).map(|r| RegistrationResultView {
        rotation_deg: r.rotation.degrees(),
        scale: r.scale,
        residuals_mm: r.residuals.clone(),
        rms_mm: r.rms(),
    });
    Some(PrintAndCutViewModel {
        targets: p.targets.map(|t| t.map(point_mm)),
        measured: p.measured.map(|t| t.map(point_mm)),
        picking: p.picking,
        scale: p.scale,
        max_deviation: p.max_deviation.clone(),
        result,
        error: p.error,
    })
}

pub(super) fn center_finder_view(s: &Session, machine: Option<&Machine>) -> Option<CenterFinderViewModel> {
    let c = s.registration.center.as_ref()?;
    let (fit, problem) = match c.session.fit() {
        Ok(f) => {
            let center = f.circle.center;
            let center_machine =
                machine.and_then(|m| m.profile.frame().to_machine(center).ok()).map(|p| (p.x_mm(), p.y_mm()));
            let view = CenterFitView {
                center: point_mm(center),
                center_machine,
                radius_mm: f.circle.radius,
                residuals_mm: f.residuals.clone(),
                rms_mm: f.rms(),
                max_residual_mm: f.max_abs_residual(),
                arc_span_rad: f.arc_span,
                poorly_determined: f.arc_span < POOR_ARC_SPAN_RAD,
            };
            (Some(view), None)
        }
        Err(CircleFitError::Coincident) => (None, Some(CenterProblem::Coincident)),
        Err(CircleFitError::Collinear) => (None, Some(CenterProblem::Collinear)),
        Err(_) => (None, None),
    };
    Some(CenterFinderViewModel {
        points: c.session.points().iter().copied().map(point_mm).collect(),
        needed: c.session.points_needed(),
        picking: c.picking,
        fit,
        problem,
        error: c.error,
    })
}
