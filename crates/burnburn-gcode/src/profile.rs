//! The GRBL backend's machine configuration and capability matrix.

use std::fmt;

use burnburn_job::{Feature, MachineCapabilities, Support};

/// How laser power is switched while the head moves (`M3` constant vs. `M4` dynamic).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerMode {
    /// `M3`: the programmed power is applied for the whole move, also during acceleration.
    Constant,
    /// `M4`: GRBL scales the power with the current speed during acceleration.
    Dynamic,
}

/// A work coordinate system (`G54`..`G59`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkCoordinate {
    G54,
    G55,
    G56,
    G57,
    G58,
    G59,
}

/// Configuration of one GRBL 1.1 machine. Configuration, not proof of hardware behaviour.
#[derive(Debug, Clone, PartialEq)]
pub struct GrblProfile {
    /// `$30`: the spindle (laser) value that means 100 % power.
    pub max_spindle: u32,
    /// Assumes `$32=1`. Without laser mode `M3` would burn during `G0`; lowering refuses then.
    pub laser_mode: bool,
    pub power_mode: PowerMode,
    /// The machine has an air-assist output on `M8`/`M9`.
    pub air_assist: bool,
    /// Select this work coordinate system in the program header (`None`: leave the controller's).
    pub work_coordinate: Option<WorkCoordinate>,
    /// Decimals for coordinates (1..=4); a coordinate is a multiple of `10^-decimals` mm.
    pub decimals: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileError {
    OutOfRange(&'static str),
}

impl fmt::Display for ProfileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProfileError::OutOfRange(what) => write!(f, "{what} out of range"),
        }
    }
}

impl std::error::Error for ProfileError {}

impl GrblProfile {
    /// Stock GRBL 1.1 laser configuration: `$30=1000`, `$32=1`, dynamic power, no air assist,
    /// three decimals.
    pub fn grbl_1_1_laser() -> GrblProfile {
        GrblProfile {
            max_spindle: 1000,
            laser_mode: true,
            power_mode: PowerMode::Dynamic,
            air_assist: false,
            work_coordinate: None,
            decimals: 3,
        }
    }

    pub fn validate(&self) -> Result<(), ProfileError> {
        if self.max_spindle == 0 {
            return Err(ProfileError::OutOfRange("max spindle ($30)"));
        }
        if !(1..=4).contains(&self.decimals) {
            return Err(ProfileError::OutOfRange("decimals"));
        }
        Ok(())
    }

    pub fn capabilities(&self) -> MachineCapabilities {
        grbl_capabilities(self)
    }
}

/// The feature matrix of a GRBL profile (decided per `Feature`; `tests/capabilities.rs` pins it).
///
/// - `Supported`: everything the lowering writes as plain `G0`/`G1` with `S`/`F` words: the
///   Ruida-equivalent ordinary events, diagonal scan runs (one `G1`) and per-run power (one `G1`
///   with its own `S`). "Supported" means the **lowering** is exact and tested against an
///   independent interpreter; it is no claim about a concrete controller (`docs/hardware/`).
/// - `Unsupported`: Z steps (no Z axis is modelled), dot pulses and power ramps: the lowering
///   has no exact form for them and refuses instead of approximating.
/// - `Unverified`: common-line cutting needs its own validated process (as on every backend).
///
/// Air assist is not a `Feature`; it is [`GrblProfile::air_assist`] and refused per operation.
pub fn grbl_capabilities(_profile: &GrblProfile) -> MachineCapabilities {
    MachineCapabilities::conservative()
        .with(Feature::ScanAngle, Support::Supported)
        .with(Feature::GrayscaleModulation, Support::Supported)
        .with(Feature::ZStep, Support::Unsupported)
        .with(Feature::DotMode, Support::Unsupported)
        .with(Feature::RampLength, Support::Unsupported)
        .with(
            Feature::CommonLine,
            Support::Unverified { reason: "common-line cutting is not validated for GRBL".into() },
        )
}
