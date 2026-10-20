//! File features of the window (W3-UI-bestanden): the import report of every format, templates,
//! the Material and Art Library panels, bundles and the export dialog. Plain data only: hosts
//! translate these into widgets and send [`FilesIntent`]s back; nothing here knows AppKit.

use std::path::PathBuf;

use burnburn_types::{DocumentId, OperationId};

use std::sync::Arc;

use crate::intent::{ImportUnit, OperationKind};
use crate::view_model::MessageKey;

/// A user action of the file features, kept apart from `UiIntent` so the file panels grow
/// without touching the shared intent list.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum FilesIntent {
    /// PDF/AI import preview: which page(s) to import.
    SetImportPages {
        document: DocumentId,
        pages: ImportPages,
    },
    /// Archief ▸ Bewaar als template: the template file the user chose and what it holds.
    SaveAsTemplate {
        document: DocumentId,
        path: PathBuf,
        scope: TemplateScope,
    },
    /// Archief ▸ Nieuw van template: opens an untitled copy; the template is only read.
    NewFromTemplate {
        path: PathBuf,
    },
    /// The recent templates the host stored (read at start).
    SetRecentTemplates {
        paths: Vec<PathBuf>,
    },
    ClearRecentTemplates,
    /// Window ▸ Materiaalbibliotheek: opens the panel (the library is read on a worker).
    OpenMaterialLibrary {
        document: DocumentId,
    },
    CloseMaterialLibrary {
        document: DocumentId,
    },
    /// The panel's search: material, thickness as typed (empty = any thickness) and machine.
    SetMaterialFilter {
        document: DocumentId,
        material: Option<String>,
        thickness: String,
        machine: Option<String>,
    },
    SelectMaterialEntry {
        document: DocumentId,
        entry: Option<u64>,
    },
    /// "Toepassen": copy the entry's mode and settings into these layers. The project gets a
    /// recipe that remembers which library entry and revision it came from.
    ApplyMaterial {
        document: DocumentId,
        entry: u64,
        operations: Vec<OperationId>,
    },
    /// "Bijwerken": bring the entry's current content into the project recipe, and, when
    /// `reapply` is set, into the layers that were set from that recipe. Never happens by itself.
    UpdateLinkedRecipe {
        document: DocumentId,
        recipe: u64,
        reapply: bool,
    },
    /// Store a layer's mode and settings as a new library entry for this material and thickness.
    SaveMaterial {
        document: DocumentId,
        operation: OperationId,
        name: String,
        material: String,
        thickness: String,
        notes: String,
    },
    /// Replace an entry's content by a layer's mode and settings (the entry gets a new revision).
    UpdateMaterialFromOperation {
        document: DocumentId,
        entry: u64,
        operation: OperationId,
    },
    RemoveMaterial {
        document: DocumentId,
        entry: u64,
    },
    /// Window ▸ Kunstbibliotheek: opens the panel (the library is read on a worker).
    OpenArtLibrary {
        document: DocumentId,
    },
    CloseArtLibrary {
        document: DocumentId,
    },
    /// Show one group (`None`: all groups).
    SelectArtGroup {
        document: DocumentId,
        group: Option<String>,
    },
    /// Search names, tags and groups.
    SetArtSearch {
        document: DocumentId,
        text: String,
    },
    SelectArtItem {
        document: DocumentId,
        item: Option<u64>,
    },
    AddArtGroup {
        document: DocumentId,
        name: String,
    },
    RenameArtGroup {
        document: DocumentId,
        old: String,
        new: String,
    },
    /// A group that still holds items is refused; move or remove them first.
    RemoveArtGroup {
        document: DocumentId,
        name: String,
    },
    /// Add a drawing file (SVG, DXF, HP-GL or PDF/AI) to a group; the name defaults to the file's.
    /// A DXF without a unit waits for `dxf_unit` (the panel shows `needs_scale`).
    AddArtFromFile {
        document: DocumentId,
        path: PathBuf,
        group: String,
        name: String,
        dxf_unit: Option<ImportUnit>,
    },
    /// Add the selected objects of the window, as the SVG they export to.
    AddArtFromSelection {
        document: DocumentId,
        name: String,
        group: String,
    },
    RenameArtItem {
        document: DocumentId,
        item: u64,
        name: String,
    },
    MoveArtItem {
        document: DocumentId,
        item: u64,
        group: String,
    },
    RemoveArtItem {
        document: DocumentId,
        item: u64,
    },
    /// Insert the drawing into the document (one history entry), centred on `at` (canvas points,
    /// where it was dropped) or on the centre of the view.
    PlaceArt {
        document: DocumentId,
        item: u64,
        at: Option<(f64, f64)>,
    },
    /// Archief ▸ Bundels ▸ Exporteren: opens the dialog that lists what can go in a bundle.
    OpenBundleExport {
        document: DocumentId,
    },
    /// Include or leave out one item.
    ToggleBundleItem {
        document: DocumentId,
        item: BundleItem,
        selected: bool,
    },
    /// Include or leave out everything of one kind.
    SetBundleKind {
        document: DocumentId,
        kind: BundleKind,
        selected: bool,
    },
    /// Write the bundle to the file the user chose (nothing selected is refused).
    ExportBundle {
        document: DocumentId,
        path: PathBuf,
    },
    /// Archief ▸ Bundels ▸ Importeren: the bundle file the user chose is read and compared with
    /// what is here; nothing changes before `ApplyBundleImport`.
    OpenBundleImport {
        document: DocumentId,
        path: PathBuf,
    },
    SetBundleConflict {
        document: DocumentId,
        choice: BundleConflict,
    },
    /// Whether a machine profile that differs locally is replaced (default: it is not).
    SetBundleProfileReplace {
        document: DocumentId,
        profile: String,
        replace: bool,
    },
    ApplyBundleImport {
        document: DocumentId,
    },
    CloseBundle {
        document: DocumentId,
    },
    /// Archief ▸ Exporteren ▸ SVG / DXF: opens the dialog with the export options and the report
    /// of what the file cannot carry.
    OpenExport {
        document: DocumentId,
        format: ExportFormat,
    },
    SetExportOptions {
        document: DocumentId,
        options: Box<ExportOptionsInput>,
    },
    /// "Ik begrijp dat dit verloren gaat": required before constraints or modifiers are dropped.
    SetExportAcknowledged {
        document: DocumentId,
        acknowledged: bool,
    },
    /// Write the file the user chose with the options of the dialog.
    ExportDrawing {
        document: DocumentId,
        path: PathBuf,
    },
    CloseExport {
        document: DocumentId,
    },
}

/// The file formats of the drawing export.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExportFormat {
    Svg,
    Dxf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExportUnit {
    #[default]
    Millimeter,
    Inch,
    /// CSS pixels, 96 per inch (SVG only).
    Pixel,
}

/// How objects are grouped into layers in the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExportLayers {
    /// One layer per object, named after it (DXF); no groups (SVG).
    #[default]
    Object,
    /// The layer (operation) of the object: a group in SVG, a layer in DXF.
    Layer,
    /// Everything on one layer.
    Single,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExportCurves {
    /// Cubics as SPLINEs (DXF).
    #[default]
    Splines,
    /// Chords within the tolerance, for programs without SPLINE support (DXF).
    Polylines,
}

/// What the export dialog's controls hold, as typed. The same shape comes back in the view model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportOptionsInput {
    /// Only the objects that were selected when the dialog opened.
    pub selection_only: bool,
    pub unit: ExportUnit,
    pub layers: ExportLayers,
    /// Use the colour of an object's layer.
    pub use_colors: bool,
    /// SVG: line width in mm.
    pub stroke_mm: String,
    /// SVG: decimals of every coordinate; empty = as exact as possible.
    pub decimals: String,
    pub curves: ExportCurves,
    /// DXF with polylines: largest deviation of a chord in mm.
    pub tolerance_mm: String,
}

impl Default for ExportOptionsInput {
    fn default() -> Self {
        ExportOptionsInput {
            selection_only: false,
            unit: ExportUnit::default(),
            layers: ExportLayers::default(),
            use_colors: true,
            stroke_mm: "0.1".to_owned(),
            decimals: String::new(),
            curves: ExportCurves::default(),
            tolerance_mm: "0.01".to_owned(),
        }
    }
}

/// The option field that holds an unusable value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportField {
    Stroke,
    Decimals,
    Tolerance,
}

/// What a bundle can hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BundleKind {
    Materials,
    Art,
    Profiles,
    Templates,
}

/// One thing that can go in a bundle.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum BundleItem {
    Material(u64),
    Art(u64),
    /// A machine profile by its id.
    Profile(String),
    /// A template by its name (the file name without extension).
    Template(String),
}

/// What happens to a bundle item that exists here with other content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BundleConflict {
    /// Keep what is here.
    #[default]
    Skip,
    /// Take the bundle's version.
    Replace,
    /// Keep both: the incoming one gets " (bundel)" behind its name.
    KeepBoth,
}

/// What a template keeps of the design.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TemplateScope {
    /// The whole design: objects, operations, recipes and assets.
    Everything,
    /// Operations without objects, order constraints and recipes only.
    SettingsOnly,
}

/// The file format of the open import preview.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ImportKind {
    Svg,
    Dxf,
    Hpgl,
    Pdf,
    Illustrator,
    Lbrn2,
}

/// Which pages of a PDF/AI file to import.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImportPages {
    /// Zero-based page index.
    One(usize),
    /// Every page, side by side.
    All,
}

/// One page of a PDF/AI file after its crop box, user unit and rotation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImportPage {
    pub width_mm: f64,
    pub height_mm: f64,
    pub rotation_deg: u16,
}

/// A layer the import proposes: objects that share a stroke/fill colour or a pen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportLayerRow {
    pub name: String,
    pub items: usize,
}

/// The report of a LightBurn project import: layer by layer what was taken over and what was not.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Lbrn2ReportViewModel {
    pub app_version: Option<String>,
    /// Shapes that become objects.
    pub shapes: usize,
    pub layers: Vec<Lbrn2LayerRow>,
    /// What the importer did not take over (unknown elements and attributes, unsupported shapes, ...).
    pub notes: Vec<burnburn_io::import::ImportNote>,
    pub fonts: Vec<burnburn_io::import::FontUse>,
    pub images: Vec<Lbrn2ImageRow>,
    pub warnings: Vec<Lbrn2Warning>,
    /// The project notes (File ▸ Show Notes in LightBurn), when the file has any.
    pub project_notes: Option<String>,
}

/// One LightBurn layer (cut setting) and what became of it.
#[derive(Debug, Clone, PartialEq)]
pub struct Lbrn2LayerRow {
    pub index: u32,
    pub kind: burnburn_io::import::CutKind,
    pub name: String,
    /// Shapes drawn with this layer.
    pub shapes: usize,
    /// An operation is made from it (the settings the process model knows).
    pub taken: bool,
    /// Settings of the layer that were not taken over, with the value they had.
    pub unmapped: Vec<burnburn_io::import::UnmappedValue>,
    /// What the importer had to assume for this layer.
    pub assumptions: Vec<burnburn_io::import::Assumption>,
}

/// A bitmap of the project.
#[derive(Debug, Clone, PartialEq)]
pub struct Lbrn2ImageRow {
    pub entity: String,
    pub size_mm: Option<(f64, f64)>,
    pub state: Lbrn2ImageState,
    /// The image layer it was drawn with.
    pub cut_index: Option<u32>,
    /// LightBurn's image adjustments, carried as written and not applied.
    pub adjustments: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lbrn2ImageState {
    /// The pixels are in the file and become an image object.
    Embedded,
    /// The file only links to a bitmap on the original computer: not imported.
    Linked(String),
    /// Embedded data that does not decode: not imported.
    Unreadable,
}

/// Something about the project that the user should know before it is imported.
#[derive(Debug, Clone, PartialEq)]
pub enum Lbrn2Warning {
    /// The project was made for another machine than the active profile.
    OtherDevice { file: String, active: Option<String> },
    /// The project mirrors the origin; BurnBurn imports the shapes as drawn and does not mirror.
    MirroredOrigin { x: bool, y: bool },
}

/// Why a file feature did nothing; hosts translate it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum FilesMessage {
    /// A PostScript `.ai` from before PDF compatibility: save it as PDF-compatible or SVG.
    ImportLegacyIllustrator,
    /// A password-protected PDF: nothing is read from it.
    ImportEncryptedPdf,
    /// The template file does not end in `.burnburntemplate`; nothing was written.
    TemplateWrongExtension,
    /// The template could not be written; the document is unchanged.
    TemplateSaveFailed,
    /// The template could not be read or is not a template; no window was opened.
    TemplateOpenFailed,
    /// The library file could not be read (damaged or from a newer version): the library is
    /// empty in memory and the file is never overwritten.
    LibraryUnreadable,
    /// The library could not be written; the change exists only in memory.
    LibrarySaveFailed,
    /// Name, material or thickness of a new library entry is missing or not a number.
    MaterialInvalid,
    /// An image layer cannot be stored in or set from a library.
    MaterialImageLayer,
    /// The layer or entry no longer exists.
    MaterialUnknown,
    /// A group or item name is empty, too long or holds control characters.
    ArtNameInvalid,
    ArtGroupExists,
    ArtGroupUnknown,
    /// The group still holds items.
    ArtGroupNotEmpty,
    ArtItemUnknown,
    /// The drawing file does not import (damaged, unsupported or empty).
    ArtFileUnreadable,
    /// The file is not a drawing the library keeps (SVG, DXF, HP-GL, PDF or AI).
    ArtFileFormat,
    /// A DXF without a unit: choose one first.
    ArtScaleRequired,
    /// There is nothing selected to add.
    ArtNothingSelected,
    /// The library holds as many items as it may.
    ArtLibraryFull,
    /// Nothing is selected for the bundle.
    BundleNothingSelected,
    /// The bundle file cannot be read (damaged, from a newer version, or not a bundle); nothing changed.
    BundleUnreadable,
    /// The bundle file could not be written.
    BundleWriteFailed,
    /// The bundle could not be applied; nothing changed.
    BundleApplyFailed,
    /// Nothing to export (no objects, or nothing selected).
    ExportEmpty,
    /// Constraints or modifiers would be lost and the dialog's acknowledgement is not given.
    ExportNeedsAcknowledgement,
    /// An option holds an unusable value; the last good options are in use.
    ExportInvalidOption,
    /// The file could not be written.
    ExportWriteFailed,
}

/// Everything the file panels of one window show.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FilesViewModel {
    pub material: Option<MaterialPanelViewModel>,
    pub art: Option<ArtPanelViewModel>,
    pub bundle: Option<BundleDialogViewModel>,
    pub export: Option<ExportDialogViewModel>,
}

/// The Material Library panel (M217): search, the entries found and what each layer's settings came from.
#[derive(Debug, Clone, PartialEq)]
pub struct MaterialPanelViewModel {
    /// The library is being read on a worker.
    pub loading: bool,
    /// Changes are written to a library file (false: no folder, or the file was unreadable).
    pub persisted: bool,
    /// Why the last input did nothing, or why the library file was not used.
    pub issue: Option<MessageKey>,
    /// Every material name in the library, sorted.
    pub materials: Vec<String>,
    /// The thicknesses (mm) the library holds for the chosen material, ascending.
    pub thicknesses_mm: Vec<f64>,
    pub machines: Vec<String>,
    pub filter: MaterialFilter,
    pub entries: Vec<MaterialRow>,
    pub selected: Option<u64>,
    /// The layers of the project and where their settings came from.
    pub operations: Vec<MaterialOperationRow>,
    /// The machine profile new entries are stored for.
    pub active_machine: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MaterialFilter {
    pub material: Option<String>,
    /// As typed; empty = any thickness.
    pub thickness_text: String,
    pub machine: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MaterialRow {
    pub id: u64,
    pub name: String,
    pub material: String,
    pub thickness_mm: f64,
    pub machine: String,
    pub revision: u32,
    pub mode: OperationKind,
    pub speed: String,
    pub power: String,
    pub passes: String,
    pub notes: String,
}

/// One layer of the project in the Material Library panel.
#[derive(Debug, Clone, PartialEq)]
pub struct MaterialOperationRow {
    pub id: OperationId,
    pub name: String,
    /// A library entry can be applied to it (image layers keep their image settings).
    pub applicable: bool,
    pub origin: MaterialOrigin,
}

/// Where a layer's settings came from. Copied settings never follow the source: a library or
/// recipe update is offered, not applied.
#[derive(Debug, Clone, PartialEq)]
pub enum MaterialOrigin {
    /// Typed in by hand.
    Manual,
    /// Copied from a recipe of this project that has no library behind it.
    Copied { recipe: u64, recipe_name: String, revision: u32 },
    /// Copied from a recipe that was copied from a library entry.
    Linked { recipe: u64, recipe_name: String, revision: u32, entry: u64, status: MaterialLinkStatus },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaterialLinkStatus {
    /// The library entry is the revision that was copied.
    UpToDate,
    /// The library holds a newer revision of the entry.
    UpdateAvailable { applied: u32, latest: u32 },
    /// The library holds an older revision than the one copied (restored from a backup?).
    LibraryOlder { applied: u32, latest: u32 },
    /// The entry is no longer in the library.
    EntryMissing,
    /// The link was made against another library (or the library is not loaded yet).
    OtherLibrary,
}

/// The Art Library panel (M210): groups, thumbnails, search and the item to insert.
#[derive(Debug, Clone, PartialEq)]
pub struct ArtPanelViewModel {
    pub loading: bool,
    pub persisted: bool,
    /// A file or the selection is being added or inserted on a worker.
    pub busy: bool,
    pub issue: Option<MessageKey>,
    pub groups: Vec<ArtGroupRow>,
    pub group: Option<String>,
    pub search: String,
    pub items: Vec<ArtItemRow>,
    pub selected: Option<u64>,
    /// A DXF without a unit is waiting for one: the file, to be sent again with `dxf_unit`.
    pub needs_scale: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtGroupRow {
    pub name: String,
    pub items: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ArtItemRow {
    pub id: u64,
    pub name: String,
    pub group: String,
    pub tags: Vec<String>,
    pub format: burnburn_io::art_library::ArtFormat,
    /// The drawing's outline, 64 × 64 coverage; `None` while it is being made.
    pub thumbnail: Option<Arc<ArtThumbnail>>,
}

/// A small picture of a drawing: `coverage[y * width + x]` is 0 (paper) … 255 (line), top row first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtThumbnail {
    pub width: u32,
    pub height: u32,
    pub coverage: Vec<u8>,
}

/// The bundle dialog (M032/M033, P07): either what to put in a bundle, or what a bundle would change.
#[derive(Debug, Clone, PartialEq)]
pub struct BundleDialogViewModel {
    /// The libraries or the bundle are being read on a worker.
    pub loading: bool,
    /// A bundle is being written or applied.
    pub busy: bool,
    pub issue: Option<MessageKey>,
    pub mode: BundleMode,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BundleMode {
    Export(BundleExportView),
    Import(BundleImportView),
}

#[derive(Debug, Clone, PartialEq)]
pub struct BundleChoice<T> {
    pub id: T,
    pub label: String,
    pub selected: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct BundleExportView {
    pub materials: Vec<BundleChoice<u64>>,
    pub art: Vec<BundleChoice<u64>>,
    pub profiles: Vec<BundleChoice<String>>,
    pub templates: Vec<BundleChoice<String>>,
    /// The file the bundle was written to.
    pub written: Option<PathBuf>,
}

/// How an incoming item relates to what is here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BundleChange {
    New,
    Identical,
    Conflict,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BundleProfileChange {
    New,
    Identical,
    /// Same machine, other content; these fields differ.
    Differs {
        fields: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct BundleRow {
    pub label: String,
    pub change: BundleChange,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BundleProfileRow {
    pub id: String,
    pub bundle_revision: u32,
    pub local_revision: Option<u32>,
    pub change: BundleProfileChange,
    /// The user wants the local profile replaced (only a differing one can be).
    pub replace: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct BundleImportView {
    pub file: String,
    pub materials: Vec<BundleRow>,
    pub art: Vec<BundleRow>,
    pub templates: Vec<BundleRow>,
    pub profiles: Vec<BundleProfileRow>,
    pub conflict: BundleConflict,
    /// Something exists here with other content: the choice above matters.
    pub has_conflicts: bool,
    /// What the import did, after `ApplyBundleImport`.
    pub report: Option<BundleReport>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BundleReport {
    pub materials_added: usize,
    pub materials_replaced: usize,
    pub art_added: usize,
    pub art_replaced: usize,
    pub templates_written: usize,
    pub profiles_added: usize,
    pub profiles_replaced: usize,
    /// Differing profiles the user did not choose to replace.
    pub profiles_pending: Vec<String>,
    pub skipped: usize,
}

/// The export dialog (M026, P05): options, and everything the file cannot carry, listed before
/// anything is written.
#[derive(Debug, Clone, PartialEq)]
pub struct ExportDialogViewModel {
    pub format: ExportFormat,
    pub options: ExportOptionsInput,
    /// The option field whose last input was refused.
    pub field_error: Option<ExportField>,
    /// Objects that go into the file.
    pub objects: usize,
    /// The report of what would be lost or approximated, in object order.
    pub losses: Vec<ExportLossRow>,
    /// Constraints or modifiers are among the losses: the file is only written after the
    /// acknowledgement.
    pub needs_acknowledgement: bool,
    pub acknowledged: bool,
    /// The report is being made on a worker.
    pub checking: bool,
    /// The file is being written.
    pub busy: bool,
    pub issue: Option<MessageKey>,
    /// The file that was written.
    pub written: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExportLossRow {
    pub object: String,
    pub loss: burnburn_io::export::ExportLoss,
}
