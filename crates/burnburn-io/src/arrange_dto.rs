//! On-disk schema for draw order and locks (W2-L1-bewerken; additive in schema 2: absent = none).

use burnburn_document::{ArrangeLayer, DocumentSnapshot};
use burnburn_types::{GeometryId, StableId};
use serde::{Deserialize, Serialize};

use crate::error::LoadError;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Arrange {
    /// Explicitly ordered objects, back to front. Omitted when the order is creation order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub order: Vec<u64>,
    /// Locked objects, ascending. Omitted when none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub locked: Vec<u64>,
}

pub(crate) fn arrange_to_dto(doc: &DocumentSnapshot) -> Option<Arrange> {
    let layer = doc.arrange();
    (!layer.is_empty()).then(|| Arrange {
        order: layer.order().iter().map(|id| id.raw()).collect(),
        locked: layer.locked().map(|id| id.raw()).collect(),
    })
}

pub(crate) fn arrange_from_dto(
    dto: Option<&Arrange>,
    snapshot: DocumentSnapshot,
) -> Result<DocumentSnapshot, LoadError> {
    let Some(dto) = dto else { return Ok(snapshot) };
    let invalid = |path: String, reason: &str| LoadError::InvalidValue { path, reason: reason.to_owned() };
    for (field, ids) in [("order", &dto.order), ("locked", &dto.locked)] {
        for (i, raw) in ids.iter().enumerate() {
            if snapshot.object(GeometryId::from_raw(*raw)).is_none() {
                return Err(invalid(format!("arrange.{field}[{i}]"), "no such object"));
            }
        }
    }
    let layer = ArrangeLayer::from_parts(
        dto.order.iter().map(|r| GeometryId::from_raw(*r)).collect(),
        dto.locked.iter().map(|r| GeometryId::from_raw(*r)).collect(),
    )
    .map_err(|_| invalid("arrange.order".into(), "an object occurs twice"))?;
    snapshot.with_arrange(layer).map_err(|e| invalid("arrange".into(), &e.to_string()))
}
