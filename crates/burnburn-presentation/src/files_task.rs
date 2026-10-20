//! Worker jobs of the file features (templates, libraries, bundles, exports). One variant of
//! `TaskInput`/`TaskOutput`/`TaskError` carries them all, so adding a file job touches this file only.

use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

use burnburn_io::art_library::{
    ArtError, ArtFormat, ArtLibrary, ArtPlacement, NewArt, load_art_library, save_art_library,
};
use burnburn_io::fs::{FileSystem, FsError};
use burnburn_io::import::DxfScale;
use burnburn_io::material_library::{MaterialLibrary, load_material_library, save_material_library};
use burnburn_io::template::{TemplateContent, TemplateError, new_from_template, save_template};
use burnburn_io::{Project, ProjectUuid};
use burnburn_types::CancellationToken;

use crate::files::ArtThumbnail;
use crate::task::TaskError;

#[derive(Clone)]
#[non_exhaustive]
pub enum FilesTask {
    /// Write a template file; the project it was made from is not touched.
    SaveTemplate { project: Box<Project>, path: PathBuf, content: TemplateContent, fs: Arc<dyn FileSystem> },
    /// Read a template as a new project with a fresh identity; the template is only read.
    OpenTemplate { path: PathBuf, uuid: ProjectUuid, fs: Arc<dyn FileSystem> },
    /// Read the libraries in the user's folder; a missing file is an empty library.
    LoadLibraries { dir: PathBuf, fs: Arc<dyn FileSystem> },
    /// Write the whole material library atomically.
    SaveMaterialLibrary { library: Box<MaterialLibrary>, path: PathBuf, fs: Arc<dyn FileSystem> },
    /// Write the whole art library atomically.
    SaveArtLibrary { library: Box<ArtLibrary>, path: PathBuf, fs: Arc<dyn FileSystem> },
    /// Add a drawing to a copy of the art library (the file is read and proven to import here);
    /// the new library and the item's thumbnail come back.
    AddArt(Box<AddArt>),
    /// The drawing of one library item, ready to insert (`library` holds just that item).
    PlaceArt { library: Box<ArtLibrary>, item: u64 },
}

/// Where the drawing of a new art item comes from.
#[derive(Clone)]
pub enum ArtSource {
    File { path: PathBuf, fs: Arc<dyn FileSystem> },
    Bytes { format: ArtFormat, bytes: Arc<[u8]> },
}

/// One art item to add.
#[derive(Clone)]
pub struct AddArt {
    pub library: ArtLibrary,
    pub source: ArtSource,
    pub name: String,
    pub group: String,
    pub dxf_scale: Option<f64>,
}

/// File name of the art library inside the user's folder.
pub const ART_LIBRARY_FILE: &str = "art-library.json";

/// The library format of a drawing file, by its extension.
pub fn art_format_of(path: &std::path::Path) -> Option<ArtFormat> {
    Some(match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "svg" => ArtFormat::Svg,
        "dxf" => ArtFormat::Dxf,
        "plt" | "hpgl" | "hgl" => ArtFormat::Hpgl,
        "pdf" | "ai" => ArtFormat::Pdf,
        _ => return None,
    })
}

/// File name of the material library inside the user's folder.
pub const MATERIAL_LIBRARY_FILE: &str = "material-library.json";

impl fmt::Debug for FilesTask {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FilesTask::SaveTemplate { path, content, .. } => {
                write!(f, "SaveTemplate({content:?} to {})", path.display())
            }
            FilesTask::OpenTemplate { path, .. } => write!(f, "OpenTemplate({})", path.display()),
            FilesTask::LoadLibraries { dir, .. } => write!(f, "LoadLibraries({})", dir.display()),
            FilesTask::SaveMaterialLibrary { path, .. } => write!(f, "SaveMaterialLibrary({})", path.display()),
            FilesTask::SaveArtLibrary { path, .. } => write!(f, "SaveArtLibrary({})", path.display()),
            FilesTask::AddArt(add) => write!(f, "AddArt({:?} to {:?})", add.name, add.group),
            FilesTask::PlaceArt { item, .. } => write!(f, "PlaceArt({item})"),
        }
    }
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum FilesOutput {
    TemplateSaved {
        path: PathBuf,
    },
    TemplateOpened {
        project: Box<Project>,
        path: PathBuf,
    },
    LibrariesLoaded {
        materials: Loaded<MaterialLibrary>,
        art: Loaded<ArtLibrary>,
        thumbnails: Vec<(u64, ArtThumbnail)>,
    },
    MaterialLibrarySaved,
    ArtLibrarySaved,
    ArtAdded {
        library: Box<ArtLibrary>,
        id: u64,
        thumbnail: Option<ArtThumbnail>,
    },
    ArtPlaced {
        placement: Box<ArtPlacement>,
    },
}

/// What reading one library file gave.
#[derive(Debug, Clone)]
pub enum Loaded<T> {
    /// No file yet: the library starts empty.
    Missing,
    Ready(T),
    /// The file exists and cannot be used (damaged, newer, too large); it is never overwritten.
    Unreadable(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum FilesError {
    Template(TemplateError),
    /// A library file could not be written.
    LibraryWrite(String),
    /// The art library refused the item (or the item's file does not import).
    Art(ArtError),
    /// The file of a new art item could not be read.
    ArtRead(FsError),
    /// The extension is none of SVG, DXF, HP-GL, PDF or AI.
    ArtFormat,
}

impl fmt::Display for FilesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FilesError::Template(e) => write!(f, "template: {e}"),
            FilesError::LibraryWrite(e) => write!(f, "library: {e}"),
            FilesError::Art(e) => write!(f, "art library: {e}"),
            FilesError::ArtRead(e) => write!(f, "art file: {e}"),
            FilesError::ArtFormat => f.write_str("not a drawing file"),
        }
    }
}

pub(crate) fn run(task: FilesTask, cancel: &CancellationToken) -> Result<FilesOutput, TaskError> {
    let cancelled = |c: &CancellationToken| c.check().map_err(|_| TaskError::Cancelled);
    cancelled(cancel)?;
    match task {
        FilesTask::SaveTemplate { project, path, content, fs } => {
            save_template(fs.as_ref(), &path, &project, content)
                .map_err(|e| TaskError::Files(FilesError::Template(e)))?;
            Ok(FilesOutput::TemplateSaved { path })
        }
        FilesTask::OpenTemplate { path, uuid, fs } => {
            let project =
                new_from_template(fs.as_ref(), &path, uuid).map_err(|e| TaskError::Files(FilesError::Template(e)))?;
            cancelled(cancel)?;
            Ok(FilesOutput::TemplateOpened { project: Box::new(project), path })
        }
        FilesTask::LoadLibraries { dir, fs } => {
            let materials = read_library(fs.as_ref(), &dir.join(MATERIAL_LIBRARY_FILE), load_material_library);
            let art = read_library(fs.as_ref(), &dir.join(ART_LIBRARY_FILE), load_art_library);
            cancelled(cancel)?;
            let mut thumbnails = Vec::new();
            if let Loaded::Ready(library) = &art {
                for item in library.items() {
                    cancelled(cancel)?;
                    if let Some(t) = thumbnail_of(library, item.id()) {
                        thumbnails.push((item.id(), t));
                    }
                }
            }
            Ok(FilesOutput::LibrariesLoaded { materials, art, thumbnails })
        }
        FilesTask::SaveMaterialLibrary { library, path, fs } => {
            save_material_library(fs.as_ref(), &path, &library)
                .map_err(|e| TaskError::Files(FilesError::LibraryWrite(e.to_string())))?;
            Ok(FilesOutput::MaterialLibrarySaved)
        }
        FilesTask::SaveArtLibrary { library, path, fs } => {
            save_art_library(fs.as_ref(), &path, &library)
                .map_err(|e| TaskError::Files(FilesError::LibraryWrite(e.to_string())))?;
            Ok(FilesOutput::ArtLibrarySaved)
        }
        FilesTask::AddArt(add) => {
            let AddArt { mut library, source, name, group, dxf_scale } = *add;
            let (format, bytes): (ArtFormat, Arc<[u8]>) = match source {
                ArtSource::Bytes { format, bytes } => (format, bytes),
                ArtSource::File { path, fs } => {
                    let format = art_format_of(&path).ok_or(TaskError::Files(FilesError::ArtFormat))?;
                    let bytes = fs.read(&path, ART_FILE_LIMIT).map_err(|e| TaskError::Files(FilesError::ArtRead(e)))?;
                    (format, Arc::from(bytes))
                }
            };
            cancelled(cancel)?;
            let new = NewArt {
                name: &name,
                group: &group,
                tags: Vec::new(),
                format,
                bytes: &bytes,
                dxf_scale: dxf_scale.and_then(DxfScale::mm_per_unit),
            };
            let id = library.add_item(new).map_err(|e| TaskError::Files(FilesError::Art(e)))?;
            let thumbnail = thumbnail_of(&library, id);
            Ok(FilesOutput::ArtAdded { library: Box::new(library), id, thumbnail })
        }
        FilesTask::PlaceArt { library, item } => {
            let placement = library.place(item).map_err(|e| TaskError::Files(FilesError::Art(e)))?;
            Ok(FilesOutput::ArtPlaced { placement: Box::new(placement) })
        }
    }
}

/// Largest drawing file the library takes (the importers have their own, tighter limits).
const ART_FILE_LIMIT: u64 = 64 << 20;

fn read_library<T, E: fmt::Display>(
    fs: &dyn FileSystem,
    path: &std::path::Path,
    load: impl Fn(&dyn FileSystem, &std::path::Path) -> Result<T, E>,
) -> Loaded<T> {
    match fs.exists(path) {
        Ok(false) => Loaded::Missing,
        Ok(true) => match load(fs, path) {
            Ok(library) => Loaded::Ready(library),
            Err(e) => Loaded::Unreadable(e.to_string()),
        },
        Err(e) => Loaded::Unreadable(format!("{e:?}")),
    }
}

fn thumbnail_of(library: &ArtLibrary, item: u64) -> Option<ArtThumbnail> {
    let placement = library.place(item).ok()?;
    crate::thumbnail::render(placement.draft.items.iter().map(|i| &i.path))
}
