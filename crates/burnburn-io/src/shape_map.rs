//! Parametric shape DTO ↔ domain. Loading validates through the domain constructors, so a damaged
//! file names the JSON path of the offending value.

use burnburn_geometry::{Affine, ParametricShape, ShapeParams};

use crate::dto::v1::Num;
use crate::error::{LoadError, SaveError};
use crate::map::num;
use crate::shape_dto::Parametric;

fn frame_to_dto(shape: &ParametricShape) -> [Num; 6] {
    shape.frame().coefficients().map(Num)
}

pub(crate) fn shape_to_dto(shape: &ParametricShape) -> Result<Parametric, SaveError> {
    let frame = frame_to_dto(shape);
    Ok(match *shape.params() {
        ShapeParams::Polygon { sides } => Parametric::Polygon { sides, frame },
        ShapeParams::Star { points, ratio } => Parametric::Star { points, ratio: Num(ratio), frame },
        ShapeParams::DoubleStar { points, ratio } => Parametric::DoubleStar { points, ratio: Num(ratio), frame },
        ShapeParams::RoundedRectangle { width, height, radius } => {
            Parametric::RoundedRectangle { width: Num(width), height: Num(height), radius: Num(radius), frame }
        }
        // A new parameter kind needs its own file form first; never store it as another shape.
        other => return Err(SaveError::Encode(format!("no file form for shape parameters {other:?}"))),
    })
}

pub(crate) fn shape_from_dto(path: &dyn Fn() -> String, dto: &Parametric) -> Result<ParametricShape, LoadError> {
    let field = |name: &str, n: Num| num(&|| format!("{}.{name}", path()), n);
    let (params, frame) = match dto {
        Parametric::Polygon { sides, frame } => (ShapeParams::Polygon { sides: *sides }, frame),
        Parametric::Star { points, ratio, frame } => {
            (ShapeParams::Star { points: *points, ratio: field("ratio", *ratio)? }, frame)
        }
        Parametric::DoubleStar { points, ratio, frame } => {
            (ShapeParams::DoubleStar { points: *points, ratio: field("ratio", *ratio)? }, frame)
        }
        Parametric::RoundedRectangle { width, height, radius, frame } => (
            ShapeParams::RoundedRectangle {
                width: field("width", *width)?,
                height: field("height", *height)?,
                radius: field("radius", *radius)?,
            },
            frame,
        ),
    };
    let mut coefficients = [0.0; 6];
    for (i, n) in frame.iter().enumerate() {
        coefficients[i] = num(&|| format!("{}.frame[{i}]", path()), *n)?;
    }
    let invalid = |reason: String| LoadError::InvalidValue { path: path(), reason };
    let frame = Affine::new(coefficients).map_err(|e| invalid(e.to_string()))?;
    ParametricShape::new(params, frame).map_err(|e| invalid(e.to_string()))
}

pub(crate) fn definition_reason(e: burnburn_document::DefinitionError) -> String {
    format!("{e:?}")
}
