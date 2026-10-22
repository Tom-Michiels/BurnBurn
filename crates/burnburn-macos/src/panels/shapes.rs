//! The shape section of the inspector, the tool options and the geometry dialog (W2-L1-tekenen):
//! what presentation's view model says, as text in the window language, and the intents the
//! typed values become. Raw text goes to presentation unparsed (DR-05).

use burnburn_presentation::{AppViewModel, GeometryDialogKind, ParamKey, ParamRow, UiIntent};
use burnburn_types::DocumentId;

use crate::l10n::{HostText, Locale, host_text, message_or_key};

/// Rows of one inspector section the host has widgets for.
pub const PARAM_SLOTS: usize = 3;

/// Which inspector section a row belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ParamSection {
    /// The parameters of the selected basic shape.
    Shape,
    /// The options of the active tool.
    Tool,
}

/// One row: caption, text as shown, an on/off choice instead of a field, and why the last input
/// was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParamLine {
    pub key: ParamKey,
    pub label: String,
    pub text: String,
    pub toggle: bool,
    pub error: String,
}

/// Both sections; an empty one is not shown.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ShapesPanel {
    pub shape_title: String,
    pub shape: Vec<ParamLine>,
    pub tool_title: String,
    pub tool: Vec<ParamLine>,
}

fn caption(key: ParamKey, locale: Locale) -> &'static str {
    let text = match key {
        ParamKey::Sides => HostText::ParamSides,
        ParamKey::Points => HostText::ParamPoints,
        ParamKey::Ratio => HostText::ParamRatio,
        ParamKey::CornerRadius => HostText::ParamCornerRadius,
        ParamKey::Columns => HostText::ParamColumns,
        ParamKey::Rows => HostText::ParamRows,
        ParamKey::Count => HostText::ParamCount,
        ParamKey::Sweep => HostText::ParamSweep,
        ParamKey::FilletRadius => HostText::ParamFilletRadius,
        ParamKey::ChamferDistance => HostText::ParamChamferDistance,
        ParamKey::ChamferDistance2 => HostText::ParamChamferDistance2,
        ParamKey::ChamferAngle => HostText::ParamChamferAngle,
        _ => HostText::ParamKeepOriginal,
    };
    host_text(text, locale)
}

fn lines(rows: &[ParamRow], locale: Locale) -> Vec<ParamLine> {
    rows.iter()
        .take(PARAM_SLOTS)
        .map(|r| ParamLine {
            key: r.key,
            label: caption(r.key, locale).to_owned(),
            text: r.text.clone(),
            toggle: r.toggle,
            error: r.error.map(|e| message_or_key(e, locale)).unwrap_or_default(),
        })
        .collect()
}

pub fn shapes_panel(vm: &AppViewModel, document: DocumentId, locale: Locale) -> ShapesPanel {
    let Some(doc) = vm.document(document) else { return ShapesPanel::default() };
    let shape = doc.shape_panel.as_ref().map(|p| lines(&p.rows, locale)).unwrap_or_default();
    let tool = lines(&doc.tool_options, locale);
    ShapesPanel {
        shape_title: if shape.is_empty() {
            String::new()
        } else {
            host_text(HostText::ShapeSectionTitle, locale).to_owned()
        },
        shape,
        tool_title: if tool.is_empty() {
            String::new()
        } else {
            host_text(HostText::ToolOptionsTitle, locale).to_owned()
        },
        tool,
    }
}

/// The typed text of a row becomes an intent unless it is what the row already shows (Return and
/// the following end of editing are one commit; a row in error still takes a retry).
pub fn param_intent(
    vm: &AppViewModel,
    document: DocumentId,
    section: ParamSection,
    slot: usize,
    text: &str,
) -> Option<UiIntent> {
    let doc = vm.document(document)?;
    let row = match section {
        ParamSection::Shape => doc.shape_panel.as_ref()?.rows.get(slot)?,
        ParamSection::Tool => doc.tool_options.get(slot)?,
    };
    if slot >= PARAM_SLOTS || (row.error.is_none() && row.text == text) {
        return None;
    }
    let (key, text) = (row.key, text.to_owned());
    Some(match section {
        ParamSection::Shape => UiIntent::CommitShapeParam { document, key, text },
        ParamSection::Tool => UiIntent::SetToolOption { document, key, text },
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeometryDialogPanel {
    pub kind: GeometryDialogKind,
    pub title: String,
    pub label: String,
    pub text: String,
    pub error: String,
}

pub fn geometry_dialog_panel(vm: &AppViewModel, document: DocumentId, locale: Locale) -> Option<GeometryDialogPanel> {
    let dialog = vm.document(document)?.geometry_dialog.as_ref()?;
    let (title, label) = match dialog.kind {
        GeometryDialogKind::Join => (HostText::GeometryJoinTitle, HostText::GeometryJoinLabel),
        GeometryDialogKind::RoundCorners => (HostText::GeometryRoundTitle, HostText::GeometryRoundLabel),
        _ => (HostText::GeometryOptimizeTitle, HostText::GeometryOptimizeLabel),
    };
    Some(GeometryDialogPanel {
        kind: dialog.kind,
        title: host_text(title, locale).to_owned(),
        label: host_text(label, locale).to_owned(),
        text: dialog.text.clone(),
        error: dialog.error.map(|e| message_or_key(e, locale)).unwrap_or_default(),
    })
}

pub fn geometry_dialog_confirm_intent(vm: &AppViewModel, document: DocumentId, text: &str) -> Option<UiIntent> {
    vm.document(document)?.geometry_dialog.as_ref()?;
    Some(UiIntent::ApplyGeometryDialog { document, text: text.to_owned() })
}

pub fn geometry_dialog_cancel_intent(vm: &AppViewModel, document: DocumentId) -> Option<UiIntent> {
    vm.document(document)?.geometry_dialog.as_ref()?;
    Some(UiIntent::CancelGeometryDialog { document })
}
