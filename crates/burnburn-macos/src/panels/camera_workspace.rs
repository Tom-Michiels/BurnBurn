//! The camera window (W3-UI-camera) as plain data: what the panel shows from the presentation's
//! `CameraWorkspaceView` and which `UiIntent` each control becomes. No AppKit here, so all of it is
//! tested headlessly; the widgets in `appkit/panels/camera_window.rs` only copy this model.
//!
//! The panel states what the camera picture is: a visual placement aid. It shows why a control is
//! off or an action was refused in words (the typed `CameraWhy`), shows a calibration that no longer
//! applies as invalid with the reason, and never sends the job that burns the alignment marks
//! without the confirmation button for exactly the job it shows. Numbers go to presentation as
//! typed (DR-05).

use burnburn_image::calib::{Invalidity, MarkRole, StoreMiss};
use burnburn_presentation::{
    AlignmentView, CalibrationKind, CalibrationRow, CalibrationSummary, CalibrationValidity, CameraAuthorization,
    CameraViewModel, CameraWhy, CameraWorkspaceIntent as I, CameraWorkspaceView, MachineWhy, UiIntent,
};

use crate::l10n::{Locale, message_or_key};

/// Texts of the camera window (host-only; refusals are in [`camera_why_text`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CameraText {
    WindowTitle,
    OpenButton,
    SectionOverlay,
    ShowOverlay,
    Opacity,
    UpdateOverlay,
    MaterialHeight,
    EdgeMargin,
    NoPicture,
    SectionCameras,
    UseCamera,
    DropCamera,
    LensProfile,
    NoCamerasRunning,
    CameraNotAuthorized,
    SectionCalibrations,
    DeleteCalibration,
    Recalibrate,
    NoCalibrations,
    KindLens,
    KindAlignment,
    StateValid,
    StateInvalid,
    StateNotInUse,
    SectionLens,
    GridColumns,
    GridRows,
    GridPitch,
    StartLens,
    AddPhoto,
    FitLens,
    AcceptLens,
    CancelWizard,
    SectionAlignment,
    AlignHeight,
    StartAlignment,
    Marks,
    MarkX,
    MarkY,
    SetMark,
    ProposeMarks,
    Power,
    Speed,
    SendMarks,
    MarksSent,
    TakePhoto,
    PhotoHint,
    FitAlignment,
    SaveAlignment,
    NoPhotoYet,
    MarkNotPlaced,
    Busy,
    Detecting,
    Fitting,
    Accessibility,
}

fn pair(text: CameraText) -> (&'static str, &'static str) {
    use CameraText as T;
    match text {
        T::WindowTitle => ("Camera", "Camera"),
        T::OpenButton => ("Camerabeheer…", "Cameras…"),
        T::SectionOverlay => ("Beeld op het bed", "Picture on the bed"),
        T::ShowOverlay => ("Camerabeeld tonen", "Show camera picture"),
        T::Opacity => ("Dekking", "Opacity"),
        T::UpdateOverlay => ("Beeld bijwerken", "Update picture"),
        T::MaterialHeight => ("Materiaalhoogte (mm)", "Material height (mm)"),
        T::EdgeMargin => ("Randmarge (px)", "Edge margin (px)"),
        T::NoPicture => ("Nog geen beeld: kies ‘Beeld bijwerken’.", "No picture yet: choose ‘Update picture’."),
        T::SectionCameras => ("Camera’s in het bedstelsel", "Cameras in the bed system"),
        T::UseCamera => ("Camera gebruiken", "Use camera"),
        T::DropCamera => ("Camera stoppen", "Stop camera"),
        T::LensProfile => ("Lensprofiel", "Lens profile"),
        T::NoCamerasRunning => ("Geen camera in gebruik.", "No camera in use."),
        T::CameraNotAuthorized => (
            "Camera niet toegestaan. Vraag toestemming in het cameragedeelte van het hoofdvenster.",
            "Camera access not granted. Ask for it in the camera section of the main window.",
        ),
        T::SectionCalibrations => ("Kalibraties", "Calibrations"),
        T::DeleteCalibration => ("Verwijderen", "Delete"),
        T::Recalibrate => ("Opnieuw kalibreren", "Recalibrate"),
        T::NoCalibrations => ("Nog geen kalibraties.", "No calibrations yet."),
        T::KindLens => ("lens", "lens"),
        T::KindAlignment => ("uitlijning", "alignment"),
        T::StateValid => ("geldig", "valid"),
        T::StateInvalid => ("ONGELDIG", "INVALID"),
        T::StateNotInUse => ("camera niet in gebruik", "camera not in use"),
        T::SectionLens => ("Lenskalibratie", "Lens calibration"),
        T::GridColumns => ("Hoeken per rij", "Corners per row"),
        T::GridRows => ("Hoeken per kolom", "Corners per column"),
        T::GridPitch => ("Afstand (mm)", "Pitch (mm)"),
        T::StartLens => ("Wizard starten", "Start wizard"),
        T::AddPhoto => ("Patroon fotograferen", "Photograph pattern"),
        T::FitLens => ("Lens berekenen", "Fit lens"),
        T::AcceptLens => ("Kalibratie bewaren", "Keep calibration"),
        T::CancelWizard => ("Annuleren", "Cancel"),
        T::SectionAlignment => ("Werkruimte uitlijnen", "Workspace alignment"),
        T::AlignHeight => ("Hoogte van het materiaal (mm)", "Height of the material (mm)"),
        T::StartAlignment => ("Uitlijning starten", "Start alignment"),
        T::Marks => ("Markering", "Mark"),
        T::MarkX => ("X (mm)", "X (mm)"),
        T::MarkY => ("Y (mm)", "Y (mm)"),
        T::SetMark => ("Positie zetten", "Set position"),
        T::ProposeMarks => ("Markeringsjob voorstellen", "Propose marks job"),
        T::Power => ("Vermogen (%)", "Power (%)"),
        T::Speed => ("Snelheid (mm/s)", "Speed (mm/s)"),
        T::SendMarks => ("Markeringen naar de machine sturen", "Send marks to the machine"),
        T::MarksSent => (
            "Verstuurd als upload. Starten doe je in het machinepaneel.",
            "Sent as an upload. Starting is done in the machine panel.",
        ),
        T::TakePhoto => ("Foto van het bed maken", "Photograph the bed"),
        T::PhotoHint => (
            "Klik in de foto op het midden van de gekozen markering.",
            "Click the centre of the chosen mark in the photo.",
        ),
        T::FitAlignment => ("Uitlijning berekenen", "Fit alignment"),
        T::SaveAlignment => ("Uitlijning bewaren", "Keep alignment"),
        T::NoPhotoYet => ("Nog geen foto.", "No photo yet."),
        T::MarkNotPlaced => ("nog niet aangeklikt", "not clicked yet"),
        T::Busy => ("Bezig…", "Working…"),
        T::Detecting => ("Hoeken zoeken…", "Looking for corners…"),
        T::Fitting => ("Lens berekenen…", "Fitting the lens…"),
        T::Accessibility => ("Camera- en kalibratievenster", "Camera and calibration window"),
    }
}

pub fn camera_text(text: CameraText, locale: Locale) -> &'static str {
    let (nl, en) = pair(text);
    match locale {
        Locale::Nl => nl,
        Locale::En => en,
    }
}

/// Every text of the window, for the completeness test.
pub fn all_camera_texts() -> Vec<CameraText> {
    use CameraText as T;
    vec![
        T::WindowTitle,
        T::OpenButton,
        T::SectionOverlay,
        T::ShowOverlay,
        T::Opacity,
        T::UpdateOverlay,
        T::MaterialHeight,
        T::EdgeMargin,
        T::NoPicture,
        T::SectionCameras,
        T::UseCamera,
        T::DropCamera,
        T::LensProfile,
        T::NoCamerasRunning,
        T::CameraNotAuthorized,
        T::SectionCalibrations,
        T::DeleteCalibration,
        T::Recalibrate,
        T::NoCalibrations,
        T::KindLens,
        T::KindAlignment,
        T::StateValid,
        T::StateInvalid,
        T::StateNotInUse,
        T::SectionLens,
        T::GridColumns,
        T::GridRows,
        T::GridPitch,
        T::StartLens,
        T::AddPhoto,
        T::FitLens,
        T::AcceptLens,
        T::CancelWizard,
        T::SectionAlignment,
        T::AlignHeight,
        T::StartAlignment,
        T::Marks,
        T::MarkX,
        T::MarkY,
        T::SetMark,
        T::ProposeMarks,
        T::Power,
        T::Speed,
        T::SendMarks,
        T::MarksSent,
        T::TakePhoto,
        T::PhotoHint,
        T::FitAlignment,
        T::SaveAlignment,
        T::NoPhotoYet,
        T::MarkNotPlaced,
        T::Busy,
        T::Detecting,
        T::Fitting,
        T::Accessibility,
    ]
}

fn invalidity_text(why: Invalidity, locale: Locale) -> &'static str {
    let (nl, en) = match why {
        Invalidity::DeviceChanged => ("andere camera", "other camera"),
        Invalidity::ResolutionChanged => ("resolutie gewijzigd", "resolution changed"),
        Invalidity::PixelFormatChanged => ("pixelformaat gewijzigd", "pixel format changed"),
        Invalidity::FrameRateChanged => ("beeldsnelheid gewijzigd", "frame rate changed"),
        Invalidity::LensChanged => ("lensprofiel gewijzigd", "lens profile changed"),
    };
    if locale == Locale::Nl { nl } else { en }
}

fn store_miss_text(miss: &StoreMiss, locale: Locale) -> String {
    let nl = locale == Locale::Nl;
    match miss {
        StoreMiss::NotCalibrated => {
            if nl {
                "Deze camera is nog niet uitgelijnd.".into()
            } else {
                "This camera has not been aligned yet.".into()
            }
        }
        StoreMiss::Invalid(why) => {
            let reason = invalidity_text(*why, locale);
            if nl {
                format!("De bestaande kalibratie geldt niet meer: {reason}. Kalibreer opnieuw.")
            } else {
                format!("The stored calibration no longer applies: {reason}. Calibrate again.")
            }
        }
        StoreMiss::NoAlignmentForHeight { available_mm } => {
            let heights: Vec<String> = available_mm.iter().map(|h| format!("{h} mm")).collect();
            if nl {
                format!("Geen uitlijning voor deze materiaalhoogte (beschikbaar: {}).", heights.join(", "))
            } else {
                format!("No alignment for this material height (available: {}).", heights.join(", "))
            }
        }
    }
}

/// The sentence for a refusal, in the user's language.
pub fn camera_why_text(why: &CameraWhy, locale: Locale) -> String {
    let nl = locale == Locale::Nl;
    let pick = |a: &str, b: &str| (if nl { a } else { b }).to_owned();
    match why {
        CameraWhy::NoCameraPort => pick("Er is geen camerapoort.", "There is no camera port."),
        CameraWhy::CameraNotAuthorized => pick(
            "De camera mag niet gebruikt worden: vraag eerst toestemming in het hoofdvenster.",
            "The camera may not be used: ask for access in the main window first.",
        ),
        CameraWhy::CameraRefused => {
            pick("De camera weigerde te starten in deze modus.", "The camera refused to start in this mode.")
        }
        CameraWhy::CameraNotRunning => pick(
            "Deze camera draait niet (in deze configuratie).",
            "This camera is not running (in this configuration).",
        ),
        CameraWhy::NoFrame => {
            pick("De camera heeft nog geen beeld geleverd.", "The camera has not delivered a frame yet.")
        }
        CameraWhy::FrameConfigChanged => pick(
            "De camera staat in een andere configuratie dan waarvoor dit begon.",
            "The camera is in another configuration than the one this started with.",
        ),
        CameraWhy::InvalidNumber => pick("Dat is geen geldig getal.", "That is not a valid number."),
        CameraWhy::NothingToUpdate => pick("Geen camera in gebruik om bij te werken.", "No camera in use to update."),
        CameraWhy::NotCalibrated(miss) => store_miss_text(miss, locale),
        CameraWhy::Unmappable => pick("Dit beeldpunt past niet op het bed.", "This pixel does not map onto the bed."),
        CameraWhy::NoLensWizard => pick("Er is geen lenswizard open.", "No lens wizard is open."),
        CameraWhy::NoAlignmentWizard => pick("Er is geen uitlijnwizard open.", "No alignment wizard is open."),
        CameraWhy::Busy => pick(
            "Er loopt nog werk; wacht tot het klaar is of annuleer.",
            "Work is still running; wait for it or cancel.",
        ),
        CameraWhy::Corners(_) => pick(
            "Het patroon is niet gevonden in dit beeld. Zorg dat het hele patroon scherp en vlak in beeld is.",
            "The pattern was not found in this picture. Make sure the whole pattern is in view, sharp and flat.",
        ),
        CameraWhy::Lens(_) => pick(
            "De lens kon niet betrouwbaar berekend worden. Voeg beelden toe vanuit andere hoeken.",
            "The lens could not be fitted reliably. Add pictures from other angles.",
        ),
        CameraWhy::Align(_) => pick(
            "De markeringen spreken elkaar tegen of zijn te weinig; controleer de aangeklikte posities.",
            "The marks contradict each other or are too few; check the clicked positions.",
        ),
        CameraWhy::NoPatternViews => {
            pick("Eerst minstens één patroonfoto toevoegen.", "Add at least one pattern photo first.")
        }
        CameraWhy::NoLensResult => pick("Er is nog geen lensresultaat.", "There is no lens result yet."),
        CameraWhy::BadGrid => pick(
            "Dit is geen dambordpatroon (minstens 2 × 2 hoeken).",
            "This is not a checkerboard (at least 2 × 2 corners).",
        ),
        CameraWhy::MarkIndex => pick("Deze markering bestaat niet.", "That mark does not exist."),
        CameraWhy::OutsideImage => pick("Dit punt ligt buiten de foto.", "That point is outside the photo."),
        CameraWhy::NoPhoto => pick("Maak eerst een foto van het bed.", "Photograph the bed first."),
        CameraWhy::MarksIncomplete => pick(
            "Klik minstens vier passtippen en één controlemarkering aan.",
            "Click at least four fit marks and one check mark.",
        ),
        CameraWhy::NoAlignmentResult => {
            pick("Er is nog geen uitlijning berekend.", "No alignment has been fitted yet.")
        }
        CameraWhy::NoMachine => pick(
            "Geen machine verbonden: de markeringen kunnen niet gebrand worden.",
            "No machine connected: the marks cannot be burned.",
        ),
        CameraWhy::NoMachineProfile => pick("Geen machineprofiel gekozen.", "No machine profile chosen."),
        CameraWhy::MarksJobFailed => {
            pick("De markeringsjob kon niet gebouwd worden.", "The marks job could not be built.")
        }
        CameraWhy::MarksOutsideBed => pick(
            "Een markering (met haar kruis) valt buiten het bed.",
            "A mark (with its cross) falls outside the bed.",
        ),
        CameraWhy::SpeedTooHigh => pick(
            "Deze snelheid is hoger dan het machineprofiel toestaat.",
            "This speed is higher than the machine profile allows.",
        ),
        CameraWhy::NotProposed => pick(
            "Dit is niet de job die nu voorgesteld wordt; er is niets verstuurd.",
            "This is not the job that is proposed now; nothing was sent.",
        ),
        CameraWhy::NotAllowedForRole => pick("Deze rol mag dit niet.", "This role may not do this."),
        CameraWhy::MachineRefused(why) => machine_why_text(*why, locale),
        _ => pick("Geweigerd.", "Refused."),
    }
}

fn machine_why_text(why: MachineWhy, locale: Locale) -> String {
    let nl = locale == Locale::Nl;
    let (a, b) = match why {
        MachineWhy::IdentityNotConfigured => (
            "De machine heeft geen model of firmware ingesteld, dus de controller kan niet gecontroleerd worden. Er is niets verstuurd.",
            "The machine has no model or firmware configured, so the controller cannot be verified. Nothing was sent.",
        ),
        MachineWhy::Busy => ("De machine is bezig met een andere job.", "The machine is busy with another job."),
        MachineWhy::StateChanged => (
            "De machinestatus is veranderd sinds je het voorstel zag; er is niets verstuurd.",
            "The machine state changed since you saw the proposal; nothing was sent.",
        ),
        MachineWhy::NotRecorded => (
            "De actie kon niet vastgelegd worden; er is niets verstuurd.",
            "The action could not be recorded; nothing was sent.",
        ),
        MachineWhy::WrongMachine => ("Dit is een andere machine.", "This is another machine."),
        _ => ("De machine weigerde de job.", "The machine refused the job."),
    };
    (if nl { a } else { b }).to_owned()
}

// ---------------------------------------------------------------------------------------------
// The panel model
// ---------------------------------------------------------------------------------------------

/// The buttons of the window; the tag of a button is its index in [`CameraCommand::ALL`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CameraCommand {
    UpdateOverlay,
    UseCamera,
    DropCamera,
    DeleteCalibration,
    Recalibrate,
    StartLens,
    AddPhoto,
    FitLens,
    AcceptLens,
    CancelLens,
    StartAlignment,
    SetMark,
    ProposeMarks,
    SendMarks,
    TakePhoto,
    FitAlignment,
    SaveAlignment,
    CancelAlignment,
}

impl CameraCommand {
    pub const ALL: [CameraCommand; 18] = [
        CameraCommand::UpdateOverlay,
        CameraCommand::UseCamera,
        CameraCommand::DropCamera,
        CameraCommand::DeleteCalibration,
        CameraCommand::Recalibrate,
        CameraCommand::StartLens,
        CameraCommand::AddPhoto,
        CameraCommand::FitLens,
        CameraCommand::AcceptLens,
        CameraCommand::CancelLens,
        CameraCommand::StartAlignment,
        CameraCommand::SetMark,
        CameraCommand::ProposeMarks,
        CameraCommand::SendMarks,
        CameraCommand::TakePhoto,
        CameraCommand::FitAlignment,
        CameraCommand::SaveAlignment,
        CameraCommand::CancelAlignment,
    ];

    pub fn from_tag(tag: isize) -> Option<CameraCommand> {
        Self::ALL.get(usize::try_from(tag).ok()?).copied()
    }

    pub fn tag(self) -> isize {
        Self::ALL.iter().position(|c| *c == self).map_or(-1, |i| i as isize)
    }

    pub fn title(self, locale: Locale) -> &'static str {
        use CameraText as T;
        let text = match self {
            CameraCommand::UpdateOverlay => T::UpdateOverlay,
            CameraCommand::UseCamera => T::UseCamera,
            CameraCommand::DropCamera => T::DropCamera,
            CameraCommand::DeleteCalibration => T::DeleteCalibration,
            CameraCommand::Recalibrate => T::Recalibrate,
            CameraCommand::StartLens => T::StartLens,
            CameraCommand::AddPhoto => T::AddPhoto,
            CameraCommand::FitLens => T::FitLens,
            CameraCommand::AcceptLens => T::AcceptLens,
            CameraCommand::CancelLens | CameraCommand::CancelAlignment => T::CancelWizard,
            CameraCommand::StartAlignment => T::StartAlignment,
            CameraCommand::SetMark => T::SetMark,
            CameraCommand::ProposeMarks => T::ProposeMarks,
            CameraCommand::SendMarks => T::SendMarks,
            CameraCommand::TakePhoto => T::TakePhoto,
            CameraCommand::FitAlignment => T::FitAlignment,
            CameraCommand::SaveAlignment => T::SaveAlignment,
        };
        camera_text(text, locale)
    }
}

/// What the user typed and chose in the window, as the controls hold it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CameraInputs {
    /// Item chosen in the pop-up of cameras the OS offers.
    pub device: isize,
    /// Item chosen in the pop-up of cameras in use.
    pub running: isize,
    pub calibration: isize,
    pub mark: isize,
    pub lens_profile: String,
    pub cols: String,
    pub rows: String,
    pub pitch: String,
    pub align_height: String,
    pub power: String,
    pub speed: String,
    pub mark_x: String,
    pub mark_y: String,
}

/// One line of the window per piece of state, and which buttons work.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CameraWindowPanel {
    pub overlay_visible: bool,
    pub opacity_percent: f64,
    pub height_text: String,
    pub margin_text: String,
    pub overlay_status: String,
    pub note: String,
    /// Names of the cameras the OS offers.
    pub devices: Vec<String>,
    /// The cameras in use, one per pop-up item.
    pub running: Vec<String>,
    pub calibrations: Vec<String>,
    pub marks: Vec<String>,
    pub lens_status: String,
    pub alignment_status: String,
    pub marks_status: String,
    pub refusal: String,
    pub enabled: Vec<(CameraCommand, bool)>,
    /// Pixel size of the photo the marks are clicked in.
    pub photo: Option<(u32, u32)>,
}

impl CameraWindowPanel {
    pub fn is_enabled(&self, command: CameraCommand) -> bool {
        self.enabled.iter().any(|(c, on)| *c == command && *on)
    }
}

fn mm(value: f64, locale: Locale) -> String {
    let text = burnburn_presentation::format_mm(value);
    if locale == Locale::Nl { text.replace('.', ",") } else { text }
}

/// The text of one calibration row, with its badge ("geldig" / "ONGELDIG: …").
pub fn calibration_row_text(row: &CalibrationRow, locale: Locale) -> String {
    let kind = match row.reference.kind {
        CalibrationKind::Lens => camera_text(CameraText::KindLens, locale).to_owned(),
        CalibrationKind::Alignment { .. } => match row.summary {
            CalibrationSummary::Alignment { material_height_mm, .. } => {
                format!("{} {} mm", camera_text(CameraText::KindAlignment, locale), mm(material_height_mm, locale))
            }
            _ => camera_text(CameraText::KindAlignment, locale).to_owned(),
        },
    };
    let state = match row.validity {
        CalibrationValidity::Valid => camera_text(CameraText::StateValid, locale).to_owned(),
        CalibrationValidity::Invalid(why) => {
            format!("{}: {}", camera_text(CameraText::StateInvalid, locale), invalidity_text(why, locale))
        }
        CalibrationValidity::CameraNotInUse => camera_text(CameraText::StateNotInUse, locale).to_owned(),
    };
    let fps = f64::from(row.frame_rate_mhz) / 1000.0;
    let fps = if fps.fract() == 0.0 { format!("{fps:.0}") } else { mm(fps, locale) };
    format!("{} · {}×{} · {} fps · {} · {} · {}", row.device.0, row.width, row.height, fps, row.lens, kind, state)
}

fn mark_text(m: &burnburn_presentation::MarkView, locale: Locale) -> String {
    let role = match (m.role, locale) {
        (MarkRole::Fit, Locale::Nl) => "pas",
        (MarkRole::Fit, Locale::En) => "fit",
        (MarkRole::Check, Locale::Nl) => "controle",
        (MarkRole::Check, Locale::En) => "check",
    };
    let pixel = match m.pixel {
        Some([x, y]) => format!("({x:.0}, {y:.0}) px"),
        None => camera_text(CameraText::MarkNotPlaced, locale).to_owned(),
    };
    format!("{} · {} · ({}, {}) mm · {}", m.index + 1, role, mm(m.bed_mm[0], locale), mm(m.bed_mm[1], locale), pixel)
}

fn lens_status(vm: &CameraWorkspaceView, locale: Locale) -> String {
    let nl = locale == Locale::Nl;
    let Some(w) = &vm.lens else { return String::new() };
    let mut lines = Vec::new();
    if w.detecting {
        lines.push(camera_text(CameraText::Detecting, locale).to_owned());
    }
    if w.fitting {
        lines.push(camera_text(CameraText::Fitting, locale).to_owned());
    }
    let views: Vec<String> = w.views.iter().map(ToString::to_string).collect();
    lines.push(if nl {
        format!("{} foto’s ({} hoeken per foto)", w.views.len(), views.join(", "))
    } else {
        format!("{} photos ({} corners each)", w.views.len(), views.join(", "))
    });
    if let Some(r) = &w.result {
        lines.push(if nl {
            format!(
                "Resultaat: k1 {:.4}, k2 {:.4}, restfout {:.2} px (max {:.2}){}",
                r.k1,
                r.k2,
                r.rms_px,
                r.max_px,
                if r.converged { "" } else { " — niet geconvergeerd" }
            )
        } else {
            format!(
                "Result: k1 {:.4}, k2 {:.4}, residual {:.2} px (max {:.2}){}",
                r.k1,
                r.k2,
                r.rms_px,
                r.max_px,
                if r.converged { "" } else { " — not converged" }
            )
        });
    }
    if let Some(e) = &w.error {
        lines.push(camera_why_text(e, locale));
    }
    lines.join("\n")
}

fn alignment_status(a: &AlignmentView, locale: Locale) -> String {
    let nl = locale == Locale::Nl;
    let mut lines = Vec::new();
    lines.push(match (a.photo, nl) {
        (Some((w, h)), true) => format!("Foto {w} × {h} px — {}", camera_text(CameraText::PhotoHint, locale)),
        (Some((w, h)), false) => format!("Photo {w} × {h} px — {}", camera_text(CameraText::PhotoHint, locale)),
        (None, _) => camera_text(CameraText::NoPhotoYet, locale).to_owned(),
    });
    lines.push(if a.uses_lens_calibration {
        if nl { "Met de bewaarde lenskalibratie.".into() } else { "With the stored lens calibration.".into() }
    } else if nl {
        "Zonder lenskalibratie (ideale lens).".into()
    } else {
        "Without a lens calibration (ideal lens).".into()
    });
    if let Some(r) = &a.result {
        let errors: Vec<String> = r.check_errors_mm.iter().map(|e| mm((e * 100.0).round() / 100.0, locale)).collect();
        lines.push(if nl {
            format!(
                "Pasfout {:.2} mm (max {:.2}); controlefout {:.2} mm (max {:.2}); per controlemarkering: {} mm",
                r.fit_rms_mm,
                r.fit_max_mm,
                r.check_rms_mm,
                r.check_max_mm,
                errors.join(", ")
            )
        } else {
            format!(
                "Fit error {:.2} mm (max {:.2}); check error {:.2} mm (max {:.2}); per check mark: {} mm",
                r.fit_rms_mm,
                r.fit_max_mm,
                r.check_rms_mm,
                r.check_max_mm,
                errors.join(", ")
            )
        });
    }
    if let Some(e) = &a.error {
        lines.push(camera_why_text(e, locale));
    }
    lines.join("\n")
}

fn marks_status(a: &AlignmentView, locale: Locale) -> String {
    let nl = locale == Locale::Nl;
    let Some(p) = &a.proposal else { return String::new() };
    let mut lines = vec![if nl {
        format!(
            "Voorstel ‘{}’: {} kruisjes, {:.0} % vermogen, {:.0} mm/s.",
            p.name,
            p.marks_mm.len(),
            p.power_percent,
            p.speed_mm_s
        )
    } else {
        format!(
            "Proposal ‘{}’: {} crosses, {:.0} % power, {:.0} mm/s.",
            p.name,
            p.marks_mm.len(),
            p.power_percent,
            p.speed_mm_s
        )
    }];
    if p.sent {
        lines.push(camera_text(CameraText::MarksSent, locale).to_owned());
    } else if let Some(why) = &p.confirm_why_not {
        lines.push(camera_why_text(why, locale));
    }
    lines.join("\n")
}

/// The window from the view models. `primary` is the strip's camera model (devices, permission).
pub fn camera_window_panel(
    vm: Option<&CameraWorkspaceView>,
    primary: Option<&CameraViewModel>,
    locale: Locale,
) -> CameraWindowPanel {
    let Some(vm) = vm else { return CameraWindowPanel::default() };
    let overlay = &vm.overlay;
    let authorized = primary.is_some_and(|c| c.authorization == CameraAuthorization::Authorized);
    let devices: Vec<String> = primary.map(|c| c.devices.iter().map(|d| d.name.clone()).collect()).unwrap_or_default();
    let has_bgra = primary.is_some_and(|c| c.devices.iter().any(|d| crate::panels::bgra_mode_of(&d.modes).is_some()));
    let lens_open = vm.lens.is_some();
    let align_open = vm.alignment.is_some();
    let running = !vm.cameras.is_empty();
    let mut overlay_lines: Vec<String> = overlay
        .layers
        .iter()
        .map(|l| {
            let name = vm.cameras.iter().find(|c| c.device == l.device).map_or(l.device.0.clone(), |c| c.name.clone());
            match &l.not_placed {
                None => {
                    if locale == Locale::Nl {
                        format!("{name}: beeld {} geplaatst ({} cellen)", l.captured_seq, l.cells)
                    } else {
                        format!("{name}: picture {} placed ({} cells)", l.captured_seq, l.cells)
                    }
                }
                Some(why) => format!("{name}: {}", camera_why_text(why, locale)),
            }
        })
        .collect();
    if overlay_lines.is_empty() {
        overlay_lines.push(camera_text(CameraText::NoPicture, locale).to_owned());
    }
    let enabled = |c: CameraCommand| match c {
        CameraCommand::UpdateOverlay => overlay.can_update,
        CameraCommand::UseCamera => authorized && has_bgra && !devices.is_empty(),
        CameraCommand::DropCamera => running,
        CameraCommand::DeleteCalibration | CameraCommand::Recalibrate => !vm.calibrations.is_empty(),
        CameraCommand::StartLens => running && !lens_open,
        CameraCommand::AddPhoto => vm.lens.as_ref().is_some_and(|w| w.can_add_view),
        CameraCommand::FitLens => vm.lens.as_ref().is_some_and(|w| w.can_fit),
        CameraCommand::AcceptLens => vm.lens.as_ref().is_some_and(|w| w.can_accept),
        CameraCommand::CancelLens => lens_open,
        CameraCommand::StartAlignment => running && !align_open,
        CameraCommand::SetMark => align_open,
        CameraCommand::ProposeMarks => vm.alignment.as_ref().is_some_and(|a| a.can_propose),
        CameraCommand::SendMarks => {
            vm.alignment.as_ref().and_then(|a| a.proposal.as_ref()).is_some_and(|p| p.confirm.is_some())
        }
        CameraCommand::TakePhoto => vm.alignment.as_ref().is_some_and(|a| a.can_capture),
        CameraCommand::FitAlignment => vm.alignment.as_ref().is_some_and(|a| a.can_fit),
        CameraCommand::SaveAlignment => vm.alignment.as_ref().is_some_and(|a| a.can_save),
        CameraCommand::CancelAlignment => align_open,
    };
    CameraWindowPanel {
        overlay_visible: overlay.visible,
        opacity_percent: overlay.opacity_percent,
        height_text: mm(vm.material_height_mm, locale),
        margin_text: mm(overlay.edge_margin_px, locale),
        overlay_status: overlay_lines.join("\n"),
        note: message_or_key(overlay.note, locale),
        devices: devices.clone(),
        running: vm
            .cameras
            .iter()
            .map(|c| format!("{} · {}×{} · {}", c.name, c.mode.width, c.mode.height, c.lens))
            .collect(),
        calibrations: vm.calibrations.iter().map(|r| calibration_row_text(r, locale)).collect(),
        marks: vm
            .alignment
            .as_ref()
            .map(|a| a.marks.iter().map(|m| mark_text(m, locale)).collect())
            .unwrap_or_default(),
        lens_status: lens_status(vm, locale),
        alignment_status: vm.alignment.as_ref().map(|a| alignment_status(a, locale)).unwrap_or_default(),
        marks_status: vm.alignment.as_ref().map(|a| marks_status(a, locale)).unwrap_or_default(),
        refusal: vm.last_refusal.as_ref().map(|w| camera_why_text(w, locale)).unwrap_or_default(),
        enabled: CameraCommand::ALL.iter().map(|c| (*c, enabled(*c))).collect(),
        photo: vm.alignment.as_ref().and_then(|a| a.photo),
    }
}

// ---------------------------------------------------------------------------------------------
// Intents
// ---------------------------------------------------------------------------------------------

fn index(i: isize) -> Option<usize> {
    usize::try_from(i).ok()
}

/// The checkbox "show camera picture".
pub fn overlay_visible_intent(on: bool) -> UiIntent {
    UiIntent::CameraWorkspace(I::SetOverlayVisible(on))
}

/// The opacity slider (0–100).
pub fn opacity_intent(percent: f64) -> UiIntent {
    UiIntent::CameraWorkspace(I::SetOverlayOpacity { percent })
}

pub fn height_intent(text: &str) -> UiIntent {
    UiIntent::CameraWorkspace(I::SetMaterialHeight { mm: text.to_owned() })
}

pub fn margin_intent(text: &str) -> UiIntent {
    UiIntent::CameraWorkspace(I::SetEdgeMargin { px: text.to_owned() })
}

fn count(text: &str) -> u32 {
    // Anything that is not a whole number goes on as 0, which presentation refuses as a bad grid.
    text.trim().parse().unwrap_or(0)
}

/// The intent a button sends, or `None` when it is off (nothing is sent for a disabled control).
pub fn camera_command_intent(
    command: CameraCommand,
    workspace: Option<&CameraWorkspaceView>,
    primary: Option<&CameraViewModel>,
    inputs: &CameraInputs,
) -> Option<UiIntent> {
    let vm = workspace?;
    let panel = camera_window_panel(Some(vm), primary, Locale::En);
    if !panel.is_enabled(command) {
        return None;
    }
    let running_device = || vm.cameras.get(index(inputs.running)?).map(|c| c.device.clone());
    let intent = match command {
        CameraCommand::UpdateOverlay => I::UpdateOverlay,
        CameraCommand::UseCamera => {
            let device = primary?.devices.get(index(inputs.device)?)?;
            let mode = crate::panels::bgra_mode_of(&device.modes)?;
            I::UseCamera { device: device.id.clone(), mode, lens: inputs.lens_profile.trim().to_owned() }
        }
        CameraCommand::DropCamera => I::DropCamera { device: running_device()? },
        CameraCommand::DeleteCalibration => {
            I::DeleteCalibration { calibration: vm.calibrations.get(index(inputs.calibration)?)?.reference.clone() }
        }
        CameraCommand::Recalibrate => {
            I::Recalibrate { calibration: vm.calibrations.get(index(inputs.calibration)?)?.reference.clone() }
        }
        CameraCommand::StartLens => I::StartLensCalibration {
            device: running_device()?,
            cols: count(&inputs.cols),
            rows: count(&inputs.rows),
            pitch_mm: inputs.pitch.clone(),
        },
        CameraCommand::AddPhoto => I::AddPatternView,
        CameraCommand::FitLens => I::FitLens { two_coefficients: false },
        CameraCommand::AcceptLens => I::AcceptLensCalibration,
        CameraCommand::CancelLens => I::CancelLensCalibration,
        CameraCommand::StartAlignment => {
            I::StartAlignment { device: running_device()?, material_height_mm: inputs.align_height.clone() }
        }
        CameraCommand::SetMark => {
            I::SetAlignmentMark { index: index(inputs.mark)?, x_mm: inputs.mark_x.clone(), y_mm: inputs.mark_y.clone() }
        }
        CameraCommand::ProposeMarks => {
            I::ProposeMarksJob { power_percent: inputs.power.clone(), speed_mm_s: inputs.speed.clone() }
        }
        CameraCommand::SendMarks => {
            let proposal = vm.alignment.as_ref()?.proposal.as_ref()?;
            let confirm = proposal.confirm.clone()?;
            I::ConfirmMarksJob { plan: proposal.plan, action: confirm.action, seen: confirm.seen }
        }
        CameraCommand::TakePhoto => I::CaptureAlignmentPhoto,
        CameraCommand::FitAlignment => I::FitAlignment,
        CameraCommand::SaveAlignment => I::SaveAlignment,
        CameraCommand::CancelAlignment => I::CancelAlignment,
    };
    Some(UiIntent::CameraWorkspace(intent))
}

/// A click in the photo with the chosen mark: the pixel of the photo, `None` when the click is
/// outside it (the photo is shown proportionally scaled and centred in the view).
pub fn photo_click_intent(
    workspace: Option<&CameraWorkspaceView>,
    mark: isize,
    view_size: (f64, f64),
    click: (f64, f64),
) -> Option<UiIntent> {
    let alignment = workspace?.alignment.as_ref()?;
    let photo = alignment.photo?;
    let pixel = photo_click_to_pixel(view_size, photo, click)?;
    Some(UiIntent::CameraWorkspace(I::PlaceAlignmentMark { index: index(mark)?, pixel }))
}

/// Maps a click in a view (origin bottom-left, as AppKit reports it) to the pixel of a picture that
/// the view shows scaled to fit and centred. Pixel coordinates have their origin at the top-left.
pub fn photo_click_to_pixel(view: (f64, f64), image: (u32, u32), click: (f64, f64)) -> Option<[f64; 2]> {
    let (iw, ih) = (f64::from(image.0), f64::from(image.1));
    if !(view.0 > 0.0 && view.1 > 0.0 && iw > 0.0 && ih > 0.0 && click.0.is_finite() && click.1.is_finite()) {
        return None;
    }
    let scale = (view.0 / iw).min(view.1 / ih);
    let (shown_w, shown_h) = (iw * scale, ih * scale);
    let (ox, oy) = ((view.0 - shown_w) / 2.0, (view.1 - shown_h) / 2.0);
    let (px, py_up) = ((click.0 - ox) / scale, (click.1 - oy) / scale);
    let py = ih - py_up;
    ((0.0..=iw).contains(&px) && (0.0..=ih).contains(&py)).then_some([px, py])
}

/// The photo with a cross drawn on every mark that was clicked: red for a fit mark, blue for a
/// check mark. Returns tightly packed RGBA with the same size; the original is untouched.
pub fn photo_with_marks(rgba: &[u8], width: u32, height: u32, marks: &[([f64; 2], MarkRole)]) -> Vec<u8> {
    let mut out = rgba.to_vec();
    let (w, h) = (width as i64, height as i64);
    if out.len() != (width as usize) * (height as usize) * 4 {
        return out;
    }
    for (pixel, role) in marks {
        let color = match role {
            MarkRole::Fit => [255, 0, 0, 255],
            MarkRole::Check => [0, 80, 255, 255],
        };
        let (cx, cy) = (pixel[0].round() as i64, pixel[1].round() as i64);
        for d in -8..=8i64 {
            for (x, y) in [(cx + d, cy), (cx, cy + d)] {
                if (0..w).contains(&x) && (0..h).contains(&y) {
                    let at = ((y * w + x) * 4) as usize;
                    out[at..at + 4].copy_from_slice(&color);
                }
            }
        }
    }
    out
}

/// The next mark without a clicked position after `after` (wrapping), for the click-through flow.
pub fn next_unplaced_mark(alignment: &AlignmentView, after: usize) -> Option<usize> {
    let n = alignment.marks.len();
    (1..=n).map(|d| (after + d) % n.max(1)).find(|i| alignment.marks.get(*i).is_some_and(|m| m.pixel.is_none()))
}
