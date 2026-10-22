//! The generator dialog (E10, G6-editor): a finger-jointed box or a living hinge from typed
//! parameters. The panel shows what the domain makes (parts, overall size) and what it refuses, at
//! the field it belongs to; Insert makes the generator definition and its outputs in one history
//! entry, and a generated object can be opened again with its parameters. Raw field text goes to
//! the controller unparsed (DR-05). A generator never applies kerf: `thickness` is the material
//! thickness (fit), kerf stays the compiler's job and is applied once.

/// What the dialog makes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GeneratorKind {
    /// Six (closed) or five (open lid) finger-jointed panels laid out flat.
    FingerBox,
    /// Staggered cut lines with bridges that let flat material bend.
    LivingHinge,
}

/// A field of the generator dialog, named in [`GeneratorProblem`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum GeneratorField {
    /// Outer width of the box / extent of the hinge across the bend.
    Width,
    /// Outer depth of the box.
    Depth,
    /// Outer height of the box / extent of the hinge along its cuts.
    Height,
    /// Material thickness of the box (a fit, never kerf).
    Thickness,
    FingerWidth,
    /// Gap between the panels in the flat layout.
    PanelSpacing,
    /// Desired cut length of the hinge.
    CutLength,
    /// Material left between cuts.
    Bridge,
    /// Distance between the hinge columns.
    ColumnSpacing,
}

/// Why the parameters cannot be made. Never a clamped substitute.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum GeneratorIssue {
    /// The field does not hold a valid number.
    InvalidNumber,
    NotPositive,
    Negative,
    /// A detail smaller than can be made: a finger narrower than the material, a hinge shorter
    /// than one cut and two bridges.
    TooSmall {
        min_mm: f64,
        value_mm: f64,
    },
    /// A count outside its range.
    Count {
        min: u64,
        max: u64,
        value: u64,
    },
    /// The domain refused something nothing above anticipated.
    Engine,
}

/// A refusal with the field it belongs to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GeneratorProblem {
    pub field: GeneratorField,
    pub issue: GeneratorIssue,
}

/// One field row: the field and its text as typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratorFieldRow {
    pub field: GeneratorField,
    pub text: String,
}

/// What the parameters make now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratorPreview {
    /// Panels of a box (1 for a hinge: one object holding every cut).
    pub parts: usize,
    /// Cut lines of a hinge (0 for a box).
    pub cuts: usize,
    /// Overall size of the result (mm, as text).
    pub width: String,
    pub height: String,
}

/// The open generator dialog.
#[derive(Debug, Clone, PartialEq)]
pub struct GeneratorDialogViewModel {
    pub kind: GeneratorKind,
    /// An existing generated object is being changed (Insert becomes Apply; one history entry).
    pub editing: bool,
    /// The fields of the kind, in display order.
    pub fields: Vec<GeneratorFieldRow>,
    /// Box only: the lid is left off.
    pub lid_open: Option<bool>,
    pub problems: Vec<GeneratorProblem>,
    /// Set when the parameters make something; shown on the canvas as a preview too.
    pub preview: Option<GeneratorPreview>,
    pub can_insert: bool,
}

/// One edit of the generator dialog.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum GeneratorEdit {
    /// Another kind of generator; its fields start at their defaults.
    Kind(GeneratorKind),
    /// Raw field text.
    Field(GeneratorField, String),
    /// Box only: leave the lid off (five panels) or put it on.
    LidOpen(bool),
    /// Make the generator (or apply the changes): one history entry.
    Insert,
    Cancel,
}
