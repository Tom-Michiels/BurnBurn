//! Texts of the golf-7 trace panels (G7-trace): lens, pointing, protected contours, node goal, dropped
//! pieces, own recipes, A/B comparison and batch. NL/EN, kept apart so the older text files stay
//! untouched; the Windows host compiles this file too.

use crate::l10n::Locale;

macro_rules! texts {
    ($($name:ident => ($nl:expr, $en:expr)),* $(,)?) => {
        /// A text only the golf-7 trace panels show.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum G7Text {
            $($name),*
        }

        impl G7Text {
            /// Every text, for the completeness test.
            pub const ALL: &'static [G7Text] = &[$(G7Text::$name),*];
        }

        fn pair(text: G7Text) -> (&'static str, &'static str) {
            match text {
                $(G7Text::$name => ($nl, $en)),*
            }
        }
    };
}

texts! {
    // --- Lens (T03).
    LensHeading => ("Lensvervorming", "Lens distortion"),
    LensHint => (
        "Radiale vervorming van de camera: negatief is tonvormig. Wordt voor de vlakcorrectie rechtgezet.",
        "Radial distortion of the camera: negative is barrel. Corrected before the plane correction."
    ),
    LensOn => ("Lenscorrectie in gebruik: k1 {0}, k2 {1}.", "Lens correction in use: k1 {0}, k2 {1}."),
    LensOnTangential => (
        "Lenscorrectie in gebruik: k1 {0}, k2 {1}, p1 {2}, p2 {3}.",
        "Lens correction in use: k1 {0}, k2 {1}, p1 {2}, p2 {3}."
    ),
    LensOff => ("Geen lenscorrectie.", "No lens correction."),
    LensK1Caption => ("k1 (negatief = tonvormig)", "k1 (negative = barrel)"),
    LensK2Caption => ("k2", "k2"),
    LensP1Caption => ("p1 (tangentieel, lens uit het midden)", "p1 (tangential, lens off-centre)"),
    LensP2Caption => ("p2 (tangentieel)", "p2 (tangential)"),
    LensApply => ("Pas lenscorrectie toe", "Apply lens correction"),
    LensClear => ("Verwijder lenscorrectie", "Remove lens correction"),
    // --- Pointing (T03).
    PickHeading => ("Aanwijzen in het beeld", "Pointing in the image"),
    PickHint => (
        "De foto komt op het blad te liggen: klik de hoeken aan of sleep met de kwast. Escape stopt.",
        "The photo is laid on the sheet: click the corners or drag the brush. Escape stops."
    ),
    PickNone => ("Er wordt niets aangewezen.", "Nothing is being pointed at."),
    PickCorners => ("Er zijn {0} van 4 hoeken aangeklikt.", "{0} of 4 corners clicked."),
    PickBrushKeep => ("Kwast in de hand: behouden, straal {0} px.", "Brush in hand: keep, radius {0} px."),
    PickBrushRemove => ("Kwast in de hand: verwijderen, straal {0} px.", "Brush in hand: remove, radius {0} px."),
    BrushRadiusCaption => ("Straal van de kwast (px)", "Radius of the brush (px)"),
    PickCornersButton => ("Hoeken aanklikken", "Click the corners"),
    PickBrushKeepButton => ("Kwast: behouden", "Brush: keep"),
    PickBrushRemoveButton => ("Kwast: verwijderen", "Brush: remove"),
    PickEndButton => ("Stop met aanwijzen", "Stop pointing"),
    BrushMask => (
        "Masker {0}: {1} met kwast, straal {2} px, rond ({3}, {4}) px",
        "Mask {0}: {1} with brush, radius {2} px, around ({3}, {4}) px"
    ),
    // --- Contours: protect and keep (T05, T06).
    ContoursHeading => ("Contouren van de trace", "Contours of the trace"),
    ContoursHint => (
        "Beschermd: blijft bij de volgende lokale correctie precies zoals hij is. Behouden: valt nooit onder \"negeer kleiner dan\".",
        "Protected: stays exactly as it is in the next local correction. Kept: never dropped by \"ignore less than\"."
    ),
    ContourLine => ("Contour {0}: {1}, {2} mm², bij ({3}, {4}) mm", "Contour {0}: {1}, {2} mm², at ({3}, {4}) mm"),
    ContourPart => ("onderdeel", "part"),
    ContourHole => ("gat", "hole"),
    ContourProtectCaption => ("Contour {0} beschermen tegen de volgende correctie", "Protect contour {0} from the next correction"),
    ContourKeepCaption => ("Contour {0} behouden bij \"negeer kleiner dan\"", "Keep contour {0} against \"ignore less than\""),
    NodeHeading => ("Knooppuntdoel van de volgende correctie", "Node goal of the next correction"),
    NodeLimitCaption => ("Hoogstens knooppunten (heel resultaat)", "At most nodes (whole result)"),
    NodeToleranceCaption => ("Grofste tolerantie (mm)", "Coarsest tolerance (mm)"),
    NodeApply => ("Stel knooppuntdoel in (leeg = geen doel)", "Set node goal (empty = no goal)"),
    NodeSet => ("Doel: hoogstens {0} knooppunten, tolerantie tot {1} mm.", "Goal: at most {0} nodes, tolerance up to {1} mm."),
    NodeNone => ("Geen knooppuntdoel.", "No node goal."),
    CorrectionExtras => (
        "Correctie {0}: {1} contour(en) beschermd{2}",
        "Correction {0}: {1} contour(s) protected{2}"
    ),
    CorrectionGoalMet => (
        "; knooppuntdoel {0} gehaald met {1} knooppunten (tolerantie {2} mm)",
        "; node goal {0} met with {1} nodes (tolerance {2} mm)"
    ),
    CorrectionGoalMissed => (
        "; knooppuntdoel {0} NIET gehaald: {1} knooppunten bij tolerantie {2} mm",
        "; node goal {0} NOT met: {1} nodes at tolerance {2} mm"
    ),
    RestoreHeading => ("Weggelaten stukken", "Dropped pieces"),
    RestoreLine => ("Stuk {0}: {1}, {2} mm² bij ({3}, {4}) mm", "Piece {0}: {1}, {2} mm² at ({3}, {4}) mm"),
    RestoreButton => ("Zet stuk {0} terug", "Bring piece {0} back"),
    // --- Own recipes (T10).
    RecipesHeading => ("Eigen recepten", "Own recipes"),
    RecipesHint => (
        "Een recept bewaart drempel, filters, tolerantie en lens; niet de resolutie, uitsnede, maskers of correcties van dit ene beeld.",
        "A recipe keeps threshold, filters, tolerance and lens; not the resolution, crop, masks or corrections of this one picture."
    ),
    RecipesNone => ("Nog geen eigen recepten.", "No own recipes yet."),
    RecipeNameCaption => ("Naam van het recept", "Name of the recipe"),
    RecipeSave => ("Bewaar de huidige instellingen als recept", "Keep the current settings as a recipe"),
    RecipeLine => ("Recept: {0} ({1})", "Recipe: {0} ({1})"),
    RecipeContour => ("contour", "contour"),
    RecipeCentreLine => ("centerline", "centre line"),
    RecipeApply => ("Pas \"{0}\" toe", "Apply \"{0}\""),
    RecipeCompare => ("Vergelijk met \"{0}\"", "Compare with \"{0}\""),
    RecipeDelete => ("Verwijder \"{0}\"", "Delete \"{0}\""),
    RecipePresetCaption => ("Vergelijk met een ingebouwd recept", "Compare with a built-in recipe"),
    RecipeComparePreset => ("Vergelijk met dit recept", "Compare with this recipe"),
    RecipeLoadProblem => (
        "Het receptenbestand in de gebruikersmap kon niet gelezen worden; het blijft ongemoeid tot je iets bewaart.",
        "The recipe file in the user folder could not be read; it is left alone until you save something."
    ),
    RecipeSaveFailed => (
        "Het receptenbestand kon niet geschreven worden; de recepten van deze sessie blijven in gebruik.",
        "The recipe file could not be written; the recipes of this session stay in use."
    ),
    // --- A/B comparison (T10).
    CompareHeading => ("Vergelijking A/B", "Comparison A/B"),
    CompareComputing => ("Vergelijking wordt berekend…", "Computing the comparison…"),
    CompareWith => ("B is: {0}", "B is: {0}"),
    CompareA => (
        "A (huidig): {0} paden, {1} onderdelen, {2} gaten, {3} knooppunten, {4} weggelaten, grootste afwijking {5} mm",
        "A (current): {0} paths, {1} parts, {2} holes, {3} nodes, {4} dropped, largest deviation {5} mm"
    ),
    CompareB => (
        "B: {0} paden, {1} onderdelen, {2} gaten, {3} knooppunten, {4} weggelaten, grootste afwijking {5} mm",
        "B: {0} paths, {1} parts, {2} holes, {3} nodes, {4} dropped, largest deviation {5} mm"
    ),
    CompareSame => ("A en B geven precies dezelfde geometrie.", "A and B give exactly the same geometry."),
    CompareDiffer => ("A en B geven verschillende geometrie (zwart = A, rood = B, zelfde schaal).", "A and B give different geometry (black = A, red = B, same scale)."),
    CompareOnSheet => (
        "De omtrekken van A en B liggen ook op het blad, waar toepassen ze zou zetten.",
        "The outlines of A and B are also on the sheet, where applying would put them."
    ),
    CompareCorrections => (
        "Lokale correcties: {0} bewaard; A past er {1} toe, B past er {2} toe (een correctie die op de contouren van B niet meer past, wordt daar niet toegepast).",
        "Local corrections: {0} kept; A applies {1}, B applies {2} (a correction that no longer fits the contours of B is not applied there)."
    ),
    CompareAdopt => ("Neem B over als huidig recept", "Take B as the current recipe"),
    CompareClear => ("Sluit de vergelijking", "Close the comparison"),
    // --- Batch (T10).
    BatchHeading => ("Meerdere afbeeldingen tracen", "Trace several pictures"),
    BatchHint => (
        "Eén pad per regel. Alle bestanden worden getraced met de huidige instellingen; je krijgt een rapport per bestand.",
        "One path per line. Every file is traced with the current settings; you get a report per file."
    ),
    BatchFilesCaption => ("Bestanden", "Files"),
    BatchChoose => ("Kies bestanden…", "Choose files…"),
    BatchRun => ("Trace deze bestanden", "Trace these files"),
    BatchCancel => ("Annuleer de batch", "Cancel the batch"),
    BatchClear => ("Sluit het batchrapport", "Close the batch report"),
    BatchRunning => ("Bezig met {0} bestand(en)…", "Working on {0} file(s)…"),
    BatchCancelled => ("De batch is geannuleerd; er is geen rapport.", "The batch was cancelled; there is no report."),
    BatchSummary => ("{0} bestand(en), {1} met iets om naar te kijken.", "{0} file(s), {1} with something to look at."),
    BatchRow => (
        "{0}: {1} onderdelen, {2} gaten, {3} knooppunten",
        "{0}: {1} parts, {2} holes, {3} nodes"
    ),
    BatchRowNone => ("{0}: niet getraced", "{0}: not traced"),
    BatchNoGeometry => ("niets getraced (leeg beeld?)", "nothing traced (empty picture?)"),
    BatchRemoved => ("{0} stuk(ken) weggelaten door \"negeer kleiner dan\"", "{0} piece(s) dropped by \"ignore less than\""),
    BatchNeedsScale => ("geen resolutie in het bestand en geen in het recept", "no resolution in the file and none in the recipe"),
    BatchUnreadable => ("kon niet gelezen worden of is geen afbeelding", "could not be read or is not a picture"),
    BatchFailed => ("de trace mislukte", "the trace failed"),
    // --- Problems of the new fields and actions.
    FieldLensK1 => ("Lens k1", "Lens k1"),
    FieldLensK2 => ("Lens k2", "Lens k2"),
    FieldLensP1 => ("Lens p1", "Lens p1"),
    FieldLensP2 => ("Lens p2", "Lens p2"),
    FieldNodeLimit => ("Hoogstens knooppunten", "At most nodes"),
    FieldNodeTolerance => ("Grofste tolerantie", "Coarsest tolerance"),
    FieldBrushRadius => ("Straal van de kwast", "Radius of the brush"),
    IssueUnknownContour => ("Deze contour staat niet in het resultaat.", "This contour is not in the result."),
    IssueUnknownRemoved => ("Dit weggelaten stuk staat niet in het rapport.", "This dropped piece is not in the report."),
    IssuePickNotStarted => ("Er wordt niets aangewezen: kies eerst hoeken of de kwast.", "Nothing is being pointed at: choose corners or the brush first."),
    IssueInvalidStroke => ("Een streek heeft minstens één punt nodig.", "A stroke needs at least one point."),
    IssueCornersTooClose => ("Deze hoek ligt op een al aangeklikte hoek.", "This corner is on one already clicked."),
    RecipeIssueEmptyName => ("Geef het recept een naam.", "Give the recipe a name."),
    RecipeIssueNameTooLong => ("De naam is te lang (hoogstens 80 tekens).", "The name is too long (80 characters at most)."),
    RecipeIssueTooMany => ("Er zijn al te veel recepten.", "There are too many recipes already."),
    RecipeIssueUnknown => ("Dit recept bestaat niet.", "There is no such recipe."),
    RecipeIssueContourOnly => ("Vergelijken en batchen kan alleen bij een contourtrace.", "Comparing and batch tracing only work for a contour trace."),
    RecipeIssueNeedsScale => ("Geef eerst een resolutie op: zonder is er niets fysieks om te vergelijken.", "Give a resolution first: without one there is nothing physical to compare."),
    RecipeIssueNoFiles => ("Geen bestanden opgegeven.", "No files given."),
    RecipeIssueInvalid => ("Dit recept past niet bij dit beeld of kon niet berekend worden.", "This recipe does not fit this picture or could not be computed."),
}

pub fn g7_text(text: G7Text, locale: Locale) -> &'static str {
    let (nl, en) = pair(text);
    match locale {
        Locale::Nl => nl,
        Locale::En => en,
    }
}

/// `text` with `{0}`, `{1}`, ... replaced by `values`.
pub fn g7_fmt(text: G7Text, locale: Locale, values: &[&str]) -> String {
    let mut s = g7_text(text, locale).to_owned();
    for (i, v) in values.iter().enumerate() {
        s = s.replace(&format!("{{{i}}}"), v);
    }
    s
}
