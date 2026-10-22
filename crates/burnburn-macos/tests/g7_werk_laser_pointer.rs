//! G7-werk, W09: the laser panel offers the red-dot switch and says which measured offset it
//! uses (or that there is none); the placement note names the dot. Headless over the presentation
//! lab; a fake controller, no laser.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, dead_code)]

#[path = "../../burnburn-presentation/tests/common/mod.rs"]
mod common;

use burnburn_machine::{JobAnchor, LiveRequest, Reply, Request, StartFrom};
use burnburn_macos::l10n::Locale;
use burnburn_macos::panels::{laser_panel, setup_intent};
use burnburn_presentation::{
    CommandKey, HeadlessEditorHarness, LaserAction, LaserCommand, LaserControlKind, LaserSetup, ProofInput, ProofKind,
    UiIntent,
};
use burnburn_test_support::Step;
use common::laser::{Lab, all_supported, lab};
use common::*;

struct Win {
    h: HeadlessEditorHarness,
    doc: burnburn_types::DocumentId,
    lab: Lab,
}

fn win() -> Win {
    let lab = lab();
    lab.service.set_capabilities(all_supported());
    let mut h = HeadlessEditorHarness::new();
    h.controller().attach_machine(lab.service.clone());
    let doc = h.new_document().expect("window");
    design(&mut h, doc);
    Win { h, doc, lab }
}

fn panel(w: &mut Win, locale: Locale) -> burnburn_macos::panels::LaserPanel {
    laser_panel(w.h.controller().laser_view().as_ref(), locale)
}

fn record_pointer(w: &mut Win) {
    w.h.send(UiIntent::RecordCalibration {
        input: ProofInput {
            kind: ProofKind::PointerOffset,
            material: String::new(),
            speed: String::new(),
            value: "2".into(),
            value_y: "-1,5".into(),
            method: "kruis gebrand".into(),
            date: "2026-10-22".into(),
        },
    });
    w.h.run_tasks();
}

#[test]
fn without_a_measured_offset_the_panel_says_so_and_the_switch_is_not_offered() {
    let mut w = win();
    let nl = panel(&mut w, Locale::Nl);
    assert_eq!(nl.pointer_offset, "Geen pointeroffset gemeten voor deze machine; de kop wordt gebruikt.");
    assert!(!nl.can_use_pointer_offset);
    let en = panel(&mut w, Locale::En);
    assert_eq!(en.pointer_offset, "No pointer offset measured for this machine; the head is used.");
}

#[test]
fn with_a_measured_offset_the_panel_names_it_and_the_switch_reaches_the_setup() {
    let mut w = win();
    record_pointer(&mut w);
    let nl = panel(&mut w, Locale::Nl);
    assert_eq!(nl.pointer_offset, "Rode stip: (2; -1,5) mm");
    assert!(nl.can_use_pointer_offset && nl.use_pointer_offset, "a measured offset is used unless switched off");
    assert_eq!(panel(&mut w, Locale::En).pointer_offset, "Red dot: (2; -1.5) mm");

    w.h.send(setup_intent(LaserSetup::UsePointerOffset(false)));
    assert!(!panel(&mut w, Locale::Nl).use_pointer_offset);
    w.h.send(setup_intent(LaserSetup::UsePointerOffset(true)));
    assert!(panel(&mut w, Locale::Nl).use_pointer_offset);
}

#[test]
fn the_placement_note_names_the_red_dot_when_the_job_was_aimed_with_it() {
    let mut w = win();
    record_pointer(&mut w);
    w.lab.peer.script(
        Step::expect(Request::Live(LiveRequest::ReadPosition))
            .reply(Reply::Ack)
            .reply(Reply::Position { x_um: Some(500_000), y_um: Some(400_000) }),
    );
    let vm = w.h.controller().laser_view().unwrap();
    let control = vm.control(&LaserControlKind::ShowPosition).unwrap();
    w.h.send(UiIntent::Laser {
        document: Some(w.doc),
        action: control.action,
        seen: vm.seen.clone(),
        command: LaserCommand::Action(LaserAction::ShowPosition),
    });
    for _ in 0..8 {
        w.lab.service.poll().unwrap();
    }
    w.h.send(setup_intent(LaserSetup::StartFrom(StartFrom::CurrentPosition)));
    w.h.send(setup_intent(LaserSetup::Anchor(JobAnchor::TopLeft)));
    w.h.invoke(w.doc, CommandKey::Compile);
    w.h.run_tasks();
    let nl = panel(&mut w, Locale::Nl);
    assert!(nl.placement.contains("rode stip (2; -1,5) mm"), "{}", nl.placement);
    let en = panel(&mut w, Locale::En);
    assert!(en.placement.contains("red dot (2; -1.5) mm"), "{}", en.placement);
}
