//! The option fields of the Cut Settings sheet beyond mode, speed, power, passes and kerf
//! (W2-L1-cut): Line tabs/leads/overcut/perforation/dot, Fill scan options and Image modes.
//!
//! Every field travels as the user typed it (DR-05): text fields as `String`, checkboxes as
//! `bool`, pop-ups as an index into [`CutField::choices`]. The sheet sends a [`CutExtras`] with
//! the fields it shows; a field that is absent keeps its stored value. Which fields exist,
//! which ones a machine profile blocks and which ones exclude each other is decided here and in
//! the controller, never by the host.

use std::collections::BTreeMap;

use crate::intent::OperationKind;
use crate::view_model::MessageKey;

/// Section of the sheet a field belongs to (the operation's mode decides which one is shown).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CutSection {
    Line,
    Fill,
    Image,
}

/// One option field of the Cut Settings sheet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum CutField {
    TabsEnabled,
    TabsMode,
    /// Tab count or spacing in mm, as `TabsMode` says.
    TabsValue,
    TabsWidth,
    LeadInEnabled,
    LeadInLength,
    LeadInShape,
    LeadInAngle,
    LeadOutEnabled,
    LeadOutLength,
    LeadOutShape,
    LeadOutAngle,
    /// Overcut length in mm; `0` or empty = off.
    Overcut,
    PerforationEnabled,
    PerforationOn,
    PerforationOff,
    DotEnabled,
    DotSpacing,
    DotDwell,
    FillInterval,
    FillAngle,
    FillBidirectional,
    FillOverscan,
    FillCrossHatch,
    FillFloodFill,
    FillGrouping,
    /// Ramp length in mm; `0` or empty = off.
    FillRamp,
    ImageMode,
    /// Threshold / Sketch level (0–255).
    ImageLevel,
    /// Ordered dither matrix size (2, 4 or 8).
    ImageMatrix,
    ImageNegative,
    ImageDotWidth,
    ImageAngle,
    /// Line interval: `0,1 mm` or `254 dpi`.
    ImageInterval,
    /// Dot pitch along a line: `0,1 mm` or `254 dpi`.
    ImageDotPitch,
    ImageBidirectional,
    ImageOverscan,
    ScreenCell,
    ScreenAngle,
    /// Focus (Z) offset of the whole operation in mm; `0` or empty = none (needs the Z axis).
    ZOffset,
}

/// The kind of control a field needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CutControl {
    Text,
    Flag,
    /// A pop-up over [`CutField::choices`].
    Choice,
}

/// An entry of a pop-up field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum CutChoice {
    TabsByCount,
    TabsBySpacing,
    LeadLine,
    LeadArc,
    GroupAllShapes,
    GroupGroups,
    GroupIndividual,
    Threshold,
    Ordered,
    Atkinson,
    Dither,
    Stucki,
    Jarvis,
    Newsprint,
    Halftone,
    Sketch,
    Grayscale,
    PassThrough,
}

const TAB_MODES: [CutChoice; 2] = [CutChoice::TabsByCount, CutChoice::TabsBySpacing];
const LEAD_SHAPES: [CutChoice; 2] = [CutChoice::LeadLine, CutChoice::LeadArc];
const GROUPINGS: [CutChoice; 3] = [CutChoice::GroupAllShapes, CutChoice::GroupGroups, CutChoice::GroupIndividual];
/// LightBurn's order of image modes.
pub const IMAGE_MODES: [CutChoice; 11] = [
    CutChoice::Threshold,
    CutChoice::Ordered,
    CutChoice::Atkinson,
    CutChoice::Dither,
    CutChoice::Stucki,
    CutChoice::Jarvis,
    CutChoice::Newsprint,
    CutChoice::Halftone,
    CutChoice::Sketch,
    CutChoice::Grayscale,
    CutChoice::PassThrough,
];

impl CutField {
    /// Every field, in sheet order.
    pub const ALL: [CutField; 40] = [
        CutField::TabsEnabled,
        CutField::TabsMode,
        CutField::TabsValue,
        CutField::TabsWidth,
        CutField::LeadInEnabled,
        CutField::LeadInLength,
        CutField::LeadInShape,
        CutField::LeadInAngle,
        CutField::LeadOutEnabled,
        CutField::LeadOutLength,
        CutField::LeadOutShape,
        CutField::LeadOutAngle,
        CutField::Overcut,
        CutField::PerforationEnabled,
        CutField::PerforationOn,
        CutField::PerforationOff,
        CutField::DotEnabled,
        CutField::DotSpacing,
        CutField::DotDwell,
        CutField::FillInterval,
        CutField::FillAngle,
        CutField::FillBidirectional,
        CutField::FillOverscan,
        CutField::FillCrossHatch,
        CutField::FillFloodFill,
        CutField::FillGrouping,
        CutField::FillRamp,
        CutField::ImageMode,
        CutField::ImageLevel,
        CutField::ImageMatrix,
        CutField::ImageNegative,
        CutField::ImageDotWidth,
        CutField::ImageAngle,
        CutField::ImageInterval,
        CutField::ImageDotPitch,
        CutField::ImageBidirectional,
        CutField::ImageOverscan,
        CutField::ScreenCell,
        CutField::ScreenAngle,
        CutField::ZOffset,
    ];
}

impl CutField {
    pub fn section(self) -> CutSection {
        use CutField as F;
        match self {
            F::FillInterval
            | F::FillAngle
            | F::FillBidirectional
            | F::FillOverscan
            | F::FillCrossHatch
            | F::FillFloodFill
            | F::FillGrouping
            | F::FillRamp => CutSection::Fill,
            F::ImageMode
            | F::ImageLevel
            | F::ImageMatrix
            | F::ImageNegative
            | F::ImageDotWidth
            | F::ImageAngle
            | F::ImageInterval
            | F::ImageDotPitch
            | F::ImageBidirectional
            | F::ImageOverscan
            | F::ScreenCell
            | F::ScreenAngle => CutSection::Image,
            _ => CutSection::Line,
        }
    }

    pub fn control(self) -> CutControl {
        use CutField as F;
        match self {
            F::TabsEnabled
            | F::LeadInEnabled
            | F::LeadOutEnabled
            | F::PerforationEnabled
            | F::DotEnabled
            | F::FillBidirectional
            | F::FillCrossHatch
            | F::FillFloodFill
            | F::ImageNegative
            | F::ImageBidirectional => CutControl::Flag,
            F::TabsMode | F::LeadInShape | F::LeadOutShape | F::FillGrouping | F::ImageMode => CutControl::Choice,
            _ => CutControl::Text,
        }
    }

    /// The entries of a pop-up field (empty for the others).
    pub fn choices(self) -> &'static [CutChoice] {
        match self {
            CutField::TabsMode => &TAB_MODES,
            CutField::LeadInShape | CutField::LeadOutShape => &LEAD_SHAPES,
            CutField::FillGrouping => &GROUPINGS,
            CutField::ImageMode => &IMAGE_MODES,
            _ => &[],
        }
    }

    /// The checkbox that switches this field's option on; the field is greyed while it is off.
    pub fn switch(self) -> Option<CutField> {
        use CutField as F;
        match self {
            F::TabsMode | F::TabsValue | F::TabsWidth => Some(F::TabsEnabled),
            F::LeadInLength | F::LeadInShape | F::LeadInAngle => Some(F::LeadInEnabled),
            F::LeadOutLength | F::LeadOutShape | F::LeadOutAngle => Some(F::LeadOutEnabled),
            F::PerforationOn | F::PerforationOff => Some(F::PerforationEnabled),
            F::DotSpacing | F::DotDwell => Some(F::DotEnabled),
            _ => None,
        }
    }

    /// Line options that Dot mode excludes (LightBurn greys them while Dot is on).
    pub fn excluded_by_dot(self) -> bool {
        use CutField as F;
        matches!(
            self,
            F::TabsEnabled
                | F::TabsMode
                | F::TabsValue
                | F::TabsWidth
                | F::LeadInEnabled
                | F::LeadInLength
                | F::LeadInShape
                | F::LeadInAngle
                | F::LeadOutEnabled
                | F::LeadOutLength
                | F::LeadOutShape
                | F::LeadOutAngle
                | F::Overcut
                | F::PerforationEnabled
                | F::PerforationOn
                | F::PerforationOff
        )
    }

    /// Whether the sheet shows this field for an operation of `kind`, given the values on the sheet.
    pub fn applies(self, kind: OperationKind, extras: &CutExtras) -> bool {
        // The focus offset belongs to the operation, whatever its mode.
        if self == CutField::ZOffset {
            return matches!(kind, OperationKind::Line | OperationKind::Fill | OperationKind::Image);
        }
        let section = match kind {
            OperationKind::Line => CutSection::Line,
            OperationKind::Fill => CutSection::Fill,
            OperationKind::Image => CutSection::Image,
            _ => return false,
        };
        if self.section() != section {
            return false;
        }
        let mode = extras.choice(CutField::ImageMode);
        match self {
            CutField::ImageLevel => matches!(mode, Some(CutChoice::Threshold | CutChoice::Sketch)),
            CutField::ImageMatrix => mode == Some(CutChoice::Ordered),
            CutField::ScreenCell => matches!(mode, Some(CutChoice::Halftone | CutChoice::Newsprint)),
            CutField::ScreenAngle => mode == Some(CutChoice::Halftone),
            _ => true,
        }
    }
}

/// The value of one field, as the sheet holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CutValue {
    Text(String),
    Flag(bool),
    /// Index into [`CutField::choices`].
    Choice(usize),
}

/// Values of the option fields. Absent fields are not shown or not sent.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CutExtras {
    values: BTreeMap<CutField, CutValue>,
}

impl CutExtras {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, field: CutField) -> Option<&CutValue> {
        self.values.get(&field)
    }

    pub fn iter(&self) -> impl Iterator<Item = (CutField, &CutValue)> {
        self.values.iter().map(|(f, v)| (*f, v))
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// The text of a text field (empty when absent).
    pub fn text(&self, field: CutField) -> &str {
        match self.values.get(&field) {
            Some(CutValue::Text(t)) => t,
            _ => "",
        }
    }

    pub fn flag(&self, field: CutField) -> bool {
        matches!(self.values.get(&field), Some(CutValue::Flag(true)))
    }

    pub fn choice(&self, field: CutField) -> Option<CutChoice> {
        match self.values.get(&field) {
            Some(CutValue::Choice(i)) => field.choices().get(*i).copied(),
            _ => None,
        }
    }

    pub fn set(&mut self, field: CutField, value: CutValue) {
        self.values.insert(field, value);
    }

    pub fn set_text(&mut self, field: CutField, text: impl Into<String>) {
        self.set(field, CutValue::Text(text.into()));
    }

    pub fn set_flag(&mut self, field: CutField, on: bool) {
        self.set(field, CutValue::Flag(on));
    }

    /// Selects `choice` in a pop-up field; a choice the field does not offer is ignored.
    pub fn set_choice(&mut self, field: CutField, choice: CutChoice) {
        if let Some(i) = field.choices().iter().position(|c| *c == choice) {
            self.set(field, CutValue::Choice(i));
        }
    }

    /// The same values with `field` set to the text.
    pub fn with_text(mut self, field: CutField, text: impl Into<String>) -> Self {
        self.set_text(field, text);
        self
    }

    pub fn with_flag(mut self, field: CutField, on: bool) -> Self {
        self.set_flag(field, on);
        self
    }

    pub fn with_choice(mut self, field: CutField, choice: CutChoice) -> Self {
        self.set_choice(field, choice);
        self
    }

    /// Overlays `other` on these values.
    pub fn merged(mut self, other: &CutExtras) -> Self {
        self.values.extend(other.values.iter().map(|(f, v)| (*f, v.clone())));
        self
    }

    /// Whether Dot mode is switched on in these values.
    pub fn dot_on(&self) -> bool {
        self.flag(CutField::DotEnabled)
    }

    /// What the sheet should do with `field` for the values it currently shows: hidden for
    /// another mode, greyed while its switch is off, greyed with a reason while Dot mode
    /// excludes it or the machine profile blocks it.
    pub fn state(&self, field: CutField, kind: OperationKind, gates: &CutGates) -> FieldState {
        if !field.applies(kind, self) {
            return FieldState::Hidden;
        }
        if let Some(reason) = gates.blocked.get(&field) {
            return FieldState::Disabled(Some(*reason));
        }
        if field.excluded_by_dot() && self.dot_on() {
            return FieldState::Disabled(Some(MessageKey::CutOptionsConflict));
        }
        if field.switch().is_some_and(|switch| !self.flag(switch)) {
            return FieldState::Disabled(None);
        }
        FieldState::Editable
    }
}

/// How the sheet presents one field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldState {
    /// Not part of this mode's sheet.
    Hidden,
    Editable,
    /// Visible but greyed; `Some` says why (machine profile or an excluding option).
    Disabled(Option<MessageKey>),
}

/// What the active machine profile does not allow, per field. A blocked field stays visible with
/// its reason; a value is never replaced by another one.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CutGates {
    /// Greyed fields and why.
    pub blocked: BTreeMap<CutField, MessageKey>,
    /// Pop-up entries that cannot be chosen.
    pub blocked_choices: BTreeMap<(CutField, CutChoice), MessageKey>,
    /// Fields that stay editable but whose off-axis values need a capability (scan angles
    /// other than 0°/90°): the reason is shown next to the field and OK refuses such a value.
    pub advisory: BTreeMap<CutField, MessageKey>,
}
