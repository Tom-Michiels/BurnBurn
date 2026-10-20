//! The cut-order priority row of the inspector (W3-UI-cam, C06): a field below the transform
//! block with a line that says when it applies. It joins the inspector's field list, so the
//! window copies its value and sends its commits like the other fields (`CommitField`).

use burnburn_presentation::{FieldKey, MessageKey};
use objc2::rc::Retained;
use objc2_app_kit::{NSAccessibility, NSColor, NSFont, NSLineBreakMode, NSTextField};
use objc2_foundation::NSString;

use super::widgets::{Column, field, label, rect};
use crate::l10n::{Locale, message_or_key};
use crate::panels::cam_priority_hint;

const CAPTION_HEIGHT: f64 = 14.0;
const FIELD_HEIGHT: f64 = 24.0;
const HINT_HEIGHT: f64 = 28.0;

/// Adds the row from the column's current position down; returns the field for the window's list.
pub(crate) fn priority_row(col: &mut Column<'_>, locale: Locale) -> (FieldKey, Retained<NSTextField>) {
    let mtm = col.mtm;
    let caption_text = message_or_key(MessageKey::Field(FieldKey::Priority), locale);
    col.y -= CAPTION_HEIGHT;
    let caption = label(mtm, &caption_text, rect(12.0, col.y, col.width, CAPTION_HEIGHT));
    caption.setFont(Some(&NSFont::systemFontOfSize(11.0)));
    col.view.addSubview(&caption);
    col.y -= FIELD_HEIGHT;
    let f = field(mtm, rect(12.0, col.y, col.width, FIELD_HEIGHT));
    f.setAccessibilityLabel(Some(&NSString::from_str(&caption_text)));
    f.setToolTip(Some(&NSString::from_str(cam_priority_hint(locale))));
    col.view.addSubview(&f);
    col.y -= HINT_HEIGHT;
    let hint = label(mtm, cam_priority_hint(locale), rect(12.0, col.y, col.width, HINT_HEIGHT));
    hint.setFont(Some(&NSFont::systemFontOfSize(10.0)));
    hint.setTextColor(Some(&NSColor::secondaryLabelColor()));
    hint.setLineBreakMode(NSLineBreakMode::ByWordWrapping);
    col.view.addSubview(&hint);
    col.y -= 6.0;
    (FieldKey::Priority, f)
}
