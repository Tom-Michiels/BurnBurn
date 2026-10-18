//! Live (non-job) laser-window requests: jog, home, move, pause/resume, position and DSP-file
//! operations. Each one is gated by its own proven capability; nothing here creates an execution
//! or starts a job.

use std::fmt;

pub use burnburn_ruida::{HomeTarget, JogKey};

use crate::controller_files::{Capability, ControllerFileName, FileCapabilities};
use crate::port::PortError;
use crate::position::PositionReading;
use crate::state::ExecutionState;

/// One laser-window request to the controller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LiveRequest {
    JogStart(JogKey),
    JogStop(JogKey),
    Home(HomeTarget),
    /// Absolute move in machine-frame micrometres (numeric move, go to origin, position laser).
    MoveAbsolute {
        x_um: i64,
        y_um: i64,
    },
    Pause,
    Resume,
    ReadPosition,
    ListFiles,
    FileMetadata(ControllerFileName),
    RunFile(ControllerFileName),
    DeleteFile(ControllerFileName),
}

/// The controller feature a request needs proven.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LiveFeature {
    Jog,
    Home,
    MoveAbsolute,
    PauseResume,
    ReadPosition,
    FileList,
    FileMetadata,
    FileRun,
    FileDelete,
    FileUpload,
}

impl LiveRequest {
    pub fn feature(&self) -> LiveFeature {
        match self {
            LiveRequest::JogStart(_) | LiveRequest::JogStop(_) => LiveFeature::Jog,
            LiveRequest::Home(_) => LiveFeature::Home,
            LiveRequest::MoveAbsolute { .. } => LiveFeature::MoveAbsolute,
            LiveRequest::Pause | LiveRequest::Resume => LiveFeature::PauseResume,
            LiveRequest::ReadPosition => LiveFeature::ReadPosition,
            LiveRequest::ListFiles => LiveFeature::FileList,
            LiveRequest::FileMetadata(_) => LiveFeature::FileMetadata,
            LiveRequest::RunFile(_) => LiveFeature::FileRun,
            LiveRequest::DeleteFile(_) => LiveFeature::FileDelete,
        }
    }

    /// Only reads may be resent automatically.
    pub fn is_idempotent(&self) -> bool {
        matches!(self, LiveRequest::ReadPosition | LiveRequest::ListFiles | LiveRequest::FileMetadata(_))
    }

    /// The head moves (or may move) because of this request.
    pub fn moves_head(&self) -> bool {
        matches!(self, LiveRequest::JogStart(_) | LiveRequest::Home(_) | LiveRequest::MoveAbsolute { .. })
    }

    /// Refused while a job may be active: it would fight the job for the head or the file store.
    pub fn needs_idle_machine(&self) -> bool {
        self.moves_head() || matches!(self, LiveRequest::RunFile(_) | LiveRequest::DeleteFile(_))
    }
}

impl LiveFeature {
    /// Hardware evidence that must exist (a row in `docs/hardware/evidence.md`) before the
    /// feature may be switched to `Supported`.
    pub fn required_evidence(self) -> &'static str {
        match self {
            LiveFeature::Jog => {
                "capture of KEYDOWN/KEYUP key events (D8 20..27 / D8 30..37) on the concrete model and firmware, with the head moving while held and stopping on release, and the axis direction of each key"
            }
            LiveFeature::Home => {
                "capture of HOME_XY (D8 2A) on the concrete model and firmware, including the controller's completion signal (an ACK alone does not say the head arrived)"
            }
            LiveFeature::MoveAbsolute => {
                "meaning of the options byte and the coordinate frame of RAPID_MOVE_XY (D9 10) on the concrete model and firmware, with measured head positions"
            }
            LiveFeature::PauseResume => {
                "capture of PAUSE_PROCESS / RESTORE_PROCESS (D8 02 / D8 03) during a real job on the concrete model and firmware, and the status bits that confirm paused and resumed"
            }
            LiveFeature::ReadPosition => {
                "capture of the GET_SETTING reply for the current X/Y memory addresses (04 21 / 04 31): reply framing, units and sign on the concrete model and firmware"
            }
            LiveFeature::FileList => {
                "capture of the DSP file-list request and reply (not in the public registry) on the concrete model and firmware"
            }
            LiveFeature::FileMetadata => {
                "capture of the DSP file-metadata request and reply (not in the public registry) on the concrete model and firmware"
            }
            LiveFeature::FileRun => {
                "capture of running a stored DSP file (not in the public registry) and the status that proves it started, on the concrete model and firmware"
            }
            LiveFeature::FileDelete => {
                "capture of deleting a stored DSP file (not in the public registry) and a file list proving it is gone, on the concrete model and firmware"
            }
            LiveFeature::FileUpload => {
                "capture of a named upload (E5 00 file header, file number and name handling) plus a file list read-back on the concrete model and firmware"
            }
        }
    }
}

/// Live-request capabilities. All start `Unverified` (behaves like `Unsupported`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveCapabilities {
    pub jog: Capability,
    pub home: Capability,
    pub move_absolute: Capability,
    pub pause_resume: Capability,
    pub read_position: Capability,
}

fn unverified() -> Capability {
    Capability::Unverified { reason: "no hardware evidence for this controller".into() }
}

impl Default for LiveCapabilities {
    fn default() -> Self {
        LiveCapabilities {
            jog: unverified(),
            home: unverified(),
            move_absolute: unverified(),
            pause_resume: unverified(),
            read_position: unverified(),
        }
    }
}

/// Everything the laser window may do besides running a compiled job.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Capabilities {
    pub live: LiveCapabilities,
    pub files: FileCapabilities,
}

impl Capabilities {
    pub fn get(&self, feature: LiveFeature) -> &Capability {
        match feature {
            LiveFeature::Jog => &self.live.jog,
            LiveFeature::Home => &self.live.home,
            LiveFeature::MoveAbsolute => &self.live.move_absolute,
            LiveFeature::PauseResume => &self.live.pause_resume,
            LiveFeature::ReadPosition => &self.live.read_position,
            LiveFeature::FileList => &self.files.list,
            LiveFeature::FileMetadata => &self.files.metadata,
            LiveFeature::FileRun => &self.files.run,
            LiveFeature::FileDelete => &self.files.delete,
            LiveFeature::FileUpload => &self.files.upload_named,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LiveError {
    Unsupported {
        feature: LiveFeature,
    },
    /// Not proven on this controller; `needs` names the hardware evidence that is missing.
    NotVerified {
        feature: LiveFeature,
        reason: String,
        needs: &'static str,
    },
    /// The capability is claimed but this build has no wire form for it.
    NoWireForm {
        feature: LiveFeature,
    },
    /// A job may be running (any non-terminal execution): the request would fight it.
    JobMayBeActive(ExecutionState),
    /// A request is still waiting for its reply.
    RequestInFlight,
    /// Another jog key is held (or its release is unconfirmed).
    JogKeyHeld(JogKey),
    Port(PortError),
}

impl fmt::Display for LiveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for LiveError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveUnknown {
    /// No reply arrived in time; the command may or may not have been executed.
    ReplyLost,
    /// The connection was re-established while the request was in flight.
    Reconnected,
    /// The transport reported an error that does not prove the request stayed local.
    SendFailed,
}

/// What is proven about one live attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LiveOutcome {
    /// Sent, waiting for the reply.
    Pending,
    /// The controller acknowledged receipt. Not proof that the head moved or the file ran.
    Acked,
    /// A query was answered with data.
    Answered,
    /// The controller answered NAK/ERR.
    Refused,
    /// The transport could not carry the request; nothing left the machine.
    NotSent,
    Unknown(LiveUnknown),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveAttempt {
    /// Order in which attempts were made on this actor, from 0.
    pub seq: u32,
    pub request: LiveRequest,
    pub outcome: LiveOutcome,
}

/// Live-side state of an actor: attempts, held jog key, last position and file data.
#[derive(Debug, Clone)]
pub(crate) struct LiveState {
    pub(crate) capabilities: Capabilities,
    pub(crate) attempts: Vec<LiveAttempt>,
    pub(crate) jog_held: Option<JogKey>,
    pub(crate) position: PositionReading,
    pub(crate) files: Option<Vec<crate::controller_files::FileEntry>>,
    pub(crate) metadata: Vec<crate::controller_files::FileEntry>,
}

impl Default for LiveState {
    fn default() -> Self {
        LiveState {
            capabilities: Capabilities::default(),
            attempts: Vec::new(),
            jog_held: None,
            position: PositionReading::UNKNOWN,
            files: None,
            metadata: Vec::new(),
        }
    }
}
