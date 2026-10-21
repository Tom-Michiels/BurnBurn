//! W4-L8b: the Windows host's panel families (laser window, image, files, camera, app) as plain
//! data, without the `windows` crate, tested on every OS.
//!
//! Every family builds a [`PanelSpec`] (rows of controls with text, enablement and tooltip) from
//! the plain-data panel models that both hosts share (`crate::panels*`) and turns a [`PanelEvent`]
//! (a control id, a notification code and the values of all controls, read by the Win32 layer)
//! into a [`PanelReaction`]: an intent for the presentation controller, a file dialog request or a
//! host action. No domain logic lives here (DR-05: numbers go on as typed).
//!
//! **Command ids**: this lane owns `6000..=7999` of the `WM_COMMAND` id space ([`ids`]).

pub mod files;
pub mod filters;
pub mod ids;
pub mod image;
pub mod laser;
pub mod spec;

use std::path::PathBuf;

pub use spec::{
    ControlKind, ControlPatch, ControlSpec, Metrics, PanelEvent, PanelSpec, PanelValues, Placement, Row, SpecDiff,
    diff_specs, layout, notify,
};

use burnburn_presentation::UiIntent;
use burnburn_types::DocumentId;

/// Which family a panel window belongs to (one window per family and document, except the app
/// panels which are per application).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Family {
    Laser,
    Image,
    Files,
    Camera,
    App,
}

/// A file type filter of a dialog (`IFileDialog::SetFileTypes`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileFilter {
    pub name: String,
    /// Extensions without dot or star, lower case.
    pub extensions: Vec<&'static str>,
}

/// Why a file dialog was opened: the answer comes back to the family that asked.
#[derive(Debug, Clone, PartialEq)]
pub enum FilePurpose {
    OpenProject,
    ImportDrawing { document: DocumentId },
    ReplaceImage { document: DocumentId },
    TraceImage { document: DocumentId },
    SaveProcessedBitmap { document: DocumentId },
    ExportRd { document: DocumentId },
    ExportDrawing { document: DocumentId },
    TemplateSave { document: DocumentId, scope: burnburn_presentation::TemplateScope },
    TemplateOpen,
    BundleOpen { document: DocumentId },
    BundleWrite { document: DocumentId },
    ArtAddFile { document: DocumentId, name: String, unit_item: isize },
    Diagnostics { options: burnburn_presentation::DiagnosticOptions },
    SupportBundle { options: burnburn_presentation::DiagnosticOptions },
}

/// Why a yes/no question was asked.
#[derive(Debug, Clone, PartialEq)]
pub enum ConfirmPurpose {
    /// Deleting a file on the controller (not undoable, not in any project): the intent to send on yes.
    DeleteControllerFile { intent: Box<UiIntent> },
}

/// A native dialog the host must show.
#[derive(Debug, Clone, PartialEq)]
pub enum DialogRequest {
    Open { purpose: FilePurpose, title: String, filters: Vec<FileFilter> },
    Save { purpose: FilePurpose, title: String, suggested_name: String, filters: Vec<FileFilter> },
    Confirm { purpose: ConfirmPurpose, title: String, body: String, confirm: String, cancel: String },
}

/// What the host does besides sending intents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostRequest {
    /// Show the family's panel window (creating it first) for the active document.
    ShowPanel(Family),
    /// Hide the family's panel window.
    HidePanel(Family),
    /// Open the user manual in the default browser.
    OpenUrl(String),
    /// Show a modal message (About).
    MessageBox { title: String, body: String },
}

/// The reaction to one panel event.
#[derive(Debug, Clone, PartialEq)]
pub enum PanelReaction {
    Nothing,
    Intent(UiIntent),
    /// Several intents, sent in order (e.g. add the art group, then the art item).
    Intents(Vec<UiIntent>),
    Dialog(DialogRequest),
    Host(HostRequest),
}

impl PanelReaction {
    /// The intents this reaction sends, in order.
    pub fn intents(&self) -> Vec<&UiIntent> {
        match self {
            PanelReaction::Intent(i) => vec![i],
            PanelReaction::Intents(v) => v.iter().collect(),
            _ => Vec::new(),
        }
    }
}

/// `path` as a dialog answer: a chosen file, or `None` for Cancel.
pub type Chosen = Option<PathBuf>;

/// The answer to a [`DialogRequest::Confirm`].
pub fn on_confirmed(purpose: &ConfirmPurpose, yes: bool) -> PanelReaction {
    match (purpose, yes) {
        (ConfirmPurpose::DeleteControllerFile { intent }, true) => PanelReaction::Intent((**intent).clone()),
        _ => PanelReaction::Nothing,
    }
}

/// Whether `command` is on for the window of `document`: the view model's command states are those
/// of the active window, so another window's panel never invokes them.
pub fn command_on(vm: &burnburn_presentation::AppViewModel, document: DocumentId, command: burnburn_presentation::CommandKey) -> bool {
    vm.active == Some(document) && vm.commands.get(&command).is_some_and(|c| c.enabled)
}
