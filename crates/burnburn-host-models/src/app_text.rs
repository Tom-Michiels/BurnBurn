//! Texts of the app-level panels and menus (W3-UI-app): the application menu, Window and Help
//! menus, the preferences window, the shortcut editor, the undo history, the preview window, the
//! RD inspector and the support dialog. NL and EN, one catalogue of its own so the other panels'
//! catalogue (`l10n`) stays untouched; the test `app_text` checks that every text exists in both.

use crate::l10n::Locale;

macro_rules! app_texts {
    ($($name:ident => ($nl:expr, $en:expr)),+ $(,)?) => {
        /// Every text of the app-level panels.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum AppText {
            $($name),+
        }

        impl AppText {
            pub const ALL: &'static [AppText] = &[$(AppText::$name),+];
        }

        /// The text in `locale`.
        pub fn app_text(text: AppText, locale: Locale) -> &'static str {
            match (text, locale) {
                $((AppText::$name, Locale::Nl) => $nl, (AppText::$name, Locale::En) => $en,)+
            }
        }
    };
}

app_texts! {
    // Menus
    MenuWindow => ("Venster", "Window"),
    MenuHelp => ("Help", "Help"),
    About => ("Over BurnBurn", "About BurnBurn"),
    Preferences => ("Instellingen…", "Settings…"),
    Options => ("Opties…", "Options…"),
    OptionsTitle => ("Opties", "Options"),
    Shortcuts => ("Sneltoetsen…", "Keyboard Shortcuts…"),
    Services => ("Diensten", "Services"),
    HideApp => ("Verberg BurnBurn", "Hide BurnBurn"),
    HideOthers => ("Verberg andere", "Hide Others"),
    ShowAll => ("Toon alles", "Show All"),
    QuitApp => ("Stop BurnBurn", "Quit BurnBurn"),
    Preview => ("Voorbeeld", "Preview"),
    InspectRd => ("RD inspecteren…", "Inspect RD…"),
    UndoHistory => ("Geschiedenis", "History"),
    MachineHub => ("Machine en productie…", "Machine and Production…"),
    Minimize => ("Minimaliseer", "Minimize"),
    ZoomWindow => ("Zoom", "Zoom"),
    BringAllToFront => ("Alles naar voren", "Bring All to Front"),
    Manual => ("BurnBurn-handleiding", "BurnBurn Manual"),
    ManualUnavailable => ("Handleiding: deze versie kent nog geen adres.", "Manual: this build has no address for it yet."),
    SupportData => ("Supportgegevens maken…", "Generate Support Data…"),
    DebugLog => ("Debuglog", "Debug Log"),
    // About
    AboutVersion => ("Versie", "Version"),
    AboutPlatform => ("Platform", "Platform"),
    AboutOffline => ("BurnBurn maakt bij het starten geen netwerkverbinding.", "BurnBurn makes no network connection at start-up."),
    // Preferences window
    PrefsTitle => ("Instellingen", "Settings"),
    PrefUnits => ("Eenheden", "Units"),
    PrefLanguage => ("Taal", "Language"),
    PrefAutosave => ("Automatisch bewaren (s, 0 = uit)", "Autosave (s, 0 = off)"),
    PrefSnapObjects => ("Vastklikken op objecten", "Snap to objects"),
    PrefSnapGrid => ("Vastklikken op raster", "Snap to grid"),
    PrefGrid => ("Rasterafstand", "Grid distance"),
    PrefObjectPx => ("Vangstraal objecten (px)", "Object snap radius (px)"),
    PrefNudgeSmall => ("Pijltje met Ctrl (klein)", "Ctrl+arrow (small)"),
    PrefNudgeNormal => ("Pijltje (normaal)", "Arrow (normal)"),
    PrefNudgeLarge => ("Pijltje met Shift (groot)", "Shift+arrow (large)"),
    PrefQuality => ("Weergavekwaliteit", "Display quality"),
    PrefDesktopAutomation => ("Desktop-automatisering (alleen deze computer)", "Desktop automation (this computer only)"),
    UnitMm => ("Millimeter", "Millimetres"),
    UnitInch => ("Inch", "Inches"),
    LangSystem => ("Volg het systeem", "Follow the system"),
    LangNl => ("Nederlands", "Dutch"),
    LangEn => ("Engels", "English"),
    QualityCoarse => ("Grof (sneller)", "Coarse (faster)"),
    QualitySmooth => ("Fijn", "Smooth"),
    SuffixMm => ("mm", "mm"),
    SuffixInch => ("in", "in"),
    SuffixPx => ("px", "px"),
    SuffixSeconds => ("s", "s"),
    PrefsTabGeneral => ("Algemeen", "General"),
    PrefsTabCanvas => ("Tekenvlak", "Canvas"),
    PrefsTabNudge => ("Verplaatsen", "Nudging"),
    PrefsReset => ("Standaardwaarden herstellen", "Restore Defaults"),
    PrefsNote => (
        "Instellingen staan in je gebruikersmap, nooit in een project. De taal geldt vanaf de volgende start; snapping geldt voor nieuwe vensters.",
        "Settings live in your user folder, never in a project. The language applies from the next start; snapping applies to new windows."
    ),
    PrefsLoadProblem => (
        "Het voorkeurenbestand kon niet (volledig) gelezen worden; waar nodig zijn standaardwaarden actief. Het bestand blijft ongewijzigd tot je iets verandert.",
        "The preferences file could not be read (completely); defaults are used where needed. The file stays as it is until you change something."
    ),
    PrefsSaveFailed => (
        "De voorkeuren zijn niet bewaard; ze gelden alleen in deze sessie.",
        "The preferences were not saved; they apply to this session only."
    ),
    PrefNotANumber => ("Geen geldig getal.", "Not a valid number."),
    PrefOutOfRange => ("De waarde valt buiten het toegestane bereik.", "The value is outside the allowed range."),
    PrefUnknownChoice => ("Onbekende keuze.", "Unknown choice."),
    // Shortcut editor
    ShortcutsTitle => ("Sneltoetsen", "Keyboard Shortcuts"),
    ShortcutColCommand => ("Opdracht", "Command"),
    ShortcutColKey => ("Sneltoets", "Shortcut"),
    ShortcutColDefault => ("Standaard", "Default"),
    ShortcutNone => ("geen", "none"),
    ShortcutRecord => ("Neem op…", "Record…"),
    ShortcutRecording => ("Druk op de nieuwe sneltoets (Esc stopt)…", "Press the new shortcut (Esc stops)…"),
    ShortcutClear => ("Wis sneltoets", "Clear Shortcut"),
    ShortcutResetOne => ("Standaard", "Default"),
    ShortcutResetAll => ("Alles terugzetten", "Reset All"),
    ShortcutSearch => ("Zoek een opdracht", "Search a command"),
    ShortcutConflict => ("is al in gebruik door", "is already used by"),
    ShortcutReplace => ("Vervang", "Replace"),
    ShortcutReserved => (
        "Deze toets is van het systeem of van het canvas en kan niet gekozen worden.",
        "This key belongs to the system or the canvas and cannot be chosen."
    ),
    ShortcutNeedsModifier => (
        "Een opdracht heeft Cmd, Ctrl of Opt nodig; alleen gereedschappen mogen een losse toets hebben.",
        "A command needs Cmd, Ctrl or Opt; only tools may use a plain key."
    ),
    ShortcutUnknown => ("Onbekende opdracht.", "Unknown command."),
    ShortcutHostPrefix => ("Programma", "App"),
    // History
    HistoryTitle => ("Geschiedenis", "History"),
    HistoryStart => ("Begin (zoals geopend)", "Start (as opened)"),
    HistoryCurrent => ("◀ huidige stand", "◀ current state"),
    HistoryUndone => ("(ongedaan)", "(undone)"),
    HistoryNoDocument => ("Geen document.", "No document."),
    HistoryHint => ("Klik op een stap om terug te gaan naar die stand.", "Click a step to go back to that state."),
    // Preview
    PreviewTitle => ("Voorbeeld", "Preview"),
    PreviewPlay => ("Afspelen", "Play"),
    PreviewPause => ("Pauze", "Pause"),
    PreviewSpeed => ("Snelheid", "Speed"),
    PreviewShowTravel => ("Verplaatsingen tonen", "Show travel moves"),
    PreviewTime => ("Tijd", "Time"),
    PreviewOf => ("van", "of"),
    PreviewEstimate => ("(schatting)", "(estimate)"),
    PreviewStale => ("Dit plan is verouderd: het document of het profiel is gewijzigd. Compileer opnieuw.", "This plan is out of date: the document or the profile changed. Compile again."),
    PreviewNoPlan => ("Er is nog geen plan. Kies Compileer in het menu Productie.", "There is no plan yet. Choose Compile in the Production menu."),
    PreviewPicked => ("Gekozen", "Picked"),
    PreviewNothingPicked => ("Klik op een beweging om de bron te kiezen.", "Click a move to pick its source."),
    PreviewMoveCut => ("snijden", "cut"),
    PreviewMoveScan => ("graveren", "scan"),
    PreviewMoveTravel => ("verplaatsing (laser uit)", "travel (laser off)"),
    PreviewMoveOverscan => ("overscan (laser uit, afhankelijk van de firmware)", "overscan (laser off, firmware dependent)"),
    PreviewMoveDot => ("puntpuls", "dot pulse"),
    PreviewAtPower => ("met vermogen", "at power"),
    PreviewSource => ("bron", "source"),
    PreviewOperation => ("bewerking", "operation"),
    PreviewLegend => ("rood = snijden/graveren · oranje = verlaagd vermogen (tab) · grijs = verplaatsing · blauw = gekozen · groen = afgespeeld", "red = cut/scan · orange = lowered power (tab) · grey = travel · blue = picked · green = played"),
    PreviewPlanId => ("Plan", "Plan"),
    // RD inspector
    RdTitle => ("RD-records (alleen lezen)", "RD records (read-only)"),
    RdColOffset => ("Offset", "Offset"),
    RdColBytes => ("Bytes", "Bytes"),
    RdColName => ("Naam", "Name"),
    RdColDecoded => ("Gedecodeerd", "Decoded"),
    RdNotCurrent => ("Dit is de lijst van een ouder plan; kies opnieuw RD inspecteren voor het huidige plan.", "This is the list of an older plan; choose Inspect RD again for the current plan."),
    RdFailed => ("Dit plan heeft geen RD-vorm", "This plan has no RD form"),
    RdTruncated => ("records niet getoond", "records not shown"),
    RdNoPlan => ("Er is geen plan om te inspecteren.", "There is no plan to inspect."),
    RdNameIsLabel => ("De naam is het label uit het openbare register, geen bewijs van firmwaregedrag.", "The name is the public registry's label, not proof of firmware behaviour."),
    RdNoUpload => ("Alleen lezen: dit venster stuurt niets naar een machine.", "Read-only: this window sends nothing to a machine."),
    // Support data
    SupportTitle => ("Supportgegevens", "Support Data"),
    SupportMessage => (
        "Het rapport bevat de versie, het platform, je voorkeuren, per venster alleen aantallen en de journaalweergave van de machine. Documentinhoud komt er nooit in. Je ziet het rapport voordat je het bewaart.",
        "The report holds the version, the platform, your preferences, per window only counts, and the machine journal replay. Document content is never in it. You see the report before you save it."
    ),
    SupportPaths => ("Projectnamen en -paden opnemen", "Include project names and paths"),
    SupportIds => ("Machine-identificaties opnemen", "Include machine identifiers"),
    SupportSave => ("Bewaar…", "Save…"),
    SupportCancel => ("Annuleer", "Cancel"),
    SupportSavePanelTitle => ("Supportgegevens bewaren", "Save Support Data"),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_text_is_empty_in_either_language() {
        for text in AppText::ALL {
            for locale in [Locale::Nl, Locale::En] {
                assert!(!app_text(*text, locale).trim().is_empty(), "{text:?} in {locale:?}");
            }
        }
    }
}
