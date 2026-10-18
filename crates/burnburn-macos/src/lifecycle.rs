//! App lifecycle decisions without AppKit: what `applicationShouldTerminate:` does with dirty
//! documents, and which Finder/Dock open requests become `Open` intents.
//!
//! Quitting asks every dirty document the same native question as its close button, one sheet at
//! a time; `Annuleer` (or cancelling the save panel, or a failed save) stops the quit and keeps
//! every document that is still open as it is. The host performs the steps; this type only
//! decides them, so it is tested against the real `AppController`.

use std::collections::VecDeque;
use std::path::PathBuf;

use burnburn_presentation::{AppViewModel, CloseChoice, DialogKind, HostEffect, UiIntent};
use burnburn_types::DocumentId;

/// What the host does next while quitting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuitStep {
    /// Send `UiIntent::Close` for this document (its window shows the close sheet if dirty).
    Close(DocumentId),
    /// A sheet or save is in progress for the asked document: wait for the user.
    Wait,
    /// Every dirty document was answered and closed: `replyToApplicationShouldTerminate(true)`.
    Terminate,
    /// The user cancelled (or saving failed): `replyToApplicationShouldTerminate(false)`.
    Abort,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuitFlow {
    /// Dirty documents still to ask, in window (view-model) order.
    queue: VecDeque<DocumentId>,
    /// The document whose close was requested and is not closed yet.
    asking: Option<DocumentId>,
    aborted: bool,
}

impl QuitFlow {
    /// `None`: nothing would be lost, terminate right away (`NSTerminateNow`). Otherwise the host
    /// answers `NSTerminateLater` and drives [`QuitFlow::step`].
    pub fn begin(vm: &AppViewModel) -> Option<QuitFlow> {
        let queue: VecDeque<DocumentId> = vm.documents.iter().filter(|d| d.dirty).map(|d| d.id).collect();
        if queue.is_empty() {
            return None;
        }
        Some(QuitFlow { queue, asking: None, aborted: false })
    }

    /// Every intent the host sends while the flow runs: a cancelled answer for the asked
    /// document ends the quit.
    pub fn observe_intent(&mut self, intent: &UiIntent) {
        let Some(asking) = self.asking else { return };
        match intent {
            UiIntent::ConfirmClose { document, choice: CloseChoice::Cancel } if *document == asking => {
                self.aborted = true;
            }
            UiIntent::DialogCancelled { document, dialog: DialogKind::SavePanel | DialogKind::CloseConfirmation }
                if *document == asking =>
            {
                self.aborted = true;
            }
            _ => {}
        }
    }

    /// Every effect presentation asks for: an error for the asked document (e.g. saving failed)
    /// ends the quit instead of leaving it waiting or closing unsaved work.
    pub fn observe_effect(&mut self, effect: &HostEffect) {
        if let (Some(asking), HostEffect::ShowError { document: Some(document), .. }) = (self.asking, effect)
            && *document == asking
        {
            self.aborted = true;
        }
    }

    /// The next step, given the current view model.
    pub fn step(&mut self, vm: &AppViewModel) -> QuitStep {
        if self.aborted {
            return QuitStep::Abort;
        }
        if let Some(asking) = self.asking {
            if vm.document(asking).is_some() {
                return QuitStep::Wait;
            }
            self.asking = None;
        }
        // Documents closed meanwhile (by the user, another window) need no question.
        while let Some(next) = self.queue.pop_front() {
            if vm.document(next).is_some_and(|d| d.dirty) {
                self.asking = Some(next);
                return QuitStep::Close(next);
            }
        }
        QuitStep::Terminate
    }
}

/// Finder double-click, "Open With" or a drop on the Dock icon (`application:openURLs:`): one
/// `Open` per project file. Other files are ignored here (import has its own dialog).
pub fn open_requests(paths: Vec<PathBuf>) -> Vec<UiIntent> {
    paths
        .into_iter()
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("burnburn")))
        .map(|path| UiIntent::Open { path })
        .collect()
}

/// Buttons of the recovery alert, in the order the host adds them.
pub const RECOVERY_ALERT_BUTTONS: [RecoveryChoice; 3] =
    [RecoveryChoice::Restore, RecoveryChoice::Later, RecoveryChoice::Discard];

/// The user's answer to one offered recovery file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryChoice {
    /// Open it as a new, unsaved window.
    Restore,
    /// Keep the file; it is offered again at the next start.
    Later,
    /// Remove the file.
    Discard,
}

impl RecoveryChoice {
    pub fn text(self) -> crate::l10n::HostText {
        match self {
            RecoveryChoice::Restore => crate::l10n::HostText::RecoveryRestore,
            RecoveryChoice::Later => crate::l10n::HostText::RecoveryLater,
            RecoveryChoice::Discard => crate::l10n::HostText::RecoveryDiscard,
        }
    }
}

/// The choice for an alert response; anything unexpected is the safe `Later` (`None`).
pub fn recovery_choice(response: isize) -> Option<RecoveryChoice> {
    let index = response.checked_sub(crate::adapter::response::FIRST_BUTTON)?;
    RECOVERY_ALERT_BUTTONS.get(usize::try_from(index).ok()?).copied()
}

/// Where autosave writes recovery files: under the user's Application Support, never next to a
/// project.
pub fn recovery_dir(application_support: &std::path::Path) -> PathBuf {
    application_support.join("BurnBurn").join("Recovery")
}

/// How often the host asks presentation to autosave (only documents changed since the last one
/// are written).
pub const AUTOSAVE_INTERVAL_S: f64 = 30.0;

/// Title and informative text of the recovery alert for file `index` (1-based) of `count`.
pub fn recovery_message(
    path: &std::path::Path,
    index: usize,
    count: usize,
    locale: crate::l10n::Locale,
) -> (String, String) {
    let file = path.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
    match locale {
        crate::l10n::Locale::Nl => (
            format!("Herstel een niet-bewaarde versie? ({index} van {count})"),
            format!(
                "BurnBurn werd afgesloten met niet-bewaarde wijzigingen. Herstellen opent een nieuw, \
                 onbewaard venster; het projectbestand zelf blijft ongewijzigd. ({file})"
            ),
        ),
        crate::l10n::Locale::En => (
            format!("Restore an unsaved version? ({index} of {count})"),
            format!(
                "BurnBurn quit with unsaved changes. Restoring opens a new, unsaved window; the \
                 project file itself is unchanged. ({file})"
            ),
        ),
    }
}
