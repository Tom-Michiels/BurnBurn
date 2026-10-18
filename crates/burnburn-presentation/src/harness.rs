//! `HeadlessEditorHarness`: drives the real [`AppController`] with the same intents a native host
//! sends. It contains no editor, selection or command logic of its own — it only turns
//! "user gestures" into `UiIntent`s (positions in world mm → canvas points through the window's
//! own camera) and runs queued tasks synchronously with the production [`run_task`].

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use burnburn_editor::{CanvasKey, Modifiers, PointerButton, PointerPhase, SceneSnapshot};
use burnburn_geometry::Point;
use burnburn_io::ProjectUuid;
use burnburn_io::fs::FileSystem;
use burnburn_io::memory::MemoryFileSystem;
use burnburn_types::{DocumentId, MonotonicTime};

use crate::controller::{AppController, AppPorts, AppUpdate, HostEffect};
use crate::intent::{CommandKey, UiIntent, ViewPointerEvent};
use crate::task::{TaskOutcome, TaskPort, TaskPortError, TaskRequest, run_task};
use crate::view_model::{AppViewModel, DocumentViewModel, MachineButton, MachineViewModel, OperatorViewModel};

/// Deterministic task port: queues requests until the test runs or reorders them.
#[derive(Default)]
pub struct QueuedTaskPort {
    queue: Mutex<VecDeque<TaskRequest>>,
}

impl QueuedTaskPort {
    fn lock(&self) -> MutexGuard<'_, VecDeque<TaskRequest>> {
        self.queue.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn take_all(&self) -> Vec<TaskRequest> {
        self.lock().drain(..).collect()
    }

    pub fn len(&self) -> usize {
        self.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl TaskPort for QueuedTaskPort {
    fn submit(&self, request: TaskRequest) -> Result<(), TaskPortError> {
        self.lock().push_back(request);
        Ok(())
    }
}

pub struct HeadlessEditorHarness {
    controller: AppController,
    tasks: Arc<QueuedTaskPort>,
    fs: Arc<MemoryFileSystem>,
    clock_ms: u64,
}

impl Default for HeadlessEditorHarness {
    fn default() -> Self {
        HeadlessEditorHarness::new()
    }
}

impl HeadlessEditorHarness {
    pub fn new() -> Self {
        HeadlessEditorHarness::with_fs(Arc::new(MemoryFileSystem::new()))
    }

    /// A harness over an existing in-memory disk (a second app start in a scenario).
    pub fn with_fs(fs: Arc<MemoryFileSystem>) -> Self {
        let tasks = Arc::new(QueuedTaskPort::default());
        let port: Arc<dyn TaskPort> = Arc::clone(&tasks) as Arc<dyn TaskPort>;
        let counter = Arc::new(AtomicU8::new(0));
        let new_project_uuid = Arc::new(move || ProjectUuid::from_bytes([counter.fetch_add(1, Ordering::Relaxed); 16]));
        let ports = AppPorts { tasks: port, fs: Arc::clone(&fs) as Arc<dyn FileSystem>, new_project_uuid };
        HeadlessEditorHarness { controller: AppController::new(ports), tasks, fs, clock_ms: 0 }
    }

    /// The in-memory filesystem, for fault injection and inspecting what was written.
    pub fn fs(&self) -> &MemoryFileSystem {
        &self.fs
    }

    /// The same in-memory disk, shareable with a second harness.
    pub fn shared_fs(&self) -> Arc<MemoryFileSystem> {
        Arc::clone(&self.fs)
    }

    /// Content of every text object of `document`, in id order.
    pub fn text_contents(&self, document: DocumentId) -> Vec<String> {
        self.controller.text_contents(document)
    }

    pub fn controller(&mut self) -> &mut AppController {
        &mut self.controller
    }

    pub fn send(&mut self, intent: UiIntent) -> AppUpdate {
        self.controller.handle(intent)
    }

    /// Cmd+N / Ctrl+N. Returns the new window's document.
    pub fn new_document(&mut self) -> Option<DocumentId> {
        let update = self.send(UiIntent::NewDocument);
        update.effects.iter().find_map(|e| match e {
            HostEffect::OpenWindow { document } => Some(*document),
            _ => None,
        })
    }

    pub fn invoke(&mut self, document: DocumentId, command: CommandKey) -> AppUpdate {
        self.send(UiIntent::Invoke { document: Some(document), command })
    }

    pub fn view(&self) -> Arc<AppViewModel> {
        self.controller.view()
    }

    pub fn document(&self, document: DocumentId) -> Option<DocumentViewModel> {
        self.view().document(document).cloned()
    }

    /// The machine panel (C2–C5).
    pub fn machine_view(&self) -> Option<MachineViewModel> {
        self.controller.machine_view()
    }

    /// The compact operator view (IM31).
    pub fn operator_view(&self) -> OperatorViewModel {
        self.controller.operator_view()
    }

    /// A click on a machine control: sends exactly what the button carries.
    pub fn press(&mut self, document: Option<DocumentId>, button: &MachineButton) -> AppUpdate {
        self.send(UiIntent::Machine { document, action: button.action, seen: button.seen.clone() })
    }

    /// The window's scene (what the host would draw).
    pub fn scene(&self, document: DocumentId) -> Option<Arc<SceneSnapshot>> {
        self.controller.scene(document)
    }

    /// A primary-button pointer event at a world position. The harness maps mm to canvas points
    /// with the window's own camera, exactly like a host's view would see the pointer.
    pub fn pointer(
        &mut self,
        document: DocumentId,
        phase: PointerPhase,
        at_mm: (f64, f64),
        modifiers: Modifiers,
    ) -> AppUpdate {
        self.pointer_event(document, phase, at_mm, modifiers, Some(PointerButton::Primary), 1)
    }

    fn pointer_event(
        &mut self,
        document: DocumentId,
        phase: PointerPhase,
        at_mm: (f64, f64),
        modifiers: Modifiers,
        button: Option<PointerButton>,
        click_count: u8,
    ) -> AppUpdate {
        let Some(vm) = self.document(document) else { return AppUpdate::default() };
        let pt = vm.camera.world_to_points(&vm.viewport, Point::new(at_mm.0, at_mm.1));
        self.clock_ms += 8;
        let mut event =
            ViewPointerEvent::mouse(phase, pt, MonotonicTime::from_offset(Duration::from_millis(self.clock_ms)));
        event.modifiers = modifiers;
        event.button = button;
        event.click_count = click_count;
        self.send(UiIntent::Pointer { document, event })
    }

    /// Pointer movement without a button (hover: snap candidates, rubber segments).
    pub fn hover(&mut self, document: DocumentId, at_mm: (f64, f64)) -> AppUpdate {
        self.pointer_event(document, PointerPhase::Move, at_mm, Modifiers::default(), None, 1)
    }

    /// Press and release at one place.
    pub fn click(&mut self, document: DocumentId, at_mm: (f64, f64), modifiers: Modifiers) -> AppUpdate {
        self.pointer(document, PointerPhase::Down, at_mm, modifiers);
        self.pointer(document, PointerPhase::Up, at_mm, modifiers)
    }

    /// The second press of a double click (after a first `click`).
    pub fn double_click(&mut self, document: DocumentId, at_mm: (f64, f64)) -> AppUpdate {
        let p = Some(PointerButton::Primary);
        self.pointer_event(document, PointerPhase::Down, at_mm, Modifiers::default(), p, 2);
        self.pointer_event(document, PointerPhase::Up, at_mm, Modifiers::default(), p, 2)
    }

    /// Press at `from`, move halfway and to `to`, but do not release.
    pub fn begin_drag(&mut self, document: DocumentId, from: (f64, f64), to: (f64, f64)) {
        self.begin_drag_with(document, from, to, Modifiers::default());
    }

    pub fn begin_drag_with(&mut self, document: DocumentId, from: (f64, f64), to: (f64, f64), modifiers: Modifiers) {
        self.pointer(document, PointerPhase::Down, from, modifiers);
        self.pointer(document, PointerPhase::Move, ((from.0 + to.0) / 2.0, (from.1 + to.1) / 2.0), modifiers);
        self.pointer(document, PointerPhase::Move, to, modifiers);
    }

    /// A complete drag: Down, Moves, Up.
    pub fn drag(&mut self, document: DocumentId, from: (f64, f64), to: (f64, f64)) -> AppUpdate {
        self.drag_with(document, from, to, Modifiers::default())
    }

    pub fn drag_with(
        &mut self,
        document: DocumentId,
        from: (f64, f64),
        to: (f64, f64),
        modifiers: Modifiers,
    ) -> AppUpdate {
        self.begin_drag_with(document, from, to, modifiers);
        self.pointer(document, PointerPhase::Up, to, modifiers)
    }

    pub fn key(&mut self, document: DocumentId, key: CanvasKey) -> AppUpdate {
        self.send(UiIntent::Key { document, key, modifiers: Modifiers::default() })
    }

    /// Esc on the canvas.
    pub fn escape(&mut self, document: DocumentId) -> AppUpdate {
        self.key(document, CanvasKey::Escape)
    }

    /// Requests submitted so far and not yet run (for tests that control delivery order).
    pub fn take_task_requests(&mut self) -> Vec<TaskRequest> {
        self.tasks.take_all()
    }

    /// Runs one request with the production task function and delivers its outcome.
    pub fn complete(&mut self, request: TaskRequest) -> AppUpdate {
        let result = run_task(request.input, &request.cancel);
        self.controller.task_finished(TaskOutcome {
            id: request.id,
            document: request.document,
            source: request.source,
            result,
        })
    }

    /// Like [`Self::complete`], but the worker finished before it saw a cancellation: a race the
    /// controller must survive by revision checking alone (cancellation is best effort).
    pub fn complete_despite_cancel(&mut self, request: TaskRequest) -> AppUpdate {
        let result = run_task(request.input, &burnburn_types::CancellationToken::never());
        self.controller.task_finished(TaskOutcome {
            id: request.id,
            document: request.document,
            source: request.source,
            result,
        })
    }

    /// Runs and delivers every queued request in submission order.
    /// Runs queued tasks until none is left, including follow-up tasks a result submits
    /// (for example removing the recovery file after a save); bounded.
    pub fn run_tasks(&mut self) -> Vec<AppUpdate> {
        let mut updates = Vec::new();
        for _ in 0..16 {
            let requests = self.take_task_requests();
            if requests.is_empty() {
                break;
            }
            updates.extend(requests.into_iter().map(|r| self.complete(r)));
        }
        updates
    }
}
