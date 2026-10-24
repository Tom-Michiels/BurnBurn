//! LFE: the laser window as a `FormSpec`: sections with a heading, the unit as a suffix, the reason
//! a section is greyed said once per section, the jog cross and the 3x3 job origin as pads, the
//! buttons by role, and every control mapped to the action it had in the hand-laid panel.
//! No AppKit, no machine: `LaserPanel` is plain data.

#![allow(clippy::expect_used, clippy::panic, clippy::bool_comparison)]

use burnburn_host_models::form::{Control, FormId, FormProblem, FormRow, FormSpec, PadCell, PadGlyph};
use burnburn_host_models::format::Quantity;
use burnburn_host_models::l10n::Locale;
use burnburn_host_models::panels::{
    LaserCheck, LaserFormAction, LaserPanel, LaserText, PanelControl, laser_form, laser_form_action, laser_id as id,
    laser_panel, laser_text, simple_controls,
};
use burnburn_presentation::{ControllerFileName, JogKey, LaserControlKind};

const REASON: &str = "Niet geverifieerd: dit is nog niet bewezen op deze controller.";

fn file(name: &str) -> ControllerFileName {
    ControllerFileName::new(name).expect("a valid name")
}

/// Every control of the window, all of them on or all of them off with `REASON`.
fn controls(on: bool) -> Vec<PanelControl> {
    let mut kinds: Vec<LaserControlKind> = simple_controls().into_iter().map(|(k, _)| k).collect();
    kinds.extend([JogKey::XLeft, JogKey::XRight, JogKey::YTop, JogKey::YBottom].map(LaserControlKind::Jog));
    kinds.extend([
        LaserControlKind::MoveTo,
        LaserControlKind::PositionLaser,
        LaserControlKind::SetOrigin,
        LaserControlKind::SaveRd,
    ]);
    for name in ["a.rd", "b.rd"] {
        kinds.extend([
            LaserControlKind::FileMetadata(file(name)),
            LaserControlKind::RunFile(file(name)),
            LaserControlKind::DeleteFile(file(name)),
        ]);
    }
    kinds
        .into_iter()
        .map(|kind| PanelControl { kind, enabled: on, tooltip: if on { String::new() } else { REASON.to_owned() } })
        .collect()
}

fn connected(on: bool) -> LaserPanel {
    let mut p = laser_panel(None, Locale::Nl);
    p.connected = true;
    p.status = String::new();
    p.position = "X: 120,5 mm   Y: onbekend".into();
    p.position_x.text = "120,5".into();
    p.position_x.known = true;
    p.step_mm = "1".into();
    p.frame_speed = "50".into();
    p.user_origin = "niet ingesteld".into();
    p.pointer_offset = "Geen pointeroffset gemeten voor deze machine; de kop wordt gebruikt.".into();
    p.controls = controls(on);
    p.files = vec![("a.rd".into(), "120 B".into()), ("b.rd".into(), "onbekend".into())];
    p
}

fn form(p: &LaserPanel) -> FormSpec {
    laser_form(p, 0, Locale::Nl)
}

fn row(spec: &FormSpec, id: u32) -> &FormRow {
    spec.row(id).unwrap_or_else(|| panic!("no row {id}"))
}

fn actions(spec: &FormSpec, id: u32) -> Vec<String> {
    match &row(spec, id).control {
        Control::Actions { titles } => titles.clone(),
        other => panic!("row {id} is not a row of buttons: {other:?}"),
    }
}

fn cells(spec: &FormSpec, id: u32) -> &[PadCell] {
    match &row(spec, id).control {
        Control::Pad { cells, .. } => cells,
        other => panic!("row {id} is not a pad: {other:?}"),
    }
}

fn t(text: LaserText) -> &'static str {
    laser_text(text, Locale::Nl)
}

#[test]
fn the_window_is_sections_with_a_heading_in_reading_order() {
    let spec = form(&connected(true));
    assert_eq!(spec.problems(), Vec::<FormProblem>::new());
    let headings: Vec<_> = spec.sections.iter().map(|s| s.heading.clone().unwrap_or_default()).collect();
    assert_eq!(
        headings,
        [
            t(LaserText::SectionMachine),
            t(LaserText::SectionMove),
            t(LaserText::SectionOrigin),
            t(LaserText::SectionStart),
            t(LaserText::SectionFrame),
            t(LaserText::Files),
            t(LaserText::SectionDiagnostics),
        ]
    );
    assert_eq!(spec.title, t(LaserText::PanelTitle));
    assert!(spec.tabs().is_empty(), "the stop button must not hide on another tab");
    // The software stop is in the first section: it is on screen when the window opens.
    let first: Vec<u32> = spec.sections[0].rows.iter().map(|r| r.id.0).collect();
    assert!(first.contains(&id::RUN_CONTROL), "{first:?}");
}

#[test]
fn units_are_suffixes_and_never_part_of_a_caption() {
    let spec = form(&connected(true));
    let unit = |i| row(&spec, i).unit;
    assert_eq!(unit(id::STEP_SIZE), Some(Quantity::Millimeters));
    assert_eq!(unit(id::MOVE_X), Some(Quantity::Millimeters));
    assert_eq!(unit(id::MOVE_Y), Some(Quantity::Millimeters));
    assert_eq!(unit(id::FRAME_SPEED), Some(Quantity::MillimetersPerSecond));
    for r in spec.sections.iter().flat_map(|s| &s.rows) {
        assert!(!r.label.contains("(mm") && !r.label.contains("mm)"), "{:?}", r.label);
    }
    // The step size, the frame speed and the move fields carry a caption (they were unlabelled once).
    for i in [id::STEP_SIZE, id::MOVE_X, id::MOVE_Y, id::FRAME_SPEED] {
        assert!(!row(&spec, i).label.is_empty(), "row {i} has no caption");
    }
    // The values are the model's text, untouched.
    assert!(matches!(&row(&spec, id::STEP_SIZE).control, Control::Number { value } if value == "1"));
    assert!(matches!(&row(&spec, id::FRAME_SPEED).control, Control::Number { value } if value == "50"));
}

#[test]
fn an_unknown_axis_reads_onbekend_without_a_unit_and_a_known_one_carries_mm() {
    let spec = form(&connected(true));
    let value = |i| match &row(&spec, i).control {
        Control::ReadOnly { value } => value.clone(),
        other => panic!("{other:?}"),
    };
    assert_eq!((value(id::POSITION_X), row(&spec, id::POSITION_X).unit), ("120,5".into(), Some(Quantity::Millimeters)));
    assert_eq!((value(id::POSITION_Y), row(&spec, id::POSITION_Y).unit), ("onbekend".into(), None));
    assert!(spec.sections.iter().flat_map(|s| s.texts()).all(|t| t != "0"), "an unknown axis is never 0");
}

#[test]
fn no_button_carries_a_unicode_glyph() {
    let spec = form(&connected(true));
    let glyphs = ['◀', '▶', '▲', '▼', '●', '○', '■'];
    let mut shown = Vec::new();
    for r in spec.sections.iter().flat_map(|s| &s.rows) {
        shown.push(r.label.clone());
        match &r.control {
            Control::Action { title } => shown.push(title.clone()),
            Control::Actions { titles } => shown.extend(titles.iter().cloned()),
            Control::Pad { cells, .. } => {
                for c in cells {
                    shown.push(c.title.clone());
                    shown.push(c.label.clone());
                }
            }
            _ => {}
        }
    }
    for text in shown {
        assert!(!text.contains(glyphs), "a glyph in {text:?}");
    }
    // The arrows are glyph ids the host draws as symbols.
    let jog = cells(&spec, id::JOG_PAD);
    assert_eq!(jog[1].glyph, Some(PadGlyph::Up));
    assert_eq!(jog[3].glyph, Some(PadGlyph::Left));
    assert_eq!(jog[5].glyph, Some(PadGlyph::Right));
    assert_eq!(jog[7].glyph, Some(PadGlyph::Down));
}

#[test]
fn the_jog_is_a_hold_cross_and_the_origin_a_3x3_grid() {
    let spec = form(&connected(true));
    let Control::Pad { columns, cells, hold } = &row(&spec, id::JOG_PAD).control else { panic!("jog is no pad") };
    assert_eq!((*columns, cells.len(), *hold), (3, 9, true), "jog presses and releases");
    let visible: Vec<usize> = cells.iter().enumerate().filter(|(_, c)| c.visible).map(|(i, _)| i).collect();
    assert_eq!(visible, vec![1, 3, 5, 7], "a cross: four buttons, the corners and the middle are gaps");
    assert_eq!(laser_form_action(FormId(id::JOG_PAD + 1)), Some(LaserFormAction::Jog(JogKey::YTop)));
    assert_eq!(laser_form_action(FormId(id::JOG_PAD + 3)), Some(LaserFormAction::Jog(JogKey::XLeft)));
    assert_eq!(laser_form_action(FormId(id::JOG_PAD + 5)), Some(LaserFormAction::Jog(JogKey::XRight)));
    assert_eq!(laser_form_action(FormId(id::JOG_PAD + 7)), Some(LaserFormAction::Jog(JogKey::YBottom)));
    for gap in [0, 2, 4, 6, 8] {
        assert_eq!(laser_form_action(FormId(id::JOG_PAD + gap)), None, "cell {gap} is a gap");
    }

    let Control::Pad { columns, cells, hold } = &row(&spec, id::ANCHOR_PAD).control else { panic!("origin is no pad") };
    assert_eq!((*columns, cells.len(), *hold), (3, 9, false));
    assert!(cells.iter().all(|c| c.visible && !c.label.is_empty()), "every cell is named for the screen reader");
    for i in 0..9 {
        assert_eq!(laser_form_action(FormId(id::ANCHOR_PAD + i)), Some(LaserFormAction::Anchor(i as usize)));
    }
}

#[test]
fn the_chosen_anchor_is_the_only_cell_that_is_on() {
    let mut p = connected(true);
    p.anchor = burnburn_presentation::JobAnchor::Center;
    let spec = form(&p);
    let on: Vec<usize> =
        cells(&spec, id::ANCHOR_PAD).iter().enumerate().filter(|(_, c)| c.on).map(|(i, _)| i).collect();
    assert_eq!(on, vec![4]);
    let glyphs: Vec<_> = cells(&spec, id::ANCHOR_PAD).iter().map(|c| c.glyph).collect();
    assert_eq!(glyphs.iter().filter(|g| **g == Some(PadGlyph::On)).count(), 1);
    assert_eq!(glyphs.iter().filter(|g| **g == Some(PadGlyph::Off)).count(), 8);
}

#[test]
fn a_section_that_is_greyed_says_why_once_and_the_rows_do_not_repeat_it() {
    // Each section holds controls that are off for a reason of its own (the words differ per kind).
    let mut p = connected(false);
    for c in &mut p.controls {
        let section = match &c.kind {
            LaserControlKind::ShowPosition | LaserControlKind::Pause | LaserControlKind::Resume => "machine",
            LaserControlKind::Home
            | LaserControlKind::GoToMachineOrigin
            | LaserControlKind::GoToUserOrigin
            | LaserControlKind::SetOrigin => "origin",
            LaserControlKind::Frame(_) => "frame",
            LaserControlKind::SaveRd
            | LaserControlKind::ListFiles
            | LaserControlKind::FileMetadata(_)
            | LaserControlKind::RunFile(_)
            | LaserControlKind::DeleteFile(_) => "files",
            _ => "move",
        };
        c.tooltip = format!("Niet geverifieerd: {section} is nog niet bewezen op deze controller.");
    }
    let spec = form(&p);
    assert_eq!(spec.problems(), Vec::<FormProblem>::new(), "no row repeats the reason of its section");
    // Machine, move, origin, frame and files each say theirs once; the start section only holds window
    // setup (nothing is sent to the machine) and has nothing to say.
    let said: Vec<(usize, String)> = spec
        .sections
        .iter()
        .enumerate()
        .map(|(i, s)| (i, s.reason.clone().unwrap_or_default()))
        .filter(|(_, r)| !r.is_empty())
        .collect();
    assert_eq!(said.iter().map(|(i, _)| *i).collect::<Vec<_>>(), vec![0, 1, 2, 4, 5], "{said:?}");
    let all: Vec<String> = spec.sections.iter().flat_map(|s| s.texts()).collect();
    for (i, reason) in &said {
        assert_eq!(all.iter().filter(|x| *x == reason).count(), 1, "section {i}: {reason}");
        assert!(reason.contains(
            ["machine", "move", "origin", "frame", "files"][match i {
                0 => 0,
                1 => 1,
                2 => 2,
                4 => 3,
                _ => 4,
            }]
        ));
    }
    // Every section keeps room for a reason, so the shape does not change when the evidence arrives.
    assert!(spec.sections.iter().all(|s| s.reason.is_some()));
    assert_eq!(form(&connected(true)).shape(), spec.shape());
    assert!(form(&connected(true)).sections.iter().all(|s| s.reason.as_deref().is_none_or(str::is_empty)));
}

#[test]
fn a_reason_that_several_sections_share_is_said_once_in_the_window() {
    // The same sentence for every control (as when nothing at all is proven on the controller).
    let spec = form(&connected(false));
    let all: Vec<String> = spec.sections.iter().flat_map(|s| s.texts()).collect();
    assert_eq!(all.iter().filter(|x| x.as_str() == REASON).count(), 1, "said once: {all:?}");
    assert_eq!(spec.sections[0].reason.as_deref(), Some(REASON), "in the first section that has it");
    assert!(spec.sections[1..].iter().all(|s| s.reason.as_deref() == Some("")), "the others keep their room");
    // Every button still has the words in its own tooltip.
    let jog = cells(&spec, id::JOG_PAD);
    assert_eq!(jog[1].tooltip.as_deref(), Some(REASON));
}

#[test]
fn without_a_machine_the_status_says_so_once_for_the_whole_window_and_everything_else_is_off() {
    let p = laser_panel(None, Locale::Nl);
    let spec = form(&p);
    let no_machine = t(LaserText::NoMachine);
    // One sentence for the whole window (the status line), not the same sentence in every section.
    for section in &spec.sections {
        assert!(section.reason.as_deref().is_none_or(str::is_empty), "{:?}", section.heading);
    }
    assert_eq!(spec.sections.iter().flat_map(|s| s.texts()).filter(|x| x == no_machine).count(), 0);
    let status = match &row(&spec, id::STATUS).control {
        Control::Note { value, .. } => value.clone(),
        other => panic!("{other:?}"),
    };
    assert_eq!(status, no_machine);
    for r in spec.sections.iter().flat_map(|s| &s.rows).filter(|r| r.id.0 != id::DIAGNOSTICS) {
        let off = match &r.control {
            Control::Pad { cells, .. } => cells.iter().all(|c| !c.enabled),
            _ => !r.enabled,
        };
        let read_only = matches!(r.control, Control::ReadOnly { .. } | Control::Note { .. });
        assert!(off || read_only, "row {} ({:?}) stays on without a machine", r.id.0, r.label);
    }
    // Exporting the diagnostics replays the journal and needs no machine.
    assert!(row(&spec, id::DIAGNOSTICS).enabled);
}

#[test]
fn each_control_follows_the_evidence_of_its_kind() {
    let mut p = connected(false);
    for c in &mut p.controls {
        if matches!(c.kind, LaserControlKind::Home | LaserControlKind::Jog(JogKey::XLeft)) {
            c.enabled = true;
            c.tooltip.clear();
        }
    }
    let spec = form(&p);
    // The buttons of one row each have their own state; the tooltip of an off one is the reason.
    let jog = cells(&spec, id::JOG_PAD);
    assert!(jog[3].enabled && jog[1].enabled == false && jog[5].enabled == false);
    assert_eq!(jog[1].tooltip.as_deref(), Some(REASON));
    assert_eq!(jog[3].tooltip.as_deref().unwrap_or(""), "");
    assert_eq!(actions(&spec, id::GO_HOME).len(), 3);
    let home: Vec<(bool, Option<&str>)> =
        row(&spec, id::GO_HOME).items.iter().map(|i| (i.enabled, i.tooltip.as_deref())).collect();
    assert_eq!(home, vec![(true, None), (false, Some(REASON)), (false, Some(REASON))]);
    assert_eq!(laser_form_action(FormId(id::GO_HOME)), Some(LaserFormAction::Simple(0)));
    assert_eq!(laser_form_action(FormId(id::GO_HOME + 1)), Some(LaserFormAction::Simple(1)));
    assert_eq!(laser_form_action(FormId(id::GO_HOME + 2)), Some(LaserFormAction::Simple(2)));
}

#[test]
fn the_buttons_have_their_role_and_the_delete_is_last_of_its_row() {
    let spec = form(&connected(true));
    // A floating panel has no dialog buttons: nothing is applied or cancelled, every control acts.
    assert_eq!(spec.buttons, Default::default());
    // Pause, resume and the software stop: the stop is the last of the row, away from the others.
    assert_eq!(actions(&spec, id::RUN_CONTROL), [t(LaserText::Pause), t(LaserText::Resume), t(LaserText::SoftStop)]);
    assert_eq!(laser_form_action(FormId(id::RUN_CONTROL + 2)), Some(LaserFormAction::SoftStop));
    assert_eq!(
        row(&spec, id::RUN_CONTROL).help.as_deref(),
        Some(t(LaserText::StopNote)),
        "the hardware stop stays leading, said under the buttons"
    );
    // Refresh, info, run and delete: delete is last, and it asks before it acts (the host sheet).
    let files = actions(&spec, id::FILE_BUTTONS);
    assert_eq!(
        files,
        [t(LaserText::ListFiles), t(LaserText::FileInfo), t(LaserText::FileRun), t(LaserText::FileDelete)]
    );
    assert_eq!(laser_form_action(FormId(id::FILE_BUTTONS)), Some(LaserFormAction::Simple(6)));
    assert_eq!(laser_form_action(FormId(id::FILE_BUTTONS + 1)), Some(LaserFormAction::File(0)));
    assert_eq!(laser_form_action(FormId(id::FILE_BUTTONS + 2)), Some(LaserFormAction::File(1)));
    assert_eq!(laser_form_action(FormId(id::FILE_BUTTONS + 3)), Some(LaserFormAction::File(2)));
}

#[test]
fn every_interactive_id_maps_to_the_action_it_had() {
    use LaserFormAction as A;
    let table = [
        (id::SHOW_POSITION, A::Simple(5)),
        (id::RUN_CONTROL, A::Simple(3)),
        (id::RUN_CONTROL + 1, A::Simple(4)),
        (id::STEP_PAD, A::Simple(7)),
        (id::STEP_PAD + 1, A::Simple(8)),
        (id::STEP_PAD + 2, A::Simple(9)),
        (id::STEP_PAD + 3, A::Simple(10)),
        (id::STEP_SIZE, A::StepSize),
        (id::MOVE_X, A::MoveX),
        (id::MOVE_Y, A::MoveY),
        (id::MOVE_TO, A::MoveTo),
        (id::POSITION_LASER, A::Check(LaserCheck::PositionLaser)),
        (id::POINTER_OFFSET, A::Check(LaserCheck::PointerOffset)),
        (id::ORIGIN, A::SetOrigin),
        (id::ORIGIN + 1, A::ClearOrigin),
        (id::START_FROM, A::StartFrom),
        (id::CUT_SELECTED, A::Check(LaserCheck::CutSelected)),
        (id::SELECTION_ORIGIN, A::Check(LaserCheck::UseSelectionOrigin)),
        (id::FRAME, A::Simple(11)),
        (id::FRAME + 1, A::Simple(12)),
        (id::FRAME_SPEED, A::FrameSpeed),
        (id::RD_NAME, A::RdName),
        (id::SAVE_RD, A::SaveRd),
        (id::FILE_CHOICE, A::FileChoice),
        (id::DIAGNOSTICS, A::Diagnostics),
    ];
    for (i, action) in table {
        assert_eq!(laser_form_action(FormId(i)), Some(action), "id {i}");
    }
    for read_only in [id::POSITION_X, id::POSITION_Y, id::STATUS, id::USER_ORIGIN, id::PLACEMENT, id::FRAME_NOTE] {
        assert_eq!(laser_form_action(FormId(read_only)), None, "id {read_only} is only read");
    }
    // The ids of the form are exactly the ones that answer, plus the pad gaps and read-only rows.
    let spec = form(&connected(true));
    let mut seen = std::collections::HashSet::new();
    for r in spec.sections.iter().flat_map(|s| &s.rows) {
        assert!(seen.insert(r.id), "id {} twice", r.id.0);
    }
}

#[test]
fn the_files_are_one_choice_with_the_names_and_the_chosen_one_decides_the_buttons() {
    let mut p = connected(true);
    // Only the second file may be run; nothing may be deleted.
    for c in &mut p.controls {
        c.enabled = match &c.kind {
            LaserControlKind::RunFile(n) => n.as_str() == "b.rd",
            LaserControlKind::DeleteFile(_) => false,
            _ => true,
        };
        if !c.enabled {
            c.tooltip = REASON.to_owned();
        }
    }
    let first = laser_form(&p, 0, Locale::Nl);
    let Control::Choice { options, selected } = &row(&first, id::FILE_CHOICE).control else { panic!("no choice") };
    assert_eq!(options, &vec!["a.rd (120 B)".to_owned(), "b.rd (onbekend)".to_owned()]);
    assert_eq!(*selected, 0);
    assert!(row(&first, id::FILE_CHOICE).enabled);
    // The buttons are the ones of the chosen file: [refresh, info, run, delete].
    let state =
        |spec: &FormSpec| -> Vec<bool> { row(spec, id::FILE_BUTTONS).items.iter().map(|i| i.enabled).collect() };
    assert_eq!(state(&first), vec![true, true, false, false]);
    let second = laser_form(&p, 1, Locale::Nl);
    assert_eq!(state(&second), vec![true, true, true, false]);
    // A choice past the end is the last file, never a panic.
    let past = laser_form(&p, 9, Locale::Nl);
    assert!(matches!(&row(&past, id::FILE_CHOICE).control, Control::Choice { selected: 1, .. }));
}

#[test]
fn no_files_is_said_in_words_and_switches_the_file_buttons_off() {
    let mut p = connected(true);
    p.files.clear();
    let spec = form(&p);
    let Control::Choice { options, .. } = &row(&spec, id::FILE_CHOICE).control else { panic!() };
    assert_eq!(options, &vec![t(LaserText::NoFiles).to_owned()]);
    assert!(!row(&spec, id::FILE_CHOICE).enabled);
}

#[test]
fn values_change_but_the_shape_of_the_window_stays() {
    let a = connected(true);
    let mut b = connected(true);
    b.step_mm = "2,5".into();
    b.frame_speed = "80".into();
    b.jog_held = true;
    b.status = "Niet geverifieerd: iets.".into();
    b.placement = "Job verschoven met (1; 2) mm".into();
    b.anchor = burnburn_presentation::JobAnchor::TopRight;
    b.cut_selected = true;
    b.use_pointer_offset = true;
    b.start_from = burnburn_presentation::StartFrom::UserOrigin;
    assert_eq!(form(&a).shape(), form(&b).shape());
    assert_ne!(form(&a), form(&b));
    // A different number of files is another list: the host builds again.
    let mut c = connected(true);
    c.files.pop();
    assert_ne!(form(&a).shape(), form(&c).shape());
}

#[test]
fn the_english_window_has_no_dutch_headings() {
    let spec = laser_form(&connected(true), 0, Locale::En);
    let headings: Vec<_> = spec.sections.iter().filter_map(|s| s.heading.clone()).collect();
    assert!(headings.contains(&"Position and Jog".to_owned()), "{headings:?}");
    assert!(headings.contains(&"Files on the controller".to_owned()), "{headings:?}");
}
