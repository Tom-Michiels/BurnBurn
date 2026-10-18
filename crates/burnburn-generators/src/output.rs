//! Generator results as derived outputs (contract §11): stable local keys per part.

use burnburn_document::{DerivedKey, DerivedOutput, DerivedOwner, ObjectKind};
use burnburn_geometry::Path;

/// One generated piece of geometry with a key that is stable for the same parameters' role
/// (panel role, cut index), so `GeometryId`s and operation targets survive a parameter change.
#[derive(Debug, Clone, PartialEq)]
pub struct GeneratedPart {
    pub local: u64,
    pub name: String,
    pub path: Path,
}

/// The parts as the output set of `owner` in a `DocumentPatch`.
pub fn to_outputs(owner: DerivedOwner, parts: Vec<GeneratedPart>) -> Vec<DerivedOutput> {
    parts
        .into_iter()
        .map(|p| DerivedOutput {
            key: DerivedKey { owner, local: p.local },
            kind: ObjectKind::Path,
            path: p.path,
            name: Some(p.name),
        })
        .collect()
}
