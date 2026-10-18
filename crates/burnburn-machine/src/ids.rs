use std::fmt;

/// Stable identity of one physical controller (e.g. a configured profile ID or a probed card ID).
/// Replies are only accepted from the machine the execution belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MachineId(String);

impl MachineId {
    pub fn new(id: impl Into<String>) -> Self {
        MachineId(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for MachineId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// What is known about the machine. Model/firmware stay `None` until confirmed by a probe;
/// they are never guessed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MachineIdentity {
    pub id: MachineId,
    pub model: Option<String>,
    pub firmware: Option<String>,
}

impl MachineIdentity {
    pub fn unprobed(id: MachineId) -> Self {
        MachineIdentity { id, model: None, firmware: None }
    }

    /// True when at least one identifying field is configured, so a reply can be compared.
    pub fn is_verifiable(&self) -> bool {
        self.model.is_some() || self.firmware.is_some()
    }

    /// Every configured field must be reported identically; a field the controller does not
    /// report counts as a mismatch (unconfirmed is not confirmed).
    pub fn matches(&self, reported: &ControllerIdentity) -> bool {
        let field = |want: &Option<String>, got: &Option<String>| want.as_ref().is_none_or(|w| got.as_ref() == Some(w));
        self.is_verifiable() && field(&self.model, &reported.model) && field(&self.firmware, &reported.firmware)
    }
}

/// What a controller reported about itself in reply to an identity query.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ControllerIdentity {
    pub model: Option<String>,
    pub firmware: Option<String>,
}

/// One attempt to run one compiled job revision on one machine. Never reused. Opaque (DR-10):
/// the production history allocates it (`max(seen) + 1`, ADR 0008); nobody invents one ad hoc.
///
/// ```compile_fail
/// let id = burnburn_machine::ExecutionId(7); // error[E0423]: tuple struct constructor is private
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ExecutionId(u64);

impl ExecutionId {
    pub fn from_raw(raw: u64) -> Self {
        ExecutionId(raw)
    }

    pub fn raw(self) -> u64 {
        self.0
    }
}

impl fmt::Display for ExecutionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "exec-{}", self.0)
    }
}
