//! The camera window on Windows (W4-L8b): the bed-camera section (access, start, stop), the overlay
//! on the bed (switch, opacity, update, material height, edge margin), the cameras in the bed
//! system, the stored calibrations, the lens-calibration wizard and the workspace-alignment wizard
//! with its photo, in which the marks are clicked.
//!
//! Texts, enablement and intents come from the models shared with the Mac host
//! (`crate::panels::camera*`). Nothing here opens a camera: the real camera source on Windows
//! (Media Foundation) is not built yet, see `camera_port`.

use burnburn_presentation::{CameraViewModel, CameraWorkspaceView, UiIntent};

use super::ids::CAMERA_CONTROLS;
use super::spec::notify;
use super::{ControlSpec, PanelEvent, PanelReaction, PanelSpec, Row};
use crate::l10n::{HostText, Locale, host_text};
use crate::panels::{
    CameraCommand, CameraControl, CameraInputs, CameraText, CameraWindowPanel, camera_command_intent, camera_intent,
    camera_panel, camera_text, camera_window_panel, height_intent, margin_intent, next_unplaced_mark, opacity_intent,
    overlay_visible_intent, photo_click_intent,
};
use crate::panels_image::BitmapImage;

/// Height of the alignment photo, in pixels at 96 dpi.
const PHOTO_PX: i32 = 300;

/// Control ids of the camera window (`7600..7800`).
pub mod id {
    use super::CAMERA_CONTROLS as B;

    // Bed-camera section.
    pub const BED_HEADING: u16 = B;
    pub const BED_STATUS: u16 = B + 1;
    pub const BED_DEVICES: u16 = B + 2;
    pub const BED_FRAME: u16 = B + 3;
    pub const BED_NOTE: u16 = B + 4;
    pub const BED_ACCESS: u16 = B + 5;
    pub const BED_START: u16 = B + 6;
    pub const BED_STOP: u16 = B + 7;

    // Overlay.
    pub const OVERLAY_HEADING: u16 = B + 10;
    pub const OVERLAY_VISIBLE: u16 = B + 11;
    pub const OPACITY_CAPTION: u16 = B + 12;
    pub const OPACITY: u16 = B + 13;
    pub const UPDATE_OVERLAY: u16 = B + 14;
    pub const HEIGHT_CAPTION: u16 = B + 15;
    pub const HEIGHT: u16 = B + 16;
    pub const MARGIN_CAPTION: u16 = B + 17;
    pub const MARGIN: u16 = B + 18;
    pub const OVERLAY_STATUS: u16 = B + 19;
    pub const NOTE: u16 = B + 20;

    // Cameras.
    pub const CAMERAS_HEADING: u16 = B + 24;
    pub const DEVICE: u16 = B + 25;
    pub const LENS_PROFILE_CAPTION: u16 = B + 26;
    pub const LENS_PROFILE: u16 = B + 27;
    pub const USE_CAMERA: u16 = B + 28;
    pub const RUNNING: u16 = B + 29;
    pub const DROP_CAMERA: u16 = B + 30;

    // Calibrations.
    pub const CAL_HEADING: u16 = B + 34;
    pub const CALIBRATIONS: u16 = B + 35;
    pub const DELETE_CAL: u16 = B + 36;
    pub const RECALIBRATE: u16 = B + 37;

    // Lens wizard.
    pub const LENS_HEADING: u16 = B + 40;
    pub const COLS_CAPTION: u16 = B + 41;
    pub const COLS: u16 = B + 42;
    pub const ROWS_CAPTION: u16 = B + 43;
    pub const ROWS: u16 = B + 44;
    pub const PITCH_CAPTION: u16 = B + 45;
    pub const PITCH: u16 = B + 46;
    pub const START_LENS: u16 = B + 47;
    pub const ADD_PHOTO: u16 = B + 48;
    pub const FIT_LENS: u16 = B + 49;
    pub const ACCEPT_LENS: u16 = B + 50;
    pub const CANCEL_LENS: u16 = B + 51;
    pub const LENS_STATUS: u16 = B + 52;

    // Alignment wizard.
    pub const ALIGN_HEADING: u16 = B + 60;
    pub const ALIGN_HEIGHT_CAPTION: u16 = B + 61;
    pub const ALIGN_HEIGHT: u16 = B + 62;
    pub const START_ALIGN: u16 = B + 63;
    pub const MARK: u16 = B + 64;
    pub const MARK_X_CAPTION: u16 = B + 65;
    pub const MARK_X: u16 = B + 66;
    pub const MARK_Y_CAPTION: u16 = B + 67;
    pub const MARK_Y: u16 = B + 68;
    pub const SET_MARK: u16 = B + 69;
    pub const POWER_CAPTION: u16 = B + 70;
    pub const POWER: u16 = B + 71;
    pub const SPEED_CAPTION: u16 = B + 72;
    pub const SPEED: u16 = B + 73;
    pub const PROPOSE: u16 = B + 74;
    pub const SEND_MARKS: u16 = B + 75;
    pub const TAKE_PHOTO: u16 = B + 76;
    pub const FIT_ALIGN: u16 = B + 77;
    pub const SAVE_ALIGN: u16 = B + 78;
    pub const CANCEL_ALIGN: u16 = B + 79;
    pub const ALIGN_STATUS: u16 = B + 80;
    pub const MARKS_STATUS: u16 = B + 81;
    pub const PHOTO: u16 = B + 82;
    pub const REFUSAL: u16 = B + 83;
}

/// What the camera window is built from.
pub struct CameraInput<'a> {
    pub locale: Locale,
    pub workspace: Option<&'a CameraWorkspaceView>,
    /// The strip's camera model (devices, permission, the running camera).
    pub primary: Option<&'a CameraViewModel>,
    /// The alignment photo with the clicked marks drawn into it (host state), if any.
    pub photo: Option<BitmapImage>,
    /// The mark the list shows chosen (host state: it moves on after a click in the photo).
    pub mark: Option<usize>,
}

const COMMANDS: [(u16, CameraCommand); 18] = [
    (id::UPDATE_OVERLAY, CameraCommand::UpdateOverlay),
    (id::USE_CAMERA, CameraCommand::UseCamera),
    (id::DROP_CAMERA, CameraCommand::DropCamera),
    (id::DELETE_CAL, CameraCommand::DeleteCalibration),
    (id::RECALIBRATE, CameraCommand::Recalibrate),
    (id::START_LENS, CameraCommand::StartLens),
    (id::ADD_PHOTO, CameraCommand::AddPhoto),
    (id::FIT_LENS, CameraCommand::FitLens),
    (id::ACCEPT_LENS, CameraCommand::AcceptLens),
    (id::CANCEL_LENS, CameraCommand::CancelLens),
    (id::START_ALIGN, CameraCommand::StartAlignment),
    (id::SET_MARK, CameraCommand::SetMark),
    (id::PROPOSE, CameraCommand::ProposeMarks),
    (id::SEND_MARKS, CameraCommand::SendMarks),
    (id::TAKE_PHOTO, CameraCommand::TakePhoto),
    (id::FIT_ALIGN, CameraCommand::FitAlignment),
    (id::SAVE_ALIGN, CameraCommand::SaveAlignment),
    (id::CANCEL_ALIGN, CameraCommand::CancelAlignment),
];

fn ct(text: CameraText, locale: Locale) -> &'static str {
    camera_text(text, locale)
}

fn caption(cid: u16, text: &str) -> ControlSpec {
    ControlSpec::label(cid, text).weighted(2)
}

fn labeled_edit(caption_id: u16, caption_text: &str, edit_id: u16, text: &str) -> Row {
    Row::of(vec![caption(caption_id, caption_text), ControlSpec::edit(edit_id, text, true)])
}

fn command_button(w: &CameraWindowPanel, cid: u16, command: CameraCommand, locale: Locale) -> ControlSpec {
    ControlSpec::button(cid, command.title(locale), w.is_enabled(command))
}

pub fn camera_spec(input: &CameraInput<'_>) -> PanelSpec {
    let locale = input.locale;
    let bed = camera_panel(input.primary, locale);
    let w = camera_window_panel(input.workspace, input.primary, locale);
    let rows = vec![
        // The bed-camera section.
        Row::one(ControlSpec::heading(id::BED_HEADING, host_text(HostText::CameraTitle, locale))),
        Row::one(ControlSpec::label(id::BED_STATUS, bed.status.clone())),
        Row::one(ControlSpec::combo(id::BED_DEVICES, bed.devices.clone(), bed.selected, !bed.devices.is_empty())),
        Row::one(ControlSpec::label(id::BED_FRAME, bed.frame.clone())),
        Row::one(ControlSpec::label(id::BED_NOTE, bed.note.clone())),
        Row::of(vec![
            ControlSpec::button(id::BED_ACCESS, host_text(HostText::CameraAccessButton, locale), bed.access_enabled),
            ControlSpec::button(id::BED_START, host_text(HostText::CameraStartButton, locale), bed.start_enabled),
            ControlSpec::button(id::BED_STOP, host_text(HostText::CameraStopButton, locale), bed.stop_enabled),
        ]),
        // The picture on the bed.
        Row::one(ControlSpec::heading(id::OVERLAY_HEADING, ct(CameraText::SectionOverlay, locale))),
        Row::of(vec![
            ControlSpec::check(id::OVERLAY_VISIBLE, ct(CameraText::ShowOverlay, locale), w.overlay_visible, true),
            command_button(&w, id::UPDATE_OVERLAY, CameraCommand::UpdateOverlay, locale),
        ]),
        Row::of(vec![
            caption(id::OPACITY_CAPTION, ct(CameraText::Opacity, locale)),
            ControlSpec::slider(id::OPACITY, 0, 100, w.opacity_percent.round().clamp(0.0, 100.0) as i32, true)
                .weighted(2),
        ]),
        labeled_edit(id::HEIGHT_CAPTION, ct(CameraText::MaterialHeight, locale), id::HEIGHT, &w.height_text),
        labeled_edit(id::MARGIN_CAPTION, ct(CameraText::EdgeMargin, locale), id::MARGIN, &w.margin_text),
        Row::one(ControlSpec::report(id::OVERLAY_STATUS, 3, w.overlay_status.clone())),
        Row::one(ControlSpec::label(id::NOTE, w.note.clone())),
        // The cameras.
        Row::one(ControlSpec::heading(id::CAMERAS_HEADING, ct(CameraText::SectionCameras, locale))),
        Row::one(ControlSpec::combo(
            id::DEVICE,
            w.devices.clone(),
            (!w.devices.is_empty()).then_some(0),
            !w.devices.is_empty(),
        )),
        labeled_edit(id::LENS_PROFILE_CAPTION, ct(CameraText::LensProfile, locale), id::LENS_PROFILE, ""),
        Row::one(command_button(&w, id::USE_CAMERA, CameraCommand::UseCamera, locale)),
        Row::one(ControlSpec::combo(
            id::RUNNING,
            w.running.clone(),
            (!w.running.is_empty()).then_some(0),
            !w.running.is_empty(),
        )),
        Row::one(command_button(&w, id::DROP_CAMERA, CameraCommand::DropCamera, locale)),
        // Stored calibrations.
        Row::one(ControlSpec::heading(id::CAL_HEADING, ct(CameraText::SectionCalibrations, locale))),
        Row::one(ControlSpec::list(
            id::CALIBRATIONS,
            4,
            if w.calibrations.is_empty() {
                vec![ct(CameraText::NoCalibrations, locale).to_owned()]
            } else {
                w.calibrations.clone()
            },
            (!w.calibrations.is_empty()).then_some(0),
            !w.calibrations.is_empty(),
        )),
        Row::of(vec![
            command_button(&w, id::DELETE_CAL, CameraCommand::DeleteCalibration, locale),
            command_button(&w, id::RECALIBRATE, CameraCommand::Recalibrate, locale),
        ]),
        // Lens wizard.
        Row::one(ControlSpec::heading(id::LENS_HEADING, ct(CameraText::SectionLens, locale))),
        labeled_edit(id::COLS_CAPTION, ct(CameraText::GridColumns, locale), id::COLS, "9"),
        labeled_edit(id::ROWS_CAPTION, ct(CameraText::GridRows, locale), id::ROWS, "6"),
        labeled_edit(id::PITCH_CAPTION, ct(CameraText::GridPitch, locale), id::PITCH, "25"),
        Row::of(vec![
            command_button(&w, id::START_LENS, CameraCommand::StartLens, locale),
            command_button(&w, id::ADD_PHOTO, CameraCommand::AddPhoto, locale),
        ]),
        Row::of(vec![
            command_button(&w, id::FIT_LENS, CameraCommand::FitLens, locale),
            command_button(&w, id::ACCEPT_LENS, CameraCommand::AcceptLens, locale),
            command_button(&w, id::CANCEL_LENS, CameraCommand::CancelLens, locale),
        ]),
        Row::one(ControlSpec::label(id::LENS_STATUS, w.lens_status.clone())),
        // Alignment wizard.
        Row::one(ControlSpec::heading(id::ALIGN_HEADING, ct(CameraText::SectionAlignment, locale))),
        labeled_edit(id::ALIGN_HEIGHT_CAPTION, ct(CameraText::AlignHeight, locale), id::ALIGN_HEIGHT, "0"),
        Row::one(command_button(&w, id::START_ALIGN, CameraCommand::StartAlignment, locale)),
        Row::one(ControlSpec::combo(
            id::MARK,
            w.marks.clone(),
            (!w.marks.is_empty()).then(|| input.mark.filter(|m| *m < w.marks.len()).unwrap_or(0)),
            !w.marks.is_empty(),
        )),
        Row::of(vec![
            caption(id::MARK_X_CAPTION, ct(CameraText::MarkX, locale)),
            ControlSpec::edit(id::MARK_X, "", true),
            caption(id::MARK_Y_CAPTION, ct(CameraText::MarkY, locale)),
            ControlSpec::edit(id::MARK_Y, "", true),
            command_button(&w, id::SET_MARK, CameraCommand::SetMark, locale),
        ]),
        Row::of(vec![
            caption(id::POWER_CAPTION, ct(CameraText::Power, locale)),
            ControlSpec::edit(id::POWER, "5", true),
            caption(id::SPEED_CAPTION, ct(CameraText::Speed, locale)),
            ControlSpec::edit(id::SPEED, "50", true),
        ]),
        Row::of(vec![
            command_button(&w, id::PROPOSE, CameraCommand::ProposeMarks, locale),
            command_button(&w, id::SEND_MARKS, CameraCommand::SendMarks, locale),
        ]),
        Row::one(ControlSpec::bitmap(id::PHOTO, PHOTO_PX, input.photo.clone())),
        Row::one(ControlSpec::label(id::MARKS_STATUS, w.marks_status.clone())),
        Row::of(vec![
            command_button(&w, id::TAKE_PHOTO, CameraCommand::TakePhoto, locale),
            command_button(&w, id::FIT_ALIGN, CameraCommand::FitAlignment, locale),
        ]),
        Row::of(vec![
            command_button(&w, id::SAVE_ALIGN, CameraCommand::SaveAlignment, locale),
            command_button(&w, id::CANCEL_ALIGN, CameraCommand::CancelAlignment, locale),
        ]),
        Row::one(ControlSpec::label(id::ALIGN_STATUS, w.alignment_status.clone())),
        Row::one(ControlSpec::label(id::REFUSAL, w.refusal.clone())),
    ];
    PanelSpec { title: ct(CameraText::WindowTitle, locale).to_owned(), rows }
}

/// What the user typed and chose, as the shared model reads it.
fn inputs_of(event: &PanelEvent) -> CameraInputs {
    let v = &event.values;
    CameraInputs {
        device: v.selected(id::DEVICE),
        running: v.selected(id::RUNNING),
        calibration: v.selected(id::CALIBRATIONS),
        mark: v.selected(id::MARK),
        lens_profile: v.text(id::LENS_PROFILE).to_owned(),
        cols: v.text(id::COLS).to_owned(),
        rows: v.text(id::ROWS).to_owned(),
        pitch: v.text(id::PITCH).to_owned(),
        align_height: v.text(id::ALIGN_HEIGHT).to_owned(),
        power: v.text(id::POWER).to_owned(),
        speed: v.text(id::SPEED).to_owned(),
        mark_x: v.text(id::MARK_X).to_owned(),
        mark_y: v.text(id::MARK_Y).to_owned(),
    }
}

fn intent(i: Option<UiIntent>) -> PanelReaction {
    i.map_or(PanelReaction::Nothing, PanelReaction::Intent)
}

pub fn on_camera_event(input: &CameraInput<'_>, event: &PanelEvent) -> PanelReaction {
    let (cid, code) = (event.id, event.code);
    let v = &event.values;
    let clicked = code == notify::CLICKED;
    match cid {
        id::BED_ACCESS | id::BED_START | id::BED_STOP if clicked => {
            let control = match cid {
                id::BED_ACCESS => CameraControl::RequestAccess,
                id::BED_START => CameraControl::Start,
                _ => CameraControl::Stop,
            };
            intent(camera_intent(input.primary, control, v.selected(id::BED_DEVICES)))
        }
        id::OVERLAY_VISIBLE if clicked => PanelReaction::Intent(overlay_visible_intent(v.checked(id::OVERLAY_VISIBLE))),
        id::OPACITY if code == notify::SLIDER => {
            let pos = v.slider(id::OPACITY).unwrap_or(0).clamp(0, 100);
            PanelReaction::Intent(opacity_intent(f64::from(pos)))
        }
        id::HEIGHT | id::MARGIN if matches!(code, notify::ENTER | notify::EDIT_DONE) => {
            // Return and the following focus loss are one commit: empty text or the text the
            // window already shows sends nothing.
            let shown = camera_window_panel(input.workspace, input.primary, input.locale);
            let (typed, current) =
                if cid == id::HEIGHT { (v.text(cid), &shown.height_text) } else { (v.text(cid), &shown.margin_text) };
            if typed.trim().is_empty() || typed == current {
                PanelReaction::Nothing
            } else if cid == id::HEIGHT {
                PanelReaction::Intent(height_intent(typed))
            } else {
                PanelReaction::Intent(margin_intent(typed))
            }
        }
        id::PHOTO if code == notify::BITMAP_CLICK => {
            let (Some(click), Some(view)) = (event.click_px, event.view_px) else { return PanelReaction::Nothing };
            // Win32 measures from the top, the photo model from the bottom.
            intent(photo_click_intent(
                input.workspace,
                v.selected(id::MARK),
                (view[0], view[1]),
                (click[0], view[1] - click[1]),
            ))
        }
        _ if clicked => {
            let command = COMMANDS.iter().find(|(c, _)| *c == cid).map(|(_, c)| *c);
            intent(command.and_then(|c| camera_command_intent(c, input.workspace, input.primary, &inputs_of(event))))
        }
        _ => PanelReaction::Nothing,
    }
}

/// After a mark was placed by a click: the next mark that still has no position (the click-through flow).
pub fn next_mark_after(input: &CameraInput<'_>, mark: isize) -> Option<usize> {
    let alignment = input.workspace?.alignment.as_ref()?;
    next_unplaced_mark(alignment, usize::try_from(mark).ok()?)
}
