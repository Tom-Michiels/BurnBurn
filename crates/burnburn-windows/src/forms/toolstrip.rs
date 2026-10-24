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
    /// The icon id of `burnburn-icons` (the tool id itself); empty without artwork.
    pub icon: &'static str,
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
        icon: b.icon,
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

/// What a strip control draws instead of text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StripIcon {
    /// Text only: no artwork for the tool.
    None,
    /// The icon of the tool (an id of `burnburn-icons`).
    Tool(&'static str),
    /// The drop-down caret of a group.
    Caret,
}

/// A strip control at its place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StripPlaced {
    pub action: StripAction,
    pub icon: StripIcon,
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
            let icon =
                if burnburn_icons::has_icon(g.button.icon) { StripIcon::Tool(g.button.icon) } else { StripIcon::None };
            let has_drop = g.variants.len() > 1;
            // The tool and its drop-down stay on one row.
            // A button with an icon is as wide as every other one; one without artwork shows its name.
            let width = if icon == StripIcon::None { label_w.max(px(28.0)) } else { px(28.0) };
            let rect = place(&mut x, &mut y, width, if has_drop { drop_w + gap } else { 0 });
            controls.push(StripPlaced {
                action: StripAction::Tool(g.button.id.clone()),
                icon,
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
                    icon: StripIcon::Caret,
                    label: String::new(),
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

/// The tooltip (label and the reason) of the disabled strip button under `point`. Enabled buttons
/// show their own tooltip; a disabled window gets no mouse input, so the frame asks this.
pub fn disabled_tip_at(layout: &StripLayout, point: (i32, i32)) -> Option<String> {
    let (x, y) = point;
    let under =
        layout.controls.iter().find(|c| x >= c.rect.x && x < c.rect.right() && y >= c.rect.y && y < c.rect.bottom())?;
    (!under.enabled && !under.tooltip.is_empty()).then(|| under.tooltip.clone())
}
