//! The host's worker port: tasks run off the calling thread with the production `run_task`, the
//! wake-up fires after the outcome is queued, and the inbox hands outcomes over without blocking.

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers outside #[test] fns

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Duration;

use burnburn_document::DocumentSnapshot;
use burnburn_io::fs::FileSystem;
use burnburn_io::memory::MemoryFileSystem;
use burnburn_io::{Project, ProjectUuid};
use burnburn_macos::tasks::task_channel;
use burnburn_presentation::{TaskInput, TaskOutput, TaskPort, TaskRequest};
use burnburn_types::{CancellationSource, ContentHash, DocumentId, RevisionId, StableId, TaskId};

#[test]
fn worker_saves_off_thread_wakes_once_and_inbox_hands_over_the_outcome() {
    let fs = Arc::new(MemoryFileSystem::new());
    let wakes = Arc::new(AtomicUsize::new(0));
    let (woke_tx, woke_rx) = mpsc::channel::<std::thread::ThreadId>();
    let counter = Arc::clone(&wakes);
    let (port, inbox) = task_channel(Arc::new(move || {
        counter.fetch_add(1, Ordering::SeqCst);
        let _ = woke_tx.send(std::thread::current().id());
    }));
    assert!(inbox.drain().is_empty(), "nothing before a task");
    let path = PathBuf::from("plaat.burnburn");
    let project = Project::new(ProjectUuid::from_bytes([7; 16]), DocumentSnapshot::from_parts(Vec::new(), 1).unwrap());
    let cancel = CancellationSource::new();
    port.submit(TaskRequest {
        id: TaskId::from_raw(1),
        document: DocumentId::from_raw(2),
        source: RevisionId::INITIAL,
        key: ContentHash::from_bytes([0; 32]),
        cancel: cancel.token(),
        input: TaskInput::Save {
            project: Box::new(project),
            path: path.clone(),
            fs: Arc::clone(&fs) as Arc<dyn FileSystem>,
        },
    })
    .unwrap();
    let worker = woke_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_ne!(worker, std::thread::current().id(), "the task ran on a worker thread");
    let outcomes = inbox.drain();
    assert_eq!(outcomes.len(), 1);
    assert_eq!((outcomes[0].id, outcomes[0].document), (TaskId::from_raw(1), DocumentId::from_raw(2)));
    assert!(matches!(&outcomes[0].result, Ok(TaskOutput::Saved { path: p }) if *p == path));
    assert!(fs.exists(&path).unwrap(), "written through the injected filesystem");
    assert_eq!(wakes.load(Ordering::SeqCst), 1);
    assert!(inbox.drain().is_empty(), "drained once");
}
