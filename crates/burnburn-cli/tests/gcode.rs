//! The CLI's GRBL backend: compile a project to `.gcode` (W4-L4). In-memory storage, no machine.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::sync::atomic::Ordering;

use burnburn_cli::CliError;
use burnburn_cli::profile_file::{Backend, parse_profile};
use burnburn_gcode::{GrblProfile, PowerMode};
use common::*;

const GRBL_PROFILE: &str = r#"{
  "id": "grbl-diode-400x300", "revision": 1, "bed_mm": [400, 300], "origin": "bottom-left",
  "max_speed_mm_s": 200, "travel_speed_mm_s": 100, "power_range": [0, 1],
  "backend": "grbl",
  "grbl": { "max_spindle": 1000, "air_assist": true }
}"#;

const GRBL_PROFILE_NO_AIR: &str = r#"{
  "id": "grbl-diode-400x300", "revision": 1, "bed_mm": [400, 300], "origin": "bottom-left",
  "max_speed_mm_s": 200, "travel_speed_mm_s": 100, "power_range": [0, 1],
  "backend": "grbl",
  "grbl": { "max_spindle": 255, "power_mode": "constant" }
}"#;

fn grbl_rig() -> Rig {
    let r = rig();
    r.put("/work/plaat.burnburn", project_bytes(&[(10.0, 10.0, 120.0, 80.0)]));
    r.put("/work/grbl.json", GRBL_PROFILE.as_bytes().to_vec());
    r
}

/// Independent reading of the G-code text: (burn length mm, spindle words on burn moves, feeds).
fn burn_length(text: &str) -> (f64, Vec<u32>, Vec<u32>) {
    let (mut x, mut y, mut s, mut f) = (0.0f64, 0.0f64, 0u32, 0u32);
    let mut on = false;
    let (mut len, mut powers, mut feeds) = (0.0, Vec::new(), Vec::new());
    for line in text.lines() {
        let line = line.split(';').next().unwrap().trim();
        let words: Vec<&str> = line.split_whitespace().collect();
        let Some(&g) = words.first() else { continue };
        if matches!(g, "M3" | "M4") {
            on = true;
        }
        if g == "M5" || g == "M2" {
            on = false;
        }
        let (mut nx, mut ny) = (x, y);
        for w in &words[1..] {
            let v = &w[1..];
            match &w[..1] {
                "X" => nx = v.parse().unwrap(),
                "Y" => ny = v.parse().unwrap(),
                "S" => s = v.parse().unwrap(),
                "F" => f = v.parse().unwrap(),
                _ => {}
            }
        }
        if matches!(g, "M3" | "M4") {
            // "M4 S0" sets S in the same word list.
        }
        if g == "G1" && on && s > 0 {
            len += (nx - x).hypot(ny - y);
            powers.push(s);
            feeds.push(f);
        }
        if g == "G0" || g == "G1" {
            x = nx;
            y = ny;
        }
    }
    (len, powers, feeds)
}

#[test]
fn a_grbl_profile_compiles_a_project_to_gcode() {
    let r = grbl_rig();
    let out = r.run("compile /work/plaat.burnburn --profile /work/grbl.json --out /work/plaat.gcode").unwrap();
    let text = String::from_utf8(r.file("/work/plaat.gcode").expect("G-code written")).unwrap();
    // Analytical: a 120 x 80 mm rectangle is 400 mm of cut at 60 % ($30=1000 -> S600), 20 mm/s -> F1200.
    let (len, powers, feeds) = burn_length(&text);
    assert!((len - 400.0).abs() < 1e-6, "{len}");
    assert!(powers.iter().all(|s| *s == 600), "{powers:?}");
    assert!(feeds.iter().all(|f| *f == 1200), "{feeds:?}");
    // The operation asks for air assist and the profile has it: on before the burn, off after.
    let lines: Vec<&str> = text.lines().collect();
    assert!(lines.iter().position(|l| *l == "M8").unwrap() < lines.iter().position(|l| l.starts_with("G1")).unwrap());
    assert!(lines.contains(&"M9"));
    assert_eq!(lines.last(), Some(&"M2"));
    // The program names the plan it was compiled from, and so does the CLI output.
    let plan = out.plan.expect("plan id");
    assert!(lines[0].contains(&plan.to_string()), "{}", lines[0]);
    assert!(out.stdout.contains(&format!("plan {plan}")), "{}", out.stdout);
    assert!(out.stdout.contains("gcode") && out.stdout.contains("/work/plaat.gcode"), "{}", out.stdout);
    assert!(out.job.is_none(), "no frozen RD job is recorded for G-code");
}

#[test]
fn compiling_twice_gives_byte_identical_gcode() {
    let r = grbl_rig();
    r.run("compile /work/plaat.burnburn --profile /work/grbl.json --out /work/a.gcode").unwrap();
    r.run("compile /work/plaat.burnburn --profile /work/grbl.json --out /work/b.gcode").unwrap();
    assert_eq!(r.file("/work/a.gcode"), r.file("/work/b.gcode"));
}

#[test]
fn air_assist_without_the_output_is_refused_with_the_reason_and_nothing_is_written() {
    let r = grbl_rig();
    r.put("/work/no-air.json", GRBL_PROFILE_NO_AIR.as_bytes().to_vec());
    let e = r.run("compile /work/plaat.burnburn --profile /work/no-air.json --out /work/plaat.gcode").unwrap_err();
    let CliError::Preflight { errors, report } = &e else { panic!("{e:?}") };
    assert!(*errors >= 1);
    assert!(report.contains("air assist"), "{report}");
    assert_eq!(e.exit_code(), 2);
    assert!(r.file("/work/plaat.gcode").is_none());
}

#[test]
fn constant_power_and_spindle_scale_come_from_the_profile() {
    let r = rig();
    // No air assist in the project operation: use a project whose operation has it off is not
    // available, so give the machine the output and only change power mode / scale.
    let profile = GRBL_PROFILE_NO_AIR
        .replace("\"power_mode\": \"constant\"", "\"power_mode\": \"constant\", \"air_assist\": true");
    r.put("/work/plaat.burnburn", project_bytes(&[(10.0, 10.0, 120.0, 80.0)]));
    r.put("/work/grbl.json", profile.into_bytes());
    r.run("compile /work/plaat.burnburn --profile /work/grbl.json --out /work/plaat.gcode").unwrap();
    let text = String::from_utf8(r.file("/work/plaat.gcode").unwrap()).unwrap();
    // 60 % of $30=255 is 153.
    let (_, powers, _) = burn_length(&text);
    assert!(!powers.is_empty() && powers.iter().all(|s| *s == 153), "{powers:?}");
    assert!(text.contains("M3 S0") && !text.contains("M4"), "{text}");
}

#[test]
fn a_project_beyond_the_bed_is_refused_by_preflight_before_any_gcode_is_written() {
    let r = grbl_rig();
    r.put("/work/groot.burnburn", project_bytes(&[(300.0, 10.0, 200.0, 80.0)]));
    let e = r.run("compile /work/groot.burnburn --profile /work/grbl.json --out /work/groot.gcode").unwrap_err();
    assert!(matches!(e, CliError::Preflight { .. }), "{e:?}");
    assert!(r.file("/work/groot.gcode").is_none());
}

#[test]
fn the_history_journal_records_rd_jobs_only() {
    let r = grbl_rig();
    let e = r
        .run("compile /work/plaat.burnburn --profile /work/grbl.json --out /work/plaat.gcode --history /h")
        .unwrap_err();
    assert!(matches!(e, CliError::Usage(ref m) if m.contains("RD")), "{e:?}");
    assert!(r.file("/work/plaat.gcode").is_none());
}

#[test]
fn upload_and_start_do_not_exist_for_grbl_and_contact_nothing() {
    let r = grbl_rig();
    for cmd in [
        "upload --history /h --job 0000000000000000000000000000000000000000000000000000000000000000 --profile /work/grbl.json --peer 192.0.2.10:50200 --allow-machine-contact",
        "start --history /h --execution 1 --profile /work/grbl.json --peer 192.0.2.10:50200 --allow-machine-contact",
    ] {
        let e = r.run(cmd).unwrap_err();
        assert!(matches!(e, CliError::Machine(ref m) if m.contains("GRBL")), "{cmd}: {e:?}");
    }
    assert_eq!(r.connector.connects.load(Ordering::SeqCst), 0);
}

#[test]
fn the_profile_file_selects_the_backend() {
    let ruida = parse_profile(PROFILE_JSON.as_bytes()).unwrap();
    assert_eq!(ruida.backend, Backend::Ruida);

    let grbl = parse_profile(GRBL_PROFILE.as_bytes()).unwrap();
    let Backend::Grbl(p) = &grbl.backend else { panic!("{:?}", grbl.backend) };
    assert_eq!(p, &GrblProfile { air_assist: true, ..GrblProfile::grbl_1_1_laser() });
    // The machine profile carries the GRBL capability matrix, so preflight judges by it.
    assert!(grbl.profile.capabilities().is_supported(burnburn_job::Feature::ScanAngle));
    assert!(!ruida.profile.capabilities().is_supported(burnburn_job::Feature::ScanAngle));

    let constant = parse_profile(GRBL_PROFILE_NO_AIR.as_bytes()).unwrap();
    let Backend::Grbl(p) = &constant.backend else { panic!() };
    assert_eq!((p.max_spindle, p.power_mode), (255, PowerMode::Constant));
}

#[test]
fn invalid_backend_sections_are_profile_errors() {
    let bad = |json: String| parse_profile(json.as_bytes()).unwrap_err();
    let base = |extra: &str| {
        format!(
            r#"{{"id":"x","revision":1,"bed_mm":[400,300],"origin":"bottom-left","max_speed_mm_s":100,"travel_speed_mm_s":100,"power_range":[0,1],{extra}}}"#
        )
    };
    assert!(matches!(bad(base(r#""backend":"marlin""#)), CliError::Profile(_)));
    assert!(matches!(bad(base(r#""backend":"grbl""#)), CliError::Profile(_)), "grbl needs its section");
    assert!(matches!(bad(base(r#""backend":"ruida""#)), CliError::Profile(_)), "ruida needs its section");
    assert!(matches!(bad(base(r#""backend":"grbl","grbl":{"max_spindle":0}"#)), CliError::Profile(_)));
    assert!(matches!(bad(base(r#""backend":"grbl","grbl":{"decimals":9}"#)), CliError::Profile(_)));
    assert!(matches!(bad(base(r#""backend":"grbl","grbl":{"power_mode":"x"}"#)), CliError::Profile(_)));
    assert!(matches!(bad(base(r#""backend":"grbl","grbl":{"work_coordinate":"G53"}"#)), CliError::Profile(_)));
    assert!(matches!(bad(base(r#""backend":"grbl","grbl":{"surprise":1}"#)), CliError::Profile(_)));
    // Both sections at once are ambiguous.
    assert!(matches!(
        bad(base(r#""backend":"grbl","grbl":{},"ruida":{"magic":136,"overscan":"reject"}"#)),
        CliError::Profile(_)
    ));
}
