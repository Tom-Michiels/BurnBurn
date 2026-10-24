//! Texts of the empty states and of the progress rows (LF17), NL/EN. Kept apart so `l10n.rs`
//! stays untouched; the Windows host compiles the same file.

use crate::l10n::Locale;

macro_rules! texts {
    ($($name:ident => ($nl:expr, $en:expr)),* $(,)?) => {
        /// A text only the empty states and the progress rows show.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum EmptyText {
            $($name),*
        }

        impl EmptyText {
            /// Every text, for the completeness test.
            pub const ALL: &'static [EmptyText] = &[$(EmptyText::$name),*];
        }

        fn pair(text: EmptyText) -> (&'static str, &'static str) {
            match text {
                $(EmptyText::$name => ($nl, $en)),*
            }
        }
    };
}

texts! {
    Objects => (
        "Nog geen objecten. Teken een vorm of importeer een bestand.",
        "No objects yet. Draw a shape or import a file."
    ),
    ObjectsAction => ("Teken een rechthoek", "Draw a Rectangle"),
    Operations => (
        "Nog geen bewerkingen. Selecteer objecten en voeg een bewerking toe.",
        "No operations yet. Select objects and add an operation."
    ),
    Materials => (
        "De materiaalbibliotheek is leeg. Bewaar de instellingen van een bewerking om ze te hergebruiken.",
        "The material library is empty. Save the settings of an operation to reuse them."
    ),
    Art => (
        "De kunstbibliotheek is leeg. Voeg een bestand of de selectie toe.",
        "The art library is empty. Add a file or the selection."
    ),
    Remnants => (
        "Nog geen restplaten bewaard. Selecteer de omtrek van een restplaat om hem te bewaren.",
        "No remnants kept yet. Select the outline of a remnant to keep it."
    ),
    Recognition => (
        "Nog geen sjabloon om te herkennen. Leer een product aan met de camera.",
        "No template to recognise yet. Teach a product with the camera."
    ),
    History => (
        "Nog geen stappen. Elke wijziging komt hier te staan.",
        "No steps yet. Every change will appear here."
    ),
    BundleExport => (
        "Er is niets om te bundelen. Vul eerst een bibliotheek of sla een sjabloon op.",
        "There is nothing to bundle. Fill a library or save a template first."
    ),
    BundleImport => (
        "Deze bundel bevat niets om te importeren.",
        "This bundle contains nothing to import."
    ),
    HubDevices => (
        "Nog geen machine ingesteld. Voeg een machineprofiel toe.",
        "No machine set up yet. Add a machine profile."
    ),
    HubQueue => ("Er staat niets in de wachtrij.", "Nothing is in the queue."),
    HubHistory => ("Er is nog niets naar de machine gestuurd.", "Nothing has been sent to the machine yet."),
    HubProofs => ("Nog geen ijkproeven vastgelegd.", "No calibration proofs recorded yet."),
    TaskCompile => ("Plan berekenen…", "Compiling the plan…"),
    TaskTrace => ("Afbeelding overtrekken…", "Tracing the image…"),
    TaskNest => ("Schikken…", "Nesting…"),
    TaskTest => ("Testbestand berekenen…", "Compiling the test job…"),
    TaskPreview => ("Voorbeeld berekenen…", "Calculating the preview…"),
    TaskOther => ("Bezig…", "Working…"),
    TaskCancel => ("Annuleer", "Cancel"),
}

/// `text` in `locale`.
pub fn empty_text(text: EmptyText, locale: Locale) -> &'static str {
    let (nl, en) = pair(text);
    match locale {
        Locale::Nl => nl,
        Locale::En => en,
    }
}
