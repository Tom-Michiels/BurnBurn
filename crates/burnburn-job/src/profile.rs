use burnburn_types::{CanonicalWriter, ContentHash, HashDomain};

use crate::{MachineCapabilities, MachineFrame, OriginCorner, PlanError, PowerRatio, RotaryConfig, RotaryKind};

/// Identity and revision of a machine profile. A plan compiled for one revision is stale for
/// another (M01: a profile change invalidates the old compile result). `revision` is a display
/// counter; `content` is the hash of the canonical profile, so two profiles with the same id
/// and counter but other content are different revisions (DR-18).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProfileRevision {
    pub id: String,
    pub revision: u32,
    pub content: ContentHash,
}

/// Capability data of one configured machine, used by compiler, preflight and preview.
/// Values are configuration, not measurements: a profile does not prove hardware behaviour.
#[derive(Debug, Clone, PartialEq)]
pub struct MachineProfile {
    revision: ProfileRevision,
    frame: MachineFrame,
    max_speed_mm_s: f64,
    travel_speed_mm_s: f64,
    min_power: PowerRatio,
    max_power: PowerRatio,
    capabilities: MachineCapabilities,
    rotary: Option<RotaryState>,
}

/// The rotary setting and the flat-bed frame to return to.
#[derive(Debug, Clone, Copy, PartialEq)]
struct RotaryState {
    config: RotaryConfig,
    flatbed: MachineFrame,
}

impl MachineProfile {
    pub fn new(
        id: impl Into<String>,
        revision: u32,
        frame: MachineFrame,
        max_speed_mm_s: f64,
        travel_speed_mm_s: f64,
        min_power: PowerRatio,
        max_power: PowerRatio,
    ) -> Result<Self, PlanError> {
        let ok = |v: f64| v.is_finite() && v > 0.0;
        if !ok(max_speed_mm_s) || !ok(travel_speed_mm_s) {
            return Err(PlanError::OutOfRange("speed"));
        }
        if min_power.get() > max_power.get() {
            return Err(PlanError::OutOfRange("power"));
        }
        let mut profile = MachineProfile {
            revision: ProfileRevision { id: id.into(), revision, content: ContentHash::of(HashDomain::Profile, &[]) },
            frame,
            max_speed_mm_s,
            travel_speed_mm_s,
            min_power,
            max_power,
            capabilities: MachineCapabilities::conservative(),
            rotary: None,
        };
        profile.revision.content = profile.content_hash()?;
        Ok(profile)
    }

    /// The same profile with another feature matrix (and therefore other content).
    pub fn with_capabilities(mut self, capabilities: MachineCapabilities) -> Result<Self, PlanError> {
        self.capabilities = capabilities;
        self.revision.content = self.content_hash()?;
        Ok(self)
    }

    /// Canonical profile (domain `profile`): `str` id, frame (`f64` width, `f64` height, origin
    /// tag), `f64` max speed, `f64` travel speed, `f64` min power, `f64` max power, then the
    /// effective support of every feature. The display counter is not content.
    fn content_hash(&self) -> Result<ContentHash, PlanError> {
        let mut w = CanonicalWriter::new();
        w.str(&self.revision.id);
        w.f64(self.frame.width_mm());
        w.f64(self.frame.height_mm());
        w.tag(origin_tag(self.frame.origin()));
        for v in [self.max_speed_mm_s, self.travel_speed_mm_s, self.min_power.get(), self.max_power.get()] {
            w.f64(v);
        }
        self.capabilities.encode(&mut w);
        // Only a profile with a rotary writes more, so flat-bed profiles keep their hash.
        if let Some(r) = &self.rotary {
            w.tag("rotary");
            match r.config.kind() {
                RotaryKind::Chuck { object_diameter_mm } => {
                    w.tag("chuck");
                    w.f64(object_diameter_mm);
                }
                RotaryKind::Roller { roller_diameter_mm } => {
                    w.tag("roller");
                    w.f64(roller_diameter_mm);
                }
            }
            w.u64(u64::from(r.config.steps_per_rotation()));
            w.f64(r.config.y_steps_per_mm());
            w.bool(r.config.mirror());
        }
        Ok(ContentHash::of(HashDomain::Profile, &w.finish().map_err(|_| PlanError::NotFinite)?))
    }

    /// The same profile with a rotary attached (M08). The flat-bed frame is remembered so that
    /// [`MachineProfile::without_rotary`] restores it exactly. With a chuck the bed is the
    /// unrolled surface: one wrap high (`π · object diameter`), same width and origin. A roller
    /// feeds the object through the unchanged bed. Attaching again replaces the setting and keeps
    /// the original flat bed.
    pub fn with_rotary(mut self, config: RotaryConfig) -> Result<Self, PlanError> {
        let flatbed = self.rotary.map_or(self.frame, |r| r.flatbed);
        self.frame = match config.kind() {
            RotaryKind::Chuck { .. } => {
                MachineFrame::new(flatbed.width_mm(), config.surface_per_rotation_mm(), flatbed.origin())?
            }
            RotaryKind::Roller { .. } => flatbed,
        };
        self.rotary = Some(RotaryState { config, flatbed });
        self.revision.content = self.content_hash()?;
        Ok(self)
    }

    /// The same profile back on the flat bed: frame and content hash are the ones it had before
    /// any rotary was attached; capabilities and limits changed meanwhile are kept.
    pub fn without_rotary(mut self) -> Self {
        if let Some(state) = self.rotary.take() {
            self.frame = state.flatbed;
            if let Ok(content) = self.content_hash() {
                self.revision.content = content;
            }
        }
        self
    }

    /// The flat-bed frame: the profile's frame, or the one it returns to without the rotary.
    pub fn flatbed_frame(&self) -> &MachineFrame {
        self.rotary.as_ref().map_or(&self.frame, |r| &r.flatbed)
    }

    pub fn rotary(&self) -> Option<&RotaryConfig> {
        self.rotary.as_ref().map(|r| &r.config)
    }

    pub fn capabilities(&self) -> &MachineCapabilities {
        &self.capabilities
    }

    pub fn revision(&self) -> &ProfileRevision {
        &self.revision
    }
    pub fn frame(&self) -> &MachineFrame {
        &self.frame
    }
    /// Highest working speed the machine accepts.
    pub fn max_speed_mm_s(&self) -> f64 {
        self.max_speed_mm_s
    }
    /// Laser-off travel speed (used for time estimates).
    pub fn travel_speed_mm_s(&self) -> f64 {
        self.travel_speed_mm_s
    }
    /// Allowed power range for operations on this machine.
    pub fn power_range(&self) -> (PowerRatio, PowerRatio) {
        (self.min_power, self.max_power)
    }
}

pub(crate) fn origin_tag(o: OriginCorner) -> &'static str {
    match o {
        OriginCorner::BottomLeft => "bottom-left",
        OriginCorner::TopLeft => "top-left",
        OriginCorner::TopRight => "top-right",
        OriginCorner::BottomRight => "bottom-right",
    }
}
