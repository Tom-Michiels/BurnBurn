//! The panels of the Windows window as plain data (no `windows` crate): a [`Form`] is a list of
//! [`FormLine`]s (fields, checks, pop-ups, buttons, lists) that `win32::forms` realises with
//! standard Win32 controls, and [`react`] says which intent a control becomes. One file per family:
//! inspector, layers, Cut Settings, CAM dialogs, Schikken, text, shapes (value dialog and
//! parameters), sketch and the tool strip.
//!
//! The rows, texts, enablement and intents come from the same headless view-model modules as the
//! macOS panels (`crate::panels`, compiled from the macOS source files), so both hosts show the
//! same panels in the same words and send the same intents. A control only reports its value
//! (field text unparsed, DR-05); whether that is a change is decided here against the view model.

use burnburn_document::process::OrderKey;
use burnburn_presentation::{
    AppViewModel, CamDialog, CutField, DialogActionKind, DialogField, DialogViewModel, FieldKey, UiIntent,
};
use burnburn_types::DocumentId;

use crate::l10n::Locale;
use crate::panels::{G5Control, G5Panel, LayerColumn, ParamSection, SchikkenControl, Slot};

pub mod cam;
pub mod choice;
pub mod cut_settings;
pub mod g5;
pub mod inspector;
pub mod layers;
pub mod layout;
pub mod schikken;
pub mod shapes;
pub mod sketch;
pub mod text;
pub mod toolstrip;

/// What the panels of one window are built from.
#[derive(Clone, Copy)]
pub struct FormContext<'a> {
    pub vm: &'a AppViewModel,
    pub document: DocumentId,
    pub locale: Locale,
}

/// Window state of the panels that is not in the view model (what the Mac keeps in its widgets).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PanelState {
    /// The Schikken options panel was asked for from the menu.
    pub schikken_shown: bool,
    /// The golf-5 panels that wait for a menu item and were asked for (offset and repair, structure).
    pub g5_shown: Vec<G5Panel>,
    /// The layer row selected in the Layers panel.
    pub layer_row: Option<usize>,
}

/// The open CAM dialogs of the window (they are not in the view model: `controller.cam_dialog`).
pub type OpenCamDialogs = [(CamDialog, DialogViewModel)];

/// Which panel a form is; one window per kind and document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FormKind {
    Inspector,
    Layers,
    CutSettings,
    Cam(CamDialog),
    Schikken,
    TextToolbar,
    TextPath,
    Variable,
    Barcode,
    GeometryDialog,
    Sketch,
    /// Notes, offset and repair, centre-line decisions or the structure panel (G5-native).
    G5(G5Panel),
}

/// Where the panel lives in the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    /// Docked in the right column.
    DockRight,
    /// Docked in the left column below the object list.
    DockLeft,
    /// A tool window of its own, owned by the frame.
    Floating,
}

/// Every control that can report something. Typed, so a handler never parses a number from a tag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ControlId {
    Inspector(FieldKey),
    /// Reference point button `n` (`RefPoint::ALL` order).
    RefPoint(usize),
    AspectLock,
    /// Flip button `n` (`INSPECTOR_FLIP_BUTTONS` order).
    Flip(usize),
    Param(ParamSection, usize),
    /// Layer palette colour.
    Palette(u8),
    LayerList,
    /// A detail editor of the selected layer.
    LayerField(LayerColumn),
    LayerCutSettings,
    LayerAddName,
    LayerAddKind,
    LayerAddSpeed,
    LayerAddPower,
    LayerAdd,
    CutMode,
    CutSpeed,
    CutPowerMin,
    CutPowerMax,
    CutPasses,
    CutKerf,
    CutOutput,
    CutVisible,
    CutOption(CutField),
    CutOk,
    CutCancel,
    Cam(DialogField),
    CamAction(DialogActionKind),
    /// Move an `Order by` level up (`true`) or down.
    CamMove(OrderKey, bool),
    Schikken(SchikkenControl),
    /// A control of one of the golf-5 panels.
    G5(G5Control),
    Text(Slot),
    GeometryValue,
    GeometryOk,
    GeometryCancel,
    SketchList,
    SketchValue,
    SketchRole,
    SketchRemove,
    SketchProblem,
    SketchParamName,
    SketchParamText,
    SketchParamSet,
    SketchParam(usize),
}

/// What a control reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ControlValue {
    Text(String),
    Toggle(bool),
    Choice(usize),
    Press,
    /// A button pressed while Shift was held (a palette colour as a second treatment).
    ShiftPress,
    Rows(Vec<usize>),
    /// A double-click (or Enter) on list row `n`.
    Activate(usize),
}

/// All controls' values at the moment of an event (a sheet's OK reads every field).
pub type FormValues = [(ControlId, ControlValue)];

/// One choice of a pop-up; a blocked one stays listed, greyed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChoiceItem {
    pub title: String,
    pub enabled: bool,
}

impl ChoiceItem {
    pub fn plain(title: impl Into<String>) -> ChoiceItem {
        ChoiceItem { title: title.into(), enabled: true }
    }
}

/// A push button.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormButton {
    pub id: ControlId,
    pub title: String,
    pub enabled: bool,
    /// Pressed look for a toggle-like button (the chosen reference point).
    pub pressed: bool,
}

impl FormButton {
    pub fn new(id: ControlId, title: impl Into<String>, enabled: bool) -> FormButton {
        FormButton { id, title: title.into(), enabled, pressed: false }
    }
}

/// One line of a form.
#[derive(Debug, Clone, PartialEq)]
pub enum FormLine {
    Heading(String),
    Text(String),
    /// Why something did not work; drawn apart from the normal text.
    Problem(String),
    Field {
        id: ControlId,
        caption: String,
        text: String,
        enabled: bool,
        /// The text was refused: what the user typed stays so it can be corrected.
        invalid: bool,
        /// Tooltip.
        hint: String,
    },
    Check {
        id: ControlId,
        caption: String,
        on: bool,
        enabled: bool,
    },
    Choice {
        id: ControlId,
        caption: String,
        items: Vec<ChoiceItem>,
        selected: usize,
        enabled: bool,
    },
    Buttons(Vec<FormButton>),
    /// Square buttons in `columns` columns (the reference point).
    Grid {
        columns: usize,
        buttons: Vec<FormButton>,
    },
    List {
        id: ControlId,
        items: Vec<String>,
        selected: Vec<usize>,
        multi: bool,
    },
    /// Lines side by side, each with the same share of the width.
    Row(Vec<FormLine>),
    /// A multi-line text (the project notes); Return makes a new line, nothing commits it but a button.
    Memo {
        id: ControlId,
        text: String,
        max_chars: usize,
    },
    /// A small picture (the preview of a found project), drawn at its pixel size scaled to the form.
    Image(crate::panels_image::BitmapImage),
}

impl FormLine {
    pub fn field(id: ControlId, caption: impl Into<String>, text: impl Into<String>) -> FormLine {
        FormLine::Field {
            id,
            caption: caption.into(),
            text: text.into(),
            enabled: true,
            invalid: false,
            hint: String::new(),
        }
    }

    /// The control ids of this line, in reading order (rows and button groups flattened).
    pub fn ids(&self) -> Vec<ControlId> {
        match self {
            FormLine::Field { id, .. }
            | FormLine::Check { id, .. }
            | FormLine::Choice { id, .. }
            | FormLine::Memo { id, .. }
            | FormLine::List { id, .. } => vec![id.clone()],
            FormLine::Buttons(b) | FormLine::Grid { buttons: b, .. } => b.iter().map(|b| b.id.clone()).collect(),
            FormLine::Row(lines) => lines.iter().flat_map(FormLine::ids).collect(),
            FormLine::Heading(_) | FormLine::Text(_) | FormLine::Problem(_) | FormLine::Image(_) => Vec::new(),
        }
    }
}

/// One panel.
#[derive(Debug, Clone, PartialEq)]
pub struct Form {
    pub kind: FormKind,
    pub title: String,
    pub placement: Placement,
    pub lines: Vec<FormLine>,
}

impl Form {
    /// The control ids in reading order.
    pub fn ids(&self) -> Vec<ControlId> {
        self.lines.iter().flat_map(FormLine::ids).collect()
    }

    /// The line holding control `id`.
    pub fn find(&self, id: &ControlId) -> Option<&FormLine> {
        fn walk<'a>(lines: &'a [FormLine], id: &ControlId) -> Option<&'a FormLine> {
            lines.iter().find_map(|l| match l {
                FormLine::Row(inner) => walk(inner, id),
                other => other.ids().contains(id).then_some(other),
            })
        }
        walk(&self.lines, id)
    }
}

impl Form {
    /// What every control of the form holds as built: the values the Win32 layer would read back
    /// from untouched controls (a field's text, a check, the chosen item, the selected rows).
    pub fn values(&self) -> Vec<(ControlId, ControlValue)> {
        fn walk(lines: &[FormLine], out: &mut Vec<(ControlId, ControlValue)>) {
            for line in lines {
                match line {
                    FormLine::Field { id, text, .. } | FormLine::Memo { id, text, .. } => {
                        out.push((id.clone(), ControlValue::Text(text.clone())))
                    }
                    FormLine::Check { id, on, .. } => out.push((id.clone(), ControlValue::Toggle(*on))),
                    FormLine::Choice { id, selected, .. } => out.push((id.clone(), ControlValue::Choice(*selected))),
                    FormLine::List { id, selected, .. } => out.push((id.clone(), ControlValue::Rows(selected.clone()))),
                    FormLine::Row(inner) => walk(inner, out),
                    FormLine::Heading(_)
                    | FormLine::Text(_)
                    | FormLine::Problem(_)
                    | FormLine::Image(_)
                    | FormLine::Buttons(_)
                    | FormLine::Grid { .. } => {}
                }
            }
        }
        let mut out = Vec::new();
        walk(&self.lines, &mut out);
        out
    }
}

/// A file a form needs before its intent can be made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilePick {
    /// Save the generated test of this CAM dialog as RD.
    CamExportRd(CamDialog),
    /// A CSV for the variable text.
    Csv,
    /// Several pictures for a batch of traces (multiple selection); answered by
    /// [`g5::batch_files_chosen`].
    TraceBatch,
}

/// What a control event leads to.
#[derive(Debug, Clone, PartialEq)]
pub enum FormOutcome {
    Nothing,
    Intent(UiIntent),
    /// The form's local draft changed: build it again from the controls' values.
    Redraw,
    /// The Layers panel selected this row (window state).
    SelectLayerRow(Option<usize>),
    /// Ask for a file, then call the family's `file_chosen`.
    PickFile(FilePick),
}

/// The open-dialog state a form builder or handler needs besides the view model.
#[derive(Clone, Copy)]
pub struct FormAux<'a> {
    pub state: &'a PanelState,
    pub cam: &'a OpenCamDialogs,
}

/// Every form the window shows now, docked ones first.
pub fn forms(ctx: &FormContext<'_>, aux: &FormAux<'_>, values: &FormValues) -> Vec<Form> {
    let mut out = Vec::new();
    out.extend(inspector::form(ctx));
    out.extend(layers::form(ctx, aux.state));
    out.extend(cut_settings::form(ctx, values));
    for (dialog, model) in aux.cam {
        out.extend(cam::form(ctx, *dialog, model));
    }
    out.extend(schikken::form(ctx, aux.state));
    out.extend(text::forms(ctx));
    out.extend(shapes::dialog_form(ctx));
    out.extend(sketch::form(ctx));
    out.extend(g5::forms(ctx, aux.state, values));
    out
}

/// The intent (or follow-up) of a control event of form `kind`. `values` are all of that form's
/// controls now.
pub fn react(
    kind: FormKind,
    id: &ControlId,
    value: &ControlValue,
    values: &FormValues,
    ctx: &FormContext<'_>,
    aux: &FormAux<'_>,
) -> FormOutcome {
    match kind {
        // A control of the stack section of the inspector: the stack panel's own intent (G9-editor).
        FormKind::Inspector if matches!(id, ControlId::G5(_)) => g5::react(G5Panel::Stack, id, values, ctx),
        FormKind::Inspector => inspector::react(id, value, ctx),
        FormKind::Layers => layers::react(id, value, values, ctx, aux.state),
        FormKind::CutSettings => cut_settings::react(id, value, values, ctx),
        FormKind::Cam(dialog) => {
            let model = aux.cam.iter().find(|(d, _)| *d == dialog).map(|(_, m)| m);
            cam::react(dialog, model, id, value, ctx)
        }
        FormKind::Schikken => schikken::react(id, value, values, ctx),
        FormKind::TextToolbar | FormKind::TextPath | FormKind::Variable | FormKind::Barcode => {
            text::react(id, value, ctx)
        }
        FormKind::GeometryDialog => shapes::dialog_react(id, value, values, ctx),
        FormKind::Sketch => sketch::react(id, value, values, ctx),
        FormKind::G5(panel) => g5::react(panel, id, values, ctx),
    }
}

/// The value of control `id` among `values`.
pub fn value_of<'a>(values: &'a FormValues, id: &ControlId) -> Option<&'a ControlValue> {
    values.iter().find(|(i, _)| i == id).map(|(_, v)| v)
}

/// The text of field `id` among `values` (empty when absent).
pub fn text_of(values: &FormValues, id: &ControlId) -> String {
    match value_of(values, id) {
        Some(ControlValue::Text(t)) => t.clone(),
        _ => String::new(),
    }
}

pub fn toggle_of(values: &FormValues, id: &ControlId) -> bool {
    matches!(value_of(values, id), Some(ControlValue::Toggle(true)))
}

pub fn choice_of(values: &FormValues, id: &ControlId) -> Option<usize> {
    match value_of(values, id) {
        Some(ControlValue::Choice(i)) => Some(*i),
        _ => None,
    }
}
