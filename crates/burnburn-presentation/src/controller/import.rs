//! The import preview (P03): parsed file, unit decision, page choice and commit.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;

use burnburn_document::process::{OperationDraft, OperationMode};
use burnburn_document::{
    CommandOutcome, CommandPayload, CompoundCommand, CompoundLabel, CompoundStep, DocumentCommand, GeometryCommand,
    ImportItem, UndoLabel,
};
use burnburn_geometry::Bounds;
use burnburn_io::import::{
    DxfScale, ImportDraft, ImportError, ImportLayer, ParseError, PdfDocument, PdfKind, PdfPages,
};
use burnburn_types::{CancellationSource, DocumentId, OperationId, RequestId, TaskId};

use super::{AppController, AppUpdate, Session, after_change, apply_command, apply_edit, error, submit};
use crate::files::{
    FilesMessage, ImportKind, ImportLayerRow, ImportPage, ImportPages, Lbrn2ImageRow, Lbrn2ImageState, Lbrn2LayerRow,
    Lbrn2ReportViewModel, Lbrn2Warning,
};
use crate::image_task::BitmapInput;
use crate::intent::ImportUnit;
use crate::task::{Lbrn2Import, ParsedImport, TaskInput, TaskKind, TaskRequest, revision_key};
use crate::view_model::{ImportViewModel, LAYER_SLOTS, MessageKey};

/// A parsed import waiting for its unit decision and commit (P03).
#[derive(Clone)]
pub(super) struct PendingImport {
    pub(super) file: String,
    pub(super) media: &'static str,
    pub(super) bytes: Arc<[u8]>,
    pub(super) parsed: ParsedImport,
    pub(super) unit: Option<ImportUnit>,
    /// PDF/AI: the page choice and the objects of the chosen pages.
    pub(super) pdf: Option<PdfState>,
    /// The user accepted the layers the file proposes: committing also creates their operations (P04).
    pub(super) layers: bool,
}

/// The page choice of a PDF/AI import: what the user picked, what has been read for it, and the
/// read that is still running (only its result may replace the draft).
#[derive(Clone)]
pub(super) struct PdfState {
    pub(super) doc: Arc<PdfDocument>,
    pub(super) choice: ImportPages,
    pub(super) draft: Arc<ImportDraft>,
    pub(super) requested: Option<(TaskId, ImportPages)>,
}

pub(super) fn media_of(parsed: &ParsedImport) -> &'static str {
    match parsed {
        ParsedImport::Svg(_) => "image/svg+xml",
        ParsedImport::Dxf(_) => "image/vnd.dxf",
        ParsedImport::Hpgl(_) => "application/vnd.hp-hpgl",
        ParsedImport::Pdf(p) if p.doc.kind() == PdfKind::Illustrator => "application/illustrator",
        ParsedImport::Pdf(_) => "application/pdf",
        ParsedImport::Lbrn2(_) => "application/x-lightburn-lbrn2",
    }
}

fn kind_of(parsed: &ParsedImport) -> ImportKind {
    match parsed {
        ParsedImport::Svg(_) => ImportKind::Svg,
        ParsedImport::Dxf(_) => ImportKind::Dxf,
        ParsedImport::Hpgl(_) => ImportKind::Hpgl,
        ParsedImport::Pdf(p) if p.doc.kind() == PdfKind::Illustrator => ImportKind::Illustrator,
        ParsedImport::Pdf(_) => ImportKind::Pdf,
        ParsedImport::Lbrn2(_) => ImportKind::Lbrn2,
    }
}

impl PendingImport {
    pub(super) fn new(file: String, bytes: Arc<[u8]>, parsed: ParsedImport) -> Self {
        let pdf = match &parsed {
            ParsedImport::Pdf(p) => Some(PdfState {
                doc: Arc::clone(&p.doc),
                choice: ImportPages::One(0),
                draft: Arc::new(p.first.clone()),
                requested: None,
            }),
            _ => None,
        };
        PendingImport { file, media: media_of(&parsed), bytes, parsed, unit: None, pdf, layers: false }
    }

    /// The import in millimetres for the current decision; `ScaleRequired` while unknown. Not
    /// for a LightBurn project: that has layers and bitmaps besides its shapes.
    pub(super) fn draft(&self) -> Result<ImportDraft, ImportError> {
        match &self.parsed {
            ParsedImport::Svg(d) | ParsedImport::Hpgl(d) => Ok((**d).clone()),
            ParsedImport::Dxf(d) => d.to_draft(self.unit.and_then(|u| DxfScale::mm_per_unit(u.mm_per_unit()))),
            ParsedImport::Pdf(_) => self.pdf.as_ref().map(|p| (*p.draft).clone()).ok_or(ImportError::Empty),
            ParsedImport::Lbrn2(_) => Err(ImportError::Empty),
        }
    }

    /// Whether Importeer is enabled: the unit is known, nothing is being read and there is something to import.
    pub(super) fn ready(&self) -> bool {
        match &self.parsed {
            ParsedImport::Lbrn2(l) => !l.draft.items.is_empty() || l.importable_images().next().is_some(),
            ParsedImport::Pdf(_) => {
                self.pdf.as_ref().is_some_and(|p| p.requested.is_none() && !p.draft.items.is_empty())
            }
            _ => self.draft().is_ok_and(|d| !d.items.is_empty()),
        }
    }

    /// A LightBurn project: committed as shapes, operations and image objects.
    pub(super) fn is_project(&self) -> bool {
        matches!(self.parsed, ParsedImport::Lbrn2(_))
    }
}

impl Lbrn2Import {
    /// The embedded bitmaps that become image objects: `(asset index, pixels)`.
    pub(crate) fn importable_images(&self) -> impl Iterator<Item = (usize, &Arc<burnburn_image::SourceImage>)> {
        self.images
            .iter()
            .enumerate()
            .filter_map(|(i, s)| s.as_ref().map(|s| (i, s)))
            .filter(|(i, _)| self.draft.report.assets[*i].placement.is_some())
    }
}

/// The message for a file that did not parse.
pub(super) fn parse_failure(e: &ParseError) -> MessageKey {
    match e {
        ParseError::Unsupported(what) if what.contains("legacy PostScript") => {
            MessageKey::Files(FilesMessage::ImportLegacyIllustrator)
        }
        ParseError::Unsupported("encrypted PDF") => MessageKey::Files(FilesMessage::ImportEncryptedPdf),
        _ => MessageKey::ImportFailed,
    }
}

fn layers_of(draft: &ImportDraft) -> Vec<ImportLayerRow> {
    let mut rows: Vec<ImportLayerRow> = Vec::new();
    for item in &draft.items {
        match rows.iter_mut().find(|r| r.name == item.name) {
            Some(row) => row.items += 1,
            None => rows.push(ImportLayerRow { name: item.name.clone(), items: 1, layer: item.layer }),
        }
    }
    rows
}

/// The layers a draft proposes, in the order they first appear: the layer, the name of its first
/// object and the positions of its objects in the draft. Objects of formats without a proposal
/// (SVG, DXF) have no layer and are in no group.
fn layer_groups(draft: &ImportDraft) -> Vec<(ImportLayer, String, Vec<usize>)> {
    let mut groups: Vec<(ImportLayer, String, Vec<usize>)> = Vec::new();
    for (i, item) in draft.items.iter().enumerate() {
        let Some(layer) = item.layer else { continue };
        match groups.iter_mut().find(|g| g.0 == layer) {
            Some(group) => group.2.push(i),
            None => groups.push((layer, item.name.clone(), vec![i])),
        }
    }
    groups
}

/// The operation a proposed layer becomes: strokes and pens are cut lines, fills are filled areas.
fn layer_operation(layer: ImportLayer, name: String) -> Option<OperationDraft> {
    let mut draft = super::layers::new_layer_draft(0)?;
    draft.name = name;
    if matches!(layer, ImportLayer::Fill(_)) {
        draft.mode = OperationMode::Fill(super::layers::default_fill()?);
    }
    Some(draft)
}

/// Commit of a PDF/AI or plotter import whose proposed layers were accepted: the objects and one
/// operation per layer as ONE compound command, one revision, one undo step (P04). `None` when
/// there is nothing to do differently (the plain commit applies).
pub(super) fn commit_with_layers(
    s: &mut Session,
    document: DocumentId,
    request: Option<RequestId>,
    palette: &[[u8; 3]],
) -> Option<AppUpdate> {
    let pending = s.import.as_ref().filter(|i| i.layers && i.ready())?;
    let draft = pending.draft().ok()?;
    let groups = layer_groups(&draft);
    if groups.is_empty() {
        return None;
    }
    let asset = s.assets.add(pending.media, &pending.bytes);
    let Ok(geometry) = draft.into_command(asset) else { return Some(error(Some(document), MessageKey::ImportFailed)) };
    super::layers::sync(s);
    let before_snapshot = s.doc.snapshot();
    let before: BTreeSet<OperationId> = before_snapshot.process().operations().map(|o| o.id()).collect();
    let taken: BTreeSet<u8> = super::layers::slots(&s.layers, &before_snapshot).into_values().collect();
    let chosen = palette_slots(&groups.iter().map(|g| g.0).collect::<Vec<_>>(), palette, &taken);
    let mut steps = vec![CompoundStep::Run(CommandPayload::Edit(geometry))];
    for (layer, name, items) in groups {
        let Some(operation) = layer_operation(layer, name) else {
            return Some(error(Some(document), MessageKey::ImportFailed));
        };
        steps.push(CompoundStep::AddOperationForItems { draft: operation, items });
    }
    let compound = CompoundCommand { label: CompoundLabel::ImportLayers, steps };
    let update = apply_edit(s, document, CommandPayload::Compound(compound), request);
    if s.doc.undo_label() == Some(UndoLabel::Compound(CompoundLabel::ImportLayers)) {
        s.import = None;
        // The new operations, in the order of the layers, take the colours chosen for them.
        let created: Vec<OperationId> =
            s.doc.snapshot().process().operations().map(|o| o.id()).filter(|id| !before.contains(id)).collect();
        if created.len() == chosen.len() {
            for (operation, slot) in created.into_iter().zip(chosen) {
                s.layers.slots_mut().insert(operation, slot);
            }
        }
    }
    Some(update)
}

/// The palette slot of each proposed layer, in order: the palette colour nearest to the layer's own
/// colour (squared distance in sRGB; a tie goes to the lower slot) among the slots no layer has yet,
/// so two similar colours stay two layers. A pen has no colour: it takes the slot of its number
/// (pen 1 = slot 0) or the next free one after it. Only the first [`LAYER_SLOTS`] palette colours
/// count. When every slot is taken the nearest one is shared. An empty palette (a host that has not
/// set one) gives an empty answer and the layers keep the lowest free slots.
fn palette_slots(layers: &[ImportLayer], palette: &[[u8; 3]], taken: &BTreeSet<u8>) -> Vec<u8> {
    let count = palette.len().min(usize::from(LAYER_SLOTS));
    if count == 0 {
        return Vec::new();
    }
    let mut used = taken.clone();
    let mut out = Vec::with_capacity(layers.len());
    for layer in layers {
        let distance = |slot: usize| -> u64 {
            match layer {
                ImportLayer::Stroke(c) | ImportLayer::Fill(c) => {
                    c.iter().zip(palette[slot]).map(|(a, b)| u64::from(a.abs_diff(b)).pow(2)).sum()
                }
                // Cyclic distance from the slot of the pen's number, forwards first.
                ImportLayer::Pen(n) => {
                    let wanted = (n.saturating_sub(1) as usize) % count;
                    ((slot + count - wanted) % count) as u64
                }
            }
        };
        let pick = |only_free: bool| {
            (0..count)
                .filter(|slot| !only_free || !used.contains(&(*slot as u8)))
                .min_by_key(|slot| (distance(*slot), *slot))
        };
        let slot = pick(true).or_else(|| pick(false)).unwrap_or(0) as u8;
        used.insert(slot);
        out.push(slot);
    }
    out
}

fn union(a: Option<Bounds>, b: Option<Bounds>) -> Option<Bounds> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.union(b)),
        (a, b) => a.or(b),
    }
}

pub(super) fn import_view(i: &PendingImport, active_machine: Option<&str>) -> ImportViewModel {
    let kind = kind_of(&i.parsed);
    if let ParsedImport::Lbrn2(l) = &i.parsed {
        return project_view(i, l, active_machine);
    }
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
    let layers = match (kind, &draft) {
        (ImportKind::Hpgl | ImportKind::Pdf | ImportKind::Illustrator, Ok(d)) => layers_of(d),
        _ => Vec::new(),
    };
    let (pages, page_choice, busy) = match &i.pdf {
        Some(p) => (
            p.doc
                .pages()
                .iter()
                .map(|g| ImportPage { width_mm: g.width_mm, height_mm: g.height_mm, rotation_deg: g.rotation_deg })
                .collect(),
            Some(p.choice),
            p.requested.is_some(),
        ),
        None => (Vec::new(), None, false),
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
        kind,
        pages,
        page_choice,
        busy,
        layers,
        layers_accepted: i.layers,
        lbrn2: None,
    }
}

fn project_view(i: &PendingImport, l: &Lbrn2Import, active_machine: Option<&str>) -> ImportViewModel {
    let d = &l.draft;
    let bounds = d
        .items
        .iter()
        .filter_map(|it| it.path.bounds())
        .chain(l.importable_images().filter_map(|(a, _)| d.report.assets[a].placement.as_ref()?.bounds()))
        .reduce(|a, b| union(Some(a), Some(b)).unwrap_or(a));
    let open_contours = d.items.iter().flat_map(|it| it.path.subpaths()).filter(|sp| !sp.is_closed()).count();
    let layers = d
        .cut_settings
        .iter()
        .map(|c| Lbrn2LayerRow {
            index: c.index,
            kind: c.kind,
            name: c
                .draft
                .as_ref()
                .map(|o| o.name.clone())
                .or_else(|| c.image.as_ref().map(|im| im.name.clone()))
                .unwrap_or_else(|| format!("C{:02}", c.index)),
            shapes: c.items.len(),
            taken: c.draft.is_some() || c.image.as_ref().is_some_and(|im| im.render.is_some()),
            unmapped: c.unmapped.clone(),
            assumptions: c.assumptions.clone(),
        })
        .collect();
    let images = d
        .report
        .assets
        .iter()
        .enumerate()
        .map(|(a, note)| Lbrn2ImageRow {
            entity: note.entity.clone(),
            size_mm: note.size_mm,
            state: match (&l.images.get(a), &note.linked_file) {
                (Some(Some(_)), _) => Lbrn2ImageState::Embedded,
                (_, Some(file)) if note.embedded_chars == 0 => Lbrn2ImageState::Linked(file.clone()),
                _ => Lbrn2ImageState::Unreadable,
            },
            cut_index: note.cut_index,
            adjustments: note.adjustments.clone(),
        })
        .collect();
    let project = &d.report.project;
    let mut warnings = Vec::new();
    if let Some(device) = project.device_name.as_deref().filter(|n| !n.trim().is_empty()) {
        let matches = active_machine.is_some_and(|m| m.to_lowercase().contains(&device.to_lowercase()));
        if !matches {
            warnings
                .push(Lbrn2Warning::OtherDevice { file: device.to_owned(), active: active_machine.map(str::to_owned) });
        }
    }
    if project.mirror_x == Some(true) || project.mirror_y == Some(true) {
        warnings.push(Lbrn2Warning::MirroredOrigin {
            x: project.mirror_x == Some(true),
            y: project.mirror_y == Some(true),
        });
    }
    let shapes = d.items.len() + l.importable_images().count();
    ImportViewModel {
        file: i.file.clone(),
        needs_unit: false,
        unit: None,
        units: None,
        size_mm: bounds.map(|b| (b.width(), b.height())),
        items: shapes,
        skipped: Vec::new(),
        open_contours,
        kind: ImportKind::Lbrn2,
        pages: Vec::new(),
        page_choice: None,
        busy: false,
        layers: Vec::new(),
        layers_accepted: false,
        lbrn2: Some(Lbrn2ReportViewModel {
            app_version: d.report.app_version.clone(),
            shapes: d.items.len(),
            layers,
            notes: d.report.notes.clone(),
            fonts: d.report.fonts.clone(),
            images,
            warnings,
            project_notes: project.notes.as_ref().map(|n| n.text.clone()).filter(|t| !t.trim().is_empty()),
        }),
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

    /// Accept or take back the layers a PDF/AI or plotter file proposes (P04).
    pub(super) fn set_import_layers(&mut self, document: DocumentId, create: bool) -> AppUpdate {
        self.with_session(document, |s| {
            let Some(import) = s.import.as_mut() else { return AppUpdate::default() };
            // Only a file that proposes layers can have them accepted.
            let proposes = import.draft().is_ok_and(|d| !layer_groups(&d).is_empty());
            import.layers = create && proposes;
            AppUpdate { view_changed: true, ..AppUpdate::default() }
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

    /// PDF/AI: the user picked other pages. The objects are read on a worker; an older read that
    /// is still running is cancelled and its late result is dropped (it no longer matches).
    pub(super) fn set_import_pages(&mut self, document: DocumentId, pages: ImportPages) -> AppUpdate {
        let Some(id) = self.next_id::<TaskId>() else { return error(Some(document), MessageKey::ImportFailed) };
        let tasks = Arc::clone(&self.ports.tasks);
        self.with_session(document, |s| {
            let Some(pdf) = s.import.as_mut().and_then(|i| i.pdf.as_mut()) else { return AppUpdate::default() };
            let count = pdf.doc.pages().len();
            let exists = match pages {
                ImportPages::One(page) => page < count,
                ImportPages::All => count > 0,
            };
            if !exists || pdf.choice == pages {
                return AppUpdate::default();
            }
            let older = pdf.requested.take().map(|(task, _)| task);
            pdf.choice = pages;
            pdf.requested = Some((id, pages));
            let input = TaskInput::ImportPages {
                pdf: Arc::clone(&pdf.doc),
                pages: match pages {
                    ImportPages::One(page) => PdfPages::One(page),
                    ImportPages::All => PdfPages::All,
                },
            };
            if let Some((_, source)) = older.and_then(|t| s.pending.get(&t)) {
                source.cancel();
            }
            let source = s.doc.revision();
            let cancel = CancellationSource::new();
            let request = TaskRequest {
                id,
                document,
                source,
                key: revision_key(TaskKind::ImportPages, document, source),
                cancel: cancel.token(),
                input,
            };
            submit(s, &tasks, request, cancel, document)
        })
    }

    /// Commit of a LightBurn project: the shapes, one operation per layer the process model can
    /// express, and each embedded bitmap as an image object with its image operation, as ONE
    /// compound command: one revision, one undo step (G5-UI-gaten P04).
    pub(super) fn commit_project_import(&mut self, document: DocumentId) -> AppUpdate {
        let request = self.next_id::<RequestId>();
        self.with_session(document, |s| {
            let Some(pending) = s.import.clone() else { return AppUpdate::default() };
            let ParsedImport::Lbrn2(project) = &pending.parsed else { return AppUpdate::default() };
            let mut steps = Vec::new();
            // Creation order inside the compound: item i of the project is position i.
            let mut created = 0usize;
            if !project.draft.items.is_empty() {
                let asset = s.assets.add(pending.media, &pending.bytes);
                let Ok(geometry) = project.draft.geometry_command(asset) else {
                    return error(Some(document), MessageKey::ImportFailed);
                };
                steps.push(CompoundStep::Run(CommandPayload::Edit(geometry)));
                created = project.draft.items.len();
                for p in &project.draft.cut_settings {
                    let Some(draft) = p.draft.clone() else { continue };
                    let items: Vec<usize> = p.items.iter().copied().filter(|i| *i < created).collect();
                    if !items.is_empty() {
                        steps.push(CompoundStep::AddOperationForItems { draft, items });
                    }
                }
            }
            let mut sources = Vec::new();
            for (a, source) in project.importable_images() {
                let note = &project.draft.report.assets[a];
                let (Some(bitmap), Some(frame)) = (note.data.as_ref(), note.placement.clone()) else { continue };
                let media = match bitmap.format {
                    burnburn_io::import::RasterFormat::Jpeg => "image/jpeg",
                    _ => "image/png",
                };
                let asset = s.assets.add(media, &bitmap.bytes);
                let item =
                    ImportItem { name: format!("Beeld {}", a + 1), entity: Some(note.entity.clone()), path: frame };
                steps.push(CompoundStep::Run(CommandPayload::Edit(GeometryCommand::Import {
                    asset,
                    items: vec![item],
                })));
                let position = created;
                created += 1;
                sources.push(source);
                if let Some(draft) =
                    note.cut_index.and_then(|index| project.draft.image_operation(index, source.content_hash()))
                {
                    steps.push(CompoundStep::AddOperationForItems { draft, items: vec![position] });
                }
            }
            if steps.is_empty() {
                s.import = None;
                return AppUpdate { view_changed: true, ..AppUpdate::default() };
            }
            let compound = CompoundCommand { label: CompoundLabel::ImportProject, steps };
            let update = apply_edit(s, document, CommandPayload::Compound(compound), request);
            if s.doc.undo_label() == Some(UndoLabel::Compound(CompoundLabel::ImportProject)) {
                for source in sources {
                    s.image.sources.insert(source.content_hash(), Arc::clone(source));
                }
                s.import = None;
            }
            let mut update = update;
            update.view_changed = true;
            update
        })
    }
}

/// The result of a page read: only the newest request of the open preview may set the draft.
pub(super) fn pages_drafted(s: &mut Session, task: TaskId, pages: PdfPages, draft: ImportDraft) {
    let Some(pdf) = s.import.as_mut().and_then(|i| i.pdf.as_mut()) else { return };
    let wanted = match pages {
        PdfPages::One(page) => ImportPages::One(page),
        PdfPages::All => ImportPages::All,
    };
    if pdf.requested == Some((task, wanted)) {
        pdf.draft = Arc::new(draft);
        pdf.requested = None;
    }
}

/// A failed page read ends the wait but keeps the last good objects; the user sees the message.
pub(super) fn pages_failed(s: &mut Session, task: TaskId) {
    if let Some(pdf) = s.import.as_mut().and_then(|i| i.pdf.as_mut())
        && pdf.requested.is_some_and(|(id, _)| id == task)
    {
        pdf.requested = None;
    }
}

/// `apply_edit` that also hands back the outcome (the ids a following command targets).
pub(in crate::controller) fn apply_with_outcome(
    s: &mut Session,
    document: DocumentId,
    payload: CommandPayload,
    request: Option<RequestId>,
) -> (AppUpdate, Option<CommandOutcome>) {
    let Some(request) = request else { return (error(Some(document), MessageKey::CommandRejected), None) };
    let cmd = DocumentCommand { document, expected: s.doc.revision(), request, payload };
    let result = apply_command(&s.fonts, &mut s.doc, cmd);
    let outcome = result.as_ref().ok().cloned();
    (after_change(s, document, result), outcome)
}

pub(in crate::controller) fn merge(into: &mut AppUpdate, other: AppUpdate) {
    into.effects.extend(other.effects);
    into.view_changed |= other.view_changed;
    for d in other.scenes_changed {
        if !into.scenes_changed.contains(&d) {
            into.scenes_changed.push(d);
        }
    }
}
