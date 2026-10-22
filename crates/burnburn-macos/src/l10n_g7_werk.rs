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
