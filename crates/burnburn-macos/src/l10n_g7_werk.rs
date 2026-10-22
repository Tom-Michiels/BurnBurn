//! Texts of the golf-7 workshop panels (G7-werk): the confirmation before a start (M06). NL/EN, kept
//! apart so the older text files stay untouched; the Windows host compiles this file too.

use crate::l10n::Locale;

macro_rules! texts {
    ($($name:ident => ($nl:expr, $en:expr)),* $(,)?) => {
        /// A text only the golf-7 workshop panels show.
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
    // --- The confirmation before a start (M06).
    StartConfirmTitle => ("Start bevestigen", "Confirm start"),
    StartConfirmHeading => ("Deze job wordt gestart", "This job will be started"),
    StartJobLine => (
        "Job: {0} — revisie {1}, {2}",
        "Job: {0} — revision {1}, {2}"
    ),
    StartMachineLine => (
        "Machine: {0} — model (door jou opgegeven): {1}; firmware (door jou opgegeven): {2}",
        "Machine: {0} — model (as you declared): {1}; firmware (as you declared): {2}"
    ),
    StartProfileLine => (
        "Machineprofiel van de job: {0}, revisie {1}",
        "Machine profile of the job: {0}, revision {1}"
    ),
    StartMaterialLine => ("Materiaal: {0}", "Material: {0}"),
    StartSizeLine => ("Afmeting op het bed: {0}", "Size on the bed: {0}"),
    StartPlacementDesigned => (
        "Plaatsing: op de ontworpen plek op het bed.",
        "Placement: at the designed place on the bed."
    ),
    StartPlacementOnHead => (
        "Plaatsing: de job is op de kopstand ({0}; {1}) mm gezet.",
        "Placement: the job was put on the head position ({0}; {1}) mm."
    ),
    NotDeclared => ("niet opgegeven", "not declared"),
    Unknown => ("onbekend", "unknown"),
    StartNotProven => (
        "Het starten is niet op hardware bewezen, voor geen enkele controller. Deze controller kan de start weigeren of negeren. Bewijs dat ontbreekt: {0}. Een start die eenmaal is verstuurd wordt nooit automatisch herhaald.",
        "Starting is not proven on hardware for any controller. This controller may refuse or ignore the start. Evidence that is missing: {0}. A start that was sent once is never repeated automatically."
    ),
    StartConfirmButton => ("Start de job", "Start the job"),
    StartCancelButton => ("Annuleer", "Cancel"),
    // --- The remnant library (W07).
    RemnantsMenu => ("Restplaten…", "Remnants…"),
    RemnantsTitle => ("Restplaten", "Remnants"),
    RemnantsHeading => ("Bewaarde restplaten", "Kept remnants"),
    RemnantsNone => ("Er zijn nog geen restplaten bewaard.", "No remnants have been kept yet."),
    RemnantLine => (
        "{0} — {1}, {2} mm dik — {3} × {4} mm, {5} cm² bruikbaar, {6} onbruikbare zone(s), bewaard op {7}",
        "{0} — {1}, {2} mm thick — {3} × {4} mm, {5} cm² usable, {6} unusable zone(s), kept on {7}"
    ),
    RemnantNote => ("Notitie: {0}", "Note: {0}"),
    RemnantPlaceButton => ("Teken {0} in het venster", "Draw {0} in the window"),
    RemnantRemoveButton => ("Verwijder {0}", "Remove {0}"),
    SaveHeading => ("Selectie bewaren als restplaat", "Keep the selection as a remnant"),
    SaveHint => (
        "Selecteer de omtrek van de restplaat en de gesloten lijnen van de stukken die niet meer bruikbaar zijn. De grootste gesloten lijn is de plaat, de andere zijn zones die buiten gebruik blijven.",
        "Select the outline of the remnant and the closed lines of the parts that are no longer usable. The largest closed line is the sheet, the others are zones that stay out of use."
    ),
    CaptionName => ("Naam", "Name"),
    CaptionMaterial => ("Materiaal", "Material"),
    CaptionThickness => ("Dikte (mm)", "Thickness (mm)"),
    CaptionNote => ("Notitie", "Note"),
    CaptionDate => ("Datum (JJJJ-MM-DD, leeg is vandaag)", "Date (YYYY-MM-DD, empty is today)"),
    SaveButton => ("Bewaar selectie", "Keep selection"),
    ProblemNotReady => ("De bibliotheek wordt nog gelezen; probeer het zo opnieuw.", "The library is still being read; try again in a moment."),
    ProblemUnreadable => (
        "Het bibliotheekbestand van de restplaten kan niet gelezen worden. Het blijft zoals het is en er wordt niets in bewaard.",
        "The remnant library file cannot be read. It is left as it is and nothing is kept in it."
    ),
    ProblemNeedsSelection => ("Selecteer eerst de restplaat in het venster.", "Select the remnant in the window first."),
    ProblemNoClosedOutline => ("De selectie bevat geen gesloten lijn.", "The selection holds no closed line."),
    ProblemNeedsName => ("Geef de restplaat een naam.", "Give the remnant a name."),
    ProblemNeedsMaterial => ("Geef het materiaal op.", "Give the material."),
    ProblemInvalidThickness => ("De dikte is een getal boven 0 en niet meer dan 100 mm.", "The thickness is a number above 0 and not more than 100 mm."),
    ProblemInvalidDate => ("De datum is JJJJ-MM-DD.", "The date is YYYY-MM-DD."),
    ProblemNameTaken => ("Er is al een restplaat met deze naam.", "A remnant with this name exists already."),
    ProblemZoneOutside => ("Een zone ligt buiten de plaat.", "A zone lies outside the sheet."),
    ProblemNotASheet => (
        "De plaat of een zone snijdt zichzelf, heeft geen oppervlak of is te gedetailleerd.",
        "The sheet or a zone crosses itself, has no area or is too detailed."
    ),
    ProblemUnknown => ("Deze restplaat bestaat niet (meer).", "This remnant does not exist (any more)."),
    ProblemNotSaved => ("De restplaten konden niet worden bewaard.", "The remnants could not be kept."),
    OrdersHeading => ("Restplaat per order", "Remnant per order"),
    OrderNoRemnant => ("(geen restplaat)", "(no remnant)"),
    OrderCaption => ("Order {0}", "Order {0}"),
    OrderLinkMissing => (
        "De restplaat {0} van order {1} staat niet meer in de bibliotheek.",
        "The remnant {0} of order {1} is no longer in the library."
    ),
    OrderLinkMismatch => (
        "Let op bij order {0}: het materiaal van de restplaat ({1}) is niet het materiaal van de job.",
        "Take care with order {0}: the material of the remnant ({1}) is not the material of the job."
    ),
    OrderLinkNote => (
        "Een koppeling verandert geen aantallen: alleen geregistreerde uitvoeringen tellen.",
        "A link changes no counts: only recorded executions count."
    ),
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
