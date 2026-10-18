//! The document side of a trace (C-9): the recipe as engine parameters for
//! `burnburn_document::trace::TraceDefinition`, and a finished result as one trace command.
//!
//! Parameter names are flat paths (`image.threshold`, `fit.tolerance_mm`, …). Every recipe field
//! has a parameter, so `recipe_from_engine(recipe_to_engine(r)) == r` and the key is unchanged.
//! Parameters of another engine version are refused, never reinterpreted.

use burnburn_document::ObjectKind;
use burnburn_document::engine::{EngineRecipe, ParamValue};
use burnburn_document::trace::{TraceCommand, TraceDefinition, TraceId, TraceOutput};
use burnburn_geometry::Affine;
use burnburn_image::{
    AlphaPolicy, ImageRecipe, Morphology, Perspective, PixelRect, PixelScale, QuarterTurn, RegionMask, RegionMode,
    RegionShape, Threshold,
};
use burnburn_types::ContentHash;

use crate::{ContourSettings, DiagonalPolicy, ENGINE_VERSION, FitSettings, TraceError, TraceRecipe, TraceResult};

use ParamValue as V;

fn int(v: u32) -> ParamValue {
    V::Integer(i64::from(v))
}

fn rect(r: &PixelRect) -> Vec<ParamValue> {
    vec![int(r.x), int(r.y), int(r.width), int(r.height)]
}

fn pair(p: [f64; 2]) -> [ParamValue; 2] {
    [V::Number(p[0]), V::Number(p[1])]
}

/// The recipe as engine parameters (engine = [`ENGINE_VERSION`]).
pub fn recipe_to_engine(recipe: &TraceRecipe) -> EngineRecipe {
    let i = &recipe.image;
    let mut r = EngineRecipe::new(ENGINE_VERSION);
    if let Some(s) = &i.scale {
        r = r.with("image.scale", V::List(vec![V::Number(s.mm_per_pixel_x()), V::Number(s.mm_per_pixel_y())]));
    }
    if let Some(c) = &i.crop {
        r = r.with("image.crop", V::List(rect(c)));
    }
    let rotation = match i.rotation {
        QuarterTurn::None => "none",
        QuarterTurn::Clockwise90 => "cw90",
        QuarterTurn::Half => "half",
        QuarterTurn::CounterClockwise90 => "ccw90",
    };
    r = r.with("image.rotation", V::Text(rotation.into()));
    r = match i.alpha {
        AlphaPolicy::CompositeOver { background } => {
            r.with("image.alpha", V::Text("composite".into())).with("image.alpha_background", int(background.into()))
        }
        AlphaPolicy::Silhouette => r.with("image.alpha", V::Text("silhouette".into())),
    };
    r = r.with("image.median_radius", int(i.median_radius));
    r = match i.threshold {
        Threshold::Fixed(level) => {
            r.with("image.threshold", V::Text("fixed".into())).with("image.level", int(level.into()))
        }
        Threshold::Otsu => r.with("image.threshold", V::Text("otsu".into())),
        Threshold::Adaptive { radius, offset } => r
            .with("image.threshold", V::Text("adaptive".into()))
            .with("image.adaptive_radius", int(radius))
            .with("image.adaptive_offset", int(offset.into())),
    };
    r = r.with("image.invert", V::Bool(i.invert));
    let regions = i
        .regions
        .iter()
        .map(|m| {
            let mode = V::Text(match m.mode {
                RegionMode::Include => "include".into(),
                RegionMode::Exclude => "exclude".into(),
            });
            let mut item = vec![mode];
            match &m.shape {
                RegionShape::Rect(rc) => {
                    item.push(V::Text("rect".into()));
                    item.extend(rect(rc));
                }
                RegionShape::Polygon(points) => {
                    item.push(V::Text("polygon".into()));
                    item.extend(points.iter().flat_map(|p| pair(*p)));
                }
            }
            V::List(item)
        })
        .collect();
    r = r.with("image.regions", V::List(regions));
    let morphology = i
        .morphology
        .iter()
        .map(|m| match m {
            Morphology::Open(n) => V::List(vec![V::Text("open".into()), int(*n)]),
            Morphology::Close(n) => V::List(vec![V::Text("close".into()), int(*n)]),
        })
        .collect();
    r = r.with("image.morphology", V::List(morphology));
    if let Some(p) = &i.perspective {
        let mut item = vec![int(p.width), int(p.height)];
        for (from, to) in &p.points {
            item.push(V::List(pair(*from).into_iter().chain(pair(*to)).collect()));
        }
        r = r.with("image.perspective", V::List(item));
    }
    let diagonal = match recipe.contour.diagonal {
        DiagonalPolicy::Connect => "connect",
        DiagonalPolicy::Separate => "separate",
    };
    r.with("contour.diagonal", V::Text(diagonal.into()))
        .with("contour.min_area_mm2", V::Number(recipe.contour.min_area_mm2))
        .with("fit.tolerance_mm", V::Number(recipe.fit.tolerance_mm))
        .with("fit.corner_angle_deg", V::Number(recipe.fit.corner_angle_deg))
        .with("fit.corner_window_px", V::Number(recipe.fit.corner_window_px))
        .with("fit.curves", V::Bool(recipe.fit.curves))
}

struct Reader<'a>(&'a EngineRecipe);

impl Reader<'_> {
    fn get(&self, name: &'static str) -> Result<&ParamValue, TraceError> {
        self.0.get(name).ok_or(TraceError::InvalidRecipe(name))
    }
    fn number(&self, name: &'static str) -> Result<f64, TraceError> {
        number(name, self.get(name)?)
    }
    fn uint<T: TryFrom<i64>>(&self, name: &'static str) -> Result<T, TraceError> {
        uint(name, self.get(name)?)
    }
    fn text(&self, name: &'static str) -> Result<&str, TraceError> {
        match self.get(name)? {
            V::Text(t) => Ok(t),
            _ => Err(TraceError::InvalidRecipe(name)),
        }
    }
    fn bool(&self, name: &'static str) -> Result<bool, TraceError> {
        match self.get(name)? {
            V::Bool(b) => Ok(*b),
            _ => Err(TraceError::InvalidRecipe(name)),
        }
    }
    fn list(&self, name: &'static str) -> Result<&[ParamValue], TraceError> {
        list(name, self.get(name)?)
    }
    fn optional_list(&self, name: &'static str) -> Result<Option<&[ParamValue]>, TraceError> {
        self.0.get(name).map(|v| list(name, v)).transpose()
    }
}

fn number(name: &'static str, v: &ParamValue) -> Result<f64, TraceError> {
    match v {
        V::Number(n) if n.is_finite() => Ok(*n),
        _ => Err(TraceError::InvalidRecipe(name)),
    }
}

fn uint<T: TryFrom<i64>>(name: &'static str, v: &ParamValue) -> Result<T, TraceError> {
    match v {
        V::Integer(i) => T::try_from(*i).map_err(|_| TraceError::InvalidRecipe(name)),
        _ => Err(TraceError::InvalidRecipe(name)),
    }
}

fn list<'a>(name: &'static str, v: &'a ParamValue) -> Result<&'a [ParamValue], TraceError> {
    match v {
        V::List(items) => Ok(items),
        _ => Err(TraceError::InvalidRecipe(name)),
    }
}

fn rect_from(name: &'static str, items: &[ParamValue]) -> Result<PixelRect, TraceError> {
    match items {
        [x, y, w, h] => {
            Ok(PixelRect { x: uint(name, x)?, y: uint(name, y)?, width: uint(name, w)?, height: uint(name, h)? })
        }
        _ => Err(TraceError::InvalidRecipe(name)),
    }
}

fn region_from(item: &ParamValue) -> Result<RegionMask, TraceError> {
    const NAME: &str = "image.regions";
    let items = list(NAME, item)?;
    let (mode, kind, rest) = match items {
        [V::Text(mode), V::Text(kind), rest @ ..] => (mode.as_str(), kind.as_str(), rest),
        _ => return Err(TraceError::InvalidRecipe(NAME)),
    };
    let mode = match mode {
        "include" => RegionMode::Include,
        "exclude" => RegionMode::Exclude,
        _ => return Err(TraceError::InvalidRecipe(NAME)),
    };
    let shape = match kind {
        "rect" => RegionShape::Rect(rect_from(NAME, rest)?),
        "polygon" if rest.len() % 2 == 0 => RegionShape::Polygon(
            rest.as_chunks::<2>()
                .0
                .iter()
                .map(|[x, y]| Ok([number(NAME, x)?, number(NAME, y)?]))
                .collect::<Result<_, TraceError>>()?,
        ),
        _ => return Err(TraceError::InvalidRecipe(NAME)),
    };
    Ok(RegionMask { mode, shape })
}

fn perspective_from(items: &[ParamValue]) -> Result<Perspective, TraceError> {
    const NAME: &str = "image.perspective";
    let [w, h, points @ ..] = items else { return Err(TraceError::InvalidRecipe(NAME)) };
    let points = points
        .iter()
        .map(|p| match list(NAME, p)? {
            [a, b, c, d] => Ok(([number(NAME, a)?, number(NAME, b)?], [number(NAME, c)?, number(NAME, d)?])),
            _ => Err(TraceError::InvalidRecipe(NAME)),
        })
        .collect::<Result<_, TraceError>>()?;
    Ok(Perspective { points, width: uint(NAME, w)?, height: uint(NAME, h)? })
}

/// The recipe from engine parameters. Refuses another engine, a missing or mistyped
/// parameter and values the recipe does not accept ([`TraceRecipe::validate`]).
pub fn recipe_from_engine(recipe: &EngineRecipe) -> Result<TraceRecipe, TraceError> {
    if recipe.engine != ENGINE_VERSION {
        return Err(TraceError::InvalidRecipe("engine"));
    }
    let r = Reader(recipe);
    let scale = match r.optional_list("image.scale")? {
        None => None,
        Some([x, y]) => Some(
            PixelScale::from_mm_per_pixel(number("image.scale", x)?, number("image.scale", y)?)
                .map_err(|_| TraceError::InvalidRecipe("image.scale"))?,
        ),
        Some(_) => return Err(TraceError::InvalidRecipe("image.scale")),
    };
    let crop = r.optional_list("image.crop")?.map(|c| rect_from("image.crop", c)).transpose()?;
    let rotation = match r.text("image.rotation")? {
        "none" => QuarterTurn::None,
        "cw90" => QuarterTurn::Clockwise90,
        "half" => QuarterTurn::Half,
        "ccw90" => QuarterTurn::CounterClockwise90,
        _ => return Err(TraceError::InvalidRecipe("image.rotation")),
    };
    let alpha = match r.text("image.alpha")? {
        "composite" => AlphaPolicy::CompositeOver { background: r.uint("image.alpha_background")? },
        "silhouette" => AlphaPolicy::Silhouette,
        _ => return Err(TraceError::InvalidRecipe("image.alpha")),
    };
    let threshold = match r.text("image.threshold")? {
        "fixed" => Threshold::Fixed(r.uint("image.level")?),
        "otsu" => Threshold::Otsu,
        "adaptive" => {
            Threshold::Adaptive { radius: r.uint("image.adaptive_radius")?, offset: r.uint("image.adaptive_offset")? }
        }
        _ => return Err(TraceError::InvalidRecipe("image.threshold")),
    };
    let regions = r.list("image.regions")?.iter().map(region_from).collect::<Result<_, _>>()?;
    let morphology = r
        .list("image.morphology")?
        .iter()
        .map(|m| match list("image.morphology", m)? {
            [V::Text(op), n] if op == "open" => Ok(Morphology::Open(uint("image.morphology", n)?)),
            [V::Text(op), n] if op == "close" => Ok(Morphology::Close(uint("image.morphology", n)?)),
            _ => Err(TraceError::InvalidRecipe("image.morphology")),
        })
        .collect::<Result<_, _>>()?;
    let perspective = r.optional_list("image.perspective")?.map(perspective_from).transpose()?;
    let diagonal = match r.text("contour.diagonal")? {
        "connect" => DiagonalPolicy::Connect,
        "separate" => DiagonalPolicy::Separate,
        _ => return Err(TraceError::InvalidRecipe("contour.diagonal")),
    };
    let out = TraceRecipe {
        image: ImageRecipe {
            scale,
            crop,
            rotation,
            alpha,
            median_radius: r.uint("image.median_radius")?,
            threshold,
            invert: r.bool("image.invert")?,
            regions,
            morphology,
            perspective,
        },
        contour: ContourSettings { diagonal, min_area_mm2: r.number("contour.min_area_mm2")? },
        fit: FitSettings {
            tolerance_mm: r.number("fit.tolerance_mm")?,
            corner_angle_deg: r.number("fit.corner_angle_deg")?,
            corner_window_px: r.number("fit.corner_window_px")?,
            curves: r.bool("fit.curves")?,
        },
    };
    out.validate()?;
    Ok(out)
}

/// A finished result as one document command: `Create` for a new trace, `Replace` for
/// `existing`. `source_asset` is the project asset holding the source image bytes. Outputs are
/// the components (outer contour with its holes), keyed by the outer contour's id, trace-local.
pub fn trace_command(
    result: &TraceResult,
    source_asset: ContentHash,
    name: &str,
    placement: Affine,
    existing: Option<TraceId>,
) -> Result<TraceCommand, TraceError> {
    let definition = Box::new(TraceDefinition {
        name: name.to_owned(),
        source: source_asset,
        recipe: recipe_to_engine(&result.recipe),
        key: result.recipe_key,
        placement,
    });
    let mut outputs = Vec::with_capacity(result.components.len());
    for (i, c) in result.components.iter().enumerate() {
        let outer = result.contours.get(c.outer).ok_or(TraceError::NonFinite)?;
        let path = result.component_path(i).ok_or(TraceError::NonFinite)?;
        outputs.push(TraceOutput {
            local: outer.id.0,
            kind: ObjectKind::Path,
            path,
            name: Some(format!("{name} {}", i + 1)),
        });
    }
    Ok(match existing {
        None => TraceCommand::Create { definition, outputs },
        Some(trace) => TraceCommand::Replace { trace, definition, outputs },
    })
}
