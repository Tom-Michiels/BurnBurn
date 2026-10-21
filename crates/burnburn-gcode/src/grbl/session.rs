//! The streaming session: a pure state machine over controller lines and user intents.
//!
//! It never does IO. Every method returns a [`Step`]: the bytes the host must write and the
//! events that happened. Rules (AGENTS.md: report unverified states, never auto-restart):
//! - **Character counting**: unacknowledged bytes in the controller's RX buffer never exceed
//!   `rx_buffer - 1`; realtime bytes are not counted (GRBL handles them out of band).
//! - **`ok` is not completion**: a job is `Completed` only when a status report says `Idle`
//!   *after* the last line was acknowledged.
//! - **Faults stop the job**: `error:N` on a job line, `ALARM:N`, an alarm state, a reset banner
//!   or a protocol desync end the job in a typed `Failed` state. After an `error:N` a feed hold
//!   is sent so the already buffered lines stop; nothing resumes or restarts by itself, and
//!   `resume` is refused for a failed job.
//! - **Lost transport** makes the job `Unknown`, never `Failed` or `Completed`.

use std::collections::VecDeque;

use crate::grbl::message::{MachineState, Message, parse_line};
use crate::grbl::prepare::PreparedJob;
use crate::number;

pub const RX_BUFFER_BYTES: usize = 128;

pub const REALTIME_STATUS: u8 = b'?';
pub const REALTIME_HOLD: u8 = b'!';
pub const REALTIME_RESUME: u8 = b'~';
pub const REALTIME_SOFT_RESET: u8 = 0x18;
pub const REALTIME_JOG_CANCEL: u8 = 0x85;

#[derive(Debug, Clone, PartialEq)]
pub struct SessionConfig {
    /// Size of the controller's RX buffer; at most `rx_buffer - 1` bytes are ever outstanding.
    pub rx_buffer: usize,
    /// Settings that must have been read back (`$$`) with exactly these values before a job
    /// starts, e.g. `(32, 1.0)` for laser mode.
    pub expected_settings: Vec<(u16, f64)>,
}

impl SessionConfig {
    /// Stock GRBL 1.1 laser: 128-byte RX buffer, `$32=1` and `$30` = the profile's maximum.
    pub fn laser(max_spindle: u32) -> SessionConfig {
        SessionConfig { rx_buffer: RX_BUFFER_BYTES, expected_settings: vec![(32, 1.0), (30, f64::from(max_spindle))] }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobFailure {
    /// The controller rejected job line `index` (0-based, in the prepared job).
    LineRejected {
        index: usize,
        text: String,
        code: u8,
    },
    Alarm {
        code: Option<u8>,
    },
    /// The controller restarted (reset banner) while the job was active.
    ControllerReset,
    /// An `ok` without an outstanding line: the stream and the controller disagree.
    Desync,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobState {
    /// Lines are being sent and acknowledged.
    Streaming,
    /// Every line is acknowledged; waiting for the controller to report `Idle`.
    Draining,
    Completed,
    Failed(JobFailure),
    Cancelled,
    /// The outcome is uncertain: the connection was lost while the job was active.
    Unknown(UnknownReason),
}

impl JobState {
    pub fn is_active(&self) -> bool {
        matches!(self, JobState::Streaming | JobState::Draining)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnknownReason {
    ConnectionLost,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobProgress {
    pub state: JobState,
    pub total: usize,
    pub sent: usize,
    pub acked: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SessionEvent {
    Status(MachineState),
    Alarm(u8),
    Feedback(String),
    SettingObserved {
        id: u16,
        value: String,
    },
    JobState(JobState),
    /// `error:N` for a non-job command.
    CommandError {
        code: u8,
    },
    CommandAccepted,
    /// A line the parser rejected or a response nobody waits for.
    Unexpected(String),
}

/// What the host must do after a call.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Step {
    pub write: Vec<u8>,
    pub events: Vec<SessionEvent>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StartRefusal {
    Busy,
    NoStatusYet,
    NotIdle(MachineState),
    Alarm,
    SettingNotConfirmed {
        id: u16,
    },
    SettingMismatch {
        id: u16,
        expected: f64,
        actual: String,
    },
    EmptyJob,
    /// A line (with its newline) is longer than the usable RX buffer and could never be sent.
    LineExceedsBuffer {
        index: usize,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum CommandRefusal {
    JobActive,
    Busy,
    NoStatusYet,
    NotIdle(MachineState),
    InvalidLine,
    InvalidJog,
    /// Resume after a failed or unknown job would continue a program the controller rejected.
    ResumeAfterFault,
}

#[derive(Debug, Clone, PartialEq)]
pub enum JogTarget {
    Relative { dx_mm: f64, dy_mm: f64 },
    Absolute { x_mm: f64, y_mm: f64 },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    Jog {
        target: JogTarget,
        feed_mm_min: f64,
    },
    Home,
    Unlock,
    /// One console/macro line.
    Line(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Owner {
    Job(usize),
    Command,
}

pub struct GrblSession {
    config: SessionConfig,
    machine: Option<MachineState>,
    alarm: Option<u8>,
    settings: Vec<(u16, String)>,
    job: Option<Active>,
    /// Lines written and not yet answered: (bytes including newline, who sent it), oldest first.
    inflight: VecDeque<(usize, Owner)>,
    position_trusted: bool,
    homing: bool,
}

struct Active {
    lines: Vec<String>,
    state: JobState,
    sent: usize,
    acked: usize,
}

fn mm_word(v: f64) -> Option<String> {
    number::quantise(v, 3).map(|q| number::format(q, 3))
}

/// A console/macro line must be one clean printable line: no terminator, no control or
/// non-ASCII character, and none of the characters GRBL treats as realtime commands.
fn clean_line(line: &str) -> bool {
    !line.trim().is_empty() && line.chars().all(|c| (' '..='~').contains(&c) && !matches!(c, '?' | '!' | '~'))
}

impl GrblSession {
    pub fn new(config: SessionConfig) -> GrblSession {
        GrblSession {
            config,
            machine: None,
            alarm: None,
            settings: Vec::new(),
            job: None,
            inflight: VecDeque::new(),
            position_trusted: true,
            homing: false,
        }
    }

    /// Last machine state a status report gave; `None` after a reset or lost connection.
    pub fn machine_state(&self) -> Option<MachineState> {
        self.machine
    }
    pub fn alarm(&self) -> Option<u8> {
        self.alarm
    }
    /// False after a soft reset or an unexpected controller reset: the controller may have
    /// lost its position until the next homing.
    pub fn position_trusted(&self) -> bool {
        self.position_trusted
    }
    pub fn setting(&self, id: u16) -> Option<&str> {
        self.settings.iter().find(|(i, _)| *i == id).map(|(_, v)| v.as_str())
    }
    pub fn job(&self) -> Option<JobProgress> {
        self.job.as_ref().map(|j| JobProgress {
            state: j.state.clone(),
            total: j.lines.len(),
            sent: j.sent,
            acked: j.acked,
        })
    }
    /// Bytes written to the controller and not yet acknowledged.
    pub fn bytes_in_flight(&self) -> usize {
        self.inflight.iter().map(|(n, _)| n).sum()
    }
    pub fn config(&self) -> &SessionConfig {
        &self.config
    }

    fn usable(&self) -> usize {
        self.config.rx_buffer.saturating_sub(1)
    }

    fn job_active(&self) -> bool {
        self.job.as_ref().is_some_and(|j| j.state.is_active())
    }

    fn set_job_state(&mut self, state: JobState, step: &mut Step) {
        if let Some(j) = self.job.as_mut() {
            j.state = state.clone();
            step.events.push(SessionEvent::JobState(state));
        }
    }

    /// Sends as many further job lines as fit the RX buffer.
    fn fill_window(&mut self, step: &mut Step) {
        let usable = self.usable();
        loop {
            let in_flight = self.bytes_in_flight();
            let Some(j) = self.job.as_mut() else { return };
            if j.state != JobState::Streaming || j.sent >= j.lines.len() {
                return;
            }
            let line = &j.lines[j.sent];
            let len = line.len() + 1;
            if in_flight + len > usable {
                return;
            }
            step.write.extend_from_slice(line.as_bytes());
            step.write.push(b'\n');
            self.inflight.push_back((len, Owner::Job(j.sent)));
            j.sent += 1;
        }
    }

    /// Fails the active job and, because lines are still buffered in the controller, feeds hold.
    fn fail_and_hold(&mut self, failure: JobFailure, step: &mut Step) {
        self.set_job_state(JobState::Failed(failure), step);
        step.write.push(REALTIME_HOLD);
    }

    pub fn on_line(&mut self, line: &str) -> Step {
        let mut step = Step::default();
        let message = match parse_line(line) {
            Ok(m) => m,
            Err(e) => {
                step.events.push(SessionEvent::Unexpected(format!("{line:?}: {e}")));
                return step;
            }
        };
        match message {
            Message::Ok => self.on_ok(&mut step),
            Message::Error(code) => self.on_error(code, &mut step),
            Message::Alarm(code) => {
                self.alarm = Some(code);
                self.machine = Some(MachineState::Alarm);
                self.inflight.clear();
                self.homing = false;
                step.events.push(SessionEvent::Alarm(code));
                if self.job_active() {
                    self.set_job_state(JobState::Failed(JobFailure::Alarm { code: Some(code) }), &mut step);
                }
            }
            Message::Status(report) => {
                self.machine = Some(report.state);
                if report.state != MachineState::Alarm {
                    self.alarm = None;
                }
                step.events.push(SessionEvent::Status(report.state));
                match (self.job.as_ref().map(|j| j.state.clone()), report.state) {
                    (Some(JobState::Streaming | JobState::Draining), MachineState::Alarm) => {
                        self.set_job_state(JobState::Failed(JobFailure::Alarm { code: None }), &mut step);
                    }
                    (Some(JobState::Draining), MachineState::Idle) => {
                        self.set_job_state(JobState::Completed, &mut step);
                    }
                    _ => {}
                }
            }
            Message::Welcome { .. } => {
                self.machine = None;
                self.alarm = None;
                self.inflight.clear();
                self.homing = false;
                if self.job_active() {
                    self.position_trusted = false;
                    self.set_job_state(JobState::Failed(JobFailure::ControllerReset), &mut step);
                }
            }
            Message::Setting { id, value } => {
                match self.settings.iter_mut().find(|(i, _)| *i == id) {
                    Some(entry) => entry.1.clone_from(&value),
                    None => self.settings.push((id, value.clone())),
                }
                step.events.push(SessionEvent::SettingObserved { id, value });
            }
            Message::Feedback(text) => step.events.push(SessionEvent::Feedback(text)),
            Message::Other(text) => {
                if !text.is_empty() {
                    step.events.push(SessionEvent::Unexpected(text));
                }
            }
        }
        step
    }

    fn on_ok(&mut self, step: &mut Step) {
        match self.inflight.pop_front() {
            None => {
                step.events.push(SessionEvent::Unexpected("ok without an outstanding line".into()));
                if self.job_active() {
                    self.fail_and_hold(JobFailure::Desync, step);
                }
            }
            Some((_, Owner::Command)) => {
                if std::mem::take(&mut self.homing) {
                    self.position_trusted = true;
                }
                step.events.push(SessionEvent::CommandAccepted);
            }
            Some((_, Owner::Job(_))) => {
                let Some(j) = self.job.as_mut() else { return };
                if j.state != JobState::Streaming {
                    return; // late ok of a job that already ended
                }
                j.acked += 1;
                if j.acked == j.lines.len() {
                    self.set_job_state(JobState::Draining, step);
                } else {
                    self.fill_window(step);
                }
            }
        }
    }

    fn on_error(&mut self, code: u8, step: &mut Step) {
        match self.inflight.pop_front() {
            None => step.events.push(SessionEvent::Unexpected(format!("error:{code} without an outstanding line"))),
            Some((_, Owner::Command)) => {
                self.homing = false;
                step.events.push(SessionEvent::CommandError { code });
            }
            Some((_, Owner::Job(index))) => {
                if self.job_active() {
                    let text = self.job.as_ref().and_then(|j| j.lines.get(index)).cloned().unwrap_or_default();
                    self.fail_and_hold(JobFailure::LineRejected { index, text, code }, step);
                }
            }
        }
    }

    /// Requests a status report (`?`, realtime).
    pub fn poll_status(&mut self) -> Step {
        Step { write: vec![REALTIME_STATUS], events: Vec::new() }
    }

    /// Asks the controller for all settings (`$$`); the answers arrive as setting lines.
    pub fn request_settings(&mut self) -> Result<Step, CommandRefusal> {
        self.send_line("$$".into())
    }

    pub fn start_job(&mut self, job: PreparedJob) -> Result<Step, StartRefusal> {
        if self.job_active() || !self.inflight.is_empty() {
            return Err(StartRefusal::Busy);
        }
        for &(id, expected) in &self.config.expected_settings {
            let Some(actual) = self.setting(id) else { return Err(StartRefusal::SettingNotConfirmed { id }) };
            match actual.parse::<f64>() {
                Ok(v) if (v - expected).abs() < 1e-9 => {}
                _ => return Err(StartRefusal::SettingMismatch { id, expected, actual: actual.to_owned() }),
            }
        }
        match self.machine {
            None => return Err(StartRefusal::NoStatusYet),
            Some(MachineState::Alarm) => return Err(StartRefusal::Alarm),
            Some(MachineState::Idle) if self.alarm.is_none() => {}
            Some(other) => return Err(StartRefusal::NotIdle(other)),
        }
        if job.is_empty() {
            return Err(StartRefusal::EmptyJob);
        }
        let usable = self.usable();
        if let Some(index) = job.lines().iter().position(|l| l.len() + 1 > usable) {
            return Err(StartRefusal::LineExceedsBuffer { index });
        }
        self.job = Some(Active { lines: job.lines().to_vec(), state: JobState::Streaming, sent: 0, acked: 0 });
        let mut step = Step::default();
        step.events.push(SessionEvent::JobState(JobState::Streaming));
        self.fill_window(&mut step);
        Ok(step)
    }

    /// Feed hold (`!`, realtime). Always allowed; the job keeps its state.
    pub fn hold(&mut self) -> Step {
        Step { write: vec![REALTIME_HOLD], events: Vec::new() }
    }

    /// Cycle start / resume (`~`, realtime). Refused after a failed, cancelled or unknown job:
    /// that would continue a program the controller rejected or whose state we lost.
    pub fn resume(&mut self) -> Result<Step, CommandRefusal> {
        if let Some(j) = &self.job
            && matches!(j.state, JobState::Failed(_) | JobState::Unknown(_) | JobState::Cancelled)
        {
            return Err(CommandRefusal::ResumeAfterFault);
        }
        Ok(Step { write: vec![REALTIME_RESUME], events: Vec::new() })
    }

    /// Soft reset (0x18, realtime): stops everything at once. The position is no longer trusted.
    pub fn cancel(&mut self) -> Step {
        let mut step = Step { write: vec![REALTIME_SOFT_RESET], events: Vec::new() };
        if self.job_active() {
            self.set_job_state(JobState::Cancelled, &mut step);
        }
        self.inflight.clear();
        self.homing = false;
        self.machine = None;
        self.position_trusted = false;
        step
    }

    /// Jog cancel (0x85, realtime). Always allowed.
    pub fn jog_cancel(&mut self) -> Step {
        Step { write: vec![REALTIME_JOG_CANCEL], events: Vec::new() }
    }

    fn send_line(&mut self, line: String) -> Result<Step, CommandRefusal> {
        if self.job_active() {
            return Err(CommandRefusal::JobActive);
        }
        if !self.inflight.is_empty() {
            return Err(CommandRefusal::Busy);
        }
        let len = line.len() + 1;
        if len > self.usable() {
            return Err(CommandRefusal::InvalidLine);
        }
        let mut write = line.into_bytes();
        write.push(b'\n');
        self.inflight.push_back((len, Owner::Command));
        Ok(Step { write, events: Vec::new() })
    }

    pub fn command(&mut self, command: Command) -> Result<Step, CommandRefusal> {
        if self.job_active() {
            return Err(CommandRefusal::JobActive);
        }
        match command {
            Command::Home => {
                let step = self.send_line("$H".into())?;
                self.homing = true;
                Ok(step)
            }
            Command::Unlock => self.send_line("$X".into()),
            Command::Line(line) => {
                if !clean_line(&line) {
                    return Err(CommandRefusal::InvalidLine);
                }
                self.send_line(line)
            }
            Command::Jog { target, feed_mm_min } => {
                let (mode, a, b) = match target {
                    JogTarget::Relative { dx_mm, dy_mm } => ("G91", dx_mm, dy_mm),
                    JogTarget::Absolute { x_mm, y_mm } => ("G90", x_mm, y_mm),
                };
                let words = (mm_word(a), mm_word(b), mm_word(feed_mm_min));
                let (Some(x), Some(y), Some(f)) = words else { return Err(CommandRefusal::InvalidJog) };
                if !(feed_mm_min > 0.0) || a.abs() > 100_000.0 || b.abs() > 100_000.0 {
                    return Err(CommandRefusal::InvalidJog);
                }
                match self.machine {
                    None => return Err(CommandRefusal::NoStatusYet),
                    Some(MachineState::Idle | MachineState::Jog) => {}
                    Some(other) => return Err(CommandRefusal::NotIdle(other)),
                }
                self.send_line(format!("$J=G21{mode}X{x}Y{y}F{f}"))
            }
        }
    }

    /// The connection to the controller broke. A running job becomes `Unknown`: it may be
    /// running, finished or stopped, and nothing here will guess.
    pub fn transport_lost(&mut self) -> Step {
        let mut step = Step::default();
        if self.job_active() {
            self.set_job_state(JobState::Unknown(UnknownReason::ConnectionLost), &mut step);
        }
        self.inflight.clear();
        self.homing = false;
        self.machine = None;
        step
    }
}
