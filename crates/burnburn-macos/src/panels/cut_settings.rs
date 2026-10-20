//! The Cut Settings sheet.

use burnburn_presentation::{
    AppViewModel, CutChoice, CutControl, CutExtras, CutField, CutSettingsViewModel, CutValue, FieldState, OperationKind,
};
use burnburn_types::{DocumentId, OperationId};

use super::operations::{layer_label, operation_kind_name};
use crate::l10n::{HostText, Locale, host_text, message_or_key};

/// Modes of the Cut Settings pop-up, top to bottom (as presentation lists them).
pub const SHEET_MODES: [OperationKind; 3] = [OperationKind::Line, OperationKind::Fill, OperationKind::OffsetFill];

/// The mode of pop-up item `index` (−1 = nothing chosen).
pub fn sheet_mode(index: isize) -> Option<OperationKind> {
    SHEET_MODES.get(usize::try_from(index).ok()?).copied()
}

/// Whether the kerf field applies to pop-up item `index` (Line only).
pub fn kerf_applies(index: isize) -> bool {
    sheet_mode(index) == Some(OperationKind::Line)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModeItem {
    pub title: String,
    /// False for a mode the machine profile does not support; the title says why.
    pub enabled: bool,
}

/// The Cut Settings sheet's texts and states.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CutSettingsPanel {
    pub operation: OperationId,
    pub slot: u8,
    pub title: String,
    pub modes: Vec<ModeItem>,
    pub mode_index: Option<usize>,
    pub speed: String,
    pub power_min: String,
    pub power_max: String,
    pub passes: String,
    pub kerf: String,
    /// Kerf applies to Line only.
    pub kerf_enabled: bool,
    pub output: bool,
    pub visible: bool,
    pub error: String,
    /// The option rows for the values the sheet opens with.
    pub options: Vec<OptionRow>,
}

pub fn cut_settings_panel(vm: &AppViewModel, document: DocumentId, locale: Locale) -> Option<CutSettingsPanel> {
    let c = vm.document(document)?.cut_settings.as_ref()?;
    let modes = c
        .modes
        .iter()
        .map(|m| {
            let name = operation_kind_name(m.kind, locale);
            match m.blocked {
                Some(reason) => {
                    ModeItem { title: format!("{name} — {}", message_or_key(reason, locale)), enabled: false }
                }
                None => ModeItem { title: name.to_owned(), enabled: true },
            }
        })
        .collect();
    Some(CutSettingsPanel {
        operation: c.operation,
        slot: c.slot,
        title: format!("{} — {} {}", host_text(HostText::CutSettingsTitle, locale), layer_label(c.slot), c.name),
        modes,
        mode_index: c.modes.iter().position(|m| m.kind == c.mode),
        speed: c.speed.clone(),
        power_min: c.power_min.clone(),
        power_max: c.power_max.clone(),
        passes: c.passes.clone(),
        kerf: c.kerf.clone(),
        kerf_enabled: c.mode == OperationKind::Line,
        output: c.output_enabled,
        visible: c.visible,
        error: c.error.map(|k| message_or_key(k, locale)).unwrap_or_default(),
        options: option_rows(c, c.mode, &c.extras, locale),
    })
}

/// The mode chosen by pop-up item `index` of the open sheet (as presentation lists them); the
/// sheet's own mode when nothing valid is chosen.
pub fn sheet_mode_of(sheet: &CutSettingsViewModel, index: isize) -> OperationKind {
    usize::try_from(index).ok().and_then(|i| sheet.modes.get(i)).map_or(sheet.mode, |m| m.kind)
}

/// The option values a confirmed sheet sends, as presentation's [`CutExtras`].
pub fn confirmed_extras(options: Vec<(CutField, CutValue)>) -> CutExtras {
    let mut extras = CutExtras::new();
    for (field, value) in options {
        extras.set(field, value);
    }
    extras
}

/// What a row's control looks like.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowControl {
    Text(String),
    Check(bool),
    Popup { items: Vec<ModeItem>, selected: usize },
}

/// One option row of the sheet: caption, control, whether it can be edited and the note under it
/// (why it is greyed, or the advice that applies to it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptionRow {
    pub field: CutField,
    pub caption: String,
    pub control: RowControl,
    pub enabled: bool,
    pub note: String,
}

pub fn option_caption(field: CutField, locale: Locale) -> &'static str {
    use CutField as F;
    let (nl, en) = match field {
        F::TabsEnabled => ("Tabs / bruggen", "Tabs / bridges"),
        F::TabsMode => ("Tabs per", "Tabs by"),
        F::TabsValue => ("Aantal of afstand (mm)", "Count or spacing (mm)"),
        F::TabsWidth => ("Tabbreedte (mm)", "Tab width (mm)"),
        F::LeadInEnabled => ("Aanloop (lead-in)", "Lead-in"),
        F::LeadInLength => ("Aanloop, lengte (mm)", "Lead-in length (mm)"),
        F::LeadInShape => ("Aanloop, vorm", "Lead-in shape"),
        F::LeadInAngle => ("Aanloop, hoek (°)", "Lead-in angle (°)"),
        F::LeadOutEnabled => ("Uitloop (lead-out)", "Lead-out"),
        F::LeadOutLength => ("Uitloop, lengte (mm)", "Lead-out length (mm)"),
        F::LeadOutShape => ("Uitloop, vorm", "Lead-out shape"),
        F::LeadOutAngle => ("Uitloop, hoek (°)", "Lead-out angle (°)"),
        F::Overcut => ("Overcut (mm, 0 = uit)", "Overcut (mm, 0 = off)"),
        F::PerforationEnabled => ("Perforatie", "Perforation"),
        F::PerforationOn => ("Perforatie, aan (mm)", "Perforation on (mm)"),
        F::PerforationOff => ("Perforatie, uit (mm)", "Perforation off (mm)"),
        F::DotEnabled => ("Dot-modus (puls per afstand)", "Dot mode (pulse per spacing)"),
        F::DotSpacing => ("Dot-afstand (mm)", "Dot spacing (mm)"),
        F::DotDwell => ("Puls (ms)", "Pulse (ms)"),
        F::FillInterval => ("Lijnafstand (mm)", "Line interval (mm)"),
        F::FillAngle => ("Scanhoek (°)", "Scan angle (°)"),
        F::FillBidirectional => ("Bidirectioneel", "Bidirectional"),
        F::FillOverscan => ("Overscan (mm)", "Overscan (mm)"),
        F::FillCrossHatch => ("Kruisarcering", "Cross-hatch"),
        F::FillFloodFill => ("Flood fill", "Flood fill"),
        F::FillGrouping => ("Vormen vullen", "Fill grouping"),
        F::FillRamp => ("Ramp-lengte (mm, 0 = uit)", "Ramp length (mm, 0 = off)"),
        F::ImageMode => ("Beeldmodus", "Image mode"),
        F::ImageLevel => ("Drempel (0–255)", "Threshold (0–255)"),
        F::ImageMatrix => ("Matrixgrootte (2, 4 of 8)", "Matrix size (2, 4 or 8)"),
        F::ImageNegative => ("Negatief beeld", "Negative image"),
        F::ImageDotWidth => ("Dot-breedtecorrectie (mm)", "Dot width correction (mm)"),
        F::ImageAngle => ("Scanhoek (°)", "Scan angle (°)"),
        F::ImageInterval => ("Lijnafstand (mm of dpi)", "Line interval (mm or dpi)"),
        F::ImageDotPitch => ("Puntafstand (mm of dpi)", "Dot pitch (mm or dpi)"),
        F::ImageBidirectional => ("Bidirectioneel", "Bidirectional"),
        F::ImageOverscan => ("Overscan (mm)", "Overscan (mm)"),
        F::ScreenCell => ("Rastercel (mm)", "Screen cell (mm)"),
        F::ScreenAngle => ("Rasterhoek (°)", "Screen angle (°)"),
        _ => ("", ""),
    };
    match locale {
        Locale::Nl => nl,
        Locale::En => en,
    }
}

pub fn choice_label(choice: CutChoice, locale: Locale) -> &'static str {
    use CutChoice as C;
    let (nl, en) = match choice {
        C::TabsByCount => ("Aantal", "Count"),
        C::TabsBySpacing => ("Afstand", "Spacing"),
        C::LeadLine => ("Lijn", "Line"),
        C::LeadArc => ("Boog", "Arc"),
        C::GroupAllShapes => ("Alle vormen tegelijk vullen", "Fill all shapes at once"),
        C::GroupGroups => ("Groepen samen vullen", "Fill groups together"),
        C::GroupIndividual => ("Vormen apart vullen", "Fill shapes individually"),
        C::Threshold => ("Drempel", "Threshold"),
        C::Ordered => ("Geordend", "Ordered"),
        C::Atkinson => ("Atkinson", "Atkinson"),
        C::Dither => ("Dither", "Dither"),
        C::Stucki => ("Stucki", "Stucki"),
        C::Jarvis => ("Jarvis", "Jarvis"),
        C::Newsprint => ("Krantenraster", "Newsprint"),
        C::Halftone => ("Halftoon", "Halftone"),
        C::Sketch => ("Schets", "Sketch"),
        C::Grayscale => ("Grijswaarden", "Grayscale"),
        C::PassThrough => ("Doorgeven", "Pass-through"),
        _ => ("", ""),
    };
    match locale {
        Locale::Nl => nl,
        Locale::En => en,
    }
}

/// The note under a row: why a greyed row is greyed, or the advice of an editable one.
fn row_note(state: FieldState, field: CutField, sheet: &CutSettingsViewModel, locale: Locale) -> String {
    match state {
        FieldState::Disabled(Some(reason)) => message_or_key(reason, locale),
        FieldState::Editable => {
            sheet.gates.advisory.get(&field).map(|k| message_or_key(*k, locale)).unwrap_or_default()
        }
        _ => String::new(),
    }
}

fn row_control(field: CutField, sheet: &CutSettingsViewModel, live: &CutExtras, locale: Locale) -> RowControl {
    match field.control() {
        CutControl::Flag => RowControl::Check(live.flag(field)),
        CutControl::Text => RowControl::Text(live.text(field).to_owned()),
        CutControl::Choice => {
            let items = field
                .choices()
                .iter()
                .map(|c| {
                    let label = choice_label(*c, locale);
                    match sheet.gates.blocked_choices.get(&(field, *c)) {
                        Some(reason) => ModeItem {
                            title: format!("{label} — {}", message_or_key(*reason, locale)),
                            enabled: false,
                        },
                        None => ModeItem { title: label.to_owned(), enabled: true },
                    }
                })
                .collect();
            let selected = live.choice(field).and_then(|c| field.choices().iter().position(|x| *x == c));
            RowControl::Popup { items, selected: selected.unwrap_or(0) }
        }
    }
}

/// The visible option rows for an operation of `kind` (the mode chosen in the sheet) with the
/// values `live` currently on the sheet, in sheet order.
pub fn option_rows(
    sheet: &CutSettingsViewModel,
    kind: OperationKind,
    live: &CutExtras,
    locale: Locale,
) -> Vec<OptionRow> {
    CutField::ALL
        .into_iter()
        .filter_map(|field| {
            let state = live.state(field, kind, &sheet.gates);
            if state == FieldState::Hidden {
                return None;
            }
            Some(OptionRow {
                field,
                caption: option_caption(field, locale).to_owned(),
                control: row_control(field, sheet, live, locale),
                enabled: state == FieldState::Editable,
                note: row_note(state, field, sheet, locale),
            })
        })
        .collect()
}
