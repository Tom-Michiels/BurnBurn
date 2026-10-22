//! G5-grbl-ui, the laser window of a GRBL machine: console (input, history up/down, transcript),
//! named macros that never run during a job, jog/home/unlock, and a job start that stays typed-
//! refused without hardware evidence. Everything runs against a fake GRBL controller behind the
//! `SerialPort` trait: no device, no thread, no clock.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use burnburn_gcode::grbl::{SessionConfig, prepare_job};
use burnburn_gcode::{GrblDriver, GrblProfile, SerialError, SerialPort, grbl_capabilities};
use burnburn_job::{MachineFrame, MachineProfile, OriginCorner, PowerRatio};
use burnburn_presentation::{
    CommandKey, ConsoleError, GrblControlKind, GrblEntryKind, GrblIntent, GrblMachineState, GrblPort, GrblViewModel,
    GrblWhy, HeadlessEditorHarness, HostEffect, LaserWhy, MacroError, MessageKey, StepDirection, UiIntent,
};

use common::{RUIDA, profile};

#[derive(Default)]
struct Inner {
    line: Vec<u8>,
    outbox: Vec<u8>,
    /// Lines the controller consumed, in order.
    received: Vec<String>,
    realtime: Vec<u8>,
    state: String,
    /// Lines containing this text are answered with `error:20`.
    reject: Option<String>,
    /// Answers are queued (held) instead of sent: the controller is "busy".
    held: bool,
    pending: VecDeque<String>,
    fail_writes: bool,
}

#[derive(Clone)]
struct Fake(Rc<RefCell<Inner>>);

impl Fake {
    fn new() -> Fake {
        Fake(Rc::new(RefCell::new(Inner { state: "Idle".into(), ..Inner::default() })))
    }
    fn received(&self) -> Vec<String> {
        self.0.borrow().received.clone()
    }
    fn realtime(&self) -> Vec<u8> {
        self.0.borrow().realtime.clone()
    }
    fn set_state(&self, state: &str) {
        self.0.borrow_mut().state = state.into();
    }
    fn reject(&self, text: &str) {
        self.0.borrow_mut().reject = Some(text.into());
    }
    fn hold_answers(&self, held: bool) {
        self.0.borrow_mut().held = held;
    }
    /// Releases the next held answer.
    fn answer_one(&self) {
        let mut c = self.0.borrow_mut();
        if let Some(a) = c.pending.pop_front() {
            c.outbox.extend_from_slice(a.as_bytes());
        }
    }
    fn fail_writes(&self) {
        self.0.borrow_mut().fail_writes = true;
    }
}

impl SerialPort for Fake {
    fn write_all(&mut self, bytes: &[u8]) -> Result<(), SerialError> {
        let mut c = self.0.borrow_mut();
        if c.fail_writes {
            return Err(SerialError::Disconnected);
        }
        for &b in bytes {
            match b {
                b'?' => {
                    let report = format!("<{}|MPos:0.000,0.000,0.000|FS:0,0>\r\n", c.state);
                    c.outbox.extend_from_slice(report.as_bytes());
                }
                b'!' | b'~' | 0x18 | 0x85 => c.realtime.push(b),
                b'\n' => {
                    let line = String::from_utf8_lossy(&std::mem::take(&mut c.line)).into_owned();
                    let mut answer = String::new();
                    if line == "$$" {
                        answer.push_str("$30=1000.000\r\n$32=1\r\n");
                    } else {
                        c.received.push(line.clone());
                    }
                    if c.reject.as_ref().is_some_and(|r| line.contains(r.as_str())) {
                        answer.push_str("error:20\r\n");
                    } else {
                        answer.push_str("ok\r\n");
                    }
                    if c.held {
                        c.pending.push_back(answer);
                    } else {
                        c.outbox.extend_from_slice(answer.as_bytes());
                    }
                }
                other => c.line.push(other),
            }
        }
        Ok(())
    }

    fn read_available(&mut self, out: &mut Vec<u8>) -> Result<(), SerialError> {
        out.append(&mut self.0.borrow_mut().outbox);
        Ok(())
    }
}

fn grbl_profile(origin: OriginCorner) -> MachineProfile {
    MachineProfile::new(
        "grbl-diode-400x400",
        1,
        MachineFrame::new(400.0, 400.0, origin).unwrap(),
        1000.0,
        400.0,
        PowerRatio::new(0.0).unwrap(),
        PowerRatio::new(1.0).unwrap(),
    )
    .unwrap()
    .with_capabilities(grbl_capabilities(&GrblProfile::grbl_1_1_laser()))
    .unwrap()
}

fn grbl_machine(h: &mut HeadlessEditorHarness, origin: OriginCorner) {
    h.send(UiIntent::SetGrblMachineProfile {
        profile: Box::new(grbl_profile(origin)),
        grbl: Box::new(GrblProfile::grbl_1_1_laser()),
    });
}

fn grbl(h: &mut HeadlessEditorHarness, intent: GrblIntent) -> burnburn_presentation::AppUpdate {
    h.send(UiIntent::Grbl(intent))
}

fn view(h: &mut HeadlessEditorHarness) -> GrblViewModel {
    h.controller().grbl_view().expect("a GRBL machine")
}

/// A GRBL machine with a fake controller attached and its first status read.
fn rig() -> (HeadlessEditorHarness, Fake) {
    let mut h = HeadlessEditorHarness::new();
    h.new_document().unwrap();
    grbl_machine(&mut h, OriginCorner::BottomLeft);
    let fake = Fake::new();
    h.controller().attach_grbl_port(Box::new(fake.clone()));
    grbl(&mut h, GrblIntent::Tick);
    (h, fake)
}

fn transcript(h: &mut HeadlessEditorHarness) -> Vec<(GrblEntryKind, String)> {
    view(h).console.transcript.into_iter().map(|r| (r.kind, r.text)).collect()
}

#[test]
fn the_window_exists_only_for_a_grbl_machine() {
    let mut h = HeadlessEditorHarness::new();
    h.new_document().unwrap();
    assert!(h.controller().grbl_view().is_none(), "no machine");
    h.send(UiIntent::SetMachineProfile { profile: Box::new(profile(1)), ruida: RUIDA });
    assert!(h.controller().grbl_view().is_none(), "a Ruida machine has the Ruida laser window");
    grbl_machine(&mut h, OriginCorner::BottomLeft);
    assert!(h.controller().grbl_view().is_some());
}

#[test]
fn without_a_port_every_control_is_off_with_the_reason_and_nothing_is_written() {
    let mut h = HeadlessEditorHarness::new();
    h.new_document().unwrap();
    grbl_machine(&mut h, OriginCorner::BottomLeft);
    let v = view(&mut h);
    assert!(!v.connected);
    for c in &v.controls {
        assert!(!c.enabled, "{:?}", c.kind);
        assert!(c.why_not.is_some(), "{:?} says why", c.kind);
    }
    assert_eq!(v.control(GrblControlKind::Home).unwrap().why_not, Some(GrblWhy::NoPort));
    grbl(&mut h, GrblIntent::Send("G0 X5".into()));
    assert_eq!(view(&mut h).last_refusal, Some(GrblWhy::NoPort));
}

#[test]
fn the_state_is_unknown_until_a_status_arrived_and_moves_wait_for_it() {
    let mut h = HeadlessEditorHarness::new();
    h.new_document().unwrap();
    grbl_machine(&mut h, OriginCorner::BottomLeft);
    let fake = Fake::new();
    h.controller().attach_grbl_port(Box::new(fake.clone()));
    let v = view(&mut h);
    assert!(v.connected);
    assert_eq!(v.state, None, "never assumed idle");
    assert_eq!(v.control(GrblControlKind::Jog(StepDirection::Left)).unwrap().why_not, Some(GrblWhy::NoStatusYet));
    grbl(&mut h, GrblIntent::Tick);
    let v = view(&mut h);
    assert_eq!(v.state, Some(GrblMachineState::Idle));
    assert!(v.offers(GrblControlKind::Jog(StepDirection::Left)));
}

#[test]
fn a_console_line_reaches_the_controller_and_its_answer_lands_in_the_transcript() {
    let (mut h, fake) = rig();
    grbl(&mut h, GrblIntent::Send("G0 X5".into()));
    grbl(&mut h, GrblIntent::Tick);
    assert_eq!(fake.received(), ["G0 X5"]);
    let rows = transcript(&mut h);
    assert!(rows.contains(&(GrblEntryKind::Sent, "G0 X5".into())), "{rows:?}");
    assert!(rows.contains(&(GrblEntryKind::Received, "ok".into())), "{rows:?}");
}

#[test]
fn a_controller_error_is_said_with_its_code_and_description() {
    let (mut h, fake) = rig();
    fake.reject("BAD");
    grbl(&mut h, GrblIntent::Send("G99 BAD".into()));
    grbl(&mut h, GrblIntent::Tick);
    let rows = transcript(&mut h);
    let line =
        rows.iter().find(|(k, t)| *k == GrblEntryKind::Received && t.starts_with("error:20")).expect("error row");
    assert!(line.1.len() > "error:20".len(), "the description follows the code: {}", line.1);
}

#[test]
fn history_recall_walks_back_with_up_and_forward_with_down_to_the_empty_line() {
    let (mut h, _fake) = rig();
    for line in ["G0 X1", "G0 X2"] {
        grbl(&mut h, GrblIntent::Send(line.into()));
        grbl(&mut h, GrblIntent::Tick);
    }
    assert_eq!(view(&mut h).console.history, ["G0 X1", "G0 X2"]);
    grbl(&mut h, GrblIntent::RecallPrevious);
    assert_eq!(view(&mut h).console.input, "G0 X2");
    grbl(&mut h, GrblIntent::RecallPrevious);
    assert_eq!(view(&mut h).console.input, "G0 X1");
    grbl(&mut h, GrblIntent::RecallPrevious);
    assert_eq!(view(&mut h).console.input, "G0 X1", "stays at the oldest");
    grbl(&mut h, GrblIntent::RecallNext);
    assert_eq!(view(&mut h).console.input, "G0 X2");
    grbl(&mut h, GrblIntent::RecallNext);
    assert_eq!(view(&mut h).console.input, "", "past the newest is the empty line");
}

#[test]
fn lines_the_console_refuses_are_not_written_and_not_remembered() {
    let (mut h, fake) = rig();
    for (line, why) in [
        ("", ConsoleError::Empty),
        ("   ", ConsoleError::Empty),
        ("G1 X1 !", ConsoleError::ForbiddenCharacter('!')),
        ("G0 X1\u{7f}", ConsoleError::ForbiddenCharacter('\u{7f}')),
    ] {
        grbl(&mut h, GrblIntent::Send(line.into()));
        assert_eq!(view(&mut h).last_refusal, Some(GrblWhy::Console(why)), "{line:?}");
    }
    assert!(fake.received().is_empty());
    assert!(view(&mut h).console.history.is_empty());
}

#[test]
fn clearing_the_transcript_keeps_the_history() {
    let (mut h, _fake) = rig();
    grbl(&mut h, GrblIntent::Send("G0 X1".into()));
    grbl(&mut h, GrblIntent::Tick);
    grbl(&mut h, GrblIntent::ClearTranscript);
    let v = view(&mut h);
    assert!(v.console.transcript.is_empty());
    assert_eq!(v.console.history, ["G0 X1"]);
}

#[test]
fn a_macro_sends_its_lines_one_by_one_each_after_the_previous_was_answered() {
    let (mut h, fake) = rig();
    grbl(
        &mut h,
        GrblIntent::AddMacro { name: "Nulpunt".into(), program: "G10 L20 P1 X0 Y0 ; zero\nG0 X0 Y0\nM5".into() },
    );
    assert_eq!(view(&mut h).macro_row("nulpunt").unwrap().lines, ["G10 L20 P1 X0 Y0", "G0 X0 Y0", "M5"]);

    fake.hold_answers(true);
    grbl(&mut h, GrblIntent::RunMacro("Nulpunt".into()));
    assert_eq!(fake.received(), ["G10 L20 P1 X0 Y0"], "one line, then wait for its answer");
    assert!(view(&mut h).macro_row("Nulpunt").unwrap().running);
    grbl(&mut h, GrblIntent::Tick);
    assert_eq!(fake.received().len(), 1, "no answer yet, nothing more is sent");

    fake.answer_one();
    grbl(&mut h, GrblIntent::Tick);
    assert_eq!(fake.received(), ["G10 L20 P1 X0 Y0", "G0 X0 Y0"]);
    fake.answer_one();
    grbl(&mut h, GrblIntent::Tick);
    fake.answer_one();
    grbl(&mut h, GrblIntent::Tick);
    assert_eq!(fake.received(), ["G10 L20 P1 X0 Y0", "G0 X0 Y0", "M5"]);
    assert!(!view(&mut h).macro_row("Nulpunt").unwrap().running, "done after the last answer");
}

#[test]
fn a_macro_stops_at_the_first_error_and_says_where() {
    let (mut h, fake) = rig();
    grbl(&mut h, GrblIntent::AddMacro { name: "Kapot".into(), program: "G0 X1\nG0 BAD\nG0 X3".into() });
    fake.reject("BAD");
    grbl(&mut h, GrblIntent::RunMacro("Kapot".into()));
    for _ in 0..4 {
        grbl(&mut h, GrblIntent::Tick);
    }
    assert_eq!(fake.received(), ["G0 X1", "G0 BAD"], "the third line is never sent");
    assert!(!view(&mut h).macro_row("Kapot").unwrap().running);
    let rows = transcript(&mut h);
    assert!(rows.iter().any(|(k, t)| *k == GrblEntryKind::Info && t.contains("Kapot") && t.contains("20")), "{rows:?}");
}

#[test]
fn macro_names_are_unique_and_unknown_ones_are_refused() {
    let (mut h, fake) = rig();
    grbl(&mut h, GrblIntent::AddMacro { name: "A".into(), program: "G0 X1".into() });
    grbl(&mut h, GrblIntent::AddMacro { name: "a".into(), program: "G0 X2".into() });
    assert_eq!(view(&mut h).last_refusal, Some(GrblWhy::Macro(MacroError::DuplicateName)));
    grbl(&mut h, GrblIntent::RunMacro("B".into()));
    assert_eq!(view(&mut h).last_refusal, Some(GrblWhy::Macro(MacroError::UnknownMacro)));
    grbl(&mut h, GrblIntent::AddMacro { name: "Leeg".into(), program: "; alleen commentaar".into() });
    assert_eq!(view(&mut h).last_refusal, Some(GrblWhy::Macro(MacroError::NoLines)));
    assert_eq!(view(&mut h).macros.len(), 1);
    assert!(fake.received().is_empty());
}

#[test]
fn macros_are_handed_to_the_host_as_text_and_read_back() {
    let (mut h, _fake) = rig();
    let update = grbl(&mut h, GrblIntent::AddMacro { name: "Home & nul".into(), program: "$H\nG92 X0 Y0".into() });
    let text = update
        .effects
        .iter()
        .find_map(|e| match e {
            HostEffect::GrblMacrosChanged { text } => Some(text.clone()),
            _ => None,
        })
        .expect("the host stores the macros");

    let (mut other, _f) = rig();
    assert!(view(&mut other).macros.is_empty());
    grbl(&mut other, GrblIntent::LoadMacros(text));
    let row = view(&mut other).macros.remove(0);
    assert_eq!(
        (row.name.as_str(), row.lines.as_slice()),
        ("Home & nul", ["$H".to_owned(), "G92 X0 Y0".to_owned()].as_slice())
    );
    grbl(&mut other, GrblIntent::LoadMacros("geen macrobestand".into()));
    assert!(matches!(view(&mut other).last_refusal, Some(GrblWhy::Macro(MacroError::Syntax { .. }))));
    assert_eq!(view(&mut other).macros.len(), 1, "a refused file replaces nothing");
}

/// A driver that already streams a job (the host's own start path, with its own evidence): the
/// fake holds its answers, so the job stays active.
fn streaming_job() -> (HeadlessEditorHarness, Fake) {
    let mut h = HeadlessEditorHarness::new();
    h.new_document().unwrap();
    grbl_machine(&mut h, OriginCorner::BottomLeft);
    let fake = Fake::new();
    let mut driver = GrblDriver::new(GrblPort(Box::new(fake.clone())), SessionConfig::laser(1000));
    driver.request_settings().unwrap();
    driver.pump().unwrap();
    driver.poll_status().unwrap();
    driver.pump().unwrap();
    fake.hold_answers(true);
    driver.start_job(prepare_job("G21\nG90\nM5\nG0 X10 Y10\nG1 X20 F600 S100\nM2").unwrap()).unwrap();
    h.controller().attach_grbl_driver(driver);
    (h, fake)
}

#[test]
fn nothing_but_the_job_moves_the_machine_while_a_job_runs() {
    let (mut h, fake) = streaming_job();
    grbl(&mut h, GrblIntent::AddMacro { name: "Nul".into(), program: "G0 X0".into() });
    let v = view(&mut h);
    assert!(v.job.as_ref().is_some_and(|j| j.state.is_active()));
    let row = v.macro_row("Nul").unwrap();
    assert!(!row.enabled);
    assert_eq!(row.why_not, Some(GrblWhy::JobActive));
    assert!(!v.console.enabled && v.console.why_not == Some(GrblWhy::JobActive));
    let before = fake.received().len();
    grbl(&mut h, GrblIntent::RunMacro("Nul".into()));
    assert_eq!(view(&mut h).last_refusal, Some(GrblWhy::Macro(MacroError::JobActive)));
    grbl(&mut h, GrblIntent::Send("G0 X5".into()));
    assert_eq!(view(&mut h).last_refusal, Some(GrblWhy::Console(ConsoleError::JobActive)));
    for kind in [GrblControlKind::Jog(StepDirection::Left), GrblControlKind::Home, GrblControlKind::Unlock] {
        assert_eq!(view(&mut h).control(kind).unwrap().why_not, Some(GrblWhy::JobActive), "{kind:?}");
    }
    grbl(&mut h, GrblIntent::Home);
    assert_eq!(fake.received().len(), before, "nothing was interleaved with the job");
    // The way out stays open: hold and reset are realtime.
    assert!(view(&mut h).offers(GrblControlKind::Hold));
    assert!(view(&mut h).offers(GrblControlKind::Reset));
}

#[test]
fn a_running_macro_blocks_the_console_and_a_second_macro() {
    let (mut h, fake) = rig();
    grbl(&mut h, GrblIntent::AddMacro { name: "Twee".into(), program: "G0 X1\nG0 X2".into() });
    grbl(&mut h, GrblIntent::AddMacro { name: "Een".into(), program: "G0 X9".into() });
    fake.hold_answers(true);
    grbl(&mut h, GrblIntent::RunMacro("Twee".into()));
    grbl(&mut h, GrblIntent::Send("G0 X5".into()));
    assert_eq!(view(&mut h).last_refusal, Some(GrblWhy::MacroRunning));
    grbl(&mut h, GrblIntent::RunMacro("Een".into()));
    assert_eq!(view(&mut h).last_refusal, Some(GrblWhy::MacroRunning));
    assert_eq!(fake.received(), ["G0 X1"]);
}

#[test]
fn jog_steps_are_relative_moves_of_the_typed_size_and_feed() {
    let (mut h, fake) = rig();
    grbl(&mut h, GrblIntent::SetJogStep("2".into()));
    grbl(&mut h, GrblIntent::SetJogFeed("600".into()));
    grbl(&mut h, GrblIntent::Jog(StepDirection::Left));
    grbl(&mut h, GrblIntent::Tick);
    grbl(&mut h, GrblIntent::Jog(StepDirection::Up));
    grbl(&mut h, GrblIntent::Tick);
    let sent = fake.received();
    assert_eq!(sent.len(), 2, "{sent:?}");
    // Independent oracle: a relative incremental jog with the typed step and feed.
    assert!(
        sent[0].starts_with("$J=") && sent[0].contains("G91") && sent[0].contains("X-2") && sent[0].contains("F600"),
        "{}",
        sent[0]
    );
    assert!(sent[1].contains("Y2") && !sent[1].contains("Y-"), "up is +Y with the origin at the bottom: {}", sent[1]);
}

#[test]
fn up_is_toward_the_top_of_the_bed_whatever_corner_the_origin_is() {
    let mut h = HeadlessEditorHarness::new();
    h.new_document().unwrap();
    grbl_machine(&mut h, OriginCorner::TopLeft);
    let fake = Fake::new();
    h.controller().attach_grbl_port(Box::new(fake.clone()));
    grbl(&mut h, GrblIntent::Tick);
    grbl(&mut h, GrblIntent::Jog(StepDirection::Up));
    assert!(fake.received()[0].contains("Y-1"), "{:?}", fake.received());
}

#[test]
fn right_is_toward_the_right_of_the_bed_when_the_origin_is_at_the_right() {
    let mut h = HeadlessEditorHarness::new();
    h.new_document().unwrap();
    grbl_machine(&mut h, OriginCorner::BottomRight);
    let fake = Fake::new();
    h.controller().attach_grbl_port(Box::new(fake.clone()));
    grbl(&mut h, GrblIntent::Tick);
    grbl(&mut h, GrblIntent::Jog(StepDirection::Right));
    assert!(fake.received()[0].contains("X-1"), "{:?}", fake.received());
}

#[test]
fn an_unusable_step_or_feed_is_refused_and_nothing_is_sent() {
    let (mut h, fake) = rig();
    for (intent, _) in [
        (GrblIntent::SetJogStep("abc".into()), 0),
        (GrblIntent::SetJogFeed("0".into()), 0),
        (GrblIntent::SetJogStep("-1".into()), 0),
    ] {
        grbl(&mut h, intent);
        assert_eq!(view(&mut h).last_refusal, Some(GrblWhy::InvalidNumber));
    }
    assert_eq!(view(&mut h).jog_step_mm, "1", "the old step stays");
    assert!(fake.received().is_empty());
}

#[test]
fn home_and_unlock_send_the_system_commands_and_unlock_only_in_an_alarm() {
    let (mut h, fake) = rig();
    assert!(!view(&mut h).offers(GrblControlKind::Unlock), "nothing to unlock when idle");
    grbl(&mut h, GrblIntent::Home);
    grbl(&mut h, GrblIntent::Tick);
    assert_eq!(fake.received(), ["$H"]);

    fake.set_state("Alarm");
    grbl(&mut h, GrblIntent::Tick);
    let v = view(&mut h);
    assert_eq!(v.state, Some(GrblMachineState::Alarm));
    assert!(v.offers(GrblControlKind::Unlock));
    assert!(!v.offers(GrblControlKind::Jog(StepDirection::Left)), "no jog in an alarm");
    grbl(&mut h, GrblIntent::Unlock);
    assert_eq!(fake.received(), ["$H", "$X"]);
}

#[test]
fn hold_resume_and_reset_are_realtime_bytes_and_a_reset_distrusts_the_position() {
    let (mut h, fake) = rig();
    fake.set_state("Hold:0");
    grbl(&mut h, GrblIntent::Tick);
    assert!(view(&mut h).offers(GrblControlKind::Resume));
    grbl(&mut h, GrblIntent::Hold);
    grbl(&mut h, GrblIntent::Resume);
    grbl(&mut h, GrblIntent::JogCancel);
    grbl(&mut h, GrblIntent::Reset);
    assert_eq!(fake.realtime(), [b'!', b'~', 0x85, 0x18]);
    let v = view(&mut h);
    assert!(!v.position_trusted);
    assert_eq!(v.state, None, "the state is unknown again after a reset");
}

#[test]
fn starting_a_job_is_refused_without_hardware_evidence_and_the_control_says_so() {
    let (mut h, fake) = rig();
    let v = view(&mut h);
    let start = v.control(GrblControlKind::StartJob).unwrap();
    assert!(!start.enabled);
    assert!(matches!(start.why_not, Some(GrblWhy::NotVerified { .. })));
    grbl(&mut h, GrblIntent::StartJob);
    assert!(matches!(view(&mut h).last_refusal, Some(GrblWhy::NotVerified { .. })));
    assert!(fake.received().is_empty());
}

#[test]
fn preparing_a_job_for_upload_is_refused_for_a_grbl_machine_like_a_start() {
    let (mut h, _fake) = rig();
    // Air assist is on, so the plan itself is clean: only the machine's backend refuses the job.
    let clean = GrblProfile { air_assist: true, ..GrblProfile::grbl_1_1_laser() };
    h.send(UiIntent::SetGrblMachineProfile {
        profile: Box::new(grbl_profile(OriginCorner::BottomLeft).with_capabilities(grbl_capabilities(&clean)).unwrap()),
        grbl: Box::new(clean),
    });
    let doc = h.view().active.unwrap();
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (10.0, 10.0), (50.0, 30.0));
    h.invoke(doc, CommandKey::ToolSelect);
    let id = h.document(doc).unwrap().objects[0].id;
    h.send(UiIntent::Select { document: doc, ids: vec![id], mode: burnburn_presentation::SelectMode::Replace });
    h.send(UiIntent::AssignLayer { document: doc, slot: 1, add: false });
    h.invoke(doc, CommandKey::Compile);
    h.run_tasks();
    assert_eq!(h.document(doc).unwrap().plan.expect("plan").errors, 0, "the plan is clean");
    assert!(!h.view().commands.get(&CommandKey::PrepareJob).is_some_and(|c| c.enabled));
    let effects = h.invoke(doc, CommandKey::PrepareJob).effects;
    assert!(
        effects.iter().any(|e| matches!(
            e,
            HostEffect::ShowError { message: MessageKey::LaserRefused(LaserWhy::NotVerified { .. }), .. }
        )),
        "{effects:?}"
    );
    assert!(h.document(doc).unwrap().job.is_none());
}

#[test]
fn a_failing_port_is_reported_and_leaves_the_state_unknown() {
    let (mut h, fake) = rig();
    fake.fail_writes();
    grbl(&mut h, GrblIntent::Send("G0 X5".into()));
    assert_eq!(view(&mut h).last_refusal, Some(GrblWhy::Serial(SerialError::Disconnected)));
    assert_eq!(view(&mut h).state, None, "after a lost connection nothing is assumed");
}

#[test]
fn a_changed_spindle_scale_marks_the_attached_session_as_stale() {
    let (mut h, _fake) = rig();
    assert!(!view(&mut h).settings_stale);
    let g = GrblProfile { max_spindle: 500, ..GrblProfile::grbl_1_1_laser() };
    h.send(UiIntent::SetGrblMachineProfile {
        profile: Box::new(grbl_profile(OriginCorner::BottomLeft).with_capabilities(grbl_capabilities(&g)).unwrap()),
        grbl: Box::new(g),
    });
    assert!(view(&mut h).settings_stale, "the session still expects $30=1000");
}
