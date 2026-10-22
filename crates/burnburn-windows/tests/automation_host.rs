//! Desktop automation on the Windows host (API01, G5-native), headless. The module is the macOS
//! host's `automation_host.rs`, compiled into this crate; its full test set is
//! `burnburn-macos/tests/automation_host.rs`. Here: the Windows build of it answers STATUS and
//! LOADFILE through the headless controller, refuses a changed document and a machine verb, and
//! its listener follows the preference. The Win32 wiring (`win32/mod.rs`) is only type-checked.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_test_support::host::g5 as g5_fixtures;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use burnburn_io::fs::FileSystem;
use burnburn_io::{Project, ProjectUuid, save_project};
use burnburn_presentation::{AppViewModel, CommandKey, HeadlessEditorHarness, HostEffect, UiIntent};
use burnburn_runtime::automation::AutomationService;
use burnburn_windows::automation_host::{Automation, AutomationApp, AutomationPump, ChannelHost, channel_pair};
use g5_fixtures::*;

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

struct Rig {
    h: HeadlessEditorHarness,
    pump: AutomationPump,
    host: ChannelHost,
    sent: Vec<UiIntent>,
    now_ms: u64,
}

fn rig() -> Rig {
    let h = HeadlessEditorHarness::new();
    let fs = h.shared_fs();
    let (host, pump) = channel_pair(Arc::new(move |p: &Path| fs.exists(p).unwrap_or(false)), Arc::new(|| {}));
    Rig { h, pump, host, sent: Vec::new(), now_ms: 0 }
}

impl Rig {
    fn turn(&mut self) {
        for update in self.h.run_tasks() {
            self.pump.observe(&update.effects);
        }
        self.now_ms += 10;
        let mut app = App { h: &mut self.h, sent: &mut self.sent };
        self.pump.step(&mut app, self.now_ms);
    }

    fn ask(&mut self, request: &str) -> String {
        self.turn();
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

    fn put_project(&self, path: &str) -> PathBuf {
        let (snapshot, _) = structure_snapshot();
        let path = PathBuf::from(path);
        self.h.fs().create_dir_all_for_test(path.parent().unwrap());
        save_project(self.h.fs(), &path, &Project::new(ProjectUuid::from_bytes([5; 16]), snapshot)).unwrap();
        path
    }
}

#[test]
fn the_windows_build_answers_status_and_loads_a_project_only_when_nothing_is_unsaved() {
    let mut r = rig();
    assert_eq!(r.ask("STATUS"), "OK:READY:NODOC");
    let path = r.put_project("/work/a.burnburn");
    assert_eq!(r.ask(&format!("LOADFILE:{}", path.display())), "OK");
    assert_eq!(r.h.view().documents.len(), 1);
    assert_eq!(r.ask("STATUS"), "OK:READY:CLEAN");

    let doc = r.h.view().active.unwrap();
    r.h.invoke(doc, CommandKey::ToolRectangle);
    r.h.drag(doc, (300.0, 300.0), (310.0, 310.0));
    assert_eq!(r.ask("STATUS"), "OK:READY:DIRTY");
    r.sent.clear();
    assert_eq!(r.ask(&format!("LOADFILE:{}", path.display())), "ERR:DIRTY");
    assert!(r.sent.is_empty(), "a refused request sends nothing");
}

#[test]
fn a_machine_verb_is_refused_and_no_machine_intent_is_ever_sent() {
    let mut r = rig();
    r.h.new_document().unwrap();
    for verb in ["START", "FRAME", "JOG:1", "HOME", "UPLOAD:/x.rd"] {
        assert_eq!(r.ask(verb), "ERR:FORBIDDEN", "{verb}");
    }
    assert!(r.sent.is_empty());
}

#[test]
fn the_listener_follows_the_preference_on_loopback() {
    let h = HeadlessEditorHarness::new();
    let fs = h.shared_fs();
    let mut automation = Automation::with(Arc::new(move |p: &Path| fs.exists(p).unwrap_or(false)), Arc::new(|| {}), 0);
    assert!(!automation.running(), "off by default");
    automation.sync_preference(true).unwrap();
    assert!(automation.running());
    assert!(automation.local_addr().unwrap().ip().is_loopback());
    automation.sync_preference(false).unwrap();
    assert!(!automation.running());
}
