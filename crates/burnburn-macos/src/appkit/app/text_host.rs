//! Host side of the text panels (W3-UI-tekst): the job clock for `{date}` in variable text and the
//! CSV file panel of the variable-text panel. A child module of `app`, so it can use the host's
//! private state.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use burnburn_presentation::JobClock;
use burnburn_types::DocumentId;
use objc2_foundation::NSTimeZone;

use super::HostApp;
use crate::adapter::NativeEvent;
use crate::appkit::panels::image::show_image_open_panel;
use crate::panels::{Slot, SlotValue};

/// UTC seconds now and the local zone's offset from UTC at this moment (the zone is fixed in the
/// record, so a job prepared in summer prints the same hour in winter).
pub(super) fn host_clock() -> Option<JobClock> {
    let seconds = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0));
    let offset_minutes = i32::try_from(NSTimeZone::localTimeZone().secondsFromGMT() / 60).unwrap_or(0);
    // A zone the format cannot hold (never in practice) falls back to UTC; no clock, no dates.
    JobClock::new(seconds, offset_minutes).or_else(|_| JobClock::new(seconds, 0)).ok()
}

impl HostApp {
    /// The variable-text panel's "Kies CSV…": a file sheet on the document's window. The host reads
    /// the file (a few kilobytes of text); presentation parses and checks it. A file that cannot
    /// be read arrives as empty bytes, which presentation refuses as an unreadable CSV.
    pub(crate) fn show_csv_panel(&self, document: DocumentId) {
        let Some(window) = self.window(document) else { return };
        show_image_open_panel(
            self.mtm,
            &window,
            self.me.clone(),
            &["csv", "tsv", "txt"],
            "csv panel completion",
            move |path: Option<PathBuf>| {
                let file = path.map(|p| {
                    let name =
                        p.file_name().map_or_else(|| p.display().to_string(), |n| n.to_string_lossy().into_owned());
                    (name, std::fs::read(&p).unwrap_or_default())
                });
                NativeEvent::TextPanel { document, slot: Slot::VarCsv, value: SlotValue::Csv(file) }
            },
        );
    }
}

/// The text panels' widgets and whether each is on screen, for tests that drive them like a user.
pub struct TextProbe<'a> {
    pub window: &'a objc2_app_kit::NSWindow,
    pub text: crate::appkit::panels::text::TextWidgets<'a>,
    pub variable: crate::appkit::panels::variable::VariableWidgets<'a>,
    pub barcode: crate::appkit::panels::barcode::BarcodeWidgets<'a>,
    pub toolbar_shown: bool,
    pub path_sheet_shown: bool,
    pub variable_shown: bool,
    pub barcode_shown: bool,
}

impl HostApp {
    pub fn with_text_widgets<T>(&self, document: DocumentId, f: impl FnOnce(&TextProbe<'_>) -> T) -> Option<T> {
        let windows = self.windows.try_borrow().ok()?;
        let w = windows.get(&document)?;
        Some(f(&TextProbe {
            window: &w.window,
            text: w.text.widgets(),
            variable: w.variable.widgets(),
            barcode: w.barcode.widgets(),
            toolbar_shown: w.text.toolbar_visible(),
            path_sheet_shown: w.text.path_visible(),
            variable_shown: w.variable.visible(),
            barcode_shown: w.barcode.visible(),
        }))
    }
}
