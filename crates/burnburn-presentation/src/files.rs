//! File features of the window (W3-UI-bestanden): the import report of every format, templates,
//! the Material and Art Library panels, bundles and the export dialog. Plain data only: hosts
//! translate these into widgets and send [`FilesIntent`]s back; nothing here knows AppKit.

use std::path::PathBuf;

use burnburn_types::DocumentId;

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
}
