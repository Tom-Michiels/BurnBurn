//! Empty states and progress rows (LF17): what a panel shows when there is nothing in it (a
//! centred symbol, one sentence and at most one button) and what the running tasks of a document
//! look like (a title, a progress bar that is determinate when the task reports progress, and a
//! cancel button where the task can be cancelled). Both hosts draw these; nothing here knows
//! AppKit or Win32.

use burnburn_presentation::{
    AppViewModel, ArtPanelViewModel, BundleDialogViewModel, BundleMode, CommandKey, DocumentViewModel,
    HistoryViewModel, MachineHubViewModel, MaterialPanelViewModel, RecognitionViewModel, RemnantsViewModel, TaskKind,
    UiIntent,
};
use burnburn_types::{DocumentId, TaskId};

use crate::l10n::Locale;
use crate::l10n_empty::{EmptyText as T, empty_text};

/// The one button of an empty state (or of a progress row) and what it sends.
#[derive(Debug, Clone, PartialEq)]
pub struct EmptyAction {
    pub label: String,
    pub intent: UiIntent,
}

/// What a panel shows instead of an empty list.
#[derive(Debug, Clone, PartialEq)]
pub struct EmptyState {
    /// SF Symbol name (the Windows host maps it to its own glyph).
    pub symbol: &'static str,
    pub text: String,
    pub action: Option<EmptyAction>,
}

/// The four lists of the machine hub window.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct HubEmptyStates {
    pub devices: Option<EmptyState>,
    pub queue: Option<EmptyState>,
    pub history: Option<EmptyState>,
    pub proofs: Option<EmptyState>,
}

fn state(symbol: &'static str, text: T, locale: Locale) -> EmptyState {
    EmptyState { symbol, text: empty_text(text, locale).to_owned(), action: None }
}

pub fn object_list_empty(vm: &AppViewModel, document: DocumentId, locale: Locale) -> Option<EmptyState> {
    let doc = vm.document(document)?;
    doc.objects.is_empty().then(|| EmptyState {
        action: Some(EmptyAction {
            label: empty_text(T::ObjectsAction, locale).to_owned(),
            intent: UiIntent::Invoke { document: Some(document), command: CommandKey::ToolRectangle },
        }),
        ..state("square.dashed", T::Objects, locale)
    })
}

pub fn operations_empty(vm: &AppViewModel, document: DocumentId, locale: Locale) -> Option<EmptyState> {
    vm.document(document)?.operations.is_empty().then(|| state("slider.horizontal.3", T::Operations, locale))
}

/// Empty once the library has been read and holds no entry.
pub fn material_empty(panel: &MaterialPanelViewModel, locale: Locale) -> Option<EmptyState> {
    (!panel.loading && panel.entries.is_empty()).then(|| state("square.stack.3d.up", T::Materials, locale))
}

/// Empty once the library has been read, nothing is being added and it holds no item.
pub fn art_empty(panel: &ArtPanelViewModel, locale: Locale) -> Option<EmptyState> {
    (!panel.loading && !panel.busy && panel.items.is_empty())
        .then(|| state("photo.on.rectangle.angled", T::Art, locale))
}

pub fn remnants_empty(remnants: &RemnantsViewModel, locale: Locale) -> Option<EmptyState> {
    remnants.rows.is_empty().then(|| state("rectangle.dashed", T::Remnants, locale))
}

/// Empty without a template in use and without a kept one.
pub fn recognition_empty(recognition: &RecognitionViewModel, locale: Locale) -> Option<EmptyState> {
    (recognition.template.is_none() && recognition.saved.is_empty())
        .then(|| state("viewfinder", T::Recognition, locale))
}

pub fn history_empty(history: &HistoryViewModel, locale: Locale) -> Option<EmptyState> {
    history.rows.is_empty().then(|| state("clock.arrow.circlepath", T::History, locale))
}

/// Export: no library or template holds anything. Import: the bundle holds nothing. Not while the
/// libraries or the bundle are still being read.
pub fn bundle_empty(dialog: &BundleDialogViewModel, locale: Locale) -> Option<EmptyState> {
    if dialog.loading {
        return None;
    }
    match &dialog.mode {
        BundleMode::Export(e) => {
            (e.materials.is_empty() && e.art.is_empty() && e.profiles.is_empty() && e.templates.is_empty())
                .then(|| state("shippingbox", T::BundleExport, locale))
        }
        BundleMode::Import(i) => {
            (i.materials.is_empty() && i.art.is_empty() && i.profiles.is_empty() && i.templates.is_empty())
                .then(|| state("shippingbox", T::BundleImport, locale))
        }
    }
}

pub fn hub_empty(hub: &MachineHubViewModel, locale: Locale) -> HubEmptyStates {
    HubEmptyStates {
        devices: hub.devices.is_empty().then(|| state("externaldrive.badge.questionmark", T::HubDevices, locale)),
        queue: hub.queue.rows.is_empty().then(|| state("tray", T::HubQueue, locale)),
        history: hub.history.is_empty().then(|| state("clock", T::HubHistory, locale)),
        proofs: hub.calibration.rows.is_empty().then(|| state("ruler", T::HubProofs, locale)),
    }
}

/// One running task as the progress area shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct ProgressRow {
    pub task: TaskId,
    pub title: String,
    /// 0.0..=1.0 where the task reports progress; `None` = an indeterminate bar.
    pub fraction: Option<f64>,
    /// The cancel button, where the task can be cancelled.
    pub cancel: Option<EmptyAction>,
}

/// The title of a task worth a progress row; `None` for the short ones (measure, previews,
/// autosave, reads) that would only flicker.
fn task_title(kind: TaskKind) -> Option<T> {
    match kind {
        TaskKind::Compile | TaskKind::ExportRd | TaskKind::ExportGcode | TaskKind::PrepareJob => Some(T::TaskCompile),
        TaskKind::Trace | TaskKind::Centerline | TaskKind::TraceStudy => Some(T::TaskTrace),
        TaskKind::Nest => Some(T::TaskNest),
        TaskKind::TestJob => Some(T::TaskTest),
        _ => None,
    }
}

/// The long-running tasks of `document`, in the order they were started.
pub fn progress_rows(document: &DocumentViewModel, locale: Locale) -> Vec<ProgressRow> {
    document
        .tasks
        .iter()
        .filter_map(|badge| {
            let title = task_title(badge.kind)?;
            Some(ProgressRow {
                task: badge.id,
                title: empty_text(title, locale).to_owned(),
                fraction: badge.progress_permille.map(|p| f64::from(p.min(1000)) / 1000.0),
                cancel: badge.cancellable.then(|| EmptyAction {
                    label: empty_text(T::TaskCancel, locale).to_owned(),
                    intent: UiIntent::CancelTask { document: document.id, task: badge.id },
                }),
            })
        })
        .collect()
}
