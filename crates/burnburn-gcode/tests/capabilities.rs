//! The GRBL capability matrix: every `Feature` has an explicit, reviewed decision.
#![allow(clippy::unwrap_used)]

use burnburn_gcode::{GrblProfile, grbl_capabilities};
use burnburn_job::{Feature, MachineCapabilities, Support};

#[derive(Clone, Copy, PartialEq, Debug)]
enum Want {
    Supported,
    Unsupported,
    Unverified,
}

/// The reviewed decision per feature. A new `Feature` variant makes
/// `the_matrix_decides_every_feature` fail until it is decided here (and in the backend).
const MATRIX: [(Feature, Want); 17] = [
    // Ordinary G0/G1 motion with S/F words: expressed exactly.
    (Feature::KerfCompensation, Want::Supported),
    (Feature::OffsetFill, Want::Supported),
    (Feature::ImageDither, Want::Supported),
    (Feature::Tabs, Want::Supported),
    (Feature::Leads, Want::Supported),
    (Feature::Overcut, Want::Supported),
    (Feature::Perforation, Want::Supported),
    (Feature::SubOperations, Want::Supported),
    (Feature::CrossHatch, Want::Supported),
    (Feature::FloodFill, Want::Supported),
    (Feature::FillGrouping, Want::Supported),
    // A diagonal run is one G1; a PowerRun is one G1 with its own S.
    (Feature::ScanAngle, Want::Supported),
    (Feature::GrayscaleModulation, Want::Supported),
    // No lowering exists for these; they must stay refused, not approximated.
    (Feature::ZStep, Want::Unsupported),
    (Feature::DotMode, Want::Unsupported),
    (Feature::RampLength, Want::Unsupported),
    // Needs its own validated process (same as every backend).
    (Feature::CommonLine, Want::Unverified),
];

fn classify(s: &Support) -> Want {
    match s {
        Support::Supported => Want::Supported,
        Support::Unsupported => Want::Unsupported,
        Support::Unverified { .. } => Want::Unverified,
    }
}

#[test]
fn the_matrix_decides_every_feature() {
    let caps = grbl_capabilities(&GrblProfile::grbl_1_1_laser());
    for f in Feature::ALL {
        let want = MATRIX.iter().find(|(m, _)| *m == f).map(|(_, w)| *w);
        assert!(want.is_some(), "feature {f:?} has no reviewed GRBL decision");
    }
    for (f, want) in MATRIX {
        assert_eq!(classify(&caps.support(f)), want, "{f:?}");
    }
}

#[test]
fn unverified_features_carry_a_reason() {
    let caps = grbl_capabilities(&GrblProfile::grbl_1_1_laser());
    let Support::Unverified { reason } = caps.support(Feature::CommonLine) else { panic!() };
    assert!(!reason.is_empty());
}

#[test]
fn the_matrix_differs_from_the_conservative_defaults_so_a_profile_hash_tells_backends_apart() {
    let grbl = grbl_capabilities(&GrblProfile::grbl_1_1_laser());
    assert_ne!(grbl, MachineCapabilities::conservative());
}

#[test]
fn a_machine_profile_built_from_the_matrix_is_a_different_revision() {
    use burnburn_job::{MachineFrame, MachineProfile, OriginCorner, PowerRatio};
    let frame = MachineFrame::new(400.0, 300.0, OriginCorner::BottomLeft).unwrap();
    let make = || {
        MachineProfile::new("x", 1, frame, 300.0, 300.0, PowerRatio::new(0.0).unwrap(), PowerRatio::new(1.0).unwrap())
            .unwrap()
    };
    let plain = make();
    let grbl = make().with_capabilities(grbl_capabilities(&GrblProfile::grbl_1_1_laser())).unwrap();
    assert_ne!(plain.revision().content, grbl.revision().content);
    assert!(grbl.capabilities().is_supported(Feature::ScanAngle));
}
