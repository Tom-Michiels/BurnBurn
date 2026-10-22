//! The operator's decisions on a centreline result as a replayable list (G6-werk, T04).
//!
//! Every edit that changes the graph records one [`CenterlineDecision`], anchored to the place it
//! was taken (a node position, the start of a chain) and not only to an id. Replaying the list on a
//! fresh trace of the same source applies each decision where its anchor still is; a decision whose
//! anchor is gone, or that the result now refuses, is skipped and reported — it is never applied to
//! a different junction.

use burnburn_geometry::Point;

use super::{CenterlineEditError, CenterlineResult, ChainEnd, ChainId, NodeKind};

/// How far a recorded place may lie from the one in the new graph (mm). The engine is
/// deterministic, so the same source gives the same places; anything further is another stroke.
const ANCHOR_TOLERANCE_MM: f64 = 1e-4;

/// One operator decision, with the place it was taken at (millimetres, the frame of the result).
#[derive(Debug, Clone, PartialEq)]
pub enum CenterlineDecision {
    /// The branch ends that continue each other at the junction at `at`.
    ResolveJunction { at: Point, pairs: Vec<(ChainEnd, ChainEnd)> },
    /// The exact line, arc or circle accepted for `chain`, whose first point is `at`.
    ConfirmPrimitive { chain: ChainId, at: Point },
    /// A repair chain between the stroke ends at `a` and `b`.
    JoinEndpoints { a: Point, b: Point },
}

/// Why a stored decision was not applied.
#[derive(Debug, Clone, PartialEq)]
pub enum SkipReason {
    /// There is no junction, chain or stroke end where the decision was taken.
    NothingThere,
    /// The result refused it.
    Refused(CenterlineEditError),
}

/// A decision that was not applied, with its place in the stored list.
#[derive(Debug, Clone, PartialEq)]
pub struct SkippedDecision {
    pub index: usize,
    pub decision: CenterlineDecision,
    pub reason: SkipReason,
}

/// The result of replaying a list of decisions on a fresh trace.
#[derive(Debug, Clone, PartialEq)]
pub struct Replayed {
    pub result: CenterlineResult,
    pub applied: usize,
    pub skipped: Vec<SkippedDecision>,
}

impl CenterlineResult {
    /// The decisions that led to this result, in the order they were taken.
    pub fn decisions(&self) -> &[CenterlineDecision] {
        &self.decisions
    }

    /// Applies `decisions` in order on this result.
    pub fn replay(&self, decisions: &[CenterlineDecision]) -> Replayed {
        let mut result = self.clone();
        let (mut applied, mut skipped) = (0, Vec::new());
        for (index, decision) in decisions.iter().enumerate() {
            match result.apply_decision(decision) {
                Ok(next) => {
                    result = next;
                    applied += 1;
                }
                Err(reason) => skipped.push(SkippedDecision { index, decision: decision.clone(), reason }),
            }
        }
        Replayed { result, applied, skipped }
    }

    fn apply_decision(&self, decision: &CenterlineDecision) -> Result<CenterlineResult, SkipReason> {
        let near = |p: Point, q: Point| p.distance(q) <= ANCHOR_TOLERANCE_MM;
        let refused = SkipReason::Refused;
        match decision {
            CenterlineDecision::ResolveJunction { at, pairs } => {
                let node = self
                    .nodes
                    .iter()
                    .find(|n| n.kind == NodeKind::Junction && near(n.at, *at))
                    .ok_or(SkipReason::NothingThere)?;
                self.resolve_junction(node.id, pairs.clone()).map_err(refused)
            }
            CenterlineDecision::ConfirmPrimitive { chain, at } => {
                let found = self.chain(*chain).filter(|c| c.points.first().is_some_and(|p| near(*p, *at)));
                let found = found.ok_or(SkipReason::NothingThere)?;
                let proposal = self.propose_replacement(found.id).map_err(refused)?;
                self.confirm_replacement(&proposal).map_err(refused)
            }
            CenterlineDecision::JoinEndpoints { a, b } => {
                let end = |p: Point| self.nodes.iter().find(|n| n.kind == NodeKind::End && near(n.at, p));
                let (na, nb) = (end(*a).ok_or(SkipReason::NothingThere)?, end(*b).ok_or(SkipReason::NothingThere)?);
                self.join_endpoints(na.id, nb.id).map_err(refused)
            }
        }
    }
}
