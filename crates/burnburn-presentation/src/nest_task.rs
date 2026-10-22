//! The nest as a background job (W3-UI-schikken, C10): which objects become parts, the sheet they
//! go on, and the result mapped back to document objects. Pure data in, pure data out; the
//! packing itself is `burnburn_cam::nest`, which checks the cancellation token.

use std::sync::Arc;

use burnburn_cam::nest::{
    MirrorPolicy, NestError, NestOptions, NestParams, NestPart, Packing, PartKey, RotationSet, Stock, StockId,
    UnplacedReason, UtilisationBasis, complete_sets, nest_packed, order_status, shared_edge_length_mm,
};
use burnburn_document::{DocumentSnapshot, GeometrySource};
use burnburn_editor::expand_to_groups;
use burnburn_geometry::{Affine, Bounds, Path, Point, ProductionTolerance, Rect, Vector};
use burnburn_types::{CancellationToken, GeometryId, GroupId, Length, StableId};

use crate::schikken::{
    NestFailure, NestHoles, NestMirror, NestPartGrain, NestPartStatusRow, NestRotations, NestScope, NestSheetChoice,
    NestSheetUseRow, NestSheetView, NestSkipReason, NestSkippedRow, NestUnplacedReason, NestUnplacedRow,
};
use crate::task::TaskError;

/// Flattening accuracy of the outlines the raster is made from (mm).
const OUTLINE_TOLERANCE_MM: f64 = 0.02;
/// Space between two sheets laid side by side (mm).
const SHEET_GAP_MM: f64 = 10.0;
/// Candidate positions tried in total before the rest is reported as not placed.
const CANDIDATE_BUDGET: u64 = 40_000_000;

/// The parsed options of the panel.
#[derive(Debug, Clone, PartialEq)]
pub struct NestSettings {
    pub padding_mm: f64,
    pub edge_buffer_mm: f64,
    pub step_mm: f64,
    pub rotations: NestRotations,
    pub mirror: NestMirror,
    pub sheets: u32,
    /// The closed object whose contour is the sheet (a remnant); `None`: the bed or the parts' bounds.
    pub sheet_object: Option<GeometryId>,
    /// Copies of every part without a number of its own (C12).
    pub copies: u32,
    /// The parts with their own number: the first object of the part and the copies.
    pub part_copies: Vec<(GeometryId, u32)>,
    pub holes: NestHoles,
    pub tight: bool,
    /// Pack without space so that parts share edges (C12).
    pub common_line: bool,
    /// Grain direction of the sheet; the parts keep their own X axis along it.
    pub grain_deg: Option<f64>,
    /// The parts with a grain rule of their own: the first object of the part and the rule.
    pub part_grain: Vec<(GeometryId, NestPartGrain)>,
}

impl NestSettings {
    /// How the part whose first object is `first` relates to the grain of the sheet.
    pub fn grain_of(&self, first: GeometryId) -> NestPartGrain {
        self.part_grain.iter().find(|(id, _)| *id == first).map_or(NestPartGrain::Along, |(_, g)| *g)
    }

    /// The number of copies of the part whose first object is `first`.
    pub fn copies_of(&self, first: GeometryId) -> u32 {
        self.part_copies.iter().find(|(id, _)| *id == first).map_or(self.copies, |(_, n)| *n)
    }
}

/// Everything the worker needs; owned, so the task outlives the window state.
#[derive(Debug, Clone)]
pub struct NestJob {
    parts: Vec<NestPart>,
    /// The document objects of each part, by `PartKey`.
    members: Vec<Vec<GeometryId>>,
    stocks: Vec<Stock>,
    options: NestOptions,
    params: NestParams,
    packing: Packing,
    /// Count the edges the parts share.
    common_line: bool,
    skipped: Vec<NestSkippedRow>,
}

impl NestJob {
    pub fn part_count(&self) -> usize {
        self.parts.len()
    }
}

/// One part's place on the sheet.
#[derive(Debug, Clone, PartialEq)]
pub struct PartPlacement {
    pub objects: Vec<GeometryId>,
    /// Which copy of the part this is; copy 0 is the part itself, the others are made on apply.
    pub copy: u32,
    pub rotation_deg: f64,
    /// Placed as its mirror image.
    pub mirrored: bool,
    /// Document coordinates → sheet coordinates (which are document coordinates).
    pub transform: Affine,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NestOutput {
    pub placements: Vec<PartPlacement>,
    pub unplaced: Vec<NestUnplacedRow>,
    pub skipped: Vec<NestSkippedRow>,
    pub utilisation: f64,
    pub mirrored: usize,
    pub sheets: Vec<NestSheetUseRow>,
    pub parts: Vec<NestPartStatusRow>,
    pub complete_sets: u32,
    /// Length of the edges the placed parts share (mm); `Some` only for a common-line nest.
    pub shared_mm: Option<f64>,
}

/// Why `build_job` made no job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildError {
    NothingToNest,
    Geometry,
    /// The chosen sheet is one of the objects being nested.
    SheetIsPart,
    /// The chosen sheet is gone, or is not a closed contour.
    SheetGone,
}

impl From<BuildError> for NestFailure {
    fn from(e: BuildError) -> Self {
        match e {
            BuildError::NothingToNest => NestFailure::NothingToNest,
            BuildError::Geometry => NestFailure::Geometry,
            BuildError::SheetIsPart => NestFailure::SheetIsPart,
            BuildError::SheetGone => NestFailure::SheetGone,
        }
    }
}

/// The nest failure of a task error.
pub fn failure_of(e: &NestError) -> NestFailure {
    match e {
        NestError::GridTooLarge => NestFailure::GridTooLarge,
        NestError::InvalidInput { .. } | NestError::Geometry { .. } => NestFailure::Geometry,
        _ => NestFailure::Refused,
    }
}

fn is_closed(path: &Path) -> bool {
    !path.subpaths().is_empty() && path.subpaths().iter().all(|sp| sp.is_closed())
}

/// Which objects move as parts, and which are left out (and why).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Classified {
    /// One entry per part: the members of a group move as one body.
    pub movable: Vec<Vec<GeometryId>>,
    pub skipped: Vec<NestSkippedRow>,
}

/// Sorts the objects `scope` covers into parts and skipped ones. Cheap: no outlines are copied.
pub fn classify(
    snapshot: &DocumentSnapshot,
    scope: NestScope,
    selection: &[GeometryId],
    holes: NestHoles,
) -> Classified {
    let candidates: Vec<GeometryId> = match scope {
        NestScope::Quick => snapshot.draw_order().into_iter().filter(|id| snapshot.is_selectable(*id)).collect(),
        NestScope::Selected => {
            let mut ids = expand_to_groups(snapshot, selection);
            ids.retain(|id| snapshot.object(*id).is_some());
            ids
        }
    };
    // Group the candidates into bodies: members of one group keep their relative places.
    let mut bodies: Vec<(Option<GroupId>, Vec<GeometryId>)> = Vec::new();
    for id in &candidates {
        let group = snapshot.object(*id).and_then(|o| o.groups().first().copied());
        match group.and_then(|g| bodies.iter().position(|(k, _)| *k == Some(g))) {
            Some(i) => bodies[i].1.push(*id),
            None => bodies.push((group, vec![*id])),
        }
    }
    let mut skipped: Vec<NestSkippedRow> = Vec::new();
    let mut movable: Vec<Vec<GeometryId>> = Vec::new();
    for (_, members) in bodies {
        let verdict = members.iter().find_map(|id| {
            let obj = snapshot.object(*id)?;
            if snapshot.is_locked(*id) {
                return Some(None);
            }
            if matches!(obj.source(), GeometrySource::Derived(_)) {
                return Some(Some(NestSkipReason::Derived));
            }
            if obj.path().subpaths().is_empty() {
                return Some(Some(NestSkipReason::NoGeometry));
            }
            (!is_closed(obj.path())).then_some(Some(NestSkipReason::OpenContour))
        });
        match verdict {
            None => movable.push(members),
            // Locked objects are a deliberate choice: they stay without being reported.
            Some(None) => {}
            Some(Some(reason)) => skipped.extend(members.iter().map(|m| NestSkippedRow { object: *m, reason })),
        }
    }
    if holes == NestHoles::Join && movable.len() <= MAX_HOLE_BODIES {
        movable = join_holes(snapshot, movable);
    }
    Classified { movable, skipped }
}

/// Most bodies the containment search looks at (it compares every pair).
const MAX_HOLE_BODIES: usize = 3000;
/// Clearance of an inner object from the outline that encloses it (mm).
const ENCLOSE_TOLERANCE_MM: f64 = 0.01;

/// The bodies that lie inside another body's outline join it (C10): an inner contour is a hole of
/// the part, an object inside that hole is an island of it, and all of it moves as one piece.
/// Each body joins the smallest outline that encloses all its objects, and that one in turn joins
/// the next, so a body ends up with the outermost one.
fn join_holes(snapshot: &DocumentSnapshot, movable: Vec<Vec<GeometryId>>) -> Vec<Vec<GeometryId>> {
    let area =
        |id: GeometryId| snapshot.object(id).and_then(|o| o.bounds()).map_or(f64::INFINITY, |b| b.width() * b.height());
    let mut parent: Vec<Option<usize>> = vec![None; movable.len()];
    for (i, body) in movable.iter().enumerate() {
        let encloser = movable
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != i)
            .flat_map(|(j, other)| other.iter().map(move |id| (j, *id)))
            .filter(|(_, outer)| {
                let Some(outer) = snapshot.object(*outer) else { return false };
                body.iter().all(|inner| {
                    snapshot
                        .object(*inner)
                        .is_some_and(|o| burnburn_geometry::encloses(outer.path(), o.path(), ENCLOSE_TOLERANCE_MM))
                })
            })
            .min_by(|a, b| area(a.1).total_cmp(&area(b.1)));
        parent[i] = encloser.map(|(j, _)| j);
    }
    let root_of = |mut i: usize| {
        // An enclosure is strict and so has no cycle; the bound is only a guard.
        for _ in 0..movable.len() {
            match parent[i] {
                Some(p) => i = p,
                None => break,
            }
        }
        i
    };
    let mut joined: Vec<Vec<GeometryId>> = Vec::new();
    let mut slot: Vec<Option<usize>> = vec![None; movable.len()];
    for (i, body) in movable.iter().enumerate() {
        if root_of(i) == i {
            slot[i] = Some(joined.len());
            joined.push(body.clone());
        }
    }
    for (i, body) in movable.iter().enumerate() {
        let root = root_of(i);
        if root != i
            && let Some(at) = slot[root]
        {
            joined[at].extend(body.iter().copied());
        }
    }
    joined
}

/// The contour of the chosen sheet object, taken out of the parts: in a nest of the selection it is
/// refused when it is selected itself, in a quick nest it is simply not a part.
fn take_sheet(
    snapshot: &DocumentSnapshot,
    classified: &mut Classified,
    scope: NestScope,
    sheet: GeometryId,
) -> Result<Arc<Path>, BuildError> {
    let object = snapshot.object(sheet).filter(|o| is_closed(o.path())).ok_or(BuildError::SheetGone)?;
    let is_sheet = |body: &Vec<GeometryId>| body.contains(&sheet);
    match scope {
        NestScope::Selected if classified.movable.iter().any(is_sheet) => return Err(BuildError::SheetIsPart),
        _ => classified.movable.retain(|body| !is_sheet(body)),
    }
    classified.skipped.retain(|row| row.object != sheet);
    Ok(Arc::clone(object.path()))
}

/// The closed objects of the drawing that may be the sheet of a nest.
pub fn sheet_choices(snapshot: &DocumentSnapshot) -> Vec<NestSheetChoice> {
    snapshot
        .draw_order()
        .into_iter()
        .filter(|id| snapshot.is_selectable(*id))
        .filter_map(|id| snapshot.object(id))
        .filter(|o| is_closed(o.path()))
        .map(|o| NestSheetChoice { object: o.id().raw(), name: o.name().to_owned() })
        .collect()
}

impl NestSettings {
    /// How far the sheet without a bed reaches beyond the parts' bounds: the margin the nest keeps
    /// clear, the padding and one raster step, so that what is there now fits again.
    fn allowance_mm(&self) -> f64 {
        self.edge_buffer_mm + self.padding_mm + self.step_mm
    }
}

/// The bounds of the sheet a nest of `movable` would go on: the bed, or the area of the parts
/// grown by [`NestSettings::allowance_mm`].
fn sheet_bounds(
    snapshot: &DocumentSnapshot,
    movable: &[Vec<GeometryId>],
    settings: &NestSettings,
    bed: Option<(f64, f64)>,
) -> Option<(Bounds, bool)> {
    match bed {
        Some((w, h)) => Some((Bounds::from_corners(Point::ORIGIN, Point::new(w, h)), true)),
        None => movable
            .iter()
            .flatten()
            .filter_map(|id| snapshot.object(*id)?.bounds())
            .reduce(|a, b| a.union(b))
            .map(|b| (b.expanded(settings.allowance_mm()), false)),
    }
}

/// The sheet a nest of `movable` would go on, for the panel: the contour of the chosen object, the
/// bed, or the bounds of the parts.
pub fn sheet_of(
    snapshot: &DocumentSnapshot,
    movable: &[Vec<GeometryId>],
    settings: &NestSettings,
    bed: Option<(f64, f64)>,
) -> Option<NestSheetView> {
    if let Some(id) = settings.sheet_object
        && let Some(b) = snapshot.object(id).filter(|o| is_closed(o.path())).and_then(|o| o.bounds())
    {
        return Some(NestSheetView {
            width_mm: b.width(),
            height_mm: b.height(),
            is_bed: false,
            contour: Some(id.raw()),
        });
    }
    let (b, is_bed) = sheet_bounds(snapshot, movable, settings, bed)?;
    Some(NestSheetView { width_mm: b.width(), height_mm: b.height(), is_bed, contour: None })
}

/// The turns that bring a part whose grain runs `own` degrees from its X axis along a sheet grain of
/// `sheet` degrees: the one turn that lines them up and, when the chosen rotations include a half
/// turn, its opposite. A mirrored placement flips the grain direction, so with mirroring allowed the
/// turns for the mirror image are offered as well (the nest keeps only the ones that follow the grain).
fn angled_rotations(sheet: f64, own: f64, chosen: &RotationSet, mirror: NestMirror) -> RotationSet {
    let half_turn = chosen.degrees().iter().any(|d| (d - 180.0).abs() < 1e-9);
    let mut turns = vec![sheet - own];
    if mirror == NestMirror::Allowed {
        turns.push(sheet + own);
    }
    if half_turn {
        turns.extend(turns.clone().into_iter().map(|t| t + 180.0));
    }
    // Turns are finite (the angles come from parsed numbers), so the set is never empty.
    RotationSet::new(turns).unwrap_or_else(|_| chosen.clone())
}

/// Makes the job for `scope` over `selection`. `bed` is the machine's bed (width, height in mm)
/// with its origin at the document origin; without one the sheet is the bounds of the parts.
pub fn build_job(
    snapshot: &DocumentSnapshot,
    scope: NestScope,
    selection: &[GeometryId],
    settings: &NestSettings,
    bed: Option<(f64, f64)>,
) -> Result<NestJob, BuildError> {
    let mut classified = classify(snapshot, scope, selection, settings.holes);
    let sheet_outline = match settings.sheet_object {
        Some(id) => Some(take_sheet(snapshot, &mut classified, scope, id)?),
        None => None,
    };
    let Classified { movable, skipped } = classified;
    if movable.is_empty() {
        return Err(BuildError::NothingToNest);
    }
    let rotations = RotationSet::new(settings.rotations.degrees()).map_err(|_| BuildError::Geometry)?;
    let moving: Vec<GeometryId> = movable.iter().flatten().copied().collect();
    let parts: Vec<NestPart> = movable
        .iter()
        .enumerate()
        .map(|(i, members)| {
            let subpaths = members
                .iter()
                .filter_map(|id| snapshot.object(*id))
                .flat_map(|o| o.path().subpaths().iter().cloned())
                .collect();
            let rule = members.first().map(|f| settings.grain_of(*f));
            let angle = settings.grain_deg.zip(rule.and_then(NestPartGrain::angle_degrees));
            NestPart {
                key: PartKey(i as u32),
                outline: Path::from_subpaths(subpaths),
                count: members.first().map_or(1, |first| settings.copies_of(*first)),
                rotations: match angle {
                    Some((sheet, own)) => angled_rotations(sheet, own, &rotations, settings.mirror),
                    None => rotations.clone(),
                },
                // The grain direction of the part in its own coordinates; a free part has no rule.
                grain_deg: settings.grain_deg.and_then(|_| match rule {
                    Some(NestPartGrain::Across) => Some(90.0),
                    Some(NestPartGrain::Free) => None,
                    Some(NestPartGrain::Angle(tenths)) => Some(f64::from(tenths) / 10.0),
                    _ => Some(0.0),
                }),
            }
        })
        .collect();
    // Everything that stays and has an outline is in the way.
    let keep_out: Vec<Path> = snapshot
        .draw_order()
        .into_iter()
        .filter(|id| !moving.contains(id) && Some(*id) != settings.sheet_object && !snapshot.is_hidden(*id))
        .filter_map(|id| snapshot.object(id))
        .filter(|o| is_closed(o.path()))
        .map(|o| o.path().as_ref().clone())
        .collect();
    let length = |mm: f64| Length::from_mm(mm).map_err(|_| BuildError::Geometry);
    let (sheet, _) = match &sheet_outline {
        Some(outline) => (outline.bounds().ok_or(BuildError::SheetGone)?, false),
        None => sheet_bounds(snapshot, &movable, settings, bed).ok_or(BuildError::Geometry)?,
    };
    // Further sheets lie beside the first, a fixed gap apart, in document coordinates: a placement on
    // sheet `k` is already where that sheet is.
    let stocks: Vec<Stock> = (0..settings.sheets)
        .map(|k| {
            let shift = f64::from(k) * (sheet.width() + SHEET_GAP_MM);
            let outline = match &sheet_outline {
                // A remnant keeps its own shape on every further sheet.
                Some(contour) if k == 0 => contour.as_ref().clone(),
                Some(contour) => {
                    contour.transform(&Affine::translate(Vector::new(shift, 0.0)).map_err(|_| BuildError::Geometry)?)
                }
                None => {
                    let origin = Point::new(sheet.min().x + shift, sheet.min().y);
                    let rect = Rect::new(origin, length(sheet.width())?, length(sheet.height())?)
                        .map_err(|_| BuildError::Geometry)?;
                    Path::rectangle(rect)
                }
            };
            Ok(Stock { id: StockId(k), outline, keep_out: keep_out.clone(), grain_deg: settings.grain_deg })
        })
        .collect::<Result<_, BuildError>>()?;
    let params = NestParams {
        // Common-line cutting needs the parts to touch: no space between them.
        spacing: length(if settings.common_line { 0.0 } else { settings.padding_mm })?,
        edge_margin: length(settings.edge_buffer_mm)?,
        step: length(settings.step_mm)?,
        tolerance: ProductionTolerance::new(length(OUTLINE_TOLERANCE_MM)?).map_err(|_| BuildError::Geometry)?,
        max_candidates: CANDIDATE_BUDGET,
        basis: UtilisationBasis::StockArea,
    };
    Ok(NestJob {
        parts,
        members: movable,
        stocks,
        options: NestOptions {
            mirror: match settings.mirror {
                NestMirror::Forbidden => MirrorPolicy::Forbidden,
                NestMirror::Allowed => MirrorPolicy::Allowed,
            },
        },
        params,
        packing: if settings.tight || settings.common_line { Packing::Tight } else { Packing::Grid },
        common_line: settings.common_line,
        skipped,
    })
}

/// Runs the job. Cancellation is checked inside the packing; a cancelled job gives no result.
pub fn run_nest(job: &NestJob, cancel: &CancellationToken) -> Result<NestOutput, TaskError> {
    let result =
        nest_packed(&job.parts, &job.stocks, &job.params, &job.options, job.packing, cancel).map_err(|e| match e {
            NestError::Cancelled => TaskError::Cancelled,
            other => TaskError::Nest(other),
        })?;
    let members = |key: PartKey| job.members.get(key.0 as usize).cloned().unwrap_or_default();
    let placements = result
        .placements
        .iter()
        .map(|p| PartPlacement {
            objects: members(p.part),
            copy: p.copy,
            rotation_deg: p.rotation_deg,
            mirrored: p.mirrored,
            transform: p.transform,
        })
        .collect();
    let unplaced = result
        .unplaced
        .iter()
        .map(|u| NestUnplacedRow {
            objects: members(u.part),
            reason: match u.reason {
                UnplacedReason::NoFit => NestUnplacedReason::NoFit,
                UnplacedReason::NoAllowedRotation => NestUnplacedReason::NoAllowedRotation,
                UnplacedReason::Budget => NestUnplacedReason::Budget,
            },
            count: u.count,
        })
        .collect();
    Ok(NestOutput {
        placements,
        unplaced,
        skipped: job.skipped.clone(),
        utilisation: result.utilisation.value,
        mirrored: result.placements.iter().filter(|p| p.mirrored).count(),
        sheets: result
            .stock_use
            .iter()
            .map(|u| NestSheetUseRow { net_mm2: u.net_mm2, placed_mm2: u.placed_mm2, free_mm2: u.free_mm2 })
            .collect(),
        parts: order_status(&result, &job.parts)
            .iter()
            .map(|st| NestPartStatusRow { objects: members(st.part), ordered: st.ordered, placed: st.placed })
            .collect(),
        // A set is one of every part.
        complete_sets: complete_sets(&result, &job.parts.iter().map(|p| (p.key, 1)).collect::<Vec<_>>()),
        shared_mm: job.common_line.then(|| {
            // Edges within the tolerance of the exact checks count as one line.
            shared_edge_length_mm(&job.parts, &result, &job.params.tolerance, 0.02)
        }),
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;
    use burnburn_geometry::Vector;
    use burnburn_types::StableId;

    fn mm(v: f64) -> Length {
        Length::from_mm(v).unwrap()
    }

    fn poly(points: &[(f64, f64)]) -> Path {
        Path::polyline(&points.iter().map(|&(x, y)| Point::new(x, y)).collect::<Vec<_>>(), true).unwrap()
    }

    /// A scalene triangle that may not turn, and a sheet that is its mirror image 12 % bigger: the
    /// part fits that sheet only mirrored (the same chiral fixture as the domain test).
    fn chiral_job(policy: MirrorPolicy) -> NestJob {
        let tri = [(0.0, 0.0), (40.0, 0.0), (10.0, 30.0)];
        let mirrored: Vec<(f64, f64)> = tri.iter().map(|&(x, y)| (40.0 - x, y)).collect();
        let cx = mirrored.iter().map(|p| p.0).sum::<f64>() / 3.0;
        let cy = mirrored.iter().map(|p| p.1).sum::<f64>() / 3.0;
        let big: Vec<(f64, f64)> =
            mirrored.iter().map(|&(x, y)| (cx + (x - cx) * 1.12, cy + (y - cy) * 1.12)).collect();
        NestJob {
            parts: vec![NestPart {
                key: PartKey(0),
                outline: poly(&tri),
                count: 1,
                rotations: RotationSet::new(vec![0.0]).unwrap(),
                grain_deg: None,
            }],
            members: vec![vec![GeometryId::from_raw(7)]],
            stocks: vec![Stock { id: StockId(0), outline: poly(&big), keep_out: vec![], grain_deg: None }],
            options: NestOptions { mirror: policy },
            packing: Packing::Grid,
            common_line: false,
            params: NestParams {
                spacing: mm(0.0),
                edge_margin: mm(0.0),
                step: mm(0.5),
                tolerance: ProductionTolerance::new(mm(0.01)).unwrap(),
                max_candidates: u64::MAX,
                basis: UtilisationBasis::StockArea,
            },
            skipped: Vec::new(),
        }
    }

    #[test]
    fn a_part_that_fits_only_mirrored_is_placed_mirrored_when_the_panel_allows_it_and_not_otherwise() {
        let never = CancellationToken::never();
        let refused = run_nest(&chiral_job(MirrorPolicy::Forbidden), &never).unwrap();
        assert_eq!((refused.placements.len(), refused.unplaced.len(), refused.mirrored), (0, 1, 0));
        assert_eq!(refused.complete_sets, 0);

        let allowed = run_nest(&chiral_job(MirrorPolicy::Allowed), &never).unwrap();
        assert_eq!((allowed.placements.len(), allowed.mirrored), (1, 1));
        assert!(allowed.placements[0].mirrored);
        // The move that is applied to the document really is a reflection (negative determinant).
        let t = allowed.placements[0].transform;
        let (a, b) = (t.apply_vector(Vector::new(1.0, 0.0)), t.apply_vector(Vector::new(0.0, 1.0)));
        assert!(a.x * b.y - a.y * b.x < 0.0, "{t:?}");
        assert_eq!(allowed.complete_sets, 1);
    }
}
