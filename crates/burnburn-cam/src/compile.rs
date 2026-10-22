use burnburn_document::DocumentSnapshot;
use std::collections::BTreeMap;

use burnburn_document::process::{
    DitherMode, FillGrouping, ImageRender, LineOptions, Operation, OperationMode, OptimizationSettings, OrderKey,
    ProcessSettings, ScanDirection, SharedEdgePolicy,
};
use burnburn_geometry::{Bounds, OffsetError, OffsetJoin, Path, Point, Vector, math, offset};
use burnburn_job::{
    AppliedCalibration, AppliedKind, Calibration, Feature, JobPlan, JobPlanBuilder, MachinePoint, MachineProfile,
    OperationSettings, PlanEvent, PlanMode, PlanSource, PowerRatio, ScanAxis, ScanLine, SourceRef, Support,
};
use burnburn_types::{CancellationToken, GeometryId, OperationId, StableId};

use crate::fill::{Rows, ScanFrame};
use crate::image::{NoRasters, RasterSource};
use crate::line::{self, Contour, Heuristics, Order};
use crate::{
    CompileError, CompileOptions, JobAnchor, Placement, StartPoint, features, fill, image, offset_fill, overlap,
};
use crate::{features::effective_angle, source_hash};

/// Recorded in every plan's source; part of the plan id.
pub const COMPILER_VERSION: &str = concat!("burnburn-cam ", env!("CARGO_PKG_VERSION"));

/// A planned step in bed-view coordinates (before the machine frame is applied).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Step {
    Travel(Point),
    Cut {
        to: Point,
        source: GeometryId,
        /// Tab Cut Power (C04): the cut runs at this percentage of the operation's power.
        tab_percent: Option<f64>,
    },
    Scan {
        from: Point,
        to: Point,
        source: GeometryId,
    },
    /// A scan run with power ramps of `ramp_mm` at both ends.
    RampScan {
        from: Point,
        to: Point,
        ramp_mm: f64,
        source: GeometryId,
    },
    /// One laser pulse at the head position (Dots mode).
    Dot {
        source: GeometryId,
    },
    /// Grayscale run: toned gray `level` (0 = black = maximum power).
    PowerScan {
        from: Point,
        to: Point,
        level: u8,
        source: GeometryId,
    },
}

/// One block of motion with its own settings: an operation, or one of its sub-operations
/// (which run right after it on the same targets, under the parent's operation id).
struct Unit<'a> {
    op: &'a Operation,
    mode: OperationMode,
    settings: ProcessSettings,
    line: LineOptions,
    shared: SharedEdgePolicy,
    contours: Vec<Contour>,
    /// Indices into `contours` that are scanned together (fill grouping); one bucket with all
    /// of them unless the fill groups its shapes.
    buckets: Vec<Vec<usize>>,
}

/// Compiles `snapshot` for `profile` without image sources (image operations fail with
/// [`CompileError::MissingImageSource`]). See [`compile_with`].
pub fn compile(
    snapshot: &DocumentSnapshot,
    profile: &MachineProfile,
    options: &CompileOptions,
    cancel: &CancellationToken,
) -> Result<JobPlan, CompileError> {
    compile_with(snapshot, profile, options, &NoRasters, cancel)
}

/// Compiles `snapshot` for `profile` into one immutable plan. Pure and deterministic: the same
/// snapshot content, image sources, profile and options give the same [`JobPlan::id`]. Every
/// operation is checked against the profile's capability matrix first; an unsupported feature
/// is a typed error, never replaced. Checks `cancel` per operation, pass, contour and scan row;
/// a cancelled compile returns `Err(Cancelled)`.
pub fn compile_with(
    snapshot: &DocumentSnapshot,
    profile: &MachineProfile,
    options: &CompileOptions,
    rasters: &dyn RasterSource,
    cancel: &CancellationToken,
) -> Result<JobPlan, CompileError> {
    let process = snapshot.process();
    let order = process.execution_order().map_err(CompileError::Order)?;
    let mut units = Vec::new();
    // Measured values the compile used (W09), noted in the plan once it is being built.
    let mut applied: Vec<AppliedCalibration> = Vec::new();
    for id in order {
        cancel.check()?;
        let Some(op) = process.operation(id) else { continue };
        if !op.output_enabled() || op.targets().is_empty() {
            continue;
        }
        features::check(op, profile)?;
        let a = op.advanced();
        let mut blocks = vec![(op.mode(), op.settings(), a.line)];
        blocks.extend(a.sub_operations.iter().map(|s| (s.mode, s.settings, LineOptions::default())));
        for (mode, settings, line) in blocks {
            let mode = calibrated_kerf(snapshot, op, mode, profile, &mut applied)?;
            let contours = contours_for(snapshot, op, mode, options, cancel)?;
            let grouping = match mode {
                OperationMode::Fill(f) => f.grouping(),
                _ => FillGrouping::AllShapes,
            };
            let buckets = group_buckets(snapshot, &contours, grouping);
            units.push(Unit { op, mode, settings, line, shared: a.shared_edges, contours, buckets });
        }
    }

    let shift = placement_offset(&units, options.placement);
    for unit in &mut units {
        for c in &mut unit.contours {
            c.translate(shift);
        }
    }

    let frame = *profile.frame();
    let origin = MachinePoint::new(0.0, 0.0)?;
    let mut head = match options.start {
        StartPoint::MachineOrigin => frame.to_bed(origin),
        StartPoint::Bed(p) => p,
    };
    let source = PlanSource {
        document_revision: snapshot.revision(),
        source_hash: source_hash(snapshot).map_err(|_| burnburn_job::PlanError::NotFinite)?,
        machine_profile: profile.revision().clone(),
        compiler_version: COMPILER_VERSION.to_owned(),
    };
    let tolerance_mm = options.tolerance.mm();
    let optimization = process.optimization();
    let heuristics = Heuristics::from(optimization);
    let mut plan = JobPlanBuilder::new(source, frame, frame.to_machine(head)?);
    if let Some(rotary) = profile.rotary() {
        plan = plan.with_rotary(*rotary);
    }
    for a in applied {
        plan.note_calibration(a);
    }
    let calibration = profile.calibration();

    // Settings blocks are registered per unit, in unit order, before any motion: chunks of one
    // unit may be interleaved with chunks of another, but each keeps the same block.
    let mut blocks = Vec::with_capacity(units.len());
    for unit in &units {
        let block = plan_settings(unit.op.id(), unit.mode, &unit.settings, &unit.line)?;
        let settings = plan.add_settings(block)?;
        // Cross-hatch: a second scan block perpendicular to the first, run in every pass.
        let hatch = match unit.mode {
            OperationMode::Fill(f) if f.cross_hatch() => {
                let angle = (effective_angle(&f) + 90.0).rem_euclid(180.0);
                let second = OperationSettings { mode: fill_mode(&f, angle), ..block };
                Some((plan.add_settings(second)?, angle))
            }
            _ => None,
        };
        blocks.push((block, settings, hatch));
    }

    let mut focus_mm = 0.0;
    // Segments kept so far per (unit, pass), for Remove Overlapping Lines.
    let mut kept: BTreeMap<(usize, u32), overlap::Kept> = BTreeMap::new();
    for chunk in order_chunks(snapshot, &units, optimization, cancel)? {
        let unit = &units[chunk.unit];
        let (block, settings, hatch) = blocks[chunk.unit];
        let id = unit.op.id();
        for pass in 0..unit.settings.passes.get() {
            cancel.check()?;
            plan.push(PlanEvent::OperationBoundary { operation: id, settings, pass })?;
            let a = unit.op.advanced();
            let wanted = a.z_offset.map_or(0.0, |z| z.mm()) + a.z_step.map_or(0.0, |z| z.mm() * f64::from(pass));
            if wanted != focus_mm {
                plan.push(PlanEvent::FocusOffset { z_mm: wanted })?;
                focus_mm = wanted;
            }
            let (steps, overscan) = match unit.mode {
                OperationMode::Line { .. } => {
                    // The overcut is cut once, on the last pass.
                    let last_pass = pass + 1 == unit.settings.passes.get();
                    let line = if last_pass { unit.line } else { LineOptions { overcut: None, ..unit.line } };
                    let before = head;
                    let steps = line::plan(
                        &chunk.contours,
                        Order::Containment,
                        &line,
                        unit.shared,
                        &heuristics,
                        &mut head,
                        id,
                        tolerance_mm,
                        cancel,
                    )?;
                    // Common-line cutting of this operation (C12) names its own tolerance and minimal
                    // overlap; otherwise the document's Remove Overlapping Lines applies.
                    let overlap = unit
                        .op
                        .advanced()
                        .common_line
                        .map(|c| (c.tolerance_mm(), c.min_overlap_mm()))
                        .or_else(|| optimization.remove_overlapping_mm().map(|tol| (tol, 0.0)));
                    let steps = match overlap {
                        Some((tol, min_overlap)) => {
                            let kept = kept.entry((chunk.unit, pass)).or_insert_with(|| overlap::Kept::new(tol));
                            overlap::remove(steps, before, tol, min_overlap, kept, cancel)?
                        }
                        None => steps,
                    };
                    (steps, 0.0)
                }
                OperationMode::OffsetFill(_) => {
                    let none = LineOptions::default();
                    let steps = line::plan(
                        &chunk.contours,
                        Order::AsGiven,
                        &none,
                        SharedEdgePolicy::CutPerRegion,
                        &heuristics,
                        &mut head,
                        id,
                        tolerance_mm,
                        cancel,
                    )?;
                    (steps, 0.0)
                }
                OperationMode::Fill(f) => {
                    let steps = fill_steps(unit.op, &chunk, &f, effective_angle(&f), &mut head, cancel)?;
                    if let Some((second, angle)) = hatch {
                        emit(&mut plan, &frame, &steps, id, f.overscan().mm(), &block, calibration)?;
                        plan.push(PlanEvent::OperationBoundary { operation: id, settings: second, pass })?;
                        let steps = fill_steps(unit.op, &chunk, &f, angle, &mut head, cancel)?;
                        let block = OperationSettings { mode: fill_mode(&f, angle), ..block };
                        emit(&mut plan, &frame, &steps, id, f.overscan().mm(), &block, calibration)?;
                        continue;
                    }
                    (steps, f.overscan().mm())
                }
                OperationMode::Image(i) => {
                    (image::plan(&chunk.contours, &i, rasters, &mut head, id, cancel)?, i.overscan_mm())
                }
                _ => return Err(CompileError::UnsupportedMode { operation: id }),
            };
            emit(&mut plan, &frame, &steps, id, overscan, &block, calibration)?;
        }
    }
    Ok(plan.finish()?)
}

/// The contours of one unit that are planned together: one run of a unit under the settings'
/// `Order by` hierarchy. Without Group/Priority in the hierarchy a unit is a single chunk.
struct Chunk {
    unit: usize,
    contours: Vec<Contour>,
    /// Fill-grouping buckets as indices into `contours`.
    buckets: Vec<Vec<usize>>,
}

/// The chunks in execution order. The hierarchy is the sort key, most significant level first;
/// levels that are not listed never reorder or split anything, they only break ties (Layer
/// last, so units never interleave their travel). Equal keys keep unit and contour order, which
/// makes the order deterministic.
fn order_chunks(
    snapshot: &DocumentSnapshot,
    units: &[Unit<'_>],
    optimization: &OptimizationSettings,
    cancel: &CancellationToken,
) -> Result<Vec<Chunk>, CompileError> {
    let listed = optimization.order_by();
    let process = snapshot.process();
    // The value of a non-layer level for a geometry; ungrouped shapes come before groups.
    let component = |key: OrderKey, geometry: GeometryId| -> i128 {
        match key {
            OrderKey::Layer => 0,
            OrderKey::Group => snapshot
                .object(geometry)
                .and_then(|o| o.groups().first().copied())
                .map_or(0, |g| i128::from(g.raw()) + 1),
            OrderKey::Priority => i128::from(process.priority(geometry).get()),
        }
    };
    let split: Vec<OrderKey> = listed.iter().copied().filter(|k| *k != OrderKey::Layer).collect();
    let mut keyed: Vec<(Vec<i128>, usize, Chunk)> = Vec::new();
    for (ui, unit) in units.iter().enumerate() {
        cancel.check()?;
        let mut parts: BTreeMap<Vec<i128>, Vec<usize>> = BTreeMap::new();
        for (i, c) in unit.contours.iter().enumerate() {
            parts.entry(split.iter().map(|k| component(*k, c.geometry)).collect()).or_default().push(i);
        }
        if parts.is_empty() {
            parts.insert(vec![0; split.len()], Vec::new());
        }
        for (part, members) in parts {
            let mut sort: Vec<i128> = Vec::with_capacity(listed.len() + 1);
            for k in listed {
                sort.push(if *k == OrderKey::Layer {
                    ui as i128
                } else {
                    part[split.iter().position(|s| s == k).unwrap_or(0)]
                });
            }
            let local = |i: usize| members.iter().position(|m| *m == i);
            let buckets = unit
                .buckets
                .iter()
                .map(|b| b.iter().filter_map(|&i| local(i)).collect::<Vec<_>>())
                .filter(|b| !b.is_empty())
                .collect();
            let contours = members.iter().map(|&i| unit.contours[i].clone()).collect();
            keyed.push((sort, ui, Chunk { unit: ui, contours, buckets }));
        }
    }
    // Stable: equal keys keep the unit and contour order they were built in.
    keyed.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    Ok(keyed.into_iter().map(|(_, _, c)| c).collect())
}

/// Flattened contours an operation block works on (kerf-compensated for Line, rings for
/// Offset Fill), in bed view before placement.
fn contours_for(
    snapshot: &DocumentSnapshot,
    op: &Operation,
    mode: OperationMode,
    options: &CompileOptions,
    cancel: &CancellationToken,
) -> Result<Vec<Contour>, CompileError> {
    let id = op.id();
    let mut contours = Vec::new();
    for &g in op.targets() {
        cancel.check()?;
        let obj = snapshot.object(g).ok_or(CompileError::UnknownGeometry { operation: id, geometry: g })?;
        let compensated;
        let path = match mode {
            OperationMode::Line { kerf } if !kerf.is_none() => {
                compensated = kerf_path(obj.path(), kerf.width_mm(), id, g, options, cancel)?;
                &compensated
            }
            OperationMode::OffsetFill(o) => {
                contours.extend(offset_fill::rings(obj.path(), &o, g, id, &options.tolerance, cancel)?);
                continue;
            }
            _ => obj.path(),
        };
        let lock = snapshot.process().path_lock(g);
        for pl in path.flatten(&options.tolerance) {
            contours.push(Contour::new(g, pl.points, pl.closed).locked(lock));
        }
    }
    Ok(contours)
}

/// Kerf compensation: the beam centre runs `width / 2` into the waste. On the even-odd area of
/// the geometry a positive offset does exactly that — outer contours grow, holes shrink — so the
/// cut part keeps its nominal size. Miter joins keep sharp outer corners sharp.
fn kerf_path(
    path: &Path,
    width_mm: f64,
    operation: OperationId,
    geometry: GeometryId,
    options: &CompileOptions,
    cancel: &CancellationToken,
) -> Result<Path, CompileError> {
    offset(path, width_mm / 2.0, OffsetJoin::Miter, &options.tolerance, cancel).map_err(|e| match e {
        OffsetError::OpenContour { .. } => CompileError::KerfOnOpenContour { operation, geometry },
        OffsetError::Cancelled => CompileError::Cancelled,
        error => CompileError::Offset { operation, geometry, error },
    })
}

/// The scan steps of one pass of a fill at `angle_deg`, bucket by bucket (fill grouping).
fn fill_steps(
    op: &Operation,
    chunk: &Chunk,
    f: &burnburn_document::process::FillSettings,
    angle_deg: f64,
    head: &mut Point,
    cancel: &CancellationToken,
) -> Result<Vec<Step>, CompileError> {
    let rows = Rows {
        interval_mm: f.interval().mm(),
        bidirectional: f.direction() == ScanDirection::Bidirectional,
        flood: f.flood_fill(),
        ramp_mm: f.ramp_mm(),
    };
    let mut steps = Vec::new();
    for bucket in &chunk.buckets {
        let contours: Vec<&Contour> = bucket.iter().map(|&i| &chunk.contours[i]).collect();
        steps.extend(fill::plan(&contours, rows, scan_frame(angle_deg), head, op.id(), cancel)?);
    }
    Ok(steps)
}

/// Which contours are scanned together. `AllShapes`: one bucket; `Individual`: one per target
/// geometry; `Groups`: one per outermost group, an ungrouped geometry being its own bucket.
/// Buckets keep the order in which their first contour appears (target order).
fn group_buckets(snapshot: &DocumentSnapshot, contours: &[Contour], grouping: FillGrouping) -> Vec<Vec<usize>> {
    #[derive(PartialEq)]
    enum Key {
        Geometry(GeometryId),
        Group(burnburn_types::GroupId),
    }
    if grouping == FillGrouping::AllShapes {
        return vec![(0..contours.len()).collect()];
    }
    let mut buckets: Vec<(Key, Vec<usize>)> = Vec::new();
    for (i, c) in contours.iter().enumerate() {
        let key = match grouping {
            FillGrouping::Groups => snapshot
                .object(c.geometry)
                .and_then(|o| o.groups().first().copied())
                .map_or(Key::Geometry(c.geometry), Key::Group),
            _ => Key::Geometry(c.geometry),
        };
        match buckets.iter_mut().find(|(k, _)| *k == key) {
            Some((_, members)) => members.push(i),
            None => buckets.push((key, vec![i])),
        }
    }
    buckets.into_iter().map(|(_, members)| members).collect()
}

/// The plan mode of a fill scanning at `angle_deg` from bed X.
fn fill_mode(f: &burnburn_document::process::FillSettings, angle_deg: f64) -> PlanMode {
    let bidirectional = f.direction() == ScanDirection::Bidirectional;
    let interval_mm = f.interval().mm();
    if angle_deg == 0.0 {
        PlanMode::Scan { interval_mm, axis: ScanAxis::X, bidirectional }
    } else if angle_deg == 90.0 {
        PlanMode::Scan { interval_mm, axis: ScanAxis::Y, bidirectional }
    } else {
        PlanMode::AngledScan { interval_mm, angle_deg, bidirectional }
    }
}

fn scan_frame(angle_deg: f64) -> ScanFrame {
    if angle_deg == 0.0 {
        ScanFrame::X
    } else if angle_deg == 90.0 {
        ScanFrame::Y
    } else {
        let (sin, cos) = math::sin_cos(angle_deg.to_radians());
        ScanFrame::Rotated { cos, sin }
    }
}

fn plan_settings(
    operation: OperationId,
    mode: OperationMode,
    s: &ProcessSettings,
    line: &LineOptions,
) -> Result<OperationSettings, CompileError> {
    let mode = match mode {
        OperationMode::Fill(f) => fill_mode(&f, effective_angle(&f)),
        OperationMode::Line { .. } => match line.dot {
            Some(dot) => PlanMode::Dots { dwell_ms: dot.dwell_ms() },
            None => PlanMode::Line,
        },
        OperationMode::OffsetFill(_) => PlanMode::Line,
        OperationMode::Image(i) => {
            let interval_mm = i.line_interval().mm();
            let bidirectional = i.direction() == ScanDirection::Bidirectional;
            let angle_deg = i.scan_angle_deg();
            match i.render() {
                // Pass-through and grayscale have no angled scan (yet): refuse instead of ignoring.
                ImageRender::Dither(DitherMode::PassThrough) | ImageRender::Grayscale if angle_deg != 0.0 => {
                    return Err(CompileError::UnsupportedMode { operation });
                }
                ImageRender::Dither(_) if angle_deg == 0.0 => {
                    PlanMode::Scan { interval_mm, axis: ScanAxis::X, bidirectional }
                }
                ImageRender::Dither(_) if angle_deg == 90.0 => {
                    PlanMode::Scan { interval_mm, axis: ScanAxis::Y, bidirectional }
                }
                ImageRender::Dither(_) => PlanMode::AngledScan { interval_mm, angle_deg, bidirectional },
                ImageRender::Grayscale => PlanMode::GrayscaleScan { interval_mm, bidirectional },
            }
        }
        _ => return Err(CompileError::UnsupportedMode { operation }),
    };
    Ok(OperationSettings {
        mode,
        speed_mm_s: s.speed.get_mm_per_s(),
        min_power: PowerRatio::new(s.power.min().get() / 100.0)?,
        max_power: PowerRatio::new(s.power.max().get() / 100.0)?,
        air_assist: s.air_assist,
    })
}

fn emit(
    plan: &mut JobPlanBuilder,
    frame: &burnburn_job::MachineFrame,
    steps: &[Step],
    operation: OperationId,
    overscan_mm: f64,
    settings: &OperationSettings,
    calibration: &Calibration,
) -> Result<(), CompileError> {
    let origin = |geometry| Some(SourceRef { geometry, operation });
    let (lo, hi) = (settings.min_power.get(), settings.max_power.get());
    // The return rows of a bidirectional scan along a machine axis move back by the scan offset
    // measured at this speed (W09).
    let offset = scan_offset_for(settings, calibration);
    let mut shifted = false;
    for step in steps {
        match *step {
            Step::Travel(to) => {
                let to = frame.to_machine(to)?;
                if to != plan.position() {
                    plan.push(PlanEvent::Travel { to })?;
                }
            }
            Step::Cut { to, source, tab_percent: None } => {
                plan.push(PlanEvent::Cut { to: frame.to_machine(to)?, origin: origin(source) })?
            }
            Step::Cut { to, source, tab_percent: Some(percent) } => {
                let power = PowerRatio::new(hi * percent / 100.0)?;
                plan.push(PlanEvent::CutPower { to: frame.to_machine(to)?, power, origin: origin(source) })?
            }
            Step::Dot { source } => plan.push(PlanEvent::Dot { origin: origin(source) })?,
            Step::Scan { from, to, source }
            | Step::PowerScan { from, to, source, .. }
            | Step::RampScan { from, to, source, .. } => {
                let (mut from, mut to) = (frame.to_machine(from)?, frame.to_machine(to)?);
                if let Some((axis, back_mm, _)) = &offset {
                    let along = match axis {
                        ScanAxis::X => to.x_mm() - from.x_mm(),
                        ScanAxis::Y => to.y_mm() - from.y_mm(),
                    };
                    if along < 0.0 {
                        let moved = |p: MachinePoint| match axis {
                            ScanAxis::X => MachinePoint::new(p.x_mm() - back_mm, p.y_mm()),
                            ScanAxis::Y => MachinePoint::new(p.x_mm(), p.y_mm() - back_mm),
                        };
                        (from, to) = (moved(from)?, moved(to)?);
                        shifted = true;
                    }
                }
                if from != plan.position() {
                    plan.push(PlanEvent::Travel { to: from })?;
                }
                let line = ScanLine { from, to, overscan_mm };
                plan.push(match *step {
                    Step::PowerScan { level, .. } => {
                        // Black (0) burns at maximum power, white (255) would be minimum.
                        let power = PowerRatio::new(lo + (hi - lo) * (1.0 - f64::from(level) / 255.0))?;
                        PlanEvent::PowerRun { line, power, origin: origin(source) }
                    }
                    Step::RampScan { ramp_mm, .. } => PlanEvent::RampRun { line, ramp_mm, origin: origin(source) },
                    _ => PlanEvent::ScanRun { line, origin: origin(source) },
                })?;
            }
        }
    }
    if shifted && let Some((_, value_mm, proofs)) = offset {
        plan.note_calibration(AppliedCalibration {
            operation,
            kind: AppliedKind::ScanOffset { speed_mm_s: settings.speed_mm_s },
            value_mm,
            proofs,
        });
    }
    Ok(())
}

/// The scan axis and the measured offset for a bidirectional scan at this speed, if there is
/// one. Angled scans have no machine axis to shift along and get none; neither does a speed
/// outside the measured ones.
fn scan_offset_for(
    settings: &OperationSettings,
    calibration: &Calibration,
) -> Option<(ScanAxis, f64, Vec<burnburn_job::ProofRef>)> {
    let axis = match settings.mode {
        PlanMode::Scan { axis, bidirectional: true, .. } => axis,
        PlanMode::GrayscaleScan { bidirectional: true, .. } => ScanAxis::X,
        _ => return None,
    };
    let at = calibration.scan_offset_at(settings.speed_mm_s)?;
    Some((axis, at.offset_mm, at.proofs))
}

/// A Line operation without a typed kerf takes the kerf measured for the material of its recipe
/// (W09). A typed kerf is never replaced, and without a recipe or a measurement nothing changes.
/// The measured kerf needs the same capability as a typed one.
fn calibrated_kerf(
    snapshot: &DocumentSnapshot,
    op: &Operation,
    mode: OperationMode,
    profile: &MachineProfile,
    applied: &mut Vec<AppliedCalibration>,
) -> Result<OperationMode, CompileError> {
    let OperationMode::Line { kerf } = mode else { return Ok(mode) };
    if !kerf.is_none() {
        return Ok(mode);
    }
    let Some(recipe) = op.recipe().and_then(|r| snapshot.process().recipe(r.recipe)) else { return Ok(mode) };
    let Some(measured) = profile.calibration().kerf_for(recipe.material()) else { return Ok(mode) };
    if measured.kerf_mm == 0.0 {
        return Ok(mode);
    }
    let support = profile.capabilities().support(Feature::KerfCompensation);
    if support != Support::Supported {
        return Err(CompileError::UnsupportedFeature {
            operation: op.id(),
            feature: Feature::KerfCompensation,
            support,
        });
    }
    let width = burnburn_types::Length::from_mm(measured.kerf_mm)
        .ok()
        .and_then(|w| burnburn_document::process::Kerf::width(w).ok())
        .ok_or(CompileError::UnsupportedMode { operation: op.id() })?;
    applied.push(AppliedCalibration {
        operation: op.id(),
        kind: AppliedKind::Kerf { material: recipe.material().to_owned() },
        value_mm: measured.kerf_mm,
        proofs: vec![measured.proof.clone()],
    });
    Ok(OperationMode::Line { kerf: width })
}

fn placement_offset(units: &[Unit<'_>], placement: Placement) -> Vector {
    let Placement::Anchored { anchor, at } = placement else { return Vector::ZERO };
    let mut bounds: Option<Bounds> = None;
    for p in units.iter().flat_map(|j| j.contours.iter()).flat_map(|c| c.points.iter()) {
        match bounds.as_mut() {
            Some(b) => b.include(*p),
            None => bounds = Some(Bounds::from_point(*p)),
        }
    }
    let Some(b) = bounds else { return Vector::ZERO };
    let (min, max, c) = (b.min(), b.max(), b.center());
    let a = match anchor {
        JobAnchor::BottomLeft => min,
        JobAnchor::Bottom => Point::new(c.x, min.y),
        JobAnchor::BottomRight => Point::new(max.x, min.y),
        JobAnchor::Left => Point::new(min.x, c.y),
        JobAnchor::Center => c,
        JobAnchor::Right => Point::new(max.x, c.y),
        JobAnchor::TopLeft => Point::new(min.x, max.y),
        JobAnchor::Top => Point::new(c.x, max.y),
        JobAnchor::TopRight => max,
    };
    at - a
}
