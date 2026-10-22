//! The remnant library as the user sees it (G7-werk, W07): leftover sheets with material, thickness,
//! contour and the zones that stay out of use, kept between sessions. Saving takes the selection
//! (the largest closed outline is the sheet, every other closed outline is a zone), placing draws a
//! remnant in the window so a nest can use its contour as the sheet, and an order can name the
//! remnant it is made from. Linking changes no count: an order's number only follows recorded
//! executions.

use burnburn_types::DocumentId;

/// What the remnant panel asks for (`UiIntent::Remnant`). Numbers arrive as typed (DR-05).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemnantIntent {
    /// Keep the selection as a remnant. `date` is `YYYY-MM-DD`; empty takes today from the job clock.
    SaveFromSelection {
        document: DocumentId,
        name: String,
        material: String,
        thickness: String,
        note: String,
        date: String,
    },
    Remove {
        id: u64,
    },
    /// Draw remnant `id` in the window, its centre on the centre of the view: the contour and one
    /// closed object per zone.
    Place {
        document: DocumentId,
        id: u64,
    },
}

/// Why a remnant intent did nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum RemnantProblem {
    /// The library file has not been read yet (try again in a moment).
    NotReady,
    /// The library file could not be read: it is kept as it is and nothing is written.
    Unreadable,
    NeedsSelection,
    /// The selection holds no closed outline.
    NoClosedOutline,
    NeedsName,
    NeedsMaterial,
    InvalidThickness,
    InvalidDate,
    NameTaken,
    /// A closed outline lies (partly) outside the sheet, so it is no zone of it.
    ZoneOutsideSheet,
    /// The sheet or a zone crosses itself, has no area or is too detailed.
    NotASheet,
    Unknown,
    /// The library could not be written.
    NotSaved,
}

/// One remnant of the library.
#[derive(Debug, Clone, PartialEq)]
pub struct RemnantRow {
    pub id: u64,
    pub name: String,
    pub material: String,
    pub thickness_mm: f64,
    pub width_mm: f64,
    pub height_mm: f64,
    /// The area of the sheet without its zones, in cm².
    pub usable_cm2: f64,
    pub zones: usize,
    pub note: String,
    pub date: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct RemnantsViewModel {
    pub rows: Vec<RemnantRow>,
    /// Why the last remnant intent did nothing.
    pub problem: Option<RemnantProblem>,
}

/// How the remnant an order names compares with the job it is made for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemnantLinkStatus {
    /// The remnant is in the library; `material_matches` is `None` when the job's material is not known.
    Known { material: String, thickness_um: u32, material_matches: Option<bool> },
    /// The library has no remnant of that name (any more).
    Missing,
}
