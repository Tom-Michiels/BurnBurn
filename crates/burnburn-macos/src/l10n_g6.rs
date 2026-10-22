//! Texts of the golf-6 editor panels (G6-editor): the generator panel (box, living hinge; E10) and
//! the project search panel (P08), NL/EN. Kept apart so `l10n.rs` stays untouched; the Windows host
//! compiles the same file.

use crate::l10n::Locale;

macro_rules! texts {
    ($($name:ident => ($nl:expr, $en:expr)),* $(,)?) => {
        /// A text only the golf-6 editor panels show.
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
    // --- Generator panel (E10).
    GeneratorTitle => ("Doos of scharnier", "Box or Hinge"),
    GeneratorMenu => ("Doos of scharnier…", "Box or Hinge…"),
    KindCaption => ("Soort", "Kind"),
    KindBox => ("Vingerverbonden doos", "Finger-jointed box"),
    KindHinge => ("Levend scharnier", "Living hinge"),
    FieldBoxWidth => ("Breedte (buitenmaat)", "Width (outer)"),
    FieldBoxDepth => ("Diepte (buitenmaat)", "Depth (outer)"),
    FieldBoxHeight => ("Hoogte (buitenmaat)", "Height (outer)"),
    FieldThickness => ("Materiaaldikte", "Material thickness"),
    FieldFingerWidth => ("Vingerbreedte", "Finger width"),
    FieldPanelSpacing => ("Ruimte tussen delen", "Gap between panels"),
    FieldHingeWidth => ("Breedte (over de buiging)", "Width (across the bend)"),
    FieldHingeHeight => ("Lengte (langs de sneden)", "Length (along the cuts)"),
    FieldCutLength => ("Snijlengte", "Cut length"),
    FieldBridge => ("Brug tussen sneden", "Bridge between cuts"),
    FieldColumnSpacing => ("Afstand tussen kolommen", "Distance between columns"),
    LidOff => ("Deksel weglaten", "Leave the lid off"),
    KerfNote => (
        "De dikte is de pasvorm van de vingers; de kerf past de snijopdracht één keer toe, niet de generator.",
        "The thickness is the fit of the fingers; the cut operation applies the kerf once, not the generator."
    ),
    PreviewBox => (
        "Voorbeeld: {0} delen, samen {1} × {2} mm. Het document is nog niet gewijzigd.",
        "Preview: {0} panels, {1} × {2} mm together. The document has not changed yet."
    ),
    PreviewHinge => (
        "Voorbeeld: {0} sneden, {1} × {2} mm. Het document is nog niet gewijzigd.",
        "Preview: {0} cuts, {1} × {2} mm. The document has not changed yet."
    ),
    ProblemNumber => ("{0}: dit is geen geldig getal.", "{0}: this is not a valid number."),
    ProblemNotPositive => ("{0}: moet groter zijn dan nul.", "{0}: must be greater than zero."),
    ProblemNegative => ("{0}: mag niet negatief zijn.", "{0}: must not be negative."),
    ProblemTooSmall => (
        "{0} is {2} mm; minstens {1} mm is nodig.",
        "{0} is {2} mm; at least {1} mm is needed."
    ),
    ProblemCount => (
        "{0}: {3} valt buiten {1} tot {2}.",
        "{0}: {3} is outside {1} to {2}."
    ),
    ProblemEngine => ("{0}: de generator kan dit niet maken.", "{0}: the generator cannot make this."),
    Insert => ("Voeg in", "Insert"),
    Apply => ("Pas toe", "Apply"),
    Cancel => ("Annuleer", "Cancel"),
    // --- Project search (P08).
    SearchTitle => ("Projecten zoeken", "Find Projects"),
    SearchMenu => ("Zoek projecten…", "Find Projects…"),
    SearchFolderCaption => ("Map", "Folder"),
    SearchQueryCaption => ("Zoek", "Search"),
    SearchRescan => ("Lees opnieuw", "Read again"),
    SearchScanning => ("Bezig met lezen…", "Reading…"),
    SearchScanned => ("{0} projecten gelezen.", "{0} projects read."),
    SearchFolderUnreadable => ("De map kan niet gelezen worden.", "The folder cannot be read."),
    SearchNone => ("Geen projecten gevonden.", "No projects found."),
    SearchIssue => ("{0} kon niet gelezen worden.", "{0} could not be read."),
    SearchResultCaption => ("Resultaat", "Result"),
    SearchInfo => ("{0} objecten, laatst bewaard {1}.", "{0} objects, last saved {1}."),
    SearchInfoUnknown => ("{0} objecten, bewaartijd onbekend.", "{0} objects, save time unknown."),
    SearchFoundIn => ("Gevonden in: {0}.", "Found in: {0}."),
    SearchFieldFileName => ("bestandsnaam", "file name"),
    SearchFieldObject => ("object", "object"),
    SearchFieldOperation => ("bewerking", "operation"),
    SearchFieldMaterial => ("materiaal", "material"),
    SearchFieldPart => ("onderdeel", "part"),
    SearchFieldPlate => ("plaat", "plate"),
    SearchFieldRevision => ("revisie", "revision"),
    SearchRevisionCaption => ("Bevroren revisie", "Frozen revision"),
    SearchOpenProject => ("Open project", "Open Project"),
    SearchOpenRevision => ("Open revisie naast het ontwerp", "Open Revision Next to the Design"),
    SearchClose => ("Sluit", "Close"),
}

/// The text in the window language.
pub fn g6_text(text: G6Text, locale: Locale) -> &'static str {
    let (nl, en) = pair(text);
    match locale {
        Locale::Nl => nl,
        Locale::En => en,
    }
}

/// The text with `{0}`, `{1}`, … replaced by `args`.
pub fn g6_fmt(text: G6Text, locale: Locale, args: &[&str]) -> String {
    let mut out = g6_text(text, locale).to_owned();
    for (i, arg) in args.iter().enumerate() {
        out = out.replace(&format!("{{{i}}}"), arg);
    }
    out
}
