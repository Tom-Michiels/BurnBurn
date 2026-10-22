//! G7-werk, W09 on Windows, headless: the laser window offers the red-dot switch next to the other
//! setup checks and names the measured offset it uses (or says there is none), and its plan section
//! lists the measured values the compiled plan used with their proofs. Same words and intents as the
//! macOS panel (`burnburn-macos/tests/g7_werk_laser_pointer.rs`).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use burnburn_presentation::{LaserSetup, ProofInput, ProofKind, UiIntent};
use burnburn_windows::host_panels::laser::{LaserInput, id, laser_spec, on_laser_event};
use burnburn_windows::host_panels::{ControlKind, PanelReaction, PanelSpec, PanelValues};
use burnburn_windows::l10n::Locale;
use common::{Rig, click, laser_view, rig};

fn spec(r: &mut Rig, locale: Locale) -> PanelSpec {
    let vm = r.h.view();
    let laser = laser_view(r);
    laser_spec(&LaserInput { document: r.doc, vm: &vm, laser: Some(&laser), machine: None, locale, file_row: None })
}

fn record_pointer(r: &mut Rig) {
    r.h.send(UiIntent::RecordCalibration {
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
    r.h.run_tasks();
}

fn checked(spec: &PanelSpec, cid: u16) -> bool {
    matches!(spec.control(cid).unwrap().kind, ControlKind::Check { checked: true })
}

#[test]
fn without_a_measured_offset_the_switch_is_off_and_the_note_says_so() {
    let mut r = rig(true);
    let nl = spec(&mut r, Locale::Nl);
    assert!(!nl.control(id::POINTER_OFFSET).unwrap().enabled);
    assert_eq!(
        nl.control(id::POINTER_NOTE).unwrap().text,
        "Geen pointeroffset gemeten voor deze machine; de kop wordt gebruikt."
    );
    assert_eq!(nl.control(id::POINTER_OFFSET).unwrap().text, "Met de rode stip richten");
    assert_eq!(spec(&mut r, Locale::En).control(id::POINTER_OFFSET).unwrap().text, "Aim with the red dot");
}

#[test]
fn with_a_measured_offset_the_note_names_it_and_the_switch_sends_the_setup() {
    let mut r = rig(true);
    record_pointer(&mut r);
    let nl = spec(&mut r, Locale::Nl);
    assert_eq!(nl.control(id::POINTER_NOTE).unwrap().text, "Rode stip: (2; -1,5) mm");
    assert!(nl.control(id::POINTER_OFFSET).unwrap().enabled);
    assert!(checked(&nl, id::POINTER_OFFSET), "a measured offset is used unless switched off");

    let vm = r.h.view();
    let laser = laser_view(&mut r);
    let input =
        LaserInput { document: r.doc, vm: &vm, laser: Some(&laser), machine: None, locale: Locale::Nl, file_row: None };
    let mut values = PanelValues::default();
    values.checks.insert(id::POINTER_OFFSET, false);
    let reaction = on_laser_event(&input, &click(id::POINTER_OFFSET, values));
    assert_eq!(reaction, PanelReaction::Intent(UiIntent::LaserSetup { setup: LaserSetup::UsePointerOffset(false) }));
}

#[test]
fn the_plan_section_has_a_line_for_the_measured_values_and_it_is_empty_for_a_plain_plan() {
    let mut r = rig(true);
    let nl = spec(&mut r, Locale::Nl);
    let calibration = nl.control(id::PLAN_CALIBRATION).expect("the plan calibration line exists");
    assert_eq!(calibration.text, "");
}
