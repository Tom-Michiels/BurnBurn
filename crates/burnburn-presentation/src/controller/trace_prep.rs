//! Preparing the image of the open tracer (G6-werk, T03): local threshold, four-point correction and
//! masks. Every action edits the recipe in force and starts a new trace of it; the source pixels are
//! never touched, and a refused action leaves the recipe as it was.

use std::collections::BTreeSet;
use std::sync::Arc;

use burnburn_image::{
    Homography, MAX_ADAPTIVE_RADIUS, Perspective, PixelRect, QuarterTurn, RegionMask, RegionMode, RegionShape, Threshold,
};
use burnburn_trace::{Anchor, CorrectionReplay, CorrectionRequest, TraceRecipe, TraceResult};
use burnburn_types::{CancellationSource, ContentHash, DocumentId, TaskId};

use crate::field::{format_mm, parse_number};
use crate::image_panel::TraceMode;
use crate::task::{TaskInput, TaskKind, TaskRequest, revision_key};
use crate::trace_prep::{
    CorrectionStatus, MaskMode, PrepCorrection, PrepField, PrepIssue, PrepMask, PrepPerspective, PrepThreshold,
    PrepTransform, TracePrepIntent, TracePrepViewModel,
};
use crate::view_model::MessageKey;

use super::trace::{Tracer, form_of};
use super::{AppController, AppUpdate, Session, error, submit};

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
        TracePrepIntent::RetraceRegion { .. } | TracePrepIntent::RemoveCorrection { .. } => {
            return Err(PrepIssue::NothingToCorrect); // handled before; never reached
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

/// The rectangle a correction is traced in, inside the image of the result.
fn region_from(rect: [&str; 4], size: (u32, u32)) -> Result<PixelRect, PrepIssue> {
    match mask_from(MaskMode::Keep, rect, size)?.shape {
        RegionShape::Rect(r) => Ok(r),
        RegionShape::Polygon(_) => Err(PrepIssue::MaskOutsideImage),
    }
}

/// `x, y; x, y` as points inside the image of the result.
fn anchors_from(text: &str, (w, h): (u32, u32)) -> Result<Vec<Anchor>, PrepIssue> {
    let mut out = Vec::new();
    for point in text.split(';').map(str::trim).filter(|p| !p.is_empty()) {
        let parts: Vec<&str> = if point.matches(',').count() == 1 {
            point.split(',').map(str::trim).collect()
        } else {
            point.split_whitespace().collect()
        };
        let [x, y] = parts[..] else { return Err(PrepIssue::InvalidNumber(PrepField::Anchors)) };
        let (x, y) = (number(x, PrepField::Anchors)?, number(y, PrepField::Anchors)?);
        if !(0.0..=f64::from(w)).contains(&x) || !(0.0..=f64::from(h)).contains(&y) {
            return Err(PrepIssue::MaskOutsideImage);
        }
        out.push(Anchor { at_px: [x, y] });
    }
    Ok(out)
}

/// The request to trace one area of the finished result again.
fn correction_request(
    t: &Tracer,
    task: TaskId,
    fields: [&str; 4],
    threshold: &str,
    anchors: &str,
) -> Result<CorrectionRequest, PrepIssue> {
    let previous = t.result.clone().filter(|_| t.form.mode == TraceMode::Contour).ok_or(PrepIssue::NothingToCorrect)?;
    let size = (previous.width_px, previous.height_px);
    let region = region_from(fields, size)?;
    let mut recipe = previous.recipe.clone();
    if !threshold.trim().is_empty() {
        recipe.image.threshold = Threshold::Fixed(whole(threshold, PrepField::RegionThreshold, (0, 255))? as u8);
    }
    let anchors = anchors_from(anchors, size)?;
    Ok(CorrectionRequest {
        task,
        previous,
        source: Arc::clone(&t.source),
        region: RegionShape::Rect(region),
        recipe,
        anchors,
        protected: BTreeSet::new(),
        node_target: None,
    })
}

fn current_key(t: &Tracer) -> Option<ContentHash> {
    t.recipe.as_ref().and_then(|r| r.key(t.source.content_hash()).ok())
}

/// A fresh trace with the stored corrections applied. Only the newest trace of the current recipe counts.
pub(super) fn corrected_finished(
    s: &mut Session,
    id: TaskId,
    replay: &CorrectionReplay,
    update: &mut AppUpdate,
    document: DocumentId,
) {
    let Some(t) = s.tracer.as_mut() else { return };
    let fresh = matches!((t.latest, current_key(t)), (Some(latest), Some(key)) if latest == id && replay.result.base_key == key);
    if fresh {
        t.result = Some(Arc::clone(&replay.result));
        t.unbound = replay.unbound.clone();
        t.failed = replay.failed.iter().map(|(i, _)| *i).collect();
        update.scenes_changed.push(document);
    }
}

/// One more area traced again on top of the result in force.
pub(super) fn region_finished(
    s: &mut Session,
    id: TaskId,
    result: Arc<TraceResult>,
    update: &mut AppUpdate,
    document: DocumentId,
) {
    let Some(t) = s.tracer.as_mut() else { return };
    let fresh = matches!((t.latest, current_key(t)), (Some(latest), Some(key)) if latest == id && result.base_key == key);
    if fresh {
        if let Some(record) = result.corrections.last() {
            t.corrections.push(record.clone());
        }
        t.result = Some(result);
        update.scenes_changed.push(document);
    }
}

impl AppController {
    pub(super) fn trace_prep_intent(&mut self, document: DocumentId, intent: TracePrepIntent) -> AppUpdate {
        match intent {
            TracePrepIntent::RetraceRegion { x, y, width, height, threshold, anchors } => {
                self.retrace_region(document, [&x, &y, &width, &height], &threshold, &anchors)
            }
            TracePrepIntent::RemoveCorrection { index } => self.remove_correction(document, index),
            other => self.prepare(document, other),
        }
    }

    fn retrace_region(&mut self, document: DocumentId, fields: [&str; 4], threshold: &str, anchors: &str) -> AppUpdate {
        let Some(id) = self.next_id::<TaskId>() else { return error(Some(document), MessageKey::TraceFailed) };
        let tasks = Arc::clone(&self.ports.tasks);
        self.with_session(document, |s| {
            let revision = s.doc.revision();
            let running = s.pending.values().any(|(kind, _)| matches!(kind, TaskKind::Trace | TaskKind::Centerline));
            let Some(t) = s.tracer.as_mut() else { return AppUpdate::default() };
            // A result that a newer trace is replacing is not what the operator sees any more.
            let request = if running {
                Err(PrepIssue::NothingToCorrect)
            } else {
                correction_request(t, id, fields, threshold, anchors)
            };
            let request = match request {
                Ok(request) => request,
                Err(issue) => {
                    t.prep_issue = Some(issue);
                    return AppUpdate::redraw(document);
                }
            };
            t.prep_issue = None;
            t.latest = Some(id);
            let cancel = CancellationSource::new();
            let task = TaskRequest {
                id,
                document,
                source: revision,
                key: revision_key(TaskKind::Trace, document, revision),
                cancel: cancel.token(),
                input: TaskInput::CorrectRegion(Box::new(request)),
            };
            submit(s, &tasks, task, cancel, document)
        })
    }

    fn remove_correction(&mut self, document: DocumentId, index: usize) -> AppUpdate {
        let mut removed = false;
        let update = self.with_session(document, |s| {
            let Some(t) = s.tracer.as_mut() else { return AppUpdate::default() };
            if index >= t.corrections.len() {
                t.prep_issue = Some(PrepIssue::UnknownMask);
            } else {
                t.corrections.remove(index);
                t.prep_issue = None;
                removed = true;
            }
            AppUpdate::redraw(document)
        });
        if removed { self.submit_trace(document) } else { update }
    }

    fn prepare(&mut self, document: DocumentId, intent: TracePrepIntent) -> AppUpdate {
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

fn corrections_view(t: &Tracer) -> Vec<PrepCorrection> {
    t.corrections
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let rect = match &c.region {
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
            let status = if let Some(u) = t.unbound.iter().find(|u| u.index == i) {
                CorrectionStatus::NeedsRelink { lost: u.lost.len(), new_in_region: u.new_in_region.len() }
            } else if t.failed.contains(&i) {
                CorrectionStatus::Failed
            } else {
                CorrectionStatus::Applied
            };
            PrepCorrection {
                x: rect.x,
                y: rect.y,
                width: rect.width,
                height: rect.height,
                threshold: match c.recipe.image.threshold {
                    Threshold::Fixed(level) => level.to_string(),
                    Threshold::Otsu => "Otsu".to_owned(),
                    Threshold::Adaptive { radius, offset } => format!("{radius}/{offset}"),
                },
                anchors: c.anchors.len(),
                status,
            }
        })
        .collect()
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
        corrections: corrections_view(t),
        issue: t.prep_issue,
        computing: s.pending.values().any(|(kind, _)| matches!(kind, TaskKind::Trace | TaskKind::Centerline)),
    })
}
