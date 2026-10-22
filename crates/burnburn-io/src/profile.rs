//! Frozen machine-profile snapshots (IM28, P07): stored with the project, compared with the
//! local profile, never applied silently. On load the content hash is recomputed from the
//! stored values and must equal the stored one.

use std::collections::BTreeMap;

use burnburn_job::{
    Calibration, Feature, KerfProof, MachineCapabilities, ProofRef, ScanOffsetProof, MachineFrame, MachineProfile, OriginCorner, PowerRatio, RotaryConfig, RotaryKind,
    Support,
};
use burnburn_types::ContentHash;
use serde::{Deserialize, Serialize};

use crate::dto::v1::Num;
use crate::error::LoadError;
use crate::map::num;

/// How a profile frozen in a project relates to the local one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileComparison {
    Identical,
    /// Same machine id, other content: these fields differ (the user decides; nothing changes).
    Differs {
        fields: Vec<&'static str>,
    },
    OtherMachine,
}

/// Compares a frozen profile with the local profile of the same machine.
pub fn compare_profile(frozen: &MachineProfile, local: &MachineProfile) -> ProfileComparison {
    if frozen.revision().id != local.revision().id {
        return ProfileComparison::OtherMachine;
    }
    if frozen.revision().content == local.revision().content {
        return ProfileComparison::Identical;
    }
    let mut fields = Vec::new();
    if frozen.frame() != local.frame() {
        fields.push("frame");
    }
    if frozen.max_speed_mm_s() != local.max_speed_mm_s() {
        fields.push("max_speed_mm_s");
    }
    if frozen.travel_speed_mm_s() != local.travel_speed_mm_s() {
        fields.push("travel_speed_mm_s");
    }
    let (fa, fb) = frozen.power_range();
    let (la, lb) = local.power_range();
    if fa.get() != la.get() || fb.get() != lb.get() {
        fields.push("power_range");
    }
    if Feature::ALL.iter().any(|f| frozen.capabilities().support(*f) != local.capabilities().support(*f)) {
        fields.push("capabilities");
    }
    if frozen.rotary() != local.rotary() {
        fields.push("rotary");
    }
    if frozen.calibration() != local.calibration() {
        fields.push("calibration");
    }
    ProfileComparison::Differs { fields }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProfileDto {
    pub id: String,
    pub revision: u32,
    /// `ContentHash` of the canonical profile (domain `profile`), checked on load.
    pub content: String,
    pub width_mm: Num,
    pub height_mm: Num,
    pub origin: Origin,
    pub max_speed_mm_s: Num,
    pub travel_speed_mm_s: Num,
    pub min_power: Num,
    pub max_power: Num,
    /// Support per feature tag where it differs from the conservative default.
    pub features: BTreeMap<String, SupportDto>,
    /// Rotary attachment (W2-L3). `width_mm`/`height_mm` are the effective frame; the flat-bed
    /// height to return to is stored here. Omitted for a flat bed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotary: Option<RotaryDto>,
    /// Measured kerf and scan offsets (W09). Omitted without measurements, so profiles without
    /// them are stored exactly as before.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calibration: Option<CalibrationDto>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CalibrationDto {
    #[serde(default)]
    pub kerf: Vec<KerfDto>,
    #[serde(default)]
    pub scan_offsets: Vec<ScanOffsetDto>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct KerfDto {
    pub material: String,
    pub kerf_mm: Num,
    /// Identity of the proof the value came from.
    pub proof: u64,
    pub date: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ScanOffsetDto {
    pub speed_mm_s: Num,
    pub offset_mm: Num,
    pub proof: u64,
    pub date: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RotaryDto {
    pub kind: RotaryKindDto,
    pub diameter_mm: Num,
    pub steps_per_rotation: u32,
    pub y_steps_per_mm: Num,
    pub mirror: bool,
    pub flatbed_height_mm: Num,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RotaryKindDto {
    Chuck,
    Roller,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Origin {
    BottomLeft,
    TopLeft,
    TopRight,
    BottomRight,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum SupportDto {
    Supported,
    Unsupported,
    Unverified { reason: String },
}

pub(crate) fn profile_to_dto(p: &MachineProfile) -> ProfileDto {
    let (min, max) = p.power_range();
    let calibration = (!p.calibration().is_empty()).then(|| CalibrationDto {
        kerf: p
            .calibration()
            .kerf()
            .iter()
            .map(|k| KerfDto {
                material: k.material.clone(),
                kerf_mm: Num(k.kerf_mm),
                proof: k.proof.id,
                date: k.proof.date.clone(),
            })
            .collect(),
        scan_offsets: p
            .calibration()
            .scan_offsets()
            .iter()
            .map(|s| ScanOffsetDto {
                speed_mm_s: Num(s.speed_mm_s),
                offset_mm: Num(s.offset_mm),
                proof: s.proof.id,
                date: s.proof.date.clone(),
            })
            .collect(),
    });
    ProfileDto {
        calibration,
        id: p.revision().id.clone(),
        revision: p.revision().revision,
        content: p.revision().content.to_hex(),
        width_mm: Num(p.frame().width_mm()),
        height_mm: Num(p.frame().height_mm()),
        rotary: p.rotary().map(|r| {
            let (kind, diameter_mm) = match r.kind() {
                RotaryKind::Chuck { object_diameter_mm } => (RotaryKindDto::Chuck, object_diameter_mm),
                RotaryKind::Roller { roller_diameter_mm } => (RotaryKindDto::Roller, roller_diameter_mm),
            };
            RotaryDto {
                kind,
                diameter_mm: Num(diameter_mm),
                steps_per_rotation: r.steps_per_rotation(),
                y_steps_per_mm: Num(r.y_steps_per_mm()),
                mirror: r.mirror(),
                flatbed_height_mm: Num(p.flatbed_frame().height_mm()),
            }
        }),
        origin: match p.frame().origin() {
            OriginCorner::BottomLeft => Origin::BottomLeft,
            OriginCorner::TopLeft => Origin::TopLeft,
            OriginCorner::TopRight => Origin::TopRight,
            OriginCorner::BottomRight => Origin::BottomRight,
        },
        max_speed_mm_s: Num(p.max_speed_mm_s()),
        travel_speed_mm_s: Num(p.travel_speed_mm_s()),
        min_power: Num(min.get()),
        max_power: Num(max.get()),
        // Only supports that differ from the conservative default (the default is part of the
        // build; the content hash below covers the effective matrix either way).
        features: Feature::ALL
            .iter()
            .filter(|f| p.capabilities().support(**f) != MachineCapabilities::conservative().support(**f))
            .map(|f| {
                let s = match p.capabilities().support(*f) {
                    Support::Supported => SupportDto::Supported,
                    Support::Unsupported => SupportDto::Unsupported,
                    Support::Unverified { reason } => SupportDto::Unverified { reason },
                };
                (f.tag().to_owned(), s)
            })
            .collect(),
    }
}

pub(crate) fn profile_from_dto(i: usize, d: &ProfileDto) -> Result<MachineProfile, LoadError> {
    let path = move |f: &str| format!("profiles[{i}].{f}");
    let bad = |f: &str, reason: String| LoadError::InvalidValue { path: path(f), reason };
    let n = |f: &'static str, v: Num| num(&|| path(f), v);
    // With a rotary the stored height is the effective one; the frame to start from is the flat bed.
    let base_height = match &d.rotary {
        Some(r) => n("rotary.flatbed_height_mm", r.flatbed_height_mm)?,
        None => n("height_mm", d.height_mm)?,
    };
    let frame = MachineFrame::new(
        n("width_mm", d.width_mm)?,
        base_height,
        match d.origin {
            Origin::BottomLeft => OriginCorner::BottomLeft,
            Origin::TopLeft => OriginCorner::TopLeft,
            Origin::TopRight => OriginCorner::TopRight,
            Origin::BottomRight => OriginCorner::BottomRight,
        },
    )
    .map_err(|e| bad("frame", format!("{e:?}")))?;
    let power = |f: &'static str, v: Num| -> Result<PowerRatio, LoadError> {
        PowerRatio::new(n(f, v)?).map_err(|e| bad(f, format!("{e:?}")))
    };
    let mut caps = MachineCapabilities::conservative();
    for (tag, s) in &d.features {
        let feature = Feature::ALL
            .iter()
            .find(|f| f.tag() == tag)
            .ok_or_else(|| bad("features", format!("unknown feature {tag:?} (newer build?)")))?;
        let support = match s {
            SupportDto::Supported => Support::Supported,
            SupportDto::Unsupported => Support::Unsupported,
            SupportDto::Unverified { reason } => Support::Unverified { reason: reason.clone() },
        };
        caps = caps.with(*feature, support);
    }
    let profile = MachineProfile::new(
        d.id.clone(),
        d.revision,
        frame,
        n("max_speed_mm_s", d.max_speed_mm_s)?,
        n("travel_speed_mm_s", d.travel_speed_mm_s)?,
        power("min_power", d.min_power)?,
        power("max_power", d.max_power)?,
    )
    .and_then(|p| p.with_capabilities(caps))
    .map_err(|e| bad("profile", format!("{e:?}")))?;
    let profile = match &d.rotary {
        None => profile,
        Some(r) => {
            let diameter = n("rotary.diameter_mm", r.diameter_mm)?;
            let kind = match r.kind {
                RotaryKindDto::Chuck => RotaryKind::Chuck { object_diameter_mm: diameter },
                RotaryKindDto::Roller => RotaryKind::Roller { roller_diameter_mm: diameter },
            };
            let config =
                RotaryConfig::new(kind, r.steps_per_rotation, n("rotary.y_steps_per_mm", r.y_steps_per_mm)?, r.mirror)
                    .map_err(|e| bad("rotary", format!("{e:?}")))?;
            profile.with_rotary(config).map_err(|e| bad("rotary", format!("{e:?}")))?
        }
    };
    let profile = match &d.calibration {
        None => profile,
        Some(c) => {
            let proof = |id, date: &String| ProofRef { id, date: date.clone() };
            let kerf = c
                .kerf
                .iter()
                .map(|k| {
                    Ok(KerfProof {
                        material: k.material.clone(),
                        kerf_mm: n("calibration.kerf_mm", k.kerf_mm)?,
                        proof: proof(k.proof, &k.date),
                    })
                })
                .collect::<Result<Vec<_>, LoadError>>()?;
            let scan = c
                .scan_offsets
                .iter()
                .map(|s| {
                    Ok(ScanOffsetProof {
                        speed_mm_s: n("calibration.speed_mm_s", s.speed_mm_s)?,
                        offset_mm: n("calibration.offset_mm", s.offset_mm)?,
                        proof: proof(s.proof, &s.date),
                    })
                })
                .collect::<Result<Vec<_>, LoadError>>()?;
            let calibration = Calibration::new(kerf, scan).map_err(|e| bad("calibration", format!("{e}")))?;
            profile.with_calibration(calibration).map_err(|e| bad("calibration", format!("{e:?}")))?
        }
    };
    let stored = ContentHash::from_hex(&d.content).map_err(|e| bad("content", e.to_string()))?;
    if profile.revision().content != stored {
        return Err(bad("content", "stored content hash does not match the stored values".into()));
    }
    Ok(profile)
}
