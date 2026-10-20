//! Worker jobs of the file features (templates, libraries, bundles, exports). One variant of
//! `TaskInput`/`TaskOutput`/`TaskError` carries them all, so adding a file job touches this file only.

use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

use burnburn_io::art_library::{
    ArtError, ArtFormat, ArtLibrary, ArtPlacement, NewArt, load_art_library, save_art_library,
};
use burnburn_io::bundle::{Bundle, BundleContents, BundleTemplate, decode_bundle, encode_bundle};
use burnburn_io::fs::{FileSystem, FsError};
use burnburn_io::import::DxfScale;
use burnburn_io::material_library::{MaterialLibrary, load_material_library, save_material_library};
use burnburn_io::template::{TemplateContent, TemplateError, new_from_template, save_template};
use burnburn_io::{Project, ProjectUuid};
use burnburn_job::MachineProfile;
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
    /// Thumbnails for these items of the art library (after a bundle brought them in).
    ArtThumbnails { library: Box<ArtLibrary>, items: Vec<u64> },
    /// Read a bundle, and the templates already in `templates_dir` to compare with.
    ReadBundle { path: PathBuf, templates_dir: Option<PathBuf>, fs: Arc<dyn FileSystem> },
    /// Read the chosen templates, pack everything and write the bundle atomically.
    WriteBundle(Box<WriteBundle>),
    /// Write templates (from an imported bundle) into a folder; an existing file of that name is replaced.
    WriteTemplates { dir: PathBuf, templates: Vec<BundleTemplate>, fs: Arc<dyn FileSystem> },
}

/// What goes into a bundle.
#[derive(Clone)]
pub struct WriteBundle {
    pub materials: Option<MaterialLibrary>,
    pub art: Option<ArtLibrary>,
    pub profiles: Vec<MachineProfile>,
    /// Template name and the file to read it from.
    pub templates: Vec<(String, PathBuf)>,
    pub path: PathBuf,
    pub fs: Arc<dyn FileSystem>,
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
            FilesTask::ArtThumbnails { items, .. } => write!(f, "ArtThumbnails({} items)", items.len()),
            FilesTask::ReadBundle { path, .. } => write!(f, "ReadBundle({})", path.display()),
            FilesTask::WriteBundle(w) => write!(f, "WriteBundle({})", w.path.display()),
            FilesTask::WriteTemplates { dir, templates, .. } => {
                write!(f, "WriteTemplates({} to {})", templates.len(), dir.display())
            }
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
    ArtThumbnails {
        thumbnails: Vec<(u64, ArtThumbnail)>,
    },
    BundleRead {
        bundle: Box<Bundle>,
        local_templates: Vec<BundleTemplate>,
    },
    BundleWritten {
        path: PathBuf,
    },
    TemplatesWritten {
        count: usize,
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
    /// The bundle file cannot be read or is not a bundle.
    BundleUnreadable(String),
    /// The bundle (or a template in a bundle) could not be written.
    BundleWrite(String),
}

impl fmt::Display for FilesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FilesError::Template(e) => write!(f, "template: {e}"),
            FilesError::LibraryWrite(e) => write!(f, "library: {e}"),
            FilesError::Art(e) => write!(f, "art library: {e}"),
            FilesError::ArtRead(e) => write!(f, "art file: {e}"),
            FilesError::ArtFormat => f.write_str("not a drawing file"),
            FilesError::BundleUnreadable(e) => write!(f, "bundle: {e}"),
            FilesError::BundleWrite(e) => write!(f, "bundle: {e}"),
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
        FilesTask::ArtThumbnails { library, items } => {
            let mut thumbnails = Vec::new();
            for id in items {
                cancelled(cancel)?;
                if let Some(t) = thumbnail_of(&library, id) {
                    thumbnails.push((id, t));
                }
            }
            Ok(FilesOutput::ArtThumbnails { thumbnails })
        }
        FilesTask::ReadBundle { path, templates_dir, fs } => {
            let unreadable = |e: &dyn fmt::Display| TaskError::Files(FilesError::BundleUnreadable(e.to_string()));
            let bytes = fs.read(&path, BUNDLE_LIMIT).map_err(|e| unreadable(&format!("{e:?}")))?;
            cancelled(cancel)?;
            let bundle = decode_bundle(&bytes).map_err(|e| unreadable(&e))?;
            let mut local_templates = Vec::new();
            if let Some(dir) = templates_dir
                && let Ok(files) = fs.list_dir(&dir)
            {
                for file in files {
                    let is_template = file
                        .extension()
                        .and_then(|e| e.to_str())
                        .is_some_and(|e| e.eq_ignore_ascii_case(burnburn_io::template::TEMPLATE_EXTENSION));
                    let name = file.file_stem().map(|n| n.to_string_lossy().into_owned());
                    if let (true, Some(name)) = (is_template, name)
                        && let Ok(bytes) = fs.read(&file, BUNDLE_LIMIT)
                    {
                        local_templates.push(BundleTemplate { name, bytes });
                    }
                }
            }
            Ok(FilesOutput::BundleRead { bundle: Box::new(bundle), local_templates })
        }
        FilesTask::WriteBundle(write) => {
            let failed = |e: &dyn fmt::Display| TaskError::Files(FilesError::BundleWrite(e.to_string()));
            let mut templates = Vec::new();
            for (name, file) in &write.templates {
                cancelled(cancel)?;
                let bytes =
                    write.fs.read(file, BUNDLE_LIMIT).map_err(|e| failed(&format!("{}: {e:?}", file.display())))?;
                templates.push(BundleTemplate { name: name.clone(), bytes });
            }
            let contents = BundleContents {
                materials: write.materials.as_ref(),
                art: write.art.as_ref(),
                profiles: &write.profiles,
                templates: &templates,
            };
            let bytes = encode_bundle(&contents).map_err(|e| failed(&e))?;
            cancelled(cancel)?;
            crate::task::write_atomically(write.fs.as_ref(), &write.path, &bytes)
                .map_err(|e| failed(&format!("{e:?}")))?;
            Ok(FilesOutput::BundleWritten { path: write.path })
        }
        FilesTask::WriteTemplates { dir, templates, fs } => {
            let failed = |e: &dyn fmt::Display| TaskError::Files(FilesError::BundleWrite(e.to_string()));
            fs.create_dir_all(&dir).map_err(|e| failed(&format!("{e:?}")))?;
            for t in &templates {
                cancelled(cancel)?;
                let file = dir.join(format!("{}.{}", t.name, burnburn_io::template::TEMPLATE_EXTENSION));
                crate::task::write_atomically(fs.as_ref(), &file, &t.bytes).map_err(|e| failed(&format!("{e:?}")))?;
            }
            Ok(FilesOutput::TemplatesWritten { count: templates.len() })
        }
        FilesTask::PlaceArt { library, item } => {
            let placement = library.place(item).map_err(|e| TaskError::Files(FilesError::Art(e)))?;
            Ok(FilesOutput::ArtPlaced { placement: Box::new(placement) })
        }
    }
}

/// Largest drawing file the library takes (the importers have their own, tighter limits).
const ART_FILE_LIMIT: u64 = 64 << 20;
/// Largest bundle or template file read here.
const BUNDLE_LIMIT: u64 = 1 << 30;

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
