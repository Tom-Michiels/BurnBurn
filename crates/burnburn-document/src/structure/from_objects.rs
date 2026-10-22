//! A part definition made from existing objects (G6-werk, S07): axis-aligned rectangles and
//! circles become parametric shapes whose size is a named parameter, everything else is kept as
//! fixed geometry. The shapes are in the part's own frame, which starts at the lower left corner of
//! the objects' union; an instance placed at [`PartFromObjects::origin`] lays them where they were.

use std::collections::BTreeMap;

use burnburn_geometry::{Affine, Bounds, Point, Vector};
use burnburn_types::Length;

use super::{Dim, NamedShape, PartDefinition, PartShape};
use crate::{GeometryObject, ObjectKind};

/// Relative difference between the outline's area and the area the plain shape would have.
const AREA_TOLERANCE: f64 = 1e-3;

/// A definition and where its frame starts in the document.
#[derive(Debug, Clone, PartialEq)]
pub struct PartFromObjects {
    pub definition: PartDefinition,
    /// Place an instance with a translation by this point and the shapes lie where the objects were.
    pub origin: Point,
}

/// Why no definition was made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartFromObjectsError {
    NoObjects,
    EmptyName,
    /// An object has no outline.
    NoGeometry,
}

/// A parameter name: the plain name for the first of its kind, numbered after that.
fn parameter(base: &str, nth: usize) -> String {
    if nth == 1 { base.to_owned() } else { format!("{base}{nth}") }
}

fn close(a: f64, b: f64, relative: f64) -> bool {
    (a - b).abs() <= relative * a.abs().max(b.abs()).max(1e-9)
}

/// The definition `name` made of `objects`, in the order given.
pub fn part_from_objects(name: &str, objects: &[&GeometryObject]) -> Result<PartFromObjects, PartFromObjectsError> {
    if objects.is_empty() {
        return Err(PartFromObjectsError::NoObjects);
    }
    let name = name.trim();
    if name.is_empty() {
        return Err(PartFromObjectsError::EmptyName);
    }
    let boxes: Vec<Bounds> =
        objects.iter().map(|o| o.bounds().ok_or(PartFromObjectsError::NoGeometry)).collect::<Result<_, _>>()?;
    let union = boxes.iter().copied().reduce(|a, b| a.union(b)).ok_or(PartFromObjectsError::NoObjects)?;
    let origin = union.min();
    let shift = Affine::translate(Vector::new(-origin.x, -origin.y)).map_err(|_| PartFromObjectsError::NoGeometry)?;
    let length = |mm: f64| Length::from_mm(mm).map_err(|_| PartFromObjectsError::NoGeometry);

    let mut parameters: BTreeMap<String, Length> = BTreeMap::new();
    let mut shapes: BTreeMap<u32, NamedShape> = BTreeMap::new();
    let (mut rectangles, mut circles) = (0usize, 0usize);
    for (index, (object, b)) in objects.iter().zip(&boxes).enumerate() {
        let key = u32::try_from(index + 1).map_err(|_| PartFromObjectsError::NoGeometry)?;
        let (w, h) = (b.width(), b.height());
        let area = object.path().signed_area().abs();
        let single = object.path().subpaths().len() == 1;
        let shape = if single && object.kind() == ObjectKind::Rectangle && close(area, w * h, AREA_TOLERANCE) {
            rectangles += 1;
            let (pw, ph) = (parameter("breedte", rectangles), parameter("hoogte", rectangles));
            parameters.insert(pw.clone(), length(w)?);
            parameters.insert(ph.clone(), length(h)?);
            PartShape::Rectangle {
                x: Dim::Mm(b.min().x - origin.x),
                y: Dim::Mm(b.min().y - origin.y),
                width: Dim::Param(pw),
                height: Dim::Param(ph),
            }
        } else if single
            && object.kind() == ObjectKind::Circle
            && close(w, h, 1e-6)
            && close(area, std::f64::consts::PI * (w / 2.0).powi(2), AREA_TOLERANCE)
        {
            circles += 1;
            let pr = parameter("straal", circles);
            parameters.insert(pr.clone(), length(w / 2.0)?);
            PartShape::Circle {
                x: Dim::Mm((b.min().x + b.max().x) / 2.0 - origin.x),
                y: Dim::Mm((b.min().y + b.max().y) / 2.0 - origin.y),
                radius: Dim::Param(pr),
            }
        } else {
            PartShape::Fixed(object.path().transform(&shift))
        };
        let shape_name = if object.name().trim().is_empty() { format!("Vorm {key}") } else { object.name().to_owned() };
        shapes.insert(key, NamedShape { name: shape_name, shape });
    }
    Ok(PartFromObjects {
        definition: PartDefinition { name: name.to_owned(), parameters, shapes, variants: BTreeMap::new() },
        origin,
    })
}
