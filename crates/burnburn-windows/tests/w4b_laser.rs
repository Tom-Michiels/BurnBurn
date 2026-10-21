//! W4-L8b family 1, headless: the Windows laser window says what the evidence says (position
//! "onbekend", never 0; a control without wire evidence is off with the refusal in words) and its
//! controls send `UiIntent::Laser` with the view model's idempotency key unchanged. The machine is
//! a service over a fake port: no laser, no network.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::HashSet;

use burnburn_presentation::{
    DiagnosticOptions, FrameShape, JobAnchor, JogKey, LaserAction, LaserCommand, LaserControlKind, LaserSetup,
    MachineAction, StartFrom, UiIntent,
};
use burnburn_windows::host_panels::laser::{
    LaserInput, id, jog_control_for_vk, laser_spec, on_laser_event, on_laser_file_chosen,
};
use burnburn_windows::host_panels::{
    ConfirmPurpose, ControlKind, DialogRequest, FilePurpose, PanelEvent, PanelReaction, PanelValues, notify,
    on_confirmed,
};
use burnburn_windows::l10n::Locale;
use burnburn_types::{DocumentId, StableId};
use common::{click, event, laser_view, rig, texts};

fn input<'a>(
    doc: DocumentId,
    vm: &'a burnburn_presentation::AppViewModel,
    laser: Option<&'a burnburn_presentation::LaserViewModel>,
    machine: Option<&'a burnburn_presentation::MachineViewModel>,
    file_row: Option<usize>,
) -> LaserInput<'a> {
    LaserInput { document: doc, vm, laser, machine, locale: Locale::Nl, file_row }
}

fn label_text(spec: &burnburn_windows::host_panels::PanelSpec, id: u16) -> String {
    spec.control(id).unwrap_or_else(|| panic!("control {id} missing")).text.clone()
}

#[test]
fn without_a_machine_the_window_says_so_and_offers_no_laser_control() {
    let r = rig(false);
    let vm = r.h.view();
    let spec = laser_spec(&input(r.doc, &vm, None, None, None));
    assert_eq!(label_text(&spec, id::STATUS), "Geen machine verbonden.");
    for control in spec.controls() {
        let acts = matches!(control.kind, ControlKind::Button | ControlKind::HoldButton);
        let diagnostics = control.id == id::DIAGNOSE;
        assert!(!acts || !control.enabled || diagnostics, "{} must be off without a machine", control.id);
    }
    // Nothing reaches presentation either.
    let reaction = on_laser_event(&input(r.doc, &vm, None, None, None), &click(id::HOME, PanelValues::default()));
    assert_eq!(reaction, PanelReaction::Nothing);
}

#[test]
fn every_control_id_is_unique_and_inside_the_laser_block() {
    let mut r = rig(true);
    let vm = r.h.view();
    let laser = laser_view(&mut r);
    let machine = r.h.controller().machine_view();
    let spec = laser_spec(&input(r.doc, &vm, Some(&laser), machine.as_ref(), None));
    let mut seen = HashSet::new();
    for c in spec.controls() {
        assert!((7000..7200).contains(&c.id), "{} is outside 7000..7200", c.id);
        assert!(seen.insert(c.id), "duplicate control id {}", c.id);
    }
    assert!(spec.controls().count() > 50, "the whole window is described");
}

#[test]
fn the_position_reads_onbekend_never_zero_and_an_off_control_explains_itself() {
    let mut r = rig(false);
    let vm = r.h.view();
    let laser = laser_view(&mut r);
    let spec = laser_spec(&input(r.doc, &vm, Some(&laser), None, None));
    assert_eq!(label_text(&spec, id::POSITION), "X: onbekend   Y: onbekend");
    assert!(!label_text(&spec, id::POSITION).contains('0'));
    assert_eq!(label_text(&spec, id::USER_ORIGIN), "niet ingesteld");
    let home = spec.control(id::HOME).unwrap();
    assert!(!home.enabled);
    assert!(home.tooltip.starts_with("Niet geverifieerd"), "{}", home.tooltip);
    assert!(home.tooltip.contains("HOME_XY"), "the missing evidence is named: {}", home.tooltip);
    // An off control sends nothing: not an intent, not a request.
    let reaction = on_laser_event(&input(r.doc, &vm, Some(&laser), None, None), &click(id::HOME, PanelValues::default()));
    assert_eq!(reaction, PanelReaction::Nothing);
    let press = event(id::JOG_LEFT, notify::PRESS, PanelValues::default());
    assert_eq!(on_laser_event(&input(r.doc, &vm, Some(&laser), None, None), &press), PanelReaction::Nothing);
    assert!(r.sent.lock().unwrap().is_empty(), "no request reached the (fake) controller");
}

#[test]
fn a_proven_control_sends_the_view_models_key_unchanged_and_jogging_ends_with_a_release() {
    let mut r = rig(true);
    let vm = r.h.view();
    let laser = laser_view(&mut r);
    assert!(spec_enabled(&laser_spec(&input(r.doc, &vm, Some(&laser), None, None)), id::JOG_LEFT));

    let press = event(id::JOG_LEFT, notify::PRESS, PanelValues::default());
    let reaction = on_laser_event(&input(r.doc, &vm, Some(&laser), None, None), &press);
    let control = laser.control(&LaserControlKind::Jog(JogKey::XLeft)).unwrap();
    assert_eq!(
        reaction,
        PanelReaction::Intent(UiIntent::Laser {
            document: Some(r.doc),
            action: control.action,
            seen: laser.seen.clone(),
            command: LaserCommand::Action(LaserAction::JogPress(JogKey::XLeft)),
        })
    );
    // Release, a lost foreground and a closed window all stop the head; always allowed.
    for code in [notify::RELEASE, notify::DEACTIVATED, notify::CLOSED] {
        let ev = event(id::JOG_LEFT, code, PanelValues::default());
        assert_eq!(
            on_laser_event(&input(r.doc, &vm, Some(&laser), None, None), &ev),
            PanelReaction::Intent(UiIntent::LaserReleaseJog),
            "code {code:#x}"
        );
    }
    // Sending it makes the key count as held; the panel then says so.
    for intent in [PanelReaction::Intent(match reaction {
        PanelReaction::Intent(i) => i,
        _ => unreachable!("asserted above"),
    })] {
        for i in intent.intents() {
            r.h.send(i.clone());
        }
    }
    let held = laser_view(&mut r);
    assert_eq!(held.jog_held, Some(JogKey::XLeft));
    let spec = laser_spec(&input(r.doc, &vm, Some(&held), None, None));
    assert!(!label_text(&spec, id::JOG_NOTE).is_empty(), "the held key is shown in words");
}

fn spec_enabled(spec: &burnburn_windows::host_panels::PanelSpec, id: u16) -> bool {
    spec.control(id).unwrap().enabled
}

#[test]
fn arrow_keys_press_the_jog_buttons() {
    assert_eq!(jog_control_for_vk(0x25), Some(id::JOG_LEFT));
    assert_eq!(jog_control_for_vk(0x27), Some(id::JOG_RIGHT));
    assert_eq!(jog_control_for_vk(0x26), Some(id::JOG_UP));
    assert_eq!(jog_control_for_vk(0x28), Some(id::JOG_DOWN));
    assert_eq!(jog_control_for_vk(0x41), None);
}

#[test]
fn typed_numbers_go_to_presentation_unparsed_and_a_refusal_shows_in_words() {
    let mut r = rig(true);
    let vm = r.h.view();
    let laser = laser_view(&mut r);
    let values = texts(&[(id::MOVE_X, "abc"), (id::MOVE_Y, "1")]);
    let reaction = on_laser_event(&input(r.doc, &vm, Some(&laser), None, None), &click(id::MOVE_GO, values));
    let intents = reaction.intents();
    assert!(
        matches!(intents.as_slice(), [UiIntent::Laser { command: LaserCommand::MoveToText { x, y }, .. }] if x == "abc" && y == "1"),
        "{reaction:?}"
    );
    r.h.send(intents[0].clone());
    let refused = laser_view(&mut r);
    let spec = laser_spec(&input(r.doc, &vm, Some(&refused), None, None));
    assert_eq!(label_text(&spec, id::STATUS), "Geen geldig getal.");
    assert!(r.sent.lock().unwrap().is_empty(), "an invalid number never reaches the port");
}

#[test]
fn home_frames_and_the_fixed_buttons_become_their_laser_intents() {
    let mut r = rig(true);
    let vm = r.h.view();
    let laser = laser_view(&mut r);
    let inp = input(r.doc, &vm, Some(&laser), None, None);
    let home = on_laser_event(&inp, &click(id::HOME, PanelValues::default()));
    assert!(
        matches!(home.intents().as_slice(), [UiIntent::Laser { command: LaserCommand::Action(LaserAction::Home(_)), .. }]),
        "{home:?}"
    );
    // Frame buttons are off until a reviewed plan exists (no plan here): nothing is sent.
    let frame = laser.control(&LaserControlKind::Frame(FrameShape::BoundingRectangle)).unwrap();
    let reaction = on_laser_event(&inp, &click(id::FRAME_RECT, PanelValues::default()));
    assert_eq!(reaction.intents().is_empty(), !frame.enabled);
    let rubber = laser.control(&LaserControlKind::Frame(FrameShape::RubberBand)).unwrap();
    let reaction = on_laser_event(&inp, &click(id::FRAME_RUBBER, PanelValues::default()));
    assert_eq!(reaction.intents().is_empty(), !rubber.enabled);
}

#[test]
fn setup_choices_become_laser_setup_intents_and_reach_the_window() {
    let mut r = rig(true);
    let vm = r.h.view();
    let laser = laser_view(&mut r);
    let inp = input(r.doc, &vm, Some(&laser), None, None);
    let setup = |reaction: PanelReaction| match reaction {
        PanelReaction::Intent(UiIntent::LaserSetup { setup }) => setup,
        other => panic!("expected a setup intent, got {other:?}"),
    };

    let mut values = PanelValues::default();
    values.selection.insert(id::START_FROM, Some(2));
    assert_eq!(
        setup(on_laser_event(&inp, &event(id::START_FROM, notify::CHANGED, values))),
        LaserSetup::StartFrom(StartFrom::UserOrigin)
    );
    for (i, anchor) in JobAnchor::ALL.iter().enumerate() {
        let reaction = on_laser_event(&inp, &click(id::ANCHOR + i as u16, PanelValues::default()));
        assert_eq!(setup(reaction), LaserSetup::Anchor(*anchor));
    }
    let mut on = PanelValues::default();
    on.checks.insert(id::CUT_SELECTED, true);
    on.checks.insert(id::USE_SELECTION_ORIGIN, true);
    on.checks.insert(id::POSITION_LASER, true);
    assert_eq!(setup(on_laser_event(&inp, &click(id::CUT_SELECTED, on.clone()))), LaserSetup::CutSelected(true));
    assert_eq!(
        setup(on_laser_event(&inp, &click(id::USE_SELECTION_ORIGIN, on.clone()))),
        LaserSetup::UseSelectionOrigin(true)
    );
    assert_eq!(setup(on_laser_event(&inp, &click(id::POSITION_LASER, on))), LaserSetup::PositionLaserMode(true));
    assert_eq!(
        setup(on_laser_event(&inp, &event(id::STEP_SIZE, notify::ENTER, texts(&[(id::STEP_SIZE, "2,5")])))),
        LaserSetup::StepMm("2,5".into())
    );
    assert_eq!(
        setup(on_laser_event(&inp, &event(id::FRAME_SPEED, notify::EDIT_DONE, texts(&[(id::FRAME_SPEED, "80")])))),
        LaserSetup::FrameSpeed("80".into())
    );
    assert_eq!(setup(on_laser_event(&inp, &click(id::CLEAR_ORIGIN, PanelValues::default()))), LaserSetup::ClearOrigin);

    // The chosen setup comes back in the window: start-from, the marked anchor and the checks.
    r.h.send(UiIntent::LaserSetup { setup: LaserSetup::StartFrom(StartFrom::UserOrigin) });
    r.h.send(UiIntent::LaserSetup { setup: LaserSetup::Anchor(JobAnchor::Center) });
    r.h.send(UiIntent::LaserSetup { setup: LaserSetup::CutSelected(true) });
    let updated = laser_view(&mut r);
    let spec = laser_spec(&input(r.doc, &vm, Some(&updated), None, None));
    match &spec.control(id::START_FROM).unwrap().kind {
        ControlKind::Combo { items, selected } => {
            assert_eq!(items.len(), 3);
            assert_eq!(*selected, Some(2));
        }
        other => panic!("{other:?}"),
    }
    assert!(label_text(&spec, id::ANCHOR + 4).contains('●'));
    assert!(label_text(&spec, id::ANCHOR).contains('○'));
    assert!(matches!(spec.control(id::CUT_SELECTED).unwrap().kind, ControlKind::Check { checked: true }));
}

#[test]
fn an_unchanged_edit_sends_nothing() {
    let mut r = rig(true);
    let vm = r.h.view();
    let laser = laser_view(&mut r);
    let inp = input(r.doc, &vm, Some(&laser), None, None);
    // Return and the following focus loss both report the same text: only the first may send.
    let shown = label_text(&laser_spec(&inp), id::STEP_SIZE);
    assert!(!shown.is_empty());
    let same = event(id::STEP_SIZE, notify::EDIT_DONE, texts(&[(id::STEP_SIZE, &shown)]));
    assert_eq!(on_laser_event(&inp, &same), PanelReaction::Nothing);
}

#[test]
fn save_rd_sends_the_trimmed_name_and_the_controller_files_ask_before_deleting() {
    let mut r = rig(true);
    let vm = r.h.view();
    let laser = laser_view(&mut r);
    let inp = input(r.doc, &vm, Some(&laser), None, None);
    let reaction = on_laser_event(&inp, &click(id::SAVE_RD, texts(&[(id::SAVE_RD_NAME, "  test.rd ")])));
    if laser.offers(&LaserControlKind::SaveRd) {
        assert!(matches!(
            reaction.intents().as_slice(),
            [UiIntent::Laser { command: LaserCommand::SaveRd { name }, .. }] if name == "test.rd"
        ));
    } else {
        assert_eq!(reaction, PanelReaction::Nothing, "off control, nothing sent");
    }
    // No file list answered yet: the buttons of a file row have nothing to act on.
    let mut sel = PanelValues::default();
    sel.selection.insert(id::FILES, Some(0));
    for button in [id::FILE_INFO, id::FILE_RUN, id::FILE_DELETE] {
        assert_eq!(on_laser_event(&inp, &click(button, sel.clone())), PanelReaction::Nothing);
    }
}

#[test]
fn deleting_a_controller_file_asks_first_and_only_yes_sends() {
    let intent = UiIntent::LaserReleaseJog;
    let purpose = ConfirmPurpose::DeleteControllerFile { intent: Box::new(intent.clone()) };
    assert_eq!(on_confirmed(&purpose, true), PanelReaction::Intent(intent));
    assert_eq!(on_confirmed(&purpose, false), PanelReaction::Nothing);
}

#[test]
fn the_diagnostic_export_asks_for_a_path_with_the_ticked_options_and_cancel_exports_nothing() {
    let mut r = rig(true);
    let vm = r.h.view();
    let inp = input(r.doc, &vm, None, None, None);
    let mut values = PanelValues::default();
    values.checks.insert(id::DIAG_PATHS, true);
    let reaction = on_laser_event(&inp, &click(id::DIAGNOSE, values));
    let options = DiagnosticOptions { include_project_paths: true, include_machine_ids: false };
    match reaction {
        PanelReaction::Dialog(DialogRequest::Save { purpose, suggested_name, .. }) => {
            assert_eq!(suggested_name, "burnburn-diagnose.txt");
            assert_eq!(purpose, FilePurpose::Diagnostics { options });
            let path = std::path::PathBuf::from("C:/tmp/diagnose.txt");
            assert_eq!(
                on_laser_file_chosen(&purpose, Some(path.clone())),
                PanelReaction::Intent(UiIntent::ExportDiagnostics { options, path })
            );
            assert_eq!(on_laser_file_chosen(&purpose, None), PanelReaction::Nothing);
        }
        other => panic!("expected a save dialog, got {other:?}"),
    }
    // Both boxes start unticked: private data goes in only when the user ticks it.
    let spec = laser_spec(&inp);
    assert!(matches!(spec.control(id::DIAG_PATHS).unwrap().kind, ControlKind::Check { checked: false }));
    assert!(matches!(spec.control(id::DIAG_IDS).unwrap().kind, ControlKind::Check { checked: false }));
}

#[test]
fn compile_and_export_rd_invoke_the_commands_and_soft_stop_is_the_machines_cancel_control() {
    let mut r = rig(true);
    let vm = r.h.view();
    let laser = laser_view(&mut r);
    let machine = r.h.controller().machine_view();
    let inp = input(r.doc, &vm, Some(&laser), machine.as_ref(), None);
    let compile = on_laser_event(&inp, &click(id::COMPILE, PanelValues::default()));
    let enabled = vm.commands.get(&burnburn_presentation::CommandKey::Compile).is_some_and(|c| c.enabled);
    assert_eq!(
        compile.intents().first().cloned().cloned(),
        enabled.then_some(UiIntent::Invoke {
            document: Some(r.doc),
            command: burnburn_presentation::CommandKey::Compile
        })
    );
    // The software stop sends the machine view's own Cancel control (key and seen state unchanged),
    // and nothing when that control is off.
    let stop = on_laser_event(&inp, &click(id::SOFT_STOP, PanelValues::default()));
    let cancel = machine.as_ref().and_then(|m| m.button(MachineAction::Cancel));
    match (cancel, stop.intents().as_slice()) {
        (Some(b), [UiIntent::Machine { action, seen, .. }]) if b.enabled => {
            assert_eq!((*action, seen), (b.action, &b.seen));
        }
        (Some(b), []) if !b.enabled => {}
        (None, []) => {}
        other => panic!("soft stop mismatch: {other:?}"),
    }
}

#[test]
fn a_panel_event_of_a_window_that_is_gone_reaches_nothing() {
    let mut r = rig(true);
    let vm = r.h.view();
    let laser = laser_view(&mut r);
    let mut gone = input(r.doc, &vm, Some(&laser), None, None);
    gone.document = DocumentId::from_raw(9_999);
    let ev = PanelEvent::click(id::HOME, PanelValues::default());
    assert_eq!(on_laser_event(&gone, &ev), PanelReaction::Nothing);
}

/// The controller answered a file list (the fake port never does, so the answer is put in the view
/// model the way the controller would: names, an unreported size and per-file controls).
fn with_files(mut vm: burnburn_presentation::LaserViewModel) -> burnburn_presentation::LaserViewModel {
    use burnburn_presentation::{ControllerFileRow, LaserControl};
    let names = ["A.rd", "B.rd"];
    let rows: Vec<ControllerFileRow> = names
        .iter()
        .zip([Some(1024u64), None])
        .map(|(n, size)| ControllerFileRow {
            name: burnburn_machine::ControllerFileName::new(n).unwrap(),
            size_bytes: size,
        })
        .collect();
    let template = vm.control(&LaserControlKind::Home).unwrap().action;
    for row in &rows {
        for kind in [
            LaserControlKind::FileMetadata(row.name.clone()),
            LaserControlKind::RunFile(row.name.clone()),
            LaserControlKind::DeleteFile(row.name.clone()),
        ] {
            let run_is_off = matches!(kind, LaserControlKind::RunFile(_)) && row.name.as_str() == "A.rd";
            vm.controls.push(LaserControl {
                kind,
                action: template,
                enabled: !run_is_off,
                why_not: run_is_off.then_some(burnburn_presentation::LaserWhy::JobMayBeActive),
            });
        }
    }
    vm.files = Some(rows);
    vm
}

#[test]
fn the_controller_file_list_shows_unreported_sizes_as_onbekend_and_acts_on_the_selected_row() {
    let mut r = rig(true);
    let vm = r.h.view();
    let laser = with_files(laser_view(&mut r));
    let spec = laser_spec(&input(r.doc, &vm, Some(&laser), None, Some(1)));
    match &spec.control(id::FILES).unwrap().kind {
        ControlKind::List { items, selected, .. } => {
            assert_eq!(items, &["A.rd — 1024 B".to_owned(), "B.rd — onbekend".to_owned()]);
            assert_eq!(*selected, Some(1));
        }
        other => panic!("{other:?}"),
    }
    // Row 1 (B.rd): all three buttons act. Row 0 (A.rd): Run is off with its reason in words.
    assert!(spec.control(id::FILE_RUN).unwrap().enabled);
    let spec0 = laser_spec(&input(r.doc, &vm, Some(&laser), None, Some(0)));
    let run0 = spec0.control(id::FILE_RUN).unwrap();
    assert!(!run0.enabled && !run0.tooltip.is_empty(), "{run0:?}");
    // No row selected: nothing to act on.
    let none = laser_spec(&input(r.doc, &vm, Some(&laser), None, None));
    assert!(!none.control(id::FILE_INFO).unwrap().enabled);

    let inp = input(r.doc, &vm, Some(&laser), None, Some(1));
    let mut sel = PanelValues::default();
    sel.selection.insert(id::FILES, Some(1));
    let run = on_laser_event(&inp, &click(id::FILE_RUN, sel.clone()));
    assert!(matches!(
        run.intents().as_slice(),
        [UiIntent::Laser { command: LaserCommand::Action(LaserAction::RunFile(n)), .. }] if n.as_str() == "B.rd"
    ));
    // Delete asks first and carries the very intent that only a yes sends.
    match on_laser_event(&inp, &click(id::FILE_DELETE, sel.clone())) {
        PanelReaction::Dialog(DialogRequest::Confirm { purpose, title, body, confirm, .. }) => {
            assert_eq!(title, "Bestand op de controller verwijderen?");
            assert!(body.contains("niet in je projecten"));
            assert_eq!(confirm, "Verwijder");
            let sent = on_confirmed(&purpose, true);
            assert!(matches!(
                sent.intents().as_slice(),
                [UiIntent::Laser { command: LaserCommand::Action(LaserAction::DeleteFile(n)), .. }] if n.as_str() == "B.rd"
            ));
            assert_eq!(on_confirmed(&purpose, false), PanelReaction::Nothing);
        }
        other => panic!("expected a confirmation, got {other:?}"),
    }
    // A row that is off sends nothing, not even a question.
    let mut sel0 = PanelValues::default();
    sel0.selection.insert(id::FILES, Some(0));
    assert_eq!(on_laser_event(&inp, &click(id::FILE_RUN, sel0)), PanelReaction::Nothing);
}
