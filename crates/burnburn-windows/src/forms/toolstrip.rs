//! The catalogue tool strip: the tool register as buttons, panel by panel (drawing tools, basic
//! shapes, node toolbar, sketch constraints, ...). The register, the states and the texts are the
//! ones the macOS strip uses (`ToolbarViewModel`, `crate::toolbar`); a click is one `SelectTool`,
//! a drop-down entry one `SelectVariant`.

use burnburn_presentation::{AppViewModel, ToolButton, UiIntent};
use burnburn_types::DocumentId;

use super::layout::Rect;
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

/// What a strip control does when clicked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StripAction {
    /// Choose the tool.
    Tool(String),
    /// Open the drop-down of group `group` (its variants are listed in the strip's group).
    Dropdown { group: String },
}

/// A strip control at its place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StripPlaced {
    pub action: StripAction,
    pub label: String,
    pub tooltip: String,
    pub selected: bool,
    pub enabled: bool,
    pub rect: Rect,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StripLayout {
    pub controls: Vec<StripPlaced>,
    pub height: i32,
}

/// Flow the strip's buttons over rows of `width` pixels: a button that does not fit starts a new
/// row, panels are set apart by a wider gap. `scale` 1.0 = 96 dpi.
pub fn strip_layout(panels: &[StripPanel], width: i32, scale: f64) -> StripLayout {
    let px = |v: f64| (v * scale).round().max(1.0) as i32;
    let (margin, gap, panel_gap, row_h, char_w, pad, drop_w) =
        (px(4.0), px(3.0), px(12.0), px(26.0), px(7.0), px(16.0), px(14.0));
    let mut controls = Vec::new();
    let (mut x, mut y) = (margin, margin);
    let place = |x: &mut i32, y: &mut i32, w: i32, extra: i32| -> Rect {
        // A control never leaves the row; a new row starts at the left margin.
        if *x > margin && *x + w + extra > width - margin {
            *x = margin;
            *y += row_h + gap;
        }
        let r = Rect { x: *x, y: *y, w: w.min(width - 2 * margin).max(1), h: row_h };
        *x += r.w + gap;
        r
    };
    for (n, panel) in panels.iter().enumerate() {
        if n > 0 {
            x += panel_gap - gap;
        }
        for g in &panel.groups {
            let label_w = i32::try_from(g.button.label.chars().count()).unwrap_or(0) * char_w + pad;
            let has_drop = g.variants.len() > 1;
            // The tool and its drop-down stay on one row.
            let rect = place(&mut x, &mut y, label_w.max(px(28.0)), if has_drop { drop_w + gap } else { 0 });
            controls.push(StripPlaced {
                action: StripAction::Tool(g.button.id.clone()),
                label: g.button.label.clone(),
                tooltip: g.button.tooltip.clone(),
                selected: g.button.selected || g.variants.iter().any(|v| v.selected),
                enabled: g.button.enabled,
                rect,
            });
            if let (true, Some(group)) = (has_drop, &g.group) {
                x -= gap;
                let r = Rect { x, y: rect.y, w: drop_w, h: row_h };
                x += drop_w + gap;
                controls.push(StripPlaced {
                    action: StripAction::Dropdown { group: group.clone() },
                    label: "\u{25BE}".to_owned(),
                    tooltip: g.button.tooltip.clone(),
                    selected: false,
                    enabled: g.variants.iter().any(|v| v.enabled),
                    rect: r,
                });
            }
        }
    }
    StripLayout { controls, height: y + row_h + margin }
}
