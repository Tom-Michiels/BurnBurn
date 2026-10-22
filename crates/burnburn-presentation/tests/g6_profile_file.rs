//! G6-machine: the machine profile file (Ruida or GRBL) is read by `burnburn_presentation`, so
//! the CLI and the hosts read one format. The file's model and firmware are what the user wrote
//! there, nothing a controller said.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_gcode::{GrblProfile, PowerMode, WorkCoordinate};
use burnburn_presentation::profile_file::{Backend, parse_profile};

const RUIDA_FILE: &str = r#"{
  "id": "bench-1500x900", "revision": 1, "bed_mm": [1500, 900], "origin": "bottom-left",
  "max_speed_mm_s": 1000, "travel_speed_mm_s": 400, "power_range": [0, 1],
  "ruida": { "magic": 136, "overscan": "reject" },
  "identity": { "machine_id": "ruida-bench-1", "model": "RDC6445G", "firmware": "fake-1" }
}"#;

const GRBL_FILE: &str = r#"{
  "id": "grbl-diode-400x300", "revision": 2, "bed_mm": [400, 300], "origin": "top-left",
  "max_speed_mm_s": 200, "travel_speed_mm_s": 100, "power_range": [0, 1],
  "backend": "grbl",
  "grbl": { "max_spindle": 255, "laser_mode": true, "power_mode": "constant", "air_assist": true,
            "work_coordinate": "G55", "decimals": 2 },
  "identity": { "machine_id": "grbl-1", "firmware": "1.1h" }
}"#;

#[test]
fn a_ruida_file_gives_the_profile_the_dialect_and_the_declared_identity() {
    let f = parse_profile(RUIDA_FILE.as_bytes()).unwrap();
    assert_eq!(f.backend, Backend::Ruida);
    assert_eq!(f.profile.revision().id, "bench-1500x900");
    assert_eq!(f.profile.frame().width_mm(), 1500.0);
    assert_eq!(f.ruida.magic.0, 136);
    let id = f.identity.expect("identity section");
    assert_eq!((id.model.as_deref(), id.firmware.as_deref()), (Some("RDC6445G"), Some("fake-1")));
}

#[test]
fn a_grbl_file_gives_every_dialect_field_and_the_grbl_capability_matrix() {
    let f = parse_profile(GRBL_FILE.as_bytes()).unwrap();
    let Backend::Grbl(grbl) = &f.backend else { panic!("{:?}", f.backend) };
    assert_eq!(
        grbl,
        &GrblProfile {
            max_spindle: 255,
            laser_mode: true,
            power_mode: PowerMode::Constant,
            air_assist: true,
            work_coordinate: Some(WorkCoordinate::G55),
            decimals: 2,
        }
    );
    assert!(f.profile.capabilities().is_supported(burnburn_job::Feature::ScanAngle), "the GRBL matrix is the profile's");
    let id = f.identity.expect("identity section");
    assert_eq!((id.model, id.firmware.as_deref()), (None, Some("1.1h")), "a missing model stays missing");
}

#[test]
fn damaged_and_ambiguous_files_name_their_reason_and_give_nothing() {
    let base = |extra: &str| {
        format!(
            r#"{{"id":"x","revision":1,"bed_mm":[400,300],"origin":"bottom-left","max_speed_mm_s":100,"travel_speed_mm_s":100,"power_range":[0,1],{extra}}}"#
        )
    };
    for extra in [
        r#""backend":"marlin""#,
        r#""backend":"grbl""#,
        r#""backend":"ruida""#,
        r#""backend":"grbl","grbl":{"max_spindle":0}"#,
        r#""backend":"grbl","grbl":{"decimals":9}"#,
        r#""backend":"grbl","grbl":{"power_mode":"x"}"#,
        r#""backend":"grbl","grbl":{"work_coordinate":"G53"}"#,
        r#""backend":"grbl","grbl":{"surprise":1}"#,
        r#""backend":"grbl","grbl":{},"ruida":{"magic":136,"overscan":"reject"}"#,
    ] {
        let e = parse_profile(base(extra).as_bytes()).unwrap_err();
        assert!(!e.to_string().is_empty(), "{extra}");
    }
    assert!(parse_profile(b"not json").is_err());
    assert!(parse_profile(b"").is_err());
}
