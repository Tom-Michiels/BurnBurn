//! The Cut Settings sheet.

use burnburn_presentation::{AppViewModel, OperationKind};
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
}

pub fn cut_settings_panel(vm: &AppViewModel, document: DocumentId, locale: Locale) -> Option<CutSettingsPanel> {
    let c = vm.document(document)?.cut_settings.as_ref()?;
    let modes = SHEET_MODES
        .iter()
        .map(|kind| {
            let name = operation_kind_name(*kind, locale);
            match c.modes.iter().find(|m| m.kind == *kind).and_then(|m| m.blocked) {
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
        mode_index: SHEET_MODES.iter().position(|k| *k == c.mode),
        speed: c.speed.clone(),
        power_min: c.power_min.clone(),
        power_max: c.power_max.clone(),
        passes: c.passes.clone(),
        kerf: c.kerf.clone(),
        kerf_enabled: c.mode == OperationKind::Line,
        output: c.output_enabled,
        visible: c.visible,
        error: c.error.map(|k| message_or_key(k, locale)).unwrap_or_default(),
    })
}
