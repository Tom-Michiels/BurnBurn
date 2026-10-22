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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartRow {
    pub id: u64,
    pub name: String,
    pub parameters: Vec<PartValue>,
    pub variants: Vec<String>,
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
    Ambiguous { candidates: usize },
    /// A visible broken link; nothing rebinds by itself.
    Broken { reason: BrokenReason },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceRow {
    pub id: u64,
    pub name: String,
    /// The referenced object and its name, when it still exists.
    pub object: u64,
    pub source: Option<String>,
    pub status: ReferenceStatus,
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
}

/// An edit of one instance, each one undo step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StructureEdit {
    /// Another variant (or none: the definition's own values).
    SetVariant { instance: u64, variant: Option<String> },
    /// A local override of one parameter as typed (`80`, `80 mm`, `3,1 in`); empty removes it.
    SetOverride { instance: u64, parameter: String, text: String },
}
