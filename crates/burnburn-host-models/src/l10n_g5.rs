//! Texts of the golf-5 panels (G5-native): project notes, offset and repair, centre-line decisions
//! and the structure panel, NL/EN. Kept apart so `l10n.rs` stays untouched; the Windows host
//! compiles the same file.

use crate::l10n::Locale;

macro_rules! texts {
    ($($name:ident => ($nl:expr, $en:expr)),* $(,)?) => {
        /// A text only the golf-5 panels show.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum G5Text {
            $($name),*
        }

        impl G5Text {
            /// Every text, for the completeness test.
            pub const ALL: &'static [G5Text] = &[$(G5Text::$name),*];
        }

        fn pair(text: G5Text) -> (&'static str, &'static str) {
            match text {
                $(G5Text::$name => ($nl, $en)),*
            }
        }
    };
}

texts! {
    // --- Notes (P04).
    NotesTitle => ("Notities", "Notes"),
    NotesSave => ("Bewaar notities", "Save Notes"),
    NotesClose => ("Sluit", "Close"),
    NotesPartName => (
        "Onderdeelnaam (zo noemt een order dit product; los van de namen van de getekende objecten)",
        "Part name (how an order names this product; separate from the names of the drawn objects)"
    ),
    NotesPartNameSave => ("Bewaar onderdeelnaam", "Save part name"),
    NotesLimit => (
        "{0} van maximaal {1} tekens; opslaan is één stap die je kunt terugdraaien.",
        "{0} of at most {1} characters; saving is one undoable step."
    ),
    NotesTooLong => (
        "De tekst is langer dan {0} tekens en wordt niet bewaard.",
        "The text is longer than {0} characters and will not be saved."
    ),
    // --- Offset and repair (E05, E09).
    OffsetTitle => ("Offset en reparatie", "Offset and Repair"),
    OffsetMenu => ("Offset en reparatie", "Offset and Repair"),
    OffsetHeading => ("Offset van de selectie", "Offset of the selection"),
    OffsetDistance => ("Afstand (negatief = naar binnen)", "Distance (negative = inward)"),
    OffsetJoin => ("Hoeken", "Corners"),
    JoinRound => ("Rond", "Round"),
    JoinMiter => ("Scherp", "Mitred"),
    JoinBevel => ("Afgeschuind", "Bevelled"),
    OffsetKeep => ("Bron behouden", "Keep the source"),
    OffsetPreview => ("Voorbeeld", "Preview"),
    OffsetApply => ("Pas offset toe", "Apply Offset"),
    OffsetPreviewLine => (
        "Voorbeeld: {0} × {1} mm, oppervlak {2} mm². Het document is nog niet gewijzigd.",
        "Preview: {0} × {1} mm, area {2} mm². The document has not changed yet."
    ),
    RepairHeading => ("Reparatie van open contouren", "Repair of open contours"),
    RepairNone => ("Geen problemen gevonden.", "No problems found."),
    RepairIssue => ("Probleem", "Problem"),
    RepairTolerance => ("Tolerantie", "Tolerance"),
    RepairPreview => ("Voor en na", "Before and After"),
    RepairApply => ("Repareer", "Repair"),
    IssueOpen => ("{0}: open contour, opening {1} mm", "{0}: open contour, gap {1} mm"),
    IssueCrossing => ("{0}: zelfdoorsnijding", "{0}: self-intersection"),
    IssueOther => ("{0}: ander probleem", "{0}: other problem"),
    RepairResult => (
        "Na reparatie: {0} openingen sluiten, {1} blijven open. Het document is nog niet gewijzigd.",
        "After repair: {0} gaps close, {1} stay open. The document has not changed yet."
    ),
    // --- Centre-line decisions (T04, T08).
    CenterlineTitle => ("Centerline-beslissingen", "Centre-line decisions"),
    CenterlineNothing => ("Er valt niets te beslissen.", "There is nothing to decide."),
    CenterlineEdits => ("Genomen beslissingen: {0}", "Decisions taken: {0}"),
    CenterlineDropped => (
        "{0} bewaarde beslissing(en) passen niet meer en zijn niet toegepast.",
        "{0} stored decision(s) no longer fit and were not applied."
    ),
    JunctionOpen => (
        "Knooppunt {0} bij ({1}, {2}) mm: kies welke takken doorlopen.",
        "Junction {0} at ({1}, {2}) mm: choose which branches continue."
    ),
    JunctionDone => ("Knooppunt {0} bij ({1}, {2}) mm: beslist.", "Junction {0} at ({1}, {2}) mm: decided."),
    JunctionPair => ("Koppeling {1} van knooppunt {0}", "Pairing {1} of junction {0}"),
    ChainLine => ("Keten {0}: {1}", "Chain {0}: {1}"),
    ChainConfirmed => ("Keten {0}: bevestigd als {1}.", "Chain {0}: confirmed as {1}."),
    ChainNoFit => ("geen exacte vorm binnen de tolerantie", "no exact shape within the tolerance"),
    ChainConfirm => ("Bevestig {1} voor keten {0}", "Confirm {1} for chain {0}"),
    PrimLine => ("lijn", "line"),
    PrimArc => ("boog", "arc"),
    PrimCircle => ("cirkel", "circle"),
    Within => ("binnen tolerantie", "within tolerance"),
    Outside => ("erbuiten", "outside"),
    Residual => ("{0} {1} (afwijking {2} mm)", "{0} {1} (deviation {2} mm)"),
    // --- Structure (S06, S07, P06).
    StructureTitle => ("Structuur", "Structure"),
    StructureMenu => ("Structuur", "Structure"),
    PlatesHeading => ("Platen", "Plates"),
    PlateCompile => ("Compileer met", "Compile with"),
    PlateWhole => ("Het hele ontwerp", "The whole design"),
    PlateLine => (
        "{0}: {1} × {2} mm, {3}, {4} onderdelen, {5} objecten",
        "{0}: {1} × {2} mm, {3}, {4} parts, {5} objects"
    ),
    PlateThickness => ("{0} mm dik", "{0} mm thick"),
    PlateNoThickness => ("dikte onbekend", "thickness unknown"),
    PlateNone => ("Geen platen.", "No plates."),
    IssueTargetOff => (
        "Bewerking {0}: object {1} staat op een andere plaat en blijft buiten dit plan.",
        "Operation {0}: object {1} is on another plate and stays out of this plan."
    ),
    IssueBlocked => (
        "Object {0} is afgeleid van een onderdeel dat niet klopt: productie wordt geweigerd.",
        "Object {0} derives from a part that does not evaluate: production is refused."
    ),
    PartsHeading => ("Onderdelen", "Parts"),
    PartsNone => ("Geen onderdelen.", "No parts."),
    PartLine => ("{0}: {1}", "{0}: {1}"),
    InstanceLine => ("Exemplaar {0}{1}: {2}", "Instance {0}{1}: {2}"),
    OnPlate => (" op {0}", " on {0}"),
    StatusValid => ("in orde", "valid"),
    StatusPending => ("nog niet berekend", "not evaluated yet"),
    StatusFailed => ("mislukt", "failed"),
    StatusFailedParam => ("mislukt bij parameter {0}", "failed at parameter {0}"),
    VariantCaption => ("Variant van exemplaar {0}", "Variant of instance {0}"),
    VariantNone => ("(geen variant)", "(no variant)"),
    OverrideCaption => ("{1} van exemplaar {0} (alleen dit exemplaar)", "{1} of instance {0} (this instance only)"),
    EffectiveLine => ("Geldt nu: {0}", "In effect: {0}"),
    ReferencesHeading => ("Referenties", "References"),
    ReferencesNone => ("Geen referenties.", "No references."),
    RefBound => ("{0} → {1}: gebonden", "{0} → {1}: bound"),
    RefAmbiguous => ("{0}: dubbelzinnig, {1} kandidaten; kies er een", "{0}: ambiguous, {1} candidates; choose one"),
    RefMissing => ("{0}: gebroken, het object bestaat niet meer", "{0}: broken, the object no longer exists"),
    RefOrigin => (
        "{0}: gebroken, de rand heeft geen oorsprong meer",
        "{0}: broken, the edge has no origin any more"
    ),
}

/// `text` in `locale`.
pub fn g5_text(text: G5Text, locale: Locale) -> &'static str {
    let (nl, en) = pair(text);
    match locale {
        Locale::Nl => nl,
        Locale::En => en,
    }
}

/// `text` with `{0}`, `{1}`, ... replaced by `values`.
pub fn g5_fmt(text: G5Text, locale: Locale, values: &[&str]) -> String {
    let mut s = g5_text(text, locale).to_owned();
    for (i, v) in values.iter().enumerate() {
        s = s.replace(&format!("{{{i}}}"), v);
    }
    s
}
