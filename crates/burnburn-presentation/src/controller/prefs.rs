//! Preferences window model (X02/X03): units, language, autosave interval, snapping defaults,
//! nudge distances and display quality. Stored in the user's folder through the injected
//! filesystem, never in a project. Every value is typed text parsed here (DR-05).

use std::collections::BTreeMap;

use burnburn_editor::SnapSettings;

use burnburn_types::Length;

use crate::field::{FieldError, format_mm, parse_length, parse_number, parse_position};

/// Length unit of the display and of bare numbers typed in length fields. The physical value
/// is always kept in millimetres.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LengthUnit {
    #[default]
    Millimeters,
    Inches,
}

/// UI language: follow the system or fix one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LanguageChoice {
    #[default]
    System,
    Dutch,
    English,
}

/// Curve flattening quality of the canvas drawing (never of a compiled job).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DisplayQuality {
    Coarse,
    #[default]
    Smooth,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Preferences {
    pub units: LengthUnit,
    pub language: LanguageChoice,
    /// 0 = autosave off.
    pub autosave_seconds: u32,
    pub snap_to_objects: bool,
    pub snap_to_grid: bool,
    pub grid_snap_mm: f64,
    pub object_snap_px: f64,
    pub nudge_small_mm: f64,
    pub nudge_normal_mm: f64,
    pub nudge_large_mm: f64,
    pub quality: DisplayQuality,
}

impl Default for Preferences {
    fn default() -> Self {
        Preferences {
            units: LengthUnit::Millimeters,
            language: LanguageChoice::System,
            autosave_seconds: 30,
            snap_to_objects: true,
            snap_to_grid: false,
            grid_snap_mm: 1.0,
            object_snap_px: 8.0,
            nudge_small_mm: 0.1,
            nudge_normal_mm: 1.0,
            nudge_large_mm: 10.0,
            quality: DisplayQuality::Smooth,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PrefKey {
    Units,
    Language,
    AutosaveSeconds,
    SnapToObjects,
    SnapToGrid,
    GridSnap,
    ObjectSnapPixels,
    NudgeSmall,
    NudgeNormal,
    NudgeLarge,
    Quality,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrefUnit {
    /// Shown in the display unit.
    Length,
    Pixels,
    Seconds,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrefValue {
    Choice { selected: &'static str, options: &'static [&'static str] },
    Flag(bool),
    Number { text: String, unit: PrefUnit },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrefRow {
    pub key: PrefKey,
    pub value: PrefValue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrefProblem {
    NotANumber,
    OutOfRange,
    UnknownChoice,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrefError {
    pub key: PrefKey,
    pub problem: PrefProblem,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PrefsViewModel {
    pub rows: Vec<PrefRow>,
    pub unit: LengthUnit,
    pub error: Option<PrefError>,
    /// The stored file could not be read; defaults are in use and the file is left alone.
    pub load_problem: bool,
    /// The last write did not succeed; the running app still uses the chosen values.
    pub save_failed: bool,
}

impl PrefsViewModel {
    pub fn row(&self, key: PrefKey) -> Option<&PrefRow> {
        self.rows.iter().find(|r| r.key == key)
    }
}

const MM_PER_INCH: f64 = 25.4;
/// Smallest and largest length a preference may hold (mm).
const LENGTH_RANGE_MM: (f64, f64) = (0.001, 1000.0);
const PIXEL_RANGE: (f64, f64) = (1.0, 100.0);
/// Autosave: off (0) or between ten seconds and an hour.
const AUTOSAVE_RANGE_S: (u32, u32) = (10, 3600);

const UNIT_IDS: &[&str] = &["mm", "inch"];
const LANGUAGE_IDS: &[&str] = &["system", "nl", "en"];
const QUALITY_IDS: &[&str] = &["coarse", "smooth"];

impl PrefKey {
    pub const ALL: [PrefKey; 11] = [
        PrefKey::Units,
        PrefKey::Language,
        PrefKey::AutosaveSeconds,
        PrefKey::SnapToObjects,
        PrefKey::SnapToGrid,
        PrefKey::GridSnap,
        PrefKey::ObjectSnapPixels,
        PrefKey::NudgeSmall,
        PrefKey::NudgeNormal,
        PrefKey::NudgeLarge,
        PrefKey::Quality,
    ];

    /// Stable id: the key in the user file.
    pub fn id(self) -> &'static str {
        match self {
            PrefKey::Units => "units",
            PrefKey::Language => "language",
            PrefKey::AutosaveSeconds => "autosave_seconds",
            PrefKey::SnapToObjects => "snap_to_objects",
            PrefKey::SnapToGrid => "snap_to_grid",
            PrefKey::GridSnap => "grid_snap_mm",
            PrefKey::ObjectSnapPixels => "object_snap_px",
            PrefKey::NudgeSmall => "nudge_small_mm",
            PrefKey::NudgeNormal => "nudge_normal_mm",
            PrefKey::NudgeLarge => "nudge_large_mm",
            PrefKey::Quality => "quality",
        }
    }
}

impl LengthUnit {
    pub fn id(self) -> &'static str {
        match self {
            LengthUnit::Millimeters => "mm",
            LengthUnit::Inches => "inch",
        }
    }

    pub fn mm_per_unit(self) -> f64 {
        match self {
            LengthUnit::Millimeters => 1.0,
            LengthUnit::Inches => MM_PER_INCH,
        }
    }
}

impl LanguageChoice {
    pub fn id(self) -> &'static str {
        match self {
            LanguageChoice::System => "system",
            LanguageChoice::Dutch => "nl",
            LanguageChoice::English => "en",
        }
    }
}

impl DisplayQuality {
    /// Curve flattening of the canvas drawing in device pixels (never a job tolerance).
    pub fn tolerance_px(self) -> f64 {
        match self {
            DisplayQuality::Coarse => 1.0,
            DisplayQuality::Smooth => 0.25,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            DisplayQuality::Coarse => "coarse",
            DisplayQuality::Smooth => "smooth",
        }
    }
}

fn flag_id(on: bool) -> &'static str {
    if on { "on" } else { "off" }
}

/// Length text as shown for a value in millimetres: the display unit, at most three decimals.
pub fn format_length(mm: f64, unit: LengthUnit) -> String {
    format_mm(mm / unit.mm_per_unit())
}

/// A length typed in a preference: a unit suffix (`mm`, `cm`, `in`, `"`) always wins; a bare
/// number is in the display unit. Zero and negative lengths are out of range, not "not a number".
pub fn parse_length_in(text: &str, default: LengthUnit) -> Result<f64, PrefProblem> {
    let mm = parse_length(text).map_err(problem_of)?.mm();
    Ok(if has_unit_suffix(text) { mm } else { mm * default.mm_per_unit() })
}

/// [`parse_length`] where a bare number is in `unit` (an explicit unit suffix always wins).
pub(super) fn parse_length_in_unit(text: &str, unit: LengthUnit) -> Result<Length, FieldError> {
    let mm = parse_length(text)?.mm();
    Length::from_mm(if has_unit_suffix(text) { mm } else { mm * unit.mm_per_unit() }).map_err(|_| FieldError::NotFinite)
}

/// [`parse_position`] where a bare number is in `unit`.
pub(super) fn parse_position_in_unit(text: &str, unit: LengthUnit) -> Result<Length, FieldError> {
    let mm = parse_position(text)?.mm();
    Length::from_mm(if has_unit_suffix(text) { mm } else { mm * unit.mm_per_unit() }).map_err(|_| FieldError::NotFinite)
}

fn has_unit_suffix(text: &str) -> bool {
    let lower = text.trim().to_ascii_lowercase();
    ["mm", "cm", "in", "\""].iter().any(|s| lower.ends_with(s))
}

fn problem_of(e: FieldError) -> PrefProblem {
    match e {
        FieldError::Empty | FieldError::NotANumber | FieldError::AmbiguousSeparator => PrefProblem::NotANumber,
        FieldError::NotFinite | FieldError::NotPositive => PrefProblem::OutOfRange,
    }
}

fn length_value(text: &str, unit: LengthUnit) -> Result<f64, PrefProblem> {
    let mm = parse_length_in(text, unit)?;
    if (LENGTH_RANGE_MM.0..=LENGTH_RANGE_MM.1).contains(&mm) { Ok(mm) } else { Err(PrefProblem::OutOfRange) }
}

fn choice<T: Copy>(text: &str, table: &[(&str, T)]) -> Result<T, PrefProblem> {
    let t = text.trim().to_ascii_lowercase();
    table.iter().find(|(id, _)| *id == t).map(|(_, v)| *v).ok_or(PrefProblem::UnknownChoice)
}

fn flag(text: &str) -> Result<bool, PrefProblem> {
    choice(text, &[("on", true), ("off", false)])
}

impl Preferences {
    /// `self` with `key` set from the typed `text`, or why not. Nothing is changed on failure.
    pub fn with(&self, key: PrefKey, text: &str) -> Result<Preferences, PrefProblem> {
        let mut next = *self;
        match key {
            PrefKey::Units => {
                next.units = choice(text, &[("mm", LengthUnit::Millimeters), ("inch", LengthUnit::Inches)])?
            }
            PrefKey::Language => {
                next.language = choice(
                    text,
                    &[
                        ("system", LanguageChoice::System),
                        ("nl", LanguageChoice::Dutch),
                        ("en", LanguageChoice::English),
                    ],
                )?
            }
            PrefKey::Quality => {
                next.quality = choice(text, &[("coarse", DisplayQuality::Coarse), ("smooth", DisplayQuality::Smooth)])?
            }
            PrefKey::SnapToObjects => next.snap_to_objects = flag(text)?,
            PrefKey::SnapToGrid => next.snap_to_grid = flag(text)?,
            PrefKey::AutosaveSeconds => {
                let t = text.trim();
                if t.is_empty() || !t.chars().all(|c| c.is_ascii_digit()) {
                    return Err(PrefProblem::NotANumber);
                }
                let seconds: u32 = t.parse().map_err(|_| PrefProblem::OutOfRange)?;
                if seconds != 0 && !(AUTOSAVE_RANGE_S.0..=AUTOSAVE_RANGE_S.1).contains(&seconds) {
                    return Err(PrefProblem::OutOfRange);
                }
                next.autosave_seconds = seconds;
            }
            PrefKey::GridSnap => next.grid_snap_mm = length_value(text, self.units)?,
            PrefKey::NudgeSmall => next.nudge_small_mm = length_value(text, self.units)?,
            PrefKey::NudgeNormal => next.nudge_normal_mm = length_value(text, self.units)?,
            PrefKey::NudgeLarge => next.nudge_large_mm = length_value(text, self.units)?,
            PrefKey::ObjectSnapPixels => {
                let px = parse_number(text, &["px"]).map_err(problem_of)?;
                if !(PIXEL_RANGE.0..=PIXEL_RANGE.1).contains(&px) {
                    return Err(PrefProblem::OutOfRange);
                }
                next.object_snap_px = px;
            }
        }
        Ok(next)
    }

    /// The text a preference has in the user file (and in the window, for choices and flags).
    pub(super) fn stored(&self, key: PrefKey) -> String {
        match key {
            PrefKey::Units => self.units.id().to_owned(),
            PrefKey::Language => self.language.id().to_owned(),
            PrefKey::Quality => self.quality.id().to_owned(),
            PrefKey::SnapToObjects => flag_id(self.snap_to_objects).to_owned(),
            PrefKey::SnapToGrid => flag_id(self.snap_to_grid).to_owned(),
            PrefKey::AutosaveSeconds => self.autosave_seconds.to_string(),
            PrefKey::GridSnap => format!("{}", self.grid_snap_mm),
            PrefKey::NudgeSmall => format!("{}", self.nudge_small_mm),
            PrefKey::NudgeNormal => format!("{}", self.nudge_normal_mm),
            PrefKey::NudgeLarge => format!("{}", self.nudge_large_mm),
            PrefKey::ObjectSnapPixels => format!("{}", self.object_snap_px),
        }
    }

    /// One row of the window for `key`.
    pub fn row(&self, key: PrefKey) -> PrefRow {
        let number = |text: String, unit| PrefValue::Number { text, unit };
        let value = match key {
            PrefKey::Units => PrefValue::Choice { selected: self.units.id(), options: UNIT_IDS },
            PrefKey::Language => PrefValue::Choice { selected: self.language.id(), options: LANGUAGE_IDS },
            PrefKey::Quality => PrefValue::Choice { selected: self.quality.id(), options: QUALITY_IDS },
            PrefKey::SnapToObjects => PrefValue::Flag(self.snap_to_objects),
            PrefKey::SnapToGrid => PrefValue::Flag(self.snap_to_grid),
            PrefKey::AutosaveSeconds => number(self.autosave_seconds.to_string(), PrefUnit::Seconds),
            PrefKey::GridSnap => number(format_length(self.grid_snap_mm, self.units), PrefUnit::Length),
            PrefKey::NudgeSmall => number(format_length(self.nudge_small_mm, self.units), PrefUnit::Length),
            PrefKey::NudgeNormal => number(format_length(self.nudge_normal_mm, self.units), PrefUnit::Length),
            PrefKey::NudgeLarge => number(format_length(self.nudge_large_mm, self.units), PrefUnit::Length),
            PrefKey::ObjectSnapPixels => number(format_mm(self.object_snap_px), PrefUnit::Pixels),
        };
        PrefRow { key, value }
    }

    /// The snapping a new window starts with.
    pub fn snap_settings(&self) -> SnapSettings {
        SnapSettings {
            objects: self.snap_to_objects,
            grid_mm: self.snap_to_grid.then_some(self.grid_snap_mm),
            radius_pt: self.object_snap_px,
        }
    }
}

/// First line of the user file; the number is the format version.
const HEADER: &str = "burnburn-preferences";
const FORMAT_VERSION: u32 = 1;
/// Prefix of a shortcut override in the user file.
pub(super) const SHORTCUT_PREFIX: &str = "shortcut.";

/// Everything the user file holds. Keys this version does not know are kept verbatim, so a
/// downgrade-and-back (or a newer build's setting) is not thrown away by a rewrite.
#[derive(Debug, Clone, PartialEq, Default)]
pub(super) struct UserSettings {
    pub(super) prefs: Preferences,
    /// Shortcut overrides: command id → binding text, or `none` for "no shortcut".
    pub(super) shortcuts: BTreeMap<String, String>,
    pub(super) unknown: Vec<String>,
}

/// What reading the user file produced.
pub(super) struct Loaded {
    pub(super) settings: UserSettings,
    /// The file is not a readable preferences file (or holds invalid values): the affected
    /// values are defaults and the file stays as it was until the user changes something.
    pub(super) damaged: bool,
}

impl UserSettings {
    pub(super) fn parse(bytes: &[u8]) -> Loaded {
        let mut settings = UserSettings::default();
        let Ok(text) = std::str::from_utf8(bytes) else { return Loaded { settings, damaged: true } };
        let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#'));
        let header_ok = lines
            .next()
            .and_then(|l| l.strip_prefix(HEADER))
            .is_some_and(|v| v.trim().chars().all(|c| c.is_ascii_digit()) && !v.trim().is_empty());
        if !header_ok {
            return Loaded { settings, damaged: true };
        }
        let mut damaged = false;
        for line in lines {
            let Some((key, value)) = line.split_once('=') else {
                damaged = true;
                continue;
            };
            let (key, value) = (key.trim(), value.trim());
            if let Some(id) = key.strip_prefix(SHORTCUT_PREFIX) {
                if super::shortcuts::override_is_valid(value) {
                    settings.shortcuts.insert(id.to_owned(), value.to_owned());
                } else {
                    damaged = true;
                }
            } else if let Some(known) = PrefKey::ALL.iter().find(|k| k.id() == key) {
                // Stored lengths are millimetres whatever the display unit is.
                match settings.prefs.with_stored(*known, value) {
                    Some(next) => settings.prefs = next,
                    None => damaged = true,
                }
            } else {
                settings.unknown.push(line.to_owned());
            }
        }
        Loaded { settings, damaged }
    }

    pub(super) fn to_text(&self) -> String {
        let mut out = format!("{HEADER} {FORMAT_VERSION}\n");
        for key in PrefKey::ALL {
            out.push_str(&format!("{} = {}\n", key.id(), self.prefs.stored(key)));
        }
        for (id, binding) in &self.shortcuts {
            out.push_str(&format!("{SHORTCUT_PREFIX}{id} = {binding}\n"));
        }
        for line in &self.unknown {
            out.push_str(line);
            out.push('\n');
        }
        out
    }
}

impl Preferences {
    /// Like [`Preferences::with`] for a value read from the file: lengths are millimetres.
    fn with_stored(&self, key: PrefKey, value: &str) -> Option<Preferences> {
        let mm_prefs = Preferences { units: LengthUnit::Millimeters, ..*self };
        let mut next = mm_prefs.with(key, value).ok()?;
        next.units = if key == PrefKey::Units { next.units } else { self.units };
        Some(next)
    }
}
