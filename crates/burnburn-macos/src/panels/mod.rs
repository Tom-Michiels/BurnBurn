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
mod laser;
mod machine;
mod operations;
mod plan;
mod sketch;

pub use camera::*;
pub use cut_settings::*;
pub use import::*;
pub use laser::*;
pub use machine::*;
pub use operations::*;
pub use plan::*;
pub use sketch::*;

/// Segments of the toolbar's Schets control (momentary), left to right.
pub const SKETCH_TOOLBAR: [CommandKey; 4] =
    [CommandKey::SketchNew, CommandKey::SketchFromSelection, CommandKey::SketchEdit, CommandKey::SketchFinish];

/// The command for Schets toolbar segment `index`.
pub fn sketch_toolbar_command(index: isize) -> Option<CommandKey> {
    SKETCH_TOOLBAR.get(usize::try_from(index).ok()?).copied()
}

/// Enablement per Schets toolbar segment (same source as `validateMenuItem:`).
pub fn sketch_toolbar_enabled(vm: &AppViewModel) -> Vec<bool> {
    SKETCH_TOOLBAR.iter().map(|k| vm.commands.get(k).is_some_and(|c| c.enabled)).collect()
}

/// The commands of the sketch panel's button grid, row by row (relations, then Maat and Verwijder).
pub const SKETCH_BUTTONS: [CommandKey; 10] = [
    CommandKey::SketchHorizontal,
    CommandKey::SketchVertical,
    CommandKey::SketchCoincident,
    CommandKey::SketchParallel,
    CommandKey::SketchPerpendicular,
    CommandKey::SketchEqual,
    CommandKey::SketchTangent,
    CommandKey::SketchFix,
    CommandKey::SketchDimension,
    CommandKey::SketchRemoveConstraint,
];

/// Inspector fields, top to bottom.
pub const INSPECTOR_FIELDS: [FieldKey; 5] =
    [FieldKey::X, FieldKey::Y, FieldKey::Width, FieldKey::Height, FieldKey::Rotation];

/// Unit shown after an inspector field's caption.
pub fn field_unit(key: FieldKey, locale: Locale) -> &'static str {
    match key {
        FieldKey::Rotation => host_text(HostText::UnitDegrees, locale),
        _ => host_text(HostText::Unit, locale),
    }
}
