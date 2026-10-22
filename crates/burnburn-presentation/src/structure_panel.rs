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

/// Everything the structure panel shows; empty lists for a document without structure.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StructureViewModel {
    pub parts: Vec<PartRow>,
    pub plates: Vec<PlateRow>,
    pub references: Vec<ReferenceRow>,
}

/// An edit of one instance, each one undo step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StructureEdit {
    /// Another variant (or none: the definition's own values).
    SetVariant { instance: u64, variant: Option<String> },
    /// A local override of one parameter as typed (`80`, `80 mm`, `3,1 in`); empty removes it.
    SetOverride { instance: u64, parameter: String, text: String },
}
