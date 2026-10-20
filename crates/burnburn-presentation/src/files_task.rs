//! Worker jobs of the file features (templates, libraries, bundles, exports). One variant of
//! `TaskInput`/`TaskOutput`/`TaskError` carries them all, so adding a file job touches this file only.

use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

use burnburn_io::fs::FileSystem;
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
}

impl fmt::Debug for FilesTask {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FilesTask::SaveTemplate { path, content, .. } => {
                write!(f, "SaveTemplate({content:?} to {})", path.display())
            }
            FilesTask::OpenTemplate { path, .. } => write!(f, "OpenTemplate({})", path.display()),
        }
    }
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum FilesOutput {
    TemplateSaved { path: PathBuf },
    TemplateOpened { project: Box<Project>, path: PathBuf },
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum FilesError {
    Template(TemplateError),
}

impl fmt::Display for FilesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FilesError::Template(e) => write!(f, "template: {e}"),
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
    }
}
