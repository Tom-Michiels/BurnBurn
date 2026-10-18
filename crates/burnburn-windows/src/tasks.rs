//! Worker threads for presentation tasks (save, open, measure) and the hand-back to the UI thread.
//!
//! Platform-independent: the host passes a `wake` function that schedules
//! [`TaskInbox::drain`] on its UI thread (Windows: `PostMessageW` to the main window). Until
//! `burnburn-runtime` exists this is the host's own minimal pool: one thread per task.

use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

use burnburn_presentation::{TaskOutcome, TaskPort, TaskPortError, TaskRequest, run_task};

/// Runs each request on its own thread with the production [`run_task`].
pub struct ThreadTaskPort {
    tx: Mutex<Sender<TaskOutcome>>,
    wake: Arc<dyn Fn() + Send + Sync>,
}

/// UI-thread side: finished outcomes, in completion order.
pub struct TaskInbox {
    rx: Receiver<TaskOutcome>,
}

/// A port and its inbox. `wake` runs on the worker thread after each outcome is queued.
pub fn task_channel(wake: Arc<dyn Fn() + Send + Sync>) -> (ThreadTaskPort, TaskInbox) {
    let (tx, rx) = channel();
    (ThreadTaskPort { tx: Mutex::new(tx), wake }, TaskInbox { rx })
}

impl TaskPort for ThreadTaskPort {
    fn submit(&self, request: TaskRequest) -> Result<(), TaskPortError> {
        let tx = self.tx.lock().map_err(|_| TaskPortError::Unavailable)?.clone();
        let wake = Arc::clone(&self.wake);
        std::thread::Builder::new()
            .name("burnburn-task".into())
            .spawn(move || {
                let TaskRequest { id, document, source, cancel, input, .. } = request;
                let result = run_task(input, &cancel);
                if tx.send(TaskOutcome { id, document, source, result }).is_ok() {
                    wake();
                }
            })
            .map(|_| ())
            .map_err(|_| TaskPortError::Unavailable)
    }
}

impl TaskInbox {
    /// Every outcome that has arrived, without blocking.
    pub fn drain(&self) -> Vec<TaskOutcome> {
        self.rx.try_iter().collect()
    }
}
