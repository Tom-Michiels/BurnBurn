//! The GRBL streaming session as a pure state machine (no IO, no clock).
#![allow(clippy::unwrap_used, clippy::panic)]

use burnburn_gcode::grbl::{
    Command, CommandRefusal, GrblSession, JobFailure, JobState, JogTarget, MachineState, SessionConfig, SessionEvent,
    StartRefusal, Step, UnknownReason, prepare_job,
};

const IDLE: &str = "<Idle|MPos:0.000,0.000,0.000|FS:0,0>";

fn text(step: &Step) -> String {
    String::from_utf8(step.write.clone()).unwrap()
}

/// A session that has read back `$32=1`, `$30=1000` and seen an idle status.
fn ready() -> GrblSession {
    let mut s = GrblSession::new(SessionConfig::laser(1000));
    s.on_line("$30=1000.000");
    s.on_line("$32=1");
    s.on_line(IDLE);
    s
}

/// `n` lines of exactly 30 bytes + newline: `G1 X10.000 Y10.000 S600 F1200` is 29 chars, so pad the F.
fn job(n: usize) -> burnburn_gcode::grbl::PreparedJob {
    let body: String = (0..n).map(|i| format!("G1 X{:06.3} Y10.000 S600 F1200\n", i as f64 % 100.0)).collect();
    prepare_job(&body).unwrap()
}

fn progress(s: &GrblSession) -> (usize, usize, usize) {
    let p = s.job().unwrap();
    (p.total, p.sent, p.acked)
}

#[test]
fn the_first_window_fills_the_rx_buffer_without_exceeding_it() {
    let mut s = ready();
    let j = job(20);
    let line_bytes = j.lines()[0].len() + 1;
    assert_eq!(line_bytes, 30);
    let step = s.start_job(j).unwrap();
    // floor(127 / 30) = 4 lines: 120 bytes outstanding, a fifth would make 150 > 127.
    assert_eq!(step.write.len(), 4 * 30);
    assert_eq!(s.bytes_in_flight(), 120);
    assert_eq!(progress(&s), (20, 4, 0));
    assert_eq!(s.job().unwrap().state, JobState::Streaming);
}

#[test]
fn each_ok_frees_room_for_the_next_line() {
    let mut s = ready();
    s.start_job(job(20)).unwrap();
    let step = s.on_line("ok");
    assert_eq!(step.write.len(), 30);
    assert_eq!(progress(&s), (20, 5, 1));
    assert_eq!(s.bytes_in_flight(), 120);
}

#[test]
fn every_line_is_sent_exactly_once_and_in_order() {
    let mut s = ready();
    let j = job(37);
    let expected: String = j.lines().iter().map(|l| format!("{l}\n")).collect();
    let mut wire = text(&s.start_job(j).unwrap());
    while s.job().unwrap().acked < 37 {
        wire.push_str(&text(&s.on_line("ok")));
    }
    assert_eq!(wire, expected);
    assert_eq!(progress(&s), (37, 37, 37));
}

#[test]
fn completion_needs_an_idle_status_after_the_last_ok() {
    let mut s = ready();
    s.start_job(job(2)).unwrap();
    s.on_line("ok");
    s.on_line("ok");
    // All lines are acknowledged: that only means "buffered", not "burnt".
    assert_eq!(s.job().unwrap().state, JobState::Draining);
    let step = s.on_line("<Run|MPos:5.000,10.000,0.000|FS:1200,600>");
    assert_eq!(s.job().unwrap().state, JobState::Draining);
    assert!(!step.events.iter().any(|e| matches!(e, SessionEvent::JobState(JobState::Completed))));
    let step = s.on_line(IDLE);
    assert_eq!(s.job().unwrap().state, JobState::Completed);
    assert!(step.events.contains(&SessionEvent::JobState(JobState::Completed)));
}

#[test]
fn an_idle_status_before_the_last_ok_does_not_complete_the_job() {
    let mut s = ready();
    s.start_job(job(2)).unwrap();
    s.on_line("ok");
    s.on_line(IDLE); // controller momentarily idle between lines
    assert_eq!(s.job().unwrap().state, JobState::Streaming);
    s.on_line("ok");
    assert_eq!(s.job().unwrap().state, JobState::Draining);
}

#[test]
fn an_error_on_a_job_line_stops_the_job_and_feeds_hold_but_never_resumes() {
    let mut s = ready();
    s.start_job(job(20)).unwrap();
    s.on_line("ok");
    let step = s.on_line("error:20");
    assert_eq!(step.write, b"!");
    let JobState::Failed(JobFailure::LineRejected { index, code, text }) = s.job().unwrap().state else { panic!() };
    assert_eq!((index, code), (1, 20));
    assert!(text.starts_with("G1 X"), "{text}");
    // Later oks (the lines that were already buffered) free nothing and send nothing.
    let sent_before = s.job().unwrap().sent;
    for _ in 0..5 {
        assert!(s.on_line("ok").write.is_empty());
    }
    assert_eq!(s.job().unwrap().sent, sent_before);
    // The hold is not undone by resume, and a new job cannot start while the machine holds.
    assert_eq!(s.resume(), Err(CommandRefusal::ResumeAfterFault));
    s.on_line("<Hold:1|MPos:3.000,10.000,0.000|FS:0,0>");
    assert!(matches!(s.start_job(job(1)), Err(StartRefusal::NotIdle(MachineState::Hold(1)))));
}

#[test]
fn an_alarm_ends_the_job_and_blocks_a_new_one_until_the_machine_reports_idle_again() {
    let mut s = ready();
    s.start_job(job(20)).unwrap();
    let step = s.on_line("ALARM:1");
    assert!(step.write.is_empty(), "nothing is sent into an alarm");
    assert_eq!(s.job().unwrap().state, JobState::Failed(JobFailure::Alarm { code: Some(1) }));
    assert_eq!(s.alarm(), Some(1));
    assert!(matches!(s.start_job(job(1)), Err(StartRefusal::Alarm)));
    // Unlock is an explicit user step; the alarm clears only when the controller says so.
    s.on_line("<Alarm|MPos:0,0,0|FS:0,0>");
    assert!(matches!(s.start_job(job(1)), Err(StartRefusal::Alarm)));
}

#[test]
fn an_alarm_state_in_a_status_report_also_fails_the_job() {
    let mut s = ready();
    s.start_job(job(20)).unwrap();
    s.on_line("<Alarm|MPos:0,0,0|FS:0,0>");
    assert_eq!(s.job().unwrap().state, JobState::Failed(JobFailure::Alarm { code: None }));
}

#[test]
fn a_reset_banner_during_a_job_fails_it_and_forgets_the_machine_state() {
    let mut s = ready();
    s.start_job(job(20)).unwrap();
    s.on_line("Grbl 1.1h ['$' for help]");
    assert_eq!(s.job().unwrap().state, JobState::Failed(JobFailure::ControllerReset));
    assert_eq!(s.machine_state(), None);
    assert_eq!(s.bytes_in_flight(), 0);
    assert!(matches!(s.start_job(job(1)), Err(StartRefusal::NoStatusYet)));
}

#[test]
fn a_stray_ok_with_nothing_outstanding_fails_the_active_job_and_holds() {
    let mut s = ready();
    s.start_job(job(1)).unwrap();
    s.on_line("ok"); // the one line: now draining
    assert_eq!(s.job().unwrap().state, JobState::Draining);
    let step = s.on_line("ok");
    assert_eq!(step.write, b"!");
    assert!(step.events.iter().any(|e| matches!(e, SessionEvent::Unexpected(_))));
    assert_eq!(s.job().unwrap().state, JobState::Failed(JobFailure::Desync));
}

#[test]
fn a_stray_ok_without_a_job_is_only_reported() {
    let mut s = ready();
    let step = s.on_line("ok");
    assert!(step.events.iter().any(|e| matches!(e, SessionEvent::Unexpected(_))));
    assert!(s.job().is_none());
}

#[test]
fn start_is_refused_until_the_machine_state_and_settings_are_confirmed() {
    let job1 = || job(1);
    let mut s = GrblSession::new(SessionConfig::laser(1000));
    assert!(matches!(s.start_job(job1()), Err(StartRefusal::SettingNotConfirmed { .. })));
    s.on_line("$30=1000.000");
    s.on_line("$32=1");
    assert!(matches!(s.start_job(job1()), Err(StartRefusal::NoStatusYet)));
    s.on_line("<Run|MPos:0,0,0|FS:0,0>");
    assert!(matches!(s.start_job(job1()), Err(StartRefusal::NotIdle(MachineState::Run))));
    s.on_line(IDLE);
    assert!(s.start_job(job1()).is_ok());
    assert!(matches!(s.start_job(job1()), Err(StartRefusal::Busy)));
}

#[test]
fn laser_mode_off_or_a_wrong_spindle_scale_blocks_the_start() {
    let mut s = GrblSession::new(SessionConfig::laser(1000));
    s.on_line("$30=1000");
    s.on_line("$32=0");
    s.on_line(IDLE);
    let StartRefusal::SettingMismatch { id, expected, actual } = s.start_job(job(1)).unwrap_err() else { panic!() };
    assert_eq!((id, expected, actual.as_str()), (32, 1.0, "0"));

    let mut s = GrblSession::new(SessionConfig::laser(1000));
    s.on_line("$30=255");
    s.on_line("$32=1");
    s.on_line(IDLE);
    assert!(matches!(s.start_job(job(1)), Err(StartRefusal::SettingMismatch { id: 30, .. })));
}

#[test]
fn cancel_sends_a_soft_reset_marks_the_job_cancelled_and_distrusts_the_position() {
    let mut s = ready();
    s.start_job(job(20)).unwrap();
    let step = s.cancel();
    assert_eq!(step.write, [0x18]);
    assert_eq!(s.job().unwrap().state, JobState::Cancelled);
    assert!(!s.position_trusted());
    assert_eq!(s.bytes_in_flight(), 0);
    assert_eq!(s.machine_state(), None);
    // The controller's banner after the reset does not turn the cancelled job into a failure.
    s.on_line("Grbl 1.1h ['$' for help]");
    assert_eq!(s.job().unwrap().state, JobState::Cancelled);
}

#[test]
fn realtime_commands_are_not_character_counted() {
    let mut s = ready();
    s.start_job(job(20)).unwrap();
    let before = s.bytes_in_flight();
    assert_eq!(s.poll_status().write, b"?");
    assert_eq!(s.hold().write, b"!");
    assert_eq!(s.bytes_in_flight(), before);
    s.on_line("<Hold:1|MPos:0,0,0|FS:0,0>");
    assert_eq!(s.resume().unwrap().write, b"~");
    assert_eq!(s.bytes_in_flight(), before);
    assert_eq!(s.job().unwrap().state, JobState::Streaming, "a user hold does not fail the job");
}

#[test]
fn a_lost_connection_makes_the_job_unknown_and_nothing_restarts() {
    let mut s = ready();
    s.start_job(job(20)).unwrap();
    let step = s.transport_lost();
    assert!(step.write.is_empty());
    assert_eq!(s.job().unwrap().state, JobState::Unknown(UnknownReason::ConnectionLost));
    assert_eq!(s.machine_state(), None);
    assert_eq!(s.resume(), Err(CommandRefusal::ResumeAfterFault));
    // Late lines from the old connection do not revive or complete it.
    s.on_line("ok");
    s.on_line(IDLE);
    assert_eq!(s.job().unwrap().state, JobState::Unknown(UnknownReason::ConnectionLost));
}

#[test]
fn a_finished_job_can_be_followed_by_a_new_one_only_from_a_fresh_idle() {
    let mut s = ready();
    s.start_job(job(1)).unwrap();
    s.on_line("ok");
    s.on_line(IDLE);
    assert_eq!(s.job().unwrap().state, JobState::Completed);
    assert!(s.start_job(job(1)).is_ok());
}

#[test]
fn jog_homing_unlock_are_single_line_commands_with_one_outstanding() {
    let mut s = ready();
    let step = s
        .command(Command::Jog { target: JogTarget::Relative { dx_mm: 10.0, dy_mm: -5.5 }, feed_mm_min: 1000.0 })
        .unwrap();
    assert_eq!(text(&step), "$J=G21G91X10Y-5.5F1000\n");
    assert_eq!(s.command(Command::Home), Err(CommandRefusal::Busy));
    let step = s.on_line("ok");
    assert!(step.events.contains(&SessionEvent::CommandAccepted));
    assert_eq!(text(&s.command(Command::Home).unwrap()), "$H\n");
    s.on_line("ok");
    assert_eq!(text(&s.command(Command::Unlock).unwrap()), "$X\n");
    s.on_line("error:9");
    assert!(s.command(Command::Home).is_ok(), "an error answers the command and frees the slot");
}

#[test]
fn absolute_jog_uses_g90() {
    let mut s = ready();
    let step = s.command(Command::Jog { target: JogTarget::Absolute { x_mm: 0.0, y_mm: 25.25 }, feed_mm_min: 600.0 });
    assert_eq!(text(&step.unwrap()), "$J=G21G90X0Y25.25F600\n");
}

#[test]
fn invalid_jogs_are_refused_before_anything_is_sent() {
    let mut s = ready();
    for (dx, feed) in [(f64::NAN, 100.0), (1.0, 0.0), (1.0, -5.0), (f64::INFINITY, 100.0), (1.0, f64::NAN)] {
        let c = Command::Jog { target: JogTarget::Relative { dx_mm: dx, dy_mm: 0.0 }, feed_mm_min: feed };
        assert_eq!(s.command(c), Err(CommandRefusal::InvalidJog), "{dx} {feed}");
    }
    assert_eq!(s.bytes_in_flight(), 0);
}

#[test]
fn jogs_need_a_known_idle_or_jogging_machine() {
    let jog = || Command::Jog { target: JogTarget::Relative { dx_mm: 1.0, dy_mm: 0.0 }, feed_mm_min: 100.0 };
    let mut s = GrblSession::new(SessionConfig::laser(1000));
    assert_eq!(s.command(jog()), Err(CommandRefusal::NoStatusYet));
    s.on_line("<Alarm|MPos:0,0,0|FS:0,0>");
    assert_eq!(s.command(jog()), Err(CommandRefusal::NotIdle(MachineState::Alarm)));
    s.on_line("<Jog|MPos:0,0,0|FS:0,0>");
    assert!(s.command(jog()).is_ok());
}

#[test]
fn no_command_but_cancel_and_realtime_is_accepted_during_a_job() {
    let mut s = ready();
    s.start_job(job(20)).unwrap();
    assert_eq!(s.command(Command::Home), Err(CommandRefusal::JobActive));
    assert_eq!(s.command(Command::Unlock), Err(CommandRefusal::JobActive));
    assert_eq!(s.command(Command::Line("G0 X0".into())), Err(CommandRefusal::JobActive));
    assert_eq!(s.jog_cancel().write, [0x85]);
    assert_eq!(s.poll_status().write, b"?");
}

#[test]
fn command_lines_must_be_single_clean_lines() {
    let mut s = ready();
    for bad in ["", "G0 X1\nG0 X2", "G0 X1\r", "G0 X1 ?", "M3 !", "G0 \u{18}", "G0 \u{e9}", "~"] {
        assert_eq!(s.command(Command::Line(bad.into())), Err(CommandRefusal::InvalidLine), "{bad:?}");
    }
    let step = s.command(Command::Line("G0 X1".into())).unwrap();
    assert_eq!(text(&step), "G0 X1\n");
}

#[test]
fn settings_can_be_requested_and_are_remembered() {
    let mut s = GrblSession::new(SessionConfig::laser(1000));
    assert_eq!(text(&s.request_settings().unwrap()), "$$\n");
    // `$$` answers with the setting lines and a final ok.
    let step = s.on_line("$32=1");
    assert!(step.events.contains(&SessionEvent::SettingObserved { id: 32, value: "1".into() }));
    s.on_line("ok");
    assert_eq!(s.setting(32), Some("1"));
}

#[test]
fn garbage_lines_are_reported_not_fatal() {
    let mut s = ready();
    s.start_job(job(20)).unwrap();
    let step = s.on_line("<Idle|MPos:oops>");
    assert!(step.events.iter().any(|e| matches!(e, SessionEvent::Unexpected(_))));
    assert_eq!(s.job().unwrap().state, JobState::Streaming);
}

proptest::proptest! {
    /// For any line lengths and any interleaving of acknowledgements, the bytes outstanding
    /// never exceed `rx_buffer - 1`, every line goes out once and in order, and the job drains.
    #[test]
    fn character_counting_never_overflows_the_rx_buffer(
        lens in proptest::collection::vec(1usize..=70, 1..80),
        acks_per_round in proptest::collection::vec(1usize..4, 1..200),
    ) {
        // Lines "G" + digits of exactly the requested lengths.
        let lines: Vec<String> = lens.iter().map(|n| format!("G{}", "1".repeat(n.saturating_sub(1).max(1)))).collect();
        let job = prepare_job(&lines.join("\n")).unwrap();
        let expected: String = job.lines().iter().map(|l| format!("{l}\n")).collect();
        let mut s = ready();
        let mut wire = text(&s.start_job(job).unwrap());
        proptest::prop_assert!(s.bytes_in_flight() <= 127);
        let mut rounds = acks_per_round.iter().cycle();
        let mut guard = 0;
        while s.job().unwrap().acked < lines.len() {
            guard += 1;
            proptest::prop_assert!(guard < 10_000);
            for _ in 0..*rounds.next().unwrap() {
                if s.bytes_in_flight() == 0 { break; }
                wire.push_str(&text(&s.on_line("ok")));
                proptest::prop_assert!(s.bytes_in_flight() <= 127, "{}", s.bytes_in_flight());
            }
        }
        proptest::prop_assert_eq!(wire, expected);
        proptest::prop_assert_eq!(s.job().unwrap().state, JobState::Draining);
    }
}
