//! The inspector's transform block: the nine fields two per row, the 3×3 reference point, the
//! aspect-lock checkbox and the flip buttons. It only copies the view model into widgets; clicks
//! become `NativeEvent`s (`referencePoint:`, `aspectLock:`, `panelButton:`).

use burnburn_presentation::{AppViewModel, FieldKey, MessageKey, RefPoint};
use burnburn_types::DocumentId;
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{
    NSAccessibility, NSBezelStyle, NSButton, NSButtonType, NSControlStateValueOff, NSControlStateValueOn, NSFont,
    NSTextField,
};
use objc2_foundation::NSString;

use super::widgets::{Column, field, label, push_button, rect};
use crate::adapter::{MenuItem, menu_tag, message_text};
use crate::appkit::window::DocWindow;
use crate::l10n::{HostText, Locale, host_text, message_or_key};
/// Tag of the caption label of inspector field `n` is `CAPTION_TAG_BASE + n` (field `n` has tag `n`).
pub(crate) const CAPTION_TAG_BASE: isize = 5000;

use crate::panels::{INSPECTOR_FLIP_BUTTONS, INSPECTOR_ROWS, field_unit, inspector_controls, reference_point_label};

/// Side of one reference-point button.
const CELL: f64 = 20.0;
const FIELD_HEIGHT: f64 = 24.0;
const CAPTION_HEIGHT: f64 = 14.0;
/// Glyphs of the flip buttons (the accessibility label and tooltip are the command's name).
const FLIP_GLYPHS: [&str; 2] = ["\u{2194}", "\u{2195}"];

/// The controls beside the fields.
pub(crate) struct InspectorControlsWidgets {
    /// Row by row, `RefPoint::ALL` order; the tag is the index.
    pub(crate) reference: Vec<Retained<NSButton>>,
    pub(crate) aspect_lock: Retained<NSButton>,
    /// Flip horizontally, flip vertically (`INSPECTOR_FLIP_BUTTONS`).
    pub(crate) flip: Vec<Retained<NSButton>>,
}

/// Build the block from the column's current position down; returns the fields and the controls.
pub(crate) fn transform_block(
    col: &mut Column<'_>,
    locale: Locale,
) -> (Vec<(FieldKey, Retained<NSTextField>)>, InspectorControlsWidgets) {
    let mtm = col.mtm;
    let half = (col.width - 8.0) / 2.0;
    let small = NSFont::systemFontOfSize(11.0);
    let mut fields = Vec::new();
    let mut flip = Vec::new();
    for (left, right) in INSPECTOR_ROWS {
        col.y -= CAPTION_HEIGHT;
        let caption_y = col.y;
        col.y -= FIELD_HEIGHT;
        let field_y = col.y;
        for (slot, key) in [Some(left), right].into_iter().enumerate() {
            let Some(key) = key else { continue };
            let x = 12.0 + slot as f64 * (half + 8.0);
            let caption = format!("{} ({})", message_or_key(MessageKey::Field(key), locale), field_unit(key, locale));
            let l = label(mtm, &caption, rect(x, caption_y, half, CAPTION_HEIGHT));
            l.setFont(Some(&small));
            // The caption is found again by tag when the display unit changes (see `DocWindow::refresh`).
            l.setTag(CAPTION_TAG_BASE + fields.len() as isize);
            col.view.addSubview(&l);
            let f = field(mtm, rect(x, field_y, half, FIELD_HEIGHT));
            f.setTag(fields.len() as isize);
            f.setAccessibilityLabel(Some(&NSString::from_str(&caption)));
            col.view.addSubview(&f);
            fields.push((key, f));
        }
        // The last row has room beside Rotation: the two flip buttons.
        if right.is_none() {
            let width = (half - 4.0) / 2.0;
            for (i, command) in INSPECTOR_FLIP_BUTTONS.iter().enumerate() {
                let name = message_text(MessageKey::Command(*command), locale);
                let b = push_button(mtm, FLIP_GLYPHS[i]);
                b.setFrame(rect(12.0 + half + 8.0 + i as f64 * (width + 4.0), field_y, width, FIELD_HEIGHT));
                b.setTag(menu_tag(MenuItem::Command(*command)));
                b.setAccessibilityLabel(Some(&NSString::from_str(&name)));
                b.setToolTip(Some(&NSString::from_str(&name)));
                col.view.addSubview(&b);
                flip.push(b);
            }
        }
        col.y -= 6.0;
    }

    // Reference point (3×3) with the aspect-lock checkbox beside it.
    let top = col.y;
    col.y -= 3.0 * CELL;
    let mut reference = Vec::new();
    for (index, point) in RefPoint::ALL.iter().enumerate() {
        let caption = reference_point_label(*point, locale);
        let b = push_button(mtm, "");
        b.setBezelStyle(NSBezelStyle::SmallSquare);
        b.setButtonType(NSButtonType::PushOnPushOff);
        b.setFrame(rect(
            12.0 + f64::from(point.column()) * CELL,
            top - (f64::from(point.row()) + 1.0) * CELL,
            CELL,
            CELL,
        ));
        b.setTag(index as isize);
        b.setAccessibilityLabel(Some(&NSString::from_str(caption)));
        b.setToolTip(Some(&NSString::from_str(caption)));
        col.view.addSubview(&b);
        reference.push(b);
    }
    let beside = 12.0 + 3.0 * CELL + 12.0;
    let beside_width = col.width - 3.0 * CELL - 12.0;
    let heading = label(
        mtm,
        host_text(HostText::ReferencePoint, locale),
        rect(beside, top - CAPTION_HEIGHT, beside_width, CAPTION_HEIGHT),
    );
    heading.setFont(Some(&small));
    col.view.addSubview(&heading);
    let lock = lock_checkbox(mtm, host_text(HostText::AspectLock, locale));
    lock.setFrame(rect(beside, top - CAPTION_HEIGHT - 26.0, beside_width, 22.0));
    col.view.addSubview(&lock);
    col.y -= 8.0;
    (fields, InspectorControlsWidgets { reference, aspect_lock: lock, flip })
}

#[allow(unsafe_code)]
fn lock_checkbox(mtm: MainThreadMarker, title: &str) -> Retained<NSButton> {
    // SAFETY: no target and no action yet; both are set once the window controller exists.
    unsafe { NSButton::checkboxWithTitle_target_action(&NSString::from_str(title), None, None, mtm) }
}

impl DocWindow {
    /// Reference point, aspect lock and flip buttons follow the view model: off without a selection.
    pub(crate) fn refresh_inspector_controls(&self, vm: &AppViewModel, document: DocumentId) {
        let c = inspector_controls(vm, document);
        let state = |on: bool| if on { NSControlStateValueOn } else { NSControlStateValueOff };
        for (button, point) in self.controls.reference.iter().zip(RefPoint::ALL) {
            button.setEnabled(c.enabled);
            // Every refresh: a click toggles the button itself, the view model decides what shows.
            button.setState(state(point == c.reference_point));
        }
        self.controls.aspect_lock.setEnabled(c.enabled);
        self.controls.aspect_lock.setState(state(c.aspect_locked));
        for (button, enabled) in self.controls.flip.iter().zip(c.flip_enabled) {
            button.setEnabled(enabled);
        }
    }
}
