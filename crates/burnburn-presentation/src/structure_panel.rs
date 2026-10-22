//! Parts, variants, plates and references in the UI (S06, S07, P06): the view model of what the
//! document's structure layer knows, and the edits a panel may make. Parameter values are shown
//! in millimetres as plain text.

/// One name/value pair of a part (parameter, override, effective value), in millimetres.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartValue {
    pub name: String,
    pub value: String,
}

/// How an instance evaluated: the invalid ones fail alone, naming the offending parameter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstanceStatus {
    Valid,
    /// Its outputs are the last valid ones, blocked for production.
    Failed {
        parameter: Option<String>,
    },
    /// Changed without an evaluation yet.
    Pending,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstanceRow {
    pub id: u64,
    pub variant: Option<String>,
    /// Local overrides of this instance only; the definition does not have them.
    pub overrides: Vec<PartValue>,
    /// Default values, then the variant, then the overrides.
    pub effective: Vec<PartValue>,
    pub plate: Option<String>,
    pub status: InstanceStatus,
}

/// A variant and the parameter values it sets (the others keep the definition's value).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariantRow {
    pub name: String,
    pub values: Vec<PartValue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartRow {
    pub id: u64,
    pub name: String,
    pub parameters: Vec<PartValue>,
    pub variants: Vec<String>,
    /// The same variants with their values (G6-werk).
    pub variant_values: Vec<VariantRow>,
    pub instances: Vec<InstanceRow>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlateRow {
    pub id: u64,
    pub name: String,
    pub width: String,
    pub height: String,
    pub material: String,
    pub thickness: Option<String>,
    /// Part instances on this plate.
    pub instances: usize,
    /// Other objects assigned to this plate.
    pub objects: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrokenReason {
    /// The referenced object no longer exists.
    ObjectMissing,
    /// The object exists but the referenced edge has no origin any more.
    OriginGone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReferenceStatus {
    Bound,
    /// The origin yields several edges now; the user must choose.
    Ambiguous {
        candidates: usize,
    },
    /// A visible broken link; nothing rebinds by itself.
    Broken {
        reason: BrokenReason,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceRow {
    pub id: u64,
    pub name: String,
    /// The referenced object and its name, when it still exists.
    pub object: u64,
    pub source: Option<String>,
    pub status: ReferenceStatus,
    /// When the reference is ambiguous: the edges it may mean, to choose from (G6-werk).
    pub candidates: Vec<String>,
}

/// One edge of the single selected object: the choice for a new reference (G6-werk).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EdgeRow {
    pub subpath: usize,
    pub segment: usize,
    /// Length of the edge in millimetres.
    pub length: String,
}

/// What the chosen plate could not include as it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlateIssueRow {
    /// An operation of the plate also names an object that is on another plate (or none); that
    /// target is left out of this plate's plan.
    TargetOffPlate { operation: String, object: u64 },
    /// Derived geometry on the plate whose owner did not evaluate: production is refused.
    Blocked { object: u64 },
}

/// The kind of one step of a modifier chain (G7-editor, E06).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ModifierKind {
    Offset,
    Boolean,
    LinearArray,
    GridArray,
    CircularArray,
    PathArray,
}

/// What the one editable number of a step means.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ModifierValueKind {
    /// Copies including the original.
    Count,
    /// Offset distance in mm (positive grows).
    Distance,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModifierStepRow {
    pub index: usize,
    pub kind: ModifierKind,
    /// The step's main number as text (`None`: the step has no single number to edit).
    pub value: Option<(ModifierValueKind, String)>,
    pub enabled: bool,
    /// This step is the one the chain could not compute.
    pub failed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModifierStatus {
    Valid,
    /// Its outputs are the last valid ones, blocked for production; `step` names the offending step.
    Failed {
        step: Option<usize>,
    },
    Pending,
}

/// One non-destructive chain: a source that stays editable, steps in order, derived outputs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModifierRow {
    pub id: u64,
    pub name: String,
    pub source: u64,
    pub source_name: Option<String>,
    pub steps: Vec<ModifierStepRow>,
    pub status: ModifierStatus,
    /// Derived objects the chain currently makes.
    pub outputs: usize,
}

/// Everything the structure panel shows; empty lists for a document without structure.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StructureViewModel {
    pub parts: Vec<PartRow>,
    pub plates: Vec<PlateRow>,
    pub references: Vec<ReferenceRow>,
    /// The plate Compile uses (`None`: the whole design).
    pub compile_plate: Option<u64>,
    /// The plate the shown plan was compiled for (`None`: the whole design).
    pub plan_plate: Option<u64>,
    /// What the chosen plate left out or blocks.
    pub plate_issues: Vec<PlateIssueRow>,
    /// How many objects are selected: what "make a part" would use (G6-werk).
    pub selected: usize,
    /// The edges of the selected object when exactly one is selected; empty otherwise.
    pub edges: Vec<EdgeRow>,
    pub edge_object: Option<u64>,
    /// The modifier chains of the document, in creation order (G7-editor, E06).
    pub modifiers: Vec<ModifierRow>,
}

/// An edit of one instance, each one undo step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StructureEdit {
    /// Another variant (or none: the definition's own values).
    SetVariant {
        instance: u64,
        variant: Option<String>,
    },
    /// A local override of one parameter as typed (`80`, `80 mm`, `3,1 in`); empty removes it.
    SetOverride {
        instance: u64,
        parameter: String,
        text: String,
    },
    /// A part made of the selected objects (G6-werk, S07): rectangles and circles with their sizes
    /// as parameters, other shapes as fixed geometry. The objects stay; nothing is placed.
    DefinePart {
        name: String,
    },
    /// An instance at the typed position of the part's frame (`x`, `y` lengths; empty is 0).
    PlaceInstance {
        definition: u64,
        x: String,
        y: String,
    },
    RemoveInstance {
        instance: u64,
    },
    /// Only when no instance uses the part.
    RemovePart {
        definition: u64,
    },
    /// The definition's own value of a parameter; every instance follows (a local override stays).
    SetParameter {
        definition: u64,
        parameter: String,
        text: String,
    },
    /// A new, empty variant (it takes the definition's values until it is given its own).
    AddVariant {
        definition: u64,
        name: String,
    },
    /// The value a variant sets for a parameter; empty takes it away.
    SetVariantValue {
        definition: u64,
        variant: String,
        parameter: String,
        text: String,
    },
    /// A named reference to edge `segment` of `subpath` of `object` (S06).
    AddReference {
        name: String,
        object: u64,
        subpath: usize,
        segment: usize,
    },
    /// The operator's choice among the candidates of an ambiguous reference.
    RebindReference {
        reference: u64,
        candidate: usize,
    },
    RemoveReference {
        reference: u64,
    },
    /// One step of a modifier chain on or off; source and other steps are untouched (E06).
    ToggleModifierStep {
        modifier: u64,
        step: usize,
        enabled: bool,
    },
    /// The step one place earlier (`up`) or later in the chain; the order changes the result.
    MoveModifierStep {
        modifier: u64,
        step: usize,
        up: bool,
    },
    /// Takes one step out of the chain (the last step stays: remove the chain instead).
    RemoveModifierStep {
        modifier: u64,
        step: usize,
    },
    /// The main number of a step as typed (copies, or an offset distance with its unit).
    SetModifierStepValue {
        modifier: u64,
        step: usize,
        text: String,
    },
    /// Removes the chain and its outputs.
    RemoveModifier {
        modifier: u64,
    },
    /// Applies the chain: its outputs become ordinary objects, the chain is gone, the source stays.
    BakeModifier {
        modifier: u64,
    },
    /// A chain with one offset step on the selected object; the distance as typed.
    CreateOffsetModifier {
        distance: String,
    },
}

/// Why a structure edit was refused (G6-werk). Nothing changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum StructureProblem {
    /// A part is made of the selected objects, and nothing is selected.
    NeedsSelection,
    /// A part or variant needs a name.
    NeedsName,
    UnknownPart,
    UnknownInstance,
    /// The part is still used by an instance.
    PartInUse,
    UnknownVariant,
    VariantExists,
    UnknownParameter,
    UnknownObject,
    /// The object has no such edge.
    NotAnEdge,
    UnknownReference,
    /// Only an ambiguous reference is rebound, by choosing one of its candidates.
    NotAmbiguous,
    NoCandidate,
    /// The chain does not exist (any more).
    UnknownModifier,
    /// The chain has no such step.
    UnknownStep,
    /// The step is already first / last.
    CannotMoveStep,
    /// The last step of a chain stays; remove the chain instead.
    LastStep,
    /// The step has no single number to edit (a boolean or a grid).
    StepHasNoValue,
    /// A number of copies must be a whole number of at least one.
    InvalidCount,
}
