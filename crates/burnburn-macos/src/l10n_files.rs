//! Texts of the file panels (import report, Material and Art Library, bundles, export dialog),
//! NL/EN. Interim home next to `l10n`: that catalogue is compiled by the Windows host too, and
//! these panels are macOS-only until the Windows file panels exist (golf 4).

use crate::l10n::Locale;

macro_rules! texts {
    ($($name:ident => ($nl:expr, $en:expr)),* $(,)?) => {
        /// A text only the file panels show.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum FilesText {
            $($name),*
        }

        impl FilesText {
            /// Every text, for the completeness test.
            pub const ALL: &'static [FilesText] = &[$(FilesText::$name),*];
        }

        fn pair(text: FilesText) -> (&'static str, &'static str) {
            match text {
                $(FilesText::$name => ($nl, $en)),*
            }
        }
    };
}

texts! {
    // --- Import sheet: pages, units, layers and the LightBurn report.
    ImportPageLabel => ("Pagina:", "Page:"),
    ImportAllPages => ("Alle pagina's naast elkaar", "All pages side by side"),
    ImportPageItem => ("Pagina {0} — {1} × {2} mm", "Page {0} — {1} × {2} mm"),
    ImportReading => ("Pagina wordt gelezen…", "Reading the page…"),
    UnitPlotter => ("Plotter-eenheden (0,025 mm per stap)", "Plotter units (0.025 mm per step)"),
    UnitPlotterScaled => (
        "Plotter-eenheden, geschaald met IP/SC uit het bestand",
        "Plotter units, scaled by the file's IP/SC"
    ),
    UnitPdf => ("Maat uit het bestand (1 punt = 0,3528 mm)", "Size from the file (1 point = 0.3528 mm)"),
    UnitLightBurn => ("LightBurn-projecten staan in millimeters", "LightBurn projects are in millimetres"),
    LayersHeader => ("Voorgestelde lagen:", "Proposed layers:"),
    LayerLine => ("  {0}: {1} objecten", "  {0}: {1} objects"),
    ProjectLayersHeader => ("Lagen van het project:", "Layers of the project:"),
    ProjectLayerTaken => ("  {0} ({1}): {2} vormen → laag wordt aangemaakt", "  {0} ({1}): {2} shapes → a layer is created"),
    ProjectLayerSkipped => (
        "  {0} ({1}): {2} vormen → geen laag (instellingen niet te vertalen)",
        "  {0} ({1}): {2} shapes → no layer (settings cannot be translated)"
    ),
    ProjectUnmapped => ("      niet overgenomen: {0}", "      not taken over: {0}"),
    ProjectAssumed => ("      aangenomen: {0} = {1}", "      assumed: {0} = {1}"),
    KindLine => ("lijn", "line"),
    KindFill => ("vulling", "fill"),
    KindImage => ("beeld", "image"),
    WarnDevice => (
        "Let op: gemaakt voor apparaat \"{0}\"; actief profiel: {1}.",
        "Note: made for device \"{0}\"; active profile: {1}."
    ),
    WarnNoProfile => ("geen", "none"),
    WarnMirror => (
        "Let op: het project spiegelt de oorsprong ({0}); BurnBurn neemt de vormen over zoals getekend.",
        "Note: the project mirrors the origin ({0}); BurnBurn imports the shapes as drawn."
    ),
    MirrorX => ("horizontaal", "horizontally"),
    MirrorY => ("verticaal", "vertically"),
    ImagesHeader => ("Beelden:", "Images:"),
    ImageEmbedded => ("  {0}: {1} × {2} mm → beeldobject", "  {0}: {1} × {2} mm → image object"),
    ImageLinked => (
        "  {0}: alleen een koppeling naar {1}; niet geïmporteerd",
        "  {0}: only a link to {1}; not imported"
    ),
    ImageUnreadable => (
        "  {0}: ingebedde data niet leesbaar; niet geïmporteerd",
        "  {0}: embedded data unreadable; not imported"
    ),
    NotesHeader => ("Niet overgenomen:", "Not taken over:"),
    NoteLine => ("  {0}: {1} ({2}×)", "  {0}: {1} ({2}×)"),
    NoteUnknownElement => ("onbekend element", "unknown element"),
    NoteUnknownAttribute => ("onbekend attribuut", "unknown attribute"),
    NoteUnmappedSetting => ("laaginstelling", "layer setting"),
    NoteUnsupportedShape => ("vorm niet ondersteund", "shape not supported"),
    NoteNotConverted => ("niet omgezet", "not converted"),
    NoteInvalidValue => ("ongeldige waarde", "invalid value"),
    FontLine => ("Lettertype {0}: {1} teksten, {2} met omtrek", "Font {0}: {1} texts, {2} with outlines"),
    ProjectNotesHeader => ("Projectnotities:", "Project notes:"),
    // --- Material Library panel.
    MaterialTitle => ("Materiaalbibliotheek", "Material Library"),
    Loading => ("De bibliotheek wordt gelezen…", "Reading the library…"),
    NotSaved => (
        "Wijzigingen worden niet in een bestand bewaard.",
        "Changes are not saved to a file."
    ),
    LabelMaterial => ("Materiaal:", "Material:"),
    LabelThickness => ("Dikte (mm):", "Thickness (mm):"),
    LabelMachine => ("Machine:", "Machine:"),
    AllMaterials => ("Alle materialen", "All materials"),
    AllMachines => ("Alle machines", "All machines"),
    LabelEntries => ("Instellingen:", "Settings:"),
    NoEntries => ("Geen instellingen gevonden", "No settings found"),
    LabelLayer => ("Laag:", "Layer:"),
    NoLayers => ("Geen lagen in dit project", "No layers in this project"),
    ButtonApply => ("Toepassen", "Apply"),
    ButtonUpdate => ("Recept bijwerken", "Update recipe"),
    ButtonUpdateApply => ("Bijwerken en toepassen", "Update and apply"),
    ButtonRemove => ("Verwijder", "Remove"),
    LabelName => ("Naam:", "Name:"),
    LabelNewMaterial => ("Materiaal:", "Material:"),
    ButtonSave => ("Bewaar laag als item", "Save layer as entry"),
    ButtonOverwrite => ("Vervang item door laag", "Replace entry by layer"),
    ButtonClose => ("Sluit", "Close"),
    EntryItem => ("{0} — {1} {2} mm (r{3})", "{0} — {1} {2} mm (r{3})"),
    EntryDetail => (
        "{0} · {1} mm/s · {2} % · {3} pass(es)",
        "{0} · {1} mm/s · {2} % · {3} pass(es)"
    ),
    ModeLine => ("Lijn", "Line"),
    ModeFill => ("Vulling", "Fill"),
    ModeOffsetFill => ("Offsetvulling", "Offset fill"),
    ModeImage => ("Beeld", "Image"),
    ModeOther => ("Overig", "Other"),
    OriginManual => ("handmatig ingesteld", "set by hand"),
    OriginCopied => ("kopie van recept {0} (r{1})", "copy of recipe {0} (r{1})"),
    OriginLinked => (
        "kopie van recept {0} (r{1}) uit bibliotheekitem {2}: {3}",
        "copy of recipe {0} (r{1}) from library entry {2}: {3}"
    ),
    LinkUpToDate => ("actueel", "up to date"),
    LinkUpdate => ("update beschikbaar (r{0} → r{1})", "update available (r{0} → r{1})"),
    LinkOlder => ("bibliotheek is ouder (r{1})", "library is older (r{1})"),
    LinkMissing => ("item staat niet meer in de bibliotheek", "entry is no longer in the library"),
    LinkOther => ("andere of nog niet geladen bibliotheek", "other or not yet loaded library"),
    LayerNotApplicable => ("(beeldlaag: niet toepasbaar)", "(image layer: not applicable)"),
    // --- Art Library panel.
    ArtTitle => ("Kunstbibliotheek", "Art Library"),
    LabelGroup => ("Groep:", "Group:"),
    AllGroups => ("Alle groepen", "All groups"),
    LabelSearch => ("Zoek:", "Search:"),
    LabelItems => ("Items:", "Items:"),
    NoItems => ("Geen items", "No items"),
    ArtItem => ("{0} ({1})", "{0} ({1})"),
    GroupItem => ("{0} ({1})", "{0} ({1})"),
    ButtonPlace => ("Plaats in document", "Place in document"),
    ButtonAddFile => ("Voeg bestand toe…", "Add file…"),
    ButtonAddSelection => ("Voeg selectie toe", "Add selection"),
    ButtonNewGroup => ("Nieuwe groep", "New group"),
    ButtonRenameGroup => ("Hernoem groep", "Rename group"),
    ButtonRemoveGroup => ("Verwijder groep", "Remove group"),
    ButtonRenameItem => ("Hernoem item", "Rename item"),
    ButtonMoveItem => ("Verplaats naar groep", "Move to group"),
    ButtonRemoveItem => ("Verwijder item", "Remove item"),
    ArtNeedsScale => (
        "{0} noemt geen eenheid. Kies er een en voeg het opnieuw toe:",
        "{0} names no unit. Choose one and add it again:"
    ),
    ArtBusy => ("Bezig met toevoegen of plaatsen…", "Adding or placing…"),
    ArtNameHint => ("Naam voor groep of item", "Name for a group or item"),
    ArtDetail => ("Formaat {0}", "Format {0}"),
    // --- Bundle sheet.
    BundleExportTitle => ("Bundel exporteren", "Export bundle"),
    BundleImportTitle => ("Bundel importeren", "Import bundle"),
    BundleChoose => ("Kies wat in de bundel komt:", "Choose what goes into the bundle:"),
    KindMaterials => ("Materiaalinstellingen", "Material settings"),
    KindArt => ("Kunst", "Art"),
    KindProfiles => ("Machineprofielen", "Machine profiles"),
    KindTemplates => ("Templates", "Templates"),
    KindCount => ("{0} ({1} van {2})", "{0} ({1} of {2})"),
    ButtonExport => ("Exporteer…", "Export…"),
    ButtonApplyBundle => ("Importeer", "Import"),
    BundleWritten => ("Bundel geschreven: {0}", "Bundle written: {0}"),
    BundleReading => ("De bundel wordt gelezen…", "Reading the bundle…"),
    BundleBusy => ("Bezig…", "Working…"),
    LabelConflict => ("Bij een verschil met wat hier is:", "When it differs from what is here:"),
    ConflictSkip => ("Laat bestaande staan", "Keep what is here"),
    ConflictReplace => ("Vervang door de bundel", "Replace by the bundle"),
    ConflictKeepBoth => ("Bewaar beide", "Keep both"),
    ChangeNew => ("nieuw", "new"),
    ChangeIdentical => ("gelijk", "identical"),
    ChangeConflict => ("verschilt", "differs"),
    BundleSectionMaterials => ("Materiaalinstellingen:", "Material settings:"),
    BundleSectionArt => ("Kunst:", "Art:"),
    BundleSectionTemplates => ("Templates:", "Templates:"),
    BundleSectionProfiles => ("Machineprofielen:", "Machine profiles:"),
    BundleRow => ("  {0} — {1}", "  {0} — {1}"),
    BundleProfileNew => ("  {0} (r{1}) — nieuw", "  {0} (r{1}) — new"),
    BundleProfileIdentical => ("  {0} (r{1}) — gelijk aan hier (r{2})", "  {0} (r{1}) — identical to here (r{2})"),
    BundleProfileDiffers => (
        "  {0} (r{1}, hier r{2}) — verschilt in: {3}",
        "  {0} (r{1}, here r{2}) — differs in: {3}"
    ),
    ReplaceProfile => ("Vervang profiel {0}", "Replace profile {0}"),
    ProfileKept => (
        "Een afwijkend profiel blijft zoals het is, tenzij je het hierboven aanvinkt.",
        "A differing profile stays as it is unless you tick it above."
    ),
    BundleReport => (
        "Klaar: {0} instellingen toegevoegd, {1} vervangen; {2} kunstitems toegevoegd, {3} vervangen; {4} templates geschreven; {5} profielen toegevoegd, {6} vervangen; {7} overgeslagen.",
        "Done: {0} settings added, {1} replaced; {2} art items added, {3} replaced; {4} templates written; {5} profiles added, {6} replaced; {7} skipped."
    ),
    BundleProfilesPending => (
        "Afwijkende profielen die ongewijzigd bleven: {0}",
        "Differing profiles left unchanged: {0}"
    ),
    // --- Export dialog.
    ExportSvgTitle => ("Exporteer als SVG", "Export as SVG"),
    ExportDxfTitle => ("Exporteer als DXF", "Export as DXF"),
    ExportObjects => ("{0} objecten worden geëxporteerd.", "{0} objects will be exported."),
    LabelUnit => ("Eenheid:", "Unit:"),
    LabelLayers => ("Lagen:", "Layers:"),
    LabelCurves => ("Krommen:", "Curves:"),
    LabelStroke => ("Lijndikte (mm):", "Line width (mm):"),
    LabelDecimals => ("Decimalen:", "Decimals:"),
    LabelTolerance => ("Afwijking (mm):", "Deviation (mm):"),
    DecimalsHint => ("leeg = exact", "empty = exact"),
    CheckSelectionOnly => ("Alleen de selectie", "Selection only"),
    CheckUseColors => ("Kleuren van de lagen gebruiken", "Use the layer colours"),
    UnitMm => ("millimeter", "millimetres"),
    UnitInch => ("inch", "inches"),
    UnitPixel => ("pixels (96 per inch)", "pixels (96 per inch)"),
    LayersObject => ("Laag per object", "One layer per object"),
    LayersObjectSvg => ("Geen groepen", "No groups"),
    LayersLayer => ("Laag per laag van het project", "One layer per project layer"),
    LayersLayerSvg => ("Groep per laag van het project", "One group per project layer"),
    LayersSingle => ("Alles op één laag", "Everything on one layer"),
    CurvesSplines => ("Splines (exact)", "Splines (exact)"),
    CurvesPolylines => ("Polylijnen (voor programma's zonder splines)", "Polylines (for programs without splines)"),
    ExportChecking => ("Het verlies wordt berekend…", "Working out what is lost…"),
    ExportLossHeader => (
        "Dit kan het bestand niet bevatten:",
        "The file cannot hold this:"
    ),
    ExportNoLoss => ("Er gaat niets verloren.", "Nothing is lost."),
    LossLine => ("  {0}: {1}", "  {0}: {1}"),
    LossElliptical => (
        "ellipsboog als spline-stukken (afwijking ≤ {0} mm)",
        "elliptical arc written as spline spans (deviation ≤ {0} mm)"
    ),
    LossCircular => (
        "cirkelboog in een kromme als spline-stukken (afwijking ≤ {0} mm)",
        "circular arc in a curve written as spline spans (deviation ≤ {0} mm)"
    ),
    LossConstraints => (
        "{0} randvoorwaarden van de schets gaan verloren (bevestiging nodig)",
        "{0} sketch constraints are lost (confirmation needed)"
    ),
    LossModifiers => (
        "{0} modifiers worden als gewone kopieën geschreven (bevestiging nodig)",
        "{0} modifiers are written as plain copies (confirmation needed)"
    ),
    LossParametric => ("parametrische vorm wordt een gewoon pad", "parametric shape becomes a plain path"),
    LossText => ("tekst wordt geschreven als omtrek", "text is written as outlines"),
    LossFlattened => (
        "krommen als rechte stukken (afwijking ≤ {0} mm)",
        "curves as straight chords (deviation ≤ {0} mm)"
    ),
    LossColor => (
        "laagkleur benaderd door de dichtstbijzijnde AutoCAD-kleur",
        "layer colour approximated by the nearest AutoCAD colour"
    ),
    ExportAcknowledge => (
        "Ik begrijp dat dit verloren gaat",
        "I understand this is lost"
    ),
    ExportWritten => ("Bestand geschreven: {0}", "File written: {0}"),
    // --- File panels (NSOpenPanel / NSSavePanel).
    PanelTemplateSave => ("Bewaar als template", "Save as template"),
    PanelTemplateContent => ("Inhoud:", "Content:"),
    TemplateEverything => ("Alles (vormen, lagen, instellingen)", "Everything (shapes, layers, settings)"),
    TemplateSettingsOnly => ("Alleen lagen en instellingen", "Layers and settings only"),
    PanelTemplateOpen => ("Kies een template", "Choose a template"),
    PanelBundleSave => ("Bewaar bundel", "Save bundle"),
    PanelBundleOpen => ("Kies een bundel", "Choose a bundle"),
    PanelMachineProfileOpen => ("Kies een machineprofiel (JSON)", "Choose a machine profile (JSON)"),
    PanelArtOpen => ("Kies een tekening voor de bibliotheek", "Choose a drawing for the library"),
    PanelExportSave => ("Bewaar tekening", "Save drawing"),
    // --- Menus.
    MenuNewFromRecentTemplate => ("Recente templates", "Recent Templates"),
    MenuExportSubmenu => ("Exporteer", "Export"),
    MenuBundleSubmenu => ("Bundels", "Bundles"),
}

/// `text` in `locale`.
pub fn files_text(text: FilesText, locale: Locale) -> &'static str {
    let (nl, en) = pair(text);
    match locale {
        Locale::Nl => nl,
        Locale::En => en,
    }
}
