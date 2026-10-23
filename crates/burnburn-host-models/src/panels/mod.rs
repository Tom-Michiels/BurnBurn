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

mod cam;
mod cam_form;
mod camera;
mod camera_workspace;
mod cut_settings;
mod g5;
mod g5_form;
mod g6;
mod g6_werk;
mod g7_editor;
mod g7_trace;
mod g7_werk;
mod grbl;
mod import;
mod inspector;
mod laser;
mod machine;
mod operations;
mod plan;
mod schikken;
mod shapes;
mod sketch;
mod text;

pub use cam::*;
pub use cam_form::{CamForm, CamTarget, CaptionParts, arrow_label, cam_form, split_caption};
pub use camera::*;
pub use camera_workspace::*;
pub use cut_settings::*;
pub use g5::*;
pub use g5_form::{G5Form, dialog_role, g5_form};
pub use g7_editor::EditorControl;
pub use g7_trace::{batch_files_intent, compare_bitmap};
pub use grbl::*;
pub use import::*;
pub use inspector::*;
pub use laser::*;
pub use machine::*;
pub use operations::*;
pub use plan::*;
pub use schikken::*;
pub use shapes::*;
pub use sketch::*;
pub use text::*;

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

/// Segments of the toolbar's Uitlijnen control (momentary), left to right: align, distribute, then
/// dock and move together (W3-UI-schikken).
pub const ARRANGE_TOOLBAR: [CommandKey; 14] = [
    CommandKey::AlignLeft,
    CommandKey::AlignCenterX,
    CommandKey::AlignRight,
    CommandKey::AlignTop,
    CommandKey::AlignCenterY,
    CommandKey::AlignBottom,
    CommandKey::DistributeHorizontal,
    CommandKey::DistributeVertical,
    CommandKey::DockLeft,
    CommandKey::DockRight,
    CommandKey::DockUp,
    CommandKey::DockDown,
    CommandKey::MoveTogetherHorizontal,
    CommandKey::MoveTogetherVertical,
];

/// The command for Uitlijnen toolbar segment `index`.
pub fn arrange_toolbar_command(index: isize) -> Option<CommandKey> {
    ARRANGE_TOOLBAR.get(usize::try_from(index).ok()?).copied()
}

/// Enablement per Uitlijnen toolbar segment (same source as `validateMenuItem:`).
pub fn arrange_toolbar_enabled(vm: &AppViewModel) -> Vec<bool> {
    ARRANGE_TOOLBAR.iter().map(|k| vm.commands.get(k).is_some_and(|c| c.enabled)).collect()
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

/// Inspector fields, top to bottom. Rotation, scale and shear are actions: they show their
/// neutral value (0, 100, 0) and a commit applies the typed amount.
pub const INSPECTOR_FIELDS: [FieldKey; 9] = [
    FieldKey::X,
    FieldKey::Y,
    FieldKey::Width,
    FieldKey::Height,
    FieldKey::Rotation,
    FieldKey::ScaleX,
    FieldKey::ScaleY,
    FieldKey::ShearX,
    FieldKey::ShearY,
];

/// The inspector's field rows, top to bottom: two fields side by side (the last row has one).
pub const INSPECTOR_ROWS: [(FieldKey, Option<FieldKey>); 5] = [
    (FieldKey::X, Some(FieldKey::Y)),
    (FieldKey::Width, Some(FieldKey::Height)),
    (FieldKey::ScaleX, Some(FieldKey::ScaleY)),
    (FieldKey::ShearX, Some(FieldKey::ShearY)),
    (FieldKey::Rotation, None),
];

/// The unit after an inspector caption when a length field is shown in `shown` (X03): lengths
/// follow the display-unit preference, angles and percentages never do.
pub fn field_unit_for(key: FieldKey, shown: burnburn_types::Unit, locale: Locale) -> &'static str {
    match key {
        FieldKey::X | FieldKey::Y | FieldKey::Width | FieldKey::Height => match shown {
            burnburn_types::Unit::Millimeter => crate::app_text::app_text(crate::app_text::AppText::SuffixMm, locale),
            burnburn_types::Unit::Inch => crate::app_text::app_text(crate::app_text::AppText::SuffixInch, locale),
        },
        other => field_unit(other, locale),
    }
}

/// Unit shown after an inspector field's caption.
pub fn field_unit(key: FieldKey, locale: Locale) -> &'static str {
    match key {
        FieldKey::Rotation | FieldKey::ShearX | FieldKey::ShearY => host_text(HostText::UnitDegrees, locale),
        FieldKey::ScaleX | FieldKey::ScaleY => host_text(HostText::UnitPercent, locale),
        _ => host_text(HostText::Unit, locale),
    }
}
