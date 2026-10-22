//! The laser window of a GRBL machine (G5-grbl-ui): plain-data types shared by the controller and
//! the hosts. A console (input, history, transcript), named macros, jog/home/unlock and the
//! feed hold / resume / reset controls, all over a [`burnburn_gcode::SerialPort`] that the host
//! hands in. Starting a job is typed-refused until a real controller has proven it
//! (`docs/hardware/evidence.md`); nothing here talks to hardware by itself.

use burnburn_gcode::SerialError;
use burnburn_gcode::SerialPort;
use burnburn_gcode::console::{ConsoleError, MacroError};
use burnburn_gcode::grbl::{CommandRefusal, JobState, MachineState};

use crate::laser::StepDirection;

/// What the host may re-export without a gcode dependency of its own.
pub use burnburn_gcode::RefusalKind as GrblRefusalKind;
pub use burnburn_gcode::grbl::{
    JobFailure as GrblJobFailure, JobState as GrblJobState, MachineState as GrblMachineState,
    UnknownReason as GrblUnknownReason,
};

/// A boxed [`SerialPort`] the controller drives. The host implements the port (the app has no
/// serial adapter yet; tests use a fake controller).
pub struct GrblPort(pub Box<dyn SerialPort>);

impl SerialPort for GrblPort {
    fn write_all(&mut self, bytes: &[u8]) -> Result<(), SerialError> {
        self.0.write_all(bytes)
    }

    fn read_available(&mut self, out: &mut Vec<u8>) -> Result<(), SerialError> {
        self.0.read_available(out)
    }
}

/// Why a GRBL window control is off or an action was refused. Typed, so every host can say it in
/// words; nothing is shown as a success that was not proven.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum GrblWhy {
    /// The active machine is not a GRBL machine (or there is none).
    NotGrblMachine,
    /// No serial port was attached by the host.
    NoPort,
    /// A job is streaming; the console, macros and moves wait until it ends.
    JobActive,
    /// A macro is running; its lines must not interleave with anything else.
    MacroRunning,
    /// The status of the controller has not been read yet (poll first).
    NoStatusYet,
    /// The controller is in a state that does not take this (running, hold, alarm, ...).
    NotIdle(MachineState),
    /// Another line is still waiting for its answer.
    Busy,
    Console(ConsoleError),
    Macro(MacroError),
    Command(CommandRefusal),
    Serial(SerialError),
    /// A number typed in a field is not usable.
    InvalidNumber,
    /// Not proven on a real controller; `needs` names the evidence that is missing.
    NotVerified {
        needs: &'static str,
    },
}

/// The evidence that starting a GRBL job on a real controller needs (`docs/hardware/evidence.md`).
pub const GRBL_START_NEEDS: &str = "a GRBL 1.1 controller of a concrete model: streaming, $32=1, M4 and the stop path";

/// Which control of the GRBL window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GrblControlKind {
    /// A bounded step of the setup's step size in a view direction (`$J=`).
    Jog(StepDirection),
    JogCancel,
    Home,
    Unlock,
    /// Feed hold (`!`).
    Hold,
    /// Cycle start (`~`).
    Resume,
    /// Soft reset (0x18): stops everything; the position is no longer trusted.
    Reset,
    /// Streaming a job: refused until a controller has proven it.
    StartJob,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GrblControl {
    pub kind: GrblControlKind,
    pub enabled: bool,
    pub why_not: Option<GrblWhy>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrblEntryKind {
    Sent,
    Received,
    Info,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrblTranscriptRow {
    pub kind: GrblEntryKind,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GrblConsoleView {
    /// The input line: what the user typed last or recalled with the arrow keys.
    pub input: String,
    /// Oldest first.
    pub history: Vec<String>,
    pub transcript: Vec<GrblTranscriptRow>,
    /// Whether the console takes a line now, and why not.
    pub enabled: bool,
    pub why_not: Option<GrblWhy>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GrblMacroRow {
    pub name: String,
    /// The lines that will be sent, without comments.
    pub lines: Vec<String>,
    pub enabled: bool,
    pub why_not: Option<GrblWhy>,
    pub running: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrblJobView {
    pub state: JobState,
    pub total: usize,
    pub sent: usize,
    pub acked: usize,
}

/// The GRBL window as hosts draw it.
#[derive(Debug, Clone, PartialEq)]
pub struct GrblViewModel {
    /// A serial port is attached.
    pub connected: bool,
    /// The last status report's state; `None` until one arrived (never assumed idle).
    pub state: Option<MachineState>,
    pub alarm: Option<u8>,
    pub job: Option<GrblJobView>,
    /// False after a soft reset: the controller may have lost its position until the next homing.
    pub position_trusted: bool,
    pub console: GrblConsoleView,
    pub macros: Vec<GrblMacroRow>,
    pub controls: Vec<GrblControl>,
    /// Jog step and feed as typed.
    pub jog_step_mm: String,
    pub jog_feed_mm_min: String,
    /// The port was attached for another `$30` than the profile now has: re-attach before a job.
    pub settings_stale: bool,
    /// Why the last action was refused.
    pub last_refusal: Option<GrblWhy>,
}

impl GrblViewModel {
    pub fn control(&self, kind: GrblControlKind) -> Option<&GrblControl> {
        self.controls.iter().find(|c| c.kind == kind)
    }

    pub fn offers(&self, kind: GrblControlKind) -> bool {
        self.control(kind).is_some_and(|c| c.enabled)
    }

    pub fn macro_row(&self, name: &str) -> Option<&GrblMacroRow> {
        self.macros.iter().find(|m| m.name.eq_ignore_ascii_case(name))
    }
}

/// What a control of the GRBL window asks for. Numbers and names arrive as typed.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum GrblIntent {
    /// Send one console line.
    Send(String),
    /// Up arrow: the previous command into the input line.
    RecallPrevious,
    /// Down arrow: the next command, then the empty line.
    RecallNext,
    ClearTranscript,
    RunMacro(String),
    AddMacro {
        name: String,
        program: String,
    },
    ReplaceMacro {
        name: String,
        program: String,
    },
    RenameMacro {
        from: String,
        to: String,
    },
    RemoveMacro(String),
    /// Replace the macros by the text form a host stored (`HostEffect::GrblMacrosChanged`).
    LoadMacros(String),
    Jog(StepDirection),
    SetJogStep(String),
    SetJogFeed(String),
    JogCancel,
    Home,
    Unlock,
    Hold,
    Resume,
    Reset,
    /// Streaming a job. Always refused for now ([`GrblWhy::NotVerified`]).
    StartJob,
    /// The host's timer: ask the controller for its status, read what it said, and let a
    /// running macro send its next line.
    Tick,
}
