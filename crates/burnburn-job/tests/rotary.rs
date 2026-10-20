//! W2-L3 / M08: rotary as a profile setting. Oracles are circumferences chosen so that the
//! expected scale is a round number, not the implementation's formula.
#![allow(clippy::unwrap_used)]

use burnburn_job::{MachineFrame, MachineProfile, OriginCorner, PowerRatio, RotaryConfig, RotaryKind};

/// A chuck/roller diameter whose circumference is exactly 200 mm.
const D200: f64 = 200.0 / std::f64::consts::PI;

fn flatbed() -> MachineProfile {
    MachineProfile::new(
        "bed",
        1,
        MachineFrame::new(600.0, 400.0, OriginCorner::TopLeft).unwrap(),
        500.0,
        300.0,
        PowerRatio::new(0.0).unwrap(),
        PowerRatio::new(1.0).unwrap(),
    )
    .unwrap()
}

fn chuck(steps: u32, y_steps_per_mm: f64, mirror: bool) -> RotaryConfig {
    RotaryConfig::new(RotaryKind::Chuck { object_diameter_mm: D200 }, steps, y_steps_per_mm, mirror).unwrap()
}

#[test]
fn a_chuck_turns_one_circumference_per_rotation_and_the_scale_maps_it_onto_the_y_axis() {
    // 200 mm of surface per rotation; 8000 steps per rotation on an axis of 20 steps/mm:
    // one rotation is 400 controller millimetres, so every surface millimetre is two.
    let c = chuck(8000, 20.0, false);
    assert!((c.surface_per_rotation_mm() - 200.0).abs() < 1e-9);
    assert!((c.y_scale() - 2.0).abs() < 1e-12);
    assert!((c.to_controller_y(50.0, 200.0) - 100.0).abs() < 1e-9);
}

#[test]
fn a_roller_ignores_the_object_diameter() {
    let roller = RotaryConfig::new(RotaryKind::Roller { roller_diameter_mm: D200 }, 8000, 20.0, false).unwrap();
    assert!((roller.surface_per_rotation_mm() - 200.0).abs() < 1e-9);
    assert!((roller.y_scale() - 2.0).abs() < 1e-12, "same rotation, same scale as the chuck of equal circumference");
    let other_object = chuck(8000, 20.0, false);
    assert_eq!(roller.y_scale(), other_object.y_scale());
}

#[test]
fn mirroring_turns_the_object_the_other_way() {
    let c = chuck(8000, 20.0, true);
    // Frame height 200 (one wrap): surface 50 becomes 150 before scaling.
    assert!((c.to_controller_y(50.0, 200.0) - 300.0).abs() < 1e-9);
    assert!((c.to_controller_y(0.0, 200.0) - 400.0).abs() < 1e-9);
}

#[test]
fn invalid_rotary_values_are_refused() {
    let kind = RotaryKind::Chuck { object_diameter_mm: 50.0 };
    assert!(RotaryConfig::new(kind, 0, 20.0, false).is_err());
    assert!(RotaryConfig::new(kind, 8000, 0.0, false).is_err());
    assert!(RotaryConfig::new(kind, 8000, f64::NAN, false).is_err());
    assert!(RotaryConfig::new(RotaryKind::Chuck { object_diameter_mm: 0.0 }, 8000, 20.0, false).is_err());
    assert!(RotaryConfig::new(RotaryKind::Roller { roller_diameter_mm: -1.0 }, 8000, 20.0, false).is_err());
    assert!(RotaryConfig::new(RotaryKind::Chuck { object_diameter_mm: 2000.1 }, 8000, 20.0, false).is_err());
}

#[test]
fn returning_to_flatbed_restores_profile() {
    let base = flatbed();
    let rotary = base.clone().with_rotary(chuck(8000, 20.0, false)).unwrap();
    assert!(rotary.rotary().is_some());
    assert_ne!(rotary.revision().content, base.revision().content, "the rotary is profile content");
    // A chuck makes the bed the unrolled surface: one wrap high, same width and origin.
    assert!((rotary.frame().height_mm() - 200.0).abs() < 1e-9);
    assert_eq!(rotary.frame().width_mm(), 600.0);
    assert_eq!(rotary.frame().origin(), OriginCorner::TopLeft);

    let back = rotary.without_rotary();
    assert_eq!(back, base, "every field is the one the profile had before");
    assert_eq!(back.revision().content, base.revision().content);
    assert!(back.rotary().is_none());
}

#[test]
fn a_roller_keeps_the_bed_frame_and_other_profile_changes_survive_the_round_trip() {
    use burnburn_job::{Feature, MachineCapabilities, Support};
    let caps = MachineCapabilities::conservative().with(Feature::ZStep, Support::Supported);
    let base = flatbed().with_capabilities(caps).unwrap();
    let roller = RotaryConfig::new(RotaryKind::Roller { roller_diameter_mm: 30.0 }, 4000, 20.0, false).unwrap();
    let on = base.clone().with_rotary(roller).unwrap();
    assert_eq!(on.frame(), base.frame(), "rollers feed a long object through the same bed");
    assert!(on.capabilities().is_supported(Feature::ZStep));
    assert_eq!(on.without_rotary(), base);
}

#[test]
fn switching_the_rotary_twice_keeps_the_flat_bed_and_replaces_the_setting() {
    let base = flatbed();
    let first = base.clone().with_rotary(chuck(8000, 20.0, false)).unwrap();
    let second = first.with_rotary(chuck(4000, 20.0, true)).unwrap();
    assert!((second.rotary().unwrap().y_scale() - 1.0).abs() < 1e-12, "4000 / (200 · 20)");
    assert_eq!(second.without_rotary(), base, "the flat bed is the one from before the first rotary");
}

#[test]
fn flatbed_profiles_keep_their_content_hash() {
    // A profile that never had a rotary hashes exactly as before this field existed: the only
    // thing that matters to stored plans is that two identical profiles agree.
    assert_eq!(flatbed().revision().content, flatbed().revision().content);
    assert_ne!(
        flatbed().with_rotary(chuck(8000, 20.0, false)).unwrap().revision().content,
        flatbed().with_rotary(chuck(8000, 20.0, true)).unwrap().revision().content,
        "mirror is content"
    );
}

mod plan {
    use super::*;
    use burnburn_job::OperationSettings;
    use burnburn_job::{JobPlan, JobPlanBuilder, MachinePoint, PlanEvent, PlanMode, PlanSource, ProfileRevision};
    use burnburn_types::{ContentHash, OperationId, RevisionId, StableId};

    fn pt(x: f64, y: f64) -> MachinePoint {
        MachinePoint::new(x, y).unwrap()
    }

    fn build(rotary: Option<RotaryConfig>) -> JobPlan {
        let source = PlanSource {
            document_revision: RevisionId::from_raw(1),
            source_hash: ContentHash::from_bytes([7; 32]),
            machine_profile: ProfileRevision {
                id: "bed".into(),
                revision: 1,
                content: ContentHash::from_bytes([3; 32]),
            },
            compiler_version: "test".into(),
        };
        let frame = MachineFrame::new(600.0, 200.0, OriginCorner::BottomLeft).unwrap();
        let mut b = JobPlanBuilder::new(source, frame, pt(0.0, 0.0));
        if let Some(r) = rotary {
            b = b.with_rotary(r);
        }
        let p = PowerRatio::new(0.2).unwrap();
        let s = b
            .add_settings(OperationSettings {
                mode: PlanMode::Line,
                speed_mm_s: 10.0,
                min_power: p,
                max_power: p,
                air_assist: false,
            })
            .unwrap();
        b.push(PlanEvent::OperationBoundary { operation: OperationId::from_raw(2), settings: s, pass: 0 }).unwrap();
        b.push(PlanEvent::Travel { to: pt(1.0, 1.0) }).unwrap();
        b.push(PlanEvent::Cut { to: pt(1.0, 51.0), origin: None }).unwrap();
        b.finish().unwrap()
    }

    #[test]
    fn a_plan_records_its_rotary_and_the_id_depends_on_it() {
        let flat = build(None);
        let turned = build(Some(chuck(8000, 20.0, false)));
        assert!(flat.rotary().is_none());
        assert_eq!(turned.rotary(), Some(&chuck(8000, 20.0, false)));
        assert_ne!(flat.id(), turned.id());
        assert_ne!(turned.id(), build(Some(chuck(8000, 20.0, true))).id());
        assert_eq!(turned.id(), build(Some(chuck(8000, 20.0, false))).id());
        // Positions stay surface millimetres: the rotary does not move any event.
        assert_eq!(flat.events(), turned.events());
        assert_eq!(flat.bounds(), turned.bounds());
    }
}
