//! The catalogue tool strip: the tool register as buttons, panel by panel (drawing tools, basic
//! shapes, node toolbar, sketch constraints, ...). The register, the states and the texts are the
//! ones the macOS strip uses (`ToolbarViewModel`, `crate::toolbar`); a click is one `SelectTool`,
//! a drop-down entry one `SelectVariant`.

use burnburn_presentation::{AppViewModel, ToolButton, UiIntent};
use burnburn_types::DocumentId;

use crate::l10n::Locale;
use crate::toolbar::{panel_title, tool_label, tooltip};

/// One button of the strip (a tool or a variant of a group).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StripButton {
    /// Catalogue id (`sketch.line`).
    pub id: String,
    pub label: String,
    /// Label, shortcut and explanation, or the reason it is disabled.
    pub tooltip: String,
    pub selected: bool,
    pub enabled: bool,
}

/// A tool, or a group of variants with the remembered one on its face.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StripGroup {
    pub group: Option<String>,
    pub button: StripButton,
    /// All variants including the one on the face; empty for a single tool.
    pub variants: Vec<StripButton>,
}

/// A toolbar panel: its title and groups, left to right.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StripPanel {
    pub title: String,
    pub groups: Vec<StripGroup>,
}

fn button(b: &ToolButton, locale: Locale) -> StripButton {
    StripButton {
        id: b.id.to_owned(),
        label: tool_label(b.id, locale),
        tooltip: tooltip(b.id, b.shortcut, b.enablement, locale),
        selected: b.selected,
        enabled: b.enablement.is_enabled(),
    }
}

/// The strip for the active window (the view model's toolbar is the active document's).
pub fn strip(vm: &AppViewModel, locale: Locale) -> Vec<StripPanel> {
    vm.toolbar
        .panels
        .iter()
        .map(|panel| StripPanel {
            title: panel_title(panel.panel, locale),
            groups: panel
                .groups
                .iter()
                .map(|g| StripGroup {
                    group: g.group.map(str::to_owned),
                    button: button(&g.active, locale),
                    variants: g.variants.iter().map(|v| button(v, locale)).collect(),
                })
                .collect(),
        })
        .collect()
}

/// A tool button: choose the tool by its catalogue id (presentation refuses a disabled one).
pub fn tool_intent(vm: &AppViewModel, document: DocumentId, id: &str) -> Option<UiIntent> {
    vm.document(document)?;
    Some(UiIntent::SelectTool { document: Some(document), id: id.to_owned() })
}

/// An entry of a group's drop-down: the group remembers it and the tool starts.
pub fn variant_intent(vm: &AppViewModel, document: DocumentId, group: &str, id: &str) -> Option<UiIntent> {
    vm.document(document)?;
    Some(UiIntent::SelectVariant { document: Some(document), group: group.to_owned(), id: id.to_owned() })
}
