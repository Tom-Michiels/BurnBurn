//! Planning of laser-window actions into live requests: coordinates, origins and refusals.
//! Pure functions; capabilities and job state are the actor's gates (`tests/live.rs`).
#![allow(clippy::unwrap_used, clippy::panic)]

use burnburn_job::{MachineFrame, OriginCorner};
use burnburn_machine::{
    AxisPosition, ControllerFileName, HomeTarget, JogKey, LaserAction, LaserContext, LaserRefusal, LiveRequest,
    OriginTarget, PositionReading, UserOrigin, plan_action,
};

fn frame(origin: OriginCorner) -> MachineFrame {
    MachineFrame::new(300.0, 200.0, origin).unwrap()
}

fn at(x_um: i64, y_um: i64) -> PositionReading {
    PositionReading {
        x: AxisPosition::Known { um: x_um },
        y: AxisPosition::Known { um: y_um },
        read_at: None,
    }
}

fn ctx<'a>(frame: &'a MachineFrame, position: PositionReading, user: Option<UserOrigin>) -> LaserContext<'a> {
    LaserContext { frame, position, user_origin: user }
}

#[test]
fn simple_actions_map_one_to_one_and_need_no_position() {
    let f = frame(OriginCorner::BottomLeft);
    let c = ctx(&f, PositionReading::UNKNOWN, None);
    assert_eq!(plan_action(&LaserAction::Pause, &c), Ok(LiveRequest::Pause));
    assert_eq!(plan_action(&LaserAction::Resume, &c), Ok(LiveRequest::Resume));
    assert_eq!(plan_action(&LaserAction::ShowPosition, &c), Ok(LiveRequest::ReadPosition));
    assert_eq!(plan_action(&LaserAction::Home(HomeTarget::Xy), &c), Ok(LiveRequest::Home(HomeTarget::Xy)));
    assert_eq!(plan_action(&LaserAction::JogPress(JogKey::XLeft), &c), Ok(LiveRequest::JogStart(JogKey::XLeft)));
    assert_eq!(plan_action(&LaserAction::JogRelease(JogKey::XLeft), &c), Ok(LiveRequest::JogStop(JogKey::XLeft)));
    let n = ControllerFileName::new("PLAAT01").unwrap();
    assert_eq!(plan_action(&LaserAction::ListFiles, &c), Ok(LiveRequest::ListFiles));
    assert_eq!(plan_action(&LaserAction::RunFile(n.clone()), &c), Ok(LiveRequest::RunFile(n.clone())));
    assert_eq!(plan_action(&LaserAction::DeleteFile(n.clone()), &c), Ok(LiveRequest::DeleteFile(n.clone())));
    assert_eq!(plan_action(&LaserAction::FileMetadata(n.clone()), &c), Ok(LiveRequest::FileMetadata(n)));
}

#[test]
fn numeric_move_is_an_absolute_machine_move_inside_the_bed() {
    let f = frame(OriginCorner::BottomLeft);
    let c = ctx(&f, PositionReading::UNKNOWN, None);
    assert_eq!(
        plan_action(&LaserAction::MoveTo { x_um: 12_345, y_um: 67_890 }, &c),
        Ok(LiveRequest::MoveAbsolute { x_um: 12_345, y_um: 67_890 })
    );
    // The bed edges are inside, one micrometre further is not.
    assert!(plan_action(&LaserAction::MoveTo { x_um: 300_000, y_um: 200_000 }, &c).is_ok());
    assert!(matches!(
        plan_action(&LaserAction::MoveTo { x_um: 300_001, y_um: 0 }, &c),
        Err(LaserRefusal::OutsideBed { .. })
    ));
    assert!(matches!(
        plan_action(&LaserAction::MoveTo { x_um: 0, y_um: -1 }, &c),
        Err(LaserRefusal::OutsideBed { .. })
    ));
}

#[test]
fn position_laser_converts_a_canvas_click_in_bed_view_to_machine_coordinates() {
    let click = LaserAction::PositionLaser { bed_x_mm: 50.0, bed_y_mm: 30.0 };
    for (corner, want) in [
        (OriginCorner::BottomLeft, (50_000, 30_000)),
        (OriginCorner::TopLeft, (50_000, 170_000)),
        (OriginCorner::TopRight, (250_000, 170_000)),
        (OriginCorner::BottomRight, (250_000, 30_000)),
    ] {
        let f = frame(corner);
        let got = plan_action(&click, &ctx(&f, PositionReading::UNKNOWN, None)).unwrap();
        assert_eq!(got, LiveRequest::MoveAbsolute { x_um: want.0, y_um: want.1 }, "{corner:?}");
    }
    let f = frame(OriginCorner::BottomLeft);
    let c = ctx(&f, PositionReading::UNKNOWN, None);
    for (x, y) in [(f64::NAN, 1.0), (1.0, f64::INFINITY)] {
        assert_eq!(
            plan_action(&LaserAction::PositionLaser { bed_x_mm: x, bed_y_mm: y }, &c),
            Err(LaserRefusal::InvalidCoordinate)
        );
    }
    assert!(matches!(
        plan_action(&LaserAction::PositionLaser { bed_x_mm: 301.0, bed_y_mm: 1.0 }, &c),
        Err(LaserRefusal::OutsideBed { .. })
    ));
}

#[test]
fn step_jog_needs_a_known_position_and_follows_the_view_directions() {
    let step = LaserAction::JogStep { dx_view_um: 5_000, dy_view_um: 2_000 };
    let f = frame(OriginCorner::BottomLeft);
    assert_eq!(plan_action(&step, &ctx(&f, PositionReading::UNKNOWN, None)), Err(LaserRefusal::PositionUnknown));
    // One unknown axis is as good as none: the move needs both.
    let half = PositionReading { x: AxisPosition::Known { um: 1 }, y: AxisPosition::Unknown, read_at: None };
    assert_eq!(plan_action(&step, &ctx(&f, half, None)), Err(LaserRefusal::PositionUnknown));

    assert_eq!(
        plan_action(&step, &ctx(&f, at(100_000, 100_000), None)),
        Ok(LiveRequest::MoveAbsolute { x_um: 105_000, y_um: 102_000 })
    );
    // With the origin at the top-right both machine axes run against the view axes.
    let mirrored = frame(OriginCorner::TopRight);
    assert_eq!(
        plan_action(&step, &ctx(&mirrored, at(100_000, 100_000), None)),
        Ok(LiveRequest::MoveAbsolute { x_um: 95_000, y_um: 98_000 })
    );
    // A step off the bed is refused, not clamped.
    assert!(matches!(
        plan_action(&step, &ctx(&f, at(299_000, 100_000), None)),
        Err(LaserRefusal::OutsideBed { .. })
    ));
}

#[test]
fn go_to_origin_machine_is_zero_and_user_origin_needs_to_be_set() {
    let f = frame(OriginCorner::TopLeft);
    let c = ctx(&f, PositionReading::UNKNOWN, None);
    assert_eq!(
        plan_action(&LaserAction::GoToOrigin(OriginTarget::Machine), &c),
        Ok(LiveRequest::MoveAbsolute { x_um: 0, y_um: 0 })
    );
    assert_eq!(plan_action(&LaserAction::GoToOrigin(OriginTarget::User), &c), Err(LaserRefusal::UserOriginNotSet));
    let user = UserOrigin::from_position(&at(40_000, 25_000)).unwrap();
    let c = ctx(&f, PositionReading::UNKNOWN, Some(user));
    assert_eq!(
        plan_action(&LaserAction::GoToOrigin(OriginTarget::User), &c),
        Ok(LiveRequest::MoveAbsolute { x_um: 40_000, y_um: 25_000 })
    );
}

#[test]
fn a_user_origin_can_only_be_set_from_a_fully_known_position() {
    assert_eq!(UserOrigin::from_position(&PositionReading::UNKNOWN), Err(LaserRefusal::PositionUnknown));
    let half = PositionReading { x: AxisPosition::Unknown, y: AxisPosition::Known { um: 5 }, read_at: None };
    assert_eq!(UserOrigin::from_position(&half), Err(LaserRefusal::PositionUnknown));
    let zero = UserOrigin::from_position(&at(0, 0)).unwrap();
    assert_eq!((zero.x_um(), zero.y_um()), (0, 0), "a known zero is a valid origin");
}
