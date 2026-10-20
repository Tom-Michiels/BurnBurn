//! The clipboard payload (W2-L1-bewerken): copied objects as bytes for the OS pasteboard under
//! BurnBurn's own type. Untrusted input: decoding validates through the same domain
//! constructors as opening a project and never panics.

use burnburn_document::{GeometryObject, GeometrySource, ObjectKind, PasteItem};
use burnburn_types::{GeometryId, OperationId, StableId};
use serde::{Deserialize, Serialize};

use crate::ProjectUuid;
use crate::dto::v1;
use crate::error::LoadError;
use crate::shape_dto::Parametric;

/// Largest payload that is parsed at all.
pub const MAX_CLIPBOARD_BYTES: usize = 64 * 1024 * 1024;

/// What was on the clipboard: the objects and the project they were copied from (a layer
/// reference is only meaningful in that project).
#[derive(Debug, Clone, PartialEq)]
pub struct Clipboard {
    pub project: ProjectUuid,
    pub items: Vec<PasteItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ClipboardError {
    Empty,
    TooLarge,
    Malformed(String),
    /// Valid JSON, but not a BurnBurn clipboard.
    NotOurs,
    UnsupportedVersion(u32),
    /// A value is not valid; `path` names it.
    Invalid {
        path: String,
        reason: String,
    },
}

const FORMAT: &str = "burnburn-clipboard";
const VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Payload {
    format: String,
    version: u32,
    /// Hyphenated project uuid.
    project: String,
    items: Vec<Item>,
}

/// Only what decides whether a payload is ours and readable.
#[derive(Deserialize)]
struct Header {
    format: String,
    version: u32,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Item {
    name: String,
    kind: v1::Kind,
    subpaths: Vec<v1::Subpath>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    group: Vec<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    operations: Vec<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    parametric: Option<Parametric>,
}

fn kind_to_dto(kind: ObjectKind) -> v1::Kind {
    match kind {
        ObjectKind::Rectangle => v1::Kind::Rectangle,
        ObjectKind::Line => v1::Kind::Line,
        ObjectKind::Circle => v1::Kind::Circle,
        ObjectKind::Ellipse => v1::Kind::Ellipse,
        ObjectKind::Polygon => v1::Kind::Polygon,
        ObjectKind::Star => v1::Kind::Star,
        ObjectKind::Polyline => v1::Kind::Polyline,
        _ => v1::Kind::Path,
    }
}

fn kind_from_dto(kind: v1::Kind) -> ObjectKind {
    match kind {
        v1::Kind::Rectangle => ObjectKind::Rectangle,
        v1::Kind::Path => ObjectKind::Path,
        v1::Kind::Line => ObjectKind::Line,
        v1::Kind::Circle => ObjectKind::Circle,
        v1::Kind::Ellipse => ObjectKind::Ellipse,
        v1::Kind::Polygon => ObjectKind::Polygon,
        v1::Kind::Star => ObjectKind::Star,
        v1::Kind::Polyline => ObjectKind::Polyline,
    }
}

fn uuid_text(u: &ProjectUuid) -> String {
    let h: String = u.as_bytes().iter().map(|b| format!("{b:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32])
}

fn invalid(e: LoadError) -> ClipboardError {
    match e {
        LoadError::InvalidValue { path, reason } => ClipboardError::Invalid { path, reason },
        other => ClipboardError::Malformed(other.to_string()),
    }
}

pub fn encode_clipboard(project: ProjectUuid, items: &[PasteItem]) -> Result<Vec<u8>, ClipboardError> {
    if items.is_empty() {
        return Err(ClipboardError::Empty);
    }
    let items = items
        .iter()
        .map(|i| -> Result<Item, ClipboardError> {
            Ok(Item {
                name: i.name.clone(),
                kind: kind_to_dto(i.kind),
                subpaths: crate::map::path_to_dto(&i.path),
                group: i.group.clone(),
                operations: i.operations.iter().map(|o| o.raw()).collect(),
                parametric: i
                    .parametric
                    .as_ref()
                    .map(crate::shape_map::shape_to_dto)
                    .transpose()
                    .map_err(|e| ClipboardError::Malformed(e.to_string()))?,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let payload = Payload { format: FORMAT.into(), version: VERSION, project: uuid_text(&project), items };
    serde_json::to_vec(&payload).map_err(|e| ClipboardError::Malformed(e.to_string()))
}

pub fn decode_clipboard(bytes: &[u8]) -> Result<Clipboard, ClipboardError> {
    if bytes.len() > MAX_CLIPBOARD_BYTES {
        return Err(ClipboardError::TooLarge);
    }
    let header: Header = serde_json::from_slice(bytes).map_err(|e| ClipboardError::Malformed(e.to_string()))?;
    if header.format != FORMAT {
        return Err(ClipboardError::NotOurs);
    }
    if header.version != VERSION {
        return Err(ClipboardError::UnsupportedVersion(header.version));
    }
    let payload: Payload = serde_json::from_slice(bytes).map_err(|e| ClipboardError::Malformed(e.to_string()))?;
    let project = ProjectUuid::parse(&payload.project)
        .ok_or_else(|| ClipboardError::Invalid { path: "project".into(), reason: "not a project uuid".into() })?;
    if payload.items.is_empty() {
        return Err(ClipboardError::Empty);
    }
    let mut items = Vec::with_capacity(payload.items.len());
    for (i, item) in payload.items.iter().enumerate() {
        let at = |field: &str| format!("items[{i}].{field}");
        let path = crate::map::path_from_dto(&|| at("subpaths"), &item.subpaths).map_err(invalid)?;
        if path.is_empty() {
            return Err(ClipboardError::Invalid { path: at("subpaths"), reason: "no geometry".into() });
        }
        let kind = kind_from_dto(item.kind);
        let parametric = match &item.parametric {
            Some(dto) => {
                let shape = crate::shape_map::shape_from_dto(&|| at("parametric"), dto).map_err(invalid)?;
                // The definition must produce the stored outline (as when opening a project).
                GeometryObject::new(GeometryId::from_raw(0), String::new(), kind, GeometrySource::Drawn, path.clone())
                    .with_definition(shape)
                    .map_err(|e| ClipboardError::Invalid {
                        path: at("parametric"),
                        reason: crate::shape_map::definition_reason(e),
                    })?;
                Some(shape)
            }
            None => None,
        };
        items.push(PasteItem {
            name: item.name.clone(),
            kind,
            path,
            parametric,
            group: item.group.clone(),
            operations: item.operations.iter().map(|o| OperationId::from_raw(*o)).collect(),
        });
    }
    Ok(Clipboard { project, items })
}
