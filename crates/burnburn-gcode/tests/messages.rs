//! GRBL 1.1 response line parser. Example lines follow the public GRBL 1.1 interface description
//! (gnea/grbl `doc/markdown/interface.md`) as written down from memory: they are hand-written
//! fixtures, **not** captures from a controller (`tests/fixtures/README.md`).
#![allow(clippy::unwrap_used)]

use burnburn_gcode::grbl::{MachineState, Message, ParseError, alarm_description, error_description, parse_line};

#[test]
fn acknowledgements_and_errors() {
    assert_eq!(parse_line("ok").unwrap(), Message::Ok);
    assert_eq!(parse_line("error:20").unwrap(), Message::Error(20));
    assert_eq!(parse_line("ALARM:1").unwrap(), Message::Alarm(1));
    // Surrounding whitespace / CR is a transport detail.
    assert_eq!(parse_line("ok\r").unwrap(), Message::Ok);
    assert_eq!(parse_line("  error:9 ").unwrap(), Message::Error(9));
}

#[test]
fn welcome_banner_means_the_controller_reset() {
    assert_eq!(parse_line("Grbl 1.1h ['$' for help]").unwrap(), Message::Welcome { version: "1.1h".into() });
}

#[test]
fn settings_and_feedback_lines() {
    assert_eq!(parse_line("$32=1").unwrap(), Message::Setting { id: 32, value: "1".into() });
    assert_eq!(parse_line("$30=1000.000").unwrap(), Message::Setting { id: 30, value: "1000.000".into() });
    assert_eq!(parse_line("[MSG:'$H'|'$X' to unlock]").unwrap(), Message::Feedback("MSG:'$H'|'$X' to unlock".into()));
    // A startup-block echo is not a numbered setting.
    assert_eq!(parse_line("$N0=G54").unwrap(), Message::Other("$N0=G54".into()));
    assert_eq!(parse_line("").unwrap(), Message::Other(String::new()));
    assert_eq!(parse_line("some banner").unwrap(), Message::Other("some banner".into()));
}

#[test]
fn idle_status_with_machine_position_and_feed_spindle() {
    let Message::Status(s) = parse_line("<Idle|MPos:0.000,0.000,0.000|FS:0,0>").unwrap() else { panic!() };
    assert_eq!(s.state, MachineState::Idle);
    let m = s.mpos.unwrap();
    assert_eq!((m.x, m.y, m.z), (0.0, 0.0, Some(0.0)));
    assert_eq!((s.feed, s.spindle), (Some(0.0), Some(0.0)));
    assert!(s.wpos.is_none() && s.wco.is_none());
}

#[test]
fn running_status_with_buffer_overrides_pins_and_line() {
    let line = "<Run|WPos:12.500,-3.250,0.000|Bf:14,120|Ln:99|FS:1200,600|Ov:100,90,110|Pn:XP>";
    let Message::Status(s) = parse_line(line).unwrap() else { panic!() };
    assert_eq!(s.state, MachineState::Run);
    let w = s.wpos.unwrap();
    assert_eq!((w.x, w.y), (12.5, -3.25));
    assert_eq!(s.buffer, Some((14, 120)));
    assert_eq!(s.line, Some(99));
    assert_eq!((s.feed, s.spindle), (Some(1200.0), Some(600.0)));
    assert_eq!(s.overrides, Some((100, 90, 110)));
    assert_eq!(s.pins.as_deref(), Some("XP"));
}

#[test]
fn work_coordinate_offset_and_feed_only_field() {
    let Message::Status(s) = parse_line("<Jog|MPos:1,2,3|F:500|WCO:10.000,20.000,0.000>").unwrap() else { panic!() };
    assert_eq!(s.state, MachineState::Jog);
    assert_eq!(s.feed, Some(500.0));
    assert_eq!(s.spindle, None);
    let c = s.wco.unwrap();
    assert_eq!((c.x, c.y), (10.0, 20.0));
}

#[test]
fn substates_of_hold_and_door() {
    for (text, state) in [
        ("<Hold:0|MPos:0,0,0>", MachineState::Hold(0)),
        ("<Hold:1|MPos:0,0,0>", MachineState::Hold(1)),
        ("<Door:3|MPos:0,0,0>", MachineState::Door(3)),
        ("<Alarm|MPos:0,0,0>", MachineState::Alarm),
        ("<Home|MPos:0,0,0>", MachineState::Home),
        ("<Check|MPos:0,0,0>", MachineState::Check),
        ("<Sleep|MPos:0,0,0>", MachineState::Sleep),
    ] {
        let Message::Status(s) = parse_line(text).unwrap() else { panic!("{text}") };
        assert_eq!(s.state, state, "{text}");
    }
}

#[test]
fn unknown_status_fields_are_kept_not_dropped() {
    let Message::Status(s) = parse_line("<Idle|MPos:0,0,0|XY:1|FS:0,0>").unwrap() else { panic!() };
    assert_eq!(s.unknown_fields, vec!["XY:1".to_owned()]);
}

#[test]
fn malformed_lines_are_typed_errors() {
    for bad in [
        "<Idle|MPos:1,2",      // no closing bracket
        "<>",                  // no state
        "<Idle|MPos:a,b,c>",   // not numbers
        "<Idle|MPos:1>",       // one axis
        "<Idle|MPos:nan,0,0>", // non-finite must not slip through f64 parsing
        "<Idle|MPos:inf,0,0>",
        "<Idle|FS:1>", // FS needs two values
        "<Idle|Bf:1,x>",
        "<Idle|MPos:1,2,3,4,5,6,7>", // more axes than any GRBL variant
        "<Hold:x|MPos:0,0,0>",       // bad substate
        "<Bogus|MPos:0,0,0>",        // unknown state
        "error:",
        "error:abc",
        "error:300", // does not fit a code
        "ALARM:",
        "$32",
    ] {
        assert!(matches!(parse_line(bad), Err(ParseError::Malformed { .. })), "{bad:?} -> {:?}", parse_line(bad));
    }
}

#[test]
fn error_and_alarm_codes_are_described_when_known() {
    assert_eq!(error_description(20), Some("Unsupported or invalid g-code command"));
    assert_eq!(error_description(9), Some("G-code locked out during alarm or jog state"));
    assert_eq!(alarm_description(1), Some("Hard limit triggered; position is likely lost"));
    assert_eq!(alarm_description(3), Some("Reset while in motion; position is likely lost"));
    assert_eq!(error_description(200), None);
    assert_eq!(alarm_description(200), None);
}

proptest::proptest! {
    /// Arbitrary text never panics the parser, and an accepted status holds only finite numbers.
    #[test]
    fn arbitrary_lines_never_panic_and_statuses_are_finite(line in "\\PC{0,200}") {
        if let Ok(Message::Status(s)) = parse_line(&line) {
            for p in [s.mpos, s.wpos, s.wco].into_iter().flatten() {
                proptest::prop_assert!(p.x.is_finite() && p.y.is_finite() && p.z.is_none_or(f64::is_finite));
            }
            proptest::prop_assert!(s.feed.is_none_or(f64::is_finite) && s.spindle.is_none_or(f64::is_finite));
        }
    }

    /// Status-shaped noise (the interesting grammar corner) never panics either.
    #[test]
    fn status_shaped_noise_never_panics(body in "[A-Za-z0-9:,.|\\- ]{0,120}") {
        let _ = parse_line(&format!("<{body}>"));
        let _ = parse_line(&format!("<{body}"));
    }
}
