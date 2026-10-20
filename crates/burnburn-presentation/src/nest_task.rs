//! The nest as a background job (W3-UI-schikken, C10): which objects become parts, the sheet they
//! go on, and the result mapped back to document objects. Pure data in, pure data out; the
//! packing itself is `burnburn_cam::nest`, which checks the cancellation token.

use burnburn_cam::nest::{
    NestError, NestParams, NestPart, PartKey, RotationSet, Stock, StockId, UnplacedReason, UtilisationBasis, nest,
};
use burnburn_document::{DocumentSnapshot, GeometrySource};
use burnburn_editor::expand_to_groups;
use burnburn_geometry::{Affine, Path, Point, ProductionTolerance, Rect};
use burnburn_types::{CancellationToken, GeometryId, GroupId, Length};

use crate::schikken::{
    NestFailure, NestRotations, NestScope, NestSheetView, NestSkipReason, NestSkippedRow, NestUnplacedReason,
    NestUnplacedRow,
};
use crate::task::TaskError;

/// Flattening accuracy of the outlines the raster is made from (mm).
const OUTLINE_TOLERANCE_MM: f64 = 0.02;
/// Candidate positions tried in total before the rest is reported as not placed.
const CANDIDATE_BUDGET: u64 = 40_000_000;

/// The parsed options of the panel.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NestSettings {
    pub padding_mm: f64,
    pub edge_buffer_mm: f64,
    pub step_mm: f64,
    pub rotations: NestRotations,
}

/// Everything the worker needs; owned, so the task outlives the window state.
#[derive(Debug, Clone)]
pub struct NestJob {
    parts: Vec<NestPart>,
    /// The document objects of each part, by `PartKey`.
    members: Vec<Vec<GeometryId>>,
    stock: Stock,
    params: NestParams,
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
    pub rotation_deg: f64,
    /// Document coordinates → sheet coordinates (which are document coordinates).
    pub transform: Affine,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NestOutput {
    pub placements: Vec<PartPlacement>,
    pub unplaced: Vec<NestUnplacedRow>,
    pub skipped: Vec<NestSkippedRow>,
    pub utilisation: f64,
}

/// Why `build_job` made no job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildError {
    NothingToNest,
    Geometry,
}

impl From<BuildError> for NestFailure {
    fn from(e: BuildError) -> Self {
        match e {
            BuildError::NothingToNest => NestFailure::NothingToNest,
            BuildError::Geometry => NestFailure::Geometry,
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
pub fn classify(snapshot: &DocumentSnapshot, scope: NestScope, selection: &[GeometryId]) -> Classified {
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
    Classified { movable, skipped }
}

/// The sheet a nest of `movable` would go on: the bed, or the bounds of the parts.
pub fn sheet_of(
    snapshot: &DocumentSnapshot,
    movable: &[Vec<GeometryId>],
    bed: Option<(f64, f64)>,
) -> Option<NestSheetView> {
    match bed {
        Some((width_mm, height_mm)) => Some(NestSheetView { width_mm, height_mm, is_bed: true }),
        None => movable
            .iter()
            .flatten()
            .filter_map(|id| snapshot.object(*id)?.bounds())
            .reduce(|a, b| a.union(b))
            .map(|b| NestSheetView { width_mm: b.width(), height_mm: b.height(), is_bed: false }),
    }
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
    let Classified { movable, skipped } = classify(snapshot, scope, selection);
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
            NestPart {
                key: PartKey(i as u32),
                outline: Path::from_subpaths(subpaths),
                count: 1,
                rotations: rotations.clone(),
                grain_deg: None,
            }
        })
        .collect();
    // Everything that stays and has an outline is in the way.
    let keep_out: Vec<Path> = snapshot
        .draw_order()
        .into_iter()
        .filter(|id| !moving.contains(id) && !snapshot.is_hidden(*id))
        .filter_map(|id| snapshot.object(id))
        .filter(|o| is_closed(o.path()))
        .map(|o| o.path().as_ref().clone())
        .collect();
    let length = |mm: f64| Length::from_mm(mm).map_err(|_| BuildError::Geometry);
    let outline = match sheet_of(snapshot, &movable, bed) {
        Some(sheet) if sheet.is_bed => Path::rectangle(
            Rect::new(Point::ORIGIN, length(sheet.width_mm)?, length(sheet.height_mm)?)
                .map_err(|_| BuildError::Geometry)?,
        ),
        Some(_) => {
            let bounds = parts
                .iter()
                .filter_map(|p| p.outline.bounds())
                .reduce(|a, b| a.union(b))
                .ok_or(BuildError::Geometry)?;
            Path::rectangle(
                Rect::new(bounds.min(), length(bounds.width())?, length(bounds.height())?)
                    .map_err(|_| BuildError::Geometry)?,
            )
        }
        None => return Err(BuildError::Geometry),
    };
    let params = NestParams {
        spacing: length(settings.padding_mm)?,
        edge_margin: length(settings.edge_buffer_mm)?,
        step: length(settings.step_mm)?,
        tolerance: ProductionTolerance::new(length(OUTLINE_TOLERANCE_MM)?).map_err(|_| BuildError::Geometry)?,
        max_candidates: CANDIDATE_BUDGET,
        basis: UtilisationBasis::StockArea,
    };
    Ok(NestJob {
        parts,
        members: movable,
        stock: Stock { id: StockId(0), outline, keep_out, grain_deg: None },
        params,
        skipped,
    })
}

/// Runs the job. Cancellation is checked inside the packing; a cancelled job gives no result.
pub fn run_nest(job: &NestJob, cancel: &CancellationToken) -> Result<NestOutput, TaskError> {
    let result = nest(&job.parts, std::slice::from_ref(&job.stock), &job.params, cancel).map_err(|e| match e {
        NestError::Cancelled => TaskError::Cancelled,
        other => TaskError::Nest(other),
    })?;
    let members = |key: PartKey| job.members.get(key.0 as usize).cloned().unwrap_or_default();
    let placements = result
        .placements
        .iter()
        .map(|p| PartPlacement { objects: members(p.part), rotation_deg: p.rotation_deg, transform: p.transform })
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
        })
        .collect();
    Ok(NestOutput { placements, unplaced, skipped: job.skipped.clone(), utilisation: result.utilisation.value })
}
