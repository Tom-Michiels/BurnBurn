//! The inspector's extra controls beside the nine fields: the 3×3 reference point, the aspect lock
//! and the flip buttons. They only forward clicks as intents and show the view model's state.

use burnburn_presentation::{AppViewModel, CommandKey, RefPoint, UiIntent};
use burnburn_types::DocumentId;

use crate::l10n::{HostText, Locale, host_text};

/// The flip buttons, left to right.
pub const INSPECTOR_FLIP_BUTTONS: [CommandKey; 2] = [CommandKey::FlipHorizontal, CommandKey::FlipVertical];

/// What the inspector's controls show for one window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InspectorControls {
    pub reference_point: RefPoint,
    pub aspect_locked: bool,
    /// Whether anything is selected (the fields and the controls act on the selection).
    pub enabled: bool,
    pub flip_enabled: [bool; 2],
}

pub fn inspector_controls(vm: &AppViewModel, document: DocumentId) -> InspectorControls {
    let doc = vm.document(document);
    let flip_enabled = INSPECTOR_FLIP_BUTTONS.map(|k| vm.commands.get(&k).is_some_and(|c| c.enabled));
    InspectorControls {
        reference_point: doc.map(|d| d.reference_point).unwrap_or_default(),
        aspect_locked: doc.is_some_and(|d| d.aspect_locked),
        enabled: doc.is_some_and(|d| d.inspector.is_some()),
        // The command table is the active window's: another window's flip buttons stay off.
        flip_enabled: if vm.active == Some(document) { flip_enabled } else { [false; 2] },
    }
}

/// The caption of a reference-point button (its accessibility label and tooltip).
pub fn reference_point_label(point: RefPoint, locale: Locale) -> &'static str {
    let text = match point {
        RefPoint::TopLeft => HostText::RefPointTopLeft,
        RefPoint::Top => HostText::RefPointTop,
        RefPoint::TopRight => HostText::RefPointTopRight,
        RefPoint::Left => HostText::RefPointLeft,
        RefPoint::Center => HostText::RefPointCenter,
        RefPoint::Right => HostText::RefPointRight,
        RefPoint::BottomLeft => HostText::RefPointBottomLeft,
        RefPoint::Bottom => HostText::RefPointBottom,
        RefPoint::BottomRight => HostText::RefPointBottomRight,
    };
    host_text(text, locale)
}

/// A click on reference-point button `index` (row by row, `RefPoint::ALL` order).
pub fn reference_point_intent(document: DocumentId, index: isize) -> Option<UiIntent> {
    let point = *RefPoint::ALL.get(usize::try_from(index).ok()?)?;
    Some(UiIntent::SetReferencePoint { document, point })
}

/// A click on the aspect-lock button: the opposite of what the window shows now. `None` for a
/// window the view model does not know (closed).
pub fn aspect_lock_intent(vm: &AppViewModel, document: DocumentId) -> Option<UiIntent> {
    let doc = vm.document(document)?;
    Some(UiIntent::SetAspectLock { document, locked: !doc.aspect_locked })
}

/// A click on flip button `index` (see [`INSPECTOR_FLIP_BUTTONS`]).
pub fn flip_intent(document: DocumentId, index: isize) -> Option<UiIntent> {
    let command = *INSPECTOR_FLIP_BUTTONS.get(usize::try_from(index).ok()?)?;
    Some(UiIntent::Invoke { document: Some(document), command })
}
