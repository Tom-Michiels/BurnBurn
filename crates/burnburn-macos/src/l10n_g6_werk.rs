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
