//! The Schikken panel (W3-UI-schikken): one floating panel that shows the flow in progress —
//! the Resize Slots dialog, the nest, Print and Cut, the Center Finder, Warp/Deform, Tangent
//! Circle — or else the Dock / Move Together options. This file decides what it shows, as plain
//! [`SchikkenLine`]s in the window language, and which intent a control sends. No AppKit: the widgets in
//! `appkit/panels/schikken.rs` only draw the lines and report a [`SchikkenControl`] with the panel's
//! current values; raw text goes to presentation unparsed (DR-05).

use burnburn_editor::ToolKind;
use burnburn_presentation::{
    AppViewModel, CenterFinderIntent, CenterFinderViewModel, CommandKey, DockContactChoice, DockReference,
    DocumentViewModel, NestInput, NestMirror, NestPanelViewModel, NestRotations, NestRun, NestScope, PlaceOptionsInput,
    PrintAndCutIntent, PrintAndCutViewModel, PrintCutScale, SchikkenIntent, SlotChoice, SlotsDialogViewModel,
    SlotsInput, TangentViewModel, TogetherAnchorChoice, UiIntent,
};
use burnburn_types::DocumentId;

use crate::l10n::{
    Locale, center_problem_text, message_or_key, nest_skip_text, nest_unplaced_text, reshape_problem_text,
    tangent_problem_text,
};

/// A control of the panel. Fields, choices and checks report their value; buttons only the click.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SchikkenControl {
    PlaceReference,
    PlaceContact,
    PlaceGap,
    PlaceByGroup,
    PlaceAnchor,
    WarpBilinear,
    ReshapeApply,
    ReshapeReset,
    SlotsFrom,
    SlotsTo,
    SlotsTolerance,
    SlotsSelect,
    SlotsOk,
    SlotsCancel,
    NestPadding,
    NestEdge,
    NestStep,
    NestRotations,
    NestMirror,
    NestSheets,
    NestSheet,
    NestStart,
    NestStop,
    NestApply,
    NestSelectUnplaced,
    NestClose,
    /// Print and Cut, per design target `0` or `1`.
    PcPick(usize),
    PcX(usize),
    PcY(usize),
    PcMeasure(usize),
    PcMeasuredX(usize),
    PcMeasuredY(usize),
    PcScale,
    PcDeviation,
    PcRegister,
    PcReset,
    PcClose,
    CfPick,
    CfX,
    CfY,
    CfAdd,
    CfMeasure,
    CfUndo,
    CfClear,
    CfClose,
}

/// One line of the panel.
#[derive(Debug, Clone, PartialEq)]
pub enum SchikkenLine {
    Heading(String),
    Text(String),
    /// Why something did not work; drawn apart from the normal text.
    Problem(String),
    Field {
        control: SchikkenControl,
        caption: String,
        text: String,
        invalid: bool,
    },
    Choice {
        control: SchikkenControl,
        caption: String,
        options: Vec<String>,
        selected: usize,
    },
    Check {
        control: SchikkenControl,
        caption: String,
        on: bool,
    },
    Button {
        control: SchikkenControl,
        title: String,
        enabled: bool,
    },
}

impl SchikkenLine {
    pub fn control(&self) -> Option<SchikkenControl> {
        match self {
            SchikkenLine::Field { control, .. }
            | SchikkenLine::Choice { control, .. }
            | SchikkenLine::Check { control, .. }
            | SchikkenLine::Button { control, .. } => Some(*control),
            _ => None,
        }
    }
}

/// What the panel shows now.
#[derive(Debug, Clone, PartialEq)]
pub struct SchikkenPanel {
    pub title: String,
    pub lines: Vec<SchikkenLine>,
    /// A flow is in progress: the panel opens by itself.
    pub flow: bool,
}

/// Texts of the panel (NL, EN).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SchikkenText {
    MenuPanel,
    MenuDock,
    MenuMoveTogether,
    PanelTitle,
    PlaceHeading,
    PlaceReference,
    ReferenceFirst,
    ReferenceLast,
    PlaceContact,
    ContactBounds,
    ContactOutline,
    PlaceGap,
    PlaceByGroup,
    PlaceAnchor,
    AnchorStart,
    AnchorEnd,
    WarpHeading,
    DeformHeading,
    ReshapeHint,
    ReshapeBilinear,
    ReshapeApply,
    ReshapeReset,
    ReshapeSimplified,
    SlotsHeading,
    SlotsFrom,
    SlotsTo,
    SlotsTolerance,
    SlotsSelect,
    SlotsSlots,
    SlotsTabs,
    SlotsBoth,
    Ok,
    Cancel,
    NestQuickHeading,
    NestSelectedHeading,
    NestSheetBed,
    NestSheetBounds,
    NestPadding,
    NestEdge,
    NestStep,
    NestRotations,
    NestMirror,
    NestSheets,
    NestSheetCaption,
    NestSheetBedOption,
    NestSheetContour,
    NestSheetUse,
    NestMirroredRow,
    NestSetsRow,
    NestOrderRow,
    RotationsFixed,
    RotationsHalf,
    RotationsQuarter,
    RotationsEighth,
    NestStart,
    NestStop,
    NestApply,
    NestSelectUnplaced,
    Close,
    NestIdle,
    NestRunning,
    NestDone,
    NestStale,
    NestApplied,
    NestUnplacedRow,
    NestSkippedRow,
    PcHeading,
    PcTarget,
    PcPick,
    PcPicking,
    PcX,
    PcY,
    PcMeasure,
    PcMeasuredX,
    PcMeasuredY,
    PcScale,
    ScaleFixed,
    ScaleUniform,
    PcDeviation,
    PcRegister,
    PcReset,
    PcResult,
    PcHint,
    CfHeading,
    CfPick,
    CfPicking,
    CfMeasure,
    CfAdd,
    CfUndo,
    CfClear,
    CfPoints,
    CfResult,
    CfPoor,
    CfCenterMachine,
    TangentHeading,
    TangentPick,
    TangentNeedsDiameter,
    TangentSolutions,
    ValueX,
    ValueY,
}

impl SchikkenText {
    pub const ALL: &'static [SchikkenText] = {
        use SchikkenText as T;
        &[
            T::MenuPanel,
            T::MenuDock,
            T::MenuMoveTogether,
            T::PanelTitle,
            T::PlaceHeading,
            T::PlaceReference,
            T::ReferenceFirst,
            T::ReferenceLast,
            T::PlaceContact,
            T::ContactBounds,
            T::ContactOutline,
            T::PlaceGap,
            T::PlaceByGroup,
            T::PlaceAnchor,
            T::AnchorStart,
            T::AnchorEnd,
            T::WarpHeading,
            T::DeformHeading,
            T::ReshapeHint,
            T::ReshapeBilinear,
            T::ReshapeApply,
            T::ReshapeReset,
            T::ReshapeSimplified,
            T::SlotsHeading,
            T::SlotsFrom,
            T::SlotsTo,
            T::SlotsTolerance,
            T::SlotsSelect,
            T::SlotsSlots,
            T::SlotsTabs,
            T::SlotsBoth,
            T::Ok,
            T::Cancel,
            T::NestQuickHeading,
            T::NestSelectedHeading,
            T::NestSheetBed,
            T::NestSheetBounds,
            T::NestPadding,
            T::NestEdge,
            T::NestStep,
            T::NestRotations,
            T::NestMirror,
            T::NestSheets,
            T::NestSheetCaption,
            T::NestSheetBedOption,
            T::NestSheetContour,
            T::NestSheetUse,
            T::NestMirroredRow,
            T::NestSetsRow,
            T::NestOrderRow,
            T::RotationsFixed,
            T::RotationsHalf,
            T::RotationsQuarter,
            T::RotationsEighth,
            T::NestStart,
            T::NestStop,
            T::NestApply,
            T::NestSelectUnplaced,
            T::Close,
            T::NestIdle,
            T::NestRunning,
            T::NestDone,
            T::NestStale,
            T::NestApplied,
            T::NestUnplacedRow,
            T::NestSkippedRow,
            T::PcHeading,
            T::PcTarget,
            T::PcPick,
            T::PcPicking,
            T::PcX,
            T::PcY,
            T::PcMeasure,
            T::PcMeasuredX,
            T::PcMeasuredY,
            T::PcScale,
            T::ScaleFixed,
            T::ScaleUniform,
            T::PcDeviation,
            T::PcRegister,
            T::PcReset,
            T::PcResult,
            T::PcHint,
            T::CfHeading,
            T::CfPick,
            T::CfPicking,
            T::CfMeasure,
            T::CfAdd,
            T::CfUndo,
            T::CfClear,
            T::CfPoints,
            T::CfResult,
            T::CfPoor,
            T::CfCenterMachine,
            T::TangentHeading,
            T::TangentPick,
            T::TangentNeedsDiameter,
            T::TangentSolutions,
            T::ValueX,
            T::ValueY,
        ]
    };
}

fn pair(t: SchikkenText) -> (&'static str, &'static str) {
    use SchikkenText as T;
    match t {
        T::MenuPanel => ("Schikkenpaneel", "Arrange Panel"),
        T::MenuDock => ("Docken", "Dock"),
        T::MenuMoveTogether => ("Samenschuiven", "Move Together"),
        T::PanelTitle => ("Schikken", "Arrange"),
        T::PlaceHeading => ("Docken en samenschuiven", "Dock and Move Together"),
        T::PlaceReference => ("Referentie", "Reference"),
        T::ReferenceFirst => ("Eerst gekozen", "First selected"),
        T::ReferenceLast => ("Laatst gekozen", "Last selected"),
        T::PlaceContact => ("Contact", "Contact"),
        T::ContactBounds => ("Omhullende", "Bounding box"),
        T::ContactOutline => ("Omtrek", "Outline"),
        T::PlaceGap => ("Tussenruimte (mm)", "Gap (mm)"),
        T::PlaceByGroup => ("Groepen als eenheid", "Groups as one body"),
        T::PlaceAnchor => ("Blijft staan", "Stays put"),
        T::AnchorStart => ("Linker/onderste", "Leftmost/lowest"),
        T::AnchorEnd => ("Rechter/bovenste", "Rightmost/highest"),
        T::WarpHeading => ("Vervorm selectie (4 punten)", "Warp Selection (4 Points)"),
        T::DeformHeading => ("Vervorm selectie (16 punten)", "Deform Selection (16 Points)"),
        T::ReshapeHint => (
            "Sleep de grepen op het canvas. Enter past toe, Esc breekt af.",
            "Drag the handles on the canvas. Enter applies, Esc cancels.",
        ),
        T::ReshapeBilinear => ("Bilineair in plaats van perspectief", "Bilinear instead of perspective"),
        T::ReshapeApply => ("Toepassen", "Apply"),
        T::ReshapeReset => ("Grepen terugzetten", "Reset handles"),
        T::ReshapeSimplified => {
            ("Grote selectie: alleen het kader wordt getoond.", "Large selection: only the frame is previewed.")
        }
        T::SlotsHeading => ("Sleuven aanpassen", "Resize Slots"),
        T::SlotsFrom => ("Huidige breedte (mm)", "Current width (mm)"),
        T::SlotsTo => ("Nieuwe breedte (mm)", "New width (mm)"),
        T::SlotsTolerance => ("Tolerantie (mm)", "Tolerance (mm)"),
        T::SlotsSelect => ("Welke", "Which"),
        T::SlotsSlots => ("Sleuven", "Slots"),
        T::SlotsTabs => ("Tabs", "Tabs"),
        T::SlotsBoth => ("Beide", "Both"),
        T::Ok => ("OK", "OK"),
        T::Cancel => ("Annuleer", "Cancel"),
        T::NestQuickHeading => ("Snel nesten", "Quick Nest"),
        T::NestSelectedHeading => ("Nest selectie", "Nest Selected"),
        T::NestSheetBed => ("Plaat: machinebed", "Sheet: machine bed"),
        T::NestSheetBounds => ("Plaat: omvang van de onderdelen", "Sheet: bounds of the parts"),
        T::NestPadding => ("Tussenruimte (mm)", "Padding (mm)"),
        T::NestEdge => ("Randmarge (mm)", "Edge buffer (mm)"),
        T::NestStep => ("Rasterstap (mm)", "Grid step (mm)"),
        T::NestRotations => ("Draaiingen", "Rotations"),
        T::NestMirror => ("Spiegelen toestaan", "Allow mirroring"),
        T::NestSheets => ("Aantal platen", "Number of sheets"),
        T::NestSheetCaption => ("Plaat", "Sheet"),
        T::NestSheetBedOption => ("Machinebed", "Machine bed"),
        T::NestSheetContour => {
            ("Plaat: contour van een restplaat ({w} × {h} mm)", "Sheet: contour of a remnant ({w} × {h} mm)")
        }
        T::NestSheetUse => {
            ("Plaat {n}: {used} cm² gebruikt, {free} cm² rest", "Sheet {n}: {used} cm² used, {free} cm² left")
        }
        T::NestMirroredRow => ("{count} gespiegeld geplaatst", "{count} placed mirrored"),
        T::NestSetsRow => (
            "Complete sets: {sets}. Onderdelen volledig: {complete} van {total}.",
            "Complete sets: {sets}. Parts complete: {complete} of {total}.",
        ),
        T::NestOrderRow => {
            ("Onderdeel {n}: {placed} van {ordered} geplaatst", "Part {n}: {placed} of {ordered} placed")
        }
        T::RotationsFixed => ("Geen", "None"),
        T::RotationsHalf => ("0° en 180°", "0° and 180°"),
        T::RotationsQuarter => ("Kwartslagen", "Quarter turns"),
        T::RotationsEighth => ("Stappen van 45°", "45° steps"),
        T::NestStart => ("Nest", "Nest"),
        T::NestStop => ("Stop", "Stop"),
        T::NestApply => ("Toepassen", "Apply"),
        T::NestSelectUnplaced => ("Kies niet-geplaatste", "Select unplaced"),
        T::Close => ("Sluit", "Close"),
        T::NestIdle => ("{parts} onderdelen klaar om te nesten.", "{parts} parts ready to nest."),
        T::NestRunning => ("Bezig met nesten…", "Nesting…"),
        T::NestDone => (
            "{placed} geplaatst, {unplaced} niet geplaatst, {skipped} overgeslagen. Benutting {percent}%.",
            "{placed} placed, {unplaced} not placed, {skipped} skipped. Utilisation {percent}%.",
        ),
        T::NestStale => ("Het document is gewijzigd: nest opnieuw.", "The document changed: nest again."),
        T::NestApplied => ("Toegepast. Ongedaan maken herstelt alle onderdelen.", "Applied. Undo restores every part."),
        T::NestUnplacedRow => ("Niet geplaatst ({count}): {reason}", "Not placed ({count}): {reason}"),
        T::NestSkippedRow => ("Overgeslagen: {reason}", "Skipped: {reason}"),
        T::PcHeading => ("Print en snij", "Print and Cut"),
        T::PcTarget => ("Doelpunt {n}", "Target {n}"),
        T::PcPick => ("Kies op het canvas", "Pick on the canvas"),
        T::PcPicking => ("Klik op het canvas… (Esc stopt)", "Click on the canvas… (Esc stops)"),
        T::PcX => ("X in ontwerp (mm)", "X in design (mm)"),
        T::PcY => ("Y in ontwerp (mm)", "Y in design (mm)"),
        T::PcMeasure => ("Meet met de laser", "Measure with the laser"),
        T::PcMeasuredX => ("X gemeten (mm)", "X measured (mm)"),
        T::PcMeasuredY => ("Y gemeten (mm)", "Y measured (mm)"),
        T::PcScale => ("Schaal", "Scale"),
        T::ScaleFixed => ("Geen schaling", "No scaling"),
        T::ScaleUniform => ("Uniforme schaal toegestaan", "Allow uniform scale"),
        T::PcDeviation => ("Toegestane afwijking (0,05 = 5 %)", "Allowed deviation (0.05 = 5 %)"),
        T::PcRegister => ("Registreer", "Register"),
        T::PcReset => ("Herstel", "Reset"),
        T::PcResult => (
            "Geregistreerd: hoek {angle}°, schaal {scale}, residu rms {rms} mm (max {max} mm). De volgende compile gebruikt dit.",
            "Registered: angle {angle}°, scale {scale}, residual rms {rms} mm (max {max} mm). The next compile uses it.",
        ),
        T::PcHint => (
            "Kies twee doelpunten in het ontwerp en meet ze met de laser.",
            "Choose two targets in the design and measure them with the laser.",
        ),
        T::CfHeading => ("Middelpuntzoeker", "Center Finder"),
        T::CfPick => ("Kies punten op het canvas", "Pick points on the canvas"),
        T::CfPicking => ("Klik randpunten… (Esc stopt)", "Click edge points… (Esc stops)"),
        T::CfMeasure => ("Meet randpunt met de laser", "Measure edge point with the laser"),
        T::CfAdd => ("Voeg punt toe", "Add point"),
        T::CfUndo => ("Herstel laatste", "Undo last"),
        T::CfClear => ("Wis punten", "Clear points"),
        T::CfPoints => ("{n} punten, nog {needed} nodig.", "{n} points, {needed} more needed."),
        T::CfResult => (
            "Middelpunt ({x}, {y}) mm, straal {r} mm, residu rms {rms} mm (max {max} mm).",
            "Centre ({x}, {y}) mm, radius {r} mm, residual rms {rms} mm (max {max} mm).",
        ),
        T::CfPoor => (
            "De punten bedekken te weinig van de cirkel: het middelpunt is slecht bepaald.",
            "The points cover too little of the circle: the centre is badly determined.",
        ),
        T::CfCenterMachine => {
            ("Middelpunt in machinecoördinaten ({x}, {y}) mm.", "Centre in machine coordinates ({x}, {y}) mm.")
        }
        T::TangentHeading => ("Raakcirkel", "Tangent Circle"),
        T::TangentPick => {
            ("Kies lijnen, cirkels of punten ({picked}/3).", "Pick lines, circles or points ({picked}/3).")
        }
        T::TangentNeedsDiameter => (
            "Typ de diameter in het maatveld, of kies een derde doel.",
            "Type the diameter in the size field, or pick a third target.",
        ),
        T::TangentSolutions => {
            ("{n} oplossingen. Klik om de dichtstbijzijnde te maken.", "{n} solutions. Click to create the nearest.")
        }
        T::ValueX => ("X (mm)", "X (mm)"),
        T::ValueY => ("Y (mm)", "Y (mm)"),
    }
}

/// A text of the panel in the window language.
pub fn schikken_text(t: SchikkenText, locale: Locale) -> String {
    let (nl, en) = pair(t);
    match locale {
        Locale::Nl => nl.to_owned(),
        Locale::En => en.to_owned(),
    }
}

/// A text with `{name}` placeholders filled in.
fn fill(t: SchikkenText, locale: Locale, values: &[(&str, String)]) -> String {
    let mut s = schikken_text(t, locale);
    for (name, value) in values {
        s = s.replace(&format!("{{{name}}}"), value);
    }
    s
}

fn mm(v: f64, locale: Locale) -> String {
    let s = burnburn_presentation::format_mm(v);
    match locale {
        Locale::Nl => s.replace('.', ","),
        Locale::En => s,
    }
}

/// `v` with a fixed number of decimals and the window language's decimal sign.
fn decimals(v: f64, places: usize, locale: Locale) -> String {
    let s = format!("{v:.places$}");
    match locale {
        Locale::Nl => s.replace('.', ","),
        Locale::En => s,
    }
}

fn percent(v: f64) -> String {
    format!("{:.0}", v * 100.0)
}

// -- The panel ----------------------------------------------------------------------------------

/// What the panel shows for `document`, if anything.
pub fn schikken_panel(vm: &AppViewModel, document: DocumentId, locale: Locale) -> Option<SchikkenPanel> {
    let doc = vm.document(document)?;
    let s = &doc.schikken;
    let t = |x| schikken_text(x, locale);
    let (heading, lines, flow) = if let Some(slots) = &s.slots {
        (t(SchikkenText::SlotsHeading), slots_lines(slots, locale), true)
    } else if let Some(nest) = &s.nest {
        let heading = match nest.scope {
            NestScope::Quick => SchikkenText::NestQuickHeading,
            NestScope::Selected => SchikkenText::NestSelectedHeading,
        };
        (t(heading), nest_lines(nest, locale), true)
    } else if let Some(pc) = &s.print_and_cut {
        (t(SchikkenText::PcHeading), print_and_cut_lines(pc, locale), true)
    } else if let Some(cf) = &s.center_finder {
        (t(SchikkenText::CfHeading), center_finder_lines(cf, locale), true)
    } else if let Some(r) = &s.reshape {
        let heading =
            if doc.tool == ToolKind::Deform { SchikkenText::DeformHeading } else { SchikkenText::WarpHeading };
        (t(heading), reshape_lines(doc, r, locale), true)
    } else if let Some(tangent) = &s.tangent {
        (t(SchikkenText::TangentHeading), tangent_lines(tangent, locale), true)
    } else {
        (t(SchikkenText::PlaceHeading), place_lines(&s.place.input, s.place.error, locale), false)
    };
    let mut all = vec![SchikkenLine::Heading(heading)];
    all.extend(lines);
    Some(SchikkenPanel { title: t(SchikkenText::PanelTitle), lines: all, flow })
}

fn place_lines(
    input: &PlaceOptionsInput,
    error: Option<burnburn_presentation::SchikkenMessage>,
    locale: Locale,
) -> Vec<SchikkenLine> {
    let t = |x| schikken_text(x, locale);
    let mut lines = vec![
        SchikkenLine::Choice {
            control: SchikkenControl::PlaceReference,
            caption: t(SchikkenText::PlaceReference),
            options: vec![t(SchikkenText::ReferenceFirst), t(SchikkenText::ReferenceLast)],
            selected: usize::from(matches!(input.reference, DockReference::LastSelected)),
        },
        SchikkenLine::Choice {
            control: SchikkenControl::PlaceContact,
            caption: t(SchikkenText::PlaceContact),
            options: vec![t(SchikkenText::ContactBounds), t(SchikkenText::ContactOutline)],
            selected: usize::from(input.contact == DockContactChoice::Outline),
        },
        SchikkenLine::Field {
            control: SchikkenControl::PlaceGap,
            caption: t(SchikkenText::PlaceGap),
            text: input.gap.clone(),
            invalid: error.is_some(),
        },
        SchikkenLine::Check {
            control: SchikkenControl::PlaceByGroup,
            caption: t(SchikkenText::PlaceByGroup),
            on: input.by_group,
        },
        SchikkenLine::Choice {
            control: SchikkenControl::PlaceAnchor,
            caption: t(SchikkenText::PlaceAnchor),
            options: vec![t(SchikkenText::AnchorStart), t(SchikkenText::AnchorEnd)],
            selected: usize::from(matches!(input.anchor, TogetherAnchorChoice::End)),
        },
    ];
    if let Some(e) = error {
        lines.push(SchikkenLine::Problem(message_or_key(burnburn_presentation::MessageKey::Schikken(e), locale)));
    }
    lines
}

fn reshape_lines(
    doc: &DocumentViewModel,
    r: &burnburn_presentation::ReshapeViewModel,
    locale: Locale,
) -> Vec<SchikkenLine> {
    let t = |x| schikken_text(x, locale);
    let mut lines = vec![SchikkenLine::Text(t(SchikkenText::ReshapeHint))];
    if doc.tool == ToolKind::Warp {
        lines.push(SchikkenLine::Check {
            control: SchikkenControl::WarpBilinear,
            caption: t(SchikkenText::ReshapeBilinear),
            on: r.bilinear,
        });
    }
    if r.simplified {
        lines.push(SchikkenLine::Text(t(SchikkenText::ReshapeSimplified)));
    }
    if let Some(p) = r.problem {
        lines.push(SchikkenLine::Problem(reshape_problem_text(p, locale)));
    }
    lines.push(SchikkenLine::Button {
        control: SchikkenControl::ReshapeApply,
        title: t(SchikkenText::ReshapeApply),
        enabled: r.modified && r.problem.is_none(),
    });
    lines.push(SchikkenLine::Button {
        control: SchikkenControl::ReshapeReset,
        title: t(SchikkenText::ReshapeReset),
        enabled: r.modified,
    });
    lines
}

fn slots_lines(d: &SlotsDialogViewModel, locale: Locale) -> Vec<SchikkenLine> {
    let t = |x| schikken_text(x, locale);
    let invalid = |field| {
        matches!(
            d.error,
            Some(burnburn_presentation::SchikkenMessage::Slots(burnburn_presentation::SlotsFailure::InvalidField(f))) if f == field
        )
    };
    use burnburn_presentation::SlotField;
    let mut lines = vec![
        SchikkenLine::Field {
            control: SchikkenControl::SlotsFrom,
            caption: t(SchikkenText::SlotsFrom),
            text: d.input.from.clone(),
            invalid: invalid(SlotField::From),
        },
        SchikkenLine::Field {
            control: SchikkenControl::SlotsTo,
            caption: t(SchikkenText::SlotsTo),
            text: d.input.to.clone(),
            invalid: invalid(SlotField::To),
        },
        SchikkenLine::Field {
            control: SchikkenControl::SlotsTolerance,
            caption: t(SchikkenText::SlotsTolerance),
            text: d.input.tolerance.clone(),
            invalid: invalid(SlotField::Tolerance),
        },
        SchikkenLine::Choice {
            control: SchikkenControl::SlotsSelect,
            caption: t(SchikkenText::SlotsSelect),
            options: vec![t(SchikkenText::SlotsSlots), t(SchikkenText::SlotsTabs), t(SchikkenText::SlotsBoth)],
            selected: match d.input.select {
                SlotChoice::Slots => 0,
                SlotChoice::Tabs => 1,
                SlotChoice::Both => 2,
            },
        },
    ];
    if let Some(e) = d.error {
        lines.push(SchikkenLine::Problem(message_or_key(burnburn_presentation::MessageKey::Schikken(e), locale)));
    }
    lines.push(SchikkenLine::Button { control: SchikkenControl::SlotsOk, title: t(SchikkenText::Ok), enabled: true });
    lines.push(SchikkenLine::Button {
        control: SchikkenControl::SlotsCancel,
        title: t(SchikkenText::Cancel),
        enabled: true,
    });
    lines
}

fn nest_lines(n: &NestPanelViewModel, locale: Locale) -> Vec<SchikkenLine> {
    let t = |x| schikken_text(x, locale);
    let running = n.run == NestRun::Running;
    let mut lines = Vec::new();
    if let Some(sheet) = n.sheet {
        if sheet.contour.is_some() {
            lines.push(SchikkenLine::Text(fill(
                SchikkenText::NestSheetContour,
                locale,
                &[("w", mm(sheet.width_mm, locale)), ("h", mm(sheet.height_mm, locale))],
            )));
        } else {
            let which = if sheet.is_bed { SchikkenText::NestSheetBed } else { SchikkenText::NestSheetBounds };
            lines.push(SchikkenLine::Text(format!(
                "{} ({} × {} mm)",
                t(which),
                mm(sheet.width_mm, locale),
                mm(sheet.height_mm, locale)
            )));
        }
    }
    if !n.sheet_choices.is_empty() {
        let mut options = vec![t(SchikkenText::NestSheetBedOption)];
        options.extend(n.sheet_choices.iter().map(|c| c.name.clone()));
        let selected =
            n.input.sheet_object.and_then(|o| n.sheet_choices.iter().position(|c| c.object == o)).map_or(0, |i| i + 1);
        lines.push(SchikkenLine::Choice {
            control: SchikkenControl::NestSheet,
            caption: t(SchikkenText::NestSheetCaption),
            options,
            selected,
        });
    }
    let invalid = |f| {
        matches!(
            n.error,
            Some(burnburn_presentation::SchikkenMessage::Nest(burnburn_presentation::NestFailure::InvalidField(x))) if x == f
        )
    };
    use burnburn_presentation::NestField;
    lines.push(SchikkenLine::Field {
        control: SchikkenControl::NestPadding,
        caption: t(SchikkenText::NestPadding),
        text: n.input.padding.clone(),
        invalid: invalid(NestField::Padding),
    });
    lines.push(SchikkenLine::Field {
        control: SchikkenControl::NestEdge,
        caption: t(SchikkenText::NestEdge),
        text: n.input.edge_buffer.clone(),
        invalid: invalid(NestField::EdgeBuffer),
    });
    lines.push(SchikkenLine::Field {
        control: SchikkenControl::NestStep,
        caption: t(SchikkenText::NestStep),
        text: n.input.step.clone(),
        invalid: invalid(NestField::Step),
    });
    lines.push(SchikkenLine::Choice {
        control: SchikkenControl::NestRotations,
        caption: t(SchikkenText::NestRotations),
        options: vec![
            t(SchikkenText::RotationsFixed),
            t(SchikkenText::RotationsHalf),
            t(SchikkenText::RotationsQuarter),
            t(SchikkenText::RotationsEighth),
        ],
        selected: match n.input.rotations {
            NestRotations::Fixed => 0,
            NestRotations::Half => 1,
            NestRotations::Quarter => 2,
            NestRotations::Eighth => 3,
        },
    });
    lines.push(SchikkenLine::Check {
        control: SchikkenControl::NestMirror,
        caption: t(SchikkenText::NestMirror),
        on: n.input.mirror == NestMirror::Allowed,
    });
    lines.push(SchikkenLine::Field {
        control: SchikkenControl::NestSheets,
        caption: t(SchikkenText::NestSheets),
        text: n.input.sheets.clone(),
        invalid: invalid(NestField::Sheets),
    });
    if let Some(e) = n.error {
        lines.push(SchikkenLine::Problem(message_or_key(burnburn_presentation::MessageKey::Schikken(e), locale)));
    }
    let status = match n.run {
        NestRun::Idle => fill(SchikkenText::NestIdle, locale, &[("parts", n.parts.to_string())]),
        NestRun::Running => t(SchikkenText::NestRunning),
        NestRun::Stale => t(SchikkenText::NestStale),
        NestRun::Done | NestRun::Applied => match &n.report {
            Some(r) => fill(
                SchikkenText::NestDone,
                locale,
                &[
                    ("placed", r.placed.to_string()),
                    ("unplaced", r.unplaced.iter().map(|u| u.objects.len()).sum::<usize>().to_string()),
                    ("skipped", r.skipped.len().to_string()),
                    ("percent", percent(r.utilisation)),
                ],
            ),
            None => String::new(),
        },
    };
    lines.push(SchikkenLine::Text(status));
    if n.run == NestRun::Applied {
        lines.push(SchikkenLine::Text(t(SchikkenText::NestApplied)));
    }
    if let Some(r) = &n.report {
        if r.mirrored > 0 {
            lines.push(SchikkenLine::Text(fill(
                SchikkenText::NestMirroredRow,
                locale,
                &[("count", r.mirrored.to_string())],
            )));
        }
        for (i, sheet) in r.sheets.iter().enumerate() {
            lines.push(SchikkenLine::Text(fill(
                SchikkenText::NestSheetUse,
                locale,
                &[
                    ("n", (i + 1).to_string()),
                    ("used", mm(sheet.placed_mm2 / 100.0, locale)),
                    ("free", mm(sheet.free_mm2 / 100.0, locale)),
                ],
            )));
        }
        let complete = r.parts.iter().filter(|p| p.placed >= p.ordered).count();
        lines.push(SchikkenLine::Text(fill(
            SchikkenText::NestSetsRow,
            locale,
            &[
                ("sets", r.complete_sets.to_string()),
                ("complete", complete.to_string()),
                ("total", r.parts.len().to_string()),
            ],
        )));
        for (i, part) in r.parts.iter().enumerate().filter(|(_, p)| p.placed < p.ordered) {
            lines.push(SchikkenLine::Problem(fill(
                SchikkenText::NestOrderRow,
                locale,
                &[
                    ("n", (i + 1).to_string()),
                    ("placed", part.placed.to_string()),
                    ("ordered", part.ordered.to_string()),
                ],
            )));
        }
        for row in &r.unplaced {
            lines.push(SchikkenLine::Problem(fill(
                SchikkenText::NestUnplacedRow,
                locale,
                &[("count", row.objects.len().to_string()), ("reason", nest_unplaced_text(row.reason, locale))],
            )));
        }
        for row in &r.skipped {
            lines.push(SchikkenLine::Text(fill(
                SchikkenText::NestSkippedRow,
                locale,
                &[("reason", nest_skip_text(row.reason, locale))],
            )));
        }
    }
    lines.push(SchikkenLine::Button {
        control: SchikkenControl::NestStart,
        title: t(SchikkenText::NestStart),
        enabled: !running,
    });
    lines.push(SchikkenLine::Button {
        control: SchikkenControl::NestStop,
        title: t(SchikkenText::NestStop),
        enabled: running,
    });
    lines.push(SchikkenLine::Button {
        control: SchikkenControl::NestApply,
        title: t(SchikkenText::NestApply),
        enabled: n.can_apply,
    });
    lines.push(SchikkenLine::Button {
        control: SchikkenControl::NestSelectUnplaced,
        title: t(SchikkenText::NestSelectUnplaced),
        enabled: n.report.as_ref().is_some_and(|r| !r.unplaced.is_empty()),
    });
    lines.push(SchikkenLine::Button {
        control: SchikkenControl::NestClose,
        title: t(SchikkenText::Close),
        enabled: true,
    });
    lines
}

fn point_text(p: Option<(f64, f64)>, axis: usize, locale: Locale) -> String {
    p.map(|p| mm(if axis == 0 { p.0 } else { p.1 }, locale)).unwrap_or_default()
}

fn print_and_cut_lines(pc: &PrintAndCutViewModel, locale: Locale) -> Vec<SchikkenLine> {
    use burnburn_presentation::{RegistrationField, SchikkenMessage};
    let t = |x| schikken_text(x, locale);
    let bad = |f| {
        matches!(
            pc.error,
            Some(SchikkenMessage::Registration(burnburn_presentation::RegistrationFailure::InvalidField(x))) if x == f
        )
    };
    let mut lines = vec![SchikkenLine::Text(t(SchikkenText::PcHint))];
    for i in 0..2 {
        lines.push(SchikkenLine::Heading(fill(SchikkenText::PcTarget, locale, &[("n", (i + 1).to_string())])));
        lines.push(SchikkenLine::Button {
            control: SchikkenControl::PcPick(i),
            title: if pc.picking == Some(i) { t(SchikkenText::PcPicking) } else { t(SchikkenText::PcPick) },
            enabled: true,
        });
        lines.push(SchikkenLine::Field {
            control: SchikkenControl::PcX(i),
            caption: t(SchikkenText::PcX),
            text: point_text(pc.targets[i], 0, locale),
            invalid: bad(RegistrationField::X),
        });
        lines.push(SchikkenLine::Field {
            control: SchikkenControl::PcY(i),
            caption: t(SchikkenText::PcY),
            text: point_text(pc.targets[i], 1, locale),
            invalid: bad(RegistrationField::Y),
        });
        lines.push(SchikkenLine::Button {
            control: SchikkenControl::PcMeasure(i),
            title: t(SchikkenText::PcMeasure),
            enabled: true,
        });
        lines.push(SchikkenLine::Field {
            control: SchikkenControl::PcMeasuredX(i),
            caption: t(SchikkenText::PcMeasuredX),
            text: point_text(pc.measured[i], 0, locale),
            invalid: false,
        });
        lines.push(SchikkenLine::Field {
            control: SchikkenControl::PcMeasuredY(i),
            caption: t(SchikkenText::PcMeasuredY),
            text: point_text(pc.measured[i], 1, locale),
            invalid: false,
        });
    }
    lines.push(SchikkenLine::Choice {
        control: SchikkenControl::PcScale,
        caption: t(SchikkenText::PcScale),
        options: vec![t(SchikkenText::ScaleFixed), t(SchikkenText::ScaleUniform)],
        selected: usize::from(pc.scale == PrintCutScale::Uniform),
    });
    if pc.scale == PrintCutScale::Uniform {
        lines.push(SchikkenLine::Field {
            control: SchikkenControl::PcDeviation,
            caption: t(SchikkenText::PcDeviation),
            text: pc.max_deviation.clone(),
            invalid: bad(RegistrationField::MaxDeviation),
        });
    }
    if let Some(e) = pc.error {
        lines.push(SchikkenLine::Problem(message_or_key(burnburn_presentation::MessageKey::Schikken(e), locale)));
    }
    if let Some(r) = &pc.result {
        lines.push(SchikkenLine::Text(fill(
            SchikkenText::PcResult,
            locale,
            &[
                ("angle", decimals(r.rotation_deg, 2, locale)),
                ("scale", decimals(r.scale, 4, locale)),
                ("rms", mm(r.rms_mm, locale)),
                ("max", mm(r.residuals_mm.iter().copied().fold(0.0, f64::max), locale)),
            ],
        )));
    }
    lines.push(SchikkenLine::Button {
        control: SchikkenControl::PcRegister,
        title: t(SchikkenText::PcRegister),
        enabled: true,
    });
    lines.push(SchikkenLine::Button {
        control: SchikkenControl::PcReset,
        title: t(SchikkenText::PcReset),
        enabled: true,
    });
    lines.push(SchikkenLine::Button {
        control: SchikkenControl::PcClose,
        title: t(SchikkenText::Close),
        enabled: true,
    });
    lines
}

fn center_finder_lines(cf: &CenterFinderViewModel, locale: Locale) -> Vec<SchikkenLine> {
    let t = |x| schikken_text(x, locale);
    let mut lines = vec![
        SchikkenLine::Text(fill(
            SchikkenText::CfPoints,
            locale,
            &[("n", cf.points.len().to_string()), ("needed", cf.needed.to_string())],
        )),
        SchikkenLine::Button {
            control: SchikkenControl::CfPick,
            title: if cf.picking { t(SchikkenText::CfPicking) } else { t(SchikkenText::CfPick) },
            enabled: true,
        },
        SchikkenLine::Button { control: SchikkenControl::CfMeasure, title: t(SchikkenText::CfMeasure), enabled: true },
        SchikkenLine::Field {
            control: SchikkenControl::CfX,
            caption: t(SchikkenText::ValueX),
            text: String::new(),
            invalid: false,
        },
        SchikkenLine::Field {
            control: SchikkenControl::CfY,
            caption: t(SchikkenText::ValueY),
            text: String::new(),
            invalid: false,
        },
        SchikkenLine::Button { control: SchikkenControl::CfAdd, title: t(SchikkenText::CfAdd), enabled: true },
    ];
    if let Some(e) = cf.error {
        lines.push(SchikkenLine::Problem(message_or_key(burnburn_presentation::MessageKey::Schikken(e), locale)));
    }
    if let Some(p) = cf.problem {
        lines.push(SchikkenLine::Problem(center_problem_text(p, locale)));
    }
    if let Some(fit) = &cf.fit {
        lines.push(SchikkenLine::Text(fill(
            SchikkenText::CfResult,
            locale,
            &[
                ("x", mm(fit.center.0, locale)),
                ("y", mm(fit.center.1, locale)),
                ("r", mm(fit.radius_mm, locale)),
                ("rms", mm(fit.rms_mm, locale)),
                ("max", mm(fit.max_residual_mm, locale)),
            ],
        )));
        if let Some(m) = fit.center_machine {
            lines.push(SchikkenLine::Text(fill(
                SchikkenText::CfCenterMachine,
                locale,
                &[("x", mm(m.0, locale)), ("y", mm(m.1, locale))],
            )));
        }
        if fit.poorly_determined {
            lines.push(SchikkenLine::Problem(t(SchikkenText::CfPoor)));
        }
    }
    lines.push(SchikkenLine::Button {
        control: SchikkenControl::CfUndo,
        title: t(SchikkenText::CfUndo),
        enabled: !cf.points.is_empty(),
    });
    lines.push(SchikkenLine::Button {
        control: SchikkenControl::CfClear,
        title: t(SchikkenText::CfClear),
        enabled: !cf.points.is_empty(),
    });
    lines.push(SchikkenLine::Button {
        control: SchikkenControl::CfClose,
        title: t(SchikkenText::Close),
        enabled: true,
    });
    lines
}

fn tangent_lines(tg: &TangentViewModel, locale: Locale) -> Vec<SchikkenLine> {
    let mut lines =
        vec![SchikkenLine::Text(fill(SchikkenText::TangentPick, locale, &[("picked", tg.picked.to_string())]))];
    if tg.picked == 2 && tg.radius_mm.is_none() {
        lines.push(SchikkenLine::Text(schikken_text(SchikkenText::TangentNeedsDiameter, locale)));
    }
    if tg.candidates > 0 {
        lines.push(SchikkenLine::Text(fill(
            SchikkenText::TangentSolutions,
            locale,
            &[("n", tg.candidates.to_string())],
        )));
    }
    if let Some(p) = tg.problem {
        lines.push(SchikkenLine::Problem(tangent_problem_text(p, locale)));
    }
    lines
}

// -- Controls → intents ---------------------------------------------------------------------------

/// What the panel reports about a control: its value as text. Fields send what is typed; a choice
/// sends the index of the chosen option; a check `1` or `0`.
pub type SchikkenValues = [(SchikkenControl, String)];

fn value(values: &SchikkenValues, control: SchikkenControl) -> &str {
    values.iter().find(|(c, _)| *c == control).map_or("", |(_, v)| v.as_str())
}

fn index(values: &SchikkenValues, control: SchikkenControl) -> usize {
    value(values, control).parse().unwrap_or(0)
}

/// The typed `x`/`y` are the point already there: a repeated commit (Return, then end of editing)
/// must not reset what depends on it.
fn same_point(shown: Option<(f64, f64)>, x: &str, y: &str) -> bool {
    use burnburn_presentation::parse_position;
    match (shown, parse_position(x), parse_position(y)) {
        (Some((sx, sy)), Ok(x), Ok(y)) => (sx - x.mm()).abs() < 1e-6 && (sy - y.mm()).abs() < 1e-6,
        _ => false,
    }
}

/// The intent a control sends. `values` are the panel's values at that moment (all of them), so a
/// dialog's OK can send every field and an option change keeps the others.
pub fn schikken_intent(
    vm: &AppViewModel,
    document: DocumentId,
    control: SchikkenControl,
    values: &SchikkenValues,
) -> Option<UiIntent> {
    let doc = vm.document(document)?;
    let s = &doc.schikken;
    let send = |i: SchikkenIntent| Some(UiIntent::Schikken(i));
    let pc = |i: PrintAndCutIntent| Some(UiIntent::Schikken(SchikkenIntent::PrintAndCut(i)));
    let cf = |i: CenterFinderIntent| Some(UiIntent::Schikken(SchikkenIntent::CenterFinder(i)));
    let invoke = |command| Some(UiIntent::Invoke { document: Some(document), command });
    let place = |f: &dyn Fn(&mut PlaceOptionsInput)| {
        let mut input = s.place.input.clone();
        f(&mut input);
        send(SchikkenIntent::SetPlaceOptions { document, options: input })
    };
    let nest = |f: &dyn Fn(&mut NestInput)| {
        let mut input = s.nest.as_ref()?.input.clone();
        f(&mut input);
        send(SchikkenIntent::SetNestOptions { document, input })
    };
    match control {
        SchikkenControl::PlaceReference => place(&|i| {
            i.reference =
                if index(values, control) == 1 { DockReference::LastSelected } else { DockReference::FirstSelected };
        }),
        SchikkenControl::PlaceContact => place(&|i| {
            i.contact =
                if index(values, control) == 1 { DockContactChoice::Outline } else { DockContactChoice::Bounds };
        }),
        SchikkenControl::PlaceGap => place(&|i| i.gap = value(values, control).to_owned()),
        SchikkenControl::PlaceByGroup => place(&|i| i.by_group = value(values, control) == "1"),
        SchikkenControl::PlaceAnchor => place(&|i| {
            i.anchor =
                if index(values, control) == 1 { TogetherAnchorChoice::End } else { TogetherAnchorChoice::Start };
        }),
        SchikkenControl::WarpBilinear => {
            send(SchikkenIntent::SetWarpKind { document, bilinear: value(values, control) == "1" })
        }
        SchikkenControl::ReshapeApply => invoke(CommandKey::ReshapeApply),
        SchikkenControl::ReshapeReset => invoke(CommandKey::ReshapeReset),
        SchikkenControl::SlotsFrom
        | SchikkenControl::SlotsTo
        | SchikkenControl::SlotsTolerance
        | SchikkenControl::SlotsSelect => None,
        SchikkenControl::SlotsOk => {
            s.slots.as_ref()?;
            send(SchikkenIntent::ApplySlots {
                document,
                input: SlotsInput {
                    from: value(values, SchikkenControl::SlotsFrom).to_owned(),
                    to: value(values, SchikkenControl::SlotsTo).to_owned(),
                    tolerance: value(values, SchikkenControl::SlotsTolerance).to_owned(),
                    select: match index(values, SchikkenControl::SlotsSelect) {
                        1 => SlotChoice::Tabs,
                        2 => SlotChoice::Both,
                        _ => SlotChoice::Slots,
                    },
                },
            })
        }
        SchikkenControl::SlotsCancel => send(SchikkenIntent::CancelSlots { document }),
        SchikkenControl::NestPadding => nest(&|i| i.padding = value(values, control).to_owned()),
        SchikkenControl::NestEdge => nest(&|i| i.edge_buffer = value(values, control).to_owned()),
        SchikkenControl::NestStep => nest(&|i| i.step = value(values, control).to_owned()),
        SchikkenControl::NestRotations => nest(&|i| {
            i.rotations = match index(values, control) {
                0 => NestRotations::Fixed,
                1 => NestRotations::Half,
                3 => NestRotations::Eighth,
                _ => NestRotations::Quarter,
            };
        }),
        SchikkenControl::NestMirror => nest(&|i| {
            i.mirror = if value(values, control) == "1" { NestMirror::Allowed } else { NestMirror::Forbidden };
        }),
        SchikkenControl::NestSheets => nest(&|i| i.sheets = value(values, control).to_owned()),
        SchikkenControl::NestSheet => {
            let object = {
                let choices = &s.nest.as_ref()?.sheet_choices;
                index(values, control).checked_sub(1).and_then(|i| choices.get(i)).map(|c| c.object)
            };
            nest(&|i| i.sheet_object = object)
        }
        SchikkenControl::NestStart => send(SchikkenIntent::StartNest { document }),
        SchikkenControl::NestStop => send(SchikkenIntent::CancelNest { document }),
        SchikkenControl::NestApply => send(SchikkenIntent::ApplyNest { document }),
        SchikkenControl::NestSelectUnplaced => send(SchikkenIntent::SelectNestUnplaced { document }),
        SchikkenControl::NestClose => send(SchikkenIntent::CloseNest { document }),
        SchikkenControl::PcPick(index) => pc(PrintAndCutIntent::PickTarget { document, index }),
        SchikkenControl::PcX(i) | SchikkenControl::PcY(i) => {
            let (x, y) = (value(values, SchikkenControl::PcX(i)), value(values, SchikkenControl::PcY(i)));
            if same_point(s.print_and_cut.as_ref()?.targets.get(i).copied().flatten(), x, y) {
                return None;
            }
            pc(PrintAndCutIntent::SetTarget { document, index: i, x: x.to_owned(), y: y.to_owned() })
        }
        SchikkenControl::PcMeasure(index) => pc(PrintAndCutIntent::MeasureTarget { document, index }),
        SchikkenControl::PcMeasuredX(i) | SchikkenControl::PcMeasuredY(i) => {
            let (x, y) =
                (value(values, SchikkenControl::PcMeasuredX(i)), value(values, SchikkenControl::PcMeasuredY(i)));
            if same_point(s.print_and_cut.as_ref()?.measured.get(i).copied().flatten(), x, y) {
                return None;
            }
            pc(PrintAndCutIntent::SetMeasured { document, index: i, x: x.to_owned(), y: y.to_owned() })
        }
        SchikkenControl::PcScale => pc(PrintAndCutIntent::SetScale {
            document,
            scale: if index(values, control) == 1 { PrintCutScale::Uniform } else { PrintCutScale::Fixed },
            max_deviation: s.print_and_cut.as_ref()?.max_deviation.clone(),
        }),
        SchikkenControl::PcDeviation => pc(PrintAndCutIntent::SetScale {
            document,
            scale: s.print_and_cut.as_ref()?.scale,
            max_deviation: value(values, control).to_owned(),
        }),
        SchikkenControl::PcRegister => pc(PrintAndCutIntent::Register { document }),
        SchikkenControl::PcReset => pc(PrintAndCutIntent::Reset { document }),
        SchikkenControl::PcClose => pc(PrintAndCutIntent::Close { document }),
        SchikkenControl::CfPick => cf(CenterFinderIntent::PickPoints { document }),
        SchikkenControl::CfX | SchikkenControl::CfY => None,
        SchikkenControl::CfAdd => cf(CenterFinderIntent::AddPoint {
            document,
            x: value(values, SchikkenControl::CfX).to_owned(),
            y: value(values, SchikkenControl::CfY).to_owned(),
        }),
        SchikkenControl::CfMeasure => cf(CenterFinderIntent::MeasurePoint { document }),
        SchikkenControl::CfUndo => cf(CenterFinderIntent::UndoLast { document }),
        SchikkenControl::CfClear => cf(CenterFinderIntent::Clear { document }),
        SchikkenControl::CfClose => cf(CenterFinderIntent::Close { document }),
    }
}
