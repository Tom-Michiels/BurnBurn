//! Laser-window user actions planned into live requests: numeric coordinates, origins and the
//! refusals that depend on what is known about the head position. Pure; the actor adds the
//! capability and job-activity gates when the request is sent.
//!
//! Coordinates: a [`LaserAction::PositionLaser`] click and the step directions are in the bed
//! view (y up, origin bottom-left); everything that reaches the controller is in the machine
//! frame of the active [`MachineFrame`], in micrometres.

use std::fmt;

use burnburn_job::{MachineFrame, OriginCorner};

use crate::controller_files::ControllerFileName;
use crate::live::{HomeTarget, JogKey, LiveRequest};
use crate::position::PositionReading;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OriginTarget {
    /// The machine origin (0, 0) of the controller frame.
    Machine,
    /// The user-set origin.
    User,
}

/// What a laser-window button does.
#[derive(Debug, Clone, PartialEq)]
pub enum LaserAction {
    /// Key down: the head moves until the release.
    JogPress(JogKey),
    /// Key up. Always allowed.
    JogRelease(JogKey),
    /// A bounded step from the known position, in bed-view micrometres (right/up positive).
    JogStep { dx_view_um: i64, dy_view_um: i64 },
    /// Numeric absolute move in machine-frame micrometres.
    MoveTo { x_um: i64, y_um: i64 },
    /// "Position Laser": a canvas click in bed-view millimetres.
    PositionLaser { bed_x_mm: f64, bed_y_mm: f64 },
    Home(HomeTarget),
    GoToOrigin(OriginTarget),
    Pause,
    Resume,
    /// "Show last position": read the head position from the controller.
    ShowPosition,
    ListFiles,
    FileMetadata(ControllerFileName),
    RunFile(ControllerFileName),
    DeleteFile(ControllerFileName),
}

/// The user-set origin, machine-frame micrometres. Only built from a fully known position (or
/// restored from a saved value), so "not set" is `None` and never (0, 0).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UserOrigin {
    x_um: i64,
    y_um: i64,
}

impl UserOrigin {
    pub fn new(x_um: i64, y_um: i64) -> UserOrigin {
        UserOrigin { x_um, y_um }
    }

    /// "Set origin to the current position".
    pub fn from_position(position: &PositionReading) -> Result<UserOrigin, LaserRefusal> {
        match (position.x.um(), position.y.um()) {
            (Some(x_um), Some(y_um)) => Ok(UserOrigin { x_um, y_um }),
            _ => Err(LaserRefusal::PositionUnknown),
        }
    }

    pub fn x_um(&self) -> i64 {
        self.x_um
    }

    pub fn y_um(&self) -> i64 {
        self.y_um
    }
}

pub struct LaserContext<'a> {
    pub frame: &'a MachineFrame,
    pub position: PositionReading,
    pub user_origin: Option<UserOrigin>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaserRefusal {
    /// The action needs the head position and it is not (fully) known.
    PositionUnknown,
    UserOriginNotSet,
    /// The target lies off the bed (machine-frame micrometres).
    OutsideBed { x_um: i64, y_um: i64 },
    /// A non-finite or absurdly large coordinate.
    InvalidCoordinate,
}

impl fmt::Display for LaserRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for LaserRefusal {}

/// Signs of machine axes against the bed view (right/up positive).
fn axis_signs(origin: OriginCorner) -> (i64, i64) {
    match origin {
        OriginCorner::BottomLeft => (1, 1),
        OriginCorner::TopLeft => (1, -1),
        OriginCorner::TopRight => (-1, -1),
        OriginCorner::BottomRight => (-1, 1),
    }
}

fn to_um(mm: f64) -> Result<i64, LaserRefusal> {
    let um = (mm * 1000.0).round();
    if um.is_finite() && um.abs() < 1.0e12 { Ok(um as i64) } else { Err(LaserRefusal::InvalidCoordinate) }
}

fn on_bed(frame: &MachineFrame, x_um: i64, y_um: i64) -> Result<LiveRequest, LaserRefusal> {
    let (w, h) = (to_um(frame.width_mm())?, to_um(frame.height_mm())?);
    if (0..=w).contains(&x_um) && (0..=h).contains(&y_um) {
        Ok(LiveRequest::MoveAbsolute { x_um, y_um })
    } else {
        Err(LaserRefusal::OutsideBed { x_um, y_um })
    }
}

/// Plans one action into the live request that carries it out.
pub fn plan_action(action: &LaserAction, ctx: &LaserContext<'_>) -> Result<LiveRequest, LaserRefusal> {
    let frame = ctx.frame;
    match action {
        LaserAction::JogPress(key) => Ok(LiveRequest::JogStart(*key)),
        LaserAction::JogRelease(key) => Ok(LiveRequest::JogStop(*key)),
        LaserAction::Home(target) => Ok(LiveRequest::Home(*target)),
        LaserAction::Pause => Ok(LiveRequest::Pause),
        LaserAction::Resume => Ok(LiveRequest::Resume),
        LaserAction::ShowPosition => Ok(LiveRequest::ReadPosition),
        LaserAction::ListFiles => Ok(LiveRequest::ListFiles),
        LaserAction::FileMetadata(name) => Ok(LiveRequest::FileMetadata(name.clone())),
        LaserAction::RunFile(name) => Ok(LiveRequest::RunFile(name.clone())),
        LaserAction::DeleteFile(name) => Ok(LiveRequest::DeleteFile(name.clone())),
        LaserAction::MoveTo { x_um, y_um } => on_bed(frame, *x_um, *y_um),
        LaserAction::PositionLaser { bed_x_mm, bed_y_mm } => {
            let (vx, vy) = (to_um(*bed_x_mm)?, to_um(*bed_y_mm)?);
            let (w, h) = (to_um(frame.width_mm())?, to_um(frame.height_mm())?);
            let (sx, sy) = axis_signs(frame.origin());
            let x_um = if sx > 0 { vx } else { w - vx };
            let y_um = if sy > 0 { vy } else { h - vy };
            on_bed(frame, x_um, y_um)
        }
        LaserAction::JogStep { dx_view_um, dy_view_um } => {
            let (x, y) = match (ctx.position.x.um(), ctx.position.y.um()) {
                (Some(x), Some(y)) => (x, y),
                _ => return Err(LaserRefusal::PositionUnknown),
            };
            let (sx, sy) = axis_signs(frame.origin());
            let overflow = LaserRefusal::InvalidCoordinate;
            let tx = sx.checked_mul(*dx_view_um).and_then(|d| x.checked_add(d)).ok_or(overflow.clone())?;
            let ty = sy.checked_mul(*dy_view_um).and_then(|d| y.checked_add(d)).ok_or(overflow)?;
            on_bed(frame, tx, ty)
        }
        LaserAction::GoToOrigin(OriginTarget::Machine) => Ok(LiveRequest::MoveAbsolute { x_um: 0, y_um: 0 }),
        LaserAction::GoToOrigin(OriginTarget::User) => {
            let o = ctx.user_origin.ok_or(LaserRefusal::UserOriginNotSet)?;
            on_bed(frame, o.x_um, o.y_um)
        }
    }
}
