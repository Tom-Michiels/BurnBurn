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
use burnburn_document::structure::StructureLabel;
use burnburn_document::{ArrangeLabel, CompoundLabel, EditLabel, ShapeLabel, UndoLabel};
use burnburn_editor::{IssueKind, SnapKind};
use burnburn_presentation::{
    CommandKey, Feature, FieldError, FieldKey, ImageField, ImageIssue, JogKey, LaserWhy, LiveFeature, MachineAction,
    MachineStatusKey, MessageKey, OperationField, Outcome, SketchConstraintKind, ToolRejection,
};

#[path = "l10n_schikken.rs"]
mod l10n_schikken;
pub use l10n_schikken::{
    center_problem_text, nest_skip_text, nest_unplaced_text, reshape_problem_text, tangent_problem_text,
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
    OperationKindImage,
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
    CutSettingsOptions,
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
    /// Sketch panel: "Herstel laatste geldige" (S04).
    SketchRestoreLast,
    /// Sketch panel: remove the selected parameter (S02).
    SketchParameterDelete,
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
    MenuShapes,
    GeometryJoinTitle,
    GeometryJoinLabel,
    GeometryRoundTitle,
    GeometryRoundLabel,
    GeometryOptimizeTitle,
    GeometryOptimizeLabel,
    GeometryOk,
    GeometryCancel,
    ShapeSectionTitle,
    ToolOptionsTitle,
    ParamSides,
    ParamPoints,
    ParamRatio,
    ParamCornerRadius,
    ParamColumns,
    ParamRows,
    ParamCount,
    ParamSweep,
    ParamKeepOriginal,
    ImageMenu,
    ImageMenuImport,
    ImageMenuTrace,
    ImageImportTitle,
    ImageImportPixels,
    ImageImportDpi,
    ImageImportSuggestionFile,
    ImageImportSuggestionNone,
    ImageImportSize,
    ImageImportSizeUnknown,
    ImageImportCommit,
    ImageImportCancel,
    ImageAdjustTitle,
    ImageAdjustSource,
    ImageAdjustSourceLoading,
    ImageAdjustBrightness,
    ImageAdjustContrast,
    ImageAdjustGamma,
    ImageAdjustSharpenAmount,
    ImageAdjustSharpenRadius,
    ImageAdjustGray,
    ImageAdjustCrop,
    ImageAdjustCropX,
    ImageAdjustCropY,
    ImageAdjustCropWidth,
    ImageAdjustCropHeight,
    ImageAdjustPreview,
    ImageAdjustApply,
    ImageAdjustCancel,
    ImageComputing,
    ImageNoPreview,
    GrayRec709,
    GrayRec601,
    GrayAverage,
    GrayLightness,
    GrayRed,
    GrayGreen,
    GrayBlue,
    TracerTitle,
    TracerSource,
    TracerRecipe,
    TracerMode,
    TracerModeContour,
    TracerModeCenterline,
    TracerDpi,
    TracerUseSuggestion,
    TracerThreshold,
    TracerThresholdOtsu,
    TracerThresholdFixed,
    TracerThresholdAdaptive,
    TracerThresholdValue,
    TracerInvert,
    TracerIgnore,
    TracerSmoothness,
    TracerCurves,
    TracerPresetLogo,
    TracerPresetPhoto,
    TracerPresetTechnical,
    TracerPresetHandwriting,
    TracerSummary,
    TracerNeedsScale,
    TracerApply,
    /// Tracer sheet: apply as a re-editable trace (T02).
    TracerApplyLinked,
    TracerClose,
    MenuView,
    RecentProjects,
    ClearRecent,
    MenuAlign,
    MenuDistribute,
    UnitPercent,
    AspectLock,
    ReferencePoint,
    RefPointTopLeft,
    RefPointTop,
    RefPointTopRight,
    RefPointLeft,
    RefPointCenter,
    RefPointRight,
    RefPointBottomLeft,
    RefPointBottom,
    RefPointBottomRight,
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
        HostText::OperationKindImage,
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
        HostText::CutSettingsOptions,
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
        HostText::SketchRestoreLast,
        HostText::SketchParameterDelete,
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
        HostText::MenuShapes,
        HostText::GeometryJoinTitle,
        HostText::GeometryJoinLabel,
        HostText::GeometryRoundTitle,
        HostText::GeometryRoundLabel,
        HostText::GeometryOptimizeTitle,
        HostText::GeometryOptimizeLabel,
        HostText::GeometryOk,
        HostText::GeometryCancel,
        HostText::ShapeSectionTitle,
        HostText::ToolOptionsTitle,
        HostText::ParamSides,
        HostText::ParamPoints,
        HostText::ParamRatio,
        HostText::ParamCornerRadius,
        HostText::ParamColumns,
        HostText::ParamRows,
        HostText::ParamCount,
        HostText::ParamSweep,
        HostText::ParamKeepOriginal,
        HostText::ImageMenu,
        HostText::ImageMenuImport,
        HostText::ImageMenuTrace,
        HostText::ImageImportTitle,
        HostText::ImageImportPixels,
        HostText::ImageImportDpi,
        HostText::ImageImportSuggestionFile,
        HostText::ImageImportSuggestionNone,
        HostText::ImageImportSize,
        HostText::ImageImportSizeUnknown,
        HostText::ImageImportCommit,
        HostText::ImageImportCancel,
        HostText::ImageAdjustTitle,
        HostText::ImageAdjustSource,
        HostText::ImageAdjustSourceLoading,
        HostText::ImageAdjustBrightness,
        HostText::ImageAdjustContrast,
        HostText::ImageAdjustGamma,
        HostText::ImageAdjustSharpenAmount,
        HostText::ImageAdjustSharpenRadius,
        HostText::ImageAdjustGray,
        HostText::ImageAdjustCrop,
        HostText::ImageAdjustCropX,
        HostText::ImageAdjustCropY,
        HostText::ImageAdjustCropWidth,
        HostText::ImageAdjustCropHeight,
        HostText::ImageAdjustPreview,
        HostText::ImageAdjustApply,
        HostText::ImageAdjustCancel,
        HostText::ImageComputing,
        HostText::ImageNoPreview,
        HostText::GrayRec709,
        HostText::GrayRec601,
        HostText::GrayAverage,
        HostText::GrayLightness,
        HostText::GrayRed,
        HostText::GrayGreen,
        HostText::GrayBlue,
        HostText::TracerTitle,
        HostText::TracerSource,
        HostText::TracerRecipe,
        HostText::TracerMode,
        HostText::TracerModeContour,
        HostText::TracerModeCenterline,
        HostText::TracerDpi,
        HostText::TracerUseSuggestion,
        HostText::TracerThreshold,
        HostText::TracerThresholdOtsu,
        HostText::TracerThresholdFixed,
        HostText::TracerThresholdAdaptive,
        HostText::TracerThresholdValue,
        HostText::TracerInvert,
        HostText::TracerIgnore,
        HostText::TracerSmoothness,
        HostText::TracerCurves,
        HostText::TracerPresetLogo,
        HostText::TracerPresetPhoto,
        HostText::TracerPresetTechnical,
        HostText::TracerPresetHandwriting,
        HostText::TracerSummary,
        HostText::TracerNeedsScale,
        HostText::TracerApply,
        HostText::TracerApplyLinked,
        HostText::TracerClose,
        HostText::MenuView,
        HostText::RecentProjects,
        HostText::ClearRecent,
        HostText::MenuAlign,
        HostText::MenuDistribute,
        HostText::UnitPercent,
        HostText::AspectLock,
        HostText::ReferencePoint,
        HostText::RefPointTopLeft,
        HostText::RefPointTop,
        HostText::RefPointTopRight,
        HostText::RefPointLeft,
        HostText::RefPointCenter,
        HostText::RefPointRight,
        HostText::RefPointBottomLeft,
        HostText::RefPointBottom,
        HostText::RefPointBottomRight,
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
        T::OperationKindImage => ("Afbeelding", "Image"),
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
        T::CutSettingsOptions => ("Opties van deze modus", "Options of this mode"),
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
        T::SketchRestoreLast => ("Herstel laatste geldige", "Restore last valid"),
        T::SketchParameterDelete => ("Verwijder parameter", "Remove parameter"),
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
        T::MenuShapes => ("Vormen", "Shapes"),
        T::GeometryJoinTitle => ("Paden verbinden of sluiten", "Join or Close Paths"),
        T::GeometryJoinLabel => ("Grootste opening (mm)", "Largest gap (mm)"),
        T::GeometryRoundTitle => ("Hoeken afronden", "Round Corners"),
        T::GeometryRoundLabel => ("Straal (mm)", "Radius (mm)"),
        T::GeometryOptimizeTitle => ("Vormen optimaliseren", "Optimize Shapes"),
        T::GeometryOptimizeLabel => ("Afwijking (mm)", "Error budget (mm)"),
        T::GeometryOk => ("OK", "OK"),
        T::GeometryCancel => ("Annuleer", "Cancel"),
        T::ShapeSectionTitle => ("Vorm", "Shape"),
        T::ToolOptionsTitle => ("Gereedschapsopties", "Tool options"),
        T::ParamSides => ("Zijden", "Sides"),
        T::ParamPoints => ("Punten", "Points"),
        T::ParamRatio => ("Verhouding", "Ratio"),
        T::ParamCornerRadius => ("Hoekstraal (mm)", "Corner radius (mm)"),
        T::ParamColumns => ("Kolommen", "Columns"),
        T::ParamRows => ("Rijen", "Rows"),
        T::ParamCount => ("Aantal", "Count"),
        T::ParamSweep => ("Hoek (°)", "Sweep (°)"),
        T::ParamKeepOriginal => ("Origineel behouden", "Keep original"),
        T::ImageMenu => ("Beeld", "Image"),
        T::ImageMenuImport => ("Beeld importeren…", "Import Image…"),
        T::ImageMenuTrace => ("Beeld tracen…", "Trace Image…"),
        T::ImageImportTitle => ("Beeld importeren", "Import Image"),
        T::ImageImportPixels => ("{0} × {1} pixels", "{0} × {1} pixels"),
        T::ImageImportDpi => ("Resolutie (dpi)", "Resolution (dpi)"),
        T::ImageImportSuggestionFile => ("Resolutie uit het bestand: {0} dpi", "Resolution from the file: {0} dpi"),
        T::ImageImportSuggestionNone => (
            "Het bestand geeft geen resolutie: vul die zelf in (bijvoorbeeld {0} dpi = 0,1 mm per pixel).",
            "The file gives no resolution: enter one yourself (for example {0} dpi = 0.1 mm per pixel).",
        ),
        T::ImageImportSize => ("Afmeting: {0} × {1} mm", "Size: {0} × {1} mm"),
        T::ImageImportSizeUnknown => {
            ("Afmeting: onbekend zolang de resolutie ongeldig is", "Size: unknown while the resolution is invalid")
        }
        T::ImageImportCommit => ("Importeer", "Import"),
        T::ImageImportCancel => ("Annuleer", "Cancel"),
        T::ImageAdjustTitle => ("Beeld aanpassen", "Adjust Image"),
        T::ImageAdjustSource => ("Bron: {0} × {1} pixels", "Source: {0} × {1} pixels"),
        T::ImageAdjustSourceLoading => ("Bron wordt gelezen…", "Reading the source…"),
        T::ImageAdjustBrightness => ("Helderheid (−1…1)", "Brightness (−1…1)"),
        T::ImageAdjustContrast => ("Contrast (0…10)", "Contrast (0…10)"),
        T::ImageAdjustGamma => ("Gamma (0,1…10)", "Gamma (0.1…10)"),
        T::ImageAdjustSharpenAmount => ("Verscherpen: sterkte (0…10, 0 = uit)", "Sharpen: amount (0…10, 0 = off)"),
        T::ImageAdjustSharpenRadius => ("Verscherpen: straal (mm)", "Sharpen: radius (mm)"),
        T::ImageAdjustGray => ("Grijswaarde", "Gray method"),
        T::ImageAdjustCrop => {
            ("Uitsnede in bronpixels (alles leeg = geen uitsnede)", "Crop in source pixels (all empty = no crop)")
        }
        T::ImageAdjustCropX => ("x", "x"),
        T::ImageAdjustCropY => ("y", "y"),
        T::ImageAdjustCropWidth => ("breedte", "width"),
        T::ImageAdjustCropHeight => ("hoogte", "height"),
        T::ImageAdjustPreview => ("Voorbeeld zoals de laser scant", "Preview as the laser scans"),
        T::ImageAdjustApply => ("Pas toe", "Apply"),
        T::ImageAdjustCancel => ("Annuleer", "Cancel"),
        T::ImageComputing => ("berekenen…", "computing…"),
        T::ImageNoPreview => ("Nog geen voorbeeld", "No preview yet"),
        T::GrayRec709 => ("Rec. 709 (standaard)", "Rec. 709 (default)"),
        T::GrayRec601 => ("Rec. 601", "Rec. 601"),
        T::GrayAverage => ("Gemiddelde", "Average"),
        T::GrayLightness => ("Lichtheid", "Lightness"),
        T::GrayRed => ("Alleen rood", "Red only"),
        T::GrayGreen => ("Alleen groen", "Green only"),
        T::GrayBlue => ("Alleen blauw", "Blue only"),
        T::TracerTitle => ("Beeld tracen", "Trace Image"),
        T::TracerSource => ("Bron: {0} × {1} pixels", "Source: {0} × {1} pixels"),
        T::TracerRecipe => ("Recept", "Recipe"),
        T::TracerMode => ("Soort", "Kind"),
        T::TracerModeContour => ("Contour (omtrek met gaten)", "Contour (outline with holes)"),
        T::TracerModeCenterline => ("Middellijn (open lijnen)", "Centerline (open lines)"),
        T::TracerDpi => ("Resolutie van het beeld (dpi)", "Image resolution (dpi)"),
        T::TracerUseSuggestion => ("Gebruik voorstel ({0} dpi)", "Use proposal ({0} dpi)"),
        T::TracerThreshold => ("Drempel", "Threshold"),
        T::TracerThresholdOtsu => ("Otsu (automatisch)", "Otsu (automatic)"),
        T::TracerThresholdFixed => ("Vast (0…255)", "Fixed (0…255)"),
        T::TracerThresholdAdaptive => ("Lokaal", "Local"),
        T::TracerThresholdValue => ("Drempelwaarde (0…255)", "Threshold value (0…255)"),
        T::TracerInvert => ("Keer zwart en wit om", "Invert black and white"),
        T::TracerIgnore => ("Negeer kleiner dan (mm²)", "Ignore smaller than (mm²)"),
        T::TracerSmoothness => ("Gladheid (mm)", "Smoothness (mm)"),
        T::TracerCurves => ("Curves (uit = rechte stukken)", "Curves (off = straight segments)"),
        T::TracerPresetLogo => ("Logo of silhouet", "Logo or silhouette"),
        T::TracerPresetPhoto => ("Foto", "Photograph"),
        T::TracerPresetTechnical => ("Technische tekening", "Technical drawing"),
        T::TracerPresetHandwriting => ("Handschrift en lijntekening", "Handwriting and line art"),
        T::TracerSummary => {
            ("Delen: {0}, gaten: {1}, formaat {2} × {3} mm", "Parts: {0}, holes: {1}, size {2} × {3} mm")
        }
        T::TracerNeedsScale => (
            "Schaal ontbreekt: geef de resolutie (dpi) van het beeld op.",
            "No scale yet: enter the resolution (dpi) of the image.",
        ),
        T::TracerApply => ("Pas trace toe", "Apply Trace"),
        T::TracerApplyLinked => ("Pas toe als trace", "Apply as Trace"),
        T::TracerClose => ("Sluit", "Close"),
        T::MenuView => ("Weergave", "View"),
        T::RecentProjects => ("Recente projecten", "Open Recent"),
        T::ClearRecent => ("Wis menu", "Clear Menu"),
        T::MenuAlign => ("Uitlijnen", "Align"),
        T::MenuDistribute => ("Verdelen", "Distribute"),
        T::UnitPercent => ("%", "%"),
        T::AspectLock => ("Verhouding vergrendelen", "Lock Aspect Ratio"),
        T::ReferencePoint => ("Referentiepunt", "Reference Point"),
        T::RefPointTopLeft => ("Linksboven", "Top left"),
        T::RefPointTop => ("Midden boven", "Top center"),
        T::RefPointTopRight => ("Rechtsboven", "Top right"),
        T::RefPointLeft => ("Midden links", "Middle left"),
        T::RefPointCenter => ("Midden", "Center"),
        T::RefPointRight => ("Midden rechts", "Middle right"),
        T::RefPointBottomLeft => ("Linksonder", "Bottom left"),
        T::RefPointBottom => ("Midden onder", "Bottom center"),
        T::RefPointBottomRight => ("Rechtsonder", "Bottom right"),
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
        C::ApplyTraceLinked => ("Pas toe als trace", "Apply as Trace"),
        C::TraceEdit => ("Bewerk trace…", "Edit Trace…"),
        C::CloseTracer => ("Sluit tracer", "Close Tracer"),
        C::PrepareJob => ("Bereid job voor", "Prepare Job"),
        C::ApproveJob => ("Geef job vrij", "Approve Job"),
        C::ToolTrim => ("Inkorten", "Trim"),
        C::CommitImport => ("Importeer", "Import"),
        C::CancelImport => ("Annuleer import", "Cancel Import"),
        C::AdjustImage => ("Beeld aanpassen…", "Adjust Image…"),
        C::ApplyImageAdjust => ("Pas beeldaanpassing toe", "Apply Image Adjustment"),
        C::CancelImageAdjust => ("Annuleer beeldaanpassing", "Cancel Image Adjustment"),
        C::MaskImage => ("Masker op beeld", "Mask Image"),
        C::ReplaceImage => ("Beeld vervangen…", "Replace Image…"),
        C::RefreshImage => ("Beeld vernieuwen", "Refresh Image"),
        C::SaveProcessedBitmap => ("Verwerkte bitmap bewaren…", "Save Processed Bitmap…"),
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
        C::ShapesJoin => ("Verbind of sluit paden…", "Join or Close Paths…"),
        C::ShapesCut => ("Snij vormen", "Cut Shapes"),
        C::ShapesRoundCorners => ("Hoeken afronden…", "Round Corners…"),
        C::ShapesOptimize => ("Vormen optimaliseren…", "Optimize Shapes…"),
        C::ShapesReverse => ("Richting omkeren", "Reverse Direction"),
        C::ShapesClockwise => ("Met de klok mee", "Clockwise"),
        C::ShapesCounterClockwise => ("Tegen de klok in", "Counter-clockwise"),
        C::ShapesConvertToPath => ("Omzetten naar pad", "Convert to Path"),
        C::NodeDelete => ("Verwijder knooppunten", "Delete Nodes"),
        C::NodeBreak => ("Splits bij knooppunten", "Break at Nodes"),
        C::NodeSmooth => ("Glad", "Smooth"),
        C::NodeSymmetric => ("Symmetrisch", "Symmetric"),
        C::NodeCorner => ("Hoekpunt", "Corner"),
        C::NodeToLine => ("Naar lijn", "To Line"),
        C::NodeToCurve => ("Naar kromme", "To Curve"),
        C::NodeClosePath => ("Sluit pad", "Close Path"),
        C::NodeJoinEnds => ("Verbind uiteinden", "Join Ends"),
        C::NodeStartPoint => ("Startpunt", "Start Point"),
        C::Cut => ("Knip", "Cut"),
        C::Copy => ("Kopieer", "Copy"),
        C::Paste => ("Plak", "Paste"),
        C::PasteInPlace => ("Plak op plaats", "Paste in Place"),
        C::Duplicate => ("Dupliceer", "Duplicate"),
        C::DeleteDuplicates => ("Verwijder dubbelen", "Delete Duplicates"),
        C::PushForward => ("Een stap naar voren", "Push Forward"),
        C::PushBackward => ("Een stap naar achteren", "Push Backward"),
        C::PushToFront => ("Voorop", "Push to Front"),
        C::PushToBack => ("Achteraan", "Push to Back"),
        C::Group => ("Groepeer", "Group"),
        C::Ungroup => ("Hef groep op", "Ungroup"),
        C::AutoGroup => ("Groepeer automatisch", "Auto-Group"),
        C::BreakApart => ("Haal uit elkaar", "Break Apart"),
        C::FlipHorizontal => ("Spiegel horizontaal", "Flip Horizontally"),
        C::FlipVertical => ("Spiegel verticaal", "Flip Vertically"),
        C::Rotate90Cw => ("Draai 90° rechtsom", "Rotate 90° Clockwise"),
        C::Rotate90Ccw => ("Draai 90° linksom", "Rotate 90° Counterclockwise"),
        C::LockSelected => ("Vergrendel selectie", "Lock Selected"),
        C::UnlockSelected => ("Ontgrendel selectie", "Unlock Selected"),
        C::UnlockAll => ("Ontgrendel alles", "Unlock All"),
        C::HideSelected => ("Verberg selectie", "Hide Selected"),
        C::ShowAll => ("Toon alles", "Show All"),
        C::InvertSelection => ("Keer selectie om", "Invert Selection"),
        C::SelectOpenShapes => ("Selecteer open vormen", "Select Open Shapes"),
        C::SelectInLayer => ("Selecteer vormen in laag", "Select Shapes in Layer"),
        C::SelectContained => ("Selecteer omsloten vormen", "Select Contained Shapes"),
        C::SelectSmaller => ("Selecteer kleinere vormen", "Select Smaller Shapes"),
        C::SelectCircles => ("Selecteer cirkels", "Select Circles"),
        C::SelectSameType => ("Selecteer zelfde type", "Select Same Type"),
        C::IsolateSelection => ("Isoleer selectie", "Isolate Selection"),
        C::ZoomPage => ("Zoom naar werkblad", "Zoom to Page"),
        C::ZoomSelection => ("Zoom naar selectie", "Zoom to Selection"),
        C::ZoomIn => ("Zoom in", "Zoom In"),
        C::ZoomOut => ("Zoom uit", "Zoom Out"),
        C::ViewWireframe => ("Draadmodel", "Wireframe"),
        C::SaveAsTemplate => ("Bewaar als template…", "Save as Template…"),
        C::NewFromTemplate => ("Nieuw van template…", "New from Template…"),
        C::ExportSvg => ("SVG…", "SVG…"),
        C::ExportDxf => ("DXF…", "DXF…"),
        C::ExportBundle => ("Bundel exporteren…", "Export Bundle…"),
        C::ImportBundle => ("Bundel importeren…", "Import Bundle…"),
        C::ShowMaterialLibrary => ("Materiaalbibliotheek", "Material Library"),
        C::ShowArtLibrary => ("Kunstbibliotheek", "Art Library"),
        C::ShowNotes => ("Notities tonen", "Show Notes"),
        C::TextOnPath => ("Tekst op pad…", "Text on a Path…"),
        C::BarcodeNew => ("Streepjescode…", "Bar Code…"),
        other => return l10n_schikken::command_pair(other),
    })
}

/// The name of an undoable step alone ("Rechthoek tekenen"), as the history panel lists it;
/// `None` for a label the catalogue does not know yet.
pub fn undo_label_text(label: UndoLabel, locale: Locale) -> Option<&'static str> {
    let (nl, en) = undo_pair(label)?;
    Some(match locale {
        Locale::Nl => nl,
        Locale::En => en,
    })
}

fn undo_pair(label: UndoLabel) -> Option<(&'static str, &'static str)> {
    Some(match label {
        UndoLabel::CreateRectangle => ("Rechthoek tekenen", "Draw Rectangle"),
        UndoLabel::SetDimension => ("Maat wijzigen", "Change Size"),
        UndoLabel::Transform => ("Verplaatsen", "Move"),
        UndoLabel::Delete => ("Verwijderen", "Delete"),
        UndoLabel::Rename => ("Naam wijzigen", "Rename"),
        UndoLabel::Compound(c) => match c {
            CompoundLabel::ImportImage => ("Beeld importeren", "Import Image"),
            CompoundLabel::MaskImage => ("Beeld maskeren", "Mask Image"),
            CompoundLabel::ImportProject => ("Project importeren", "Import Project"),
            _ => return None,
        },
        UndoLabel::Arrange(a) => match a {
            ArrangeLabel::DrawOrder => ("Tekenvolgorde wijzigen", "Change Draw Order"),
            ArrangeLabel::Lock => ("Vergrendelen", "Lock"),
            ArrangeLabel::Unlock => ("Ontgrendelen", "Unlock"),
            ArrangeLabel::Hide => ("Verbergen", "Hide"),
            ArrangeLabel::Show => ("Tonen", "Show"),
            ArrangeLabel::Paste => ("Plakken", "Paste"),
            ArrangeLabel::BreakApart => ("Uit elkaar halen", "Break Apart"),
            ArrangeLabel::AutoGroup => ("Automatisch groeperen", "Auto-Group"),
            ArrangeLabel::Ungroup => ("Groep opheffen", "Ungroup"),
            _ => return None,
        },
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
            EditLabel::CutCorner => ("Hoek afronden of afschuinen", "Fillet or Chamfer"),
            EditLabel::Break => ("Splitsen", "Break"),
            EditLabel::RoundCorners => ("Hoeken afronden", "Round Corners"),
            EditLabel::OptimizeShapes => ("Vormen optimaliseren", "Optimize Shapes"),
            EditLabel::CutShapes => ("Vormen snijden", "Cut Shapes"),
            EditLabel::JoinShapes => ("Vormen verbinden", "Join Shapes"),
            EditLabel::Direction => ("Richting wijzigen", "Change Direction"),
            other => return l10n_schikken::edit_label_pair(other),
        },
        UndoLabel::Shape(l) => match l {
            ShapeLabel::Create => ("Vorm tekenen", "Draw Shape"),
            ShapeLabel::SetParams => ("Vormparameters wijzigen", "Change Shape Parameters"),
            ShapeLabel::ConvertToPath => ("Omzetten naar pad", "Convert to Path"),
            _ => return None,
        },
        UndoLabel::Structure(StructureLabel::CreateModifier) => ("Patroon maken", "Create Pattern"),
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
            ProcessLabel::SetOptimization => ("Optimalisatie-instellingen wijzigen", "Change Optimization Settings"),
            ProcessLabel::SetPriority => ("Prioriteit wijzigen", "Change Cut Priority"),
            ProcessLabel::SetNotes => ("Notities wijzigen", "Change Notes"),
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
        FieldKey::ScaleX => ("Schaal X", "Scale X"),
        FieldKey::ScaleY => ("Schaal Y", "Scale Y"),
        FieldKey::ShearX => ("Scheef X", "Shear X"),
        FieldKey::ShearY => ("Scheef Y", "Shear Y"),
        FieldKey::Priority => ("Prioriteit", "Priority"),
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
pub fn option_caption(field: burnburn_presentation::CutField, locale: Locale) -> &'static str {
    use burnburn_presentation::CutField as F;
    let (nl, en) = match field {
        F::TabsEnabled => ("Tabs / bruggen", "Tabs / bridges"),
        F::TabsMode => ("Tabs per", "Tabs by"),
        F::TabsValue => ("Aantal of afstand (mm)", "Count or spacing (mm)"),
        F::TabsWidth => ("Tabbreedte (mm)", "Tab width (mm)"),
        F::LeadInEnabled => ("Aanloop (lead-in)", "Lead-in"),
        F::LeadInLength => ("Aanloop, lengte (mm)", "Lead-in length (mm)"),
        F::LeadInShape => ("Aanloop, vorm", "Lead-in shape"),
        F::LeadInAngle => ("Aanloop, hoek (°)", "Lead-in angle (°)"),
        F::LeadOutEnabled => ("Uitloop (lead-out)", "Lead-out"),
        F::LeadOutLength => ("Uitloop, lengte (mm)", "Lead-out length (mm)"),
        F::LeadOutShape => ("Uitloop, vorm", "Lead-out shape"),
        F::LeadOutAngle => ("Uitloop, hoek (°)", "Lead-out angle (°)"),
        F::Overcut => ("Overcut (mm, 0 = uit)", "Overcut (mm, 0 = off)"),
        F::PerforationEnabled => ("Perforatie", "Perforation"),
        F::PerforationOn => ("Perforatie, aan (mm)", "Perforation on (mm)"),
        F::PerforationOff => ("Perforatie, uit (mm)", "Perforation off (mm)"),
        F::DotEnabled => ("Dot-modus (puls per afstand)", "Dot mode (pulse per spacing)"),
        F::DotSpacing => ("Dot-afstand (mm)", "Dot spacing (mm)"),
        F::DotDwell => ("Puls (ms)", "Pulse (ms)"),
        F::FillInterval => ("Lijnafstand (mm)", "Line interval (mm)"),
        F::FillAngle => ("Scanhoek (°)", "Scan angle (°)"),
        F::FillBidirectional => ("Bidirectioneel", "Bidirectional"),
        F::FillOverscan => ("Overscan (mm)", "Overscan (mm)"),
        F::FillCrossHatch => ("Kruisarcering", "Cross-hatch"),
        F::FillFloodFill => ("Flood fill", "Flood fill"),
        F::FillGrouping => ("Vormen vullen", "Fill grouping"),
        F::FillRamp => ("Ramp-lengte (mm, 0 = uit)", "Ramp length (mm, 0 = off)"),
        F::ImageMode => ("Beeldmodus", "Image mode"),
        F::ImageLevel => ("Drempel (0–255)", "Threshold (0–255)"),
        F::ImageMatrix => ("Matrixgrootte (2, 4 of 8)", "Matrix size (2, 4 or 8)"),
        F::ImageNegative => ("Negatief beeld", "Negative image"),
        F::ImageDotWidth => ("Dot-breedtecorrectie (mm)", "Dot width correction (mm)"),
        F::ImageAngle => ("Scanhoek (°)", "Scan angle (°)"),
        F::ImageInterval => ("Lijnafstand (mm of dpi)", "Line interval (mm or dpi)"),
        F::ImageDotPitch => ("Puntafstand (mm of dpi)", "Dot pitch (mm or dpi)"),
        F::ImageBidirectional => ("Bidirectioneel", "Bidirectional"),
        F::ImageOverscan => ("Overscan (mm)", "Overscan (mm)"),
        F::ScreenCell => ("Rastercel (mm)", "Screen cell (mm)"),
        F::ScreenAngle => ("Rasterhoek (°)", "Screen angle (°)"),
        F::ZOffset => ("Z-offset (mm, 0 = geen)", "Z offset (mm, 0 = none)"),
        _ => ("", ""),
    };
    match locale {
        Locale::Nl => nl,
        Locale::En => en,
    }
}

pub fn choice_label(choice: burnburn_presentation::CutChoice, locale: Locale) -> &'static str {
    use burnburn_presentation::CutChoice as C;
    let (nl, en) = match choice {
        C::TabsByCount => ("Aantal", "Count"),
        C::TabsBySpacing => ("Afstand", "Spacing"),
        C::LeadLine => ("Lijn", "Line"),
        C::LeadArc => ("Boog", "Arc"),
        C::GroupAllShapes => ("Alle vormen tegelijk vullen", "Fill all shapes at once"),
        C::GroupGroups => ("Groepen samen vullen", "Fill groups together"),
        C::GroupIndividual => ("Vormen apart vullen", "Fill shapes individually"),
        C::Threshold => ("Drempel", "Threshold"),
        C::Ordered => ("Geordend", "Ordered"),
        C::Atkinson => ("Atkinson", "Atkinson"),
        C::Dither => ("Dither", "Dither"),
        C::Stucki => ("Stucki", "Stucki"),
        C::Jarvis => ("Jarvis", "Jarvis"),
        C::Newsprint => ("Krantenraster", "Newsprint"),
        C::Halftone => ("Halftoon", "Halftone"),
        C::Sketch => ("Schets", "Sketch"),
        C::Grayscale => ("Grijswaarden", "Grayscale"),
        C::PassThrough => ("Doorgeven", "Pass-through"),
        _ => ("", ""),
    };
    match locale {
        Locale::Nl => nl,
        Locale::En => en,
    }
}

/// Why a file feature did nothing (import, templates, libraries, bundles, export).
fn files_pair(m: burnburn_presentation::FilesMessage) -> Option<(&'static str, &'static str)> {
    use burnburn_presentation::FilesMessage as F;
    Some(match m {
        F::ImportLegacyIllustrator => (
            "Dit is een oud Illustrator-bestand (PostScript). Bewaar het in Illustrator met PDF-compatibiliteit of als SVG en importeer dat.",
            "This is an old Illustrator file (PostScript). Save it from Illustrator with PDF compatibility or as SVG and import that.",
        ),
        F::ImportEncryptedPdf => (
            "Dit PDF-bestand is beveiligd met een wachtwoord en wordt niet gelezen.",
            "This PDF is password protected and is not read.",
        ),
        F::TemplateWrongExtension => (
            "Een template eindigt op .burnburntemplate; er is niets bewaard.",
            "A template ends in .burnburntemplate; nothing was saved.",
        ),
        F::TemplateSaveFailed => ("De template kon niet bewaard worden.", "The template could not be saved."),
        F::TemplateOpenFailed => (
            "De template kon niet gelezen worden; er is geen venster geopend.",
            "The template could not be read; no window was opened.",
        ),
        F::LibraryUnreadable => (
            "Het bibliotheekbestand kan niet gelezen worden en blijft ongemoeid. Wijzigingen blijven alleen in het geheugen.",
            "The library file cannot be read and is left alone. Changes stay in memory only.",
        ),
        F::LibrarySaveFailed => (
            "De bibliotheek kon niet bewaard worden; de wijziging bestaat alleen in het geheugen.",
            "The library could not be saved; the change exists in memory only.",
        ),
        F::MaterialInvalid => (
            "Geef een naam, een materiaal en een dikte groter dan nul.",
            "Give a name, a material and a thickness greater than zero.",
        ),
        F::MaterialImageLayer => (
            "Een beeldlaag past niet in de bibliotheek en kan geen bibliotheekinstellingen krijgen.",
            "An image layer does not fit the library and cannot take library settings.",
        ),
        F::MaterialUnknown => {
            ("Die laag of dat bibliotheekitem bestaat niet meer.", "That layer or library entry no longer exists.")
        }
        F::ArtNameInvalid => (
            "Die naam is leeg, te lang of bevat stuurtekens.",
            "That name is empty, too long or contains control characters.",
        ),
        F::ArtGroupExists => ("Die groep bestaat al.", "That group already exists."),
        F::ArtGroupUnknown => ("Die groep bestaat niet.", "That group does not exist."),
        F::ArtGroupNotEmpty => (
            "De groep bevat nog items: verplaats of verwijder ze eerst.",
            "The group still holds items: move or remove them first.",
        ),
        F::ArtItemUnknown => ("Dat item bestaat niet meer.", "That item no longer exists."),
        F::ArtFileUnreadable => (
            "Het bestand kon niet als tekening gelezen worden of bevat geen vormen.",
            "The file could not be read as a drawing or holds no shapes.",
        ),
        F::ArtFileFormat => (
            "Alleen SVG-, DXF-, HP-GL-, PDF- en AI-bestanden gaan in de bibliotheek.",
            "Only SVG, DXF, HP-GL, PDF and AI files go into the library.",
        ),
        F::ArtScaleRequired => {
            ("Het DXF-bestand noemt geen eenheid: kies er een.", "The DXF file names no unit: choose one.")
        }
        F::ArtNothingSelected => {
            ("Selecteer eerst de objecten die je wilt toevoegen.", "Select the objects you want to add first.")
        }
        F::ArtLibraryFull => ("De bibliotheek is vol.", "The library is full."),
        F::BundleNothingSelected => ("Kies wat in de bundel moet.", "Choose what goes into the bundle."),
        F::BundleUnreadable => (
            "Dit bestand is geen leesbare bundel; er is niets veranderd.",
            "This file is not a readable bundle; nothing changed.",
        ),
        F::BundleWriteFailed => ("De bundel kon niet geschreven worden.", "The bundle could not be written."),
        F::BundleApplyFailed => (
            "De bundel kon niet toegepast worden; er is niets veranderd.",
            "The bundle could not be applied; nothing changed.",
        ),
        F::ExportEmpty => ("Er is niets om te exporteren.", "There is nothing to export."),
        F::ExportNeedsAcknowledgement => (
            "Bevestig eerst dat de randvoorwaarden of modifiers verloren gaan.",
            "First confirm that the constraints or modifiers are lost.",
        ),
        F::ExportInvalidOption => (
            "Een optie bevat een onbruikbare waarde; de laatste goede opties blijven gelden.",
            "An option holds an unusable value; the last good options stay in use.",
        ),
        F::ExportWriteFailed => ("Het bestand kon niet geschreven worden.", "The file could not be written."),
        _ => return None,
    })
}

fn cut_caption(field: burnburn_presentation::CutField, locale: Locale) -> &'static str {
    option_caption(field, locale)
}

pub fn message(key: MessageKey, locale: Locale) -> Option<String> {
    use MessageKey as M;
    match key {
        M::LaserRefused(why) => return Some(laser_why_text(why, locale)),
        M::Schikken(m) => return Some(l10n_schikken::message(m, locale)),
        M::DiagnosticsFailed => {
            return Some(pick(
                (
                    "Het diagnoserapport kon niet gemaakt of geschreven worden.",
                    "The diagnostic report could not be made or written.",
                ),
                locale,
            ));
        }
        _ => {}
    }
    let pair: (&str, &str) = match key {
        M::Command(k) => command_pair(k)?,
        M::Files(m) => files_pair(m)?,
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
        M::InvalidCutField(f) => {
            let (nl, en) = (cut_caption(f, Locale::Nl), cut_caption(f, Locale::En));
            return Some(match locale {
                Locale::Nl => format!("Geen geldige waarde voor \u{201c}{nl}\u{201d}; er is niets gewijzigd."),
                Locale::En => format!("Not a valid value for \u{201c}{en}\u{201d}; nothing was changed."),
            });
        }
        M::CutOptionsConflict => (
            "Dot-modus sluit tabs, aanloop/uitloop, overcut en perforatie uit.",
            "Dot mode excludes tabs, leads, overcut and perforation.",
        ),
        M::CutOptionNotForMode(f) => {
            let (nl, en) = (cut_caption(f, Locale::Nl), cut_caption(f, Locale::En));
            return Some(match locale {
                Locale::Nl => format!("\u{201c}{nl}\u{201d} past niet bij deze beeldmodus (alleen 0°)."),
                Locale::En => format!("\u{201c}{en}\u{201d} does not apply to this image mode (0° only)."),
            });
        }
        M::SaveFailed => ("Bewaren is mislukt.", "Saving failed."),
        M::OpenFailed => ("Openen is mislukt.", "Opening failed."),
        M::OpenNewerSchema => (
            "Dit project is gemaakt met een nieuwere versie van BurnBurn en wordt niet geopend; het bestand is niet gewijzigd.",
            "This project was made with a newer version of BurnBurn and is not opened; the file was not changed.",
        ),
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
            ToolRejection::AmbiguousRelations => (
                "Niet eenduidig: er is niets toegepast. Maak de lijn recht of beperk handmatig.",
                "Ambiguous: nothing was applied. Straighten the line or constrain it by hand.",
            ),
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
            ToolRejection::SingleObjectOnly => (
                "Selecteer één object: meerdere tegelijk volgt (het document heeft er nog geen atomair commando voor).",
                "Select a single object: several at once follows (the document has no atomic command for it yet).",
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
        M::SketchParameterInUse => (
            "Deze parameter wordt nog gebruikt door een maat of een andere parameter.",
            "A dimension or another parameter still uses this parameter.",
        ),
        M::TraceSourceMissing => (
            "De bronafbeelding van deze trace zit niet meer in het project.",
            "The source image of this trace is no longer in the project.",
        ),
        M::SketchNothingToRestore => (
            "Geen eerdere stand van deze schets was geldig; er is niets hersteld.",
            "No earlier state of this sketch was valid; nothing was restored.",
        ),
        M::SketchShapeUnsupported => (
            "Dit kan nog niet in een schets getekend worden (alleen lijn, polylijn, rechthoek en cirkel).",
            "This cannot be drawn into a sketch yet (only line, polyline, rectangle and circle).",
        ),
        M::SketchConversionImpossible => (
            "Alleen rechte lijnen en volle cirkels kunnen een schets worden.",
            "Only straight lines and full circles can become a sketch.",
        ),
        M::SketchEditRefused => ("De schetswijziging is geweigerd.", "The sketch change was refused."),
        M::NothingToChange => (
            "Er is niets te wijzigen: de bewerking laat deze vormen zoals ze zijn.",
            "Nothing to change: the operation leaves these shapes as they are.",
        ),
        M::ShapeParamInvalid => ("Ongeldige waarde voor deze vorm.", "Invalid value for this shape."),
        M::ClipboardUnreadable => (
            "Het klembord bevat geen leesbare BurnBurn-objecten.",
            "The clipboard does not hold readable BurnBurn objects.",
        ),
        M::NoDuplicatesFound => ("Geen dubbele vormen gevonden.", "No duplicate shapes found."),
        M::NothingFound => ("Niets gevonden om te selecteren.", "Nothing found to select."),
        M::ObjectsLocked => {
            ("Vergrendelde objecten kunnen niet gewijzigd worden.", "Locked objects cannot be changed.")
        }
        M::RdRefused(reason) => return Some(rd_refusal_text(reason, locale).to_owned()),
        M::VisualPlacementOnly => (
            "Camerabeeld is alleen een visuele plaatsingshulp, geen productiegeometrie.",
            "The camera image is only a visual placement aid, not production geometry.",
        ),
        _ => return None,
    };
    Some(pick(pair, locale))
}

/// The refusal of RD output in words.
pub fn rd_refusal_text(reason: burnburn_presentation::RdRefusal, locale: Locale) -> &'static str {
    use burnburn_presentation::RdRefusal;
    let (nl, en) = match reason {
        RdRefusal::CrossHatch => (
            "Kruisarcering heeft nog geen bewezen Ruida-vorm: plan en voorvertoning bestaan, RD-uitvoer wordt geweigerd.",
            "Cross-hatch has no proven Ruida form yet: the plan and preview exist, RD output is refused.",
        ),
        RdRefusal::DotMode => (
            "Puntmodus (dwell) heeft nog geen bewezen Ruida-vorm: RD-uitvoer wordt geweigerd.",
            "Dot (dwell) mode has no proven Ruida form yet: RD output is refused.",
        ),
        RdRefusal::RampLength => (
            "Vermogensverloop (ramp) heeft nog geen bewezen Ruida-vorm: RD-uitvoer wordt geweigerd.",
            "Power ramp has no proven Ruida form yet: RD output is refused.",
        ),
        RdRefusal::FocusOffset => (
            "Focus-/Z-verschuiving heeft nog geen bewezen Ruida-vorm (zie docs/hardware/evidence-cam.md): RD-uitvoer wordt geweigerd.",
            "Focus/Z offsets have no proven Ruida form yet (see docs/hardware/evidence-cam.md): RD output is refused.",
        ),
        RdRefusal::ScanAngle => (
            "Schuine scanlijnen hebben nog geen bewezen Ruida-vorm: RD-uitvoer wordt geweigerd.",
            "Angled scan lines have no proven Ruida form yet: RD output is refused.",
        ),
        RdRefusal::GrayscaleModulation => (
            "Grijswaarden (vermogen per run) hebben nog geen bewezen Ruida-vorm: RD-uitvoer wordt geweigerd.",
            "Grayscale (power per run) has no proven Ruida form yet: RD output is refused.",
        ),
        RdRefusal::TooManyParts => (
            "Meer instellingenblokken dan het RD-formaat toelaat (128): RD-uitvoer wordt geweigerd.",
            "More settings blocks than the RD format allows (128): RD output is refused.",
        ),
        RdRefusal::NegativeCoordinate => (
            "Een coördinaat onder nul is niet bevestigd voor RD: RD-uitvoer wordt geweigerd.",
            "A coordinate below zero is not confirmed for RD: RD output is refused.",
        ),
        _ => (
            "Dit onderdeel heeft nog geen bewezen Ruida-vorm: RD-uitvoer wordt geweigerd.",
            "This part has no proven Ruida form yet: RD output is refused.",
        ),
    };
    match locale {
        Locale::Nl => nl,
        Locale::En => en,
    }
}

/// The name of a machine-profile feature (capability lists, W3-UI-cam).
pub fn feature_name(feature: Feature, locale: Locale) -> String {
    feature_pair(feature).map_or_else(|| format!("{feature:?}"), |pair| pick(pair, locale))
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
    keys.push(MessageKey::DiagnosticsFailed);
    keys.extend(l10n_schikken::known_keys());
    keys.extend(burnburn_presentation::FilesMessage::ALL.iter().map(|m| MessageKey::Files(*m)));
    keys.extend(all_laser_whys().into_iter().map(MessageKey::LaserRefused));
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
            EditLabel::CutCorner,
            EditLabel::Break,
            EditLabel::RoundCorners,
            EditLabel::OptimizeShapes,
            EditLabel::CutShapes,
            EditLabel::JoinShapes,
            EditLabel::Direction,
        ]
        .map(UndoLabel::Edit),
    );
    undo.extend([ShapeLabel::Create, ShapeLabel::SetParams, ShapeLabel::ConvertToPath].map(UndoLabel::Shape));
    undo.extend([CompoundLabel::ImportImage, CompoundLabel::MaskImage, CompoundLabel::ImportProject].map(UndoLabel::Compound));
    undo.push(UndoLabel::Structure(StructureLabel::CreateModifier));
    undo.extend(
        [
            ArrangeLabel::DrawOrder,
            ArrangeLabel::Lock,
            ArrangeLabel::Unlock,
            ArrangeLabel::Hide,
            ArrangeLabel::Show,
            ArrangeLabel::Paste,
            ArrangeLabel::BreakApart,
            ArrangeLabel::AutoGroup,
            ArrangeLabel::Ungroup,
        ]
        .map(UndoLabel::Arrange),
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
            ProcessLabel::SetOptimization,
            ProcessLabel::SetPriority,
            ProcessLabel::SetNotes,
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
        [
            FieldKey::X,
            FieldKey::Y,
            FieldKey::Width,
            FieldKey::Height,
            FieldKey::Rotation,
            FieldKey::ScaleX,
            FieldKey::ScaleY,
            FieldKey::ShearX,
            FieldKey::ShearY,
            FieldKey::Priority,
        ]
        .map(MessageKey::Field),
    );
    keys.extend(
        [
            burnburn_presentation::RdRefusal::CrossHatch,
            burnburn_presentation::RdRefusal::DotMode,
            burnburn_presentation::RdRefusal::RampLength,
            burnburn_presentation::RdRefusal::FocusOffset,
            burnburn_presentation::RdRefusal::ScanAngle,
            burnburn_presentation::RdRefusal::GrayscaleModulation,
            burnburn_presentation::RdRefusal::Other,
            burnburn_presentation::RdRefusal::TooManyParts,
            burnburn_presentation::RdRefusal::NegativeCoordinate,
        ]
        .map(MessageKey::RdRefused),
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
    keys.extend(burnburn_presentation::CutField::ALL.map(MessageKey::InvalidCutField));
    keys.extend([
        MessageKey::CutOptionsConflict,
        MessageKey::CutOptionNotForMode(burnburn_presentation::CutField::ImageAngle),
    ]);
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
        MessageKey::SketchNothingToRestore,
        MessageKey::TraceSourceMissing,
        MessageKey::SketchParameterInUse,
        MessageKey::SketchShapeUnsupported,
        MessageKey::SketchConversionImpossible,
        MessageKey::SketchEditRefused,
        MessageKey::NothingToChange,
        MessageKey::ShapeParamInvalid,
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
        MessageKey::OpenNewerSchema,
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
        MessageKey::ClipboardUnreadable,
        MessageKey::NoDuplicatesFound,
        MessageKey::NothingFound,
        MessageKey::ObjectsLocked,
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

/// One of each laser-window refusal (the features of the evidence-bound ones are examples).
pub fn all_laser_whys() -> Vec<LaserWhy> {
    use LaserWhy as W;
    let needs = LiveFeature::Home.required_evidence();
    vec![
        W::NoMachine,
        W::NoMachineProfile,
        W::NotVerified { feature: LiveFeature::Home, needs },
        W::Unsupported(LiveFeature::Jog),
        W::NoWireForm(LiveFeature::FileUpload),
        W::JobMayBeActive,
        W::RequestInFlight,
        W::JogKeyHeld(JogKey::XLeft),
        W::PortFailed,
        W::NotRecorded,
        W::PositionUnknown,
        W::UserOriginNotSet,
        W::OutsideBed,
        W::InvalidCoordinate,
        W::InvalidNumber,
        W::InvalidFileName,
        W::NoReviewedPlan,
        W::NothingSelected,
        W::NoObjects,
        W::PlacementOutsideBed,
        W::PlacementStale,
        W::NotAllowed,
        W::Refused,
    ]
}

/// Name of a laser-window feature, for refusals.
pub fn live_feature_name(feature: LiveFeature, locale: Locale) -> &'static str {
    let (nl, en) = match feature {
        LiveFeature::Jog => ("Jog (ingedrukt houden)", "Jog (hold)"),
        LiveFeature::Home => ("Home", "Home"),
        LiveFeature::MoveAbsolute => ("Verplaatsen naar een positie", "Moving to a position"),
        LiveFeature::PauseResume => ("Pauzeren en hervatten", "Pause and resume"),
        LiveFeature::ReadPosition => ("Positie uitlezen", "Reading the position"),
        LiveFeature::FileList => ("Bestandslijst van de controller", "Controller file list"),
        LiveFeature::FileMetadata => ("Bestandsinformatie", "File information"),
        LiveFeature::FileRun => ("Bestand op de controller uitvoeren", "Running a controller file"),
        LiveFeature::FileDelete => ("Bestand op de controller verwijderen", "Deleting a controller file"),
        LiveFeature::FileUpload => ("RD naar de controller bewaren", "Saving RD to the controller"),
    };
    match locale {
        Locale::Nl => nl,
        Locale::En => en,
    }
}

/// The words for a laser-window refusal or for why a control is off. A feature that is not
/// proven on the controller says so and names the hardware evidence that is missing.
pub fn laser_why_text(why: LaserWhy, locale: Locale) -> String {
    use LaserWhy as W;
    let (nl, en): (String, String) = match why {
        W::NotVerified { feature, needs } => {
            let (n, e) = (live_feature_name(feature, Locale::Nl), live_feature_name(feature, Locale::En));
            (
                format!(
                    "Niet geverifieerd: {n} is op deze controller nog niet bewezen en wordt daarom niet verstuurd. Bewijs nodig: {needs}"
                ),
                format!(
                    "Not verified: {e} is not yet proven on this controller and is therefore not sent. Evidence needed: {needs}"
                ),
            )
        }
        W::Unsupported(f) => (
            format!("{} wordt door deze controller niet ondersteund.", live_feature_name(f, Locale::Nl)),
            format!("{} is not supported by this controller.", live_feature_name(f, Locale::En)),
        ),
        W::NoWireForm(f) => (
            format!(
                "{}: deze versie van BurnBurn kan dit nog niet naar de controller sturen, ook niet als het bewezen zou zijn.",
                live_feature_name(f, Locale::Nl)
            ),
            format!(
                "{}: this version of BurnBurn cannot send this to the controller yet, even if it were proven.",
                live_feature_name(f, Locale::En)
            ),
        ),
        other => {
            let (n, e) = match other {
                W::NoMachine => ("Geen machine verbonden.", "No machine connected."),
                W::NoMachineProfile => ("Kies eerst een machineprofiel.", "Choose a machine profile first."),
                W::JobMayBeActive => (
                    "Er kan een job actief zijn: de kop wordt nu niet bewogen en er wordt geen bestand uitgevoerd of verwijderd.",
                    "A job may be active: the head is not moved now and no file is run or deleted.",
                ),
                W::RequestInFlight => (
                    "Er wacht nog een verzoek op het antwoord van de controller.",
                    "A request is still waiting for the controller's reply.",
                ),
                W::JogKeyHeld(_) => (
                    "Een jogtoets is nog ingedrukt, of het loslaten is niet bevestigd.",
                    "A jog key is still held, or its release is unconfirmed.",
                ),
                W::PortFailed => {
                    ("De verbinding kon het verzoek niet versturen.", "The connection could not send the request.")
                }
                W::NotRecorded => (
                    "De actie kon niet in het journaal worden vastgelegd en is niet verstuurd.",
                    "The action could not be recorded in the journal and was not sent.",
                ),
                W::PositionUnknown => (
                    "De positie van de kop is onbekend (niet 0). Lees eerst de positie uit.",
                    "The head position is unknown (not 0). Read the position first.",
                ),
                W::UserOriginNotSet => ("De gebruikersoorsprong is niet ingesteld.", "The user origin is not set."),
                W::OutsideBed => ("Dat punt ligt buiten het bed.", "That point is outside the bed."),
                W::InvalidCoordinate => ("Ongeldige coördinaat.", "Invalid coordinate."),
                W::InvalidNumber => ("Geen geldig getal.", "Not a valid number."),
                W::InvalidFileName => (
                    "Ongeldige bestandsnaam (leesbare ASCII, niet leeg).",
                    "Invalid file name (printable ASCII, not empty).",
                ),
                W::NoReviewedPlan => (
                    "Compileer en beoordeel eerst een plan zonder fouten.",
                    "Compile and review a plan without errors first.",
                ),
                W::NothingSelected => ("Er is niets geselecteerd.", "Nothing is selected."),
                W::NoObjects => ("Er is niets om te branden.", "There is nothing to burn."),
                W::PlacementOutsideBed => ("De job zou buiten het bed vallen.", "The job would fall outside the bed."),
                W::PlacementStale => (
                    "Het plan is gemaakt met een andere instelling of een andere positie van de kop: compileer opnieuw.",
                    "The plan was made with another setup or another head position: compile again.",
                ),
                W::NotAllowed => ("Niet toegestaan.", "Not allowed."),
                _ => ("De actie is geweigerd.", "The action was refused."),
            };
            (n.to_owned(), e.to_owned())
        }
    };
    match locale {
        Locale::Nl => nl,
        Locale::En => en,
    }
}

/// Text for `key`, or a visible placeholder naming the key: never an empty label.
pub fn message_or_key(key: MessageKey, locale: Locale) -> String {
    message(key, locale).unwrap_or_else(|| format!("[{key:?}]"))
}

/// The sentence for an image-panel issue (bitmap import, Adjust Image, tracer). The field an
/// issue belongs to is named apart ([`image_field_name`]).
pub fn image_issue_text(issue: ImageIssue, locale: Locale) -> String {
    let (nl, en) = match issue {
        ImageIssue::InvalidNumber(_) => ("Ongeldig getal", "Not a valid number"),
        ImageIssue::OutOfRange(_) => ("Waarde buiten bereik", "Value out of range"),
        ImageIssue::CropOutsideSource => ("Uitsnede valt buiten de afbeelding", "The crop lies outside the image"),
        ImageIssue::CropIncomplete => ("Uitsnede is niet volledig ingevuld", "The crop is not completely filled in"),
        ImageIssue::UnsupportedFormat => {
            ("Bestandstype niet ondersteund (alleen PNG en JPEG)", "File type not supported (PNG and JPEG only)")
        }
        ImageIssue::Unreadable => ("Bestand kon niet worden gelezen", "The file could not be read"),
        ImageIssue::SourceMissing => {
            ("Bronpixels ontbreken in het project", "The source pixels are missing from the project")
        }
        _ => return format!("[{issue:?}]"),
    };
    pick((nl, en), locale)
}

/// The caption of an image-panel field, for issue lines.
pub fn image_field_name(field: ImageField, locale: Locale) -> &'static str {
    let (nl, en) = match field {
        ImageField::Dpi => ("Resolutie", "Resolution"),
        ImageField::Brightness => ("Helderheid", "Brightness"),
        ImageField::Contrast => ("Contrast", "Contrast"),
        ImageField::Gamma => ("Gamma", "Gamma"),
        ImageField::SharpenAmount => ("Verscherpen: sterkte", "Sharpen: amount"),
        ImageField::SharpenRadius => ("Verscherpen: straal", "Sharpen: radius"),
        ImageField::CropX => ("Uitsnede x", "Crop x"),
        ImageField::CropY => ("Uitsnede y", "Crop y"),
        ImageField::CropWidth => ("Uitsnede breedte", "Crop width"),
        ImageField::CropHeight => ("Uitsnede hoogte", "Crop height"),
        ImageField::TraceThreshold => ("Drempel", "Threshold"),
        ImageField::TraceIgnoreBelow => ("Negeer kleiner dan", "Ignore smaller than"),
        ImageField::TraceSmoothness => ("Gladheid", "Smoothness"),
        _ => ("?", "?"),
    };
    match locale {
        Locale::Nl => nl,
        Locale::En => en,
    }
}
