//! The laser window on Windows (W4-L8b): `LaserViewModel` + `MachineViewModel` become a
//! [`PanelSpec`], and what the user does in it becomes the same `UiIntent`s the Mac panel sends.
//!
//! Nothing here contacts a machine. The position reads "onbekend" until the controller answered
//! (never 0), a control without wire evidence is off with the refusal in words as its tooltip,
//! numbers go on as typed (DR-05) and a held jog key is always released again.

use burnburn_presentation::{
    AppViewModel, CommandKey, DiagnosticOptions, FrameShape, JogKey, LaserControlKind, LaserSetup, LaserViewModel,
    MachineAction, MachineViewModel, UiIntent,
};
use burnburn_types::DocumentId;

use super::ids::LASER_CONTROLS;
use super::spec::notify;
use super::{
    Chosen, ConfirmPurpose, ControlSpec, DialogRequest, FileFilter, FilePurpose, PanelEvent, PanelReaction, PanelSpec,
    Row,
};
use crate::l10n::{HostText, Locale, host_text};
use crate::panels::{
    LaserPanel, LaserText, anchor_for_tag, anchor_text, file_intent, jog_press_intent, laser_panel, laser_text,
    machine_intent, machine_panel, move_intent, plan_panel, save_rd_intent, segment_for_start_from, setup_intent,
    simple_controls, simple_intent, start_from_for_segment,
};

/// Control ids of the laser window (`7000..7200`).
pub mod id {
    use super::LASER_CONTROLS as B;

    pub const STATUS: u16 = B;
    pub const POSITION: u16 = B + 1;
    pub const SHOW_POSITION: u16 = B + 2;
    pub const MACHINE_STATUS: u16 = B + 3;
    pub const MACHINE_MESSAGE: u16 = B + 4;
    pub const MACHINE_PROGRESS: u16 = B + 5;
    pub const MACHINE_JOB: u16 = B + 6;
    /// The machine panel's buttons in view-model order (Upload excluded by presentation): `+0..+7`.
    pub const MACHINE_BUTTON: u16 = B + 10;
    pub const MACHINE_BUTTONS: u16 = 8;
    pub const COMPILE: u16 = B + 18;
    pub const EXPORT_RD: u16 = B + 19;
    pub const PLAN_STATE: u16 = B + 20;
    pub const PLAN_FINDINGS: u16 = B + 21;
    pub const SOFT_STOP: u16 = B + 22;
    pub const STOP_NOTE: u16 = B + 23;
    pub const JOG_HEADING: u16 = B + 29;
    pub const JOG_LEFT: u16 = B + 30;
    pub const JOG_RIGHT: u16 = B + 31;
    pub const JOG_UP: u16 = B + 32;
    pub const JOG_DOWN: u16 = B + 33;
    pub const STEP_LEFT: u16 = B + 34;
    pub const STEP_RIGHT: u16 = B + 35;
    pub const STEP_UP: u16 = B + 36;
    pub const STEP_DOWN: u16 = B + 37;
    pub const STEP_SIZE: u16 = B + 38;
    pub const JOG_NOTE: u16 = B + 39;
    pub const MOVE_X: u16 = B + 40;
    pub const MOVE_Y: u16 = B + 41;
    pub const MOVE_GO: u16 = B + 42;
    pub const POSITION_LASER: u16 = B + 43;
    pub const HOME: u16 = B + 44;
    pub const GO_MACHINE_ORIGIN: u16 = B + 45;
    pub const GO_USER_ORIGIN: u16 = B + 46;
    pub const SET_ORIGIN: u16 = B + 47;
    pub const CLEAR_ORIGIN: u16 = B + 48;
    pub const USER_ORIGIN: u16 = B + 49;
    pub const START_FROM: u16 = B + 50;
    /// The 3×3 job origin, row by row from the top left: `+0..+8`.
    pub const ANCHOR: u16 = B + 51;
    pub const CUT_SELECTED: u16 = B + 60;
    pub const USE_SELECTION_ORIGIN: u16 = B + 61;
    pub const PLACEMENT: u16 = B + 62;
    pub const FRAME_RECT: u16 = B + 63;
    pub const FRAME_RUBBER: u16 = B + 64;
    pub const FRAME_SPEED: u16 = B + 65;
    pub const FRAME_NOTE: u16 = B + 66;
    pub const SAVE_RD_NAME: u16 = B + 67;
    pub const SAVE_RD: u16 = B + 68;
    pub const FILES_HEADING: u16 = B + 69;
    pub const FILES: u16 = B + 70;
    pub const LIST_FILES: u16 = B + 71;
    pub const FILE_INFO: u16 = B + 72;
    pub const FILE_RUN: u16 = B + 73;
    pub const FILE_DELETE: u16 = B + 74;
    pub const DIAG_HEADING: u16 = B + 75;
    pub const DIAG_PATHS: u16 = B + 76;
    pub const DIAG_IDS: u16 = B + 77;
    pub const DIAGNOSE: u16 = B + 78;
    pub const DIAG_NOTE: u16 = B + 79;
    /// Captions (labels) in front of edits and groups: `+80..`.
    pub const CAPTION_STEP: u16 = B + 80;
    pub const CAPTION_MOVE: u16 = B + 81;
    pub const START_HEADING: u16 = B + 82;
    pub const ANCHOR_HEADING: u16 = B + 83;
    pub const CAPTION_SPEED: u16 = B + 84;
    pub const CAPTION_RD_NAME: u16 = B + 85;
}

/// What the laser window is built from.
pub struct LaserInput<'a> {
    pub document: DocumentId,
    pub vm: &'a AppViewModel,
    pub laser: Option<&'a LaserViewModel>,
    pub machine: Option<&'a MachineViewModel>,
    pub locale: Locale,
    /// The row selected in the controller file list (host state), if any.
    pub file_row: Option<usize>,
}

fn t(text: LaserText, locale: Locale) -> &'static str {
    laser_text(text, locale)
}

/// Whether the control of `kind` is on, and the reason in words when it is not.
fn state_of(model: &LaserPanel, kind: &LaserControlKind) -> (bool, String) {
    model.controls.iter().find(|c| &c.kind == kind).map_or((false, String::new()), |c| (c.enabled, c.tooltip.clone()))
}

fn action_button(model: &LaserPanel, id: u16, text: &str, kind: &LaserControlKind) -> ControlSpec {
    let (enabled, why) = state_of(model, kind);
    ControlSpec::button(id, text, enabled).tip(why)
}

const JOGS: [(u16, JogKey, LaserText); 4] = [
    (id::JOG_LEFT, JogKey::XLeft, LaserText::JogLeft),
    (id::JOG_RIGHT, JogKey::XRight, LaserText::JogRight),
    (id::JOG_UP, JogKey::YTop, LaserText::JogUp),
    (id::JOG_DOWN, JogKey::YBottom, LaserText::JogDown),
];

/// Step buttons: control id, index in the shared `simple_controls`, caption.
const STEPS: [(u16, usize, LaserText); 4] = [
    (id::STEP_LEFT, 7, LaserText::JogLeft),
    (id::STEP_RIGHT, 8, LaserText::JogRight),
    (id::STEP_UP, 9, LaserText::JogUp),
    (id::STEP_DOWN, 10, LaserText::JogDown),
];

/// The one-shot buttons and their index in the shared `simple_controls`.
const SIMPLE: [(u16, usize); 7] = [
    (id::HOME, 0),
    (id::GO_MACHINE_ORIGIN, 1),
    (id::GO_USER_ORIGIN, 2),
    (id::SHOW_POSITION, 5),
    (id::LIST_FILES, 6),
    (id::FRAME_RECT, 11),
    (id::FRAME_RUBBER, 12),
];

fn diagnostics_rows(locale: Locale) -> Vec<Row> {
    vec![
        Row::one(ControlSpec::heading(id::DIAG_HEADING, t(LaserText::DiagnosticsTitle, locale))),
        Row::one(ControlSpec::check(id::DIAG_PATHS, t(LaserText::DiagnosticsPaths, locale), false, true)),
        Row::one(ControlSpec::check(id::DIAG_IDS, t(LaserText::DiagnosticsIds, locale), false, true)),
        Row::one(ControlSpec::label(id::DIAG_NOTE, t(LaserText::DiagnosticsMessage, locale))),
        Row::one(ControlSpec::button(id::DIAGNOSE, t(LaserText::DiagnosticsItem, locale), true)),
    ]
}

pub fn laser_spec(input: &LaserInput<'_>) -> PanelSpec {
    let locale = input.locale;
    let model = laser_panel(input.laser, locale);
    let title = t(LaserText::PanelTitle, locale).to_owned();
    if !model.connected {
        let mut rows = vec![Row::one(ControlSpec::label(id::STATUS, model.status.clone()))];
        rows.extend(diagnostics_rows(locale));
        return PanelSpec { title, rows };
    }
    let mut rows = vec![
        Row::one(ControlSpec::label(id::STATUS, model.status.clone())),
        Row::of(vec![
            ControlSpec::label(id::POSITION, model.position.clone()).weighted(3),
            action_button(&model, id::SHOW_POSITION, t(LaserText::ShowPosition, locale), &LaserControlKind::ShowPosition),
        ]),
    ];

    // Machine and job: status in words, the controller's own buttons, compile and export.
    let machine = machine_panel(input.machine, locale);
    rows.push(Row::one(ControlSpec::label(id::MACHINE_STATUS, machine.status.clone())));
    rows.push(Row::one(ControlSpec::label(id::MACHINE_MESSAGE, machine.message.clone())));
    rows.push(Row::one(ControlSpec::label(id::MACHINE_PROGRESS, machine.progress.clone())));
    rows.push(Row::one(ControlSpec::label(id::MACHINE_JOB, machine.job.clone())));
    let buttons: Vec<ControlSpec> = (0..id::MACHINE_BUTTONS)
        .map(|i| {
            let shown = machine.buttons.get(usize::from(i));
            ControlSpec::button(
                id::MACHINE_BUTTON + i,
                shown.map_or_else(String::new, |(label, _)| label.clone()),
                shown.is_some_and(|(_, on)| *on),
            )
            .hidden_if(shown.is_none())
        })
        .collect();
    rows.push(Row::of(buttons[..4].to_vec()));
    rows.push(Row::of(buttons[4..].to_vec()));
    let plan = plan_panel(input.vm, input.document, locale);
    rows.push(Row::one(ControlSpec::label(id::PLAN_STATE, format!("{} — {}", plan.title, plan.state))));
    rows.push(Row::one(ControlSpec::label(id::PLAN_FINDINGS, format!("{}  {}", plan.estimate, plan.findings))));
    let command_on = |key: CommandKey| input.vm.commands.get(&key).is_some_and(|c| c.enabled);
    let soft_stop_on = input.machine.and_then(|m| m.button(MachineAction::Cancel)).is_some_and(|b| b.enabled);
    rows.push(Row::of(vec![
        ControlSpec::button(id::COMPILE, host_text(HostText::CompileButton, locale), command_on(CommandKey::Compile)),
        ControlSpec::button(id::EXPORT_RD, host_text(HostText::ExportButton, locale), command_on(CommandKey::ExportRd)),
        ControlSpec::button(id::SOFT_STOP, t(LaserText::SoftStop, locale), soft_stop_on),
    ]));
    rows.push(Row::one(ControlSpec::label(id::STOP_NOTE, t(LaserText::StopNote, locale))));

    // Jog (hold), steps and move.
    rows.push(Row::one(ControlSpec::heading(id::JOG_HEADING, t(LaserText::Jog, locale))));
    rows.push(Row::of(
        JOGS.iter()
            .map(|(cid, key, text)| {
                let (enabled, why) = state_of(&model, &LaserControlKind::Jog(*key));
                ControlSpec::hold_button(*cid, t(*text, locale), enabled).tip(why)
            })
            .collect(),
    ));
    rows.push(Row::one(ControlSpec::label(
        id::JOG_NOTE,
        if model.jog_held { t(LaserText::JogHeld, locale) } else { "" },
    )));
    let controls = simple_controls();
    rows.push(Row::of(
        STEPS
            .iter()
            .map(|(cid, tag, text)| {
                let kind = controls.get(*tag).map(|(k, _)| k.clone());
                let (enabled, why) = kind.as_ref().map_or((false, String::new()), |k| state_of(&model, k));
                ControlSpec::button(*cid, format!("{} {}", t(LaserText::Step, locale), t(*text, locale)), enabled)
                    .tip(why)
            })
            .collect(),
    ));
    rows.push(Row::of(vec![
        ControlSpec::label(id::CAPTION_STEP, t(LaserText::StepSize, locale)).weighted(2),
        ControlSpec::edit(id::STEP_SIZE, model.step_mm.clone(), true),
    ]));
    let (move_on, move_why) = state_of(&model, &LaserControlKind::MoveTo);
    rows.push(Row::of(vec![
        ControlSpec::label(id::CAPTION_MOVE, t(LaserText::MoveTo, locale)).weighted(2),
        ControlSpec::edit(id::MOVE_X, "", move_on),
        ControlSpec::edit(id::MOVE_Y, "", move_on),
        ControlSpec::button(id::MOVE_GO, t(LaserText::MoveToButton, locale), move_on).tip(move_why).weighted(2),
    ]));
    let (pl_on, pl_why) = state_of(&model, &LaserControlKind::PositionLaser);
    rows.push(Row::one(
        ControlSpec::check(
            id::POSITION_LASER,
            t(LaserText::PositionLaser, locale),
            model.position_laser_mode,
            pl_on || model.position_laser_mode,
        )
        .tip(pl_why),
    ));

    // Home and origins.
    rows.push(Row::of(vec![
        action_button(&model, id::HOME, t(LaserText::Home, locale), &LaserControlKind::Home),
        action_button(
            &model,
            id::GO_MACHINE_ORIGIN,
            t(LaserText::GoMachineOrigin, locale),
            &LaserControlKind::GoToMachineOrigin,
        ),
        action_button(
            &model,
            id::GO_USER_ORIGIN,
            t(LaserText::GoUserOrigin, locale),
            &LaserControlKind::GoToUserOrigin,
        ),
    ]));
    rows.push(Row::of(vec![
        action_button(&model, id::SET_ORIGIN, t(LaserText::SetOrigin, locale), &LaserControlKind::SetOrigin),
        ControlSpec::button(id::CLEAR_ORIGIN, t(LaserText::ClearOrigin, locale), true),
        ControlSpec::label(id::USER_ORIGIN, model.user_origin.clone()).weighted(2),
    ]));

    // Start from and the 3x3 job origin.
    rows.push(Row::one(ControlSpec::heading(id::START_HEADING, t(LaserText::StartFrom, locale))));
    rows.push(Row::one(ControlSpec::combo(
        id::START_FROM,
        vec![
            t(LaserText::StartAbsolute, locale).to_owned(),
            t(LaserText::StartCurrent, locale).to_owned(),
            t(LaserText::StartUser, locale).to_owned(),
        ],
        usize::try_from(segment_for_start_from(model.start_from)).ok(),
        true,
    )));
    rows.push(Row::one(ControlSpec::heading(id::ANCHOR_HEADING, t(LaserText::JobOrigin, locale))));
    for line in 0..3u16 {
        rows.push(Row::of(
            (0..3u16)
                .map(|col| {
                    let n = line * 3 + col;
                    let anchor = anchor_for_tag(isize::from(n as i16));
                    let mark = if anchor == Some(model.anchor) { "●" } else { "○" };
                    let name = anchor.map_or("", |a| t(anchor_text(a), locale));
                    ControlSpec::button(id::ANCHOR + n, format!("{mark} {name}"), true)
                })
                .collect(),
        ));
    }
    rows.push(Row::of(vec![
        ControlSpec::check(id::CUT_SELECTED, t(LaserText::CutSelected, locale), model.cut_selected, true),
        ControlSpec::check(
            id::USE_SELECTION_ORIGIN,
            t(LaserText::UseSelectionOrigin, locale),
            model.use_selection_origin,
            true,
        ),
    ]));
    rows.push(Row::one(ControlSpec::label(id::PLACEMENT, model.placement.clone())));

    // Frame.
    let frame_kinds = [
        (id::FRAME_RECT, LaserText::FrameRectangle, FrameShape::BoundingRectangle),
        (id::FRAME_RUBBER, LaserText::FrameRubberBand, FrameShape::RubberBand),
    ];
    let mut frame_row: Vec<ControlSpec> = frame_kinds
        .iter()
        .map(|(cid, text, shape)| action_button(&model, *cid, t(*text, locale), &LaserControlKind::Frame(*shape)))
        .collect();
    frame_row.push(ControlSpec::label(id::CAPTION_SPEED, t(LaserText::FrameSpeed, locale)).weighted(2));
    frame_row.push(ControlSpec::edit(id::FRAME_SPEED, model.frame_speed.clone(), true));
    rows.push(Row::of(frame_row));
    rows.push(Row::one(ControlSpec::label(id::FRAME_NOTE, t(LaserText::FrameNote, locale))));

    // Save RD to the controller.
    let (rd_on, rd_why) = state_of(&model, &LaserControlKind::SaveRd);
    rows.push(Row::of(vec![
        ControlSpec::label(id::CAPTION_RD_NAME, t(LaserText::SaveRdName, locale)).weighted(2),
        ControlSpec::edit(id::SAVE_RD_NAME, "", true).weighted(2),
        ControlSpec::button(id::SAVE_RD, t(LaserText::SaveRd, locale), rd_on).tip(rd_why).weighted(2),
    ]));

    // Files on the controller.
    rows.push(Row::one(ControlSpec::heading(id::FILES_HEADING, t(LaserText::Files, locale))));
    let items: Vec<String> = model.files.iter().map(|(name, size)| format!("{name} — {size}")).collect();
    let selected = input.file_row.filter(|r| *r < items.len());
    rows.push(Row::one(ControlSpec::list(id::FILES, 5, items, selected, true)));
    let file_state = |which: usize| -> (bool, String) {
        let name = selected.and_then(|r| input.laser?.files.as_ref()?.get(r)).map(|f| f.name.clone());
        let kind = name.map(|n| match which {
            0 => LaserControlKind::FileMetadata(n),
            1 => LaserControlKind::RunFile(n),
            _ => LaserControlKind::DeleteFile(n),
        });
        kind.as_ref().map_or((false, String::new()), |k| state_of(&model, k))
    };
    let (info, info_why) = file_state(0);
    let (run, run_why) = file_state(1);
    let (del, del_why) = file_state(2);
    rows.push(Row::of(vec![
        action_button(&model, id::LIST_FILES, t(LaserText::ListFiles, locale), &LaserControlKind::ListFiles),
        ControlSpec::button(id::FILE_INFO, t(LaserText::FileInfo, locale), info).tip(info_why),
        ControlSpec::button(id::FILE_RUN, t(LaserText::FileRun, locale), run).tip(run_why),
        ControlSpec::button(id::FILE_DELETE, t(LaserText::FileDelete, locale), del).tip(del_why),
    ]));

    rows.extend(diagnostics_rows(locale));
    PanelSpec { title, rows }
}

fn diagnostic_options(event: &PanelEvent) -> DiagnosticOptions {
    DiagnosticOptions {
        include_project_paths: event.values.checked(id::DIAG_PATHS),
        include_machine_ids: event.values.checked(id::DIAG_IDS),
    }
}

fn intent(i: Option<UiIntent>) -> PanelReaction {
    i.map_or(PanelReaction::Nothing, PanelReaction::Intent)
}

pub fn on_laser_event(input: &LaserInput<'_>, event: &PanelEvent) -> PanelReaction {
    let document = input.document;
    let locale = input.locale;
    // The head must stop whatever happens to the window: this needs no live view model.
    if matches!(event.code, notify::RELEASE | notify::DEACTIVATED | notify::CLOSED) {
        return PanelReaction::Intent(UiIntent::LaserReleaseJog);
    }
    if input.vm.document(document).is_none() {
        return PanelReaction::Nothing;
    }
    if event.id == id::DIAGNOSE && event.code == notify::CLICKED {
        return PanelReaction::Dialog(DialogRequest::Save {
            purpose: FilePurpose::Diagnostics { options: diagnostic_options(event) },
            title: t(LaserText::DiagnosticsTitle, locale).to_owned(),
            suggested_name: "burnburn-diagnose.txt".to_owned(),
            filters: vec![FileFilter { name: "Tekst / Text".to_owned(), extensions: vec!["txt"] }],
        });
    }
    let Some(vm) = input.laser else { return PanelReaction::Nothing };
    let model = laser_panel(Some(vm), locale);
    let setup = |s: LaserSetup| PanelReaction::Intent(setup_intent(s));
    let text = |cid: u16| event.values.text(cid);
    let (cid, code) = (event.id, event.code);

    if code == notify::PRESS {
        let key = JOGS.iter().find(|(j, ..)| *j == cid).map(|(_, k, _)| *k);
        return intent(key.and_then(|k| jog_press_intent(document, vm, k)));
    }
    if matches!(code, notify::ENTER | notify::EDIT_DONE) && (cid == id::STEP_SIZE || cid == id::FRAME_SPEED) {
        // Return and the following focus loss are one commit: empty text or the text the window
        // already shows sends nothing.
        let typed = text(cid);
        let shown = if cid == id::STEP_SIZE { &model.step_mm } else { &model.frame_speed };
        return if typed.trim().is_empty() || typed == shown {
            PanelReaction::Nothing
        } else if cid == id::STEP_SIZE {
            setup(LaserSetup::StepMm(typed.to_owned()))
        } else {
            setup(LaserSetup::FrameSpeed(typed.to_owned()))
        };
    }
    if cid == id::START_FROM && code == notify::CHANGED {
        return match start_from_for_segment(event.values.selected(id::START_FROM)) {
            Some(start) => setup(LaserSetup::StartFrom(start)),
            None => PanelReaction::Nothing,
        };
    }
    if code != notify::CLICKED {
        return PanelReaction::Nothing;
    }
    match cid {
        id::MOVE_GO => intent(move_intent(document, vm, text(id::MOVE_X), text(id::MOVE_Y))),
        id::SAVE_RD => intent(save_rd_intent(document, vm, text(id::SAVE_RD_NAME))),
        id::SOFT_STOP => {
            let cancel = input.machine.and_then(|m| m.button(MachineAction::Cancel));
            intent(cancel.and_then(|b| machine_intent(Some(document), b)))
        }
        id::COMPILE | id::EXPORT_RD => {
            let command = if cid == id::COMPILE { CommandKey::Compile } else { CommandKey::ExportRd };
            if input.vm.commands.get(&command).is_some_and(|c| c.enabled) {
                PanelReaction::Intent(UiIntent::Invoke { document: Some(document), command })
            } else {
                PanelReaction::Nothing
            }
        }
        id::SET_ORIGIN if state_of(&model, &LaserControlKind::SetOrigin).0 => setup(LaserSetup::SetOriginHere),
        id::CLEAR_ORIGIN => setup(LaserSetup::ClearOrigin),
        id::CUT_SELECTED => setup(LaserSetup::CutSelected(event.values.checked(cid))),
        id::USE_SELECTION_ORIGIN => setup(LaserSetup::UseSelectionOrigin(event.values.checked(cid))),
        id::POSITION_LASER => setup(LaserSetup::PositionLaserMode(event.values.checked(cid))),
        id::FILE_INFO | id::FILE_RUN | id::FILE_DELETE => {
            let Some(row) = event.values.selection.get(&id::FILES).copied().flatten() else {
                return PanelReaction::Nothing;
            };
            let which = match cid {
                id::FILE_INFO => 0,
                id::FILE_RUN => 1,
                _ => 2,
            };
            match file_intent(document, vm, row, which) {
                // Delete asks first: the file is on the controller, not in any project.
                Some(i) if which == 2 => PanelReaction::Dialog(DialogRequest::Confirm {
                    purpose: ConfirmPurpose::DeleteControllerFile { intent: Box::new(i) },
                    title: t(LaserText::DeleteTitle, locale).to_owned(),
                    body: t(LaserText::DeleteBody, locale).to_owned(),
                    confirm: t(LaserText::DeleteConfirm, locale).to_owned(),
                    cancel: t(LaserText::Cancel, locale).to_owned(),
                }),
                other => intent(other),
            }
        }
        _ => {
            if (id::MACHINE_BUTTON..id::MACHINE_BUTTON + id::MACHINE_BUTTONS).contains(&cid) {
                let button = input.machine.and_then(|m| m.buttons.get(usize::from(cid - id::MACHINE_BUTTON)));
                return intent(button.and_then(|b| machine_intent(Some(document), b)));
            }
            if (id::ANCHOR..id::ANCHOR + 9).contains(&cid) {
                return match anchor_for_tag(isize::from((cid - id::ANCHOR) as i16)) {
                    Some(anchor) => setup(LaserSetup::Anchor(anchor)),
                    None => PanelReaction::Nothing,
                };
            }
            let tag = SIMPLE
                .iter()
                .find(|(s, _)| *s == cid)
                .map(|(_, tag)| *tag)
                .or_else(|| STEPS.iter().find(|(s, ..)| *s == cid).map(|(_, tag, _)| *tag));
            intent(tag.and_then(|tag| simple_intent(document, vm, tag as isize)))
        }
    }
}

/// A file dialog of this family closed.
pub fn on_laser_file_chosen(purpose: &FilePurpose, path: Chosen) -> PanelReaction {
    match (purpose, path) {
        (FilePurpose::Diagnostics { options }, Some(path)) => {
            PanelReaction::Intent(UiIntent::ExportDiagnostics { options: *options, path })
        }
        _ => PanelReaction::Nothing,
    }
}

/// The arrow key (`VK_LEFT` …) as the jog control it presses while the laser window has the keyboard.
/// The mapping to machine keys is the registry's naming (X_LEFT, X_RIGHT, Y_TOP, Y_BOTTOM); which
/// way the head really moves per key is part of the missing hardware evidence for `Jog`.
pub fn jog_control_for_vk(vk: u16) -> Option<u16> {
    match vk {
        0x25 => Some(id::JOG_LEFT),
        0x27 => Some(id::JOG_RIGHT),
        0x26 => Some(id::JOG_UP),
        0x28 => Some(id::JOG_DOWN),
        _ => None,
    }
}
