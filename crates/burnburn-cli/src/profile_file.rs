//! The CLI's machine profile file (JSON). Configuration, not proof of hardware behaviour.

use burnburn_job::{MachineFrame, MachineProfile, OriginCorner, PowerRatio};
use burnburn_machine::{MachineId, MachineIdentity};
use burnburn_ruida::{Magic, OverscanPolicy, RuidaProfile};
use serde::Deserialize;

use crate::CliError;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProfileDto {
    id: String,
    revision: u32,
    bed_mm: [f64; 2],
    origin: String,
    max_speed_mm_s: f64,
    travel_speed_mm_s: f64,
    power_range: [f64; 2],
    ruida: RuidaDto,
    #[serde(default)]
    identity: Option<IdentityDto>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuidaDto {
    magic: u8,
    overscan: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct IdentityDto {
    machine_id: String,
    model: Option<String>,
    firmware: Option<String>,
}

/// Everything the CLI knows about one machine.
#[derive(Debug, Clone, PartialEq)]
pub struct ProfileFile {
    pub profile: MachineProfile,
    pub ruida: RuidaProfile,
    /// Needed for upload/start; compile does not need it.
    pub identity: Option<MachineIdentity>,
}

pub fn parse_profile(bytes: &[u8]) -> Result<ProfileFile, CliError> {
    let bad = |m: String| CliError::Profile(m);
    let dto: ProfileDto = serde_json::from_slice(bytes).map_err(|e| bad(e.to_string()))?;
    let origin = match dto.origin.as_str() {
        "bottom-left" => OriginCorner::BottomLeft,
        "top-left" => OriginCorner::TopLeft,
        "top-right" => OriginCorner::TopRight,
        "bottom-right" => OriginCorner::BottomRight,
        other => return Err(bad(format!("unknown origin {other:?}"))),
    };
    let frame = MachineFrame::new(dto.bed_mm[0], dto.bed_mm[1], origin).map_err(|e| bad(e.to_string()))?;
    let power = |r: f64| PowerRatio::new(r).map_err(|e| bad(e.to_string()));
    let profile = MachineProfile::new(
        dto.id,
        dto.revision,
        frame,
        dto.max_speed_mm_s,
        dto.travel_speed_mm_s,
        power(dto.power_range[0])?,
        power(dto.power_range[1])?,
    )
    .map_err(|e| bad(e.to_string()))?;
    let overscan = match dto.ruida.overscan.as_str() {
        "reject" => OverscanPolicy::Reject,
        "controller-managed" => OverscanPolicy::ControllerManaged,
        other => return Err(bad(format!("unknown overscan policy {other:?}"))),
    };
    let identity = dto.identity.map(|i| MachineIdentity {
        id: MachineId::new(i.machine_id),
        model: i.model,
        firmware: i.firmware,
    });
    Ok(ProfileFile { profile, ruida: RuidaProfile { magic: Magic(dto.ruida.magic), overscan }, identity })
}
