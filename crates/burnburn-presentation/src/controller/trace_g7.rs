//! The golf-7 additions to preparing a trace (G7-trace): pointing in the image, protected contours
//! and a node goal for area retraces, and keeping pieces that "ignore less than" would drop.
//! (The lens correction is one more preparation and lives with the others in `trace_prep`.)

use std::sync::Arc;

use burnburn_editor::{PointerButton, PointerPhase};
use burnburn_geometry::Point;
use burnburn_image::{ImageRecipe, QuarterTurn, RegionMask, RegionMode, RegionShape, Threshold, prepare};
use burnburn_trace::{ContourId, NodeTarget};
use burnburn_types::{CancellationToken, DocumentId};

use crate::ViewPointerEvent;
use crate::field::parse_number;
use crate::image_panel::TraceMode;
use crate::trace_pick::{PickKind, PickPicture, PrepPick, TracePick};
use crate::trace_prep::{MaskMode, PrepField, PrepIssue};

use super::trace::Tracer;
use super::trace_prep::{perspective_of, working_size};
use super::{AppController, AppUpdate, Session};

/// How near (pixels) two kept anchors have to be to name the same outline.
const SAME_ANCHOR_PX: f64 = 0.25;
/// Two clicked corners closer than this are the same corner.
const CORNER_DISTANCE_PX: f64 = 1.0;
/// Longest side of the picture to point at.
const PICTURE_SIDE: u32 = 512;
/// Largest brush radius (pixels).
const MAX_BRUSH_PX: f64 = 4096.0;

/// What pointing in the image is waiting for.
#[derive(Debug, Clone)]
pub(super) enum PickMode {
    Corners(Vec<[f64; 2]>),
    Brush { mode: MaskMode, radius_px: f64 },
}

#[derive(Debug, Clone)]
pub(super) struct PickState {
    mode: PickMode,
    surface_px: (u32, u32),
    picture: Option<PickPicture>,
    /// Millimetres per pixel of the picture on the sheet: the declared scale.
    mm_per_px: (f64, f64),
    /// The stroke the brush is drawing now, in pixels of the picture.
    stroke: Option<Vec<[f64; 2]>>,
}

impl PickState {
    /// Pixels of the picture for a point of the sheet (y up, origin at the bottom left).
    fn to_px(&self, mm: Point) -> [f64; 2] {
        [mm.x / self.mm_per_px.0, f64::from(self.surface_px.1) - mm.y / self.mm_per_px.1]
    }

    /// The sheet position of a pixel of the picture (the other way round).
    fn to_mm(&self, px: [f64; 2]) -> Point {
        Point::new(px[0] * self.mm_per_px.0, (f64::from(self.surface_px.1) - px[1]) * self.mm_per_px.1)
    }
}

/// Whether `anchor` is one of `keep` (the same outline).
pub(super) fn names_same(keep: &[[f64; 2]], anchor: [f64; 2]) -> bool {
    keep.iter().any(|k| (k[0] - anchor[0]).hypot(k[1] - anchor[1]) <= SAME_ANCHOR_PX)
}

pub(super) fn pick_view(t: &Tracer) -> Option<PrepPick> {
    let p = t.pick.as_ref()?;
    Some(PrepPick {
        kind: match &p.mode {
            PickMode::Corners(picked) => PickKind::Corners { picked: picked.clone() },
            PickMode::Brush { mode, radius_px } => PickKind::Brush { mode: *mode, radius_px: *radius_px },
        },
        surface_px: p.surface_px,
        picture: p.picture.clone(),
    })
}

/// The gray picture of the image as the tracer gets it, before the plane correction (`corrected`
/// false) or after it, plus its size. Without a declared scale nothing is prepared: the size is the
/// source's and there is no picture.
fn surface(t: &Tracer, corrected: bool) -> ((u32, u32), Option<PickPicture>, (f64, f64)) {
    let recipe = t.recipe.clone().unwrap_or_default();
    let image = ImageRecipe {
        regions: Vec::new(),
        morphology: Vec::new(),
        median_radius: 0,
        threshold: Threshold::Fixed(127),
        invert: false,
        perspective: if corrected { recipe.image.perspective.clone() } else { None },
        ..recipe.image.clone()
    };
    let size = if corrected { working_size(t, &recipe) } else { photo_size(t, &recipe) };
    match prepare(&t.source, &image, &CancellationToken::never()) {
        // The field is the signed distance to the level 127.5, so the gray value is exact.
        Ok(p) => {
            let (w, h) = (p.width(), p.height());
            let gray: Vec<u8> = p.field.iter().map(|f| (127.5 - f).round().clamp(0.0, 255.0) as u8).collect();
            ((w, h), Some(downscale(&gray, w, h)), (p.scale.mm_per_pixel_x(), p.scale.mm_per_pixel_y()))
        }
        Err(_) => (size, None, (1.0, 1.0)),
    }
}

/// Size of the photo after crop and rotation (the plane correction not yet applied).
fn photo_size(t: &Tracer, recipe: &burnburn_trace::TraceRecipe) -> (u32, u32) {
    let (w, h) = match &recipe.image.crop {
        Some(c) => (c.width, c.height),
        None => (t.source.width(), t.source.height()),
    };
    match recipe.image.rotation {
        QuarterTurn::Clockwise90 | QuarterTurn::CounterClockwise90 => (h, w),
        _ => (w, h),
    }
}

/// Box-averaged copy of `gray` whose long side is at most [`PICTURE_SIDE`].
fn downscale(gray: &[u8], w: u32, h: u32) -> PickPicture {
    let step = w.max(h).div_ceil(PICTURE_SIDE).max(1);
    let (nw, nh) = (w.div_ceil(step), h.div_ceil(step));
    let mut out = Vec::with_capacity(nw as usize * nh as usize);
    for by in 0..nh {
        for bx in 0..nw {
            let (mut sum, mut n) = (0u32, 0u32);
            for y in by * step..((by + 1) * step).min(h) {
                for x in bx * step..((bx + 1) * step).min(w) {
                    sum += u32::from(gray[(y * w + x) as usize]);
                    n += 1;
                }
            }
            out.push((sum / n.max(1)) as u8);
        }
    }
    PickPicture { width: nw, height: nh, gray8: Arc::from(out) }
}

/// The four corners in the order top-left, top-right, bottom-right, bottom-left, whichever order
/// they were clicked in (by angle around their centre, y down).
fn ordered(mut corners: Vec<[f64; 2]>) -> [[f64; 2]; 4] {
    let n = corners.len().max(1) as f64;
    let (cx, cy) = (corners.iter().map(|c| c[0]).sum::<f64>() / n, corners.iter().map(|c| c[1]).sum::<f64>() / n);
    corners.sort_by(|a, b| (a[1] - cy).atan2(a[0] - cx).total_cmp(&(b[1] - cy).atan2(b[0] - cx)));
    let mut out = [[0.0; 2]; 4];
    for (slot, c) in out.iter_mut().zip(corners) {
        *slot = c;
    }
    out
}

impl AppController {
    /// Protect (or release) a contour of the result in force from the next area retrace.
    pub(super) fn protect_contour(&mut self, document: DocumentId, id: u64, on: bool) -> AppUpdate {
        self.with_session(document, |s| {
            let Some(t) = s.tracer.as_mut() else { return AppUpdate::default() };
            let known = t.form.mode == TraceMode::Contour
                && t.result.as_ref().is_some_and(|r| r.contour(ContourId(id)).is_some());
            if !known {
                t.prep_issue = Some(PrepIssue::UnknownContour);
            } else {
                t.prep_issue = None;
                if on {
                    t.protect.insert(ContourId(id));
                } else {
                    t.protect.remove(&ContourId(id));
                }
            }
            AppUpdate::redraw(document)
        })
    }

    /// The node goal of the next area retrace; both fields empty clears it.
    pub(super) fn set_node_target(&mut self, document: DocumentId, nodes: &str, tolerance: &str) -> AppUpdate {
        self.with_session(document, |s| {
            let Some(t) = s.tracer.as_mut() else { return AppUpdate::default() };
            let base = t.recipe.as_ref().map_or(0.0, |r| r.fit.tolerance_mm);
            let parsed = parse_target(nodes, tolerance, base);
            match parsed {
                Ok(target) => {
                    t.node_target = target;
                    t.prep_issue = None;
                }
                Err(issue) => t.prep_issue = Some(issue),
            }
            AppUpdate::redraw(document)
        })
    }

    /// Never drop contour `id` of the result in force with "ignore less than" (or stop doing so).
    pub(super) fn keep_contour(&mut self, document: DocumentId, id: u64, keep: bool) -> AppUpdate {
        let mut changed = false;
        let update = self.with_session(document, |s| {
            let Some(t) = s.tracer.as_mut() else { return AppUpdate::default() };
            let anchor = t
                .result
                .as_ref()
                .filter(|_| t.form.mode == TraceMode::Contour)
                .and_then(|r| r.contour_anchor_px(ContourId(id)));
            let Some(anchor) = anchor else {
                t.prep_issue = Some(PrepIssue::UnknownContour);
                return AppUpdate::redraw(document);
            };
            t.prep_issue = None;
            changed = set_kept(t, anchor, keep);
            AppUpdate::redraw(document)
        });
        if changed { self.submit_trace(document) } else { update }
    }

    /// Bring back piece `index` of the report of what "ignore less than" dropped.
    pub(super) fn restore_removed(&mut self, document: DocumentId, index: usize) -> AppUpdate {
        let mut changed = false;
        let update = self.with_session(document, |s| {
            let Some(t) = s.tracer.as_mut() else { return AppUpdate::default() };
            let anchor = t
                .result
                .as_ref()
                .filter(|_| t.form.mode == TraceMode::Contour)
                .and_then(|r| r.removed.get(index))
                .map(|f| f.anchor_px);
            let Some(anchor) = anchor else {
                t.prep_issue = Some(PrepIssue::UnknownRemoved);
                return AppUpdate::redraw(document);
            };
            t.prep_issue = None;
            changed = set_kept(t, anchor, true);
            AppUpdate::redraw(document)
        });
        if changed { self.submit_trace(document) } else { update }
    }

    /// Pointing in the image.
    pub(super) fn trace_pick(&mut self, document: DocumentId, pick: TracePick) -> AppUpdate {
        let mut changed = false;
        let update = self.with_session(document, |s| {
            let Some(t) = s.tracer.as_mut() else { return AppUpdate::default() };
            changed = pointed(t, pick);
            AppUpdate::redraw(document)
        });
        if changed { self.submit_trace(document) } else { update }
    }
}

fn parse_target(nodes: &str, tolerance: &str, base_tolerance_mm: f64) -> Result<Option<NodeTarget>, PrepIssue> {
    if nodes.trim().is_empty() && tolerance.trim().is_empty() {
        return Ok(None);
    }
    let count = parse_number(nodes, &[]).map_err(|_| PrepIssue::InvalidNumber(PrepField::NodeLimit))?;
    if count.fract() != 0.0 || !(1.0..=1e9).contains(&count) {
        return Err(PrepIssue::OutOfRange(PrepField::NodeLimit));
    }
    let coarsest = parse_number(tolerance, &["mm"]).map_err(|_| PrepIssue::InvalidNumber(PrepField::NodeTolerance))?;
    if !coarsest.is_finite() || coarsest < base_tolerance_mm || coarsest > 100.0 {
        return Err(PrepIssue::OutOfRange(PrepField::NodeTolerance));
    }
    Ok(Some(NodeTarget { max_nodes: count as usize, max_tolerance_mm: coarsest }))
}

/// Puts `anchor` in the recipe's keep list (or takes it out). Whether the recipe changed.
fn set_kept(t: &mut Tracer, anchor: [f64; 2], keep: bool) -> bool {
    let Some(recipe) = t.recipe.as_mut() else { return false };
    let list = &mut recipe.contour.keep;
    if keep {
        if names_same(list, anchor) {
            return false;
        }
        list.push(anchor);
        true
    } else {
        let before = list.len();
        list.retain(|k| (k[0] - anchor[0]).hypot(k[1] - anchor[1]) > SAME_ANCHOR_PX);
        list.len() != before
    }
}

/// One pointing action on the tracer; whether the recipe changed (a new trace is needed).
fn pointed(t: &mut Tracer, pick: TracePick) -> bool {
    match pick {
        TracePick::End => {
            t.pick = None;
            t.prep_issue = None;
            false
        }
        TracePick::BeginCorners => {
            let (surface_px, picture, mm_per_px) = surface(t, false);
            t.pick =
                Some(PickState { mode: PickMode::Corners(Vec::new()), surface_px, picture, mm_per_px, stroke: None });
            t.prep_issue = None;
            false
        }
        TracePick::BeginBrush { mode, radius_px } => {
            if !(radius_px.is_finite() && radius_px > 0.0 && radius_px <= MAX_BRUSH_PX) {
                t.prep_issue = Some(PrepIssue::OutOfRange(PrepField::BrushRadius));
                return false;
            }
            let (surface_px, picture, mm_per_px) = surface(t, true);
            t.pick = Some(PickState {
                mode: PickMode::Brush { mode, radius_px },
                surface_px,
                picture,
                mm_per_px,
                stroke: None,
            });
            t.prep_issue = None;
            false
        }
        TracePick::Point { x, y } => {
            let brush = matches!(t.pick.as_ref().map(|p| &p.mode), Some(PickMode::Brush { .. }));
            if brush { stroke(t, vec![[x, y]]) } else { click(t, [x, y]) }
        }
        TracePick::Stroke { points } => stroke(t, points),
    }
}

fn click(t: &mut Tracer, at: [f64; 2]) -> bool {
    let Some(PickState { mode: PickMode::Corners(picked), .. }) = t.pick.as_mut() else {
        t.prep_issue = Some(PrepIssue::PickNotStarted);
        return false;
    };
    if !(at[0].is_finite() && at[1].is_finite()) {
        t.prep_issue = Some(PrepIssue::InvalidNumber(PrepField::CornerX(picked.len())));
        return false;
    }
    if picked.iter().any(|p| (p[0] - at[0]).hypot(p[1] - at[1]) < CORNER_DISTANCE_PX) {
        t.prep_issue = Some(PrepIssue::CornersTooClose);
        return false;
    }
    picked.push(at);
    t.prep_issue = None;
    if picked.len() < 4 {
        return false;
    }
    let corners = ordered(std::mem::take(picked));
    t.pick = None;
    let mut recipe = t.recipe.clone().unwrap_or_default();
    match perspective_of(corners, "", "") {
        Ok(p) => {
            recipe.image.perspective = Some(p);
            t.form = super::trace::form_of(&recipe, t.form.mode);
            t.recipe = Some(recipe);
            true
        }
        Err(issue) => {
            t.prep_issue = Some(issue);
            false
        }
    }
}

fn stroke(t: &mut Tracer, points: Vec<[f64; 2]>) -> bool {
    let Some(PickState { mode: PickMode::Brush { mode, radius_px }, .. }) = t.pick.as_ref() else {
        t.prep_issue = Some(PrepIssue::PickNotStarted);
        return false;
    };
    let (mode, radius) = (*mode, *radius_px);
    if points.is_empty() || points.iter().any(|p| !(p[0].is_finite() && p[1].is_finite())) {
        t.prep_issue = Some(PrepIssue::InvalidStroke);
        return false;
    }
    let mut recipe = t.recipe.clone().unwrap_or_default();
    let (w, h) = working_size(t, &recipe);
    let (w, h) = (f64::from(w), f64::from(h));
    let touches = points.iter().any(|p| p[0] >= -radius && p[0] <= w + radius && p[1] >= -radius && p[1] <= h + radius);
    if !touches {
        t.prep_issue = Some(PrepIssue::MaskOutsideImage);
        return false;
    }
    recipe.image.regions.push(RegionMask {
        mode: match mode {
            MaskMode::Keep => RegionMode::Include,
            MaskMode::Remove => RegionMode::Exclude,
        },
        shape: RegionShape::Stroke { points, radius },
    });
    t.form = super::trace::form_of(&recipe, t.form.mode);
    t.recipe = Some(recipe);
    t.prep_issue = None;
    true
}

// ---- pointing with the pointer of the canvas -------------------------------------------------

/// Where the picture and the marks of an open pick are on the sheet, for the scene.
pub(super) struct PickScene {
    /// Corners of the picture on the sheet: top left, top right, bottom right, bottom left.
    pub(super) quad: [Point; 4],
    pub(super) picture: PickPicture,
    /// The corners clicked so far.
    pub(super) corners: Vec<Point>,
    /// The stroke in progress.
    pub(super) stroke: Vec<Point>,
}

pub(super) fn pick_scene(t: &Tracer) -> Option<PickScene> {
    let p = t.pick.as_ref()?;
    let picture = p.picture.clone()?;
    let (w, h) = (f64::from(p.surface_px.0), f64::from(p.surface_px.1));
    let quad = [p.to_mm([0.0, 0.0]), p.to_mm([w, 0.0]), p.to_mm([w, h]), p.to_mm([0.0, h])];
    let corners = match &p.mode {
        PickMode::Corners(picked) => picked.iter().map(|c| p.to_mm(*c)).collect(),
        PickMode::Brush { .. } => Vec::new(),
    };
    let stroke = p.stroke.iter().flatten().map(|c| p.to_mm(*c)).collect();
    Some(PickScene { quad, picture, corners, stroke })
}

/// Escape puts the pointing down. Whether there was any.
pub(super) fn end_pick(s: &mut Session) -> bool {
    s.tracer.as_mut().is_some_and(|t| t.pick.take().is_some())
}

/// A point per half pixel is plenty: the stroke stays short and the recipe small.
const STROKE_STEP_PX: f64 = 0.5;

fn far_enough(last: Option<&[f64; 2]>, px: [f64; 2]) -> bool {
    last.is_none_or(|l| (l[0] - px[0]).hypot(l[1] - px[1]) >= STROKE_STEP_PX)
}

impl AppController {
    /// A pointer event of the canvas while the tracer asks for corners or a brush: it is the pointer
    /// in the picture. `None` when nothing is being pointed at (the event is an ordinary one).
    pub(super) fn trace_pointer(&mut self, document: DocumentId, event: &ViewPointerEvent) -> Option<AppUpdate> {
        let (px, brush) = {
            let s = self.sessions.get(&document)?;
            let pick = s.tracer.as_ref()?.pick.as_ref()?;
            let mm = s.camera.points_to_world(&s.viewport, event.position_pt);
            (pick.to_px(mm), matches!(pick.mode, PickMode::Brush { .. }))
        };
        let primary = event.button.is_none_or(|b| b == PointerButton::Primary);
        match (event.phase, brush) {
            (PointerPhase::Down, false) if primary => {
                Some(self.trace_pick(document, TracePick::Point { x: px[0], y: px[1] }))
            }
            (PointerPhase::Down, true) if primary => Some(self.with_session(document, |s| {
                if let Some(p) = s.tracer.as_mut().and_then(|t| t.pick.as_mut()) {
                    p.stroke = Some(vec![px]);
                }
                AppUpdate::redraw(document)
            })),
            (PointerPhase::Move, true) => Some(self.with_session(document, |s| {
                if let Some(stroke) = s.tracer.as_mut().and_then(|t| t.pick.as_mut()).and_then(|p| p.stroke.as_mut())
                    && far_enough(stroke.last(), px)
                {
                    stroke.push(px);
                }
                AppUpdate::redraw(document)
            })),
            (PointerPhase::Up, true) => {
                let points = self
                    .sessions
                    .get_mut(&document)
                    .and_then(|s| s.tracer.as_mut())
                    .and_then(|t| t.pick.as_mut())
                    .and_then(|p| p.stroke.take());
                match points {
                    Some(mut points) => {
                        if far_enough(points.last(), px) {
                            points.push(px);
                        }
                        Some(self.trace_pick(document, TracePick::Stroke { points }))
                    }
                    None => Some(AppUpdate::default()),
                }
            }
            (PointerPhase::Cancel, _) => Some(self.with_session(document, |s| {
                if let Some(p) = s.tracer.as_mut().and_then(|t| t.pick.as_mut()) {
                    p.stroke = None;
                }
                AppUpdate::redraw(document)
            })),
            // Everything else (hover, a press with another button) is eaten while pointing.
            _ => Some(AppUpdate::default()),
        }
    }
}
