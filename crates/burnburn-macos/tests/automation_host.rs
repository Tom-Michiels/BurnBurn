//! Desktop automation on the hosts (API01, G5-native): the protocol of `burnburn-runtime` over the
//! presentation controller, headless. A fake filesystem and the headless controller stand in for a
//! host; the listener side runs on a thread and talks to the UI side only through the channel,
//! exactly as in the application. Only `automation_loopback` opens a real socket (127.0.0.1).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod g5_fixtures;

use std::net::UdpSocket;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use burnburn_io::fs::FileSystem;
use burnburn_io::{Project, ProjectUuid, save_project};
use burnburn_macos::automation_host::{
    Automation, AutomationApp, AutomationPump, ChannelHost, Job, channel_pair, status_of,
};
use burnburn_presentation::{AppViewModel, CommandKey, HeadlessEditorHarness, HostEffect, UiIntent};
use burnburn_runtime::automation::{AutomationHost, AutomationService, HostError};
use g5_fixtures::*;

/// A host made of the headless controller; every intent it was given is kept, so a test can say
/// which kinds of intent automation ever sent.
struct Rig {
    h: HeadlessEditorHarness,
    pump: AutomationPump,
    host: ChannelHost,
    sent: Vec<UiIntent>,
    now_ms: u64,
}

struct App<'a> {
    h: &'a mut HeadlessEditorHarness,
    sent: &'a mut Vec<UiIntent>,
}

impl AutomationApp for App<'_> {
    fn view(&self) -> Arc<AppViewModel> {
        self.h.view()
    }

    fn dispatch(&mut self, intent: UiIntent) -> Vec<HostEffect> {
        self.sent.push(intent.clone());
        self.h.send(intent).effects
    }
}

fn rig() -> Rig {
    let h = HeadlessEditorHarness::new();
    let fs = h.shared_fs();
    let exists: Arc<dyn Fn(&Path) -> bool + Send + Sync> = Arc::new(move |p| fs.exists(p).unwrap_or(false));
    let (host, pump) = channel_pair(exists, Arc::new(|| {}));
    Rig { h, pump, host, sent: Vec::new(), now_ms: 0 }
}

impl Rig {
    /// One turn of the UI loop: finished tasks arrive, the pump looks at the world.
    fn turn(&mut self) {
        for update in self.h.run_tasks() {
            self.pump.observe(&update.effects);
        }
        self.now_ms += 10;
        let mut app = App { h: &mut self.h, sent: &mut self.sent };
        self.pump.step(&mut app, self.now_ms);
    }

    /// Sends one datagram through a service on its own (listener) thread while this thread plays
    /// the UI loop, and returns the reply text.
    fn ask(&mut self, request: &str) -> String {
        self.turn(); // the status is published
        let mut service = AutomationService::new(self.host.clone(), true);
        let request = request.to_owned();
        let listener = std::thread::spawn(move || service.handle(request.as_bytes()));
        let started = Instant::now();
        while !listener.is_finished() {
            assert!(started.elapsed() < Duration::from_secs(10), "the listener never got its answer");
            self.turn();
            std::thread::sleep(Duration::from_millis(1));
        }
        let reply = listener.join().unwrap().expect("automation is on");
        self.turn();
        String::from_utf8(reply).unwrap()
    }

    fn put(&self, path: &str, bytes: &[u8]) -> PathBuf {
        let path = PathBuf::from(path);
        let dir = path.parent().unwrap();
        self.h.fs().create_dir_all_for_test(dir);
        let temp = self.h.fs().create_temp_in(dir, ".fixture").unwrap();
        self.h.fs().write_all(&temp, bytes).unwrap();
        self.h.fs().replace(&temp, &path).unwrap();
        path
    }

    fn put_project(&self, path: &str) -> PathBuf {
        let (snapshot, _) = structure_snapshot();
        let path = PathBuf::from(path);
        self.h.fs().create_dir_all_for_test(path.parent().unwrap());
        save_project(self.h.fs(), &path, &Project::new(ProjectUuid::from_bytes([5; 16]), snapshot)).unwrap();
        path
    }

    fn documents(&self) -> usize {
        self.h.view().documents.len()
    }
}

const SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="40mm" height="20mm" viewBox="0 0 40 20">
<rect x="0" y="0" width="40" height="20" fill="none" stroke="black"/></svg>"#;

// ---------------------------------------------------------------------------------------------
// Status
// ---------------------------------------------------------------------------------------------

/// STATUS follows the active document: no window, an untouched window, a changed one.
#[test]
fn status_follows_the_active_document_and_its_unsaved_changes() {
    let mut r = rig();
    assert_eq!(r.ask("PING"), "OK:PONG");
    assert_eq!(r.ask("STATUS"), "OK:READY:NODOC");
    let doc = r.h.new_document().unwrap();
    assert_eq!(r.ask("STATUS"), "OK:READY:CLEAN");
    r.h.invoke(doc, CommandKey::ToolRectangle);
    r.h.drag(doc, (0.0, 0.0), (10.0, 10.0));
    assert_eq!(r.ask("STATUS"), "OK:READY:DIRTY");
    assert_eq!(r.sent, Vec::<UiIntent>::new(), "asking never sends an intent");
}

#[test]
fn the_status_marks_a_running_task_as_busy() {
    let mut r = rig();
    let (mut h, doc, [a, _]) = two_rects();
    let clean = status_of(&r.h.view(), false);
    assert!(!clean.has_document, "no window yet");
    select(&mut h, doc, &[a]);
    assert!(!status_of(&h.view(), false).busy);
    assert!(status_of(&h.view(), true).busy, "a job of its own makes it busy");
    h.send(UiIntent::PreviewOffset { document: doc, distance: "1 mm".into() });
    let running = status_of(&h.view(), false);
    assert!(running.busy && running.has_document, "a worker task is running: {running:?}");
    h.run_tasks();
    assert!(!status_of(&h.view(), false).busy, "and it is over");
    r.turn();
}

// ---------------------------------------------------------------------------------------------
// LOADFILE
// ---------------------------------------------------------------------------------------------

/// A clean application opens the project in a window; the reply comes when the window exists.
#[test]
fn loadfile_opens_the_project_when_nothing_is_unsaved() {
    let mut r = rig();
    let path = r.put_project("/work/a.burnburn");
    assert_eq!(r.documents(), 0);
    assert_eq!(r.ask(&format!("LOADFILE:{}", path.display())), "OK");
    assert_eq!(r.documents(), 1);
    assert_eq!(r.sent, [UiIntent::Open { path }], "one Open intent and nothing else");
    // The opened project is the active document and is clean.
    assert_eq!(r.ask("STATUS"), "OK:READY:CLEAN");
}

/// A changed document is never replaced: ERR:DIRTY, and nothing is opened or sent.
#[test]
fn loadfile_never_replaces_unsaved_work() {
    let mut r = rig();
    let path = r.put_project("/work/a.burnburn");
    let doc = r.h.new_document().unwrap();
    r.h.invoke(doc, CommandKey::ToolRectangle);
    r.h.drag(doc, (0.0, 0.0), (10.0, 10.0));
    let documents = r.documents();
    assert_eq!(r.ask(&format!("LOADFILE:{}", path.display())), "ERR:DIRTY");
    assert_eq!((r.documents(), r.sent.len()), (documents, 0));
}

/// A project that is not there is NOTFOUND; one that cannot be read is FAILED, and no window opens.
#[test]
fn loadfile_names_a_missing_and_an_unreadable_file() {
    let mut r = rig();
    assert_eq!(r.ask("LOADFILE:/work/none.burnburn"), "ERR:NOTFOUND");
    let broken = r.put("/work/broken.burnburn", b"this is not a project");
    assert_eq!(r.ask(&format!("LOADFILE:{}", broken.display())), "ERR:FAILED");
    assert_eq!(r.documents(), 0);
}

// ---------------------------------------------------------------------------------------------
// IMPORT
// ---------------------------------------------------------------------------------------------

/// IMPORT adds the drawing to the active document as one undo step, also when it has changes.
#[test]
fn import_adds_the_drawing_in_one_undo_step() {
    let mut r = rig();
    let svg = r.put("/in/logo.svg", SVG.as_bytes());
    assert_eq!(r.ask(&format!("IMPORT:{}", svg.display())), "ERR:NODOC", "nothing to import into");
    let doc = r.h.new_document().unwrap();
    r.h.invoke(doc, CommandKey::ToolRectangle);
    r.h.drag(doc, (100.0, 100.0), (110.0, 110.0));
    let objects = vm(&r.h, doc).objects.len();
    assert_eq!(r.ask(&format!("IMPORT:{}", svg.display())), "OK");
    let after = vm(&r.h, doc);
    assert_eq!(after.objects.len(), objects + 1, "the drawing arrived");
    assert!(after.import.is_none(), "no preview is left open");
    let undo = after.undo;
    r.h.invoke(doc, CommandKey::Undo);
    assert_eq!(vm(&r.h, doc).objects.len(), objects, "one undo step takes the whole import back");
    assert!(undo.is_some());
    assert!(
        r.sent
            .iter()
            .all(|i| matches!(i, UiIntent::Import { .. } | UiIntent::Invoke { command: CommandKey::CommitImport, .. }))
    );
}

/// A drawing without a unit needs a decision only the user can take: automation cancels the
/// preview and answers FORMAT; the document is untouched.
#[test]
fn import_of_a_unitless_drawing_is_refused_and_leaves_no_preview() {
    let mut r = rig();
    let bytes = burnburn_test_support::fixtures::read_fixture("import", "unitless-lines.dxf").unwrap();
    let dxf = r.put("/in/unitless.dxf", &bytes);
    let doc = r.h.new_document().unwrap();
    assert_eq!(r.ask(&format!("IMPORT:{}", dxf.display())), "ERR:FORMAT");
    let after = vm(&r.h, doc);
    assert!(after.objects.is_empty() && after.import.is_none() && !after.dirty);
    assert!(r.sent.iter().any(|i| matches!(i, UiIntent::Invoke { command: CommandKey::CancelImport, .. })));
}

#[test]
fn import_of_a_file_presentation_cannot_read_fails_cleanly() {
    let mut r = rig();
    let bad = r.put("/in/bad.svg", b"<not svg");
    let doc = r.h.new_document().unwrap();
    assert_eq!(r.ask(&format!("IMPORT:{}", bad.display())), "ERR:FAILED");
    let after = vm(&r.h, doc);
    assert!(after.objects.is_empty() && after.import.is_none());
}

// ---------------------------------------------------------------------------------------------
// Never a machine action
// ---------------------------------------------------------------------------------------------

/// Every machine verb is refused by the protocol, and over a whole session of verbs the pump only
/// ever sent the intents of opening and importing.
#[test]
fn machine_verbs_are_refused_and_automation_only_ever_sends_document_intents() {
    let mut r = rig();
    let doc = r.h.new_document().unwrap();
    for verb in burnburn_runtime::automation::MACHINE_VERBS {
        assert_eq!(r.ask(&format!("{verb}:/work/x.burnburn")), "ERR:FORBIDDEN", "{verb}");
        assert_eq!(r.ask(verb), "ERR:FORBIDDEN", "{verb}");
    }
    let svg = r.put("/in/logo.svg", SVG.as_bytes());
    assert_eq!(r.ask(&format!("IMPORT:{}", svg.display())), "OK");
    let path = r.put_project("/work/a.burnburn");
    let _ = doc;
    // The import left the document changed: LOADFILE is refused.
    assert_eq!(r.ask(&format!("LOADFILE:{}", path.display())), "ERR:DIRTY");
    for intent in &r.sent {
        assert!(
            matches!(
                intent,
                UiIntent::Open { .. }
                    | UiIntent::Import { .. }
                    | UiIntent::Invoke { command: CommandKey::CommitImport | CommandKey::CancelImport, .. }
            ),
            "{intent:?}"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// A listener that gives up
// ---------------------------------------------------------------------------------------------

/// When the listener stops waiting, the UI thread drops the job and is free for the next one.
#[test]
fn a_job_the_listener_gave_up_on_is_dropped_and_the_pump_is_free_again() {
    let mut r = rig();
    let path = r.put_project("/work/a.burnburn");
    r.turn();
    let mut host = r.host.clone().with_timeout(Duration::from_millis(30));
    // Nobody steps the pump during these 30 ms: the listener answers FAILED itself.
    assert_eq!(host.load(&path), Err(HostError::Failed));
    r.turn();
    assert_eq!(r.documents(), 0, "an abandoned job is not started");
    assert!(!r.pump.busy());
    // The next request works.
    assert_eq!(r.ask(&format!("LOADFILE:{}", path.display())), "OK");
    assert_eq!(r.documents(), 1);
}

#[test]
fn a_job_that_never_ends_times_out_on_the_ui_side_too() {
    let mut r = rig();
    let path = r.put_project("/work/a.burnburn");
    let pending = r.host.submit(Job::Load(path)).unwrap();
    // The task never runs: the clock moves on without `run_tasks` helping.
    let mut app = App { h: &mut r.h, sent: &mut r.sent };
    r.pump.step(&mut app, 0);
    assert!(r.pump.busy(), "the job started");
    r.pump.step(&mut app, 60_000);
    assert!(!r.pump.busy(), "after the time limit the job is over");
    assert_eq!(pending.try_answer(), Some(Err(HostError::Failed)));
}

// ---------------------------------------------------------------------------------------------
// The listener thread follows the preference
// ---------------------------------------------------------------------------------------------

/// With the preference off nothing listens (the port is free); on, the real loopback socket answers
/// PING; off again, the thread is gone and the port is free again. Loopback only.
#[test]
fn the_listener_runs_only_while_the_preference_is_on_and_only_on_loopback() {
    let h = HeadlessEditorHarness::new();
    let fs = h.shared_fs();
    let mut automation = Automation::with(Arc::new(move |p: &Path| fs.exists(p).unwrap_or(false)), Arc::new(|| {}), 0);
    assert!(!automation.running(), "off by default: no thread, no socket");
    automation.sync_preference(false).unwrap();
    assert!(!automation.running());
    assert!(automation.local_addr().is_none());

    automation.sync_preference(true).unwrap();
    assert!(automation.running());
    let addr = automation.local_addr().expect("a bound address");
    assert!(addr.ip().is_loopback(), "{addr}");
    let client = UdpSocket::bind("127.0.0.1:0").unwrap();
    client.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    client.send_to(b"PING", addr).unwrap();
    let mut buf = [0u8; 64];
    let (n, from) = client.recv_from(&mut buf).expect("a reply on loopback");
    assert_eq!((&buf[..n], from), (&b"OK:PONG"[..], addr));

    // Turning it off stops the thread and frees the port; nothing answers any more.
    automation.sync_preference(false).unwrap();
    assert!(!automation.running());
    UdpSocket::bind(addr).expect("the port is free again");
    client.set_read_timeout(Some(Duration::from_millis(300))).unwrap();
    client.send_to(b"PING", addr).unwrap();
    assert!(client.recv_from(&mut buf).is_err(), "no answer when automation is off");

    // And back on, then the application quits.
    automation.sync_preference(true).unwrap();
    assert!(automation.running());
    automation.shutdown();
    assert!(!automation.running(), "shutdown stops the listener");
}

/// A port that is taken leaves automation off and says why; it never listens elsewhere.
#[test]
fn a_port_in_use_leaves_automation_off_with_an_error() {
    let taken = UdpSocket::bind("127.0.0.1:0").unwrap();
    let port = taken.local_addr().unwrap().port();
    let mut automation = Automation::with(Arc::new(|_: &Path| false), Arc::new(|| {}), port);
    assert!(automation.sync_preference(true).is_err());
    assert!(!automation.running());
}
