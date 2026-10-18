//! The operations panel: layer table, colour palette and the add row.

use burnburn_presentation::{AppViewModel, LAYER_SLOTS, OperationField, OperationKind};
use burnburn_types::DocumentId;

use crate::l10n::{HostText, Locale, host_text, message_or_key};

/// Kinds the operations panel's pop-up offers, top to bottom.
pub const OPERATION_KINDS: [OperationKind; 2] = [OperationKind::Line, OperationKind::Fill];

/// The kind for pop-up item `index` (−1 = nothing chosen).
pub fn operation_kind(index: isize) -> Option<OperationKind> {
    OPERATION_KINDS.get(usize::try_from(index).ok()?).copied()
}

pub fn operation_kind_name(kind: OperationKind, locale: Locale) -> &'static str {
    let text = match kind {
        OperationKind::Line => HostText::OperationKindLine,
        OperationKind::Fill => HostText::OperationKindFill,
        OperationKind::OffsetFill => HostText::OperationKindOffsetFill,
        _ => HostText::OperationKindOther,
    };
    host_text(text, locale)
}

/// One operation as the panel shows it; `speed`/`power` are the view model's texts (the editable
/// field contents), units are in the column captions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationLine {
    /// Palette colour (`C03`); the host draws the swatch in the theme colour of `slot`.
    pub slot: u8,
    pub layer: String,
    pub name: String,
    pub kind: String,
    pub speed: String,
    pub power: String,
    pub passes: String,
    pub targets: String,
    pub output: bool,
    pub visible: bool,
}

/// Columns of the layer (operations) table, left to right.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LayerColumn {
    Layer,
    Name,
    Kind,
    Speed,
    Power,
    Passes,
    Targets,
    /// Checkbox: the operation goes into the job.
    Output,
    /// Checkbox: its objects are drawn on the canvas.
    Visible,
}

pub const LAYER_COLUMNS: [LayerColumn; 9] = [
    LayerColumn::Layer,
    LayerColumn::Name,
    LayerColumn::Kind,
    LayerColumn::Speed,
    LayerColumn::Power,
    LayerColumn::Passes,
    LayerColumn::Targets,
    LayerColumn::Output,
    LayerColumn::Visible,
];

impl LayerColumn {
    /// `NSTableColumn` identifier.
    pub fn id(self) -> &'static str {
        match self {
            LayerColumn::Layer => "layer",
            LayerColumn::Name => "name",
            LayerColumn::Kind => "kind",
            LayerColumn::Speed => "speed",
            LayerColumn::Power => "power",
            LayerColumn::Passes => "passes",
            LayerColumn::Targets => "targets",
            LayerColumn::Output => "output",
            LayerColumn::Visible => "visible",
        }
    }

    pub fn from_id(id: &str) -> Option<LayerColumn> {
        LAYER_COLUMNS.iter().copied().find(|c| c.id() == id)
    }

    /// The raw-text field an edit of this column sets.
    pub fn field(self) -> Option<OperationField> {
        match self {
            LayerColumn::Name => Some(OperationField::Name),
            LayerColumn::Speed => Some(OperationField::Speed),
            LayerColumn::Power => Some(OperationField::PowerMax),
            LayerColumn::Passes => Some(OperationField::Passes),
            _ => None,
        }
    }

    pub fn is_toggle(self) -> bool {
        matches!(self, LayerColumn::Output | LayerColumn::Visible)
    }

    pub fn caption(self, locale: Locale) -> &'static str {
        let text = match self {
            LayerColumn::Layer => HostText::LayerColumn,
            LayerColumn::Name => HostText::OperationName,
            LayerColumn::Kind => HostText::OperationKind,
            LayerColumn::Speed => HostText::OperationSpeed,
            LayerColumn::Power => HostText::OperationPower,
            LayerColumn::Passes => HostText::OperationPasses,
            LayerColumn::Targets => HostText::OperationTargets,
            LayerColumn::Output => HostText::OperationOutput,
            LayerColumn::Visible => HostText::OperationVisible,
        };
        host_text(text, locale)
    }
}

impl OperationLine {
    /// The text a column shows (empty for the checkbox columns).
    pub fn text(&self, column: LayerColumn) -> String {
        match column {
            LayerColumn::Layer => self.layer.clone(),
            LayerColumn::Name => self.name.clone(),
            LayerColumn::Kind => self.kind.clone(),
            LayerColumn::Speed => self.speed.clone(),
            LayerColumn::Power => self.power.clone(),
            LayerColumn::Passes => self.passes.clone(),
            LayerColumn::Targets => self.targets.clone(),
            LayerColumn::Output | LayerColumn::Visible => String::new(),
        }
    }

    /// The state a checkbox column shows.
    pub fn checked(&self, column: LayerColumn) -> Option<bool> {
        match column {
            LayerColumn::Output => Some(self.output),
            LayerColumn::Visible => Some(self.visible),
            _ => None,
        }
    }
}

/// Palette label of a colour slot (`C00` … `C29`, as LightBurn names its layers).
pub fn layer_label(slot: u8) -> String {
    format!("C{slot:02}")
}

/// The colour slot of palette button `tag` (tag = slot).
pub fn palette_slot(tag: isize) -> Option<u8> {
    u8::try_from(tag).ok().filter(|s| *s < LAYER_SLOTS)
}

fn objects_text(n: usize, locale: Locale) -> String {
    match (locale, n) {
        (_, 1) => "1 object".to_owned(),
        (Locale::Nl, n) => format!("{n} objecten"),
        (Locale::En, n) => format!("{n} objects"),
    }
}

/// One palette button.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaletteButton {
    pub slot: u8,
    pub label: String,
    /// Tooltip and accessibility label: the colour and its layer (or that it is still empty).
    pub help: String,
    /// The colour has a layer in this document.
    pub used: bool,
}

pub fn palette_buttons(vm: &AppViewModel, document: DocumentId, locale: Locale) -> Vec<PaletteButton> {
    let ops = vm.document(document).map(|d| d.operations.as_slice()).unwrap_or_default();
    (0..LAYER_SLOTS)
        .map(|slot| {
            let label = layer_label(slot);
            let layer = ops.iter().find(|o| o.slot == slot);
            let help = match layer {
                Some(op) => format!("{label} — {} ({})", op.name, objects_text(op.targets, locale)),
                None => format!("{label} — {}", host_text(HostText::PaletteNewLayer, locale)),
            };
            PaletteButton { slot, label, help, used: layer.is_some() }
        })
        .collect()
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OperationsPanel {
    pub rows: Vec<OperationLine>,
    /// Why presentation sent no command for the last input (empty: no problem).
    pub error: String,
    /// Shown instead of rows when there are none.
    pub empty_text: String,
}

pub fn operations_panel(vm: &AppViewModel, document: DocumentId, locale: Locale) -> OperationsPanel {
    let Some(doc) = vm.document(document) else { return OperationsPanel::default() };
    let rows = doc
        .operations
        .iter()
        .map(|op| OperationLine {
            name: op.name.clone(),
            kind: operation_kind_name(op.kind, locale).to_owned(),
            slot: op.slot,
            layer: layer_label(op.slot),
            speed: op.speed.clone(),
            power: op.power.clone(),
            passes: op.passes.clone(),
            output: op.output_enabled,
            visible: op.visible,
            targets: objects_text(op.targets, locale),
        })
        .collect();
    OperationsPanel {
        rows,
        error: doc.operation_error.map(|k| message_or_key(k, locale)).unwrap_or_default(),
        empty_text: host_text(HostText::NoOperations, locale).to_owned(),
    }
}
