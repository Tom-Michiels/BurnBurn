//! What the window's panels show, from the existing presentation view models (no AppKit): the
//! toolbar's tool control, the inspector's field order and the JobPlan preview panel.
//!
//! Panels only show and forward: tools and buttons become `UiIntent::Invoke`, field text goes to
//! presentation unparsed (DR-05), also in the operations panel; the bed-camera section only asks
//! for access or starts the camera on an explicit click.
//!
//! One file per panel (`plan`, `import`, `machine`, `operations`, `cut_settings`, `camera`); this file keeps
//! the toolbar and inspector-field bits. Everything is re-exported, so `panels::X` paths are unchanged.

use burnburn_presentation::{AppViewModel, CommandKey, FieldKey};

use crate::l10n::{HostText, Locale, host_text};

mod camera;
mod cut_settings;
mod import;
mod machine;
mod operations;
mod plan;

pub use camera::*;
pub use cut_settings::*;
pub use import::*;
pub use machine::*;
pub use operations::*;
pub use plan::*;

/// Segments of the toolbar's tool control, left to right.
pub const TOOLBAR_TOOLS: [CommandKey; 6] = [
    CommandKey::ToolSelect,
    CommandKey::ToolRectangle,
    CommandKey::ToolEllipse,
    CommandKey::ToolLine,
    CommandKey::ToolPen,
    CommandKey::ToolNode,
];

/// Inspector fields, top to bottom.
pub const INSPECTOR_FIELDS: [FieldKey; 5] =
    [FieldKey::X, FieldKey::Y, FieldKey::Width, FieldKey::Height, FieldKey::Rotation];

/// The tool command for toolbar segment `index` (AppKit's `selectedSegment`, −1 = none).
pub fn toolbar_tool(index: isize) -> Option<CommandKey> {
    TOOLBAR_TOOLS.get(usize::try_from(index).ok()?).copied()
}

/// The segment of the active tool (from the commands' checked state, like the menu checkmarks).
pub fn toolbar_selected(vm: &AppViewModel) -> Option<usize> {
    TOOLBAR_TOOLS.iter().position(|k| vm.commands.get(k).is_some_and(|c| c.checked))
}

/// Enablement per segment (same source as `validateMenuItem:`).
pub fn toolbar_enabled(vm: &AppViewModel) -> Vec<bool> {
    TOOLBAR_TOOLS.iter().map(|k| vm.commands.get(k).is_some_and(|c| c.enabled)).collect()
}

/// Unit shown after an inspector field's caption.
pub fn field_unit(key: FieldKey, locale: Locale) -> &'static str {
    match key {
        FieldKey::Rotation => host_text(HostText::UnitDegrees, locale),
        _ => host_text(HostText::Unit, locale),
    }
}
