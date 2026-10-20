//! Worker jobs of the file features (templates, libraries, bundles, exports). One variant of
//! `TaskInput`/`TaskOutput`/`TaskError` carries them all, so adding a file job touches this file only.

use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

use burnburn_io::fs::FileSystem;
use burnburn_io::material_library::{MaterialLibrary, load_material_library, save_material_library};
use burnburn_io::template::{TemplateContent, TemplateError, new_from_template, save_template};
use burnburn_io::{Project, ProjectUuid};
use burnburn_types::CancellationToken;

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
        }
    }
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum FilesOutput {
    TemplateSaved { path: PathBuf },
    TemplateOpened { project: Box<Project>, path: PathBuf },
    LibrariesLoaded { materials: Loaded<MaterialLibrary> },
    MaterialLibrarySaved,
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
}

impl fmt::Display for FilesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FilesError::Template(e) => write!(f, "template: {e}"),
            FilesError::LibraryWrite(e) => write!(f, "library: {e}"),
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
            let path = dir.join(MATERIAL_LIBRARY_FILE);
            let materials = match fs.exists(&path) {
                Ok(false) => Loaded::Missing,
                Ok(true) => match load_material_library(fs.as_ref(), &path) {
                    Ok(library) => Loaded::Ready(library),
                    Err(e) => Loaded::Unreadable(e.to_string()),
                },
                Err(e) => Loaded::Unreadable(format!("{e:?}")),
            };
            cancelled(cancel)?;
            Ok(FilesOutput::LibrariesLoaded { materials })
        }
        FilesTask::SaveMaterialLibrary { library, path, fs } => {
            save_material_library(fs.as_ref(), &path, &library)
                .map_err(|e| TaskError::Files(FilesError::LibraryWrite(e.to_string())))?;
            Ok(FilesOutput::MaterialLibrarySaved)
        }
    }
}
