//! The laser window as a [`FormSpec`] (LFE): sections with a heading, the unit as a suffix, the
//! reason a section is greyed said once, the jog cross and the 3x3 job origin as pads. The Mac
//! host builds the form with `build_form`; this module decides what it says and which control
//! is which, and nothing else. The intents stay in `laser.rs` (`simple_intent`, `jog_press_intent`, …).

use burnburn_presentation::{JobAnchor, JogKey, LaserControlKind, StepDirection};

use crate::form::{Control, FormId, FormRow, FormSection, FormSpec, ItemState, PadCell, PadGlyph, split_caption};
use crate::l10n::Locale;
use crate::style::StatusTone;

use super::laser::{LaserPanel, LaserText, PanelControl, anchor_text, laser_text, segment_for_start_from};

/// The ids of the rows (a pad or a row of buttons answers `id`, `id + 1`, …).
pub mod laser_id {
    pub const POSITION_X: u32 = 1;
    pub const POSITION_Y: u32 = 2;
    pub const STATUS: u32 = 3;
    pub const SHOW_POSITION: u32 = 4;
    /// Pause, resume, software stop.
    pub const RUN_CONTROL: u32 = 5;
    /// The jog cross: nine cells, the four buttons on the cross.
    pub const JOG_PAD: u32 = 10;
    /// The step buttons: left, right, up, down.
    pub const STEP_PAD: u32 = 30;
    /// The 3x3 job origin, row by row from the top left.
    pub const ANCHOR_PAD: u32 = 40;
    pub const STEP_SIZE: u32 = 101;
    pub const MOVE_X: u32 = 102;
    pub const MOVE_Y: u32 = 103;
    pub const MOVE_TO: u32 = 104;
    pub const POSITION_LASER: u32 = 105;
    pub const POINTER_OFFSET: u32 = 106;
    /// Home, go to machine origin, go to user origin.
    pub const GO_HOME: u32 = 110;
    /// Set the origin here, clear the origin.
    pub const ORIGIN: u32 = 113;
    pub const USER_ORIGIN: u32 = 115;
    pub const START_FROM: u32 = 120;
    pub const CUT_SELECTED: u32 = 121;
    pub const SELECTION_ORIGIN: u32 = 122;
    pub const PLACEMENT: u32 = 123;
    /// Frame (rectangle), frame (rubber band).
    pub const FRAME: u32 = 130;
    pub const FRAME_SPEED: u32 = 132;
    pub const FRAME_NOTE: u32 = 133;
    pub const RD_NAME: u32 = 150;
    pub const SAVE_RD: u32 = 151;
    pub const FILE_CHOICE: u32 = 152;
    /// Refresh list, info, run, delete.
    pub const FILE_BUTTONS: u32 = 153;
    pub const DIAGNOSTICS: u32 = 160;
}

/// The three check boxes and the red-dot switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaserCheck {
    CutSelected,
    UseSelectionOrigin,
    PositionLaser,
    PointerOffset,
}

/// What a control of the form stands for; the host turns it into the intent the old panel sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaserFormAction {
    /// A button with a fixed command: its index in `simple_controls()`.
    Simple(isize),
    /// A jog button (press and hold).
    Jog(JogKey),
    StepSize,
    FrameSpeed,
    MoveX,
    MoveY,
    MoveTo,
    SetOrigin,
    ClearOrigin,
    SoftStop,
    RdName,
    SaveRd,
    Check(LaserCheck),
    StartFrom,
    /// A cell of the 3x3 job origin (the tag `anchor_for_tag` takes).
    Anchor(usize),
    FileChoice,
    /// Info, run or delete of the chosen file: 0, 1, 2.
    File(usize),
    Diagnostics,
}

/// The action a form id stands for, `None` for a row the user only reads or a gap in a pad.
pub fn laser_form_action(id: FormId) -> Option<LaserFormAction> {
    use LaserFormAction as A;
    use laser_id as i;
    let n = id.0;
    let within = |base: u32, len: u32| (n >= base && n < base + len).then(|| (n - base) as usize);
    if let Some(cell) = within(i::JOG_PAD, 9) {
        return match cell {
            1 => Some(A::Jog(JogKey::YTop)),
            3 => Some(A::Jog(JogKey::XLeft)),
            5 => Some(A::Jog(JogKey::XRight)),
            7 => Some(A::Jog(JogKey::YBottom)),
            _ => None,
        };
    }
    if let Some(cell) = within(i::STEP_PAD, 4) {
        return Some(A::Simple(STEP_TAGS[cell]));
    }
    if let Some(cell) = within(i::ANCHOR_PAD, 9) {
        return Some(A::Anchor(cell));
    }
    if let Some(button) = within(i::GO_HOME, 3) {
        return Some(A::Simple(button as isize));
    }
    if let Some(button) = within(i::RUN_CONTROL, 3) {
        return Some(match button {
            0 => A::Simple(PAUSE_TAG),
            1 => A::Simple(PAUSE_TAG + 1),
            _ => A::SoftStop,
        });
    }
    if let Some(button) = within(i::FRAME, 2) {
        return Some(A::Simple(FRAME_TAG + button as isize));
    }
    if let Some(button) = within(i::FILE_BUTTONS, 4) {
        return Some(if button == 0 { A::Simple(LIST_FILES_TAG) } else { A::File(button - 1) });
    }
    if let Some(button) = within(i::ORIGIN, 2) {
        return Some(if button == 0 { A::SetOrigin } else { A::ClearOrigin });
    }
    Some(match n {
        i::SHOW_POSITION => A::Simple(SHOW_POSITION_TAG),
        i::STEP_SIZE => A::StepSize,
        i::MOVE_X => A::MoveX,
        i::MOVE_Y => A::MoveY,
        i::MOVE_TO => A::MoveTo,
        i::POSITION_LASER => A::Check(LaserCheck::PositionLaser),
        i::POINTER_OFFSET => A::Check(LaserCheck::PointerOffset),
        i::START_FROM => A::StartFrom,
        i::CUT_SELECTED => A::Check(LaserCheck::CutSelected),
        i::SELECTION_ORIGIN => A::Check(LaserCheck::UseSelectionOrigin),
        i::FRAME_SPEED => A::FrameSpeed,
        i::RD_NAME => A::RdName,
        i::SAVE_RD => A::SaveRd,
        i::FILE_CHOICE => A::FileChoice,
        i::DIAGNOSTICS => A::Diagnostics,
        _ => return None,
    })
}

// Indices in `simple_controls()`; the test `the_simple_tags_are_the_ones_of_simple_controls` pins them.
const PAUSE_TAG: isize = 3;
const SHOW_POSITION_TAG: isize = 5;
const LIST_FILES_TAG: isize = 6;
const STEP_TAGS: [isize; 4] = [7, 8, 9, 10];
const FRAME_TAG: isize = 11;

/// The text of a jog caption without the arrow glyph the old panel put on the button (the host
/// draws a symbol on it instead).
fn plain(text: &str) -> String {
    text.chars().filter(|c| !"◀▶▲▼".contains(*c)).collect::<String>().trim().to_owned()
}

fn find<'a>(panel: &'a LaserPanel, kind: &LaserControlKind) -> Option<&'a PanelControl> {
    panel.controls.iter().find(|c| &c.kind == kind)
}

/// A button of the window: on when the controller evidence says so, with the reason in words if not.
fn item(panel: &LaserPanel, kind: &LaserControlKind) -> ItemState {
    match find(panel, kind) {
        Some(c) => ItemState::new(c.enabled, &c.tooltip),
        None => ItemState::new(false, ""),
    }
}

/// A button that needs a connected machine and nothing more.
fn connected_item(panel: &LaserPanel) -> ItemState {
    ItemState::new(panel.connected, "")
}

/// The reason a section is greyed, said once: the first reason one of its controls gives (a missing
/// machine is said by the status line, for the whole window). The controls keep their own tooltip with their own words.
fn reason(panel: &LaserPanel, kinds: &[LaserControlKind]) -> String {
    if !panel.connected {
        // The status line says "no machine" once for the whole window.
        return String::new();
    }
    kinds
        .iter()
        .filter_map(|k| find(panel, k))
        .find(|c| !c.enabled && !c.tooltip.is_empty())
        .map(|c| c.tooltip.clone())
        .unwrap_or_default()
}

fn pad_cell(panel: &LaserPanel, kind: &LaserControlKind, title: &str, label: &str, glyph: PadGlyph) -> PadCell {
    let item = item(panel, kind);
    let mut cell = PadCell::new(title, label, Some(glyph)).enabled(item.enabled);
    cell.tooltip = item.tooltip;
    cell
}

/// The laser window for `panel`. `selected_file` is the row of the file the user chose in the list
/// (clamped to the files there are).
pub fn laser_form(panel: &LaserPanel, selected_file: usize, locale: Locale) -> FormSpec {
    use laser_id as i;
    let t = |text: LaserText| laser_text(text, locale);
    let on = panel.connected;
    let unit_row = |id: u32, caption: &str, value: &str| {
        let split = split_caption(caption);
        let mut row = FormRow::new(id, &split.label, Control::Number { value: value.to_owned() });
        row.unit = split.unit;
        row.enabled = on;
        row
    };
    let check = |id: u32, text: LaserText, checked: bool, state: ItemState| {
        let mut row = FormRow::new(id, t(text), Control::Check { on: checked });
        row.enabled = state.enabled;
        row.tooltip = state.tooltip;
        row
    };
    let button = |id: u32, text: LaserText, state: ItemState| {
        let mut row = FormRow::new(id, "", Control::Action { title: t(text).to_owned() });
        row.enabled = state.enabled;
        row.tooltip = state.tooltip;
        row
    };
    let buttons = |id: u32, titles: &[LaserText], items: Vec<ItemState>| {
        let mut row =
            FormRow::new(id, "", Control::Actions { titles: titles.iter().map(|x| t(*x).to_owned()).collect() })
                .items(items);
        row.enabled = on;
        row
    };
    let axis = |id: u32, name: &str, reading: &super::laser::AxisReading| {
        let mut row = FormRow::new(id, name, Control::ReadOnly { value: reading.text.clone() });
        if reading.known {
            row.unit = Some(crate::format::Quantity::Millimeters);
        }
        row
    };

    // Machine: where the head is, what was refused, pause, resume and the software stop.
    let status = FormRow::new(
        i::STATUS,
        "",
        Control::Note { value: panel.status.clone(), lines: 2, tone: StatusTone::Error, strong: false },
    );
    let machine_kinds = [LaserControlKind::ShowPosition, LaserControlKind::Pause, LaserControlKind::Resume];
    let mut machine = FormSection::new(
        Some(t(LaserText::SectionMachine)),
        vec![
            axis(i::POSITION_X, "X", &panel.position_x),
            axis(i::POSITION_Y, "Y", &panel.position_y),
            status,
            button(i::SHOW_POSITION, LaserText::ShowPosition, item(panel, &LaserControlKind::ShowPosition)),
            buttons(
                i::RUN_CONTROL,
                &[LaserText::Pause, LaserText::Resume, LaserText::SoftStop],
                vec![
                    item(panel, &LaserControlKind::Pause),
                    item(panel, &LaserControlKind::Resume),
                    connected_item(panel),
                ],
            )
            .help(t(LaserText::StopNote)),
        ],
    );
    machine = machine.reason(&reason(panel, &machine_kinds));

    // Position and jog.
    let jog_caption = split_caption(t(LaserText::Jog));
    let jog_help =
        if panel.jog_held { t(LaserText::JogHeld).to_owned() } else { jog_caption.hint.clone().unwrap_or_default() };
    let jog_cell = |key: JogKey, text: LaserText, glyph: PadGlyph| {
        let title = plain(t(text));
        pad_cell(panel, &LaserControlKind::Jog(key), &title, &title, glyph)
    };
    let jog = FormRow::new(
        i::JOG_PAD,
        &jog_caption.label,
        Control::Pad {
            columns: 3,
            cells: vec![
                PadCell::gap(),
                jog_cell(JogKey::YTop, LaserText::JogUp, PadGlyph::Up),
                PadCell::gap(),
                jog_cell(JogKey::XLeft, LaserText::JogLeft, PadGlyph::Left),
                PadCell::gap(),
                jog_cell(JogKey::XRight, LaserText::JogRight, PadGlyph::Right),
                PadCell::gap(),
                jog_cell(JogKey::YBottom, LaserText::JogDown, PadGlyph::Down),
                PadCell::gap(),
            ],
            hold: true,
        },
    )
    .help(&jog_help);
    let step_cell = |dir: StepDirection, text: LaserText, glyph: PadGlyph| {
        let label = format!("{} {}", t(LaserText::Step), plain(t(text)));
        pad_cell(panel, &LaserControlKind::Step(dir), "", &label, glyph)
    };
    let step = FormRow::new(
        i::STEP_PAD,
        t(LaserText::Step),
        Control::Pad {
            columns: 4,
            cells: vec![
                step_cell(StepDirection::Left, LaserText::JogLeft, PadGlyph::Left),
                step_cell(StepDirection::Right, LaserText::JogRight, PadGlyph::Right),
                step_cell(StepDirection::Up, LaserText::JogUp, PadGlyph::Up),
                step_cell(StepDirection::Down, LaserText::JogDown, PadGlyph::Down),
            ],
            hold: false,
        },
    );
    let move_caption = split_caption(t(LaserText::MoveTo));
    let mut move_x = unit_row(i::MOVE_X, t(LaserText::MoveTo), "");
    move_x.label = format!("{} X", move_caption.label);
    let mut move_y = unit_row(i::MOVE_Y, t(LaserText::MoveTo), "");
    move_y.label = format!("{} Y", move_caption.label);
    let position_laser = find(panel, &LaserControlKind::PositionLaser);
    let position_state = ItemState {
        enabled: position_laser.is_some_and(|c| c.enabled) || panel.position_laser_mode,
        tooltip: position_laser.map(|c| c.tooltip.clone()).filter(|x| !x.is_empty()),
    };
    let mut pointer = check(
        i::POINTER_OFFSET,
        LaserText::PointerOffsetUse,
        panel.use_pointer_offset,
        ItemState::new(on && panel.can_use_pointer_offset, ""),
    );
    pointer.help = Some(panel.pointer_offset.clone());
    let move_kinds: Vec<LaserControlKind> = [JogKey::XLeft, JogKey::XRight, JogKey::YTop, JogKey::YBottom]
        .map(LaserControlKind::Jog)
        .into_iter()
        .chain([LaserControlKind::MoveTo, LaserControlKind::PositionLaser])
        .collect();
    let motion = FormSection::new(
        Some(t(LaserText::SectionMove)),
        vec![
            jog,
            step,
            unit_row(i::STEP_SIZE, t(LaserText::StepSize), &panel.step_mm),
            move_x,
            move_y,
            button(i::MOVE_TO, LaserText::MoveToButton, item(panel, &LaserControlKind::MoveTo)),
            check(i::POSITION_LASER, LaserText::PositionLaser, panel.position_laser_mode, position_state),
            pointer,
        ],
    )
    .reason(&reason(panel, &move_kinds));

    // Origin.
    let origin_kinds = [
        LaserControlKind::Home,
        LaserControlKind::GoToMachineOrigin,
        LaserControlKind::GoToUserOrigin,
        LaserControlKind::SetOrigin,
    ];
    let origin = FormSection::new(
        Some(t(LaserText::SectionOrigin)),
        vec![
            buttons(
                i::GO_HOME,
                &[LaserText::Home, LaserText::GoMachineOrigin, LaserText::GoUserOrigin],
                vec![
                    item(panel, &LaserControlKind::Home),
                    item(panel, &LaserControlKind::GoToMachineOrigin),
                    item(panel, &LaserControlKind::GoToUserOrigin),
                ],
            ),
            buttons(
                i::ORIGIN,
                &[LaserText::SetOrigin, LaserText::ClearOrigin],
                vec![item(panel, &LaserControlKind::SetOrigin), connected_item(panel)],
            ),
            FormRow::new(
                i::USER_ORIGIN,
                t(LaserText::UserOrigin),
                Control::ReadOnly { value: panel.user_origin.clone() },
            ),
        ],
    )
    .reason(&reason(panel, &origin_kinds));

    // Start from and the job origin: window setup, nothing is sent to the machine.
    let anchor_caption = split_caption(t(LaserText::JobOrigin));
    let anchors: Vec<PadCell> = JobAnchor::ALL
        .iter()
        .map(|a| {
            let name = t(anchor_text(*a));
            let chosen = *a == panel.anchor;
            PadCell::new("", name, Some(if chosen { PadGlyph::On } else { PadGlyph::Off }))
                .enabled(on)
                .on(chosen)
                .tooltip(name)
        })
        .collect();
    let mut start_from = FormRow::new(
        i::START_FROM,
        t(LaserText::StartFrom),
        Control::Choice {
            options: [LaserText::StartAbsolute, LaserText::StartCurrent, LaserText::StartUser]
                .map(|x| t(x).to_owned())
                .to_vec(),
            selected: usize::try_from(segment_for_start_from(panel.start_from)).unwrap_or(0),
        },
    );
    start_from.enabled = on;
    let start = FormSection::new(
        Some(t(LaserText::SectionStart)),
        vec![
            start_from,
            FormRow::new(
                i::ANCHOR_PAD,
                &anchor_caption.label,
                Control::Pad { columns: 3, cells: anchors, hold: false },
            )
            .help(anchor_caption.hint.as_deref().unwrap_or("")),
            check(i::CUT_SELECTED, LaserText::CutSelected, panel.cut_selected, connected_item(panel)),
            check(
                i::SELECTION_ORIGIN,
                LaserText::UseSelectionOrigin,
                panel.use_selection_origin,
                connected_item(panel),
            ),
            FormRow::new(i::PLACEMENT, "", Control::ReadOnly { value: panel.placement.clone() }),
        ],
    )
    .reason(&reason(panel, &[]));

    // Frame.
    let frame_kinds = [
        LaserControlKind::Frame(burnburn_presentation::FrameShape::BoundingRectangle),
        LaserControlKind::Frame(burnburn_presentation::FrameShape::RubberBand),
    ];
    let frame = FormSection::new(
        Some(t(LaserText::SectionFrame)),
        vec![
            buttons(
                i::FRAME,
                &[LaserText::FrameRectangle, LaserText::FrameRubberBand],
                frame_kinds.iter().map(|k| item(panel, k)).collect(),
            ),
            unit_row(i::FRAME_SPEED, t(LaserText::FrameSpeed), &panel.frame_speed),
            FormRow::new(i::FRAME_NOTE, "", Control::ReadOnly { value: t(LaserText::FrameNote).to_owned() }),
        ],
    )
    .reason(&reason(panel, &frame_kinds));

    // Files on the controller.
    let chosen = selected_file.min(panel.files.len().saturating_sub(1));
    let options: Vec<String> = if panel.files.is_empty() {
        vec![t(LaserText::NoFiles).to_owned()]
    } else {
        panel.files.iter().map(|(name, size)| format!("{name} ({size})")).collect()
    };
    let file_kind = |which: usize| -> Option<LaserControlKind> {
        let name = panel.files.get(chosen).map(|(n, _)| n.as_str())?;
        panel
            .controls
            .iter()
            .map(|c| &c.kind)
            .find(|k| match (which, k) {
                (0, LaserControlKind::FileMetadata(n))
                | (1, LaserControlKind::RunFile(n))
                | (2, LaserControlKind::DeleteFile(n)) => n.as_str() == name,
                _ => false,
            })
            .cloned()
    };
    let file_item = |which: usize| file_kind(which).map_or_else(|| ItemState::new(false, ""), |k| item(panel, &k));
    let mut file_choice =
        FormRow::new(i::FILE_CHOICE, t(LaserText::FileChoice), Control::Choice { options, selected: chosen });
    file_choice.enabled = on && !panel.files.is_empty();
    let mut rd_name = FormRow::new(
        i::RD_NAME,
        t(LaserText::SaveRdName),
        Control::Text { value: String::new(), placeholder: String::new() },
    );
    rd_name.enabled = on;
    let mut file_reason_kinds = vec![LaserControlKind::SaveRd, LaserControlKind::ListFiles];
    file_reason_kinds.extend((0..3).filter_map(file_kind));
    let files = FormSection::new(
        Some(t(LaserText::Files)),
        vec![
            rd_name,
            button(i::SAVE_RD, LaserText::SaveRd, item(panel, &LaserControlKind::SaveRd)),
            file_choice,
            buttons(
                i::FILE_BUTTONS,
                &[LaserText::ListFiles, LaserText::FileInfo, LaserText::FileRun, LaserText::FileDelete],
                vec![item(panel, &LaserControlKind::ListFiles), file_item(0), file_item(1), file_item(2)],
            ),
        ],
    )
    .reason(&reason(panel, &file_reason_kinds));

    // Diagnostics replay the journal; they need no machine.
    let mut diagnostics_row = button(i::DIAGNOSTICS, LaserText::DiagnosticsItem, ItemState::new(true, ""));
    diagnostics_row.help = Some(t(LaserText::DiagnosticsMessage).to_owned());
    let diagnostics = FormSection::new(Some(t(LaserText::SectionDiagnostics)), vec![diagnostics_row]).reason("");

    // A sentence two sections would share (nothing is proven on the controller: every control gives
    // the same words) is said once in the window, in the first of them; the others keep their room.
    let mut sections = vec![machine, motion, origin, start, frame, files, diagnostics];
    let mut said: Vec<String> = Vec::new();
    for section in &mut sections {
        match section.reason.clone().filter(|r| !r.is_empty()) {
            Some(r) if said.contains(&r) => section.reason = Some(String::new()),
            Some(r) => said.push(r),
            None => {}
        }
    }
    FormSpec { title: t(LaserText::PanelTitle).to_owned(), sections, ..FormSpec::default() }
}

#[cfg(test)]
mod tests {
    use super::super::laser::simple_controls;
    use super::*;

    #[test]
    fn the_simple_tags_are_the_ones_of_simple_controls() {
        let kinds: Vec<LaserControlKind> = simple_controls().into_iter().map(|(k, _)| k).collect();
        assert_eq!(kinds[PAUSE_TAG as usize], LaserControlKind::Pause);
        assert_eq!(kinds[PAUSE_TAG as usize + 1], LaserControlKind::Resume);
        assert_eq!(kinds[SHOW_POSITION_TAG as usize], LaserControlKind::ShowPosition);
        assert_eq!(kinds[LIST_FILES_TAG as usize], LaserControlKind::ListFiles);
        for (tag, dir) in
            STEP_TAGS.iter().zip([StepDirection::Left, StepDirection::Right, StepDirection::Up, StepDirection::Down])
        {
            assert_eq!(kinds[*tag as usize], LaserControlKind::Step(dir));
        }
        use burnburn_presentation::FrameShape;
        assert_eq!(kinds[FRAME_TAG as usize], LaserControlKind::Frame(FrameShape::BoundingRectangle));
        assert_eq!(kinds[FRAME_TAG as usize + 1], LaserControlKind::Frame(FrameShape::RubberBand));
        assert_eq!(kinds[0], LaserControlKind::Home);
        assert_eq!(kinds[1], LaserControlKind::GoToMachineOrigin);
        assert_eq!(kinds[2], LaserControlKind::GoToUserOrigin);
    }
}
