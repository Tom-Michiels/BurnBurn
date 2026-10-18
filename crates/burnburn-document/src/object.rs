use std::sync::Arc;

use burnburn_geometry::{Affine, Bounds, GeometryError, ParametricShape, Path, Point, Segment, ShapeParams, Subpath};
use burnburn_types::{ContentHash, GeometryId, GroupId};

/// Where a geometry object came from. Kept through edits, undo, save and reopen.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum GeometrySource {
    /// Drawn in BurnBurn.
    Drawn,
    /// Imported from an asset stored in the project (content-addressed), optionally one entity of it.
    Imported { asset: ContentHash, entity: Option<String> },
    /// Output of an evaluator (sketch, …); regenerated from its owner's definition.
    Derived(crate::DerivedKey),
}

/// What the object was created as. Informational (labels, inspector); geometry is in `path`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ObjectKind {
    Rectangle,
    Path,
    Line,
    Circle,
    Ellipse,
    Polygon,
    Star,
    Polyline,
}

impl ObjectKind {
    /// What a parametric basic shape is called in the inspector and in files.
    pub fn of_shape(params: &ShapeParams) -> ObjectKind {
        match params {
            ShapeParams::Polygon { .. } => ObjectKind::Polygon,
            ShapeParams::Star { .. } | ShapeParams::DoubleStar { .. } => ObjectKind::Star,
            ShapeParams::RoundedRectangle { .. } => ObjectKind::Rectangle,
            _ => ObjectKind::Path,
        }
    }
}

/// Largest distance (mm) between a stored outline and the outline its definition resolves to.
pub const DEFINITION_TOLERANCE_MM: f64 = 1e-6;

/// Why a stored definition cannot belong to a stored outline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DefinitionError {
    /// The definition does not resolve (non-finite coordinates).
    Unresolvable(GeometryError),
    /// The outline differs from the definition's outline.
    OutlineMismatch,
}

/// Same structure (subpaths, segment kinds) and every start and end point within the tolerance.
fn outlines_agree(a: &Path, b: &Path) -> bool {
    a.subpaths().len() == b.subpaths().len()
        && a.subpaths().iter().zip(b.subpaths()).all(|(x, y)| {
            x.is_closed() == y.is_closed()
                && x.segments().len() == y.segments().len()
                && x.start().distance(y.start()) <= DEFINITION_TOLERANCE_MM
                && x.segments().iter().zip(y.segments()).all(|(p, q)| {
                    std::mem::discriminant(p) == std::mem::discriminant(q)
                        && p.end().distance(q.end()) <= DEFINITION_TOLERANCE_MM
                })
        })
}

/// One design object: stable identity, name, provenance and resolved geometry (world mm).
#[derive(Debug, Clone, PartialEq)]
pub struct GeometryObject {
    id: GeometryId,
    name: String,
    kind: ObjectKind,
    source: GeometrySource,
    path: Arc<Path>,
    /// Group chain from the outermost to the innermost group (empty: ungrouped).
    groups: Arc<[GroupId]>,
    /// Stable per-segment edge keys (DR-20). `None`: the default numbering 0..n.
    edge_keys: Option<Arc<EdgeKeys>>,
    /// The editable definition of a basic shape (W1-L2b); `path` is always its resolved outline.
    /// Any edit that changes the outline without a matching definition drops it.
    parametric: Option<Arc<ParametricShape>>,
}

/// Stable identity of one edge of a drawn object (DR-20), unique within the object and never
/// reused there. Survives node insertion before it; a split edge keeps its key on both halves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EdgeKey(pub u32);

/// The edge key of every drawn segment (including a non-degenerate implicit closing line),
/// per subpath, plus the object's next unused key.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EdgeKeys {
    keys: Vec<Vec<EdgeKey>>,
    next: u32,
}

/// Structural noise below which an implicit closing line is not a segment (as in geometry).
const CLOSING_NOISE_MM: f64 = 1e-9;

/// The drawn segments of a subpath with their start points, including a non-degenerate
/// implicit closing line (numbered `segments.len()`, as `SegmentRef` does).
pub(crate) fn drawn_segments(sp: &Subpath) -> Vec<(Point, Segment)> {
    let mut out = Vec::with_capacity(sp.segments().len() + 1);
    let mut from = sp.start();
    for seg in sp.segments() {
        out.push((from, *seg));
        from = seg.end();
    }
    if sp.is_closed() && from.distance(sp.start()) > CLOSING_NOISE_MM {
        out.push((from, Segment::Line { to: sp.start() }));
    }
    out
}

fn shape_of(path: &Path) -> Vec<usize> {
    path.subpaths().iter().map(|sp| drawn_segments(sp).len()).collect()
}

/// Why stored edge keys do not fit a path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeKeyError {
    /// Not one key per drawn segment.
    Shape,
    /// A key is duplicated in different places or not below `next`.
    Key(u32),
}

impl EdgeKeys {
    /// Keys 0..n in drawing order (what a file without stored keys gets).
    pub fn sequential(path: &Path) -> Self {
        let mut next = 0u32;
        let keys = shape_of(path)
            .into_iter()
            .map(|n| {
                (0..n)
                    .map(|_| {
                        let k = EdgeKey(next);
                        next = next.saturating_add(1);
                        k
                    })
                    .collect()
            })
            .collect();
        EdgeKeys { keys, next }
    }

    /// Stored keys (open/recovery), checked against `path`. A key may repeat only on
    /// consecutive segments of one subpath (a split edge).
    pub fn from_parts(path: &Path, keys: Vec<Vec<EdgeKey>>, next: u32) -> Result<Self, EdgeKeyError> {
        if keys.iter().map(Vec::len).collect::<Vec<_>>() != shape_of(path) {
            return Err(EdgeKeyError::Shape);
        }
        let mut seen = std::collections::BTreeSet::new();
        for sub in &keys {
            for (i, k) in sub.iter().enumerate() {
                let repeat = i > 0 && sub[i - 1] == *k;
                if k.0 >= next || (!repeat && !seen.insert(*k)) {
                    return Err(EdgeKeyError::Key(k.0));
                }
            }
        }
        Ok(EdgeKeys { keys, next })
    }

    pub fn subpaths(&self) -> &[Vec<EdgeKey>] {
        &self.keys
    }

    pub fn next(&self) -> u32 {
        self.next
    }

    fn fresh(&mut self) -> EdgeKey {
        let k = EdgeKey(self.next);
        self.next = self.next.saturating_add(1);
        k
    }

    /// Keys for `new` after an edit of `old` that may change its structure: segments that are
    /// unchanged at the start and end of a subpath keep their key, everything in between gets
    /// a new key. With a different subpath count every segment gets a new key. Conservative:
    /// a reference may break, but it never moves to another edge.
    pub(crate) fn remap(&self, old: &Path, new: &Path) -> EdgeKeys {
        let mut out = EdgeKeys { keys: Vec::new(), next: self.next };
        if shape_of(old) == shape_of(new) {
            out.keys = self.keys.clone();
            return out;
        }
        if old.subpaths().len() != new.subpaths().len() {
            out.keys = shape_of(new).into_iter().map(|n| (0..n).map(|_| out.fresh()).collect()).collect();
            return out;
        }
        for (si, (o, n)) in old.subpaths().iter().zip(new.subpaths()).enumerate() {
            let (o, n) = (drawn_segments(o), drawn_segments(n));
            let ok = self.keys.get(si).cloned().unwrap_or_default();
            let prefix = o.iter().zip(&n).take_while(|(a, b)| a == b).count();
            let max_suffix = o.len().min(n.len()) - prefix;
            let suffix = o.iter().rev().zip(n.iter().rev()).take(max_suffix).take_while(|(a, b)| a == b).count();
            let mut sub: Vec<EdgeKey> = ok.iter().take(prefix).copied().collect();
            for _ in prefix..n.len() - suffix {
                let k = out.fresh();
                sub.push(k);
            }
            sub.extend(ok.iter().skip(o.len() - suffix).copied());
            out.keys.push(sub);
        }
        out
    }

    /// Keys after splitting drawn segment `segment` of `subpath` in two: both halves keep it.
    pub(crate) fn split(&self, subpath: usize, segment: usize) -> Option<EdgeKeys> {
        let mut out = self.clone();
        let sub = out.keys.get_mut(subpath)?;
        let k = *sub.get(segment)?;
        sub.insert(segment + 1, k);
        Some(out)
    }

    /// Keys after `ReverseSubpath`: each new segment takes the key of the old segment with the
    /// same end points.
    pub(crate) fn reversed(&self, old: &Path, new: &Path, subpath: usize) -> Option<EdgeKeys> {
        let o = drawn_segments(old.subpaths().get(subpath)?);
        let n = drawn_segments(new.subpaths().get(subpath)?);
        if o.len() != n.len() {
            return None;
        }
        let ok = self.keys.get(subpath)?;
        let mut sub = Vec::with_capacity(n.len());
        for (from, seg) in &n {
            let i = o.iter().position(|(f, s)| *f == seg.end() && s.end() == *from)?;
            sub.push(*ok.get(i)?);
        }
        let mut out = self.clone();
        *out.keys.get_mut(subpath)? = sub;
        Some(out)
    }

    /// Keys after `ConvertToCubic`: every cubic of a converted arc keeps the arc's key.
    pub(crate) fn converted(&self, old: &Path, new: &Path, subpath: usize) -> Option<EdgeKeys> {
        let o = drawn_segments(old.subpaths().get(subpath)?);
        let ok = self.keys.get(subpath)?;
        let mut sub = Vec::new();
        for ((_, seg), k) in o.iter().zip(ok) {
            let n = match seg {
                Segment::Arc(a) => a.to_cubics().len(),
                _ => 1,
            };
            sub.extend(std::iter::repeat_n(*k, n));
        }
        if sub.len() != drawn_segments(new.subpaths().get(subpath)?).len() {
            return None;
        }
        let mut out = self.clone();
        *out.keys.get_mut(subpath)? = sub;
        Some(out)
    }
}

impl GeometryObject {
    pub fn new(id: GeometryId, name: String, kind: ObjectKind, source: GeometrySource, path: Path) -> Self {
        GeometryObject {
            id,
            name,
            kind,
            source,
            path: Arc::new(path),
            groups: Arc::from([]),
            edge_keys: None,
            parametric: None,
        }
    }

    /// A basic shape that stays editable: its outline is the resolved definition.
    pub fn new_parametric(
        id: GeometryId,
        name: String,
        kind: ObjectKind,
        source: GeometrySource,
        shape: ParametricShape,
    ) -> Result<Self, GeometryError> {
        let path = shape.to_path()?;
        Ok(GeometryObject { parametric: Some(Arc::new(shape)), ..GeometryObject::new(id, name, kind, source, path) })
    }

    /// Attaches a stored definition to a loaded object. The stored outline stays as it is but must
    /// be the definition's outline (within [`DEFINITION_TOLERANCE_MM`]); a file that disagrees with
    /// itself is refused instead of silently preferring one of the two.
    pub fn with_definition(self, shape: ParametricShape) -> Result<Self, DefinitionError> {
        let resolved = shape.to_path().map_err(DefinitionError::Unresolvable)?;
        if !outlines_agree(&resolved, &self.path) {
            return Err(DefinitionError::OutlineMismatch);
        }
        Ok(GeometryObject { parametric: Some(Arc::new(shape)), ..self })
    }

    /// The editable definition of a basic shape, or `None` for a plain path.
    pub fn parametric(&self) -> Option<&ParametricShape> {
        self.parametric.as_deref()
    }

    /// Stable edge keys of every drawn segment (DR-20).
    pub fn edge_keys(&self) -> EdgeKeys {
        match &self.edge_keys {
            Some(k) => (**k).clone(),
            None => EdgeKeys::sequential(&self.path),
        }
    }

    /// Stored keys only when they differ from the default numbering (io writes them then).
    pub fn stored_edge_keys(&self) -> Option<&EdgeKeys> {
        self.edge_keys.as_deref()
    }

    /// The same object with stored edge keys (io when loading); checked against the path.
    pub fn with_edge_keys(mut self, keys: EdgeKeys) -> Result<Self, EdgeKeyError> {
        let keys = EdgeKeys::from_parts(&self.path, keys.keys, keys.next)?;
        self.edge_keys = (keys != EdgeKeys::sequential(&self.path)).then(|| Arc::new(keys));
        Ok(self)
    }

    /// New geometry with explicitly maintained edge keys (node edits that know their mapping).
    pub(crate) fn with_path_and_keys(&self, path: Path, keys: EdgeKeys) -> Self {
        let keys = (keys != EdgeKeys::sequential(&path)).then(|| Arc::new(keys));
        // An outline edit without a matching definition ends the parametric form (W1-L2b).
        GeometryObject { path: Arc::new(path), edge_keys: keys, parametric: None, ..self.clone() }
    }

    pub fn id(&self) -> GeometryId {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn kind(&self) -> ObjectKind {
        self.kind
    }

    pub fn source(&self) -> &GeometrySource {
        &self.source
    }

    /// Shared, immutable geometry (cheap to hand to the scene builder or a worker).
    pub fn path(&self) -> &Arc<Path> {
        &self.path
    }

    pub fn bounds(&self) -> Option<Bounds> {
        self.path.bounds()
    }

    /// Groups containing this object, outermost first.
    pub fn groups(&self) -> &[GroupId] {
        &self.groups
    }

    /// The same object in the given group chain (outermost first); used by io when loading.
    pub fn with_groups(mut self, groups: Vec<GroupId>) -> Self {
        self.groups = Arc::from(groups);
        self
    }

    /// New geometry; edge keys follow conservatively ([`EdgeKeys::remap`]).
    pub(crate) fn with_path(&self, path: Path) -> Self {
        let keys = self.edge_keys().remap(&self.path, &path);
        self.with_path_and_keys(path, keys)
    }

    /// The same object as another parametric shape: the outline is regenerated, the kind follows
    /// the parameters and edge keys follow conservatively.
    pub(crate) fn with_shape(&self, shape: ParametricShape) -> Result<Self, GeometryError> {
        let path = shape.to_path()?;
        let moved = self.with_path(path);
        Ok(GeometryObject { kind: ObjectKind::of_shape(shape.params()), parametric: Some(Arc::new(shape)), ..moved })
    }

    /// The same object without its definition: the outline and everything else stay.
    pub(crate) fn without_definition(&self) -> Self {
        GeometryObject { parametric: None, ..self.clone() }
    }

    /// The object after an affine edit: a parametric shape stays parametric when its family
    /// allows the map, otherwise (or when the outline cannot be regenerated) it becomes a plain path.
    pub(crate) fn transformed(&self, m: &Affine) -> Self {
        if let Some(moved) = self.parametric().and_then(|shape| shape.transformed(m))
            && let Ok(object) = self.with_shape(moved)
        {
            return object;
        }
        self.with_path(self.path.transform(m))
    }

    pub(crate) fn with_name(&self, name: String) -> Self {
        GeometryObject { name, ..self.clone() }
    }
}
