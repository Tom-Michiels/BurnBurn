//! Preflight (IM15): reviewable findings about one plan and its source, linked to geometry.

use std::collections::{BTreeMap, BTreeSet};

use burnburn_document::DocumentSnapshot;
use burnburn_document::process::{DitherMode, ImageRender, OperationMode, SharedEdgePolicy};
use burnburn_geometry::{Bounds, Point, Segment};
use burnburn_job::{Feature, JobPlan, MachinePoint, MachineProfile, PlanEvent, SourceRef};
use burnburn_types::{GeometryId, OperationId};

/// Errors block production; warnings are reviewable (C09).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Severity {
    Error,
    Warning,
}

/// One preflight finding. Geometric findings carry the source so the UI can zoom to it.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Finding {
    /// Burning motion of `source` leaves the bed.
    OutsideBed { source: SourceRef },
    /// Laser-off motion (travel/overscan) leaves the bed.
    TravelOutsideBed { operation: Option<OperationId> },
    /// A fill operation references an open contour; it has no inside and is not filled.
    OpenFillContour { operation: OperationId, geometry: GeometryId },
    /// Operation speed above the machine profile's maximum.
    SpeedAboveMachineMax { operation: OperationId, speed_mm_s: f64, max_mm_s: f64 },
    /// Operation power outside the profile's allowed range.
    PowerOutsideMachineRange { operation: OperationId },
    /// The same geometry is cut by more than one line operation (maybe intended: review).
    DuplicateCut { geometry: GeometryId, operations: Vec<OperationId> },
    /// Enabled operation without any geometry.
    EmptyOperation { operation: OperationId },
    /// Geometry that no operation references (it will not be produced).
    UnassignedGeometry { geometry: GeometryId },
    /// The plan was compiled for another document revision or machine profile revision.
    StalePlan,
    /// The operation needs a feature this profile does not support (for example after a
    /// machine change); recompiling for this profile fails with `UnsupportedFeature`.
    UnsupportedFeature { operation: OperationId, feature: Feature },
    /// The tabs of a Line operation cover a whole contour of `geometry` (for example after the
    /// source shrank); compiling fails with `InvalidTabs` until the tabs or the source change.
    InvalidTabs { operation: OperationId, geometry: GeometryId },
    /// These geometries of one Line operation share boundary segments exactly; with the default
    /// `SharedEdgePolicy::CutOnce` each shared segment is cut only once (IM32).
    SharedBoundaryCutOnce { operation: OperationId, geometries: Vec<GeometryId> },
    /// A pass-through image (burnt pixel for pixel, for images that are dithered already) lands on
    /// a grid of dots that is not a whole multiple of its pixels: pixels are duplicated or skipped
    /// unevenly, which spoils the dither (G6-machine, C05).
    PassThroughResampled {
        operation: OperationId,
        geometry: GeometryId,
        /// Width and height of the source window in pixels.
        source_px: (u32, u32),
        /// Width and height of the grid of dots it is burnt on.
        dots: (u32, u32),
    },
    /// `geometry` is derived output whose owner's last evaluation is not valid (a failed generator,
    /// a text whose font is missing, a sketch that does not solve): the outline on the canvas is the
    /// last valid one, not the current definition, and must never reach a job.
    BlockedGeometry { operation: OperationId, geometry: GeometryId },
}

impl Finding {
    pub fn severity(&self) -> Severity {
        match self {
            Finding::OutsideBed { .. }
            | Finding::TravelOutsideBed { .. }
            | Finding::OpenFillContour { .. }
            | Finding::SpeedAboveMachineMax { .. }
            | Finding::PowerOutsideMachineRange { .. }
            | Finding::StalePlan
            | Finding::InvalidTabs { .. }
            | Finding::UnsupportedFeature { .. }
            | Finding::BlockedGeometry { .. } => Severity::Error,
            Finding::DuplicateCut { .. }
            | Finding::EmptyOperation { .. }
            | Finding::UnassignedGeometry { .. }
            | Finding::SharedBoundaryCutOnce { .. }
            | Finding::PassThroughResampled { .. } => Severity::Warning,
        }
    }
}

/// Nominal size of a source geometry versus the size of its planned cut path (C03: without
/// kerf compensation both are equal up to the flattening tolerance; with kerf the path is
/// offset by k/2 per edge while the part keeps its nominal size).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SizeCheck {
    pub source: SourceRef,
    /// Width and height of the source geometry (exact curve bounds), mm.
    pub nominal_mm: (f64, f64),
    /// Width and height of the planned cut path for that source, mm.
    pub planned_mm: (f64, f64),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PreflightReport {
    pub findings: Vec<Finding>,
    /// One entry per (geometry, line operation) that has cuts in the plan.
    pub sizes: Vec<SizeCheck>,
}

impl PreflightReport {
    pub fn errors(&self) -> impl Iterator<Item = &Finding> {
        self.findings.iter().filter(|f| f.severity() == Severity::Error)
    }

    pub fn warnings(&self) -> impl Iterator<Item = &Finding> {
        self.findings.iter().filter(|f| f.severity() == Severity::Warning)
    }

    pub fn has_errors(&self) -> bool {
        self.errors().next().is_some()
    }
}

/// Checks `plan` (compiled from `snapshot` for `profile`). Bounds checks use the plan's exact
/// `f64` machine coordinates against the bed, without tolerance or rounding, before any
/// backend quantises (IM15).
pub fn preflight(snapshot: &DocumentSnapshot, plan: &JobPlan, profile: &MachineProfile) -> PreflightReport {
    preflight_with(snapshot, plan, profile, &crate::NoRasters)
}

/// [`preflight`] with the source pixels of image operations, for the findings that need them. An
/// image whose pixels are not there is not judged.
pub fn preflight_with(
    snapshot: &DocumentSnapshot,
    plan: &JobPlan,
    profile: &MachineProfile,
    rasters: &dyn crate::RasterSource,
) -> PreflightReport {
    let mut report = PreflightReport::default();
    let mut add = |f: Finding| {
        if !report.findings.contains(&f) {
            report.findings.push(f);
        }
    };
    let source = plan.source();
    if source.document_revision != snapshot.revision()
        || source.machine_profile != *profile.revision()
        || plan.frame() != profile.frame()
    {
        add(Finding::StalePlan);
    }

    // Motion against the bed (the bed is convex, so checking segment ends suffices).
    let bed = profile.frame();
    let mut operation = None;
    let mut at = plan.start();
    let mut planned: BTreeMap<SourceRef, Bounds> = BTreeMap::new();
    let mut grow = |src: SourceRef, a: MachinePoint, b: MachinePoint| {
        for p in [a, b] {
            let p = Point::new(p.x_mm(), p.y_mm());
            planned.entry(src).and_modify(|bb| bb.include(p)).or_insert_with(|| Bounds::from_point(p));
        }
    };
    for e in plan.events() {
        match *e {
            PlanEvent::OperationBoundary { operation: op, .. } => operation = Some(op),
            PlanEvent::Travel { to } => {
                if !bed.contains(to) {
                    add(Finding::TravelOutsideBed { operation });
                }
                at = to;
            }
            PlanEvent::Cut { to, origin } | PlanEvent::CutPower { to, origin, .. } => {
                if let Some(src) = origin {
                    grow(src, at, to);
                }
                if !bed.contains(to) || !bed.contains(at) {
                    burning_outside(&mut add, origin, operation);
                }
                at = to;
            }
            PlanEvent::ScanRun { line, origin } => {
                if !bed.contains(line.from) || !bed.contains(line.to) {
                    burning_outside(&mut add, origin, operation);
                }
                let (run_up, run_out) = line.overscan_ends();
                if !bed.contains(run_up) || !bed.contains(run_out) {
                    add(Finding::TravelOutsideBed { operation });
                }
                at = line.to;
            }
            _ => {}
        }
    }

    // Operations against capabilities and their geometry.
    let process = snapshot.process();
    let (min_power, max_power) = profile.power_range();
    let mut line_users: BTreeMap<GeometryId, Vec<OperationId>> = BTreeMap::new();
    // Guides and registration marks on a tool layer are on purpose in the document and off the job.
    let mut used: BTreeSet<GeometryId> =
        process.operations().filter(|o| o.tool().is_some()).flat_map(|o| o.targets().iter().copied()).collect();
    for op in process.operations().filter(|o| o.output_enabled()) {
        let id = op.id();
        let settings = op.settings();
        if op.targets().is_empty() {
            add(Finding::EmptyOperation { operation: id });
        }
        let speed = settings.speed.get_mm_per_s();
        if speed > profile.max_speed_mm_s() {
            add(Finding::SpeedAboveMachineMax { operation: id, speed_mm_s: speed, max_mm_s: profile.max_speed_mm_s() });
        }
        let (lo, hi) = (settings.power.min().get() / 100.0, settings.power.max().get() / 100.0);
        if lo < min_power.get() || hi > max_power.get() {
            add(Finding::PowerOutsideMachineRange { operation: id });
        }
        for &g in op.targets() {
            used.insert(g);
            if snapshot.production_blocked(g) {
                add(Finding::BlockedGeometry { operation: id, geometry: g });
            }
            match op.mode() {
                OperationMode::Fill(_) | OperationMode::OffsetFill(_) => {
                    let open = snapshot.object(g).is_some_and(|o| o.path().subpaths().iter().any(|sp| !sp.is_closed()));
                    if open {
                        add(Finding::OpenFillContour { operation: id, geometry: g });
                    }
                }
                OperationMode::Line { .. } => line_users.entry(g).or_default().push(id),
                _ => {}
            }
        }
    }
    for op in process.operations().filter(|o| o.output_enabled()) {
        add_invalid_tabs(snapshot, op, &mut add);
        for feature in crate::required_features(op) {
            if !profile.capabilities().is_supported(feature) {
                add(Finding::UnsupportedFeature { operation: op.id(), feature });
            }
        }
        if let OperationMode::Image(image) = op.mode()
            && image.render() == ImageRender::Dither(DitherMode::PassThrough)
            && let Some(source) = rasters.source_image(image.source())
        {
            for &geometry in op.targets() {
                let Some(bounds) = snapshot.object(geometry).and_then(|o| o.bounds()) else { continue };
                let (window, dots) = crate::image::pass_through_grids(&image, source, &bounds);
                // A whole number of dots per pixel keeps every pixel intact; anything else
                // duplicates some pixels more than others, or skips some.
                if dots.0 % window.0.max(1) != 0 || dots.1 % window.1.max(1) != 0 {
                    add(Finding::PassThroughResampled { operation: op.id(), geometry, source_px: window, dots });
                }
            }
        }
        if matches!(op.mode(), OperationMode::Line { .. }) && op.advanced().shared_edges == SharedEdgePolicy::CutOnce {
            let geometries = shared_boundaries(snapshot, op.targets());
            if !geometries.is_empty() {
                add(Finding::SharedBoundaryCutOnce { operation: op.id(), geometries });
            }
        }
    }
    for (geometry, operations) in line_users {
        if operations.len() > 1 {
            add(Finding::DuplicateCut { geometry, operations });
        }
    }
    for (g, _) in snapshot.objects() {
        if !used.contains(&g) {
            add(Finding::UnassignedGeometry { geometry: g });
        }
    }

    for (src, b) in planned {
        if let Some(nominal) = snapshot.object(src.geometry).and_then(|o| o.bounds()) {
            report.sizes.push(SizeCheck {
                source: src,
                nominal_mm: (nominal.width(), nominal.height()),
                planned_mm: (b.width(), b.height()),
            });
        }
    }
    report
}

/// Exact identity of an undirected straight or cubic segment (bit patterns of its points).
type SegKey = Vec<(u64, u64)>;

fn bits(p: Point) -> (u64, u64) {
    (p.x.to_bits(), p.y.to_bits())
}

/// Geometries (sorted) that share at least one segment exactly with another target. Lines and
/// cubics are compared by their points in either direction; arcs are not compared.
fn shared_boundaries(snapshot: &DocumentSnapshot, targets: &[GeometryId]) -> Vec<GeometryId> {
    let mut owners: BTreeMap<SegKey, BTreeSet<GeometryId>> = BTreeMap::new();
    for &g in targets {
        let Some(obj) = snapshot.object(g) else { continue };
        for sp in obj.path().subpaths() {
            let mut at = sp.start();
            let mut segs: Vec<SegKey> = Vec::new();
            for seg in sp.segments() {
                let pts = match *seg {
                    Segment::Line { to } => vec![at, to],
                    Segment::Cubic { c1, c2, to } => vec![at, c1, c2, to],
                    Segment::Arc(_) => Vec::new(),
                };
                at = seg.end();
                if pts.len() >= 2 {
                    segs.push(pts.into_iter().map(bits).collect());
                }
            }
            if sp.is_closed() && at != sp.start() {
                segs.push(vec![bits(at), bits(sp.start())]);
            }
            for k in segs {
                let rev: SegKey = k.iter().rev().copied().collect();
                owners.entry(k.min(rev)).or_default().insert(g);
            }
        }
    }
    let mut shared: BTreeSet<GeometryId> = BTreeSet::new();
    for gs in owners.values().filter(|gs| gs.len() > 1) {
        shared.extend(gs.iter().copied());
    }
    shared.into_iter().collect()
}

fn burning_outside(add: &mut impl FnMut(Finding), origin: Option<SourceRef>, operation: Option<OperationId>) {
    match origin {
        Some(source) => add(Finding::OutsideBed { source }),
        None => add(Finding::TravelOutsideBed { operation }),
    }
}

/// Tabs that cannot be placed on the current source geometry (C04). Uses the nominal contour
/// length (flattened at 0.01 mm, no kerf); compiling stays the authority and fails with
/// `InvalidTabs` for the same operation and geometry.
fn add_invalid_tabs(
    snapshot: &DocumentSnapshot,
    op: &burnburn_document::process::Operation,
    add: &mut impl FnMut(Finding),
) {
    for geometry in invalid_tab_geometries(snapshot, op) {
        add(Finding::InvalidTabs { operation: op.id(), geometry });
    }
}

/// The targets of Line operation `op` on which its tabs cannot be placed (they cover a whole
/// contour, or the spacing asks for too many), without compiling: the Cut Settings sheet says so
/// at the tab field as soon as the source changed.
pub fn invalid_tab_geometries(
    snapshot: &DocumentSnapshot,
    op: &burnburn_document::process::Operation,
) -> Vec<GeometryId> {
    let mut found = Vec::new();
    let (OperationMode::Line { .. }, Some(tabs)) = (op.mode(), op.advanced().line.tabs) else { return found };
    let Ok(tolerance) = burnburn_types::Length::from_mm(0.01).map(burnburn_geometry::ProductionTolerance::new) else {
        return found;
    };
    let Ok(tolerance) = tolerance else { return found };
    for &g in op.targets() {
        let Some(object) = snapshot.object(g) else { continue };
        let invalid =
            object.path().flatten(&tolerance).iter().filter(|pl| pl.closed && pl.points.len() >= 2).any(|pl| {
                let n = pl.points.len();
                let perimeter: f64 = (0..n).map(|k| pl.points[k].distance(pl.points[(k + 1) % n])).sum();
                crate::line::tab_count(&tabs, perimeter).is_none()
            });
        if invalid {
            found.push(g);
        }
    }
    found
}
