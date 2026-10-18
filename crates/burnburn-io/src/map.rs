//! DTO v1 ↔ domain mapping. Loading validates every value through domain constructors.

use burnburn_document::{DocumentSnapshot, GeometryObject, GeometrySource, ObjectKind};
use burnburn_geometry::{EllipticalArc, Path, Point, Segment, Subpath, Vector};
use burnburn_types::{Angle, ContentHash, GeometryId, StableId};

use crate::dto::v1;
use crate::error::{LoadError, SaveError};

pub(crate) const ASSET_PREFIX: &str = "sha256-";

pub(crate) fn asset_key(h: &ContentHash) -> String {
    format!("{ASSET_PREFIX}{}", h.to_hex())
}

pub(crate) fn parse_asset_key(path: impl Fn() -> String, key: &str) -> Result<ContentHash, LoadError> {
    key.strip_prefix(ASSET_PREFIX)
        .and_then(|hex| ContentHash::from_hex(hex).ok())
        .ok_or_else(|| LoadError::InvalidValue { path: path(), reason: format!("not an asset key: {key:?}") })
}

pub(crate) fn xy(p: Point) -> v1::Xy {
    [v1::Num(p.x), v1::Num(p.y)]
}

fn vxy(v: Vector) -> v1::Xy {
    [v1::Num(v.x), v1::Num(v.y)]
}

pub(crate) fn document_to_dto(doc: &DocumentSnapshot) -> Result<v1::Document, SaveError> {
    let objects = doc
        .objects()
        .map(|(id, o)| -> Result<v1::Object, SaveError> {
            Ok(v1::Object {
                id: id.raw(),
                name: o.name().to_owned(),
                kind: match o.kind() {
                    ObjectKind::Rectangle => v1::Kind::Rectangle,
                    ObjectKind::Line => v1::Kind::Line,
                    ObjectKind::Circle => v1::Kind::Circle,
                    ObjectKind::Ellipse => v1::Kind::Ellipse,
                    ObjectKind::Polygon => v1::Kind::Polygon,
                    ObjectKind::Star => v1::Kind::Star,
                    ObjectKind::Polyline => v1::Kind::Polyline,
                    _ => v1::Kind::Path,
                },
                groups: o.groups().iter().map(|g| g.raw()).collect(),
                parametric: o.parametric().map(crate::shape_map::shape_to_dto).transpose()?,
                edge_keys: o.stored_edge_keys().map(|k| crate::structure_dto::EdgeKeys {
                    next: k.next(),
                    subpaths: k.subpaths().iter().map(|s| s.iter().map(|e| e.0).collect()).collect(),
                }),
                source: match o.source() {
                    GeometrySource::Imported { asset, entity } => {
                        v1::Source::Imported { asset: asset_key(asset), entity: entity.clone() }
                    }
                    GeometrySource::Derived(key) => match key.owner {
                        burnburn_document::DerivedOwner::Sketch(id) => {
                            v1::Source::DerivedSketch { sketch: id.raw(), local: key.local }
                        }
                        burnburn_document::DerivedOwner::Text(id) => {
                            v1::Source::DerivedText { text: id.raw(), local: key.local }
                        }
                        burnburn_document::DerivedOwner::Modifier(id) => {
                            v1::Source::DerivedModifier { modifier: id.raw(), local: key.local }
                        }
                        burnburn_document::DerivedOwner::Part(id) => {
                            v1::Source::DerivedPart { instance: id.raw(), local: key.local }
                        }
                        burnburn_document::DerivedOwner::Trace(id) => {
                            v1::Source::DerivedTrace { trace: id.raw(), local: key.local }
                        }
                        burnburn_document::DerivedOwner::Generator(id) => {
                            v1::Source::DerivedGenerator { generator: id.raw(), local: key.local }
                        }
                        // Never store derived geometry of an unknown owner as drawn (it would lose its
                        // owner silently): a new owner needs its own DTO variant first.
                        other => return Err(SaveError::Encode(format!("no file form for derived owner {other:?}"))),
                    },
                    GeometrySource::Drawn => v1::Source::Drawn,
                    other => return Err(SaveError::Encode(format!("no file form for source {other:?}"))),
                },
                subpaths: o
                    .path()
                    .subpaths()
                    .iter()
                    .map(|sp| v1::Subpath {
                        start: xy(sp.start()),
                        closed: sp.is_closed(),
                        segments: sp.segments().iter().map(segment_to_dto).collect(),
                    })
                    .collect(),
            })
        })
        .collect::<Result<Vec<_>, SaveError>>()?;
    let (operations, constraints, recipes) = crate::process_map::process_to_dto(doc)?;
    let (traces, generators) = crate::engine_map::engines_to_dto(doc);
    Ok(v1::Document {
        next_id: doc.next_id(),
        objects,
        operations,
        constraints,
        recipes,
        sketches: crate::sketch_map::sketches_to_dto(doc),
        texts: crate::text_map::texts_to_dto(doc),
        structure: crate::structure_map::structure_to_dto(doc)?,
        traces,
        generators,
    })
}

fn segment_to_dto(s: &Segment) -> v1::Segment {
    match *s {
        Segment::Line { to } => v1::Segment::Line { to: xy(to) },
        Segment::Cubic { c1, c2, to } => v1::Segment::Cubic { c1: xy(c1), c2: xy(c2), to: xy(to) },
        Segment::Arc(arc) => {
            let (u, v) = arc.semi_diameters();
            v1::Segment::Arc {
                center: xy(arc.center()),
                u: vxy(u),
                v: vxy(v),
                start: v1::Num(arc.start_parameter()),
                sweep: v1::Num(arc.sweep_parameter()),
            }
        }
    }
}

pub(crate) fn num(path: &dyn Fn() -> String, n: v1::Num) -> Result<f64, LoadError> {
    if n.0.is_finite() {
        Ok(n.0)
    } else {
        Err(LoadError::InvalidValue { path: path(), reason: "not a finite number".into() })
    }
}

pub(crate) fn point(path: &dyn Fn() -> String, p: v1::Xy) -> Result<Point, LoadError> {
    Ok(Point::new(num(&|| format!("{}[0]", path()), p[0])?, num(&|| format!("{}[1]", path()), p[1])?))
}

fn vector(path: &dyn Fn() -> String, p: v1::Xy) -> Result<Vector, LoadError> {
    point(path, p).map(Point::to_vector)
}

fn segment_from_dto(path: &dyn Fn() -> String, s: &v1::Segment) -> Result<Segment, LoadError> {
    let at = |field: &'static str| move || format!("{}.{field}", path());
    Ok(match *s {
        v1::Segment::Line { to } => Segment::Line { to: point(&at("to"), to)? },
        v1::Segment::Cubic { c1, c2, to } => {
            Segment::Cubic { c1: point(&at("c1"), c1)?, c2: point(&at("c2"), c2)?, to: point(&at("to"), to)? }
        }
        v1::Segment::Arc { center, u, v, start, sweep } => {
            let start = Angle::from_radians(num(&at("start"), start)?)
                .map_err(|e| LoadError::InvalidValue { path: at("start")(), reason: e.to_string() })?;
            let sweep = Angle::from_radians(num(&at("sweep"), sweep)?)
                .map_err(|e| LoadError::InvalidValue { path: at("sweep")(), reason: e.to_string() })?;
            let arc = EllipticalArc::new(
                point(&at("center"), center)?,
                vector(&at("u"), u)?,
                vector(&at("v"), v)?,
                start,
                sweep,
            )
            .map_err(|e| LoadError::InvalidValue { path: path(), reason: e.to_string() })?;
            Segment::Arc(arc)
        }
    })
}

/// Rebuilds the domain document (objects, sketches, process layer) of a schema-`schema` file;
/// ids, references and numbers are validated.
pub(crate) fn document_from_dto(
    dto: &v1::Document,
    schema: u32,
    asset_exists: &dyn Fn(&ContentHash) -> bool,
) -> Result<DocumentSnapshot, LoadError> {
    let mut objects = Vec::with_capacity(dto.objects.len());
    for (oi, o) in dto.objects.iter().enumerate() {
        let opath = move || format!("objects[{oi}]");
        let mut subpaths = Vec::with_capacity(o.subpaths.len());
        for (si, sp) in o.subpaths.iter().enumerate() {
            let spath = move || format!("{}.subpaths[{si}]", opath());
            let start = point(&|| format!("{}.start", spath()), sp.start)?;
            let mut segments = Vec::with_capacity(sp.segments.len());
            for (gi, seg) in sp.segments.iter().enumerate() {
                segments.push(segment_from_dto(&|| format!("{}.segments[{gi}]", spath()), seg)?);
            }
            let sub = Subpath::new(start, segments, sp.closed)
                .map_err(|e| LoadError::InvalidValue { path: spath(), reason: e.to_string() })?;
            subpaths.push(sub);
        }
        let source = match &o.source {
            v1::Source::Drawn => GeometrySource::Drawn,
            v1::Source::Imported { asset, entity } => {
                let hash = parse_asset_key(|| format!("{}.source.asset", opath()), asset)?;
                if !asset_exists(&hash) {
                    return Err(LoadError::MissingAsset(hash));
                }
                GeometrySource::Imported { asset: hash, entity: entity.clone() }
            }
            v1::Source::DerivedSketch { sketch, local } => GeometrySource::Derived(burnburn_document::DerivedKey {
                owner: burnburn_document::DerivedOwner::Sketch(burnburn_document::sketch::SketchId::from_raw(*sketch)),
                local: *local,
            }),
            v1::Source::DerivedText { text, local } => GeometrySource::Derived(burnburn_document::DerivedKey {
                owner: burnburn_document::DerivedOwner::Text(burnburn_document::text::TextId::from_raw(*text)),
                local: *local,
            }),
            v1::Source::DerivedModifier { modifier, local } => GeometrySource::Derived(burnburn_document::DerivedKey {
                owner: burnburn_document::DerivedOwner::Modifier(burnburn_document::structure::ModifierId::from_raw(
                    *modifier,
                )),
                local: *local,
            }),
            v1::Source::DerivedPart { instance, local } => GeometrySource::Derived(burnburn_document::DerivedKey {
                owner: burnburn_document::DerivedOwner::Part(burnburn_document::structure::PartInstanceId::from_raw(
                    *instance,
                )),
                local: *local,
            }),
            v1::Source::DerivedTrace { trace, local } => GeometrySource::Derived(burnburn_document::DerivedKey {
                owner: burnburn_document::DerivedOwner::Trace(burnburn_document::trace::TraceId::from_raw(*trace)),
                local: *local,
            }),
            v1::Source::DerivedGenerator { generator, local } => {
                GeometrySource::Derived(burnburn_document::DerivedKey {
                    owner: burnburn_document::DerivedOwner::Generator(
                        burnburn_document::generator::GeneratorId::from_raw(*generator),
                    ),
                    local: *local,
                })
            }
        };
        let kind = match o.kind {
            v1::Kind::Rectangle => ObjectKind::Rectangle,
            v1::Kind::Path => ObjectKind::Path,
            v1::Kind::Line => ObjectKind::Line,
            v1::Kind::Circle => ObjectKind::Circle,
            v1::Kind::Ellipse => ObjectKind::Ellipse,
            v1::Kind::Polygon => ObjectKind::Polygon,
            v1::Kind::Star => ObjectKind::Star,
            v1::Kind::Polyline => ObjectKind::Polyline,
        };
        let mut obj = GeometryObject::new(
            GeometryId::from_raw(o.id),
            o.name.clone(),
            kind,
            source,
            Path::from_subpaths(subpaths),
        )
        .with_groups(o.groups.iter().map(|g| burnburn_types::GroupId::from_raw(*g)).collect());
        if let Some(k) = &o.edge_keys {
            let keys = k.subpaths.iter().map(|s| s.iter().map(|e| burnburn_document::EdgeKey(*e)).collect()).collect();
            let keys = burnburn_document::EdgeKeys::from_parts(obj.path(), keys, k.next).map_err(|e| {
                LoadError::InvalidValue { path: format!("{}.edge_keys", opath()), reason: format!("{e:?}") }
            })?;
            obj = obj.with_edge_keys(keys).map_err(|e| LoadError::InvalidValue {
                path: format!("{}.edge_keys", opath()),
                reason: format!("{e:?}"),
            })?;
        }
        if let Some(p) = &o.parametric {
            let shape = crate::shape_map::shape_from_dto(&|| format!("{}.parametric", opath()), p)?;
            obj = obj.with_definition(shape).map_err(|e| LoadError::InvalidValue {
                path: format!("{}.parametric", opath()),
                reason: crate::shape_map::definition_reason(e),
            })?;
        }
        objects.push(obj);
    }
    // Group ids share the project id space: below next_id and never an object id.
    let object_ids: std::collections::BTreeSet<u64> = dto.objects.iter().map(|o| o.id).collect();
    for (oi, o) in dto.objects.iter().enumerate() {
        for (gi, g) in o.groups.iter().enumerate() {
            if *g >= dto.next_id || object_ids.contains(g) || o.groups[..gi].contains(g) {
                return Err(LoadError::InvalidValue {
                    path: format!("objects[{oi}].groups[{gi}]"),
                    reason: format!("group id {g} is reused or not below next_id {}", dto.next_id),
                });
            }
        }
    }
    let snapshot = DocumentSnapshot::from_parts(objects, dto.next_id)
        .map_err(|e| LoadError::InvalidValue { path: "objects".into(), reason: e.to_string() })?;
    let layer = crate::sketch_map::sketches_from_dto(&dto.sketches)?;
    let snapshot = snapshot
        .with_sketches(layer)
        .map_err(|e| LoadError::InvalidValue { path: "sketches".into(), reason: e.to_string() })?;
    let texts = crate::text_map::texts_from_dto(&dto.texts)?;
    let snapshot = snapshot
        .with_texts(texts)
        .map_err(|e| LoadError::InvalidValue { path: "texts".into(), reason: e.to_string() })?;
    let snapshot = crate::process_map::process_from_dto(dto, schema, snapshot)?;
    let snapshot = crate::structure_map::structure_from_dto(dto.structure.as_ref(), snapshot)?;
    crate::engine_map::engines_from_dto(dto, asset_exists, snapshot)
}

/// A path as DTO subpaths (part definitions with fixed geometry).
pub(crate) fn path_to_dto(p: &Path) -> Vec<v1::Subpath> {
    p.subpaths()
        .iter()
        .map(|sp| v1::Subpath {
            start: xy(sp.start()),
            closed: sp.is_closed(),
            segments: sp.segments().iter().map(segment_to_dto).collect(),
        })
        .collect()
}

/// DTO subpaths as a validated path.
pub(crate) fn path_from_dto(path: &dyn Fn() -> String, subs: &[v1::Subpath]) -> Result<Path, LoadError> {
    let mut out = Vec::with_capacity(subs.len());
    for (si, sp) in subs.iter().enumerate() {
        let spath = || format!("{}.subpaths[{si}]", path());
        let start = point(&|| format!("{}.start", spath()), sp.start)?;
        let mut segments = Vec::with_capacity(sp.segments.len());
        for (gi, seg) in sp.segments.iter().enumerate() {
            segments.push(segment_from_dto(&|| format!("{}.segments[{gi}]", spath()), seg)?);
        }
        out.push(
            Subpath::new(start, segments, sp.closed)
                .map_err(|e| LoadError::InvalidValue { path: spath(), reason: e.to_string() })?,
        );
    }
    Ok(Path::from_subpaths(out))
}
