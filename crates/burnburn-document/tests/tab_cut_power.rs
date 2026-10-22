//! Tab Cut Power (C04, G7-cam): tabs can be cut with a lowered power instead of being skipped.
//! The power is validated at construction, is part of the canonical hash, and a tab without it
//! keeps the hash it had before.

#![allow(clippy::unwrap_used)]

use burnburn_document::process::{
    AdvancedOptions, Kerf, LineOptions, Operation, OperationDraft, OperationMode, Passes, PowerPercent, PowerRange,
    ProcessError, ProcessSettings, Speed, TabSpec,
};
use burnburn_types::{CanonicalEncode, CanonicalWriter, GeometryId, Length, OperationId, StableId};

fn mm(v: f64) -> Length {
    Length::from_mm(v).unwrap()
}

fn operation(tabs: TabSpec) -> Operation {
    let p = PowerPercent::new(20.0).unwrap();
    let settings = ProcessSettings {
        speed: Speed::mm_per_s(100.0).unwrap(),
        power: PowerRange::new(p, p).unwrap(),
        passes: Passes::new(1).unwrap(),
        air_assist: true,
    };
    let draft = OperationDraft {
        name: "Op".into(),
        mode: OperationMode::Line { kerf: Kerf::NONE },
        settings,
        output_enabled: true,
    };
    Operation::new(OperationId::from_raw(5), draft, vec![GeometryId::from_raw(1)], None)
        .unwrap()
        .with_advanced(AdvancedOptions {
            line: LineOptions { tabs: Some(tabs), ..Default::default() },
            ..Default::default()
        })
        .unwrap()
}

fn bytes(op: &Operation) -> Vec<u8> {
    let mut w = CanonicalWriter::new();
    op.encode(&mut w);
    w.finish().unwrap()
}

#[test]
fn tabs_skip_the_cut_unless_a_power_is_set() {
    let tabs = TabSpec::new(4, mm(1.0)).unwrap();
    assert_eq!(tabs.cut_power_percent(), None);
    let lowered = tabs.with_cut_power(25.0).unwrap();
    assert_eq!(lowered.cut_power_percent(), Some(25.0));
    // The rest of the spec is untouched.
    assert_eq!((lowered.count(), lowered.width_mm()), (4, 1.0));
    assert_eq!(lowered.without_cut_power().cut_power_percent(), None);
}

#[test]
fn a_tab_power_must_lie_strictly_between_skipping_and_a_full_cut() {
    let tabs = TabSpec::new(4, mm(1.0)).unwrap();
    for bad in [0.0, -5.0, 100.0, 140.0, f64::NAN, f64::INFINITY] {
        assert_eq!(tabs.with_cut_power(bad), Err(ProcessError::OutOfRange { field: "tab cut power" }), "{bad}");
    }
    for good in [0.1, 50.0, 99.9] {
        assert!(tabs.with_cut_power(good).is_ok(), "{good}");
    }
}

#[test]
fn the_tab_power_changes_the_hash_for_both_placements_and_a_plain_tab_keeps_its_own() {
    let counted = TabSpec::new(4, mm(1.0)).unwrap();
    let spaced = TabSpec::spaced(mm(25.0), mm(1.0)).unwrap();
    for plain in [counted, spaced] {
        let a = bytes(&operation(plain));
        let b = bytes(&operation(plain.with_cut_power(25.0).unwrap()));
        let c = bytes(&operation(plain.with_cut_power(30.0).unwrap()));
        assert_ne!(a, b, "a lowered power is part of the content");
        assert_ne!(b, c, "so is its value");
        assert_eq!(a, bytes(&operation(plain.with_cut_power(25.0).unwrap().without_cut_power())));
    }
}
