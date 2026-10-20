//! The import preview (P03): parsed file, unit decision and commit.

use std::path::PathBuf;
use std::sync::Arc;

use burnburn_io::import::{DxfScale, ImportDraft, ImportError};
use burnburn_types::{CancellationSource, DocumentId, TaskId};

use super::{AppController, AppUpdate, error, submit};
use crate::image_task::BitmapInput;
use crate::intent::ImportUnit;
use crate::task::{ParsedImport, TaskInput, TaskKind, TaskRequest, revision_key};
use crate::view_model::{ImportViewModel, MessageKey};

/// A parsed import waiting for its unit decision and commit (P03).
#[derive(Clone)]
pub(super) struct PendingImport {
    pub(super) file: String,
    pub(super) media: &'static str,
    pub(super) bytes: Arc<[u8]>,
    pub(super) parsed: ParsedImport,
    pub(super) unit: Option<ImportUnit>,
}

impl PendingImport {
    /// The import in millimetres for the current decision; `ScaleRequired` while unknown.
    pub(super) fn draft(&self) -> Result<ImportDraft, ImportError> {
        match &self.parsed {
            ParsedImport::Svg(d) => Ok((**d).clone()),
            ParsedImport::Dxf(d) => d.to_draft(self.unit.and_then(|u| DxfScale::mm_per_unit(u.mm_per_unit()))),
        }
    }
}

pub(super) fn import_view(i: &PendingImport) -> ImportViewModel {
    let draft = i.draft();
    let size_mm = draft.as_ref().ok().and_then(|d| {
        d.items.iter().filter_map(|it| it.path.bounds()).reduce(|a, b| a.union(b)).map(|b| (b.width(), b.height()))
    });
    let (skipped, open_contours, units, items) = match (&draft, &i.parsed) {
        (Ok(d), _) => (d.report.skipped.clone(), d.report.open_contours.len(), Some(d.report.units), d.items.len()),
        // Unitless DXF: the report is known only in drawing units; entity counts still apply.
        (Err(_), ParsedImport::Dxf(doc)) => match doc.to_draft(DxfScale::mm_per_unit(1.0)) {
            Ok(d) => (d.report.skipped, d.report.open_contours.len(), None, d.items.len()),
            Err(_) => (Vec::new(), 0, None, 0),
        },
        (Err(_), _) => (Vec::new(), 0, None, 0),
    };
    ImportViewModel {
        file: i.file.clone(),
        needs_unit: matches!(draft, Err(ImportError::ScaleRequired)),
        unit: i.unit,
        units,
        size_mm,
        items,
        skipped,
        open_contours,
    }
}

impl AppController {
    /// Parses a file on a worker; the preview opens when the result arrives (P03).
    pub(super) fn parse_import(&mut self, document: DocumentId, path: PathBuf) -> AppUpdate {
        // Bitmaps have their own preview (resolution instead of unit) and commit.
        if super::image::is_bitmap_path(&path) {
            let fs = Arc::clone(&self.ports.fs);
            return self.load_bitmap(document, BitmapInput::File { path, fs }, super::image::LoadPurpose::Import);
        }
        let Some(id) = self.next_id::<TaskId>() else { return error(Some(document), MessageKey::ImportFailed) };
        let (tasks, fs) = (Arc::clone(&self.ports.tasks), Arc::clone(&self.ports.fs));
        self.with_session(document, |s| {
            let source = s.doc.revision();
            let cancel = CancellationSource::new();
            let request = TaskRequest {
                id,
                document,
                source,
                key: revision_key(TaskKind::ParseImport, document, source),
                cancel: cancel.token(),
                input: TaskInput::ParseImport { path, fs },
            };
            submit(s, &tasks, request, cancel, document)
        })
    }

    pub(super) fn set_import_unit(&mut self, document: DocumentId, unit: ImportUnit) -> AppUpdate {
        self.with_session(document, |s| match s.import.as_mut() {
            Some(i) => {
                i.unit = Some(unit);
                AppUpdate { view_changed: true, ..AppUpdate::default() }
            }
            None => AppUpdate::default(),
        })
    }
}
