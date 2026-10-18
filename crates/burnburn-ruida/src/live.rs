//! Live (non-file) commands of the laser window: jog key events, home, pause/resume/stop and
//! controller memory reads.
//!
//! Every wire form is looked up in the public command registry (`registry.rs`, generated from
//! `research/lightburn/ruida/command-registry.json`); nothing is invented here. A registry entry
//! only proves that a public implementation uses the bytes, not that a given controller model
//! and firmware accepts them: the evidence level is [`WireEvidence::PublicRegistry`] until a row
//! in `docs/hardware/evidence.md` says otherwise. Commands whose parameters the registry does not
//! explain (rapid moves with their `options` byte, file list/run/delete) have no encoder at all.
//!
//! `START_PROCESS` (`D8 00`) is deliberately absent: starting a job stays the stored-job start
//! request of the execution actor.

use std::fmt;

use crate::registry::REGISTRY;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JogKey {
    XLeft,
    XRight,
    YTop,
    YBottom,
    ZUp,
    ZDown,
    UForward,
    UBackwards,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HomeTarget {
    Xy,
    Z,
    U,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LiveProcess {
    Pause,
    Restore,
    Stop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MemoryAddress {
    CardId,
    MachineStatus,
    CurrentX,
    CurrentY,
    CurrentZ,
    CurrentU,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WireEvidence {
    PublicRegistry,
    Hardware,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LiveCommand {
    KeyDown(JogKey),
    KeyUp(JogKey),
    Home(HomeTarget),
    Process(LiveProcess),
    ReadMemory(MemoryAddress),
}

impl LiveCommand {
    /// Every live command is registry-named only; none is hardware-verified in this build.
    pub fn evidence(&self) -> WireEvidence {
        WireEvidence::PublicRegistry
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveEncodeError {
    NotInRegistry(&'static str),
}

impl fmt::Display for LiveEncodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LiveEncodeError::NotInRegistry(label) => write!(f, "label {label} is not in the public registry"),
        }
    }
}

/// The logical (unscrambled) bytes of `command`. Scrambling and datagram framing are the
/// transport's job.
pub fn encode_live(command: &LiveCommand) -> Result<Vec<u8>, LiveEncodeError> {
    let mut out = prefix(command.label())?;
    if let LiveCommand::ReadMemory(addr) = command {
        out.extend_from_slice(&addr.bytes());
    }
    Ok(out)
}

fn prefix(label: &'static str) -> Result<Vec<u8>, LiveEncodeError> {
    REGISTRY
        .iter()
        .find(|(_, l)| l.split('/').any(|x| x == label))
        .map(|(bytes, _)| bytes.to_vec())
        .ok_or(LiveEncodeError::NotInRegistry(label))
}

impl LiveCommand {
    fn label(&self) -> &'static str {
        match self {
            LiveCommand::KeyDown(k) => match k {
                JogKey::XLeft => "KEYDOWN_X_LEFT",
                JogKey::XRight => "KEYDOWN_X_RIGHT",
                JogKey::YTop => "KEYDOWN_Y_TOP",
                JogKey::YBottom => "KEYDOWN_Y_BOTTOM",
                JogKey::ZUp => "KEYDOWN_Z_UP",
                JogKey::ZDown => "KEYDOWN_Z_DOWN",
                JogKey::UForward => "KEYDOWN_U_FORWARD",
                JogKey::UBackwards => "KEYDOWN_U_BACKWARDS",
            },
            LiveCommand::KeyUp(k) => match k {
                JogKey::XLeft => "KEYUP_X_LEFT",
                JogKey::XRight => "KEYUP_X_RIGHT",
                JogKey::YTop => "KEYUP_Y_TOP",
                JogKey::YBottom => "KEYUP_Y_BOTTOM",
                JogKey::ZUp => "KEYUP_Z_UP",
                JogKey::ZDown => "KEYUP_Z_DOWN",
                JogKey::UForward => "KEYUP_U_FORWARD",
                JogKey::UBackwards => "KEYUP_U_BACKWARDS",
            },
            LiveCommand::Home(HomeTarget::Xy) => "HOME_XY",
            LiveCommand::Home(HomeTarget::Z) => "HOME_Z",
            LiveCommand::Home(HomeTarget::U) => "HOME_U",
            LiveCommand::Process(LiveProcess::Pause) => "PAUSE_PROCESS",
            LiveCommand::Process(LiveProcess::Restore) => "RESTORE_PROCESS",
            LiveCommand::Process(LiveProcess::Stop) => "STOP_PROCESS",
            LiveCommand::ReadMemory(_) => "GET_SETTING",
        }
    }
}

impl MemoryAddress {
    /// The two 7-bit address bytes from the registry's memory table.
    fn bytes(self) -> [u8; 2] {
        match self {
            MemoryAddress::CardId => [0x05, 0x7e],
            MemoryAddress::MachineStatus => [0x04, 0x00],
            MemoryAddress::CurrentX => [0x04, 0x21],
            MemoryAddress::CurrentY => [0x04, 0x31],
            MemoryAddress::CurrentZ => [0x04, 0x41],
            MemoryAddress::CurrentU => [0x04, 0x51],
        }
    }
}
