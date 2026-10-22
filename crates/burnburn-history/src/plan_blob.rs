//! Self-checking JSON encoding of a frozen [`JobPlan`] for the plan blob.
//!
//! Decoding rebuilds the plan through `JobPlanBuilder` (same validation as the compiler) and
//! refuses it unless the rebuilt `JobPlanId` equals the recorded one, so a blob can never stand
//! in for another plan.

use burnburn_job::{
    AppliedCalibration, AppliedKind, JobPlan, JobPlanBuilder, MachineFrame, MachinePoint, OperationSettings,
    OriginCorner, PlanEvent, PlanMode, PlanSource, PowerRatio, ProfileRevision, ProofRef, ScanAxis, ScanLine,
    SettingsIndex, SourceRef,
};
use burnburn_types::{ContentHash, GeometryId, OperationId, RevisionId, StableId};
use serde::{Deserialize, Serialize};

use crate::HistoryError;
use crate::hex::{hash_from_hex, hash_to_hex};

pub const PLAN_BLOB_SCHEMA: u32 = 1;

#[derive(Serialize, Deserialize)]
struct PlanDto {
    schema: u32,
    id: String,
    source: SourceDto,
    frame: FrameDto,
    start: [f64; 2],
    settings: Vec<SettingsDto>,
    events: Vec<EventDto>,
    /// The measured values the plan was made with (W09). Absent for a plan made without any.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    calibration: Vec<AppliedDto>,
}

#[derive(Serialize, Deserialize)]
struct AppliedDto {
    operation: u64,
    #[serde(flatten)]
    kind: AppliedKindDto,
    value_mm: f64,
    /// `(id, date)` of each proof the value came from.
    proofs: Vec<(u64, String)>,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "applied", rename_all = "kebab-case")]
enum AppliedKindDto {
    Kerf { material: String },
    ScanOffset { speed_mm_s: f64 },
}

#[derive(Serialize, Deserialize)]
struct SourceDto {
    document_revision: u64,
    source_hash: String,
    profile_id: String,
    profile_revision: u32,
    profile_content: String,
    compiler_version: String,
}

#[derive(Serialize, Deserialize)]
struct FrameDto {
    width_mm: f64,
    height_mm: f64,
    origin: String,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "kebab-case")]
enum ModeDto {
    Line,
    Scan { interval_mm: f64, axis_y: bool, bidirectional: bool },
}

#[derive(Serialize, Deserialize)]
struct SettingsDto {
    mode: ModeDto,
    speed_mm_s: f64,
    min_power: f64,
    max_power: f64,
    air_assist: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "kebab-case")]
enum EventDto {
    Operation { operation: u64, settings: usize, pass: u32 },
    Travel { to: [f64; 2] },
    Cut { to: [f64; 2], origin: Option<[u64; 2]> },
    Scan { from: [f64; 2], to: [f64; 2], overscan_mm: f64, origin: Option<[u64; 2]> },
}

fn origin_name(o: OriginCorner) -> &'static str {
    match o {
        OriginCorner::BottomLeft => "bottom-left",
        OriginCorner::TopLeft => "top-left",
        OriginCorner::TopRight => "top-right",
        OriginCorner::BottomRight => "bottom-right",
    }
}

fn pt(p: MachinePoint) -> [f64; 2] {
    [p.x_mm(), p.y_mm()]
}

fn src(o: Option<SourceRef>) -> Option<[u64; 2]> {
    o.map(|o| [o.geometry.raw(), o.operation.raw()])
}

pub fn encode_plan(plan: &JobPlan) -> Result<Vec<u8>, HistoryError> {
    let s = plan.source();
    let settings = plan
        .settings()
        .iter()
        .map(|st| {
            let mode = match st.mode {
                PlanMode::Line => ModeDto::Line,
                PlanMode::Scan { interval_mm, axis, bidirectional } => {
                    ModeDto::Scan { interval_mm, axis_y: axis == ScanAxis::Y, bidirectional }
                }
                _ => return Err(HistoryError::PlanBlob("plan mode not representable")),
            };
            Ok(SettingsDto {
                mode,
                speed_mm_s: st.speed_mm_s,
                min_power: st.min_power.get(),
                max_power: st.max_power.get(),
                air_assist: st.air_assist,
            })
        })
        .collect::<Result<_, _>>()?;
    let events = plan
        .events()
        .iter()
        .map(|e| match *e {
            PlanEvent::OperationBoundary { operation, settings, pass } => {
                Ok(EventDto::Operation { operation: operation.raw(), settings: settings.get(), pass })
            }
            PlanEvent::Travel { to } => Ok(EventDto::Travel { to: pt(to) }),
            PlanEvent::Cut { to, origin } => Ok(EventDto::Cut { to: pt(to), origin: src(origin) }),
            PlanEvent::ScanRun { line, origin } => Ok(EventDto::Scan {
                from: pt(line.from),
                to: pt(line.to),
                overscan_mm: line.overscan_mm,
                origin: src(origin),
            }),
            _ => Err(HistoryError::PlanBlob("plan event not representable")),
        })
        .collect::<Result<_, _>>()?;
    let dto = PlanDto {
        schema: PLAN_BLOB_SCHEMA,
        id: hash_to_hex(&plan.id().hash()),
        source: SourceDto {
            document_revision: s.document_revision.raw(),
            source_hash: hash_to_hex(&s.source_hash),
            profile_id: s.machine_profile.id.clone(),
            profile_revision: s.machine_profile.revision,
            profile_content: hash_to_hex(&s.machine_profile.content),
            compiler_version: s.compiler_version.clone(),
        },
        frame: FrameDto {
            width_mm: plan.frame().width_mm(),
            height_mm: plan.frame().height_mm(),
            origin: origin_name(plan.frame().origin()).to_owned(),
        },
        start: pt(plan.start()),
        settings,
        events,
        calibration: plan
            .calibration()
            .iter()
            .map(|a| AppliedDto {
                operation: a.operation.raw(),
                kind: match &a.kind {
                    AppliedKind::Kerf { material } => AppliedKindDto::Kerf { material: material.clone() },
                    AppliedKind::ScanOffset { speed_mm_s } => AppliedKindDto::ScanOffset { speed_mm_s: *speed_mm_s },
                },
                value_mm: a.value_mm,
                proofs: a.proofs.iter().map(|p| (p.id, p.date.clone())).collect(),
            })
            .collect(),
    };
    serde_json::to_vec(&dto).map_err(|_| HistoryError::PlanBlob("plan not encodable"))
}

/// Rebuilds and verifies a plan blob. `expected` is the plan hash recorded in the CompiledJob.
pub fn decode_plan(bytes: &[u8], expected: &ContentHash) -> Result<JobPlan, HistoryError> {
    let bad = |what| HistoryError::PlanBlob(what);
    let dto: PlanDto = serde_json::from_slice(bytes).map_err(|_| bad("plan blob is not valid JSON"))?;
    if dto.schema != PLAN_BLOB_SCHEMA {
        return Err(bad("unknown plan blob schema"));
    }
    let origin = match dto.frame.origin.as_str() {
        "bottom-left" => OriginCorner::BottomLeft,
        "top-left" => OriginCorner::TopLeft,
        "top-right" => OriginCorner::TopRight,
        "bottom-right" => OriginCorner::BottomRight,
        _ => return Err(bad("unknown origin corner")),
    };
    let s = &dto.source;
    let source = PlanSource {
        document_revision: RevisionId::from_raw(s.document_revision),
        source_hash: hash_from_hex(&s.source_hash).ok_or(bad("source hash"))?,
        machine_profile: ProfileRevision {
            id: s.profile_id.clone(),
            revision: s.profile_revision,
            content: hash_from_hex(&s.profile_content).ok_or(bad("profile hash"))?,
        },
        compiler_version: s.compiler_version.clone(),
    };
    let p = |[x, y]: [f64; 2]| MachinePoint::new(x, y).map_err(|_| bad("non-finite point"));
    let frame = MachineFrame::new(dto.frame.width_mm, dto.frame.height_mm, origin).map_err(|_| bad("frame"))?;
    let mut b = JobPlanBuilder::new(source, frame, p(dto.start)?);
    let mut indices: Vec<SettingsIndex> = Vec::with_capacity(dto.settings.len());
    for st in &dto.settings {
        let mode = match st.mode {
            ModeDto::Line => PlanMode::Line,
            ModeDto::Scan { interval_mm, axis_y, bidirectional } => {
                PlanMode::Scan { interval_mm, axis: if axis_y { ScanAxis::Y } else { ScanAxis::X }, bidirectional }
            }
        };
        let power = |r| PowerRatio::new(r).map_err(|_| bad("power"));
        indices.push(
            b.add_settings(OperationSettings {
                mode,
                speed_mm_s: st.speed_mm_s,
                min_power: power(st.min_power)?,
                max_power: power(st.max_power)?,
                air_assist: st.air_assist,
            })
            .map_err(|_| bad("settings"))?,
        );
    }
    let origin_of = |o: Option<[u64; 2]>| {
        o.map(|[g, op]| SourceRef { geometry: GeometryId::from_raw(g), operation: OperationId::from_raw(op) })
    };
    for e in dto.events {
        let event = match e {
            EventDto::Operation { operation, settings, pass } => PlanEvent::OperationBoundary {
                operation: OperationId::from_raw(operation),
                settings: *indices.get(settings).ok_or(bad("settings index"))?,
                pass,
            },
            EventDto::Travel { to } => PlanEvent::Travel { to: p(to)? },
            EventDto::Cut { to, origin } => PlanEvent::Cut { to: p(to)?, origin: origin_of(origin) },
            EventDto::Scan { from, to, overscan_mm, origin } => PlanEvent::ScanRun {
                line: ScanLine { from: p(from)?, to: p(to)?, overscan_mm },
                origin: origin_of(origin),
            },
        };
        b.push(event).map_err(|_| bad("event"))?;
    }
    for a in dto.calibration {
        b.note_calibration(AppliedCalibration {
            operation: OperationId::from_raw(a.operation),
            kind: match a.kind {
                AppliedKindDto::Kerf { material } => AppliedKind::Kerf { material },
                AppliedKindDto::ScanOffset { speed_mm_s } => AppliedKind::ScanOffset { speed_mm_s },
            },
            value_mm: a.value_mm,
            proofs: a.proofs.into_iter().map(|(id, date)| ProofRef { id, date }).collect(),
        });
    }
    let plan = b.finish().map_err(|_| bad("plan"))?;
    let recorded = hash_from_hex(&dto.id).ok_or(bad("plan id"))?;
    if plan.id().hash() != recorded || recorded != *expected {
        return Err(HistoryError::BlobMismatch);
    }
    Ok(plan)
}
