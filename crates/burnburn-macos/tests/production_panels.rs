//! Poort C screens, headless: the machine panel says exactly what the evidence says (uploaded is
//! not stored, start sent is not running, nothing is "klaar" before the machine reports it), its
//! buttons send `UiIntent::Machine` with `action`/`seen` unchanged; the import preview needs an
//! explicit unit before Importeer.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use burnburn_io::fs::FileSystem;
use burnburn_machine::{ExecutionState, MachineId};
use burnburn_macos::adapter::{NativeEvent, Reaction, translate};
use burnburn_macos::l10n::{Locale, machine_status_text};
use burnburn_macos::panels::{IMPORT_UNITS, import_panel, machine_intent, machine_panel};
use burnburn_presentation::{
    ActionId, ActionSubject, CommandKey, HeadlessEditorHarness, ImportUnit, MachineAction, MachineButton,
    MachineStatusKey, MachineViewModel, MessageKey, UiIntent,
};
use burnburn_types::StableId;

const ALL_STATUS: [MachineStatusKey; 12] = [
    MachineStatusKey::Idle,
    MachineStatusKey::VerifyingIdentity,
    MachineStatusKey::Reverifying,
    MachineStatusKey::Uploading,
    MachineStatusKey::UploadedNotConfirmed,
    MachineStatusKey::Stored,
    MachineStatusKey::StartRequested,
    MachineStatusKey::Running,
    MachineStatusKey::Completed,
    MachineStatusKey::Failed,
    MachineStatusKey::Cancelled,
    MachineStatusKey::Unknown,
];

#[test]
fn machine_status_is_explicit_and_never_done_before_the_machine_says_so() {
    for status in ALL_STATUS {
        for locale in [Locale::Nl, Locale::En] {
            let text = machine_status_text(status, locale).to_lowercase();
            assert!(!text.trim().is_empty(), "{status:?} {locale:?}");
            if status != MachineStatusKey::Completed {
                for word in ["klaar", "voltooid", "gereed", "done", "complete", "finished", "ready"] {
                    assert!(!text.contains(word), "{status:?} {locale:?} says {text:?}");
                }
            }
        }
    }
    assert_eq!(
        machine_status_text(MachineStatusKey::UploadedNotConfirmed, Locale::Nl),
        "Geüpload — opslag nog niet bevestigd"
    );
    assert_eq!(machine_status_text(MachineStatusKey::Stored, Locale::Nl), "Opgeslagen (bevestigd door de controller)");
    assert_eq!(machine_status_text(MachineStatusKey::StartRequested, Locale::Nl), "Start verzonden");
    assert_eq!(machine_status_text(MachineStatusKey::Unknown, Locale::Nl), "Onbekend — controleer de machine");
    assert_eq!(machine_status_text(MachineStatusKey::Unknown, Locale::En), "Unknown — check the machine");
}

fn button(kind: MachineAction, enabled: bool, seen: Option<ExecutionState>) -> MachineButton {
    MachineButton {
        action: ActionId { kind, subject: ActionSubject::Machine, generation: 7 },
        enabled,
        seen,
        label: MessageKey::MachineAction(kind),
    }
}

#[test]
fn machine_panel_shows_status_message_progress_and_buttons() {
    let none = machine_panel(None, Locale::Nl);
    assert_eq!(none.status, "Geen machine verbonden");
    assert!(none.buttons.is_empty());

    let vm = MachineViewModel {
        machine: MachineId::new("bench-1"),
        status: MachineStatusKey::UploadedNotConfirmed,
        message: Some(MessageKey::UploadedNotStoredConfirmed),
        execution: None,
        job: None,
        job_name: Some("Plaat".into()),
        progress: Some((12, 12)),
        buttons: vec![
            button(MachineAction::ConfirmStored, true, Some(ExecutionState::UploadAcked)),
            button(MachineAction::Start, false, Some(ExecutionState::UploadAcked)),
        ],
        last_action: None,
    };
    let panel = machine_panel(Some(&vm), Locale::Nl);
    assert_eq!(panel.status, "Geüpload — opslag nog niet bevestigd");
    assert_eq!(panel.message, "Geüpload, nog niet opgeslagen bevestigd.");
    assert_eq!(panel.progress, "12 van 12 pakketten ontvangen (transport)");
    assert_eq!(panel.job, "Plaat");
    assert_eq!(panel.buttons, vec![("Controleer opslag".to_owned(), true), ("Start".to_owned(), false)]);
}

#[test]
fn machine_buttons_send_their_action_unchanged_and_disabled_ones_send_nothing() {
    let doc = Some(burnburn_types::DocumentId::from_raw(3));
    let confirm = button(MachineAction::ConfirmStored, true, Some(ExecutionState::UploadAcked));
    assert_eq!(
        machine_intent(doc, &confirm),
        Some(UiIntent::Machine { document: doc, action: confirm.action, seen: confirm.seen.clone() })
    );
    let start = button(MachineAction::Start, false, Some(ExecutionState::UploadAcked));
    assert_eq!(machine_intent(doc, &start), None, "a disabled control is not a click");
}

fn import_fixture(name: &str) -> (HeadlessEditorHarness, burnburn_types::DocumentId, PathBuf) {
    let mut h = HeadlessEditorHarness::new();
    h.fs().create_dir_all_for_test(std::path::Path::new("/in"));
    let bytes = std::fs::read(format!("{}/../../test-data/import/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap();
    let temp = h.fs().create_temp_in(std::path::Path::new("/in"), ".fixture").unwrap();
    h.fs().write_all(&temp, &bytes).unwrap();
    let path = PathBuf::from(format!("/in/{name}"));
    h.fs().replace(&temp, &path).unwrap();
    let doc = h.new_document().unwrap();
    (h, doc, path)
}

#[test]
fn import_preview_asks_for_a_unit_and_the_choice_becomes_one_intent() {
    let (mut h, doc, path) = import_fixture("unitless-lines.dxf");
    assert_eq!(import_panel(&h.view(), doc, Locale::Nl), None, "no preview before a file");
    let Reaction::Intent(intent) =
        translate(NativeEvent::ImportFileChosen { document: doc, path: Some(path.clone()) }, &h.view())
    else {
        panic!("chosen file is an Import")
    };
    assert_eq!(intent, UiIntent::Import { document: doc, path });
    assert_eq!(translate(NativeEvent::ImportFileChosen { document: doc, path: None }, &h.view()), Reaction::Nothing);
    h.send(intent);
    h.run_tasks();

    let panel = import_panel(&h.view(), doc, Locale::Nl).expect("preview");
    assert_eq!(panel.file, "unitless-lines.dxf");
    assert_eq!(panel.unit_prompt, "Het bestand noemt geen eenheid: kies er een.");
    assert_eq!(panel.selected_unit, None);
    assert_eq!(panel.size, "Maat: onbekend tot je een eenheid kiest");
    assert!(!panel.commit_enabled);
    assert_eq!(IMPORT_UNITS, [ImportUnit::Millimeter, ImportUnit::Centimeter, ImportUnit::Inch]);

    let inch = IMPORT_UNITS.iter().position(|u| *u == ImportUnit::Inch).unwrap();
    let Reaction::Intent(intent) =
        translate(NativeEvent::ImportUnitChosen { document: doc, index: inch as isize }, &h.view())
    else {
        panic!("unit choice")
    };
    assert_eq!(intent, UiIntent::SetImportUnit { document: doc, unit: ImportUnit::Inch });
    h.send(intent);
    let panel = import_panel(&h.view(), doc, Locale::Nl).unwrap();
    assert_eq!(panel.selected_unit, Some(inch));
    assert_eq!(panel.size, "Maat: 101.6 × 50.8 mm");
    assert!(panel.commit_enabled);
    assert_eq!(translate(NativeEvent::ImportUnitChosen { document: doc, index: -1 }, &h.view()), Reaction::Nothing);

    let Reaction::Intent(commit) =
        translate(NativeEvent::PanelCommand { document: doc, command: CommandKey::CommitImport }, &h.view())
    else {
        panic!("Importeer")
    };
    h.send(commit);
    assert_eq!(h.document(doc).unwrap().objects.len(), 2);
    assert_eq!(import_panel(&h.view(), doc, Locale::Nl), None);
}

#[test]
fn import_report_lists_skipped_entities_and_open_contours() {
    let (mut h, doc, path) = import_fixture("mm-plate.dxf");
    h.send(UiIntent::Import { document: doc, path });
    h.run_tasks();
    let panel = import_panel(&h.view(), doc, Locale::En).unwrap();
    assert_eq!(panel.unit_prompt, "Unit from the file: mm");
    assert!(panel.commit_enabled);
    assert!(panel.report.contains("1 open contour"), "{}", panel.report);
    assert!(panel.report.contains("not imported"), "{}", panel.report);
}
