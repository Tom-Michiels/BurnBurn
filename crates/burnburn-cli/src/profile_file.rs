//! The CLI's machine profile file (JSON). Configuration, not proof of hardware behaviour.

use burnburn_gcode::{GrblProfile, PowerMode, WorkCoordinate, grbl_capabilities};
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
    #[serde(default)]
    backend: Option<String>,
    #[serde(default)]
    ruida: Option<RuidaDto>,
    #[serde(default)]
    grbl: Option<GrblDto>,
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
struct GrblDto {
    max_spindle: Option<u32>,
    laser_mode: Option<bool>,
    power_mode: Option<String>,
    air_assist: Option<bool>,
    work_coordinate: Option<String>,
    decimals: Option<u8>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct IdentityDto {
    machine_id: String,
    model: Option<String>,
    firmware: Option<String>,
}

/// Which machine backend a profile compiles for.
#[derive(Debug, Clone, PartialEq)]
pub enum Backend {
    /// Ruida RD files (the default; unchanged paths).
    Ruida,
    /// GRBL 1.1 G-code.
    Grbl(GrblProfile),
}

/// Everything the CLI knows about one machine.
#[derive(Debug, Clone, PartialEq)]
pub struct ProfileFile {
    pub profile: MachineProfile,
    /// Ruida settings. For a GRBL profile this is a neutral placeholder that is never used.
    pub ruida: RuidaProfile,
    pub backend: Backend,
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
    let mut profile = MachineProfile::new(
        dto.id,
        dto.revision,
        frame,
        dto.max_speed_mm_s,
        dto.travel_speed_mm_s,
        power(dto.power_range[0])?,
        power(dto.power_range[1])?,
    )
    .map_err(|e| bad(e.to_string()))?;
    let (ruida, backend) = match (dto.backend.as_deref().unwrap_or("ruida"), dto.ruida, dto.grbl) {
        ("ruida", Some(r), None) => {
            let overscan = match r.overscan.as_str() {
                "reject" => OverscanPolicy::Reject,
                "controller-managed" => OverscanPolicy::ControllerManaged,
                other => return Err(bad(format!("unknown overscan policy {other:?}"))),
            };
            (RuidaProfile { magic: Magic(r.magic), overscan }, Backend::Ruida)
        }
        ("grbl", None, Some(g)) => {
            let mut gp = GrblProfile::grbl_1_1_laser();
            gp.max_spindle = g.max_spindle.unwrap_or(gp.max_spindle);
            gp.laser_mode = g.laser_mode.unwrap_or(gp.laser_mode);
            gp.air_assist = g.air_assist.unwrap_or(gp.air_assist);
            gp.decimals = g.decimals.unwrap_or(gp.decimals);
            gp.power_mode = match g.power_mode.as_deref() {
                None | Some("dynamic") => PowerMode::Dynamic,
                Some("constant") => PowerMode::Constant,
                Some(other) => return Err(bad(format!("unknown power mode {other:?}"))),
            };
            gp.work_coordinate = match g.work_coordinate.as_deref() {
                None => None,
                Some("G54") => Some(WorkCoordinate::G54),
                Some("G55") => Some(WorkCoordinate::G55),
                Some("G56") => Some(WorkCoordinate::G56),
                Some("G57") => Some(WorkCoordinate::G57),
                Some("G58") => Some(WorkCoordinate::G58),
                Some("G59") => Some(WorkCoordinate::G59),
                Some(other) => return Err(bad(format!("unknown work coordinate system {other:?}"))),
            };
            gp.validate().map_err(|e| bad(e.to_string()))?;
            profile = profile.with_capabilities(grbl_capabilities(&gp)).map_err(|e| bad(e.to_string()))?;
            // Neutral placeholder: never used by the GRBL backend.
            (RuidaProfile { magic: Magic(0x88), overscan: OverscanPolicy::Reject }, Backend::Grbl(gp))
        }
        ("ruida", _, _) => return Err(bad("backend ruida needs a \"ruida\" section and no \"grbl\" section".into())),
        ("grbl", _, _) => return Err(bad("backend grbl needs a \"grbl\" section and no \"ruida\" section".into())),
        (other, _, _) => return Err(bad(format!("unknown backend {other:?}"))),
    };
    let identity = dto.identity.map(|i| MachineIdentity {
        id: MachineId::new(i.machine_id),
        model: i.model,
        firmware: i.firmware,
    });
    Ok(ProfileFile { profile, ruida, backend, identity })
}
