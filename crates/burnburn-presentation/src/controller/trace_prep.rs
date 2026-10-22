//! Preparing the image of the open tracer (G6-werk, T03): local threshold, four-point correction and
//! masks. Every action edits the recipe in force and starts a new trace of it; the source pixels are
//! never touched, and a refused action leaves the recipe as it was.

use burnburn_image::{
    Homography, MAX_ADAPTIVE_RADIUS, Perspective, PixelRect, QuarterTurn, RegionMask, RegionMode, RegionShape, Threshold,
};
use burnburn_trace::TraceRecipe;
use burnburn_types::DocumentId;

use crate::field::{format_mm, parse_number};
use crate::task::TaskKind;
use crate::trace_prep::{
    MaskMode, PrepField, PrepIssue, PrepMask, PrepPerspective, PrepThreshold, PrepTransform, TracePrepIntent,
    TracePrepViewModel,
};

use super::trace::{Tracer, form_of};
use super::{AppController, AppUpdate, Session};

/// Largest side of a corrected image (pixels): the working image is held in memory.
const MAX_CORRECTED_SIDE: u32 = 16_384;

type Corners = [[f64; 2]; 4];

fn number(text: &str, field: PrepField) -> Result<f64, PrepIssue> {
    parse_number(text, &[]).map_err(|_| PrepIssue::InvalidNumber(field))
}

/// A whole number within `range`.
fn whole(text: &str, field: PrepField, range: (u32, u32)) -> Result<u32, PrepIssue> {
    let v = number(text, field)?;
    if v.fract() != 0.0 || v < f64::from(range.0) || v > f64::from(range.1) {
        return Err(PrepIssue::OutOfRange(field));
    }
    Ok(v as u32)
}

/// Size of the working image the masks and the correction refer to.
fn working_size(t: &Tracer, recipe: &TraceRecipe) -> (u32, u32) {
    if let Some(p) = &recipe.image.perspective {
        return (p.width, p.height);
    }
    let (w, h) = match &recipe.image.crop {
        Some(c) => (c.width, c.height),
        None => (t.source.width(), t.source.height()),
    };
    match recipe.image.rotation {
        QuarterTurn::Clockwise90 | QuarterTurn::CounterClockwise90 => (h, w),
        _ => (w, h),
    }
}

fn edge(a: [f64; 2], b: [f64; 2]) -> f64 {
    (b[0] - a[0]).hypot(b[1] - a[1])
}

fn parse_corners(corners: &[[String; 2]; 4]) -> Result<Corners, PrepIssue> {
    let mut out = [[0.0; 2]; 4];
    for (i, c) in corners.iter().enumerate() {
        out[i] = [number(&c[0], PrepField::CornerX(i))?, number(&c[1], PrepField::CornerY(i))?];
    }
    Ok(out)
}

fn perspective_from(corners: &[[String; 2]; 4], width: &str, height: &str) -> Result<Perspective, PrepIssue> {
    let c = parse_corners(corners)?;
    // Empty sizes take the longer of each pair of opposite edges.
    let side = |text: &str, field: PrepField, derived: f64| match text.trim() {
        "" => Ok((derived.round() as u32).clamp(1, MAX_CORRECTED_SIDE)),
        typed => whole(typed, field, (1, MAX_CORRECTED_SIDE)),
    };
    let w = side(width, PrepField::OutWidth, edge(c[0], c[1]).max(edge(c[3], c[2])))?;
    let h = side(height, PrepField::OutHeight, edge(c[0], c[3]).max(edge(c[1], c[2])))?;
    let (wf, hf) = (f64::from(w), f64::from(h));
    let target = [[0.0, 0.0], [wf, 0.0], [wf, hf], [0.0, hf]];
    let points: Vec<([f64; 2], [f64; 2])> = c.iter().copied().zip(target).collect();
    Homography::from_points(&points).map_err(|_| PrepIssue::DegenerateCorners)?;
    Ok(Perspective { points, width: w, height: h })
}

fn mask_from(
    mode: MaskMode,
    rect: [&str; 4],
    (image_w, image_h): (u32, u32),
) -> Result<RegionMask, PrepIssue> {
    let x = whole(rect[0], PrepField::MaskX, (0, u32::MAX))?;
    let y = whole(rect[1], PrepField::MaskY, (0, u32::MAX))?;
    let width = whole(rect[2], PrepField::MaskWidth, (1, u32::MAX))?;
    let height = whole(rect[3], PrepField::MaskHeight, (1, u32::MAX))?;
    let inside = x.checked_add(width).is_some_and(|r| r <= image_w) && y.checked_add(height).is_some_and(|b| b <= image_h);
    if !inside {
        return Err(PrepIssue::MaskOutsideImage);
    }
    Ok(RegionMask {
        mode: match mode {
            MaskMode::Keep => RegionMode::Include,
            MaskMode::Remove => RegionMode::Exclude,
        },
        shape: RegionShape::Rect(PixelRect { x, y, width, height }),
    })
}

/// The recipe after `intent`, or why it was refused.
fn apply(t: &Tracer, intent: TracePrepIntent) -> Result<TraceRecipe, PrepIssue> {
    let mut recipe = t.recipe.clone().unwrap_or_default();
    match intent {
        TracePrepIntent::UseLocalThreshold { radius, offset } => {
            let radius = whole(&radius, PrepField::Radius, (1, MAX_ADAPTIVE_RADIUS))?;
            let offset = whole(&offset, PrepField::Offset, (0, 255))?;
            recipe.image.threshold = Threshold::Adaptive { radius, offset: offset as u8 };
        }
        TracePrepIntent::UseGlobalThreshold => recipe.image.threshold = Threshold::Otsu,
        TracePrepIntent::ApplyPerspective { corners, width, height } => {
            recipe.image.perspective = Some(perspective_from(&corners, &width, &height)?);
        }
        TracePrepIntent::ClearPerspective => recipe.image.perspective = None,
        TracePrepIntent::AddMask { mode, x, y, width, height } => {
            let size = working_size(t, &recipe);
            recipe.image.regions.push(mask_from(mode, [&x, &y, &width, &height], size)?);
        }
        TracePrepIntent::RemoveMask { index } => {
            if index >= recipe.image.regions.len() {
                return Err(PrepIssue::UnknownMask);
            }
            recipe.image.regions.remove(index);
        }
    }
    Ok(recipe)
}

impl AppController {
    pub(super) fn trace_prep_intent(&mut self, document: DocumentId, intent: TracePrepIntent) -> AppUpdate {
        let mut changed = false;
        let update = self.with_session(document, |s| {
            let Some(t) = s.tracer.as_mut() else { return AppUpdate::default() };
            match apply(t, intent) {
                Ok(recipe) => {
                    t.form = form_of(&recipe, t.form.mode);
                    t.recipe = Some(recipe);
                    t.issue = None;
                    t.prep_issue = None;
                    changed = true;
                }
                Err(issue) => t.prep_issue = Some(issue),
            }
            AppUpdate::redraw(document)
        });
        if changed { self.submit_trace(document) } else { update }
    }
}

fn text(v: f64) -> String {
    format_mm(v)
}

pub(super) fn view(s: &Session) -> Option<TracePrepViewModel> {
    let t = s.tracer.as_ref()?;
    let default_recipe = TraceRecipe::default();
    let recipe = t.recipe.as_ref().unwrap_or(&default_recipe);
    let (w, h) = (t.source.width(), t.source.height());
    let threshold = match recipe.image.threshold {
        Threshold::Adaptive { radius, offset } => {
            PrepThreshold { local: true, radius: radius.to_string(), offset: offset.to_string() }
        }
        _ => PrepThreshold { local: false, radius: "15".to_owned(), offset: "10".to_owned() },
    };
    let perspective = match &recipe.image.perspective {
        Some(p) => {
            let corner = |i: usize| p.points.get(i).map_or([String::new(), String::new()], |q| [text(q.0[0]), text(q.0[1])]);
            let transform = Homography::from_points(&p.points)
                .ok()
                .map(|(hom, residual_px)| PrepTransform { matrix: hom.m, residual_px });
            PrepPerspective {
                on: true,
                corners: [corner(0), corner(1), corner(2), corner(3)],
                width: p.width.to_string(),
                height: p.height.to_string(),
                transform,
            }
        }
        None => {
            let (wf, hf) = (f64::from(w), f64::from(h));
            PrepPerspective {
                on: false,
                corners: [[0.0, 0.0], [wf, 0.0], [wf, hf], [0.0, hf]].map(|c| c.map(text)),
                width: String::new(),
                height: String::new(),
                transform: None,
            }
        }
    };
    let masks = recipe
        .image
        .regions
        .iter()
        .map(|m| {
            let rect = match &m.shape {
                RegionShape::Rect(r) => *r,
                RegionShape::Polygon(points) => {
                    let (x0, x1) = points.iter().fold((f64::MAX, f64::MIN), |a, p| (a.0.min(p[0]), a.1.max(p[0])));
                    let (y0, y1) = points.iter().fold((f64::MAX, f64::MIN), |a, p| (a.0.min(p[1]), a.1.max(p[1])));
                    PixelRect {
                        x: x0.max(0.0) as u32,
                        y: y0.max(0.0) as u32,
                        width: (x1 - x0).max(0.0).ceil() as u32,
                        height: (y1 - y0).max(0.0).ceil() as u32,
                    }
                }
            };
            PrepMask {
                mode: if m.mode == RegionMode::Include { MaskMode::Keep } else { MaskMode::Remove },
                x: rect.x,
                y: rect.y,
                width: rect.width,
                height: rect.height,
            }
        })
        .collect();
    Some(TracePrepViewModel {
        source_px: (w, h),
        threshold,
        perspective,
        masks,
        issue: t.prep_issue,
        computing: s.pending.values().any(|(kind, _)| matches!(kind, TaskKind::Trace | TaskKind::Centerline)),
    })
}
