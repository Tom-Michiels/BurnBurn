//! A panel as data: rows of controls, how they are laid out in pixels, what changed between two
//! specs (so the Win32 layer updates controls in place instead of rebuilding and losing focus),
//! and the event a control reports. Nothing here knows Win32.

use std::collections::BTreeMap;

use crate::panels_image::BitmapImage;

/// Notification codes of a [`PanelEvent`]: Win32's own where one exists, host-defined above `0x7000`.
pub mod notify {
    /// `BN_CLICKED`.
    pub const CLICKED: u16 = 0;
    /// `CBN_SELCHANGE` / `LBN_SELCHANGE`.
    pub const CHANGED: u16 = 1;
    /// `LBN_DBLCLK`.
    pub const DOUBLE: u16 = 2;
    /// `EN_KILLFOCUS`: an edit lost the keyboard focus.
    pub const EDIT_DONE: u16 = 0x0200;
    /// Return in an edit control.
    pub const ENTER: u16 = 0x7001;
    /// Mouse down (or key down) on a hold button: the jog starts.
    pub const PRESS: u16 = 0x7002;
    /// Mouse up, capture lost or focus lost on a hold button: the jog stops.
    pub const RELEASE: u16 = 0x7003;
    /// A click on a bitmap control; the position is in [`super::PanelEvent::click_px`].
    pub const BITMAP_CLICK: u16 = 0x7004;
    /// A slider moved.
    pub const SLIDER: u16 = 0x7005;
    /// The panel window was closed by the user.
    pub const CLOSED: u16 = 0x7006;
    /// The panel window lost the foreground (a held jog key must be released).
    pub const DEACTIVATED: u16 = 0x7007;
}

/// What a control is.
#[derive(Debug, Clone, PartialEq)]
pub enum ControlKind {
    /// Static text, one line.
    Label,
    /// Static text in bold: a section header.
    Heading,
    /// Read-only multi-line text (reports, lists of findings).
    Report { lines: u16 },
    Button,
    /// A button that reports press and release instead of a click (jog).
    HoldButton,
    Check { checked: bool },
    /// A single-line edit; the text is committed by Return or by leaving the control.
    Edit,
    /// A drop-down list.
    Combo { items: Vec<String>, selected: Option<usize> },
    /// A list box of `lines` rows.
    List { items: Vec<String>, selected: Option<usize>, lines: u16 },
    /// A horizontal slider.
    Slider { min: i32, max: i32, pos: i32 },
    /// A picture of fixed height; clicks are reported with their position.
    Bitmap { image: Option<BitmapImage>, height_px: i32 },
}

/// One control. `weight` is its share of the row's width.
#[derive(Debug, Clone, PartialEq)]
pub struct ControlSpec {
    pub id: u16,
    pub kind: ControlKind,
    pub text: String,
    pub enabled: bool,
    /// The reason in words when the control is off (the tooltip), empty when there is none.
    pub tooltip: String,
    pub weight: u16,
}

impl ControlSpec {
    fn new(id: u16, kind: ControlKind, text: impl Into<String>) -> Self {
        ControlSpec { id, kind, text: text.into(), enabled: true, tooltip: String::new(), weight: 1 }
    }
    pub fn label(id: u16, text: impl Into<String>) -> Self {
        Self::new(id, ControlKind::Label, text)
    }
    pub fn heading(id: u16, text: impl Into<String>) -> Self {
        Self::new(id, ControlKind::Heading, text)
    }
    pub fn report(id: u16, lines: u16, text: impl Into<String>) -> Self {
        Self::new(id, ControlKind::Report { lines }, text)
    }
    pub fn button(id: u16, text: impl Into<String>, enabled: bool) -> Self {
        ControlSpec { enabled, ..Self::new(id, ControlKind::Button, text) }
    }
    pub fn hold_button(id: u16, text: impl Into<String>, enabled: bool) -> Self {
        ControlSpec { enabled, ..Self::new(id, ControlKind::HoldButton, text) }
    }
    pub fn check(id: u16, text: impl Into<String>, checked: bool, enabled: bool) -> Self {
        ControlSpec { enabled, ..Self::new(id, ControlKind::Check { checked }, text) }
    }
    pub fn edit(id: u16, text: impl Into<String>, enabled: bool) -> Self {
        ControlSpec { enabled, ..Self::new(id, ControlKind::Edit, text) }
    }
    pub fn combo(id: u16, items: Vec<String>, selected: Option<usize>, enabled: bool) -> Self {
        ControlSpec { enabled, ..Self::new(id, ControlKind::Combo { items, selected }, "") }
    }
    pub fn list(id: u16, lines: u16, items: Vec<String>, selected: Option<usize>, enabled: bool) -> Self {
        ControlSpec { enabled, ..Self::new(id, ControlKind::List { items, selected, lines }, "") }
    }
    pub fn slider(id: u16, min: i32, max: i32, pos: i32, enabled: bool) -> Self {
        ControlSpec { enabled, ..Self::new(id, ControlKind::Slider { min, max, pos }, "") }
    }
    pub fn bitmap(id: u16, height_px: i32, image: Option<BitmapImage>) -> Self {
        Self::new(id, ControlKind::Bitmap { image, height_px }, "")
    }
    pub fn weighted(mut self, weight: u16) -> Self {
        self.weight = weight.max(1);
        self
    }
    pub fn tip(mut self, tooltip: impl Into<String>) -> Self {
        self.tooltip = tooltip.into();
        self
    }
    pub fn disabled_because(mut self, why: impl Into<String>) -> Self {
        self.enabled = false;
        self.tooltip = why.into();
        self
    }
}

/// Controls side by side.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Row {
    pub controls: Vec<ControlSpec>,
}

impl Row {
    pub fn of(controls: Vec<ControlSpec>) -> Self {
        Row { controls }
    }
    pub fn one(control: ControlSpec) -> Self {
        Row { controls: vec![control] }
    }
}

/// A whole panel: the window title and its rows.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PanelSpec {
    pub title: String,
    pub rows: Vec<Row>,
}

impl PanelSpec {
    pub fn controls(&self) -> impl Iterator<Item = &ControlSpec> {
        self.rows.iter().flat_map(|r| r.controls.iter())
    }
    pub fn control(&self, id: u16) -> Option<&ControlSpec> {
        self.controls().find(|c| c.id == id)
    }
}

/// Pixel metrics of a layout (already scaled for the window's DPI by the caller).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Metrics {
    /// Client width the rows fill.
    pub width: i32,
    pub margin: i32,
    pub gap: i32,
    /// Height of a one-line control.
    pub line: i32,
    /// Height of one text line inside a report or list.
    pub text_line: i32,
}

impl Metrics {
    /// 96 dpi metrics for a panel `width` pixels wide.
    pub fn base(width: i32) -> Self {
        Metrics { width, margin: 10, gap: 6, line: 24, text_line: 16 }
    }
}

/// Where a control goes, in client pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placement {
    pub id: u16,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// The height of a control inside a row.
fn control_height(kind: &ControlKind, m: &Metrics) -> i32 {
    match kind {
        ControlKind::Report { lines } => i32::from(*lines).max(1) * m.text_line + 8,
        ControlKind::List { lines, .. } => i32::from(*lines).max(1) * m.text_line + 8,
        ControlKind::Bitmap { height_px, .. } => (*height_px).max(1),
        _ => m.line,
    }
}

/// Places every control of `spec`; the second value is the total client height needed.
pub fn layout(spec: &PanelSpec, metrics: &Metrics) -> (Vec<Placement>, i32) {
    let m = metrics;
    let inner = (m.width - 2 * m.margin).max(0);
    let mut places = Vec::new();
    let mut y = m.margin;
    for row in spec.rows.iter().filter(|r| !r.controls.is_empty()) {
        let count = i32::try_from(row.controls.len()).unwrap_or(i32::MAX);
        let shared = (inner - m.gap * (count - 1)).max(0);
        let total_weight: i32 = row.controls.iter().map(|c| i32::from(c.weight.max(1))).sum();
        let height = row.controls.iter().map(|c| control_height(&c.kind, m)).max().unwrap_or(m.line);
        let mut x = m.margin;
        let mut used = 0;
        let mut weight_so_far = 0;
        for control in &row.controls {
            weight_so_far += i32::from(control.weight.max(1));
            // Cumulative rounding: the widths always add up to the shared width.
            let end = shared * weight_so_far / total_weight.max(1);
            let w = end - used;
            used = end;
            places.push(Placement { id: control.id, x, y, w, h: control_height(&control.kind, m) });
            x += w + m.gap;
        }
        y += height + m.gap;
    }
    // The last row's trailing gap is replaced by the bottom margin.
    let bottom = if spec.rows.iter().any(|r| !r.controls.is_empty()) { y - m.gap + m.margin } else { 2 * m.margin };
    (places, bottom)
}

/// What to do to a live control so that it shows the new spec.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ControlPatch {
    pub id: u16,
    pub text: Option<String>,
    pub enabled: Option<bool>,
    pub tooltip: Option<String>,
    pub checked: Option<bool>,
    /// New items and selection of a combo or list.
    pub items: Option<(Vec<String>, Option<usize>)>,
    pub selected: Option<Option<usize>>,
    pub slider: Option<i32>,
    pub bitmap: Option<Option<BitmapImage>>,
}

/// The difference between the spec on screen and the next one.
#[derive(Debug, Clone, PartialEq)]
pub enum SpecDiff {
    /// Nothing to do.
    Same,
    /// Same controls, some values changed.
    Patch(Vec<ControlPatch>),
    /// Different controls: destroy and create again.
    Rebuild,
}

/// Controls that must be created again when this differs: same id, same kind of control, same
/// size-giving properties, so that the layout stays valid.
fn same_shape(a: &ControlSpec, b: &ControlSpec) -> bool {
    a.id == b.id
        && a.weight == b.weight
        && match (&a.kind, &b.kind) {
            (ControlKind::Report { lines: x }, ControlKind::Report { lines: y }) => x == y,
            (ControlKind::List { lines: x, .. }, ControlKind::List { lines: y, .. }) => x == y,
            (ControlKind::Bitmap { height_px: x, .. }, ControlKind::Bitmap { height_px: y, .. }) => x == y,
            (ControlKind::Slider { min: a1, max: a2, .. }, ControlKind::Slider { min: b1, max: b2, .. }) => {
                a1 == b1 && a2 == b2
            }
            (x, y) => std::mem::discriminant(x) == std::mem::discriminant(y),
        }
}

pub fn diff_specs(old: &PanelSpec, new: &PanelSpec) -> SpecDiff {
    let shape_of = |s: &PanelSpec| s.rows.iter().map(|r| r.controls.len()).collect::<Vec<_>>();
    if shape_of(old) != shape_of(new) {
        return SpecDiff::Rebuild;
    }
    let mut patches = Vec::new();
    for (a, b) in old.controls().zip(new.controls()) {
        if !same_shape(a, b) {
            return SpecDiff::Rebuild;
        }
        let mut patch = ControlPatch { id: b.id, ..ControlPatch::default() };
        let mut changed = false;
        if a.text != b.text {
            patch.text = Some(b.text.clone());
            changed = true;
        }
        if a.enabled != b.enabled {
            patch.enabled = Some(b.enabled);
            changed = true;
        }
        if a.tooltip != b.tooltip {
            patch.tooltip = Some(b.tooltip.clone());
            changed = true;
        }
        match (&a.kind, &b.kind) {
            (ControlKind::Check { checked: x }, ControlKind::Check { checked: y }) if x != y => {
                patch.checked = Some(*y);
                changed = true;
            }
            (ControlKind::Combo { items: ia, selected: sa }, ControlKind::Combo { items: ib, selected: sb })
            | (ControlKind::List { items: ia, selected: sa, .. }, ControlKind::List { items: ib, selected: sb, .. }) => {
                if ia != ib {
                    patch.items = Some((ib.clone(), *sb));
                    changed = true;
                } else if sa != sb {
                    patch.selected = Some(*sb);
                    changed = true;
                }
            }
            (ControlKind::Slider { pos: x, .. }, ControlKind::Slider { pos: y, .. }) if x != y => {
                patch.slider = Some(*y);
                changed = true;
            }
            (ControlKind::Bitmap { image: x, .. }, ControlKind::Bitmap { image: y, .. }) if x != y => {
                patch.bitmap = Some(y.clone());
                changed = true;
            }
            _ => {}
        }
        if changed {
            patches.push(patch);
        }
    }
    if patches.is_empty() { SpecDiff::Same } else { SpecDiff::Patch(patches) }
}

/// What the Win32 layer read from all controls of a panel when one of them reported.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PanelValues {
    pub texts: BTreeMap<u16, String>,
    pub checks: BTreeMap<u16, bool>,
    pub selection: BTreeMap<u16, Option<usize>>,
    pub sliders: BTreeMap<u16, i32>,
}

impl PanelValues {
    pub fn text(&self, id: u16) -> &str {
        self.texts.get(&id).map_or("", String::as_str)
    }
    pub fn checked(&self, id: u16) -> bool {
        self.checks.get(&id).copied().unwrap_or(false)
    }
    /// The selected item as the `isize` the shared panel models take (`-1` = nothing chosen).
    pub fn selected(&self, id: u16) -> isize {
        self.selection.get(&id).copied().flatten().map_or(-1, |i| isize::try_from(i).unwrap_or(-1))
    }
    pub fn slider(&self, id: u16) -> Option<i32> {
        self.sliders.get(&id).copied()
    }
}

/// One notification of a panel control, with the state of all controls at that moment.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PanelEvent {
    pub id: u16,
    pub code: u16,
    pub values: PanelValues,
    /// For [`notify::BITMAP_CLICK`]: the click in the control's client pixels.
    pub click_px: Option<[f64; 2]>,
    /// For [`notify::BITMAP_CLICK`]: the control's client size in pixels.
    pub view_px: Option<[f64; 2]>,
}

impl PanelEvent {
    pub fn click(id: u16, values: PanelValues) -> Self {
        PanelEvent { id, code: notify::CLICKED, values, click_px: None, view_px: None }
    }
}
