//! Desktop automation on the hosts (API01, G5-native): the protocol of `burnburn_runtime::automation`
//! connected to the presentation controller. Shared by the macOS and Windows hosts (the Windows
//! crate compiles this file); it has no AppKit and no Win32.
//!
//! Three parts:
//!
//! - [`ChannelHost`] is the [`AutomationHost`] of the listener thread. `status` reads the last
//!   status the UI thread published; `load` and `import` hand a [`Job`] to the UI thread and wait
//!   for its answer, with a time limit. It has no machine method because the trait has none.
//! - [`AutomationPump`] lives on the UI thread. The host calls [`AutomationPump::step`] after every
//!   update and every wake-up; it publishes the status, starts one job at a time through plain
//!   intents (`Open`, `Import`, `CommitImport`) and watches the view model and the host effects to
//!   see how the job ended. Nothing here sends a machine, job or upload intent.
//! - [`AutomationRuntime`] starts the listener thread when the preference "Desktop-automatisering"
//!   is on and stops it when it is turned off or the application quits. The socket is bound to
//!   127.0.0.1 only.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use burnburn_presentation::{AppViewModel, CommandKey, HostEffect, UiIntent};
use burnburn_runtime::automation::{
    AutomationEndpoint, AutomationHost, AutomationService, EndpointError, HostError, HostStatus, bind_loopback,
};
use burnburn_types::DocumentId;

/// How long the listener waits for the UI thread to finish a job.
pub const JOB_TIMEOUT: Duration = Duration::from_secs(20);
/// How long the listener thread blocks on its socket before it looks at the stop flag.
pub const POLL: Duration = Duration::from_millis(100);

/// What an automation request asks the UI thread to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Job {
    /// Open the project at the path (in a window of its own).
    Load(PathBuf),
    /// Import the drawing file into the active document.
    Import(PathBuf),
}

type Reply = Sender<Result<(), HostError>>;

struct Request {
    job: Job,
    reply: Reply,
    /// The listener gave up waiting: the UI thread drops the job.
    abandoned: Arc<AtomicBool>,
}

/// The [`AutomationHost`] of the listener thread.
#[derive(Clone)]
pub struct ChannelHost {
    status: Arc<Mutex<HostStatus>>,
    requests: Sender<Request>,
    exists: Arc<dyn Fn(&Path) -> bool + Send + Sync>,
    wake: Arc<dyn Fn() + Send + Sync>,
    timeout: Duration,
}

/// A request in flight: the answer arrives when the UI thread has finished the job.
pub struct Pending {
    answer: Receiver<Result<(), HostError>>,
    abandoned: Arc<AtomicBool>,
}

impl Pending {
    /// The answer if it is there already.
    pub fn try_answer(&self) -> Option<Result<(), HostError>> {
        self.answer.try_recv().ok()
    }

    fn wait(self, timeout: Duration, wake: &(dyn Fn() + Send + Sync)) -> Result<(), HostError> {
        match self.answer.recv_timeout(timeout) {
            Ok(result) => result,
            Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => {
                self.abandoned.store(true, Ordering::SeqCst);
                wake();
                Err(HostError::Failed)
            }
        }
    }
}

impl ChannelHost {
    /// Hands `job` to the UI thread without waiting for the end. A file that is not there is
    /// refused here, so the UI thread never sees it.
    pub fn submit(&self, job: Job) -> Result<Pending, HostError> {
        let (Job::Load(path) | Job::Import(path)) = &job;
        if !(self.exists)(path) {
            return Err(HostError::NotFound);
        }
        let (reply, answer) = channel();
        let abandoned = Arc::new(AtomicBool::new(false));
        self.requests
            .send(Request { job, reply, abandoned: Arc::clone(&abandoned) })
            .map_err(|_| HostError::Failed)?;
        (self.wake)();
        Ok(Pending { answer, abandoned })
    }
}

impl AutomationHost for ChannelHost {
    fn status(&self) -> HostStatus {
        // A poisoned lock is a crashed UI thread: report busy so nothing is started.
        self.status.lock().map_or(HostStatus { has_document: false, dirty: false, busy: true }, |s| *s)
    }

    fn load(&mut self, path: &Path) -> Result<(), HostError> {
        self.submit(Job::Load(path.to_path_buf()))?.wait(self.timeout, &*self.wake)
    }

    fn import(&mut self, path: &Path) -> Result<(), HostError> {
        self.submit(Job::Import(path.to_path_buf()))?.wait(self.timeout, &*self.wake)
    }
}

/// What the UI thread offers the pump: the view model and the one way in, an intent.
pub trait AutomationApp {
    fn view(&self) -> Arc<AppViewModel>;
    /// Sends the intent as the host's own dispatcher does (effects applied) and returns the effects.
    fn dispatch(&mut self, intent: UiIntent) -> Vec<HostEffect>;
}

/// The job being carried out.
struct Active {
    request: Request,
    stage: Stage,
    started_ms: u64,
}

enum Stage {
    /// `Open` sent; waiting for a window (or an error).
    Opening,
    /// `Import` sent; waiting for the preview, which is then committed or cancelled at once.
    Previewing { document: DocumentId },
}

/// The UI-thread side. One job at a time.
pub struct AutomationPump {
    requests: Receiver<Request>,
    status: Arc<Mutex<HostStatus>>,
    active: Option<Active>,
    /// Effects seen since the job started (open windows, errors).
    seen: Vec<HostEffect>,
    timeout_ms: u64,
}

/// A host's [`ChannelHost`] and its [`AutomationPump`], connected.
pub fn channel_pair(
    exists: Arc<dyn Fn(&Path) -> bool + Send + Sync>,
    wake: Arc<dyn Fn() + Send + Sync>,
) -> (ChannelHost, AutomationPump) {
    let (requests_tx, requests_rx) = channel();
    let status = Arc::new(Mutex::new(HostStatus { has_document: false, dirty: false, busy: false }));
    let host = ChannelHost { status: Arc::clone(&status), requests: requests_tx, exists, wake, timeout: JOB_TIMEOUT };
    let pump = AutomationPump {
        requests: requests_rx,
        status,
        active: None,
        seen: Vec::new(),
        timeout_ms: u64::try_from(JOB_TIMEOUT.as_millis()).unwrap_or(u64::MAX),
    };
    (host, pump)
}

impl ChannelHost {
    /// As [`channel_pair`], with another time limit for the listener (tests).
    pub fn with_timeout(mut self, timeout: Duration) -> ChannelHost {
        self.timeout = timeout;
        self
    }
}

impl AutomationPump {
    /// Records the effects of an update (the host calls this for every update it applies): while a
    /// job runs, an opened window or an error tells how it went.
    pub fn observe(&mut self, effects: &[HostEffect]) {
        if self.active.is_some() {
            self.seen.extend(effects.iter().filter(|e| matches!(e, HostEffect::OpenWindow { .. } | HostEffect::ShowError { .. })).cloned());
        }
    }

    /// A job is being carried out.
    pub fn busy(&self) -> bool {
        self.active.is_some()
    }

    fn publish(&self, app: &dyn AutomationApp) {
        self.publish_status(&app.view());
    }

    /// Makes `vm` the status the listener answers STATUS with (the host calls this after every
    /// refresh, so a reply is never older than the last update).
    pub fn publish_status(&self, vm: &AppViewModel) {
        if let Ok(mut status) = self.status.lock() {
            *status = status_of(vm, self.active.is_some());
        }
    }

    fn finish(&mut self, result: Result<(), HostError>) {
        if let Some(active) = self.active.take() {
            // The listener may have gone: an answer nobody waits for is dropped.
            let _ = active.request.reply.send(result);
        }
        self.seen.clear();
    }

    /// Closes an import preview that is still open for `document` (a cancelled or failed job).
    fn cancel_preview(app: &mut dyn AutomationApp, document: DocumentId) -> Vec<HostEffect> {
        let open = app.view().document(document).is_some_and(|d| d.import.is_some());
        if open {
            app.dispatch(UiIntent::Invoke { document: Some(document), command: CommandKey::CancelImport })
        } else {
            Vec::new()
        }
    }

    fn start(&mut self, app: &mut dyn AutomationApp, request: Request, now_ms: u64) {
        let vm = app.view();
        self.seen.clear();
        match &request.job {
            Job::Load(path) => {
                // The service refused a changed document before; a change since then is a refusal too.
                if status_of(&vm, false).dirty {
                    let _ = request.reply.send(Err(HostError::Failed));
                    return;
                }
                let path = path.clone();
                self.active = Some(Active { request, stage: Stage::Opening, started_ms: now_ms });
                let effects = app.dispatch(UiIntent::Open { path });
                self.observe(&effects);
            }
            Job::Import(path) => {
                let Some(document) = vm.active.filter(|d| vm.document(*d).is_some()) else {
                    let _ = request.reply.send(Err(HostError::Failed));
                    return;
                };
                let path = path.clone();
                self.active = Some(Active { request, stage: Stage::Previewing { document }, started_ms: now_ms });
                let effects = app.dispatch(UiIntent::Import { document, path });
                self.observe(&effects);
            }
        }
    }

    fn advance(&mut self, app: &mut dyn AutomationApp, now_ms: u64) {
        let Some(active) = &self.active else { return };
        let stage = match &active.stage {
            Stage::Opening => Stage::Opening,
            Stage::Previewing { document } => Stage::Previewing { document: *document },
        };
        let (abandoned, expired) =
            (active.request.abandoned.load(Ordering::SeqCst), now_ms.saturating_sub(active.started_ms) > self.timeout_ms);
        let errored = self.seen.iter().any(|e| matches!(e, HostEffect::ShowError { .. }));
        match stage {
            Stage::Opening => {
                if self.seen.iter().any(|e| matches!(e, HostEffect::OpenWindow { .. })) {
                    self.finish(Ok(()));
                } else if abandoned || expired || errored {
                    self.finish(Err(HostError::Failed));
                }
            }
            Stage::Previewing { document } => {
                let vm = app.view();
                let Some(doc) = vm.document(document) else {
                    return self.finish(Err(HostError::Failed));
                };
                if abandoned || expired || errored {
                    let effects = Self::cancel_preview(app, document);
                    self.observe(&effects);
                    return self.finish(Err(HostError::Failed));
                }
                let Some(import) = &doc.import else { return };
                if import.busy {
                    return;
                }
                if import.needs_unit {
                    // Only the user can say which unit a drawing without one is in.
                    Self::cancel_preview(app, document);
                    return self.finish(Err(HostError::Unsupported));
                }
                let effects =
                    app.dispatch(UiIntent::Invoke { document: Some(document), command: CommandKey::CommitImport });
                self.observe(&effects);
                let committed = app.view().document(document).is_some_and(|d| d.import.is_none());
                if committed {
                    self.finish(Ok(()));
                } else {
                    Self::cancel_preview(app, document);
                    self.finish(Err(HostError::Failed));
                }
            }
        }
    }

    /// Publishes the status and moves the current job on (or starts the next one). `now_ms` is a
    /// monotonic clock in milliseconds.
    pub fn step(&mut self, app: &mut dyn AutomationApp, now_ms: u64) {
        self.publish(app);
        if self.active.is_some() {
            self.advance(app, now_ms);
        }
        // Requests the listener gave up on never start.
        while self.active.is_none() {
            let Ok(request) = self.requests.try_recv() else { break };
            if request.abandoned.load(Ordering::SeqCst) {
                continue;
            }
            self.start(app, request, now_ms);
            self.advance(app, now_ms);
        }
        self.publish(app);
    }
}

/// Why the listener could not start.
#[derive(Debug)]
pub enum RuntimeError {
    Bind(std::io::Error),
    Endpoint(EndpointError),
}

impl std::fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RuntimeError::Bind(e) => write!(f, "cannot bind the automation socket: {e}"),
            RuntimeError::Endpoint(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for RuntimeError {}

/// The listener thread, running only while the preference is on.
pub struct AutomationRuntime {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    addr: Option<SocketAddr>,
}

impl AutomationRuntime {
    pub fn new() -> AutomationRuntime {
        AutomationRuntime { stop: Arc::new(AtomicBool::new(false)), thread: None, addr: None }
    }

    /// Whether the listener thread runs.
    pub fn running(&self) -> bool {
        self.thread.is_some()
    }

    /// Where it listens (always loopback).
    pub fn local_addr(&self) -> Option<SocketAddr> {
        self.addr
    }

    /// Starts the listener on loopback `port` (`0`: any free port) with `host`. Does nothing when it
    /// already runs.
    pub fn start(&mut self, port: u16, host: ChannelHost) -> Result<(), RuntimeError> {
        if self.thread.is_some() {
            return Ok(());
        }
        let socket = bind_loopback(port).map_err(RuntimeError::Bind)?;
        socket.set_read_timeout(Some(POLL)).map_err(RuntimeError::Bind)?;
        let addr = socket.local_addr().map_err(RuntimeError::Bind)?;
        let mut endpoint =
            AutomationEndpoint::new(socket, AutomationService::new(host, true)).map_err(RuntimeError::Endpoint)?;
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let thread = std::thread::Builder::new()
            .name("burnburn-automation".into())
            .spawn(move || {
                while !flag.load(Ordering::SeqCst) {
                    match endpoint.serve_one() {
                        Ok(_) => {}
                        // The read timeout: look at the stop flag again.
                        Err(EndpointError::Io(e))
                            if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {}
                        // Anything else (a reset from an unreachable sender): pause, never spin.
                        Err(_) => std::thread::sleep(POLL),
                    }
                }
            })
            .map_err(RuntimeError::Bind)?;
        self.stop = stop;
        self.thread = Some(thread);
        self.addr = Some(addr);
        Ok(())
    }

    /// Stops the listener and waits for its thread; the socket is closed when this returns.
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            // A panicked listener has nothing left to clean up.
            let _ = thread.join();
        }
        self.addr = None;
    }
}

impl Default for AutomationRuntime {
    fn default() -> Self {
        AutomationRuntime::new()
    }
}

impl Drop for AutomationRuntime {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Everything a host keeps: the pump, the runtime and the channel to start the listener with.
pub struct Automation {
    pump: AutomationPump,
    runtime: AutomationRuntime,
    host: ChannelHost,
    port: u16,
    /// Binding failed for the current "on": not tried again until the preference is turned off.
    start_failed: bool,
}

impl Automation {
    /// `wake` runs on the listener thread when a request is waiting: it must make the UI thread
    /// call [`Automation::step`] (the host's own task wake-up does).
    pub fn new(fs: Arc<dyn burnburn_io::fs::FileSystem>, wake: Arc<dyn Fn() + Send + Sync>) -> Automation {
        Automation::with(
            Arc::new(move |p: &Path| fs.exists(p).unwrap_or(false)),
            wake,
            burnburn_runtime::automation::DEFAULT_PORT,
        )
    }

    /// As [`Automation::new`], with the file check and the port chosen (tests).
    pub fn with(
        exists: Arc<dyn Fn(&Path) -> bool + Send + Sync>,
        wake: Arc<dyn Fn() + Send + Sync>,
        port: u16,
    ) -> Automation {
        let (host, pump) = channel_pair(exists, wake);
        Automation { pump, runtime: AutomationRuntime::new(), host, port, start_failed: false }
    }

    /// Follows the preference "Desktop-automatisering": the listener runs exactly while it is on.
    /// A socket that cannot be bound leaves automation off; the error is returned for the log.
    pub fn sync_preference(&mut self, enabled: bool) -> Result<(), RuntimeError> {
        if !enabled {
            self.runtime.stop();
            self.start_failed = false;
            return Ok(());
        }
        // A port that failed once is not tried again at every refresh, only after being turned off.
        if self.runtime.running() || self.start_failed {
            return Ok(());
        }
        self.runtime.start(self.port, self.host.clone()).inspect_err(|_| self.start_failed = true)
    }

    /// Another port for the next start (tests; `0` picks a free one).
    pub fn set_port(&mut self, port: u16) {
        self.port = port;
    }

    pub fn observe(&mut self, effects: &[HostEffect]) {
        self.pump.observe(effects);
    }

    pub fn publish_status(&self, vm: &AppViewModel) {
        self.pump.publish_status(vm);
    }

    pub fn step(&mut self, app: &mut dyn AutomationApp, now_ms: u64) {
        self.pump.step(app, now_ms);
    }

    pub fn running(&self) -> bool {
        self.runtime.running()
    }

    pub fn local_addr(&self) -> Option<SocketAddr> {
        self.runtime.local_addr()
    }

    /// Ends the listener (application quit).
    pub fn shutdown(&mut self) {
        self.runtime.stop();
    }
}

/// The status the pump publishes for `vm`: the active document, whether it has unsaved changes and
/// whether a job or a task is running.
pub fn status_of(vm: &AppViewModel, job_active: bool) -> HostStatus {
    let doc = vm.active.and_then(|d| vm.document(d));
    HostStatus {
        has_document: doc.is_some(),
        dirty: doc.is_some_and(|d| d.dirty),
        busy: job_active || doc.is_some_and(|d| !d.tasks.is_empty()),
    }
}
