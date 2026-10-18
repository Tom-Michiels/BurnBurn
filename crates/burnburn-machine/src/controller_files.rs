//! Controller file management (IM29, M09): listing, named upload and deletion are explicit
//! user intents, each gated by its own proven capability. Nothing in the execution flow lists,
//! replaces or deletes a controller file implicitly (cancel keeps a partial file, see the actor).
//!
//! No wire form is defined here: the file commands of Ruida controllers are model-bound
//! observations (`research/rudia/03-eigen-software.md`), so every capability starts
//! `Unverified`, which behaves like `Unsupported`.

use std::fmt;

use crate::artifact::RdArtifact;

/// Evidence level of one controller feature (contracts-phase-c principle 4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Capability {
    Supported,
    Unsupported,
    /// Claimed or plausible, not proven on this model/firmware: treated as unsupported.
    Unverified {
        reason: String,
    },
}

impl Capability {
    pub fn allows(&self) -> bool {
        matches!(self, Capability::Supported)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileCapabilities {
    pub list: Capability,
    pub metadata: Capability,
    pub upload_named: Capability,
    pub run: Capability,
    pub delete: Capability,
}

impl Default for FileCapabilities {
    fn default() -> Self {
        let unverified = || Capability::Unverified { reason: "no hardware evidence for this controller".into() };
        FileCapabilities {
            list: unverified(),
            metadata: unverified(),
            upload_named: unverified(),
            run: unverified(),
            delete: unverified(),
        }
    }
}

/// A file name on the controller. Encoding and length limits are model-specific and unknown;
/// only names that cannot be ambiguous on any wire are accepted (printable ASCII, no NUL).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ControllerFileName(String);

impl ControllerFileName {
    pub fn new(name: &str) -> Result<Self, FileOpError> {
        let ok = !name.is_empty() && name.bytes().all(|b| (0x20..0x7f).contains(&b));
        if ok { Ok(ControllerFileName(name.to_owned())) } else { Err(FileOpError::InvalidName) }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One stored file as the controller reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    pub name: ControllerFileName,
    /// `None` when the controller did not report a size (never 0 by default).
    pub size_bytes: Option<u64>,
}

/// An explicit file action chosen by the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ControllerFileIntent {
    List,
    Metadata { name: ControllerFileName },
    Upload { name: ControllerFileName, artifact: RdArtifact },
    /// Run a stored file. Not the start of an uploaded job: it is its own intent.
    Run { name: ControllerFileName },
    Delete { name: ControllerFileName },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileOpError {
    InvalidName,
    Unsupported { operation: &'static str },
    Unverified { operation: &'static str, reason: String },
}

impl fmt::Display for FileOpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for FileOpError {}

/// May this intent be sent to a controller with these capabilities?
pub fn authorize(intent: &ControllerFileIntent, caps: &FileCapabilities) -> Result<(), FileOpError> {
    let (operation, cap) = match intent {
        ControllerFileIntent::List => ("list", &caps.list),
        ControllerFileIntent::Metadata { .. } => ("metadata", &caps.metadata),
        ControllerFileIntent::Upload { .. } => ("upload", &caps.upload_named),
        ControllerFileIntent::Run { .. } => ("run", &caps.run),
        ControllerFileIntent::Delete { .. } => ("delete", &caps.delete),
    };
    match cap {
        Capability::Supported => Ok(()),
        Capability::Unsupported => Err(FileOpError::Unsupported { operation }),
        Capability::Unverified { reason } => Err(FileOpError::Unverified { operation, reason: reason.clone() }),
    }
}
