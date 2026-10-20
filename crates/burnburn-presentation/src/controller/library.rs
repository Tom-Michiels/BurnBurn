//! The file features of the window (W3-UI-bestanden): import pages, templates, the Material and
//! Art Library panels, bundles and the export dialog.

use std::path::PathBuf;

use burnburn_types::DocumentId;

use super::{AppController, AppUpdate};
use crate::files::FilesIntent;
use crate::task::TaskOutcome;

/// A worker job of the file features that is waiting for its result.
#[derive(Debug, Clone)]
pub(super) enum FilesJob {
    SaveTemplate { document: DocumentId },
    OpenTemplate { document: DocumentId, path: PathBuf },
}

impl AppController {
    pub(super) fn files(&mut self, intent: FilesIntent) -> AppUpdate {
        match intent {
            FilesIntent::SetImportPages { document, pages } => self.set_import_pages(document, pages),
            FilesIntent::SaveAsTemplate { document, path, scope } => self.save_as_template(document, path, scope),
            FilesIntent::NewFromTemplate { path } => self.new_from_template(path),
            FilesIntent::SetRecentTemplates { paths } => {
                self.restore_recent_templates(paths);
                AppUpdate { view_changed: true, ..AppUpdate::default() }
            }
            FilesIntent::ClearRecentTemplates => {
                AppUpdate { effects: self.clear_recent_templates(), view_changed: true, ..AppUpdate::default() }
            }
        }
    }

    /// A worker result of a file-feature job.
    pub(super) fn files_finished(&mut self, job: FilesJob, outcome: TaskOutcome) -> AppUpdate {
        match job {
            FilesJob::SaveTemplate { document } => self.template_saved(document, outcome),
            FilesJob::OpenTemplate { document, path } => self.template_opened(document, path, outcome),
        }
    }
}
