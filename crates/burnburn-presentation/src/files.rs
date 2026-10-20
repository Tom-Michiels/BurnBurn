//! File features of the window (W3-UI-bestanden): the import report of every format, templates,
//! the Material and Art Library panels, bundles and the export dialog. Plain data only: hosts
//! translate these into widgets and send [`FilesIntent`]s back; nothing here knows AppKit.

use std::path::PathBuf;

use burnburn_types::{DocumentId, OperationId};

use crate::intent::OperationKind;
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
}

/// Everything the file panels of one window show.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FilesViewModel {
    pub material: Option<MaterialPanelViewModel>,
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
