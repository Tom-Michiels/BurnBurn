//! Bitmap flows (W2-L1-beeld): import with DPI proposal (P03), Adjust Image with live preview
//! (T09), crop, Mask Image, Replace/Refresh and Save Processed Bitmap.
//!
//! An image in the document is a frame (an imported rectangle whose source is the bitmap asset)
//! plus an Image operation that targets it; the pixels are stretched over the frame's bounds.
//! Everything in [`ImageState`] is window state: previews and decoded sources are never document
//! data and never part of undo.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use burnburn_document::process::{
    DitherMode, ImageCrop, ImageRender, ImageSettings, ImageSharpen, OperationDraft, OperationMode, Passes,
    PowerPercent, PowerRange, ProcessCommand, ProcessSettings, Speed, ToneSettings,
};
use burnburn_document::{
    CommandPayload, CompoundCommand, CompoundLabel, CompoundStep, GeometryCommand, ImportItem, UndoLabel,
};
use burnburn_editor::{BedPicture, BedQuad, Overlay};
use burnburn_geometry::{Bounds, Path, Point, ProductionTolerance, Rect};
use burnburn_image::SourceImage;
use burnburn_io::AssetStore;
use burnburn_types::{
    CancellationSource, ContentHash, DocumentId, GeometryId, HashDomain, Length, OperationId, RequestId, TaskId,
};

use crate::field::{format_mm, parse_number};
use crate::image_panel::{
    ImageAdjustInput, ImageAdjustViewModel, ImageField, ImageImportViewModel, ImageIssue, ImagePreview, ImageViewModel,
};
use crate::image_task::{BitmapInput, BitmapJob, BitmapSource};
use crate::intent::CommandKey;
use crate::task::{TaskError, TaskInput, TaskKind, TaskOutput, TaskPort, TaskRequest, revision_key};
use crate::view_model::{MessageKey, StatusMessage};

use super::{
    AppController, AppUpdate, COMPILE_TOLERANCE_MM, HostEffect, Session, apply_edit, error, file_title, submit,
};

/// Scan interval and dot pitch of a new image operation (mm), the usual 254 lines per inch.
const DEFAULT_IMAGE_INTERVAL_MM: f64 = 0.1;
/// Speed (mm/s) and power (%) of a new image operation: deliberately modest; the user sets the
/// real values in Cut Settings for the material.
const DEFAULT_IMAGE_SPEED_MM_S: f64 = 100.0;
const DEFAULT_IMAGE_POWER_PERCENT: f64 = 20.0;
/// Resolution range accepted for a bitmap (dots per inch).
const DPI_RANGE: std::ops::RangeInclusive<f64> = 1.0..=100_000.0;
/// Slack for "the shape lies inside the image" (mm).
const INSIDE_EPSILON_MM: f64 = 1e-6;

/// Why a bitmap is being read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LoadPurpose {
    /// A new image: opens the import sheet.
    Import,
    /// Replace the pixels of an operation (one history entry).
    Replace(OperationId, Option<RequestId>),
    /// Re-read the file an operation came from.
    Refresh(OperationId, Option<RequestId>),
    /// Decode the stored pixels of a project that was opened from disk into the cache.
    Warm,
}

/// A decoded bitmap waiting for its resolution decision and commit (P03).
#[derive(Clone)]
pub(super) struct PendingBitmap {
    pub(super) file: String,
    pub(super) path: PathBuf,
    pub(super) source: Arc<SourceImage>,
    pub(super) bytes: Arc<[u8]>,
    pub(super) media: &'static str,
    pub(super) dpi_text: String,
    pub(super) issue: Option<ImageIssue>,
}

/// The open Adjust Image panel.
#[derive(Clone)]
pub(super) struct AdjustState {
    pub(super) operation: OperationId,
    pub(super) input: ImageAdjustInput,
    pub(super) issue: Option<(ImageField, ImageIssue)>,
    /// The validated settings the newest preview is for; `None` while the input is invalid.
    pub(super) parsed: Option<ImageSettings>,
    pub(super) preview: Option<ImagePreview>,
    /// The preview as a canvas picture (gray8 expanded once, id from the pixels).
    pub(super) picture: Option<Arc<BedPicture>>,
    /// Newest submitted preview; only its result may be shown.
    pub(super) latest: Option<TaskId>,
}

#[derive(Default)]
pub(super) struct ImageState {
    pub(super) pending: Option<PendingBitmap>,
    pub(super) adjust: Option<AdjustState>,
    /// Decoded originals by pixel hash.
    pub(super) sources: BTreeMap<ContentHash, Arc<SourceImage>>,
    /// The file an image operation was read from (Refresh).
    pub(super) origins: BTreeMap<OperationId, PathBuf>,
    pub(super) loads: BTreeMap<TaskId, LoadPurpose>,
}

// ---- parsing ---------------------------------------------------------------------------------

fn dpi_of(text: &str) -> Result<f64, ImageIssue> {
    let invalid = ImageIssue::InvalidNumber(ImageField::Dpi);
    let value = parse_number(text, &["dpi"]).map_err(|_| invalid)?;
    if value <= 0.0 {
        return Err(invalid);
    }
    if DPI_RANGE.contains(&value) { Ok(value) } else { Err(ImageIssue::OutOfRange(ImageField::Dpi)) }
}

type FieldIssue = (ImageField, ImageIssue);

fn number(text: &str, field: ImageField, range: (f64, f64)) -> Result<f64, FieldIssue> {
    let value = parse_number(text, &[]).map_err(|_| (field, ImageIssue::InvalidNumber(field)))?;
    if (range.0..=range.1).contains(&value) { Ok(value) } else { Err((field, ImageIssue::OutOfRange(field))) }
}

fn pixels(text: &str, field: ImageField) -> Result<u32, FieldIssue> {
    text.trim().parse::<u32>().map_err(|_| (field, ImageIssue::InvalidNumber(field)))
}

/// The settings the panel's text describes, on top of the operation's other settings. `dims` is
/// the size of the source when it is known (crop window check).
fn settings_from(
    input: &ImageAdjustInput,
    base: &ImageSettings,
    dims: Option<(u32, u32)>,
) -> Result<ImageSettings, FieldIssue> {
    let brightness = number(&input.brightness, ImageField::Brightness, (-1.0, 1.0))?;
    let contrast = number(&input.contrast, ImageField::Contrast, (0.0, 10.0))?;
    let gamma = number(&input.gamma, ImageField::Gamma, (0.1, 10.0))?;
    let amount = number(&input.sharpen_amount, ImageField::SharpenAmount, (0.0, 10.0))?;
    let sharpen = if amount >= 0.005 {
        let field = ImageField::SharpenRadius;
        let radius = number(&input.sharpen_radius, field, (0.01, 100.0))?;
        let radius = Length::from_mm(radius).map_err(|_| (field, ImageIssue::OutOfRange(field)))?;
        let field = ImageField::SharpenAmount;
        Some(ImageSharpen::new(amount, radius).map_err(|_| (field, ImageIssue::OutOfRange(field)))?)
    } else {
        None
    };
    let crop = crop_from(input, dims)?;
    let tone = ToneSettings::new(gamma, brightness, contrast)
        .map_err(|_| (ImageField::Gamma, ImageIssue::OutOfRange(ImageField::Gamma)))?;
    base.with_tone(tone)
        .with_sharpen(sharpen)
        .with_gray_method(input.gray)
        .with_crop(crop)
        .map_err(|_| (ImageField::CropWidth, ImageIssue::OutOfRange(ImageField::CropWidth)))
}

fn crop_from(input: &ImageAdjustInput, dims: Option<(u32, u32)>) -> Result<Option<ImageCrop>, FieldIssue> {
    let fields = [ImageField::CropX, ImageField::CropY, ImageField::CropWidth, ImageField::CropHeight];
    let texts = [&input.crop_x, &input.crop_y, &input.crop_width, &input.crop_height];
    let filled = texts.iter().filter(|t| !t.trim().is_empty()).count();
    if filled == 0 {
        return Ok(None);
    }
    if filled < 4 {
        let first_empty = fields[texts.iter().position(|t| t.trim().is_empty()).unwrap_or(0)];
        return Err((first_empty, ImageIssue::CropIncomplete));
    }
    let x = pixels(&input.crop_x, ImageField::CropX)?;
    let y = pixels(&input.crop_y, ImageField::CropY)?;
    let width = pixels(&input.crop_width, ImageField::CropWidth)?;
    let height = pixels(&input.crop_height, ImageField::CropHeight)?;
    if width == 0 {
        return Err((ImageField::CropWidth, ImageIssue::OutOfRange(ImageField::CropWidth)));
    }
    if height == 0 {
        return Err((ImageField::CropHeight, ImageIssue::OutOfRange(ImageField::CropHeight)));
    }
    if let Some((w, h)) = dims {
        if !x.checked_add(width).is_some_and(|e| e <= w) {
            return Err((ImageField::CropWidth, ImageIssue::CropOutsideSource));
        }
        if !y.checked_add(height).is_some_and(|e| e <= h) {
            return Err((ImageField::CropHeight, ImageIssue::CropOutsideSource));
        }
    }
    Ok(Some(ImageCrop { x, y, width, height }))
}

/// The panel text for the values of `settings`.
fn input_of(settings: &ImageSettings) -> ImageAdjustInput {
    let tone = settings.tone();
    let (amount, radius) = settings.sharpen().map_or((0.0, 0.1), |s| (s.amount(), s.radius_mm()));
    let crop = settings.crop();
    let text = |f: fn(&ImageCrop) -> u32| crop.as_ref().map(f).map_or_else(String::new, |v| v.to_string());
    ImageAdjustInput {
        brightness: format_mm(tone.brightness()),
        contrast: format_mm(tone.contrast()),
        gamma: format_mm(tone.gamma()),
        sharpen_amount: format_mm(amount),
        sharpen_radius: format_mm(radius),
        gray: settings.gray_method(),
        crop_x: text(|c| c.x),
        crop_y: text(|c| c.y),
        crop_width: text(|c| c.width),
        crop_height: text(|c| c.height),
    }
}

fn media_of(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(&[0xff, 0xd8]) { "image/jpeg" } else { "image/png" }
}

/// Files the bitmap flow takes over from the generic import (the decoder refuses what it cannot read).
pub(super) fn is_bitmap_path(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .is_some_and(|e| matches!(e.as_str(), "png" | "jpg" | "jpeg" | "bmp" | "tif" | "tiff"))
}

// ---- document lookups -------------------------------------------------------------------------

/// The first selected object that an Image operation targets: (operation, frame).
fn selected_image(s: &Session) -> Option<(OperationId, GeometryId)> {
    let snapshot = s.doc.snapshot();
    s.selection.ids().iter().find_map(|gid| {
        snapshot
            .process()
            .operations()
            .find(|op| matches!(op.mode(), OperationMode::Image(_)) && op.targets().contains(gid))
            .map(|op| (op.id(), *gid))
    })
}

fn image_settings(s: &Session, operation: OperationId) -> Option<ImageSettings> {
    match s.doc.snapshot().process().operation(operation)?.mode() {
        OperationMode::Image(settings) => Some(settings),
        _ => None,
    }
}

fn tolerance() -> Option<ProductionTolerance> {
    ProductionTolerance::new(Length::from_mm(COMPILE_TOLERANCE_MM).ok()?).ok()
}

/// What the image of `operation` is stretched over: its first target.
fn outline_of(s: &Session, operation: OperationId) -> Option<Path> {
    let snapshot = s.doc.snapshot();
    let target = *snapshot.process().operation(operation)?.targets().first()?;
    snapshot.object(target).map(|o| o.path().as_ref().clone())
}

fn source_for(s: &Session, settings: &ImageSettings) -> BitmapSource {
    match s.image.sources.get(&settings.source()) {
        Some(image) => BitmapSource::Decoded(Arc::clone(image)),
        None => BitmapSource::Stored { assets: s.assets.clone(), wanted: settings.source() },
    }
}

fn dims_of(s: &Session, settings: &ImageSettings) -> Option<(u32, u32)> {
    s.image.sources.get(&settings.source()).map(|i| (i.width(), i.height()))
}

fn bitmap_job(s: &Session, operation: OperationId, settings: ImageSettings) -> Option<BitmapJob> {
    Some(BitmapJob {
        settings,
        source: source_for(s, &settings),
        outline: outline_of(s, operation)?,
        tolerance: tolerance()?,
    })
}

// ---- canvas -----------------------------------------------------------------------------------

/// The preview as a picture for the canvas. Its id comes from the pixels, so a renderer keeps the
/// texture of an unchanged preview and replaces it as soon as the values change the result.
fn picture_of(preview: &ImagePreview) -> Option<Arc<BedPicture>> {
    let hash = ContentHash::of(HashDomain::SourcePixels, &preview.gray8);
    let mut id = [0u8; 8];
    id.copy_from_slice(&hash.as_bytes()[..8]);
    BedPicture::from_gray8(u64::from_be_bytes(id), preview.width, preview.height, &preview.gray8).map(Arc::new)
}

/// The open Adjust Image preview, laid on the bounds of the image it is computed for (the scan
/// grid covers the target geometry). A visual aid of the window; never document data.
pub(super) fn preview_overlay(s: &Session) -> Option<Overlay> {
    let adjust = s.image.adjust.as_ref()?;
    let picture = Arc::clone(adjust.picture.as_ref()?);
    let snapshot = s.doc.snapshot();
    let target = *snapshot.process().operation(adjust.operation)?.targets().first()?;
    let bounds = snapshot.object(target)?.bounds()?;
    let (min, max) = (bounds.min(), bounds.max());
    if !(max.x > min.x && max.y > min.y) {
        return None;
    }
    let quad = BedQuad {
        bed_mm: [Point::new(min.x, max.y), Point::new(max.x, max.y), Point::new(max.x, min.y), Point::new(min.x, min.y)],
        uv: [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
    };
    Some(Overlay::ImagePreview { picture, quads: Arc::from(vec![quad]), opacity: 1.0 })
}

// ---- view -------------------------------------------------------------------------------------

pub(super) fn image_view(s: &Session) -> ImageViewModel {
    let import = s.image.pending.as_ref().map(|p| ImageImportViewModel {
        file: p.file.clone(),
        width_px: p.source.width(),
        height_px: p.source.height(),
        dpi_suggestion: p.source.metadata().dpi_suggestion.map(|(x, _)| x),
        dpi_text: p.dpi_text.clone(),
        size_mm: dpi_of(&p.dpi_text).ok().map(|d| size_mm(&p.source, d)),
        issue: p.issue,
    });
    let adjust = s.image.adjust.as_ref().and_then(|a| {
        let snapshot = s.doc.snapshot();
        let op = snapshot.process().operation(a.operation)?;
        let OperationMode::Image(current) = op.mode() else { return None };
        Some(ImageAdjustViewModel {
            operation: a.operation,
            title: op.name().to_owned(),
            source_px: dims_of(s, &current),
            input: a.input.clone(),
            issue: a.issue,
            computing: a.latest.is_some_and(|id| s.pending.contains_key(&id)),
            preview: a.preview.clone(),
            can_apply: a.parsed.is_some_and(|p| p != current),
        })
    });
    ImageViewModel { import, adjust }
}

fn size_mm(source: &SourceImage, dpi: f64) -> (f64, f64) {
    (f64::from(source.width()) * 25.4 / dpi, f64::from(source.height()) * 25.4 / dpi)
}

/// The bitmap import can be committed (valid resolution).
pub(super) fn commit_enabled(s: &Session) -> bool {
    s.image.pending.as_ref().is_some_and(|p| dpi_of(&p.dpi_text).is_ok())
}

/// Enablement of the image commands.
pub(super) fn command_enabled(key: CommandKey, s: &Session) -> bool {
    match key {
        CommandKey::AdjustImage | CommandKey::ReplaceImage | CommandKey::SaveProcessedBitmap => {
            selected_image(s).is_some()
        }
        CommandKey::RefreshImage => selected_image(s).is_some_and(|(op, _)| s.image.origins.contains_key(&op)),
        CommandKey::MaskImage => mask_plan(s).is_some(),
        CommandKey::ApplyImageAdjust => image_view(s).adjust.is_some_and(|a| a.can_apply),
        CommandKey::CancelImageAdjust => s.image.adjust.is_some(),
        _ => false,
    }
}

// ---- import -----------------------------------------------------------------------------------

impl AppController {
    /// Reads and decodes a bitmap on a worker; what happens with it depends on `purpose`.
    pub(super) fn load_bitmap(&mut self, document: DocumentId, input: BitmapInput, purpose: LoadPurpose) -> AppUpdate {
        let Some(id) = self.next_id::<TaskId>() else { return error(Some(document), MessageKey::ImageFailed) };
        let tasks = Arc::clone(&self.ports.tasks);
        self.with_session(document, |s| {
            let source = s.doc.revision();
            if purpose == LoadPurpose::Import {
                for (task, p) in &s.image.loads {
                    if *p == LoadPurpose::Import
                        && let Some((_, older)) = s.pending.get(task)
                    {
                        older.cancel();
                    }
                }
                s.image.pending = None;
            }
            let cancel = CancellationSource::new();
            let request = TaskRequest {
                id,
                document,
                source,
                key: revision_key(TaskKind::LoadBitmap, document, source),
                cancel: cancel.token(),
                input: TaskInput::LoadBitmap { input },
            };
            let update = submit(s, &tasks, request, cancel, document);
            if s.pending.contains_key(&id) {
                s.image.loads.insert(id, purpose);
            }
            update
        })
    }

    pub(super) fn set_image_dpi(&mut self, document: DocumentId, dpi: &str) -> AppUpdate {
        self.with_session(document, |s| {
            let Some(p) = s.image.pending.as_mut() else { return AppUpdate::default() };
            p.dpi_text = dpi.to_owned();
            p.issue = if dpi.trim().is_empty() { None } else { dpi_of(dpi).err() };
            AppUpdate { view_changed: true, ..AppUpdate::default() }
        })
    }

    /// Commit of the bitmap import sheet: the frame and its image operation, as one document
    /// transaction and so one undo step.
    fn commit_bitmap(&mut self, document: DocumentId) -> AppUpdate {
        let request = self.next_id::<RequestId>();
        self.with_session(document, |s| {
            let Some(pending) = s.image.pending.clone() else { return AppUpdate::default() };
            let Ok(dpi) = dpi_of(&pending.dpi_text) else { return AppUpdate::default() };
            let (width, height) = size_mm(&pending.source, dpi);
            let Some(frame) = Length::from_mm(width)
                .ok()
                .zip(Length::from_mm(height).ok())
                .and_then(|(w, h)| Rect::new(Point::new(0.0, 0.0), w, h).ok())
            else {
                return error(Some(document), MessageKey::ImportFailed);
            };
            let Some(draft) = new_image_operation(&pending) else {
                return error(Some(document), MessageKey::CommandRejected);
            };
            let asset = s.assets.add(pending.media, &pending.bytes);
            let item = ImportItem {
                name: format!("Beeld {}", pending.file),
                entity: Some("image".to_owned()),
                path: Path::rectangle(frame),
            };
            let compound = CompoundCommand {
                label: CompoundLabel::ImportImage,
                steps: vec![
                    CompoundStep::Run(CommandPayload::Edit(GeometryCommand::Import { asset, items: vec![item] })),
                    CompoundStep::AddOperationForCreated { draft },
                ],
            };
            let update = apply_edit(s, document, CommandPayload::Compound(compound), request);
            if s.doc.undo_label() != Some(UndoLabel::Compound(CompoundLabel::ImportImage)) {
                return update;
            }
            if let Some(target) = s.selection.single()
                && let Some(op) = s.doc.snapshot().process().operations_using(target).first().copied()
            {
                s.image.origins.insert(op, pending.path.clone());
            }
            s.image.sources.insert(pending.source.content_hash(), Arc::clone(&pending.source));
            s.image.pending = None;
            update
        })
    }
}

fn new_image_operation(p: &PendingBitmap) -> Option<OperationDraft> {
    let pitch = Length::from_mm(DEFAULT_IMAGE_INTERVAL_MM).ok()?;
    let render = ImageRender::Dither(DitherMode::Jarvis);
    let settings = ImageSettings::new(p.source.content_hash(), pitch, pitch, render).ok()?;
    let power = PowerPercent::new(DEFAULT_IMAGE_POWER_PERCENT).ok()?;
    Some(OperationDraft {
        name: format!("Beeld {}", p.file),
        mode: OperationMode::Image(settings),
        settings: ProcessSettings {
            speed: Speed::mm_per_s(DEFAULT_IMAGE_SPEED_MM_S).ok()?,
            power: PowerRange::new(power, power).ok()?,
            passes: Passes::new(1).ok()?,
            air_assist: true,
        },
        output_enabled: true,
    })
}

// ---- adjust -----------------------------------------------------------------------------------

impl AppController {
    /// Runs one image command (some need more than one request id, so they live above the session).
    pub(super) fn image_command(&mut self, document: DocumentId, command: CommandKey) -> AppUpdate {
        match command {
            CommandKey::CommitImport => self.commit_bitmap(document),
            CommandKey::CancelImport => self.with_session(document, |s| {
                s.image.pending = None;
                AppUpdate { view_changed: true, ..AppUpdate::default() }
            }),
            CommandKey::AdjustImage => self.open_adjust(document),
            CommandKey::ApplyImageAdjust => self.apply_adjust(document),
            CommandKey::CancelImageAdjust => self.with_session(document, |s| {
                close_adjust(s);
                AppUpdate::redraw(document)
            }),
            CommandKey::MaskImage => self.mask_image(document),
            CommandKey::RefreshImage => self.refresh_image(document),
            _ => AppUpdate::default(),
        }
    }

    fn open_adjust(&mut self, document: DocumentId) -> AppUpdate {
        let Some(id) = self.next_id::<TaskId>() else { return error(Some(document), MessageKey::CommandRejected) };
        let tasks = Arc::clone(&self.ports.tasks);
        self.with_session(document, |s| {
            let Some((operation, _)) = selected_image(s) else { return AppUpdate::default() };
            let Some(settings) = image_settings(s, operation) else { return AppUpdate::default() };
            close_adjust(s);
            s.image.adjust = Some(AdjustState {
                operation,
                input: input_of(&settings),
                issue: None,
                parsed: Some(settings),
                preview: None,
                picture: None,
                latest: None,
            });
            submit_preview(s, &tasks, document, id, operation, settings)
        })
    }

    pub(super) fn set_image_adjust(&mut self, document: DocumentId, input: ImageAdjustInput) -> AppUpdate {
        let Some(id) = self.next_id::<TaskId>() else { return error(Some(document), MessageKey::CommandRejected) };
        let tasks = Arc::clone(&self.ports.tasks);
        self.with_session(document, |s| {
            let Some(operation) = s.image.adjust.as_ref().map(|a| a.operation) else { return AppUpdate::default() };
            let Some(base) = image_settings(s, operation) else { return AppUpdate::default() };
            let parsed = settings_from(&input, &base, dims_of(s, &base));
            cancel_previews(s);
            let Some(a) = s.image.adjust.as_mut() else { return AppUpdate::default() };
            a.input = input;
            a.latest = None;
            match parsed {
                Err(issue) => {
                    a.issue = Some(issue);
                    a.parsed = None;
                    AppUpdate { view_changed: true, ..AppUpdate::default() }
                }
                Ok(settings) => {
                    a.issue = None;
                    a.parsed = Some(settings);
                    submit_preview(s, &tasks, document, id, operation, settings)
                }
            }
        })
    }

    fn apply_adjust(&mut self, document: DocumentId) -> AppUpdate {
        let request = self.next_id::<RequestId>();
        self.with_session(document, |s| {
            let Some(a) = s.image.adjust.as_ref() else { return AppUpdate::default() };
            let (operation, Some(settings)) = (a.operation, a.parsed) else { return AppUpdate::default() };
            let snapshot = s.doc.snapshot();
            let Some(op) = snapshot.process().operation(operation) else { return AppUpdate::default() };
            if op.mode() == OperationMode::Image(settings) {
                return AppUpdate::default();
            }
            let draft = OperationDraft { mode: OperationMode::Image(settings), ..op.draft() };
            close_adjust(s);
            let edit = ProcessCommand::UpdateOperation { operation, draft };
            apply_edit(s, document, CommandPayload::Process(edit), request)
        })
    }
}

fn cancel_previews(s: &mut Session) {
    for (kind, source) in s.pending.values() {
        if *kind == TaskKind::ImagePreview {
            source.cancel();
        }
    }
}

fn close_adjust(s: &mut Session) {
    cancel_previews(s);
    s.image.adjust = None;
}

/// Starts the preview task for `settings`; its result counts only while it is the newest.
fn submit_preview(
    s: &mut Session,
    tasks: &Arc<dyn TaskPort>,
    document: DocumentId,
    id: TaskId,
    operation: OperationId,
    settings: ImageSettings,
) -> AppUpdate {
    let Some(job) = bitmap_job(s, operation, settings) else { return error(Some(document), MessageKey::ImageFailed) };
    let revision = s.doc.revision();
    let cancel = CancellationSource::new();
    let request = TaskRequest {
        id,
        document,
        source: revision,
        key: revision_key(TaskKind::ImagePreview, document, revision),
        cancel: cancel.token(),
        input: TaskInput::ImagePreview(Box::new(job)),
    };
    let update = submit(s, tasks, request, cancel, document);
    if let Some(a) = s.image.adjust.as_mut()
        && s.pending.contains_key(&id)
    {
        a.latest = Some(id);
    }
    update
}

/// A document change may remove the image the open panel is about.
pub(super) fn sync(s: &mut Session) {
    let gone = s.image.adjust.as_ref().is_some_and(|a| image_settings(s, a.operation).is_none());
    if gone {
        close_adjust(s);
    }
    let snapshot = s.doc.snapshot();
    s.image.origins.retain(|op, _| snapshot.process().operation(*op).is_some());
}

// ---- mask -------------------------------------------------------------------------------------

struct MaskPlan {
    operation: OperationId,
    frame: GeometryId,
    shapes: Vec<GeometryId>,
    settings: ImageSettings,
    crop: ImageCrop,
}

/// One selected image and one or more selected closed shapes that lie inside it.
fn mask_plan(s: &Session) -> Option<MaskPlan> {
    let (operation, frame) = selected_image(s)?;
    let settings = image_settings(s, operation)?;
    let (sw, sh) = dims_of(s, &settings)?;
    let snapshot = s.doc.snapshot();
    let frame_bounds: Bounds = snapshot.object(frame)?.bounds()?;
    let mut shapes = Vec::new();
    let mut area: Option<Bounds> = None;
    for gid in s.selection.ids().iter().filter(|g| **g != frame) {
        let object = snapshot.object(*gid)?;
        let image_target =
            snapshot.process().operations_using(*gid).iter().any(|op| {
                snapshot.process().operation(*op).is_some_and(|o| matches!(o.mode(), OperationMode::Image(_)))
            });
        let path = object.path();
        if image_target || path.subpaths().is_empty() || !path.subpaths().iter().all(|sp| sp.is_closed()) {
            return None;
        }
        let b = object.bounds()?;
        area = Some(area.map_or(b, |a| a.union(b)));
        shapes.push(*gid);
    }
    let area = area?;
    let inside = area.min().x >= frame_bounds.min().x - INSIDE_EPSILON_MM
        && area.min().y >= frame_bounds.min().y - INSIDE_EPSILON_MM
        && area.max().x <= frame_bounds.max().x + INSIDE_EPSILON_MM
        && area.max().y <= frame_bounds.max().y + INSIDE_EPSILON_MM;
    if !inside || frame_bounds.width() <= 0.0 || frame_bounds.height() <= 0.0 {
        return None;
    }
    // The window the operation shows now (the whole source unless cropped already).
    let current = settings.crop().unwrap_or(ImageCrop { x: 0, y: 0, width: sw, height: sh });
    let along = |lo: f64, hi: f64, extent: u32| {
        let a = (lo.clamp(0.0, 1.0) * f64::from(extent)).round() as u32;
        let b = (hi.clamp(0.0, 1.0) * f64::from(extent)).round() as u32;
        let a = a.min(extent.saturating_sub(1));
        (a, b.max(a + 1).min(extent))
    };
    let (fx, fy) = (frame_bounds.width(), frame_bounds.height());
    let (x0, x1) =
        along((area.min().x - frame_bounds.min().x) / fx, (area.max().x - frame_bounds.min().x) / fx, current.width);
    // Pixel rows count from the top, the document's y points up.
    let (y0, y1) =
        along((frame_bounds.max().y - area.max().y) / fy, (frame_bounds.max().y - area.min().y) / fy, current.height);
    let crop = ImageCrop { x: current.x + x0, y: current.y + y0, width: x1 - x0, height: y1 - y0 };
    Some(MaskPlan { operation, frame, shapes, settings, crop })
}

impl AppController {
    /// Mask Image: the shapes become the operation's targets and the crop window is the part of
    /// the picture they cover, so the picture stays where it was. Assign, unassign and update
    /// are one document transaction and so one undo step.
    fn mask_image(&mut self, document: DocumentId) -> AppUpdate {
        let request = self.next_id::<RequestId>();
        self.with_session(document, |s| {
            let Some(plan) = mask_plan(s) else { return AppUpdate::default() };
            let masked = match plan.settings.with_crop(Some(plan.crop)) {
                Ok(m) => m.with_mask(true),
                Err(_) => return error(Some(document), MessageKey::CommandRejected),
            };
            let snapshot = s.doc.snapshot();
            let Some(op) = snapshot.process().operation(plan.operation) else { return AppUpdate::default() };
            let draft = OperationDraft { mode: OperationMode::Image(masked), ..op.draft() };
            let operation = plan.operation;
            let compound = CompoundCommand {
                label: CompoundLabel::MaskImage,
                steps: vec![
                    CompoundStep::Run(CommandPayload::Process(ProcessCommand::AssignGeometry {
                        operation,
                        targets: plan.shapes.clone(),
                    })),
                    CompoundStep::Run(CommandPayload::Process(ProcessCommand::UnassignGeometry {
                        operation,
                        targets: vec![plan.frame],
                    })),
                    CompoundStep::Run(CommandPayload::Process(ProcessCommand::UpdateOperation { operation, draft })),
                ],
            };
            apply_edit(s, document, CommandPayload::Compound(compound), request)
        })
    }
}

// ---- replace, refresh, save --------------------------------------------------------------------

impl AppController {
    pub(super) fn replace_image(&mut self, document: DocumentId, path: PathBuf) -> AppUpdate {
        let request = self.next_id::<RequestId>();
        let Some(operation) = self.sessions.get(&document).and_then(selected_image).map(|(op, _)| op) else {
            return AppUpdate::default();
        };
        let fs = Arc::clone(&self.ports.fs);
        self.load_bitmap(document, BitmapInput::File { path, fs }, LoadPurpose::Replace(operation, request))
    }

    fn refresh_image(&mut self, document: DocumentId) -> AppUpdate {
        let request = self.next_id::<RequestId>();
        let Some((operation, path)) = self.sessions.get(&document).and_then(|s| {
            let (op, _) = selected_image(s)?;
            Some((op, s.image.origins.get(&op)?.clone()))
        }) else {
            return AppUpdate::default();
        };
        let fs = Arc::clone(&self.ports.fs);
        self.load_bitmap(document, BitmapInput::File { path, fs }, LoadPurpose::Refresh(operation, request))
    }

    pub(super) fn save_processed_bitmap(&mut self, document: DocumentId, path: PathBuf) -> AppUpdate {
        let Some(id) = self.next_id::<TaskId>() else { return error(Some(document), MessageKey::ExportFailed) };
        let (tasks, fs) = (Arc::clone(&self.ports.tasks), Arc::clone(&self.ports.fs));
        self.with_session(document, |s| {
            let Some((operation, _)) = selected_image(s) else { return AppUpdate::default() };
            let Some(settings) = image_settings(s, operation) else { return AppUpdate::default() };
            let Some(job) = bitmap_job(s, operation, settings) else {
                return error(Some(document), MessageKey::ExportFailed);
            };
            let revision = s.doc.revision();
            let cancel = CancellationSource::new();
            let request = TaskRequest {
                id,
                document,
                source: revision,
                key: revision_key(TaskKind::SaveBitmap, document, revision),
                cancel: cancel.token(),
                input: TaskInput::SaveBitmap { job: Box::new(job), path, fs },
            };
            submit(s, &tasks, request, cancel, document)
        })
    }

    /// Decodes the stored pixels of the image operations of a freshly opened project, so Adjust,
    /// Mask and the first preview do not wait for a decode.
    pub(super) fn warm_images(&mut self, document: DocumentId) {
        let wanted: Vec<(ContentHash, AssetStore)> = match self.sessions.get(&document) {
            Some(s) => s
                .doc
                .snapshot()
                .process()
                .operations()
                .filter_map(|op| match op.mode() {
                    OperationMode::Image(i) if !s.image.sources.contains_key(&i.source()) => {
                        Some((i.source(), s.assets.clone()))
                    }
                    _ => None,
                })
                .collect(),
            None => return,
        };
        for (hash, assets) in wanted {
            self.load_bitmap(document, BitmapInput::Stored { assets, wanted: hash }, LoadPurpose::Warm);
        }
    }
}

/// Applies a re-read bitmap to an operation: new pixels, same frame and settings.
fn apply_replacement(
    s: &mut Session,
    document: DocumentId,
    operation: OperationId,
    loaded: (Arc<SourceImage>, Arc<[u8]>, PathBuf),
    request: Option<RequestId>,
) -> AppUpdate {
    let (source, bytes, path) = loaded;
    let snapshot = s.doc.snapshot();
    let Some(op) = snapshot.process().operation(operation) else { return AppUpdate::default() };
    let OperationMode::Image(old) = op.mode() else { return AppUpdate::default() };
    if old.source() == source.content_hash() {
        return AppUpdate { view_changed: true, ..AppUpdate::default() };
    }
    let replaced = old.with_source(source.content_hash());
    // A crop in the old pixels only means something while it fits the new ones.
    let fits =
        |c: ImageCrop| c.x.saturating_add(c.width) <= source.width() && c.y.saturating_add(c.height) <= source.height();
    let replaced = match old.crop() {
        Some(c) if !fits(c) => replaced.with_crop(None).unwrap_or(replaced),
        _ => replaced,
    };
    s.assets.add(media_of(&bytes), &bytes);
    s.image.sources.insert(source.content_hash(), source);
    s.image.origins.insert(operation, path);
    let draft = OperationDraft { mode: OperationMode::Image(replaced), ..op.draft() };
    apply_edit(s, document, CommandPayload::Process(ProcessCommand::UpdateOperation { operation, draft }), request)
}

// ---- task results ------------------------------------------------------------------------------

/// Delivery of the results of `LoadBitmap`, `ImagePreview` and `SaveBitmap` tasks.
pub(super) fn finished(
    s: &mut Session,
    document: DocumentId,
    kind: TaskKind,
    id: TaskId,
    result: Result<TaskOutput, TaskError>,
    update: &mut AppUpdate,
) {
    let purpose = s.image.loads.remove(&id);
    let failed = |s: &mut Session, update: &mut AppUpdate, message: MessageKey| {
        s.messages.push(StatusMessage { key: message });
        update.effects.push(HostEffect::ShowError { document: Some(document), message });
    };
    match (kind, result) {
        (_, Err(TaskError::Cancelled)) => {}
        (TaskKind::LoadBitmap, Ok(TaskOutput::ImageLoaded { source, bytes, path })) => match purpose {
            Some(LoadPurpose::Import) => {
                let dpi_text = source.metadata().dpi_suggestion.map_or_else(String::new, |(x, _)| format_mm(x));
                let media = media_of(&bytes);
                s.image.pending =
                    Some(PendingBitmap { file: file_title(&path), path, source, bytes, media, dpi_text, issue: None });
            }
            Some(LoadPurpose::Replace(op, request) | LoadPurpose::Refresh(op, request)) => {
                let applied = apply_replacement(s, document, op, (source, bytes, path), request);
                update.effects.extend(applied.effects);
                update.scenes_changed.extend(applied.scenes_changed);
            }
            Some(LoadPurpose::Warm) => {
                s.image.sources.insert(source.content_hash(), source);
            }
            None => {}
        },
        (TaskKind::LoadBitmap, Err(_)) if purpose == Some(LoadPurpose::Warm) => {}
        (TaskKind::LoadBitmap, Err(_)) => failed(s, update, MessageKey::ImageFailed),
        (TaskKind::ImagePreview, Ok(TaskOutput::ImagePreview { preview, source })) => {
            s.image.sources.insert(source.content_hash(), source);
            if let Some(a) = s.image.adjust.as_mut().filter(|a| a.latest == Some(id)) {
                a.picture = picture_of(&preview);
                a.preview = Some(preview);
                a.latest = None;
                update.scenes_changed.push(document);
                update.effects.push(HostEffect::Redraw { document });
            }
        }
        (TaskKind::ImagePreview, Err(e)) => {
            if let Some(a) = s.image.adjust.as_mut().filter(|a| a.latest == Some(id)) {
                a.latest = None;
                a.parsed = None;
                a.issue = Some(match e {
                    TaskError::Raster(burnburn_image::RasterError::CropOutOfBounds) => {
                        (ImageField::CropWidth, ImageIssue::CropOutsideSource)
                    }
                    TaskError::SourceMissing => (ImageField::Gamma, ImageIssue::SourceMissing),
                    _ => (ImageField::Gamma, ImageIssue::Unreadable),
                });
            }
        }
        (TaskKind::SaveBitmap, Ok(_)) => {}
        (TaskKind::SaveBitmap, Err(_)) => failed(s, update, MessageKey::ExportFailed),
        _ => {}
    }
}
