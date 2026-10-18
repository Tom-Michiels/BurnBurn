//! Machine driver glue: history first, then the actor (ADR 0008 §5, contracts-phase-c §IM17).
//!
//! Every machine intent is recorded durably (`IntentRecorded`, synced) **before** the actor —
//! and through it the transport — receives it. If the record cannot be made durable, nothing
//! is sent. After each actor step the observed evidence and state are journaled. A repeated
//! action id is a no-op; a start is never sent for an execution whose history already holds a
//! start intent, also not after a restart.

use std::fmt;
use std::sync::Arc;

use burnburn_history::{
    ActionId, CompiledJobId, ExecutionStateDto, History, HistoryError, IntentKind, JournalFs, QueueItemId, WallTime,
};
use burnburn_job::ProfileRevision;
use burnburn_machine::{
    ActorError, Capabilities, ControllerPort, ExecutionActor, ExecutionId, ExecutionState, LiveError, LiveOutcome,
    LiveRequest,
};

/// Wall-clock port: display time for history records only.
pub trait WallClock: Send + Sync {
    fn now(&self) -> WallTime;
}

/// `SystemTime` as milliseconds since the Unix epoch (before 1970 → the epoch).
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemWallClock;

impl WallClock for SystemWallClock {
    fn now(&self) -> WallTime {
        let ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX));
        WallTime::from_unix_millis(ms)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum DriverError {
    History(HistoryError),
    Actor(ActorError),
    Live(LiveError),
    UnknownExecution(ExecutionId),
    /// The history already holds a start intent for this execution: never a second one.
    StartAlreadyRecorded,
    /// The execution is not (freshly) proven stored in this session.
    NotStartable(ExecutionStateDto),
}

impl From<HistoryError> for DriverError {
    fn from(e: HistoryError) -> Self {
        DriverError::History(e)
    }
}

impl From<LiveError> for DriverError {
    fn from(e: LiveError) -> Self {
        DriverError::Live(e)
    }
}

impl From<ActorError> for DriverError {
    fn from(e: ActorError) -> Self {
        DriverError::Actor(e)
    }
}

impl fmt::Display for DriverError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for DriverError {}

/// One machine: its actor plus the production history. Single-threaded and step-driven, like
/// the actor; the owning service thread calls [`MachineDriver::poll`].
pub struct MachineDriver<P: ControllerPort> {
    actor: ExecutionActor<P>,
    history: History,
    fs: Arc<dyn JournalFs>,
    wall: Arc<dyn WallClock>,
    max_chunk: usize,
    /// Which journaled laser-window action each actor attempt belongs to.
    live_tracks: Vec<LiveTrack>,
    tracked_attempts: usize,
}

struct LiveTrack {
    seq: u32,
    action: ActionId,
    journaled: bool,
}

impl<P: ControllerPort> MachineDriver<P> {
    pub fn new(actor: ExecutionActor<P>, history: History, fs: Arc<dyn JournalFs>, wall: Arc<dyn WallClock>) -> Self {
        MachineDriver {
            actor,
            history,
            fs,
            wall,
            max_chunk: burnburn_ruida::udp::MAX_PAYLOAD,
            live_tracks: Vec::new(),
            tracked_attempts: 0,
        }
    }

    pub fn actor(&self) -> &ExecutionActor<P> {
        &self.actor
    }

    pub fn history(&self) -> &History {
        &self.history
    }

    pub fn into_history(self) -> History {
        self.history
    }

    /// New execution of a frozen job: `ExecutionCreated` → `IntentRecorded(Upload)` → actor.
    /// The RD bytes come from the job's blob, never from a new compile.
    pub fn upload(
        &mut self,
        job: CompiledJobId,
        item: Option<QueueItemId>,
        current_profile: &ProfileRevision,
        action: ActionId,
    ) -> Result<ExecutionId, DriverError> {
        self.upload_as(IntentKind::Upload, job, item, current_profile, action)
    }

    fn upload_as(
        &mut self,
        kind: IntentKind,
        job: CompiledJobId,
        item: Option<QueueItemId>,
        current_profile: &ProfileRevision,
        action: ActionId,
    ) -> Result<ExecutionId, DriverError> {
        if let Some(done) = self.history.action_outcome(action) {
            return Ok(done.execution);
        }
        let fs = Arc::clone(&self.fs);
        let artifact = self.history.load_artifact(fs.as_ref(), job, self.max_chunk)?;
        let now = self.wall.now();
        let machine = self.actor.machine().id.clone();
        let id = self.history.create_execution(fs.as_ref(), job, machine, item, current_profile, now)?;
        self.history.record_intent(fs.as_ref(), id, action, kind, now)?;
        self.actor.begin_upload(id, &artifact)?;
        self.observe()?;
        Ok(id)
    }

    /// Read-back of the current upload.
    pub fn confirm_stored(&mut self, action: ActionId) -> Result<(), DriverError> {
        let id = self.actor.execution().map(|e| e.id).ok_or(DriverError::Actor(ActorError::NoExecution))?;
        let fs = Arc::clone(&self.fs);
        if self.history.record_intent(fs.as_ref(), id, action, IntentKind::ConfirmStored, self.wall.now())?.fresh {
            self.actor.confirm_stored()?;
            self.observe()?;
        }
        Ok(())
    }

    /// New session for an execution uploaded earlier (e.g. after a restart): identity and
    /// read-back again, no job bytes. Refused once a start intent exists.
    pub fn reverify(&mut self, execution: ExecutionId, action: ActionId) -> Result<(), DriverError> {
        let view = self.history.execution(execution).ok_or(DriverError::UnknownExecution(execution))?;
        if view.intent_recorded(IntentKind::Start) {
            return Err(DriverError::StartAlreadyRecorded);
        }
        let job = view.job;
        let fs = Arc::clone(&self.fs);
        let artifact = self.history.load_artifact(fs.as_ref(), job, self.max_chunk)?;
        if self.history.record_intent(fs.as_ref(), execution, action, IntentKind::ConfirmStored, self.wall.now())?.fresh
        {
            self.actor.begin_reverify(execution, &artifact)?;
            self.observe()?;
        }
        Ok(())
    }

    /// Start a stored execution once. The start intent is durable before the actor sees it.
    pub fn start(&mut self, execution: ExecutionId, action: ActionId) -> Result<(), DriverError> {
        let view = self.history.execution(execution).ok_or(DriverError::UnknownExecution(execution))?;
        if let Some(done) = self.history.action_outcome(action) {
            // The same click again (reconnect, double click): show the outcome, send nothing.
            return if done.execution == execution { Ok(()) } else { Err(DriverError::StartAlreadyRecorded) };
        }
        if view.intent_recorded(IntentKind::Start) {
            return Err(DriverError::StartAlreadyRecorded);
        }
        let stored_now = self.actor.execution().is_some_and(|e| e.id == execution && e.state == ExecutionState::Stored);
        if !stored_now {
            return Err(DriverError::NotStartable(view.state.clone()));
        }
        let fs = Arc::clone(&self.fs);
        self.history.record_intent(fs.as_ref(), execution, action, IntentKind::Start, self.wall.now())?;
        self.actor.start()?;
        self.observe()?;
        Ok(())
    }

    /// Status query (idempotent; allowed in any state, also for Unknown).
    pub fn query_status(&mut self, action: ActionId) -> Result<(), DriverError> {
        let id = self.actor.execution().map(|e| e.id).ok_or(DriverError::Actor(ActorError::NoExecution))?;
        let fs = Arc::clone(&self.fs);
        if self.history.record_intent(fs.as_ref(), id, action, IntentKind::QueryStatus, self.wall.now())?.fresh {
            self.actor.query_status()?;
            self.observe()?;
        }
        Ok(())
    }

    pub fn set_capabilities(&mut self, capabilities: Capabilities) {
        self.actor.set_capabilities(capabilities);
    }

    /// Journals and sends one laser-window action (jog, home, move, pause, file operations).
    ///
    /// - A repeated action id is a no-op.
    /// - A request the actor would refuse (unverified feature, job may be active, ...) is
    ///   refused before anything is journaled: it never touches the machine.
    /// - Otherwise the intent is durable first; the outcome is journaled when it is proven
    ///   (reply, refusal, not sent) or stays unknown. Nothing is ever resent.
    pub fn live(&mut self, action: ActionId, request: LiveRequest) -> Result<(), DriverError> {
        if self.history.live_action(action).is_some() {
            return Ok(());
        }
        self.actor.live_preflight(&request)?;
        let fs = Arc::clone(&self.fs);
        let machine = self.actor.machine().id.clone();
        let receipt = self.history.record_live_intent(fs.as_ref(), machine, action, &request, self.wall.now())?;
        if !receipt.fresh {
            return Ok(());
        }
        let sent = self.actor.live(request);
        let attempts = self.actor.live_attempts().len();
        if attempts > self.tracked_attempts {
            let seq = u32::try_from(attempts - 1).unwrap_or(u32::MAX);
            self.live_tracks.push(LiveTrack { seq, action, journaled: false });
            self.tracked_attempts = attempts;
        }
        self.observe_live()?;
        sent.map_err(DriverError::Live)
    }

    /// Releases the held jog key (focus loss, window close). `Ok(false)`: nothing was held and
    /// nothing was journaled.
    pub fn release_jog(&mut self, action: ActionId) -> Result<bool, DriverError> {
        match self.actor.jog_held() {
            Some(key) => self.live(action, LiveRequest::JogStop(key)).map(|()| true),
            None => Ok(false),
        }
    }

    /// A laser-off frame run: like [`MachineDriver::upload`] for a frame job (a compiled frame
    /// plan from `burnburn_machine::FramePlan::artifact`), journaled as a `Frame` intent. The
    /// run belongs to no queue item, so it never counts as a produced piece. Starting it is the
    /// ordinary [`MachineDriver::start`].
    pub fn upload_frame(
        &mut self,
        job: CompiledJobId,
        current_profile: &ProfileRevision,
        action: ActionId,
    ) -> Result<ExecutionId, DriverError> {
        self.upload_as(IntentKind::Frame, job, None, current_profile, action)
    }

    /// Replays the journal into a report. There is no port in this path: it cannot touch a
    /// machine.
    pub fn diagnostic_report(
        &self,
        options: &burnburn_history::DiagnosticOptions,
    ) -> burnburn_history::DiagnosticReport {
        burnburn_history::diagnostic_replay(&self.history, options)
    }

    /// Processes replies/timeouts and journals what was observed.
    pub fn poll(&mut self) -> Result<(), DriverError> {
        self.actor.poll()?;
        self.observe()?;
        self.observe_live()
    }

    /// Journals the outcome of every live attempt that has been proven (or has stayed unknown).
    fn observe_live(&mut self) -> Result<(), DriverError> {
        let fs = Arc::clone(&self.fs);
        let now = self.wall.now();
        for track in self.live_tracks.iter_mut().filter(|t| !t.journaled) {
            let Some(attempt) = self.actor.live_attempts().get(track.seq as usize) else { continue };
            if attempt.outcome == LiveOutcome::Pending {
                continue;
            }
            self.history.record_live_outcome(fs.as_ref(), track.action, &attempt.outcome, now)?;
            track.journaled = true;
        }
        Ok(())
    }

    fn observe(&mut self) -> Result<(), DriverError> {
        let Some(exec) = self.actor.execution() else { return Ok(()) };
        let fs = Arc::clone(&self.fs);
        self.history.observe(fs.as_ref(), exec, self.wall.now())?;
        Ok(())
    }
}
