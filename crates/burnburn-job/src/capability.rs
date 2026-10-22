//! Capability matrix per machine profile (contracts-phase-c principle 4, IM25).
//!
//! Every process option beyond plain Line/Fill names a [`Feature`]. A profile states for each
//! feature whether it is [`Support::Supported`], [`Support::Unsupported`] or
//! [`Support::Unverified`]. Only `Supported` may be compiled; `Unverified` behaves like
//! `Unsupported` until there is evidence for the concrete model/firmware (AGENTS.md). The
//! compiler never replaces a feature that is not supported by another one.

use std::collections::BTreeMap;

use burnburn_types::CanonicalWriter;

/// A process option that needs an explicit capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum Feature {
    /// Line kerf compensation (geometric offset; ordinary cut events).
    KerfCompensation,
    /// Concentric offset rings as cuts (ordinary cut events).
    OffsetFill,
    /// Scan lines at an angle other than 0°/90° (diagonal scan runs: not RD-confirmed).
    ScanAngle,
    /// Binary dithered image scanning (ordinary on/off scan runs).
    ImageDither,
    /// Power modulated per run (grayscale). Semantics of the RD records are OPEN.
    GrayscaleModulation,
    /// Uncut bridges on closed cut paths (laser-off gaps).
    Tabs,
    /// Lead-in / lead-out cuts on the waste side.
    Leads,
    /// Cutting past the entry point of a closed contour.
    Overcut,
    /// Alternating on/off segments along a cut path.
    Perforation,
    /// Focus/Z offset between passes (needs a Z axis the backend can drive).
    ZStep,
    /// Child operations with their own settings on the parent's geometry.
    SubOperations,
    /// Common-line cutting of touching parts (separately validated process, C12).
    CommonLine,
    /// Discrete laser pulses instead of a continuous cut (dot / dwell mode).
    DotMode,
    /// A second scan pass perpendicular to the first (ordinary scan events).
    CrossHatch,
    /// Fill regions scanned one connected region after the other (ordinary scan events).
    FloodFill,
    /// Fill grouping other than "all shapes at once" (ordinary scan events, other order).
    FillGrouping,
    /// Power ramp at the ends of scan runs (`RampRun` events).
    RampLength,
    /// Tabs cut with a lowered power instead of skipped (`CutPower` events, C04). Whether a
    /// controller takes a power per movement is a hardware fact, not a compiler one.
    TabCutPower,
}

impl Feature {
    /// Every feature, in canonical order.
    pub const ALL: [Feature; 18] = [
        Feature::KerfCompensation,
        Feature::OffsetFill,
        Feature::ScanAngle,
        Feature::ImageDither,
        Feature::GrayscaleModulation,
        Feature::Tabs,
        Feature::Leads,
        Feature::Overcut,
        Feature::Perforation,
        Feature::ZStep,
        Feature::SubOperations,
        Feature::CommonLine,
        Feature::DotMode,
        Feature::CrossHatch,
        Feature::FloodFill,
        Feature::FillGrouping,
        Feature::RampLength,
        Feature::TabCutPower,
    ];

    /// Number of features (a prefix of [`Feature::ALL`]) whose support is part of every profile
    /// hash; later ones only when overridden.
    const LEGACY: usize = 12;

    pub fn tag(self) -> &'static str {
        match self {
            Feature::KerfCompensation => "kerf",
            Feature::OffsetFill => "offset-fill",
            Feature::ScanAngle => "scan-angle",
            Feature::ImageDither => "image-dither",
            Feature::GrayscaleModulation => "grayscale-modulation",
            Feature::Tabs => "tabs",
            Feature::Leads => "leads",
            Feature::Overcut => "overcut",
            Feature::Perforation => "perforation",
            Feature::ZStep => "z-step",
            Feature::SubOperations => "sub-operations",
            Feature::CommonLine => "common-line",
            Feature::DotMode => "dot-mode",
            Feature::CrossHatch => "cross-hatch",
            Feature::FloodFill => "flood-fill",
            Feature::FillGrouping => "fill-grouping",
            Feature::RampLength => "ramp-length",
            Feature::TabCutPower => "tab-cut-power",
        }
    }
}

/// Whether a profile may use a feature.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Support {
    Supported,
    Unsupported,
    /// Not proven for this machine; treated as unsupported until there is evidence.
    Unverified {
        reason: String,
    },
}

/// The feature matrix of one profile. Features not set explicitly have their conservative
/// default (see [`MachineCapabilities::conservative`]).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MachineCapabilities {
    explicit: BTreeMap<Feature, Support>,
}

impl Default for MachineCapabilities {
    fn default() -> Self {
        Self::conservative()
    }
}

impl MachineCapabilities {
    /// Defaults without any machine evidence:
    /// - `Supported`: features the compiler expresses with ordinary travel, cut and axis-aligned
    ///   on/off scan events only (kerf, offset fill, image dither, tabs, leads, overcut,
    ///   perforation, sub-operations) — the same event kinds as plain Line/Fill.
    /// - `Unverified`: diagonal scan angles and grayscale power modulation (RD semantics OPEN,
    ///   `04-exportanalyse.md`), common-line cutting (needs its own validated process).
    /// - `Unsupported`: Z steps (no Z axis in any profile yet).
    pub fn conservative() -> Self {
        MachineCapabilities { explicit: BTreeMap::new() }
    }

    /// Sets the support of one feature.
    pub fn with(mut self, feature: Feature, support: Support) -> Self {
        self.explicit.insert(feature, support);
        self
    }

    pub fn support(&self, feature: Feature) -> Support {
        if let Some(s) = self.explicit.get(&feature) {
            return s.clone();
        }
        match feature {
            Feature::ScanAngle => Support::Unverified { reason: "diagonal scan runs are not RD-confirmed".into() },
            Feature::GrayscaleModulation => {
                Support::Unverified { reason: "grayscale power records (C7/C2) are not decoded".into() }
            }
            Feature::CommonLine => Support::Unverified { reason: "common-line cutting is not validated".into() },
            Feature::ZStep => Support::Unsupported,
            Feature::DotMode => Support::Unverified { reason: "dot pulses have no RD form yet".into() },
            Feature::RampLength => Support::Unverified { reason: "power ramps along a run have no RD form yet".into() },
            Feature::TabCutPower => {
                Support::Unverified { reason: "a power per movement has no proven RD form yet".into() }
            }
            _ => Support::Supported,
        }
    }

    pub fn is_supported(&self, feature: Feature) -> bool {
        self.support(feature) == Support::Supported
    }

    /// Canonical projection: the effective support of every feature in [`Feature::ALL`] order.
    /// The first [`Feature::LEGACY`] features are always written (so profiles keep the hash they
    /// had before later features existed); a later feature is written only when its support
    /// differs from the conservative default.
    pub(crate) fn encode(&self, w: &mut CanonicalWriter) {
        let default = MachineCapabilities::conservative();
        for (i, f) in Feature::ALL.into_iter().enumerate() {
            if i >= Feature::LEGACY && self.support(f) == default.support(f) {
                continue;
            }
            w.tag(f.tag());
            match self.support(f) {
                Support::Supported => w.tag("supported"),
                Support::Unsupported => w.tag("unsupported"),
                Support::Unverified { reason } => {
                    w.tag("unverified");
                    w.str(&reason);
                }
            }
        }
    }
}
