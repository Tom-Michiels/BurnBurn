//! The hosts' one Rust string catalogue (NL/EN): every view-model [`MessageKey`] and every text
//! only a host shows (menu titles, sheets, panels). Presentation hands out keys, never strings;
//! hosts show what this catalogue says, in the language chosen once at start-up from the system's
//! preferred languages.
//!
//! Interim home: contracts-phase-c §IM20 puts the catalogue in `burnburn-presentation` (where a
//! `match` over its own `#[non_exhaustive]` enums can be exhaustive). Until User-flow moves it
//! there, the Windows host compiles this same file (`#[path]`), so both hosts share one source.

use burnburn_document::process::ProcessLabel;
use burnburn_document::sketch::SketchLabel;
use burnburn_document::{EditLabel, UndoLabel};
use burnburn_editor::{IssueKind, SnapKind};
use burnburn_presentation::{
    CommandKey, Feature, FieldError, FieldKey, MachineAction, MachineStatusKey, MessageKey, OperationField, Outcome,
    SketchConstraintKind, ToolRejection,
};

/// UI language. Physical values never depend on it (fields accept both decimal signs, DR-05).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Locale {
    Nl,
    En,
}

impl Locale {
    /// Decimal sign used when the host formats a number for display only.
    pub fn decimal_sign(self) -> char {
        match self {
            Locale::Nl => ',',
            Locale::En => '.',
        }
    }
}

/// The first preferred language we ship (`nl`/`nld`, `en`/`eng`), in the OS's order; English
/// when none matches. `preferred` are BCP-47-like tags as `NSLocale.preferredLanguages` or
/// `GetUserPreferredUILanguages` return them (`nl-BE`, `en_US`, …).
pub fn locale_from_preferred<'a>(preferred: impl IntoIterator<Item = &'a str>) -> Locale {
    for tag in preferred {
        let language = tag.split(['-', '_']).next().unwrap_or("").to_ascii_lowercase();
        match language.as_str() {
            "nl" | "nld" | "dut" => return Locale::Nl,
            "en" | "eng" => return Locale::En,
            _ => {}
        }
    }
    Locale::En
}

/// Texts only hosts show (presentation has no key for them).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HostText {
    AppMenu,
    Quit,
    About,
    Hide,
    MenuFile,
    MenuEdit,
    MenuTools,
    MenuArrange,
    MenuProduction,
    MenuWindow,
    Open,
    SelectAll,
    ObjectsColumn,
    CloseAlertInfo,
    CloseSave,
    CloseCancel,
    CloseDiscard,
    ToolbarTools,
    Unit,
    PlanTitle,
    NoPlan,
    NoPlanHint,
    PlanCurrent,
    PlanStale,
    PlanEstimateUnknown,
    CompileButton,
    ExportButton,
    CanvasLabel,
    InspectorLabel,
    ObjectListLabel,
    Selected,
    RecoveryTitle,
    CameraUsage,
    Import,
    ImportTitle,
    ImportUnitLabel,
    ImportCommit,
    ImportCancel,
    MachineTitle,
    NoMachine,
    UnitDegrees,
    OperationsTitle,
    OperationName,
    OperationKind,
    OperationSpeed,
    OperationPower,
    OperationTargets,
    OperationAdd,
    OperationKindLine,
    OperationKindFill,
    OperationKindOther,
    NoOperations,
    OperationKindOffsetFill,
    PaletteLabel,
    PaletteNewLayer,
    LayerColumn,
    OperationPasses,
    OperationOutput,
    OperationVisible,
    CutSettingsButton,
    CutSettingsTitle,
    CutSettingsMode,
    CutSettingsSpeed,
    CutSettingsPowerMin,
    CutSettingsPowerMax,
    CutSettingsPasses,
    CutSettingsKerf,
    CutSettingsOutput,
    CutSettingsVisible,
    CutSettingsOk,
    CutSettingsCancel,
    CameraTitle,
    CameraNoPort,
    CameraNotDetermined,
    CameraDenied,
    CameraRestricted,
    CameraAuthorized,
    CameraRunning,
    CameraNoDevices,
    CameraNoFrame,
    CameraAccessButton,
    CameraStartButton,
    CameraStopButton,
    RecoveryRestore,
    RecoveryLater,
    RecoveryDiscard,
    MenuSketch,
    SketchPanelTitle,
    SketchColumnKind,
    SketchColumnEntities,
    SketchColumnValue,
    SketchColumnStatus,
    SketchFreedom,
    SketchFullyConstrained,
    SketchRemove,
    SketchToggleRole,
    SketchParametersTitle,
    SketchParameterName,
    SketchParameterValue,
    SketchParameterSet,
    SketchProblemHint,
    SketchHint,
    SketchPending,
    SketchHealthSatisfied,
    SketchHealthRedundant,
    SketchHealthConflicting,
    SketchHealthUnmet,
    SketchHealthInvalid,
    SketchHealthMeasured,
    SketchHealthUnknown,
    SketchRoleDriving,
    SketchRoleDriven,
    SketchNoConstraints,
}

impl HostText {
    pub const ALL: &'static [HostText] = &[
        HostText::AppMenu,
        HostText::Quit,
        HostText::About,
        HostText::Hide,
        HostText::MenuFile,
        HostText::MenuEdit,
        HostText::MenuTools,
        HostText::MenuArrange,
        HostText::MenuProduction,
        HostText::MenuWindow,
        HostText::Open,
        HostText::SelectAll,
        HostText::ObjectsColumn,
        HostText::CloseAlertInfo,
        HostText::CloseSave,
        HostText::CloseCancel,
        HostText::CloseDiscard,
        HostText::ToolbarTools,
        HostText::Unit,
        HostText::PlanTitle,
        HostText::NoPlan,
        HostText::NoPlanHint,
        HostText::PlanCurrent,
        HostText::PlanStale,
        HostText::PlanEstimateUnknown,
        HostText::CompileButton,
        HostText::ExportButton,
        HostText::CanvasLabel,
        HostText::InspectorLabel,
        HostText::ObjectListLabel,
        HostText::Selected,
        HostText::RecoveryTitle,
        HostText::CameraUsage,
        HostText::Import,
        HostText::ImportTitle,
        HostText::ImportUnitLabel,
        HostText::ImportCommit,
        HostText::ImportCancel,
        HostText::MachineTitle,
        HostText::NoMachine,
        HostText::UnitDegrees,
        HostText::OperationsTitle,
        HostText::OperationName,
        HostText::OperationKind,
        HostText::OperationSpeed,
        HostText::OperationPower,
        HostText::OperationTargets,
        HostText::OperationAdd,
        HostText::OperationKindLine,
        HostText::OperationKindFill,
        HostText::OperationKindOther,
        HostText::NoOperations,
        HostText::OperationKindOffsetFill,
        HostText::PaletteLabel,
        HostText::PaletteNewLayer,
        HostText::LayerColumn,
        HostText::OperationPasses,
        HostText::OperationOutput,
        HostText::OperationVisible,
        HostText::CutSettingsButton,
        HostText::CutSettingsTitle,
        HostText::CutSettingsMode,
        HostText::CutSettingsSpeed,
        HostText::CutSettingsPowerMin,
        HostText::CutSettingsPowerMax,
        HostText::CutSettingsPasses,
        HostText::CutSettingsKerf,
        HostText::CutSettingsOutput,
        HostText::CutSettingsVisible,
        HostText::CutSettingsOk,
        HostText::CutSettingsCancel,
        HostText::CameraTitle,
        HostText::CameraNoPort,
        HostText::CameraNotDetermined,
        HostText::CameraDenied,
        HostText::CameraRestricted,
        HostText::CameraAuthorized,
        HostText::CameraRunning,
        HostText::CameraNoDevices,
        HostText::CameraNoFrame,
        HostText::CameraAccessButton,
        HostText::CameraStartButton,
        HostText::CameraStopButton,
        HostText::RecoveryRestore,
        HostText::RecoveryLater,
        HostText::RecoveryDiscard,
        HostText::MenuSketch,
        HostText::SketchPanelTitle,
        HostText::SketchColumnKind,
        HostText::SketchColumnEntities,
        HostText::SketchColumnValue,
        HostText::SketchColumnStatus,
        HostText::SketchFreedom,
        HostText::SketchFullyConstrained,
        HostText::SketchRemove,
        HostText::SketchToggleRole,
        HostText::SketchParametersTitle,
        HostText::SketchParameterName,
        HostText::SketchParameterValue,
        HostText::SketchParameterSet,
        HostText::SketchProblemHint,
        HostText::SketchHint,
        HostText::SketchPending,
        HostText::SketchHealthSatisfied,
        HostText::SketchHealthRedundant,
        HostText::SketchHealthConflicting,
        HostText::SketchHealthUnmet,
        HostText::SketchHealthInvalid,
        HostText::SketchHealthMeasured,
        HostText::SketchHealthUnknown,
        HostText::SketchRoleDriving,
        HostText::SketchRoleDriven,
        HostText::SketchNoConstraints,
    ];
}

/// `(nl, en)` for a host text.
fn host_pair(text: HostText) -> (&'static str, &'static str) {
    use HostText as T;
    match text {
        T::AppMenu => ("BurnBurn", "BurnBurn"),
        T::Quit => ("Stop BurnBurn", "Quit BurnBurn"),
        T::About => ("Over BurnBurn", "About BurnBurn"),
        T::Hide => ("Verberg BurnBurn", "Hide BurnBurn"),
        T::MenuFile => ("Archief", "File"),
        T::MenuEdit => ("Wijzig", "Edit"),
        T::MenuTools => ("Gereedschap", "Tools"),
        T::MenuArrange => ("Schikken", "Arrange"),
        T::MenuProduction => ("Productie", "Production"),
        T::MenuWindow => ("Venster", "Window"),
        T::Open => ("Open…", "Open…"),
        T::SelectAll => ("Selecteer alles", "Select All"),
        T::ObjectsColumn => ("Objecten", "Objects"),
        T::CloseAlertInfo => (
            "Je wijzigingen gaan verloren als je ze niet bewaart.",
            "Your changes will be lost if you don’t save them.",
        ),
        T::CloseSave => ("Bewaar", "Save"),
        T::CloseCancel => ("Annuleer", "Cancel"),
        T::CloseDiscard => ("Niet bewaren", "Don’t Save"),
        T::ToolbarTools => ("Gereedschap", "Tools"),
        T::Unit => ("mm", "mm"),
        T::PlanTitle => ("Voorbeeld van het plan", "Plan preview"),
        T::NoPlan => ("Geen plan", "No plan"),
        T::NoPlanHint => {
            ("Compileer om het plan en de geschatte tijd te zien.", "Compile to see the plan and its estimated time.")
        }
        T::PlanCurrent => ("Actueel", "Current"),
        T::PlanStale => (
            "Verouderd: het ontwerp of de machine is gewijzigd sinds het compileren",
            "Out of date: the design or the machine changed since compiling",
        ),
        T::PlanEstimateUnknown => ("onbekend", "unknown"),
        T::CompileButton => ("Compileer", "Compile"),
        T::ExportButton => ("Exporteer RD…", "Export RD…"),
        T::CanvasLabel => ("Ontwerpcanvas", "Design canvas"),
        T::InspectorLabel => ("Inspecteur", "Inspector"),
        T::ObjectListLabel => ("Objectenlijst", "Object list"),
        T::Selected => ("geselecteerd", "selected"),
        T::RecoveryTitle => ("Herstelde versie", "Recovered version"),
        T::CameraUsage => (
            "BurnBurn gebruikt de camera alleen als plaatsingshulp boven het laserbed.",
            "BurnBurn uses the camera only as a placement aid above the laser bed.",
        ),
        T::Import => ("Importeer…", "Import…"),
        T::ImportTitle => ("Import", "Import"),
        T::ImportUnitLabel => ("Eenheid van de tekening", "Drawing unit"),
        T::ImportCommit => ("Importeer", "Import"),
        T::ImportCancel => ("Annuleer", "Cancel"),
        T::MachineTitle => ("Machine", "Machine"),
        T::NoMachine => ("Geen machine verbonden", "No machine connected"),
        T::UnitDegrees => ("°", "°"),
        T::OperationsTitle => ("Bewerkingen", "Operations"),
        T::OperationName => ("Naam", "Name"),
        T::OperationKind => ("Soort", "Kind"),
        T::OperationSpeed => ("Snelheid (mm/s)", "Speed (mm/s)"),
        T::OperationPower => ("Vermogen (%)", "Power (%)"),
        T::OperationTargets => ("Objecten", "Objects"),
        T::OperationAdd => ("Voeg toe voor selectie", "Add for Selection"),
        T::OperationKindLine => ("Lijn", "Line"),
        T::OperationKindFill => ("Vulling", "Fill"),
        T::OperationKindOther => ("Andere", "Other"),
        T::NoOperations => (
            "Nog geen bewerkingen: selecteer objecten en voeg er een toe.",
            "No operations yet: select objects and add one.",
        ),
        T::OperationKindOffsetFill => ("Offsetvulling", "Offset Fill"),
        T::PaletteLabel => (
            "Kleurenpalet: klik een kleur om de selectie aan die laag te geven",
            "Colour palette: click a colour to put the selection on that layer",
        ),
        T::PaletteNewLayer => (
            "nog leeg — klik met een selectie om de laag te maken",
            "empty — click with a selection to create the layer",
        ),
        T::LayerColumn => ("Laag", "Layer"),
        T::OperationPasses => ("Doorgangen", "Passes"),
        T::OperationOutput => ("Uitvoer", "Output"),
        T::OperationVisible => ("Tonen", "Show"),
        T::CutSettingsButton => ("Snij-instellingen…", "Cut Settings…"),
        T::CutSettingsTitle => ("Snij-instellingen", "Cut Settings"),
        T::CutSettingsMode => ("Modus", "Mode"),
        T::CutSettingsSpeed => ("Snelheid (mm/s)", "Speed (mm/s)"),
        T::CutSettingsPowerMin => ("Min. vermogen (%)", "Min power (%)"),
        T::CutSettingsPowerMax => ("Max. vermogen (%)", "Max power (%)"),
        T::CutSettingsPasses => ("Doorgangen", "Passes"),
        T::CutSettingsKerf => ("Kerfcompensatie (mm)", "Kerf offset (mm)"),
        T::CutSettingsOutput => ("Uitvoer aan (komt in de job)", "Output enabled (goes into the job)"),
        T::CutSettingsVisible => ("Zichtbaar op het canvas", "Visible on the canvas"),
        T::CutSettingsOk => ("OK", "OK"),
        T::CutSettingsCancel => ("Annuleer", "Cancel"),
        T::CameraTitle => ("Bedcamera", "Bed camera"),
        T::CameraNoPort => ("Geen camera beschikbaar in deze versie", "No camera available in this build"),
        T::CameraNotDetermined => ("Nog geen toegang gevraagd", "Access not requested yet"),
        T::CameraDenied => (
            "Toegang geweigerd — wijzig dit in Systeeminstellingen › Privacy",
            "Access denied — change this in System Settings › Privacy",
        ),
        T::CameraRestricted => ("Camera niet toegestaan op deze Mac", "Camera not allowed on this Mac"),
        T::CameraAuthorized => ("Toegang verleend — camera staat uit", "Access granted — camera is off"),
        T::CameraRunning => ("Camera loopt", "Camera running"),
        T::CameraNoDevices => ("Geen camera gevonden", "No camera found"),
        T::CameraNoFrame => ("Nog geen beeld", "No image yet"),
        T::CameraAccessButton => ("Vraag toegang…", "Request Access…"),
        T::CameraStartButton => ("Start camera", "Start Camera"),
        T::CameraStopButton => ("Stop camera", "Stop Camera"),
        T::RecoveryRestore => ("Herstel", "Restore"),
        T::RecoveryLater => ("Later", "Later"),
        T::RecoveryDiscard => ("Verwerp", "Discard"),
        T::MenuSketch => ("Schets", "Sketch"),
        T::SketchPanelTitle => ("Schets", "Sketch"),
        T::SketchColumnKind => ("Relatie", "Constraint"),
        T::SketchColumnEntities => ("Elementen", "Elements"),
        T::SketchColumnValue => ("Waarde", "Value"),
        T::SketchColumnStatus => ("Status", "Status"),
        T::SketchFreedom => ("Vrijheidsgraden", "Degrees of freedom"),
        T::SketchFullyConstrained => ("Volledig bepaald", "Fully constrained"),
        T::SketchRemove => ("Verwijder relatie", "Remove Constraint"),
        T::SketchToggleRole => ("Meten/Sturen", "Measure/Drive"),
        T::SketchParametersTitle => ("Parameters", "Parameters"),
        T::SketchParameterName => ("Naam", "Name"),
        T::SketchParameterValue => (
            "Waarde of formule (bv. 3,1 mm of dikte + speling)",
            "Value or formula (e.g. 3.1 mm or thickness + clearance)",
        ),
        T::SketchParameterSet => ("Zet parameter", "Set Parameter"),
        T::SketchProblemHint => {
            ("Klik om de betrokken relaties te selecteren.", "Click to select the constraints involved.")
        }
        T::SketchHint => (
            "Cmd+Shift bij het loslaten onderdrukt de voorgestelde relaties.",
            "Cmd+Shift on release suppresses the suggested relations.",
        ),
        T::SketchPending => ("Nog niet doorgerekend", "Not solved yet"),
        T::SketchHealthSatisfied => ("✓ voldaan", "✓ satisfied"),
        T::SketchHealthRedundant => ("= overbodig", "= redundant"),
        T::SketchHealthConflicting => ("✗ conflict", "✗ conflict"),
        T::SketchHealthUnmet => ("✗ niet bereikt", "✗ not reached"),
        T::SketchHealthInvalid => ("✗ ongeldig", "✗ invalid"),
        T::SketchHealthMeasured => ("( ) gemeten", "( ) measured"),
        T::SketchHealthUnknown => ("—", "—"),
        T::SketchRoleDriving => ("sturend", "driving"),
        T::SketchRoleDriven => ("meetend", "driven"),
        T::SketchNoConstraints => (
            "Nog geen relaties of maten: teken, selecteer en kies een relatie.",
            "No constraints yet: draw, select and choose a relation.",
        ),
    }
}

/// The machine status in words. Only `Completed` (reported by the machine) may sound finished:
/// an upload ACK is "uploaded", never "klaar" (AGENTS.md, contract §9).
pub fn machine_status_text(status: MachineStatusKey, locale: Locale) -> &'static str {
    use MachineStatusKey as S;
    let (nl, en) = match status {
        S::Idle => ("Geen opdracht", "No job"),
        S::VerifyingIdentity => ("Machine-identiteit wordt gecontroleerd…", "Checking the machine identity…"),
        S::Reverifying => ("Eerdere upload wordt opnieuw gecontroleerd…", "Re-checking an earlier upload…"),
        S::Uploading => ("Uploaden…", "Uploading…"),
        S::UploadedNotConfirmed => ("Geüpload — opslag nog niet bevestigd", "Uploaded — storage not confirmed"),
        S::Stored => ("Opgeslagen (bevestigd door de controller)", "Stored (confirmed by the controller)"),
        S::StartRequested => ("Start verzonden", "Start sent"),
        S::Running => ("Bezig (gemeld door de machine)", "Running (reported by the machine)"),
        S::Completed => ("Voltooid (gemeld door de machine)", "Completed (reported by the machine)"),
        S::Failed => ("Mislukt", "Failed"),
        S::Cancelled => ("Geannuleerd", "Cancelled"),
        S::Unknown => ("Onbekend — controleer de machine", "Unknown — check the machine"),
    };
    match locale {
        Locale::Nl => nl,
        Locale::En => en,
    }
}

pub fn host_text(text: HostText, locale: Locale) -> &'static str {
    let (nl, en) = host_pair(text);
    match locale {
        Locale::Nl => nl,
        Locale::En => en,
    }
}

/// Message text of the "save changes?" sheet.
pub fn close_alert_title(title: &str, locale: Locale) -> String {
    match locale {
        Locale::Nl => format!("Wil je de wijzigingen in “{title}” bewaren?"),
        Locale::En => format!("Do you want to save the changes made to “{title}”?"),
    }
}

fn command_pair(key: CommandKey) -> Option<(&'static str, &'static str)> {
    use CommandKey as C;
    Some(match key {
        C::New => ("Nieuw", "New"),
        C::Save => ("Bewaar", "Save"),
        C::SaveAs => ("Bewaar als…", "Save As…"),
        C::Close => ("Sluit", "Close"),
        C::Undo => ("Herstel", "Undo"),
        C::Redo => ("Opnieuw", "Redo"),
        C::Delete => ("Verwijder", "Delete"),
        C::SelectAll => ("Selecteer alles", "Select All"),
        C::ToolSelect => ("Selecteer", "Select"),
        C::ToolNode => ("Knooppunten", "Nodes"),
        C::ToolRectangle => ("Rechthoek", "Rectangle"),
        C::ToolEllipse => ("Ellips", "Ellipse"),
        C::ToolCircle => ("Cirkel", "Circle"),
        C::ToolLine => ("Lijn", "Line"),
        C::ToolPolyline => ("Polylijn", "Polyline"),
        C::ToolPen => ("Pen", "Pen"),
        C::SnapToObjects => ("Kleef aan objecten", "Snap to Objects"),
        C::SnapToGrid => ("Kleef aan raster", "Snap to Grid"),
        C::AlignLeft => ("Lijn links uit", "Align Left"),
        C::AlignRight => ("Lijn rechts uit", "Align Right"),
        C::AlignTop => ("Lijn boven uit", "Align Top"),
        C::AlignBottom => ("Lijn onder uit", "Align Bottom"),
        C::AlignCenterX => ("Centreer horizontaal", "Center Horizontally"),
        C::AlignCenterY => ("Centreer verticaal", "Center Vertically"),
        C::DistributeHorizontal => ("Verdeel horizontaal", "Distribute Horizontally"),
        C::DistributeVertical => ("Verdeel verticaal", "Distribute Vertically"),
        C::BooleanUnion => ("Verenig", "Union"),
        C::BooleanDifference => ("Trek af", "Subtract"),
        C::BooleanIntersection => ("Doorsnede", "Intersect"),
        C::ApplyPreview => ("Pas toe", "Apply"),
        C::Measure => ("Meet document", "Measure Document"),
        C::Compile => ("Compileer plan", "Compile Plan"),
        C::ExportRd => ("Exporteer RD…", "Export RD…"),
        C::ApplyTrace => ("Pas trace toe", "Apply Trace"),
        C::CloseTracer => ("Sluit tracer", "Close Tracer"),
        C::PrepareJob => ("Bereid job voor", "Prepare Job"),
        C::ApproveJob => ("Geef job vrij", "Approve Job"),
        C::ToolTrim => ("Inkorten", "Trim"),
        C::CommitImport => ("Importeer", "Import"),
        C::CancelImport => ("Annuleer import", "Cancel Import"),
        C::SketchNew => ("Nieuwe schets", "New Sketch"),
        C::SketchFromSelection => ("Maak schets van selectie", "Make Sketch from Selection"),
        C::SketchEdit => ("Bewerk schets", "Edit Sketch"),
        C::SketchFinish => ("Sluit schets", "Finish Sketch"),
        C::SketchCoincident => ("Samenvallend", "Coincident"),
        C::SketchHorizontal => ("Horizontaal", "Horizontal"),
        C::SketchVertical => ("Verticaal", "Vertical"),
        C::SketchParallel => ("Parallel", "Parallel"),
        C::SketchPerpendicular => ("Loodrecht", "Perpendicular"),
        C::SketchEqual => ("Gelijk", "Equal"),
        C::SketchTangent => ("Raaklijn", "Tangent"),
        C::SketchFix => ("Vast", "Fix"),
        C::SketchDimension => ("Maat", "Dimension"),
        C::SketchRemoveConstraint => ("Verwijder relatie", "Remove Constraint"),
        _ => return None,
    })
}

fn undo_pair(label: UndoLabel) -> Option<(&'static str, &'static str)> {
    Some(match label {
        UndoLabel::CreateRectangle => ("Rechthoek tekenen", "Draw Rectangle"),
        UndoLabel::SetDimension => ("Maat wijzigen", "Change Size"),
        UndoLabel::Transform => ("Verplaatsen", "Move"),
        UndoLabel::Delete => ("Verwijderen", "Delete"),
        UndoLabel::Rename => ("Naam wijzigen", "Rename"),
        UndoLabel::Edit(e) => match e {
            EditLabel::CreateShape => ("Vorm tekenen", "Draw Shape"),
            EditLabel::CreatePath => ("Pad tekenen", "Draw Path"),
            EditLabel::EditNodes => ("Knooppunten wijzigen", "Edit Nodes"),
            EditLabel::Boolean => ("Booleaanse bewerking", "Boolean Operation"),
            EditLabel::Offset => ("Verschuiven", "Offset"),
            EditLabel::Trim => ("Inkorten", "Trim"),
            EditLabel::Extend => ("Verlengen", "Extend"),
            EditLabel::CloseGaps => ("Openingen sluiten", "Close Gaps"),
            EditLabel::Align => ("Uitlijnen", "Align"),
            EditLabel::Distribute => ("Verdelen", "Distribute"),
            EditLabel::Group => ("Groeperen", "Group"),
            EditLabel::Ungroup => ("Groep opheffen", "Ungroup"),
            EditLabel::Import => ("Importeren", "Import"),
            _ => return None,
        },
        UndoLabel::Process(p) => match p {
            ProcessLabel::AddOperation => ("Bewerking toevoegen", "Add Operation"),
            ProcessLabel::UpdateOperation => ("Bewerking wijzigen", "Change Operation"),
            ProcessLabel::RemoveOperation => ("Bewerking verwijderen", "Remove Operation"),
            ProcessLabel::AssignGeometry => ("Objecten toewijzen", "Assign Objects"),
            ProcessLabel::UnassignGeometry => ("Toewijzing opheffen", "Unassign Objects"),
            ProcessLabel::AddOrderConstraint => ("Volgorde vastleggen", "Add Order Constraint"),
            ProcessLabel::RemoveOrderConstraint => ("Volgorde vrijgeven", "Remove Order Constraint"),
            ProcessLabel::AddRecipe => ("Recept toevoegen", "Add Recipe"),
            ProcessLabel::UpdateRecipe => ("Recept wijzigen", "Change Recipe"),
            ProcessLabel::ApplyRecipe => ("Recept toepassen", "Apply Recipe"),
            _ => return None,
        },
        UndoLabel::Sketch(s) => match s {
            SketchLabel::Create => ("Schets maken", "Create Sketch"),
            SketchLabel::Delete => ("Schets verwijderen", "Delete Sketch"),
            SketchLabel::AddEntity => ("Schetselement toevoegen", "Add Sketch Entity"),
            SketchLabel::AddConstraint => ("Relatie toevoegen", "Add Constraint"),
            SketchLabel::RemoveConstraint => ("Relatie verwijderen", "Remove Constraint"),
            SketchLabel::SetParameter => ("Parameter wijzigen", "Change Parameter"),
            SketchLabel::RemoveParameter => ("Parameter verwijderen", "Remove Parameter"),
            SketchLabel::SetDimensionValue => ("Maat wijzigen", "Change Dimension"),
            SketchLabel::SetDimensionRole => ("Maatrol wijzigen", "Change Dimension Role"),
            SketchLabel::CreateFrom => ("Schets maken van selectie", "Make Sketch from Selection"),
            SketchLabel::Batch => ("Schets bewerken", "Edit Sketch"),
            _ => return None,
        },
        _ => return None,
    })
}

fn field_pair(key: FieldKey) -> Option<(&'static str, &'static str)> {
    Some(match key {
        FieldKey::X => ("X", "X"),
        FieldKey::Y => ("Y", "Y"),
        FieldKey::Width => ("Breedte", "Width"),
        FieldKey::Height => ("Hoogte", "Height"),
        FieldKey::Rotation => ("Rotatie", "Rotation"),
        _ => return None,
    })
}

fn pick(pair: (&str, &str), locale: Locale) -> String {
    match locale {
        Locale::Nl => pair.0.to_owned(),
        Locale::En => pair.1.to_owned(),
    }
}

/// The text for a view-model message; `None` for a key this catalogue does not know yet (a test
/// failure for every key in [`known_message_keys`]).
pub fn message(key: MessageKey, locale: Locale) -> Option<String> {
    use MessageKey as M;
    let pair: (&str, &str) = match key {
        M::Command(k) => command_pair(k)?,
        M::UndoAction(l) => {
            let (nl, en) = undo_pair(l)?;
            return Some(match locale {
                Locale::Nl => format!("Herstel {nl}"),
                Locale::En => format!("Undo {en}"),
            });
        }
        M::RedoAction(l) => {
            let (nl, en) = undo_pair(l)?;
            return Some(match locale {
                Locale::Nl => format!("Opnieuw {nl}"),
                Locale::En => format!("Redo {en}"),
            });
        }
        M::Field(f) => field_pair(f)?,
        M::InvalidLength(e) => match e {
            FieldError::Empty => ("Vul een maat in.", "Enter a size."),
            FieldError::NotANumber => ("Geen geldig getal.", "Not a valid number."),
            FieldError::AmbiguousSeparator => ("Gebruik één decimaalteken.", "Use a single decimal separator."),
            FieldError::NotFinite => ("De maat is te groot.", "The size is too large."),
            FieldError::NotPositive => ("De maat moet groter dan 0 zijn.", "The size must be greater than 0."),
        },
        M::CommandRejected => ("De wijziging is geweigerd.", "The change was rejected."),
        M::SaveFailed => ("Bewaren is mislukt.", "Saving failed."),
        M::OpenFailed => ("Openen is mislukt.", "Opening failed."),
        M::Snap(s) => match s {
            SnapKind::Endpoint => ("Eindpunt", "Endpoint"),
            SnapKind::Intersection => ("Snijpunt", "Intersection"),
            SnapKind::Midpoint => ("Midden", "Midpoint"),
            SnapKind::Center => ("Centrum", "Center"),
            SnapKind::Nearest => ("Op de lijn", "On Path"),
            SnapKind::Grid => ("Raster", "Grid"),
            _ => return None,
        },
        M::Issue(i) => match i {
            IssueKind::OpenContour => ("Open contour", "Open contour"),
            IssueKind::SelfIntersection => ("Zelfdoorsnijding", "Self-intersection"),
            _ => return None,
        },
        M::BooleanFailed => ("De booleaanse bewerking is mislukt.", "The boolean operation failed."),
        M::NoMachineProfile => ("Kies eerst een machineprofiel.", "Choose a machine profile first."),
        M::CompileFailed => ("Compileren is mislukt.", "Compiling failed."),
        M::ExportFailed => ("Exporteren is mislukt.", "Exporting failed."),
        M::ImageFailed => ("Het beeld kon niet gelezen worden.", "The image could not be read."),
        M::TraceFailed => ("Traceren is mislukt.", "Tracing failed."),
        M::MachineAction(a) => match a {
            MachineAction::Upload => ("Upload naar machine", "Upload to Machine"),
            MachineAction::ConfirmStored => ("Controleer opslag", "Check Storage"),
            MachineAction::Start => ("Start", "Start"),
            MachineAction::QueryStatus => ("Vraag status", "Query Status"),
            MachineAction::Cancel => ("Software-stop", "Software Stop"),
            MachineAction::Reconnect => ("Verbind opnieuw", "Reconnect"),
            MachineAction::RecordOutcome(Outcome::RanToCompletion) => {
                ("Registreer uitkomst: voltooid", "Record Outcome: Completed")
            }
            MachineAction::RecordOutcome(Outcome::DidNotRun) => {
                ("Registreer uitkomst: niet gelopen", "Record Outcome: Did Not Run")
            }
            MachineAction::RecordOutcome(Outcome::Partial) => {
                ("Registreer uitkomst: gedeeltelijk", "Record Outcome: Partial")
            }
            _ => return None,
        },
        M::PrepareFailed => ("De job kon niet voorbereid worden.", "The job could not be prepared."),
        M::DesignChangedSinceJob => (
            "Ontwerp gewijzigd sinds de job – nieuwe conceptversie.",
            "Design changed since the job – new draft version.",
        ),
        M::UploadedNotStoredConfirmed => {
            ("Geüpload, nog niet opgeslagen bevestigd.", "Uploaded, storage not yet confirmed.")
        }
        M::StartRequestedAwaitingStatus => ("Start verzonden; wacht op status.", "Start sent; waiting for status."),
        M::StatusUnknownCheckMachine => {
            ("Status onbekend — controleer de machine.", "Status unknown — check the machine.")
        }
        M::StopSentOutcomeUnknown => (
            "Software-stop verzonden; uitkomst onbekend — controleer de machine. De hardware-noodstop blijft leidend.",
            "Software stop sent; outcome unknown — check the machine. The hardware emergency stop stays authoritative.",
        ),
        M::UploadUnconfirmedCheckController => (
            "Upload onbevestigd — controleer de controller (er wordt niet automatisch opnieuw geüpload).",
            "Upload unconfirmed — check the controller (no automatic second upload).",
        ),
        M::ControllerFileMayRemain => (
            "Een (gedeeltelijk) bestand kan op de controller achtergebleven zijn; het wordt niet verwijderd.",
            "A (partial) file may remain on the controller; it is not deleted.",
        ),
        M::UploadRejected => ("De controller weigerde de upload.", "The controller rejected the upload."),
        M::IdentityMismatch => (
            "Deze controller is niet de machine van het profiel: er is niets verzonden.",
            "This controller is not the profile's machine: nothing was sent.",
        ),
        M::IdentityUnconfirmed => (
            "De identiteit van de controller is niet bevestigd: er is niets verzonden.",
            "The controller's identity is not confirmed: nothing was sent.",
        ),
        M::NotStored => {
            ("De controller meldt de job niet als opgeslagen.", "The controller does not report the job as stored.")
        }
        M::ControllerFault => ("De controller meldt een fout.", "The controller reports a fault."),
        M::MachineActionRefused => (
            "Deze machineactie kan nu niet (de toestand is intussen veranderd).",
            "This machine action is not possible now (the state has changed).",
        ),
        M::IntentNotRecorded => (
            "De actie kon niet in de historie vastgelegd worden; er is niets verzonden.",
            "The action could not be recorded in the history; nothing was sent.",
        ),
        M::ImportFailed => ("Importeren is mislukt.", "Importing failed."),
        M::OffsetImpossible => ("Deze verschuiving is niet mogelijk.", "This offset is not possible."),
        M::TrimImpossible => ("Inkorten is hier niet mogelijk.", "Trimming is not possible here."),
        M::NothingToRepair => ("Er is niets te herstellen.", "There is nothing to repair."),
        M::NotAllowedForRole => ("Dit mag niet in deze rol.", "Not allowed in this role."),
        M::SaveNotConfirmedDurable => (
            "Bewaard, maar het systeem bevestigde niet dat het bestand veilig op schijf staat.",
            "Saved, but the system did not confirm the file is safely on disk.",
        ),
        M::ToolRejected(r) => match r {
            ToolRejection::NothingPicked => ("Hier is niets om te kiezen.", "Nothing to pick here."),
            ToolRejection::DerivedGeometry => (
                "Dit komt uit een schets of generator; pas de bron aan.",
                "This comes from a sketch or generator; edit its source.",
            ),
            ToolRejection::NeedsSelection => ("Selecteer eerst iets.", "Select something first."),
            ToolRejection::InvalidSelection => {
                ("Deze selectie past niet bij dit gereedschap.", "This selection does not fit the tool.")
            }
            ToolRejection::NothingToConstrain => ("Er is niets om te beperken.", "Nothing to constrain."),
            ToolRejection::InvalidValue => ("Ongeldige waarde.", "Invalid value."),
            ToolRejection::Corner => ("Deze hoek kan niet worden bewerkt.", "This corner cannot be modified."),
            ToolRejection::Trim => ("Inkorten is hier niet mogelijk.", "Trimming is not possible here."),
            ToolRejection::Extend => ("Verlengen is hier niet mogelijk.", "Extending is not possible here."),
            ToolRejection::Break => ("Splitsen is hier niet mogelijk.", "Breaking is not possible here."),
            ToolRejection::Offset => ("Deze verschuiving is niet mogelijk.", "This offset is not possible."),
            ToolRejection::ConstructionOutsideSketch => (
                "Nog niet beschikbaar: hulplijnen bestaan alleen in een schets.",
                "Not available yet: construction geometry exists only in a sketch.",
            ),
            ToolRejection::CurvedCorner => (
                "Nog niet beschikbaar: afronden en afschuinen vragen twee rechte zijden.",
                "Not available yet: fillet and chamfer need two straight sides.",
            ),
            ToolRejection::BezierExtend => (
                "Nog niet beschikbaar: alleen lijnen en bogen kunnen worden verlengd.",
                "Not available yet: only lines and arcs can be extended.",
            ),
            _ => ("Nog niet beschikbaar.", "Not available yet."),
        },
        M::NoFontAvailable => {
            ("Er is geen lettertype beschikbaar om tekst te plaatsen.", "No font is available to place text.")
        }
        M::InvalidNumber(f) => match f {
            OperationField::Name => ("Geef de bewerking een naam.", "Give the operation a name."),
            OperationField::Speed => {
                ("Geen geldige snelheid (mm/s, groter dan 0).", "Not a valid speed (mm/s, greater than 0).")
            }
            OperationField::Power => ("Geen geldig vermogen (0–100 %).", "Not a valid power (0–100 %)."),
            OperationField::PowerMin => (
                "Geen geldig minimumvermogen (0–100 %, niet boven het maximum).",
                "Not a valid minimum power (0–100 %, not above the maximum).",
            ),
            OperationField::PowerMax => {
                ("Geen geldig maximumvermogen (0–100 %).", "Not a valid maximum power (0–100 %).")
            }
            OperationField::Passes => (
                "Geen geldig aantal doorgangen (geheel getal 1–1000).",
                "Not a valid number of passes (whole number 1–1000).",
            ),
            OperationField::Kerf => (
                "Geen geldige kerf (een maat van 0 of meer, bv. 0,15 mm).",
                "Not a valid kerf (a size of 0 or more, e.g. 0.15 mm).",
            ),
            _ => return None,
        },
        M::SelectObjectsFirst => {
            ("Selecteer eerst objecten en klik dan een kleur.", "Select objects first, then click a colour.")
        }
        M::MissingCapability(feature) => {
            let (nl, en) = feature_pair(feature)?;
            return Some(match locale {
                Locale::Nl => {
                    format!("Ontbrekende capability: {nl} (niet ondersteund of niet gevalideerd voor deze machine)")
                }
                Locale::En => format!("Missing capability: {en} (not supported or not validated for this machine)"),
            });
        }
        M::CameraNotAuthorized => {
            ("Geen toegang tot de camera: vraag eerst toegang.", "No access to the camera: request access first.")
        }
        M::SketchConstraint(k) => match k {
            SketchConstraintKind::Coincident => ("Samenvallend", "Coincident"),
            SketchConstraintKind::PointOn => ("Punt op curve", "Point on Curve"),
            SketchConstraintKind::Horizontal => ("Horizontaal", "Horizontal"),
            SketchConstraintKind::Vertical => ("Verticaal", "Vertical"),
            SketchConstraintKind::Parallel => ("Parallel", "Parallel"),
            SketchConstraintKind::Perpendicular => ("Loodrecht", "Perpendicular"),
            SketchConstraintKind::Tangent => ("Raaklijn", "Tangent"),
            SketchConstraintKind::Equal => ("Gelijk", "Equal"),
            SketchConstraintKind::Midpoint => ("Midden", "Midpoint"),
            SketchConstraintKind::Symmetric => ("Symmetrisch", "Symmetric"),
            SketchConstraintKind::Concentric => ("Concentrisch", "Concentric"),
            SketchConstraintKind::Fixed => ("Vast", "Fixed"),
            SketchConstraintKind::Length => ("Lengte", "Length"),
            SketchConstraintKind::Distance => ("Afstand", "Distance"),
            SketchConstraintKind::DistanceX => ("Afstand X", "Distance X"),
            SketchConstraintKind::DistanceY => ("Afstand Y", "Distance Y"),
            SketchConstraintKind::Radius => ("Straal", "Radius"),
            SketchConstraintKind::Diameter => ("Diameter", "Diameter"),
            SketchConstraintKind::Angle => ("Hoek", "Angle"),
            _ => return None,
        },
        M::SketchConflict => (
            "De schets heeft tegenstrijdige relaties; de laatste geldige vorm blijft staan.",
            "The sketch has contradicting constraints; the last valid shape stays.",
        ),
        M::SketchNotConverged => (
            "De schets kon niet opgelost worden; de laatste geldige vorm blijft staan.",
            "The sketch could not be solved; the last valid shape stays.",
        ),
        M::SketchInvalidDimensions => {
            ("Een maat is ongeldig (eenheid of tekst).", "A dimension is invalid (unit or text).")
        }
        M::SketchInvalidParameters => {
            ("Een parameter is ongeldig (eenheid, naam of cyclus).", "A parameter is invalid (unit, name or cycle).")
        }
        M::SketchInvalidDefinition => ("De schetsdefinitie is ongeldig.", "The sketch definition is invalid."),
        M::SketchSelectionDoesNotFit => (
            "Deze selectie past niet bij deze relatie of maat.",
            "This selection does not fit this constraint or dimension.",
        ),
        M::SketchNothingSelected => ("Selecteer eerst elementen van de schets.", "Select sketch elements first."),
        M::SketchShapeUnsupported => (
            "Dit kan nog niet in een schets getekend worden (alleen lijn, polylijn, rechthoek en cirkel).",
            "This cannot be drawn into a sketch yet (only line, polyline, rectangle and circle).",
        ),
        M::SketchConversionImpossible => (
            "Alleen rechte lijnen en volle cirkels kunnen een schets worden.",
            "Only straight lines and full circles can become a sketch.",
        ),
        M::SketchEditRefused => ("De schetswijziging is geweigerd.", "The sketch change was refused."),
        M::VisualPlacementOnly => (
            "Camerabeeld is alleen een visuele plaatsingshulp, geen productiegeometrie.",
            "The camera image is only a visual placement aid, not production geometry.",
        ),
        _ => return None,
    };
    Some(pick(pair, locale))
}

/// Names of the machine-profile features (capability matrix, M02).
fn feature_pair(feature: Feature) -> Option<(&'static str, &'static str)> {
    Some(match feature {
        Feature::KerfCompensation => ("kerfcompensatie", "kerf compensation"),
        Feature::OffsetFill => ("offsetvulling", "offset fill"),
        Feature::ScanAngle => ("schuine scanhoek", "angled scan lines"),
        Feature::ImageDither => ("gedithered beeld", "dithered image"),
        Feature::GrayscaleModulation => ("grijswaarden", "grayscale"),
        Feature::Tabs => ("bruggetjes (tabs)", "tabs"),
        Feature::Leads => ("in- en uitloop", "lead-in/lead-out"),
        Feature::Overcut => ("oversnijden", "overcut"),
        Feature::Perforation => ("perforatie", "perforation"),
        Feature::ZStep => ("Z-stap tussen doorgangen", "Z step between passes"),
        Feature::SubOperations => ("deelbewerkingen", "sub-operations"),
        Feature::CommonLine => ("gemeenschappelijke snijlijn", "common-line cutting"),
        Feature::DotMode => ("puntmodus (dwell)", "dot (dwell) mode"),
        Feature::CrossHatch => ("kruisarcering", "cross-hatch"),
        Feature::FloodFill => ("flood fill", "flood fill"),
        Feature::FillGrouping => ("vulgroepering", "fill grouping"),
        Feature::RampLength => ("vermogensverloop (ramp)", "power ramp"),
        _ => return None,
    })
}

/// Every message key the catalogue covers (for the completeness test and for review).
pub fn known_message_keys() -> Vec<MessageKey> {
    let mut keys: Vec<MessageKey> = CommandKey::ALL.iter().map(|k| MessageKey::Command(*k)).collect();
    let mut undo = vec![
        UndoLabel::CreateRectangle,
        UndoLabel::SetDimension,
        UndoLabel::Transform,
        UndoLabel::Delete,
        UndoLabel::Rename,
    ];
    undo.extend(
        [
            EditLabel::CreateShape,
            EditLabel::CreatePath,
            EditLabel::EditNodes,
            EditLabel::Boolean,
            EditLabel::Offset,
            EditLabel::Trim,
            EditLabel::Extend,
            EditLabel::CloseGaps,
            EditLabel::Align,
            EditLabel::Distribute,
            EditLabel::Group,
            EditLabel::Ungroup,
            EditLabel::Import,
        ]
        .map(UndoLabel::Edit),
    );
    undo.extend(
        [
            ProcessLabel::AddOperation,
            ProcessLabel::UpdateOperation,
            ProcessLabel::RemoveOperation,
            ProcessLabel::AssignGeometry,
            ProcessLabel::UnassignGeometry,
            ProcessLabel::AddOrderConstraint,
            ProcessLabel::RemoveOrderConstraint,
            ProcessLabel::AddRecipe,
            ProcessLabel::UpdateRecipe,
            ProcessLabel::ApplyRecipe,
        ]
        .map(UndoLabel::Process),
    );
    undo.extend(
        [
            SketchLabel::Create,
            SketchLabel::Delete,
            SketchLabel::AddEntity,
            SketchLabel::AddConstraint,
            SketchLabel::RemoveConstraint,
            SketchLabel::SetParameter,
            SketchLabel::RemoveParameter,
            SketchLabel::SetDimensionValue,
            SketchLabel::SetDimensionRole,
            SketchLabel::CreateFrom,
            SketchLabel::Batch,
        ]
        .map(UndoLabel::Sketch),
    );
    for l in undo {
        keys.push(MessageKey::UndoAction(l));
        keys.push(MessageKey::RedoAction(l));
    }
    keys.extend(
        [FieldKey::X, FieldKey::Y, FieldKey::Width, FieldKey::Height, FieldKey::Rotation].map(MessageKey::Field),
    );
    keys.extend([MessageKey::NoFontAvailable, MessageKey::CameraNotAuthorized, MessageKey::VisualPlacementOnly]);
    keys.extend(
        [
            OperationField::Name,
            OperationField::Speed,
            OperationField::Power,
            OperationField::PowerMin,
            OperationField::PowerMax,
            OperationField::Passes,
            OperationField::Kerf,
        ]
        .map(MessageKey::InvalidNumber),
    );
    keys.push(MessageKey::SelectObjectsFirst);
    keys.extend(
        [
            SketchConstraintKind::Coincident,
            SketchConstraintKind::PointOn,
            SketchConstraintKind::Horizontal,
            SketchConstraintKind::Vertical,
            SketchConstraintKind::Parallel,
            SketchConstraintKind::Perpendicular,
            SketchConstraintKind::Tangent,
            SketchConstraintKind::Equal,
            SketchConstraintKind::Midpoint,
            SketchConstraintKind::Symmetric,
            SketchConstraintKind::Concentric,
            SketchConstraintKind::Fixed,
            SketchConstraintKind::Length,
            SketchConstraintKind::Distance,
            SketchConstraintKind::DistanceX,
            SketchConstraintKind::DistanceY,
            SketchConstraintKind::Radius,
            SketchConstraintKind::Diameter,
            SketchConstraintKind::Angle,
        ]
        .map(MessageKey::SketchConstraint),
    );
    keys.extend([
        MessageKey::SketchConflict,
        MessageKey::SketchNotConverged,
        MessageKey::SketchInvalidDimensions,
        MessageKey::SketchInvalidParameters,
        MessageKey::SketchInvalidDefinition,
        MessageKey::SketchSelectionDoesNotFit,
        MessageKey::SketchNothingSelected,
        MessageKey::SketchShapeUnsupported,
        MessageKey::SketchConversionImpossible,
        MessageKey::SketchEditRefused,
    ]);
    keys.extend(ToolRejection::ALL.map(MessageKey::ToolRejected));
    keys.extend(Feature::ALL.map(MessageKey::MissingCapability));
    keys.extend(Outcome::ALL.map(|o| MessageKey::MachineAction(MachineAction::RecordOutcome(o))));
    keys.extend(
        [
            FieldError::Empty,
            FieldError::NotANumber,
            FieldError::AmbiguousSeparator,
            FieldError::NotFinite,
            FieldError::NotPositive,
        ]
        .map(MessageKey::InvalidLength),
    );
    keys.extend(
        [
            SnapKind::Endpoint,
            SnapKind::Intersection,
            SnapKind::Midpoint,
            SnapKind::Center,
            SnapKind::Nearest,
            SnapKind::Grid,
        ]
        .map(MessageKey::Snap),
    );
    keys.extend([IssueKind::OpenContour, IssueKind::SelfIntersection].map(MessageKey::Issue));
    keys.extend([
        MessageKey::CommandRejected,
        MessageKey::SaveFailed,
        MessageKey::OpenFailed,
        MessageKey::BooleanFailed,
        MessageKey::NoMachineProfile,
        MessageKey::CompileFailed,
        MessageKey::ExportFailed,
        MessageKey::ImageFailed,
        MessageKey::TraceFailed,
        MessageKey::SaveNotConfirmedDurable,
        MessageKey::PrepareFailed,
        MessageKey::DesignChangedSinceJob,
        MessageKey::UploadedNotStoredConfirmed,
        MessageKey::StartRequestedAwaitingStatus,
        MessageKey::StatusUnknownCheckMachine,
        MessageKey::StopSentOutcomeUnknown,
        MessageKey::UploadUnconfirmedCheckController,
        MessageKey::ControllerFileMayRemain,
        MessageKey::UploadRejected,
        MessageKey::IdentityMismatch,
        MessageKey::IdentityUnconfirmed,
        MessageKey::NotStored,
        MessageKey::ControllerFault,
        MessageKey::MachineActionRefused,
        MessageKey::IntentNotRecorded,
        MessageKey::ImportFailed,
        MessageKey::OffsetImpossible,
        MessageKey::TrimImpossible,
        MessageKey::NothingToRepair,
        MessageKey::NotAllowedForRole,
    ]);
    keys.extend(
        [
            MachineAction::Upload,
            MachineAction::ConfirmStored,
            MachineAction::Start,
            MachineAction::QueryStatus,
            MachineAction::Cancel,
            MachineAction::Reconnect,
        ]
        .map(MessageKey::MachineAction),
    );
    keys
}

/// Text for `key`, or a visible placeholder naming the key: never an empty label.
pub fn message_or_key(key: MessageKey, locale: Locale) -> String {
    message(key, locale).unwrap_or_else(|| format!("[{key:?}]"))
}
