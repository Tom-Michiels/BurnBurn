//! The cut-order priority row of the inspector (W3-UI-cam, C06): a caption and a field below the
//! transform block with one line that says when it applies (the tooltip has the full sentence). It
//! joins the inspector's field list, so the window copies its value and sends its commits like the
//! other fields (`CommitField`).

use burnburn_presentation::{FieldKey, MessageKey};
use objc2::rc::Retained;
use objc2_app_kit::{NSAccessibility, NSColor, NSFont, NSLineBreakMode, NSTextField};
use objc2_foundation::NSString;

use super::widgets::{Column, field, label, rect};
use crate::l10n::{Locale, message_or_key};
use crate::panels::{cam_priority_hint, cam_priority_hint_short};

const CAPTION_WIDTH: f64 = 86.0;
const FIELD_HEIGHT: f64 = 24.0;
const HINT_HEIGHT: f64 = 13.0;
/// View tag of the priority field. The transform fields use their index as tag and the window
/// finds their unit captions by tag; the default tag 0 would make it rename the X caption.
const PRIORITY_FIELD_TAG: isize = 900;

/// Adds the row from the column's current position down; returns the field for the window's list.
pub(crate) fn priority_row(col: &mut Column<'_>, locale: Locale) -> (FieldKey, Retained<NSTextField>) {
    let mtm = col.mtm;
    let caption_text = message_or_key(MessageKey::Field(FieldKey::Priority), locale);
    col.y -= FIELD_HEIGHT;
    let caption = label(mtm, &caption_text, rect(12.0, col.y, CAPTION_WIDTH, 18.0));
    caption.setFont(Some(&NSFont::systemFontOfSize(11.0)));
    col.view.addSubview(&caption);
    let f = field(mtm, rect(12.0 + CAPTION_WIDTH + 6.0, col.y, col.width - CAPTION_WIDTH - 6.0, FIELD_HEIGHT));
    f.setTag(PRIORITY_FIELD_TAG);
    f.setAccessibilityLabel(Some(&NSString::from_str(&caption_text)));
    f.setToolTip(Some(&NSString::from_str(cam_priority_hint(locale))));
    col.view.addSubview(&f);
    col.y -= HINT_HEIGHT;
    let hint = label(mtm, cam_priority_hint_short(locale), rect(12.0, col.y, col.width, HINT_HEIGHT));
    hint.setFont(Some(&NSFont::systemFontOfSize(10.0)));
    hint.setTextColor(Some(&NSColor::secondaryLabelColor()));
    hint.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
    hint.setToolTip(Some(&NSString::from_str(cam_priority_hint(locale))));
    col.view.addSubview(&hint);
    col.y -= 6.0;
    (FieldKey::Priority, f)
}
