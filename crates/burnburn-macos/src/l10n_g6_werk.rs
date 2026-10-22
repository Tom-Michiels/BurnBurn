//! Texts of the golf-6 workshop panels (G6-werk): preparing an image for tracing (T03, T05, T06).
//! NL/EN, kept apart so the older text files stay untouched; the Windows host compiles this file too.

use crate::l10n::Locale;

macro_rules! texts {
    ($($name:ident => ($nl:expr, $en:expr)),* $(,)?) => {
        /// A text only the golf-6 workshop panels show.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum G6Text {
            $($name),*
        }

        impl G6Text {
            /// Every text, for the completeness test.
            pub const ALL: &'static [G6Text] = &[$(G6Text::$name),*];
        }

        fn pair(text: G6Text) -> (&'static str, &'static str) {
            match text {
                $(G6Text::$name => ($nl, $en)),*
            }
        }
    };
}

texts! {
    // --- Prepare image (T03, T05, T06).
    TracePrepTitle => ("Beeld voorbereiden", "Prepare image"),
    TracePrepMenu => ("Beeld voorbereiden…", "Prepare image…"),
    ThresholdHeading => ("Drempel", "Threshold"),
    ThresholdGlobal => (
        "De globale drempel van het tracerblad is in gebruik.",
        "The global threshold of the tracer sheet is in use."
    ),
    ThresholdLocal => (
        "Lokale drempel in gebruik: venster {0} px, verschil {1}.",
        "Local threshold in use: window {0} px, offset {1}."
    ),
    RadiusCaption => ("Venster (px)", "Window (px)"),
    OffsetCaption => ("Donkerder dan het lokale gemiddelde (grijswaarden)", "Darker than the local mean (gray levels)"),
    UseLocal => ("Gebruik lokale drempel", "Use local threshold"),
    UseGlobal => ("Gebruik globale drempel", "Use global threshold"),
    PerspectiveHeading => ("Vlakcorrectie (4 punten)", "Plane correction (4 points)"),
    PerspectiveHint => (
        "Hoeken in pixels van de foto: linksboven, rechtsboven, rechtsonder, linksonder.",
        "Corners in pixels of the photo: top left, top right, bottom right, bottom left."
    ),
    CornerXCaption => ("Hoek {0}, x", "Corner {0}, x"),
    CornerYCaption => ("Hoek {0}, y", "Corner {0}, y"),
    CorrectedWidth => ("Breedte na correctie (px, leeg = langste rand)", "Width after correction (px, empty = longest edge)"),
    CorrectedHeight => ("Hoogte na correctie (px, leeg = langste rand)", "Height after correction (px, empty = longest edge)"),
    ApplyCorrection => ("Pas correctie toe", "Apply correction"),
    ClearCorrection => ("Verwijder correctie", "Remove correction"),
    TransformLine => (
        "Vlaktransformatie (foto naar gecorrigeerd): [{0} {1} {2} / {3} {4} {5} / {6} {7} 1]; grootste afwijking van de hoeken {8} px.",
        "Plane transform (photo to corrected): [{0} {1} {2} / {3} {4} {5} / {6} {7} 1]; largest corner error {8} px."
    ),
    MasksHeading => ("Maskers", "Masks"),
    MasksNone => ("Geen maskers.", "No masks."),
    MaskKind => ("Soort masker", "Kind of mask"),
    MaskKeep => ("Behouden (alleen hier tracen)", "Keep (trace only here)"),
    MaskRemove => ("Verwijderen (hier is achtergrond)", "Remove (background here)"),
    MaskXCaption => ("Gebied x (px)", "Area x (px)"),
    MaskYCaption => ("Gebied y (px)", "Area y (px)"),
    MaskWidthCaption => ("Gebied breedte (px)", "Area width (px)"),
    MaskHeightCaption => ("Gebied hoogte (px)", "Area height (px)"),
    AddMask => ("Voeg masker toe", "Add mask"),
    MaskLine => ("Masker {0}: {1}, {2}, {3}, {4} × {5} px", "Mask {0}: {1}, {2}, {3}, {4} × {5} px"),
    MaskKeepShort => ("behouden", "keep"),
    MaskRemoveShort => ("verwijderen", "remove"),
    RemoveMask => ("Verwijder masker {0}", "Remove mask {0}"),
    CorrectionsHeading => ("Lokale correctie", "Local correction"),
    RegionThresholdCaption => ("Drempel in het gebied (leeg = zoals de trace)", "Threshold in the area (empty = as the trace)"),
    AnchorsCaption => ("Vaste punten (x, y; x, y)", "Fixed points (x, y; x, y)"),
    RetraceRegion => ("Trace dit gebied opnieuw", "Trace this area again"),
    CorrectionsNone => ("Geen lokale correcties.", "No local corrections."),
    CorrectionLine => (
        "Correctie {0}: gebied {1}, {2}, {3} × {4} px, drempel {5}, {6} vast(e) punt(en): {7}",
        "Correction {0}: area {1}, {2}, {3} × {4} px, threshold {5}, {6} fixed point(s): {7}"
    ),
    CorrectionApplied => ("toegepast", "applied"),
    CorrectionRelink => (
        "niet toegepast: {0} contour(en) waaraan ze gebonden was bestaan niet meer en {1} nieuwe liggen in het gebied; verwijder haar en maak ze opnieuw",
        "not applied: {0} contour(s) it was bound to are gone and {1} new ones lie in the area; remove it and make it again"
    ),
    CorrectionFailed => ("mislukt", "failed"),
    RemoveCorrection => ("Verwijder correctie {0}", "Remove correction {0}"),
    ReportHeading => ("Rapport van de trace", "Trace report"),
    ReportBudget => (
        "Fit-tolerantie {0} mm; grootste afwijking {1} mm, 95 % binnen {2} mm.",
        "Fit tolerance {0} mm; largest deviation {1} mm, 95 % within {2} mm."
    ),
    ReportOver => (
        "De grootste afwijking is groter dan de fit-tolerantie: de pixelgrens laat geen nauwere pasvorm toe.",
        "The largest deviation is above the fit tolerance: the pixel boundary does not allow a tighter fit."
    ),
    ReportWorst => ("Afwijking {0} mm bij ({1}, {2}) mm", "Deviation {0} mm at ({1}, {2}) mm"),
    ReportShapes => ("{0} onderdelen en {1} gaten in de trace.", "{0} parts and {1} holes in the trace."),
    ReportRemoved => (
        "Weggelaten {0}: {1} mm² bij ({2}, {3}) mm",
        "Dropped {0}: {1} mm² at ({2}, {3}) mm"
    ),
    RemovedPart => ("onderdeel", "part"),
    RemovedHole => ("gat", "hole"),
    // --- Problems.
    IssueNumber => ("{0}: dit is geen getal.", "{0}: this is not a number."),
    IssueRange => ("{0}: de waarde ligt buiten wat kan.", "{0}: the value is out of range."),
    IssueCorners => (
        "De vier hoeken spannen geen vlak op: drie liggen op één lijn of twee vallen samen.",
        "The four corners span no plane: three are on one line or two coincide."
    ),
    IssueOutside => ("Het gebied of punt ligt buiten het beeld.", "The area or point lies outside the image."),
    IssueUnknown => ("Dit nummer bestaat niet.", "There is no such number."),
    IssueNothing => (
        "Er is geen afgeronde contourtrace om te corrigeren (wacht op het resultaat, of kies contourmodus).",
        "There is no finished contour trace to correct (wait for the result, or choose contour mode)."
    ),
    IssueStored => (
        "De correcties die bij deze trace horen konden niet gelezen worden; de trace begint zonder.",
        "The corrections stored with this trace could not be read; the trace starts without them."
    ),
    FieldRadius => ("Venster", "Window"),
    FieldOffset => ("Verschil", "Offset"),
    FieldCorner => ("Hoek {0}", "Corner {0}"),
    FieldWidth => ("Breedte", "Width"),
    FieldHeight => ("Hoogte", "Height"),
    FieldMaskX => ("Gebied x", "Area x"),
    FieldMaskY => ("Gebied y", "Area y"),
    FieldThreshold => ("Drempel", "Threshold"),
    FieldAnchors => ("Vaste punten", "Fixed points"),
    // --- Parts and references (S06, S07).
    DefineHeading => ("Onderdeel maken", "Make a part"),
    DefineSelected => (
        "{0} object(en) geselecteerd. Rechthoeken en cirkels worden parametrisch, de rest vaste vorm; de objecten blijven staan.",
        "{0} object(s) selected. Rectangles and circles become parametric, the rest fixed shapes; the objects stay."
    ),
    PartNameCaption => ("Naam van het onderdeel", "Name of the part"),
    DefineButton => ("Maak onderdeel van selectie", "Make part from selection"),
    PlaceXCaption => ("{0}: plaats x (mm)", "{0}: place at x (mm)"),
    PlaceYCaption => ("{0}: plaats y (mm)", "{0}: place at y (mm)"),
    PlaceButton => ("Plaats exemplaar van {0}", "Place an instance of {0}"),
    ParamCaption => ("{0}: standaardwaarde van {1} (mm)", "{0}: default of {1} (mm)"),
    VariantNameCaption => ("{0}: naam van een nieuwe variant", "{0}: name of a new variant"),
    AddVariantButton => ("Voeg variant toe aan {0}", "Add a variant to {0}"),
    VariantValueCaption => (
        "{0}, variant {1}: waarde van {2} (mm, leeg = zoals de standaard)",
        "{0}, variant {1}: value of {2} (mm, empty = as the default)"
    ),
    RemovePartButton => ("Verwijder onderdeel {0}", "Remove part {0}"),
    RemoveInstanceButton => ("Verwijder exemplaar {0}", "Remove instance {0}"),
    RefMakeHeading => ("Referentie maken", "Make a reference"),
    RefNoEdges => (
        "Selecteer precies één object om een van zijn randen als referentie te kiezen.",
        "Select exactly one object to choose one of its edges as a reference."
    ),
    RefNameCaption => ("Naam van de referentie", "Name of the reference"),
    RefEdgeCaption => ("Rand van het geselecteerde object", "Edge of the selected object"),
    RefEdgeItem => ("Rand {0}: {1} mm", "Edge {0}: {1} mm"),
    RefAddButton => ("Maak referentie", "Make reference"),
    RefCandidateCaption => ("{0}: welke rand wordt bedoeld", "{0}: which edge is meant"),
    RefRebindButton => ("Kies deze rand voor {0}", "Choose this edge for {0}"),
    RefRemoveButton => ("Verwijder referentie {0}", "Remove reference {0}"),
    // --- The compact operator view (W08).
    OperatorTitle => ("Operatorweergave", "Operator view"),
    OperatorMenu => ("Operatorweergave…", "Operator view…"),
    OperatorMachineHeading => ("Machine", "Machine"),
    OperatorNoMachine => ("Geen machine gekoppeld.", "No machine connected."),
    OperatorMachineLine => ("{0}: {1}", "{0}: {1}"),
    OperatorRunning => ("Job op de machine: {0}", "Job on the machine: {0}"),
    OperatorProgress => ("Upload: {0} van {1} blokken bevestigd door de transportlaag", "Upload: {0} of {1} chunks acknowledged by the transport"),
    OperatorJobsHeading => ("Goedgekeurde jobs", "Approved jobs"),
    OperatorJobsNone => ("Geen goedgekeurde jobs.", "No approved jobs."),
    OperatorJobLine => (
        "{0}: revisie {1}, {2}, materiaal {3}",
        "{0}: revision {1}, {2}, material {3}"
    ),
    OperatorSizeUnknown => ("afmeting onbekend", "size unknown"),
    OperatorMaterialUnknown => ("onbekend", "unknown"),
    OperatorUploadButton => ("Upload {0}", "Upload {0}"),
    OperatorOrdersHeading => ("Orders", "Orders"),
    OperatorOrdersNone => ("Geen orders.", "No orders."),
    OperatorOrderLine => (
        "{0}: {1} van {2} voltooid ({3}){4}",
        "{0}: {1} of {2} completed ({3}){4}"
    ),
    OperatorOnRemnant => (" op restplaat {0}", " on remnant {0}"),
    OperatorNeedsChoice => (
        "{0}: meerdere goedgekeurde jobs passen; er wordt er niet vanzelf een gekozen.",
        "{0}: several approved jobs fit; none is chosen automatically."
    ),
    OperatorOrderUploadButton => ("Upload voor order {0}", "Upload for order {0}"),
    // --- Why a structure edit was refused.
    ProblemNeedsSelection => (
        "Selecteer eerst de objecten waar het onderdeel van gemaakt wordt.",
        "Select the objects the part is made of first."
    ),
    ProblemNeedsName => ("Geef een naam.", "Give a name."),
    ProblemUnknownPart => ("Dit onderdeel bestaat niet.", "This part does not exist."),
    ProblemUnknownInstance => ("Dit exemplaar bestaat niet.", "This instance does not exist."),
    ProblemPartInUse => (
        "Het onderdeel wordt nog gebruikt door een exemplaar: verwijder eerst de exemplaren.",
        "The part is still used by an instance: remove the instances first."
    ),
    ProblemUnknownVariant => ("Deze variant bestaat niet.", "This variant does not exist."),
    ProblemVariantExists => ("Er is al een variant met deze naam.", "A variant with this name exists already."),
    ProblemUnknownParameter => ("Dit onderdeel heeft deze parameter niet.", "This part has no such parameter."),
    ProblemUnknownObject => ("Dit object bestaat niet.", "This object does not exist."),
    ProblemNotAnEdge => ("Het object heeft deze rand niet.", "The object has no such edge."),
    ProblemUnknownReference => ("Deze referentie bestaat niet.", "This reference does not exist."),
    ProblemNotAmbiguous => (
        "Alleen een dubbelzinnige referentie kan opnieuw gekoppeld worden.",
        "Only an ambiguous reference can be linked again."
    ),
    ProblemNoCandidate => ("Deze rand is geen kandidaat.", "This edge is not a candidate."),
}

/// `text` in `locale`.
pub fn g6_text(text: G6Text, locale: Locale) -> &'static str {
    let (nl, en) = pair(text);
    match locale {
        Locale::Nl => nl,
        Locale::En => en,
    }
}

/// `text` with `{0}`, `{1}`, ... replaced by `values`.
pub fn g6_fmt(text: G6Text, locale: Locale, values: &[&str]) -> String {
    let mut s = g6_text(text, locale).to_owned();
    for (i, v) in values.iter().enumerate() {
        s = s.replace(&format!("{{{i}}}"), v);
    }
    s
}

/// The words for a refused structure edit.
pub fn structure_problem_text(problem: burnburn_presentation::StructureProblem, locale: Locale) -> String {
    use burnburn_presentation::StructureProblem as P;
    let text = match problem {
        P::NeedsSelection => G6Text::ProblemNeedsSelection,
        P::NeedsName => G6Text::ProblemNeedsName,
        P::UnknownPart => G6Text::ProblemUnknownPart,
        P::UnknownInstance => G6Text::ProblemUnknownInstance,
        P::PartInUse => G6Text::ProblemPartInUse,
        P::UnknownVariant => G6Text::ProblemUnknownVariant,
        P::VariantExists => G6Text::ProblemVariantExists,
        P::UnknownParameter => G6Text::ProblemUnknownParameter,
        P::UnknownObject => G6Text::ProblemUnknownObject,
        P::NotAnEdge => G6Text::ProblemNotAnEdge,
        P::UnknownReference => G6Text::ProblemUnknownReference,
        P::NotAmbiguous => G6Text::ProblemNotAmbiguous,
        P::NoCandidate | _ => G6Text::ProblemNoCandidate,
    };
    g6_text(text, locale).to_owned()
}
