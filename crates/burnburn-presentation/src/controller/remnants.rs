//! The remnant library (W07): keep the selection as a leftover sheet, remove it, draw it in a window
//! so a nest can use its contour as the sheet, and let an order name the remnant it is made from.
//!
//! The library belongs to the app, not to a window; it is read with the other libraries, changed on
//! the UI thread and written back one write at a time. A file that could not be read is kept as it is
//! and never written. Linking an order changes no count.

use burnburn_document::CommandPayload;
use burnburn_document::GeometryCommand;
use burnburn_geometry::{PathBuilder, Point, ProductionTolerance};
use burnburn_io::remnant_library::{RemnantRecord, validate_remnant_library};
use burnburn_types::{DocumentId, Length, RequestId};

use super::machine::{Prepared, plate_material};
use super::machine_hub::iso_date;
use super::{AppController, AppUpdate, Order, Session, apply_edit};
use crate::field::parse_number;
use crate::production::OrderId;
use crate::remnants::{RemnantIntent, RemnantLinkStatus, RemnantProblem, RemnantRow, RemnantsViewModel};

/// Curves of the selection become chords this close to the exact outline (mm).
const OUTLINE_TOLERANCE_MM: f64 = 0.01;
/// Thickest sheet a remnant can be (mm), as the library holds it.
const MAX_THICKNESS_MM: f64 = 100.0;

fn changed() -> AppUpdate {
    AppUpdate { view_changed: true, ..AppUpdate::default() }
}

/// Twice the signed area of a closed outline.
fn double_area(points: &[[f64; 2]]) -> f64 {
    let n = points.len();
    (0..n)
        .map(|i| {
            let (a, b) = (points[i], points[(i + 1) % n]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum()
}

fn bounds_of(points: &[[f64; 2]]) -> ([f64; 2], [f64; 2]) {
    points.iter().fold(([f64::MAX; 2], [f64::MIN; 2]), |(lo, hi), p| {
        ([lo[0].min(p[0]), lo[1].min(p[1])], [hi[0].max(p[0]), hi[1].max(p[1])])
    })
}

/// The closed outlines of the selected objects as polygons (curves flattened).
fn closed_outlines(s: &Session) -> Vec<Vec<[f64; 2]>> {
    let Ok(tolerance) = Length::from_mm(OUTLINE_TOLERANCE_MM).map(ProductionTolerance::new) else { return Vec::new() };
    let Ok(tolerance) = tolerance else { return Vec::new() };
    let snapshot = s.doc.snapshot();
    let mut out = Vec::new();
    for id in s.selection.ids() {
        let Some(object) = snapshot.object(*id) else { continue };
        for line in object.path().flatten(&tolerance) {
            if !line.closed {
                continue;
            }
            let mut points: Vec<[f64; 2]> = line.points.iter().map(|p| [p.x, p.y]).collect();
            points.dedup();
            if points.len() > 1 && points.first() == points.last() {
                points.pop();
            }
            if points.len() >= 3 {
                out.push(points);
            }
        }
    }
    out
}

fn polygon_path(points: &[[f64; 2]], shift: [f64; 2]) -> Option<burnburn_geometry::Path> {
    let (first, rest) = points.split_first()?;
    let at = |p: &[f64; 2]| Point::new(p[0] + shift[0], p[1] + shift[1]);
    let mut builder = PathBuilder::new().move_to(at(first));
    for p in rest {
        builder = builder.line_to(at(p));
    }
    builder.close().build().ok()
}

impl AppController {
    pub(super) fn remnants_view(&self) -> RemnantsViewModel {
        let rows = self.libs.remnants.iter().flat_map(|l| l.remnants.iter()).map(row_of).collect();
        RemnantsViewModel { rows, problem: self.libs.remnant_problem }
    }

    pub(super) fn remnant_intent(&mut self, intent: RemnantIntent) -> AppUpdate {
        self.libs.remnant_problem = None;
        self.ensure_libraries();
        let problem = match intent {
            RemnantIntent::SaveFromSelection { document, name, material, thickness, note, date } => {
                self.save_remnant(document, &name, &material, &thickness, &note, &date).err()
            }
            RemnantIntent::Remove { id } => self.remove_remnant(id).err(),
            RemnantIntent::Place { document, id } => return self.place_remnant(document, id),
        };
        self.libs.remnant_problem = problem;
        changed()
    }

    fn ready(&self) -> Result<(), RemnantProblem> {
        if !self.libs.is_ready() || self.libs.remnants.is_none() {
            return Err(RemnantProblem::NotReady);
        }
        if self.libs.remnants_unreadable {
            return Err(RemnantProblem::Unreadable);
        }
        Ok(())
    }

    fn save_remnant(
        &mut self,
        document: DocumentId,
        name: &str,
        material: &str,
        thickness: &str,
        note: &str,
        date: &str,
    ) -> Result<(), RemnantProblem> {
        self.ready()?;
        let session = self.sessions.get(&document).ok_or(RemnantProblem::NeedsSelection)?;
        if session.selection.ids().is_empty() {
            return Err(RemnantProblem::NeedsSelection);
        }
        let (name, material) = (name.trim(), material.trim());
        if name.is_empty() {
            return Err(RemnantProblem::NeedsName);
        }
        if material.is_empty() {
            return Err(RemnantProblem::NeedsMaterial);
        }
        let thickness_mm = match parse_number(thickness, &["mm"]) {
            Ok(v) if v.is_finite() && v > 0.0 && v <= MAX_THICKNESS_MM => v,
            _ => return Err(RemnantProblem::InvalidThickness),
        };
        let date = if date.trim().is_empty() {
            self.clock
                .as_ref()
                .and_then(|c| c())
                .and_then(|c| c.format("%Y-%m-%d").ok())
                .ok_or(RemnantProblem::InvalidDate)?
        } else {
            iso_date(date).ok_or(RemnantProblem::InvalidDate)?
        };
        let mut outlines = closed_outlines(session);
        if outlines.is_empty() {
            return Err(RemnantProblem::NoClosedOutline);
        }
        // The largest outline is the sheet, the others are zones that stay out of use.
        outlines.sort_by(|a, b| double_area(b).abs().total_cmp(&double_area(a).abs()));
        let mut outlines = outlines.into_iter();
        let contour = outlines.next().ok_or(RemnantProblem::NoClosedOutline)?;
        let (lo, hi) = bounds_of(&contour);
        let exclusions: Vec<Vec<[f64; 2]>> = outlines.collect();
        for zone in &exclusions {
            let (zlo, zhi) = bounds_of(zone);
            if zlo[0] < lo[0] || zlo[1] < lo[1] || zhi[0] > hi[0] || zhi[1] > hi[1] {
                return Err(RemnantProblem::ZoneOutsideSheet);
            }
        }
        // The remnant lives in a frame of its own: the corner of its bounds is the origin.
        let shift = |p: &[f64; 2]| [p[0] - lo[0], p[1] - lo[1]];
        let library = self.libs.remnants.as_mut().ok_or(RemnantProblem::NotReady)?;
        if library.remnants.iter().any(|r| r.name.trim().to_lowercase() == name.to_lowercase()) {
            return Err(RemnantProblem::NameTaken);
        }
        let record = RemnantRecord {
            id: library.next_id,
            name: name.to_owned(),
            material: material.to_owned(),
            thickness_mm,
            contour: contour.iter().map(shift).collect(),
            exclusions: exclusions.iter().map(|z| z.iter().map(shift).collect()).collect(),
            note: note.trim().to_owned(),
            date,
        };
        let mut candidate = library.clone();
        candidate.remnants.push(record);
        candidate.next_id = candidate.next_id.saturating_add(1);
        validate_remnant_library(&candidate).map_err(|_| RemnantProblem::NotASheet)?;
        *library = candidate;
        self.flush_remnants();
        Ok(())
    }

    fn remove_remnant(&mut self, id: u64) -> Result<(), RemnantProblem> {
        self.ready()?;
        let library = self.libs.remnants.as_mut().ok_or(RemnantProblem::NotReady)?;
        let before = library.remnants.len();
        library.remnants.retain(|r| r.id != id);
        if library.remnants.len() == before {
            return Err(RemnantProblem::Unknown);
        }
        self.flush_remnants();
        Ok(())
    }

    fn place_remnant(&mut self, document: DocumentId, id: u64) -> AppUpdate {
        let record = match self.ready() {
            Ok(()) => self.libs.remnants.as_ref().and_then(|l| l.remnants.iter().find(|r| r.id == id)).cloned(),
            Err(problem) => {
                self.libs.remnant_problem = Some(problem);
                return changed();
            }
        };
        let Some(record) = record else {
            self.libs.remnant_problem = Some(RemnantProblem::Unknown);
            return changed();
        };
        // One command per outline: the sheet first, then its zones.
        let mut requests: Vec<Option<RequestId>> = (0..=record.exclusions.len()).map(|_| self.next_id()).collect();
        requests.reverse();
        self.with_session(document, |s| {
            let (lo, hi) = bounds_of(&record.contour);
            let centre = s.camera.center_mm();
            let shift = [centre.x - (lo[0] + hi[0]) / 2.0, centre.y - (lo[1] + hi[1]) / 2.0];
            let mut update = AppUpdate::default();
            let outlines = std::iter::once((record.name.clone(), &record.contour)).chain(
                record.exclusions.iter().enumerate().map(|(i, z)| (format!("{} zone {}", record.name, i + 1), z)),
            );
            for (name, points) in outlines {
                let Some(path) = polygon_path(points, shift) else { continue };
                let request = requests.pop().flatten();
                let step = apply_edit(s, document, CommandPayload::Edit(GeometryCommand::CreatePath { path, name }), request);
                update.view_changed |= step.view_changed;
                update.effects.extend(step.effects);
                for d in step.scenes_changed {
                    if !update.scenes_changed.contains(&d) {
                        update.scenes_changed.push(d);
                    }
                }
            }
            update
        })
    }

    /// Names (or clears) the remnant an order is made from. The name must be in the library; the
    /// order's counts do not move.
    pub(super) fn set_order_remnant(&mut self, order: OrderId, remnant: Option<String>) -> AppUpdate {
        self.libs.remnant_problem = None;
        self.ensure_libraries();
        let Some(mut changed_order) = self.orders.iter().find(|o| o.id == order).cloned() else {
            return AppUpdate::default();
        };
        let named = match remnant.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
            None => None,
            Some(wanted) => {
                if let Err(problem) = self.ready() {
                    self.libs.remnant_problem = Some(problem);
                    return changed();
                }
                let found = self
                    .libs
                    .remnants
                    .as_ref()
                    .and_then(|l| l.remnants.iter().find(|r| r.name.trim().to_lowercase() == wanted.to_lowercase()));
                match found {
                    Some(r) => Some(r.name.clone()),
                    None => {
                        self.libs.remnant_problem = Some(RemnantProblem::Unknown);
                        return changed();
                    }
                }
            }
        };
        changed_order.draft.remnant = named;
        if let Err(e) = self.store_order(&changed_order) {
            return e;
        }
        if let Some(o) = self.orders.iter_mut().find(|o| o.id == order) {
            *o = changed_order;
        }
        changed()
    }

    /// How the remnant an order names compares with its job.
    pub(super) fn remnant_link(&self, order: &Order, job: Option<&Prepared>) -> Option<RemnantLinkStatus> {
        let name = order.draft.remnant.as_deref()?;
        let library = self.libs.remnants.as_ref()?;
        let found = library.remnants.iter().find(|r| r.name == name);
        Some(match found {
            None => RemnantLinkStatus::Missing,
            Some(r) => {
                let job_material =
                    job.and_then(|p| p.job.inputs.as_ref()).and_then(|i| plate_material(&i.project.document));
                RemnantLinkStatus::Known {
                    material: r.material.clone(),
                    thickness_um: (r.thickness_mm * 1000.0).round() as u32,
                    material_matches: job_material.map(|m| m.trim().to_lowercase() == r.material.trim().to_lowercase()),
                }
            }
        })
    }
}

fn row_of(r: &RemnantRecord) -> RemnantRow {
    let (lo, hi) = bounds_of(&r.contour);
    let zones: f64 = r.exclusions.iter().map(|z| double_area(z).abs() / 2.0).sum();
    let usable_mm2 = double_area(&r.contour).abs() / 2.0 - zones;
    RemnantRow {
        id: r.id,
        name: r.name.clone(),
        material: r.material.clone(),
        thickness_mm: r.thickness_mm,
        width_mm: hi[0] - lo[0],
        height_mm: hi[1] - lo[1],
        usable_cm2: usable_mm2 / 100.0,
        zones: r.exclusions.len(),
        note: r.note.clone(),
        date: r.date.clone(),
    }
}
