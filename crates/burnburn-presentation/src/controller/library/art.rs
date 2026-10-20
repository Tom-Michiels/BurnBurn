//! The Art Library panel (M210): groups, thumbnails, adding drawings and inserting them.
//!
//! Adding and inserting read and parse a drawing, so they run on a worker. A worker result made
//! from an older copy of the library is not applied over a newer one: the add is repeated on the
//! current library instead.

use std::path::PathBuf;
use std::sync::Arc;

use burnburn_document::{CommandPayload, GeometryCommand, ImportItem};
use burnburn_geometry::{Affine, Bounds, Point, Vector};
use burnburn_io::art_library::{ArtError, ArtFormat, ArtLibrary, ArtPlacement};
use burnburn_io::export::{ExportItem, SvgOptions, export_svg_with};
use burnburn_types::{DocumentId, RequestId};

use super::{AppController, AppUpdate, FilesJob, Libraries, LoadState};
use crate::controller::{Session, apply_edit};
use crate::files::{ArtGroupRow, ArtItemRow, ArtPanelViewModel, FilesIntent, FilesMessage};
use crate::files_task::{AddArt, ArtSource, FilesError, FilesOutput, FilesTask};
use crate::intent::ImportUnit;
use crate::task::{TaskError, TaskOutcome, TaskOutput};
use crate::view_model::MessageKey;

/// Window state of the panel.
#[derive(Debug, Default)]
pub(in crate::controller) struct ArtUi {
    group: Option<String>,
    search: String,
    selected: Option<u64>,
    issue: Option<MessageKey>,
    needs_scale: Option<PathBuf>,
}

/// Where a new item's drawing comes from, without the file system handle (the job is `Debug`).
#[derive(Debug, Clone)]
pub(in crate::controller) enum Origin {
    File(PathBuf),
    Bytes(ArtFormat, Arc<[u8]>),
}

/// An item to add, kept whole so the add can be repeated on a newer library.
#[derive(Debug, Clone)]
pub(in crate::controller) struct AddRequest {
    origin: Origin,
    name: String,
    group: String,
    dxf_mm_per_unit: Option<f64>,
    generation: u64,
}

fn message(error: &ArtError) -> FilesMessage {
    match error {
        ArtError::InvalidName => FilesMessage::ArtNameInvalid,
        ArtError::DuplicateGroup(_) => FilesMessage::ArtGroupExists,
        ArtError::UnknownGroup(_) => FilesMessage::ArtGroupUnknown,
        ArtError::GroupNotEmpty(_) => FilesMessage::ArtGroupNotEmpty,
        ArtError::UnknownItem(_) => FilesMessage::ArtItemUnknown,
        ArtError::ScaleRequired => FilesMessage::ArtScaleRequired,
        ArtError::TooMany => FilesMessage::ArtLibraryFull,
        _ => FilesMessage::ArtFileUnreadable,
    }
}

impl AppController {
    pub(super) fn art_intent(&mut self, intent: FilesIntent) -> AppUpdate {
        match intent {
            FilesIntent::OpenArtLibrary { document } => {
                self.ensure_libraries();
                self.with_session(document, |s| {
                    s.files.art.get_or_insert_with(ArtUi::default);
                    AppUpdate { view_changed: true, ..AppUpdate::default() }
                })
            }
            FilesIntent::CloseArtLibrary { document } => self.with_session(document, |s| {
                s.files.art = None;
                AppUpdate { view_changed: true, ..AppUpdate::default() }
            }),
            FilesIntent::SelectArtGroup { document, group } => self.with_art_ui(document, |ui| {
                ui.group = group;
                ui.issue = None;
            }),
            FilesIntent::SetArtSearch { document, text } => self.with_art_ui(document, |ui| {
                ui.search = text;
                ui.issue = None;
            }),
            FilesIntent::SelectArtItem { document, item } => self.with_art_ui(document, |ui| {
                ui.selected = item;
                ui.issue = None;
            }),
            FilesIntent::AddArtGroup { document, name } => {
                self.change_art(document, |library| library.add_group(&name))
            }
            FilesIntent::RenameArtGroup { document, old, new } => {
                let update = self.change_art(document, |library| library.rename_group(&old, &new));
                self.with_art_ui(document, |ui| {
                    if ui.issue.is_none() && ui.group.as_deref() == Some(old.as_str()) {
                        ui.group = Some(new);
                    }
                });
                update
            }
            FilesIntent::RemoveArtGroup { document, name } => {
                let update = self.change_art(document, |library| library.remove_group(&name, None));
                self.with_art_ui(document, |ui| {
                    if ui.issue.is_none() && ui.group.as_deref() == Some(name.as_str()) {
                        ui.group = None;
                    }
                });
                update
            }
            FilesIntent::RenameArtItem { document, item, name } => {
                self.change_art(document, |library| library.rename_item(item, &name))
            }
            FilesIntent::MoveArtItem { document, item, group } => {
                self.change_art(document, |library| library.move_item(item, &group))
            }
            FilesIntent::RemoveArtItem { document, item } => {
                let update = self.change_art(document, |library| {
                    if library.remove_item(item) { Ok(()) } else { Err(ArtError::UnknownItem(item)) }
                });
                self.libs.thumbnails.remove(&item);
                self.with_art_ui(document, |ui| {
                    if ui.issue.is_none() && ui.selected == Some(item) {
                        ui.selected = None;
                    }
                });
                update
            }
            FilesIntent::AddArtFromFile { document, path, group, name, dxf_unit } => {
                let name = if name.trim().is_empty() {
                    path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
                } else {
                    name
                };
                let dxf_mm_per_unit = dxf_unit.map(ImportUnit::mm_per_unit);
                self.add_art(document, Origin::File(path), name, group, dxf_mm_per_unit)
            }
            FilesIntent::AddArtFromSelection { document, name, group } => {
                self.add_art_from_selection(document, name, group)
            }
            FilesIntent::PlaceArt { document, item, at } => self.place_art(document, item, at),
            _ => AppUpdate::default(),
        }
    }

    fn with_art_ui(&mut self, document: DocumentId, f: impl FnOnce(&mut ArtUi)) -> AppUpdate {
        self.with_session(document, |s| match s.files.art.as_mut() {
            Some(ui) => {
                f(ui);
                AppUpdate { view_changed: true, ..AppUpdate::default() }
            }
            None => AppUpdate::default(),
        })
    }

    fn art_issue(&mut self, document: DocumentId, message: Option<FilesMessage>) -> AppUpdate {
        self.with_art_ui(document, |ui| ui.issue = message.map(MessageKey::Files))
    }

    /// A quick change of the library on the UI thread (groups, names): written back when it worked.
    fn change_art(
        &mut self,
        document: DocumentId,
        change: impl FnOnce(&mut ArtLibrary) -> Result<(), ArtError>,
    ) -> AppUpdate {
        let Some(library) = self.libs.art.as_mut() else { return AppUpdate::default() };
        match change(library) {
            Ok(()) => {
                self.libs.art_generation += 1;
                let effects = self.flush_art();
                let mut update = self.art_issue(document, None);
                update.effects.extend(effects);
                update
            }
            Err(e) => self.art_issue(document, Some(message(&e))),
        }
    }

    fn add_art_from_selection(&mut self, document: DocumentId, name: String, group: String) -> AppUpdate {
        let Some(s) = self.sessions.get(&document) else { return AppUpdate::default() };
        let snapshot = s.doc.snapshot();
        let objects: Vec<_> = s.selection.ids().iter().filter_map(|id| snapshot.object(*id)).collect();
        let items: Vec<ExportItem<'_>> = objects.iter().map(|o| ExportItem::new(o.name(), o.path())).collect();
        if items.is_empty() {
            return self.art_issue(document, Some(FilesMessage::ArtNothingSelected));
        }
        let svg = export_svg_with(&items, &SvgOptions::default());
        drop(items);
        let Ok((text, _)) = svg else { return self.art_issue(document, Some(FilesMessage::ArtNothingSelected)) };
        let origin = Origin::Bytes(ArtFormat::Svg, Arc::from(text.into_bytes()));
        self.add_art(document, origin, name, group, None)
    }

    fn add_art(
        &mut self,
        document: DocumentId,
        origin: Origin,
        name: String,
        group: String,
        dxf_mm_per_unit: Option<f64>,
    ) -> AppUpdate {
        if self.libs.art.is_none() {
            return AppUpdate::default();
        }
        let request = AddRequest { origin, name, group, dxf_mm_per_unit, generation: self.libs.art_generation };
        self.submit_add(document, request);
        self.art_issue(document, None)
    }

    fn submit_add(&mut self, document: DocumentId, request: AddRequest) {
        let Some(library) = self.libs.art.clone() else { return };
        let source = match &request.origin {
            Origin::File(path) => ArtSource::File { path: path.clone(), fs: Arc::clone(&self.ports.fs) },
            Origin::Bytes(format, bytes) => ArtSource::Bytes { format: *format, bytes: Arc::clone(bytes) },
        };
        let task = FilesTask::AddArt(Box::new(AddArt {
            library,
            source,
            name: request.name.clone(),
            group: request.group.clone(),
            dxf_scale: request.dxf_mm_per_unit,
        }));
        if self.submit_files(task, FilesJob::AddArt { document, request: Box::new(request) }) {
            self.libs.art_busy += 1;
        }
    }

    pub(super) fn art_added(
        &mut self,
        document: DocumentId,
        mut request: AddRequest,
        outcome: TaskOutcome,
    ) -> AppUpdate {
        self.libs.art_busy = self.libs.art_busy.saturating_sub(1);
        match outcome.result {
            Ok(TaskOutput::Files(FilesOutput::ArtAdded { library, id, thumbnail })) => {
                if self.libs.art_generation != request.generation {
                    // The library changed while the worker was busy: repeat the add on the current one.
                    request.generation = self.libs.art_generation;
                    self.submit_add(document, request);
                    return AppUpdate { view_changed: true, ..AppUpdate::default() };
                }
                self.libs.art = Some(*library);
                if let Some(t) = thumbnail {
                    self.libs.thumbnails.insert(id, Arc::new(t));
                }
                self.libs.art_generation += 1;
                let effects = self.flush_art();
                let mut update = self.with_art_ui(document, |ui| {
                    ui.selected = Some(id);
                    ui.issue = None;
                    ui.needs_scale = None;
                });
                update.effects.extend(effects);
                update.view_changed = true;
                update
            }
            Err(TaskError::Cancelled) => AppUpdate { view_changed: true, ..AppUpdate::default() },
            Err(TaskError::Files(FilesError::Art(ArtError::ScaleRequired))) => {
                let file = match request.origin {
                    Origin::File(path) => Some(path),
                    Origin::Bytes(..) => None,
                };
                self.with_art_ui(document, |ui| {
                    ui.needs_scale = file;
                    ui.issue = None;
                })
            }
            Err(TaskError::Files(FilesError::Art(e))) => {
                let m = message(&e);
                self.with_art_ui(document, |ui| {
                    ui.needs_scale = None;
                    ui.issue = Some(MessageKey::Files(m));
                })
            }
            Err(TaskError::Files(FilesError::ArtFormat)) => self.art_issue(document, Some(FilesMessage::ArtFileFormat)),
            _ => self.art_issue(document, Some(FilesMessage::ArtFileUnreadable)),
        }
    }

    fn place_art(&mut self, document: DocumentId, item: u64, at: Option<(f64, f64)>) -> AppUpdate {
        let Some(library) = self.libs.art.as_ref() else { return AppUpdate::default() };
        if library.item(item).is_none() {
            return self.art_issue(document, Some(FilesMessage::ArtItemUnknown));
        }
        let subset = library.subset(&[item]);
        let Some(s) = self.sessions.get(&document) else { return AppUpdate::default() };
        let at = at.map(|(x, y)| s.camera.points_to_world(&s.viewport, [x, y]));
        let task = FilesTask::PlaceArt { library: Box::new(subset), item };
        if self.submit_files(task, FilesJob::PlaceArt { document, at }) {
            self.libs.art_busy += 1;
        }
        self.art_issue(document, None)
    }

    pub(super) fn art_placed(&mut self, document: DocumentId, at: Option<Point>, outcome: TaskOutcome) -> AppUpdate {
        self.libs.art_busy = self.libs.art_busy.saturating_sub(1);
        match outcome.result {
            Ok(TaskOutput::Files(FilesOutput::ArtPlaced { placement })) => {
                let request = self.next_id::<RequestId>();
                self.with_session(document, |s| insert_art(s, document, *placement, at, request))
            }
            Err(TaskError::Cancelled) => AppUpdate { view_changed: true, ..AppUpdate::default() },
            _ => self.art_issue(document, Some(FilesMessage::ArtFileUnreadable)),
        }
    }
}

/// Puts the drawing in the document, its centre on `at` (or on the view's centre), as one import.
fn insert_art(
    s: &mut Session,
    document: DocumentId,
    placement: ArtPlacement,
    at: Option<Point>,
    request: Option<RequestId>,
) -> AppUpdate {
    let bounds = placement.draft.items.iter().filter_map(|i| i.path.bounds()).reduce(Bounds::union);
    let target = at.unwrap_or_else(|| s.camera.center_mm());
    let shift = bounds.map_or(Vector { x: 0.0, y: 0.0 }, |b| Vector {
        x: target.x - (b.min().x + b.max().x) / 2.0,
        y: target.y - (b.min().y + b.max().y) / 2.0,
    });
    let Ok(move_by) = Affine::translate(shift) else {
        return AppUpdate::default();
    };
    let asset = s.assets.add(placement.media, &placement.bytes);
    let items: Vec<ImportItem> = placement
        .draft
        .items
        .into_iter()
        .map(|i| ImportItem { name: i.name, entity: Some(i.entity), path: i.path.transform(&move_by) })
        .collect();
    if items.is_empty() {
        return AppUpdate::default();
    }
    apply_edit(s, document, CommandPayload::Edit(GeometryCommand::Import { asset, items }), request)
}

pub(super) fn panel_view(s: &Session, libs: &Libraries) -> Option<ArtPanelViewModel> {
    let ui = s.files.art.as_ref()?;
    let library: Option<&ArtLibrary> = libs.art.as_ref();
    let search = ui.search.trim();
    let items: Vec<ArtItemRow> = library
        .map(|l| {
            let found = if search.is_empty() { l.items().iter().collect() } else { l.search(search) };
            found
                .into_iter()
                .filter(|i| ui.group.as_deref().is_none_or(|g| i.group() == g))
                .map(|i| ArtItemRow {
                    id: i.id(),
                    name: i.name().to_owned(),
                    group: i.group().to_owned(),
                    tags: i.tags().to_vec(),
                    format: i.format(),
                    thumbnail: libs.thumbnails.get(&i.id()).cloned(),
                })
                .collect()
        })
        .unwrap_or_default();
    let groups = library
        .map(|l| l.groups().iter().map(|g| ArtGroupRow { name: g.clone(), items: l.items_in(g).len() }).collect())
        .unwrap_or_default();
    let library_issue = if libs.art_unreadable {
        Some(MessageKey::Files(FilesMessage::LibraryUnreadable))
    } else if libs.art_writer.failed {
        Some(MessageKey::Files(FilesMessage::LibrarySaveFailed))
    } else {
        None
    };
    Some(ArtPanelViewModel {
        loading: libs.state != LoadState::Ready || library.is_none(),
        persisted: libs.art_persisted(),
        busy: libs.art_busy > 0,
        issue: ui.issue.or(library_issue),
        groups,
        group: ui.group.clone(),
        search: ui.search.clone(),
        items,
        selected: ui.selected,
        needs_scale: ui.needs_scale.clone(),
    })
}
