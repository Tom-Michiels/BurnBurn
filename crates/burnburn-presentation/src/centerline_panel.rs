//! Editing a centre-line result in the tracer (T04, T08): the operator decides which branches
//! continue through an ambiguous junction and accepts an exact line/arc/circle for a chain. Nothing
//! is decided or replaced automatically; the view model lists what can be decided.

/// One operator decision on the open centre-line result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CenterlineEdit {
    /// Picks pairing `candidate` (an index into the junction's ranked candidates) at `node`.
    ResolveJunction { node: u32, candidate: usize },
    /// Accepts the best primitive within the tolerance for `chain`, replacing its free curve.
    ConfirmPrimitive { chain: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrimitiveKind {
    Line,
    Arc,
    Circle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JunctionState {
    /// The tracer does not decide; `candidates` pairings are ranked by deviation from straight.
    Ambiguous { candidates: usize },
    /// Decided by the operator.
    Resolved,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct JunctionRow {
    pub node: u32,
    pub at: (f64, f64),
    pub state: JunctionState,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PrimitiveRow {
    pub kind: PrimitiveKind,
    pub max_residual_mm: f64,
    pub within_tolerance: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChainRow {
    pub chain: u32,
    pub candidates: Vec<PrimitiveRow>,
    /// The primitive the operator accepted for this chain.
    pub confirmed: Option<PrimitiveKind>,
}

/// What can be decided on the open centre-line result.
#[derive(Debug, Clone, PartialEq)]
pub struct CenterlineEditViewModel {
    pub junctions: Vec<JunctionRow>,
    pub chains: Vec<ChainRow>,
    /// Number of operator decisions taken so far.
    pub edits: u64,
    /// Stored decisions that no longer fit this trace (their place is gone) and were not applied.
    pub dropped: usize,
}
