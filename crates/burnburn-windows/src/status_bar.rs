//! The status bar of the document window (LF14): four parts, the status with its symbol, the
//! pointer coordinate, the zoom and the unit. The texts come from the shared model
//! (`panels::status_bar`, `status_line`); this file only says which part holds what and where the
//! part edges lie for `SB_SETPARTS`. The `msctls_statusbar32` control is in `win32`.

use crate::forms::spec_layout::{DluBase, TextMeter};
use crate::l10n::Locale;
use crate::panels::{StatusBar, status_bar_labels};
use crate::status_line::StatusLine;
use crate::style::StatusTone;

/// What a part holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusPartKind {
    Status,
    Coordinate,
    Zoom,
    Unit,
}

/// One part of the bar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusPart {
    pub kind: StatusPartKind,
    pub text: String,
    /// What a screen reader calls the part.
    pub name: &'static str,
}

/// The parts, left to right, and the tone of the first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusParts {
    pub parts: Vec<StatusPart>,
    pub tone: StatusTone,
}

/// The four parts for a status line and the bar texts.
pub fn status_parts(line: &StatusLine, bar: &StatusBar, locale: Locale) -> StatusParts {
    let (coordinate, zoom, unit) = status_bar_labels(locale);
    let part = |kind, text: &str, name| StatusPart { kind, text: text.to_owned(), name };
    StatusParts {
        parts: vec![
            part(StatusPartKind::Status, &line.text, "Status"),
            part(StatusPartKind::Coordinate, &bar.coordinate, coordinate),
            part(StatusPartKind::Zoom, &bar.zoom, zoom),
            part(StatusPartKind::Unit, &bar.unit, unit),
        ],
        tone: line.tone,
    }
}

/// The widest text each fixed part ever shows, so its width does not change with the text.
const WIDEST_COORDINATE: &str = "X -9999,999  Y -9999,999";
const WIDEST_ZOOM: &str = "9999,9 %";
const WIDEST_UNIT: &str = "mm";
/// Padding of a part and the least width of the status and the coordinate, in dialog units.
const PART_PAD: f64 = 8.0;
const MIN_STATUS: f64 = 60.0;
const MIN_COORDINATE: f64 = 50.0;

/// The right edges of the parts for a bar `width` pixels wide, as `SB_SETPARTS` takes them: the
/// last is -1 (to the end). The coordinate, zoom and unit parts keep a width that does not change
/// with their text, so the bar does not jump while the pointer moves; the status takes the rest.
pub fn part_edges(width: i32, base: DluBase, meter: &dyn TextMeter) -> Vec<i32> {
    let pad = base.x(PART_PAD);
    let zoom = meter.width(WIDEST_ZOOM, false) + pad;
    let unit = meter.width(WIDEST_UNIT, false) + pad;
    let room_for_coordinate = width - base.x(MIN_STATUS) - zoom - unit;
    let coordinate = (meter.width(WIDEST_COORDINATE, false) + pad).min(room_for_coordinate).max(base.x(MIN_COORDINATE));
    let zoom_end = (width - unit).max(base.x(MIN_STATUS) + coordinate + zoom);
    let coordinate_end = zoom_end - zoom;
    let status_end = coordinate_end - coordinate;
    vec![status_end, coordinate_end, zoom_end, -1]
}
