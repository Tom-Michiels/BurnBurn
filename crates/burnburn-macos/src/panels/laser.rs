//! The laser window (W2-L1-laser) as plain data: what the panel shows from the presentation's
//! `LaserViewModel` and which `UiIntent` each control becomes. No AppKit here, so all of it is
//! tested headlessly; the widgets in `appkit/panels/laser.rs` only copy this model.
//!
//! The panel shows and forwards: the position reads "onbekend" until the controller answered (never
//! 0), a control without wire evidence is off with the typed reason in words as its tooltip, and
//! numbers go to presentation as typed (DR-05).

use burnburn_presentation::{
    ControllerFileName, FrameShape, HomeTarget, JobAnchor, JogKey, LaserAction, LaserCommand, LaserControlKind,
    LaserSetup, LaserViewModel, OriginTarget, StartFrom, StepDirection, UiIntent, format_mm,
};
use burnburn_types::DocumentId;

use crate::l10n::{Locale, laser_why_text};

/// Texts of the laser panel (host-only; the refusal sentences are in `l10n::laser_why_text`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LaserText {
    PanelTitle,
    MenuLaser,
    MenuWindowItem,
    NoMachine,
    Position,
    Unknown,
    ShowPosition,
    Jog,
    JogLeft,
    JogRight,
    JogUp,
    JogDown,
    Step,
    StepSize,
    MoveTo,
    MoveToButton,
    PositionLaser,
    Home,
    GoMachineOrigin,
    GoUserOrigin,
    SetOrigin,
    ClearOrigin,
    UserOrigin,
    UserOriginNotSet,
    Pause,
    Resume,
    SoftStop,
    StopNote,
    StartFrom,
    StartAbsolute,
    StartCurrent,
    StartUser,
    JobOrigin,
    AnchorTopLeft,
    AnchorTopCenter,
    AnchorTopRight,
    AnchorMiddleLeft,
    AnchorCenter,
    AnchorMiddleRight,
    AnchorBottomLeft,
    AnchorBottomCenter,
    AnchorBottomRight,
    CutSelected,
    UseSelectionOrigin,
    FrameRectangle,
    FrameRubberBand,
    FrameSpeed,
    FrameNote,
    SaveRd,
    SaveRdName,
    Files,
    ListFiles,
    FileInfo,
    FileRun,
    FileDelete,
    FileName,
    FileSize,
    SizeUnknown,
    DeleteTitle,
    DeleteBody,
    DeleteConfirm,
    Cancel,
    PlacementShift,
    PlacementOnHead,
    PlacementStale,
    JogHeld,
    HelpMenu,
    DiagnosticsItem,
    DiagnosticsTitle,
    DiagnosticsPaths,
    DiagnosticsIds,
    DiagnosticsMessage,
    PointerOffsetUse,
    PointerOffsetNone,
    PlacementWithDot,
    PointerOffsetLabel,
}

impl LaserText {
    pub const ALL: &'static [LaserText] = &[
        LaserText::PanelTitle,
        LaserText::MenuLaser,
        LaserText::MenuWindowItem,
        LaserText::NoMachine,
        LaserText::Position,
        LaserText::Unknown,
        LaserText::ShowPosition,
        LaserText::Jog,
        LaserText::JogLeft,
        LaserText::JogRight,
        LaserText::JogUp,
        LaserText::JogDown,
        LaserText::Step,
        LaserText::StepSize,
        LaserText::MoveTo,
        LaserText::MoveToButton,
        LaserText::PositionLaser,
        LaserText::Home,
        LaserText::GoMachineOrigin,
        LaserText::GoUserOrigin,
        LaserText::SetOrigin,
        LaserText::ClearOrigin,
        LaserText::UserOrigin,
        LaserText::UserOriginNotSet,
        LaserText::Pause,
        LaserText::Resume,
        LaserText::SoftStop,
        LaserText::StopNote,
        LaserText::StartFrom,
        LaserText::StartAbsolute,
        LaserText::StartCurrent,
        LaserText::StartUser,
        LaserText::JobOrigin,
        LaserText::AnchorTopLeft,
        LaserText::AnchorTopCenter,
        LaserText::AnchorTopRight,
        LaserText::AnchorMiddleLeft,
        LaserText::AnchorCenter,
        LaserText::AnchorMiddleRight,
        LaserText::AnchorBottomLeft,
        LaserText::AnchorBottomCenter,
        LaserText::AnchorBottomRight,
        LaserText::CutSelected,
        LaserText::UseSelectionOrigin,
        LaserText::FrameRectangle,
        LaserText::FrameRubberBand,
        LaserText::FrameSpeed,
        LaserText::FrameNote,
        LaserText::SaveRd,
        LaserText::SaveRdName,
        LaserText::Files,
        LaserText::ListFiles,
        LaserText::FileInfo,
        LaserText::FileRun,
        LaserText::FileDelete,
        LaserText::FileName,
        LaserText::FileSize,
        LaserText::SizeUnknown,
        LaserText::DeleteTitle,
        LaserText::DeleteBody,
        LaserText::DeleteConfirm,
        LaserText::Cancel,
        LaserText::PlacementShift,
        LaserText::PlacementOnHead,
        LaserText::PlacementStale,
        LaserText::JogHeld,
        LaserText::HelpMenu,
        LaserText::DiagnosticsItem,
        LaserText::DiagnosticsTitle,
        LaserText::DiagnosticsPaths,
        LaserText::DiagnosticsIds,
        LaserText::DiagnosticsMessage,
        LaserText::PointerOffsetUse,
        LaserText::PointerOffsetNone,
        LaserText::PlacementWithDot,
        LaserText::PointerOffsetLabel,
    ];
}

fn pair(text: LaserText) -> (&'static str, &'static str) {
    use LaserText as T;
    match text {
        T::PanelTitle => ("Laservenster", "Laser"),
        T::MenuLaser => ("Laser", "Laser"),
        T::MenuWindowItem => ("Laservenster tonen", "Show Laser Window"),
        T::NoMachine => ("Geen machine verbonden.", "No machine connected."),
        T::Position => ("Positie", "Position"),
        T::Unknown => ("onbekend", "unknown"),
        T::ShowPosition => ("Positie uitlezen", "Show Last Position"),
        T::Jog => ("Jog (ingedrukt houden)", "Jog (hold)"),
        T::JogLeft => ("◀ X−", "◀ X−"),
        T::JogRight => ("X+ ▶", "X+ ▶"),
        T::JogUp => ("▲ Y+", "▲ Y+"),
        T::JogDown => ("▼ Y−", "▼ Y−"),
        T::Step => ("Stap", "Step"),
        T::StepSize => ("Stapgrootte (mm)", "Step size (mm)"),
        T::MoveTo => ("Ga naar (mm)", "Move to (mm)"),
        T::MoveToButton => ("Ga naar", "Move"),
        T::PositionLaser => ("Laser positioneren (klik op het canvas)", "Position Laser (click on the canvas)"),
        T::Home => ("Home", "Home"),
        T::GoMachineOrigin => ("Naar machine-oorsprong", "Go to Origin"),
        T::GoUserOrigin => ("Naar gebruikersoorsprong", "Go to User Origin"),
        T::SetOrigin => ("Oorsprong hier instellen", "Set Origin Here"),
        T::ClearOrigin => ("Wis oorsprong", "Clear Origin"),
        T::UserOrigin => ("Gebruikersoorsprong", "User origin"),
        T::UserOriginNotSet => ("niet ingesteld", "not set"),
        T::Pause => ("Pauze", "Pause"),
        T::Resume => ("Hervat", "Resume"),
        T::SoftStop => ("Software-stop", "Software Stop"),
        T::StopNote => (
            "De hardware-noodstop blijft leidend; een software-stop bevestigt niet dat de kop stilstaat.",
            "The hardware emergency stop stays authoritative; a software stop does not confirm that the head stopped.",
        ),
        T::StartFrom => ("Start vanaf", "Start From"),
        T::StartAbsolute => ("Absolute coördinaten", "Absolute Coordinates"),
        T::StartCurrent => ("Huidige positie", "Current Position"),
        T::StartUser => ("Gebruikersoorsprong", "User Origin"),
        T::JobOrigin => ("Job-oorsprong (punt op de job)", "Job Origin (point on the job)"),
        T::AnchorTopLeft => ("Linksboven", "Top left"),
        T::AnchorTopCenter => ("Boven midden", "Top center"),
        T::AnchorTopRight => ("Rechtsboven", "Top right"),
        T::AnchorMiddleLeft => ("Links midden", "Middle left"),
        T::AnchorCenter => ("Midden", "Center"),
        T::AnchorMiddleRight => ("Rechts midden", "Middle right"),
        T::AnchorBottomLeft => ("Linksonder", "Bottom left"),
        T::AnchorBottomCenter => ("Onder midden", "Bottom center"),
        T::AnchorBottomRight => ("Rechtsonder", "Bottom right"),
        T::CutSelected => ("Alleen selectie snijden", "Cut Selected Graphics"),
        T::UseSelectionOrigin => ("Oorsprong van de selectie gebruiken", "Use Selection Origin"),
        T::FrameRectangle => ("Frame (rechthoek)", "Frame (rectangle)"),
        T::FrameRubberBand => ("Frame (rubber band)", "Frame (rubber band)"),
        T::FrameSpeed => ("Framesnelheid (mm/s)", "Frame speed (mm/s)"),
        T::FrameNote => (
            "Een frame is een laser-uit job: geüpload, niet geverifieerd tot de controller dat bevestigt. Starten is een aparte stap.",
            "A frame is a laser-off job: uploaded, not verified until the controller confirms it. Starting it is a separate step.",
        ),
        T::SaveRd => ("RD bewaren op controller", "Save RD to Controller"),
        T::SaveRdName => ("Naam op de controller", "Name on the controller"),
        T::Files => ("Bestanden op de controller", "Files on the controller"),
        T::ListFiles => ("Lijst vernieuwen", "Refresh List"),
        T::FileInfo => ("Informatie", "Info"),
        T::FileRun => ("Uitvoeren", "Run"),
        T::FileDelete => ("Verwijderen", "Delete"),
        T::FileName => ("Naam", "Name"),
        T::FileSize => ("Grootte", "Size"),
        T::SizeUnknown => ("onbekend", "unknown"),
        T::DeleteTitle => ("Bestand op de controller verwijderen?", "Delete the file on the controller?"),
        T::DeleteBody => (
            "Dit bestand staat op de controller, niet in je projecten. Verwijderen kan niet ongedaan gemaakt worden.",
            "This file is on the controller, not in your projects. Deleting cannot be undone.",
        ),
        T::DeleteConfirm => ("Verwijder", "Delete"),
        T::Cancel => ("Annuleer", "Cancel"),
        T::PlacementShift => ("Job verschoven met", "Job moved by"),
        T::PlacementOnHead => ("naar de kop op", "to the head at"),
        T::PlacementStale => (
            "Instelling of positie gewijzigd sinds het compileren: compileer opnieuw.",
            "Setup or position changed since compiling: compile again.",
        ),
        T::JogHeld => ("Jog actief: loslaten stopt de kop.", "Jogging: releasing stops the head."),
        T::HelpMenu => ("Help", "Help"),
        T::DiagnosticsItem => ("Diagnose exporteren…", "Export Diagnostics…"),
        T::DiagnosticsTitle => ("Diagnose exporteren", "Export Diagnostics"),
        T::DiagnosticsPaths => ("Projectpaden opnemen", "Include project paths"),
        T::DiagnosticsIds => ("Machine-ID's opnemen", "Include machine IDs"),
        T::DiagnosticsMessage => (
            "Het rapport is een replay van het journaal; er is geen contact met de machine. Privé-gegevens staan er alleen in als je ze aanvinkt.",
            "The report is a replay of the journal; there is no contact with the machine. Private data is included only if you tick it.",
        ),
        T::PointerOffsetUse => ("Met de rode stip richten", "Aim with the red dot"),
        T::PointerOffsetNone => (
            "Geen pointeroffset gemeten voor deze machine; de kop wordt gebruikt.",
            "No pointer offset measured for this machine; the head is used.",
        ),
        T::PlacementWithDot => ("rode stip", "red dot"),
        T::PointerOffsetLabel => ("Rode stip", "Red dot"),
    }
}

pub fn laser_text(text: LaserText, locale: Locale) -> &'static str {
    let (nl, en) = pair(text);
    match locale {
        Locale::Nl => nl,
        Locale::En => en,
    }
}

/// Millimetres in the locale's decimal sign (the fields accept both, DR-05).
fn mm_text(mm: f64, locale: Locale) -> String {
    let text = format_mm(mm);
    match locale {
        Locale::Nl => text.replace('.', ","),
        Locale::En => text,
    }
}

/// One control as the panel shows it: the caption, whether it acts, and in words why not.
#[derive(Debug, Clone, PartialEq)]
pub struct PanelControl {
    pub kind: LaserControlKind,
    pub enabled: bool,
    /// The reason in words when the control is off; empty when it is on.
    pub tooltip: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LaserPanel {
    pub connected: bool,
    /// "X: 120,5 mm   Y: onbekend" — an unknown axis is never a number.
    pub position: String,
    /// The last refusal in words, or empty.
    pub status: String,
    /// Where the setup put the compiled plan, or empty.
    pub placement: String,
    pub user_origin: String,
    pub start_from: StartFrom,
    pub anchor: JobAnchor,
    pub cut_selected: bool,
    pub use_selection_origin: bool,
    pub position_laser_mode: bool,
    /// The measured red-dot offset in words, or that there is none.
    pub pointer_offset: String,
    /// The switch "aim with the red dot"; only effective with a measured offset.
    pub use_pointer_offset: bool,
    pub can_use_pointer_offset: bool,
    pub step_mm: String,
    pub frame_speed: String,
    pub jog_held: bool,
    pub controls: Vec<PanelControl>,
    /// `(name, size)` per controller file; an unreported size reads "onbekend", never 0.
    pub files: Vec<(String, String)>,
}

pub fn laser_panel(vm: Option<&LaserViewModel>, locale: Locale) -> LaserPanel {
    let t = |text: LaserText| laser_text(text, locale);
    let Some(vm) = vm else {
        return LaserPanel {
            connected: false,
            position: String::new(),
            status: t(LaserText::NoMachine).to_owned(),
            placement: String::new(),
            user_origin: String::new(),
            start_from: StartFrom::AbsoluteCoordinates,
            anchor: JobAnchor::BottomLeft,
            cut_selected: false,
            use_selection_origin: false,
            position_laser_mode: false,
            pointer_offset: String::new(),
            use_pointer_offset: false,
            can_use_pointer_offset: false,
            step_mm: String::new(),
            frame_speed: String::new(),
            jog_held: false,
            controls: Vec::new(),
            files: Vec::new(),
        };
    };
    let axis = |name: &str, v: Option<f64>| {
        format!(
            "{name}: {}",
            v.map_or_else(|| t(LaserText::Unknown).to_owned(), |v| format!("{} mm", mm_text(v, locale)))
        )
    };
    let placement = vm.placement.as_ref().map_or_else(String::new, |p| {
        if p.setup_changed {
            return t(LaserText::PlacementStale).to_owned();
        }
        let shift = format!("{}; {}", mm_text(p.delta_mm.0, locale), mm_text(p.delta_mm.1, locale));
        let base = match p.from_position_mm {
            Some((x, y)) => format!(
                "{} ({shift}) mm {} ({}; {}) mm",
                t(LaserText::PlacementShift),
                t(LaserText::PlacementOnHead),
                mm_text(x, locale),
                mm_text(y, locale)
            ),
            None => format!("{} ({shift}) mm", t(LaserText::PlacementShift)),
        };
        // The offset the anchor was aimed with is said, never silently applied.
        match p.pointer_offset_mm {
            Some((x, y)) => {
                format!(
                    "{base}, {} ({}; {}) mm",
                    t(LaserText::PlacementWithDot),
                    mm_text(x, locale),
                    mm_text(y, locale)
                )
            }
            None => base,
        }
    });
    LaserPanel {
        connected: true,
        position: format!("{}   {}", axis("X", vm.position.x_mm), axis("Y", vm.position.y_mm)),
        status: vm.last_refusal.map_or_else(String::new, |why| laser_why_text(why, locale)),
        placement,
        user_origin: vm.setup.user_origin_mm.map_or_else(
            || t(LaserText::UserOriginNotSet).to_owned(),
            |(x, y)| format!("{}; {} mm", mm_text(x, locale), mm_text(y, locale)),
        ),
        start_from: vm.setup.start_from,
        anchor: vm.setup.anchor,
        cut_selected: vm.setup.cut_selected,
        use_selection_origin: vm.setup.use_selection_origin,
        position_laser_mode: vm.setup.position_laser_mode,
        pointer_offset: vm.setup.pointer_offset_mm.map_or_else(
            || t(LaserText::PointerOffsetNone).to_owned(),
            |(x, y)| {
                format!("{}: ({}; {}) mm", t(LaserText::PointerOffsetLabel), mm_text(x, locale), mm_text(y, locale))
            },
        ),
        use_pointer_offset: vm.setup.use_pointer_offset,
        can_use_pointer_offset: vm.setup.pointer_offset_mm.is_some(),
        step_mm: mm_text(vm.setup.step_mm, locale),
        frame_speed: mm_text(vm.setup.frame_speed_mm_s, locale),
        jog_held: vm.jog_held.is_some(),
        controls: vm
            .controls
            .iter()
            .map(|c| PanelControl {
                kind: c.kind.clone(),
                enabled: c.enabled,
                tooltip: c.why_not.map_or_else(String::new, |why| laser_why_text(why, locale)),
            })
            .collect(),
        files: vm
            .files
            .iter()
            .flatten()
            .map(|f| {
                (
                    f.name.as_str().to_owned(),
                    f.size_bytes.map_or_else(|| t(LaserText::SizeUnknown).to_owned(), |b| format!("{b} B")),
                )
            })
            .collect(),
    }
}

/// The controls with a fixed command, in the order the panel's buttons carry as their tag.
pub fn simple_controls() -> Vec<(LaserControlKind, LaserCommand)> {
    vec![
        (LaserControlKind::Home, LaserCommand::Action(LaserAction::Home(HomeTarget::Xy))),
        (LaserControlKind::GoToMachineOrigin, LaserCommand::Action(LaserAction::GoToOrigin(OriginTarget::Machine))),
        (LaserControlKind::GoToUserOrigin, LaserCommand::Action(LaserAction::GoToOrigin(OriginTarget::User))),
        (LaserControlKind::Pause, LaserCommand::Action(LaserAction::Pause)),
        (LaserControlKind::Resume, LaserCommand::Action(LaserAction::Resume)),
        (LaserControlKind::ShowPosition, LaserCommand::Action(LaserAction::ShowPosition)),
        (LaserControlKind::ListFiles, LaserCommand::Action(LaserAction::ListFiles)),
        (LaserControlKind::Step(StepDirection::Left), LaserCommand::Step(StepDirection::Left)),
        (LaserControlKind::Step(StepDirection::Right), LaserCommand::Step(StepDirection::Right)),
        (LaserControlKind::Step(StepDirection::Up), LaserCommand::Step(StepDirection::Up)),
        (LaserControlKind::Step(StepDirection::Down), LaserCommand::Step(StepDirection::Down)),
        (LaserControlKind::Frame(FrameShape::BoundingRectangle), LaserCommand::Frame(FrameShape::BoundingRectangle)),
        (LaserControlKind::Frame(FrameShape::RubberBand), LaserCommand::Frame(FrameShape::RubberBand)),
    ]
}

/// The intent of a control: the view model's idempotency key and seen state, unchanged. A control
/// that is off sends nothing.
pub fn laser_intent(
    document: DocumentId,
    vm: &LaserViewModel,
    kind: &LaserControlKind,
    command: LaserCommand,
) -> Option<UiIntent> {
    let control = vm.control(kind).filter(|c| c.enabled)?;
    Some(UiIntent::Laser { document: Some(document), action: control.action, seen: vm.seen.clone(), command })
}

/// A button of the fixed set (see [`simple_controls`]) by its tag.
pub fn simple_intent(document: DocumentId, vm: &LaserViewModel, tag: isize) -> Option<UiIntent> {
    let (kind, command) = simple_controls().into_iter().nth(usize::try_from(tag).ok()?)?;
    laser_intent(document, vm, &kind, command)
}

/// Key down on a jog button or arrow key.
pub fn jog_press_intent(document: DocumentId, vm: &LaserViewModel, key: JogKey) -> Option<UiIntent> {
    laser_intent(document, vm, &LaserControlKind::Jog(key), LaserCommand::Action(LaserAction::JogPress(key)))
}

/// Key up, focus lost, panel hidden or window closed: the held key is released. Always allowed.
pub fn jog_release_intent() -> UiIntent {
    UiIntent::LaserReleaseJog
}

/// "Ga naar": the typed X and Y, parsed by presentation.
pub fn move_intent(document: DocumentId, vm: &LaserViewModel, x: &str, y: &str) -> Option<UiIntent> {
    laser_intent(document, vm, &LaserControlKind::MoveTo, LaserCommand::MoveToText { x: x.to_owned(), y: y.to_owned() })
}

pub fn save_rd_intent(document: DocumentId, vm: &LaserViewModel, name: &str) -> Option<UiIntent> {
    laser_intent(document, vm, &LaserControlKind::SaveRd, LaserCommand::SaveRd { name: name.trim().to_owned() })
}

/// What a file row's button does; `row` is the table row, `which` 0 = info, 1 = run, 2 = delete.
pub fn file_intent(document: DocumentId, vm: &LaserViewModel, row: usize, which: usize) -> Option<UiIntent> {
    let name: ControllerFileName = vm.files.as_ref()?.get(row)?.name.clone();
    let (kind, action) = match which {
        0 => (LaserControlKind::FileMetadata(name.clone()), LaserAction::FileMetadata(name)),
        1 => (LaserControlKind::RunFile(name.clone()), LaserAction::RunFile(name)),
        2 => (LaserControlKind::DeleteFile(name.clone()), LaserAction::DeleteFile(name)),
        _ => return None,
    };
    laser_intent(document, vm, &kind, LaserCommand::Action(action))
}

/// Setup intents of the panel (window state; the next compile uses them).
pub fn start_from_for_segment(index: isize) -> Option<StartFrom> {
    match index {
        0 => Some(StartFrom::AbsoluteCoordinates),
        1 => Some(StartFrom::CurrentPosition),
        2 => Some(StartFrom::UserOrigin),
        _ => None,
    }
}

pub fn segment_for_start_from(start: StartFrom) -> isize {
    match start {
        StartFrom::AbsoluteCoordinates => 0,
        StartFrom::CurrentPosition => 1,
        StartFrom::UserOrigin => 2,
    }
}

/// The 3×3 job origin by the tag of its button, row by row from the top left.
pub fn anchor_for_tag(tag: isize) -> Option<JobAnchor> {
    JobAnchor::ALL.get(usize::try_from(tag).ok()?).copied()
}

pub fn anchor_text(anchor: JobAnchor) -> LaserText {
    match anchor {
        JobAnchor::TopLeft => LaserText::AnchorTopLeft,
        JobAnchor::TopCenter => LaserText::AnchorTopCenter,
        JobAnchor::TopRight => LaserText::AnchorTopRight,
        JobAnchor::MiddleLeft => LaserText::AnchorMiddleLeft,
        JobAnchor::Center => LaserText::AnchorCenter,
        JobAnchor::MiddleRight => LaserText::AnchorMiddleRight,
        JobAnchor::BottomLeft => LaserText::AnchorBottomLeft,
        JobAnchor::BottomCenter => LaserText::AnchorBottomCenter,
        JobAnchor::BottomRight => LaserText::AnchorBottomRight,
    }
}

pub fn setup_intent(setup: LaserSetup) -> UiIntent {
    UiIntent::LaserSetup { setup }
}

/// The jog key an arrow key stands for while the laser window has the keyboard. The mapping to
/// machine keys is the registry's naming (X_LEFT, X_RIGHT, Y_TOP, Y_BOTTOM); which way the head
/// really moves per key is part of the missing hardware evidence for `Jog`.
pub fn jog_key_for_key_code(code: u16) -> Option<JogKey> {
    match code {
        key_codes::LEFT => Some(JogKey::XLeft),
        key_codes::RIGHT => Some(JogKey::XRight),
        key_codes::UP => Some(JogKey::YTop),
        key_codes::DOWN => Some(JogKey::YBottom),
        _ => None,
    }
}

/// Virtual key codes of the arrow keys (stable macOS values).
pub mod key_codes {
    pub const LEFT: u16 = 123;
    pub const RIGHT: u16 = 124;
    pub const DOWN: u16 = 125;
    pub const UP: u16 = 126;
}
