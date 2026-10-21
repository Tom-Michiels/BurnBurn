//! The driver (serial port + session) against the fake controller. No real device, no thread,
//! no clock: the test steps the controller.
#![allow(clippy::unwrap_used, clippy::panic)]

mod support;

use burnburn_gcode::grbl::{
    Command, JobFailure, JobState, JogTarget, SessionConfig, SessionEvent, UnknownReason, prepare_job,
};
use burnburn_gcode::{DriverError, GrblDriver, GrblProfile, SerialError, lower};
use support::fake_grbl::FakeGrbl;
use support::{interp, plans};

fn bring_up(fake: &FakeGrbl) -> GrblDriver<FakeGrbl> {
    let mut d = GrblDriver::new(fake.clone(), SessionConfig::laser(1000));
    d.request_settings().unwrap();
    fake.process(1);
    d.pump().unwrap();
    d.poll_status().unwrap();
    d.pump().unwrap();
    d
}

fn rectangle_job() -> burnburn_gcode::grbl::PreparedJob {
    let text = lower(&plans::rectangle_cut(false), &GrblProfile::grbl_1_1_laser()).unwrap().to_text();
    prepare_job(&text).unwrap()
}

/// Runs the stream to the end: the controller consumes `per_round` lines per round.
fn run_to_end(d: &mut GrblDriver<FakeGrbl>, fake: &FakeGrbl, per_round: usize) {
    for _ in 0..1000 {
        fake.process(per_round);
        d.pump().unwrap();
        if !d.session().job().unwrap().state.is_active() {
            return;
        }
        if d.session().job().unwrap().state == JobState::Draining {
            fake.finish();
            d.poll_status().unwrap();
            d.pump().unwrap();
        }
    }
}

#[test]
fn bring_up_reads_the_settings_and_the_idle_state() {
    let fake = FakeGrbl::new();
    let d = bring_up(&fake);
    assert_eq!(d.session().setting(32), Some("1"));
    assert_eq!(d.session().machine_state(), Some(burnburn_gcode::grbl::MachineState::Idle));
}

#[test]
fn a_lowered_program_streams_to_the_fake_without_overflowing_its_rx_buffer() {
    let fake = FakeGrbl::new();
    let mut d = bring_up(&fake);
    let job = rectangle_job();
    let expected: String = job.lines().iter().map(|l| format!("{l}\n")).collect();
    d.start_job(job).unwrap();
    run_to_end(&mut d, &fake, 2);
    assert_eq!(d.session().job().unwrap().state, JobState::Completed);
    let c = fake.inner();
    assert!(!c.overflowed, "the controller's RX buffer overflowed");
    assert!(c.max_rx_fill <= 127, "{}", c.max_rx_fill);
    drop(c);
    // Byte-for-byte what the lowering produced (comments stripped), in order.
    assert_eq!(fake.received_text(), expected);
    // And what the controller received burns exactly the rectangle perimeter.
    let run = interp::run(&fake.received_text()).unwrap();
    assert!((run.laser_on_length() - 120.0).abs() < 1e-9);
    assert!(run.ended && !run.laser_on_at_end);
}

#[test]
fn a_fast_controller_and_a_slow_controller_both_work() {
    for per_round in [1, 3, 50] {
        let fake = FakeGrbl::new();
        let mut d = bring_up(&fake);
        d.start_job(rectangle_job()).unwrap();
        run_to_end(&mut d, &fake, per_round);
        assert_eq!(d.session().job().unwrap().state, JobState::Completed, "per_round {per_round}");
        assert!(!fake.inner().overflowed);
    }
}

#[test]
fn a_rejected_line_holds_the_machine_and_the_job_fails_without_a_restart() {
    let fake = FakeGrbl::new();
    fake.inner().reject_lines = vec![3];
    let mut d = bring_up(&fake);
    d.start_job(rectangle_job()).unwrap();
    for _ in 0..50 {
        fake.process(1);
        d.pump().unwrap();
    }
    let JobState::Failed(JobFailure::LineRejected { index, code, .. }) = d.session().job().unwrap().state else {
        panic!("{:?}", d.session().job())
    };
    assert_eq!((index, code), (3, 20));
    assert!(fake.inner().held, "the feed hold must reach the controller");
    assert_eq!(fake.inner().resets, 0, "no automatic reset or restart");
    // The driver does not resume it either.
    assert!(matches!(d.resume(), Err(DriverError::Command(_))));
    assert!(fake.inner().held);
}

#[test]
fn cancel_resets_the_controller() {
    let fake = FakeGrbl::new();
    let mut d = bring_up(&fake);
    d.start_job(rectangle_job()).unwrap();
    d.cancel().unwrap();
    assert_eq!(fake.inner().resets, 1);
    d.pump().unwrap(); // the welcome banner
    assert_eq!(d.session().job().unwrap().state, JobState::Cancelled);
    assert!(!d.session().position_trusted());
}

#[test]
fn a_failing_write_at_start_leaves_the_outcome_unknown_and_nothing_is_retried() {
    let fake = FakeGrbl::new();
    let mut d = bring_up(&fake);
    let before = fake.inner().write_attempts;
    fake.inner().fail_writes = true;
    let e = d.start_job(rectangle_job()).unwrap_err();
    assert_eq!(e, DriverError::Serial(SerialError::Disconnected));
    assert_eq!(d.session().job().unwrap().state, JobState::Unknown(UnknownReason::ConnectionLost));
    fake.inner().fail_writes = false;
    d.pump().unwrap();
    d.poll_status().unwrap();
    assert_eq!(fake.inner().write_attempts, before + 2, "one failed start write, then only the status poll");
    assert!(fake.received_text().is_empty());
}

#[test]
fn a_failing_read_marks_the_running_job_unknown() {
    let fake = FakeGrbl::new();
    let mut d = bring_up(&fake);
    d.start_job(rectangle_job()).unwrap();
    fake.inner().fail_reads = true;
    assert_eq!(d.pump(), Err(SerialError::Disconnected));
    assert_eq!(d.session().job().unwrap().state, JobState::Unknown(UnknownReason::ConnectionLost));
}

#[test]
fn lines_split_across_reads_are_reassembled() {
    let fake = FakeGrbl::new();
    fake.inner().read_chunk = 3;
    let mut d = GrblDriver::new(fake.clone(), SessionConfig::laser(1000));
    d.request_settings().unwrap();
    fake.process(1);
    for _ in 0..40 {
        d.pump().unwrap();
    }
    assert_eq!(d.session().setting(30), Some("1000.000"));
    assert_eq!(d.session().setting(32), Some("1"));
}

#[test]
fn an_endless_line_is_discarded_and_reported_not_buffered_forever() {
    let fake = FakeGrbl::new();
    let mut d = GrblDriver::new(fake.clone(), SessionConfig::laser(1000));
    fake.say(&vec![b'x'; 10_000]);
    let events = d.pump().unwrap();
    assert!(events.iter().any(|e| matches!(e, SessionEvent::Unexpected(_))), "{events:?}");
    // The stream recovers at the next newline.
    fake.say(b"\r\n<Idle|MPos:0,0,0|FS:0,0>\r\n");
    d.pump().unwrap();
    assert_eq!(d.session().machine_state(), Some(burnburn_gcode::grbl::MachineState::Idle));
}

#[test]
fn non_utf8_bytes_do_not_break_the_driver() {
    let fake = FakeGrbl::new();
    let mut d = GrblDriver::new(fake.clone(), SessionConfig::laser(1000));
    fake.say(&[0xff, 0xfe, b'\n']);
    fake.say(b"<Idle|MPos:0,0,0|FS:0,0>\n");
    d.pump().unwrap();
    assert_eq!(d.session().machine_state(), Some(burnburn_gcode::grbl::MachineState::Idle));
}

#[test]
fn jog_home_and_unlock_reach_the_wire_as_lines() {
    let fake = FakeGrbl::new();
    let mut d = bring_up(&fake);
    d.command(Command::Jog { target: JogTarget::Relative { dx_mm: 1.0, dy_mm: 2.0 }, feed_mm_min: 500.0 }).unwrap();
    fake.process(1);
    d.pump().unwrap();
    d.command(Command::Home).unwrap();
    fake.process(1);
    d.pump().unwrap();
    d.jog_cancel().unwrap();
    assert_eq!(fake.inner().jog_cancels, 1);
    assert!(d.session().position_trusted());
    // `$` lines are not job lines.
    assert!(fake.inner().received.is_empty());
}
