//! BurnBurn's own objects on an `NSPasteboard`, under [`CLIPBOARD_TYPE`]. The bytes are opaque
//! here; presentation validates them. Every function takes the pasteboard as a parameter, so the
//! tests use a private one and never touch the user's clipboard.

use objc2::rc::Retained;
use objc2_app_kit::NSPasteboard;
use objc2_foundation::{NSData, NSString};

use crate::pasteboard::CLIPBOARD_TYPE;

/// The user's general pasteboard (what the app uses).
pub fn general_pasteboard() -> Retained<NSPasteboard> {
    NSPasteboard::generalPasteboard()
}

/// A pasteboard with a unique private name (tests): not shared with any other process.
pub fn private_pasteboard() -> Retained<NSPasteboard> {
    NSPasteboard::pasteboardWithUniqueName()
}

fn clipboard_type() -> Retained<NSString> {
    NSString::from_str(CLIPBOARD_TYPE)
}

/// Replace the pasteboard's contents with `bytes` under BurnBurn's own type.
pub fn write_clipboard(pasteboard: &NSPasteboard, bytes: &[u8]) -> Result<(), String> {
    let accepted = super::objc_try("writing the pasteboard", || {
        pasteboard.clearContents();
        pasteboard.setData_forType(Some(&NSData::with_bytes(bytes)), &clipboard_type())
    })?;
    if accepted { Ok(()) } else { Err("the pasteboard did not accept the data".to_owned()) }
}

/// Whether the pasteboard holds BurnBurn's own type.
pub fn has_clipboard(pasteboard: &NSPasteboard) -> bool {
    let wanted = clipboard_type();
    super::objc_try("reading the pasteboard types", || {
        pasteboard.types().is_some_and(|types| types.iter().any(|t| t.isEqualToString(&wanted)))
    })
    .unwrap_or(false)
}

/// The bytes under BurnBurn's own type, if the pasteboard holds that type. A payload larger than
/// `max_bytes` is not copied out: it comes back empty, so presentation reports it as unreadable
/// instead of silently pasting an older copy.
pub fn read_clipboard_limited(pasteboard: &NSPasteboard, max_bytes: usize) -> Option<Vec<u8>> {
    if !has_clipboard(pasteboard) {
        return None;
    }
    let data = super::objc_try("reading the pasteboard", || pasteboard.dataForType(&clipboard_type())).ok()??;
    Some(if data.len() > max_bytes { Vec::new() } else { data.to_vec() })
}

/// [`read_clipboard_limited`] with the largest payload presentation parses at all.
pub fn read_clipboard(pasteboard: &NSPasteboard) -> Option<Vec<u8>> {
    read_clipboard_limited(pasteboard, burnburn_io::clipboard::MAX_CLIPBOARD_BYTES)
}
