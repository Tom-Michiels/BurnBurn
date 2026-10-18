use burnburn_types::{CanonicalWriter, ContentHash, HashDomain};

use crate::{MachineCapabilities, MachineFrame, OriginCorner, PlanError, PowerRatio};

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
        Ok(ContentHash::of(HashDomain::Profile, &w.finish().map_err(|_| PlanError::NotFinite)?))
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
