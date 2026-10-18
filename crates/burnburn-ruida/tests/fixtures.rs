//! Contract tests against the three real LightBurn Core 2.1.04 exports.
//!
//! Expected values come from `research/lightburn/ruida/04-exportanalyse.md` (the original UI
//! conditions: 100 mm/s, min/max 20 %, 30 × 15 mm rectangle) and from the independent Python
//! decode in `test-data/ruida/oracle/*-decoded.json` — never from this crate's own output.

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers outside #[test] fns

mod common;

use burnburn_ruida::{AbsCoord, BoundsKind, Command, Laser, Magic, Power, PowerBound, Speed, decode_rd_file};
use common::{fixture, oracle};

fn xy(x: u64, y: u64) -> (AbsCoord, AbsCoord) {
    (AbsCoord::from_um(x).unwrap(), AbsCoord::from_um(y).unwrap())
}

#[test]
fn reference_line_fixture_decodes_expected_bounds_and_power() {
    let bytes = fixture("rectangle-default.rd");
    assert_eq!(bytes.len(), 525, "fixture size per fixtures-manifest.json");

    let file = decode_rd_file(&bytes, Magic::FIXTURE_0X88).unwrap();
    let cmds: Vec<&Command> = file.commands().collect();

    // Record count from the independent decode (73 high-bit records).
    assert_eq!(cmds.len(), 73);

    // UI speed 100 mm/s → 100000 µm/s; UI 20 % → raw14 3277 for laser-1 min and max.
    assert!(cmds.contains(&&Command::Speed(Speed::from_um_per_s(100_000).unwrap())));
    let p20 = Power::from_raw(3277).unwrap();
    assert!(cmds.contains(&&Command::LaserPower { laser: Laser::L1, bound: PowerBound::Min, value: p20 }));
    assert!(cmds.contains(&&Command::LaserPower { laser: Laser::L1, bound: PowerBound::Max, value: p20 }));

    // Document bounds (header rounding: X-min 771690 µm vs motion 771687 µm, see 04-exportanalyse).
    let (x0, y0) = xy(771_690, 448_020);
    let (x1, y1) = xy(801_690, 463_020);
    assert!(cmds.contains(&&Command::Bounds { kind: BoundsKind::DocumentMin, x: x0, y: y0 }));
    assert!(cmds.contains(&&Command::Bounds { kind: BoundsKind::DocumentMax, x: x1, y: y1 }));

    // Motion: one travel then four absolute cuts around the 30 × 15 mm frame.
    let motion: Vec<&Command> =
        cmds.iter().copied().filter(|c| matches!(c, Command::MoveAbs { .. } | Command::CutAbs { .. })).collect();
    let mv = |x, y| {
        let (x, y) = xy(x, y);
        Command::MoveAbs { x, y }
    };
    let cut = |x, y| {
        let (x, y) = xy(x, y);
        Command::CutAbs { x, y }
    };
    assert_eq!(
        motion,
        vec![
            &mv(801_687, 448_024),
            &cut(801_687, 463_024),
            &cut(771_687, 463_024),
            &cut(771_687, 448_024),
            &cut(801_687, 448_024),
        ]
    );

    // Footer: file sum 24999 and end record.
    assert_eq!(file.file_sum, 24_999);
    assert_eq!(cmds.last(), Some(&&Command::EndOfFile));
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ")
}

/// Field-by-field comparison with the independent Python decode (`rd_inspect.py --json`).
fn assert_matches_independent_decode(rd: &str, json: &str, expected_sum: u64) {
    let file = decode_rd_file(&fixture(rd), Magic::FIXTURE_0X88).unwrap();
    let oracle = oracle(json);
    let expected = oracle["records"].as_array().unwrap();
    assert_eq!(file.records.len(), expected.len(), "{rd}: record count");
    assert_eq!(file.file_sum, expected_sum, "{rd}: file sum");

    for (rec, exp) in file.records.iter().zip(expected) {
        let at = exp["offset"].as_u64().unwrap() as usize;
        assert_eq!(rec.offset, at, "{rd}: offset");
        assert_eq!(hex(&file.raw[rec.offset..rec.offset + rec.len]), exp["hex"].as_str().unwrap(), "{rd}@{at:#x}");
        let unnamed = exp["names"].as_array().unwrap().is_empty();
        assert_eq!(matches!(rec.command, Command::Unknown { .. }), unnamed, "{rd}@{at:#x}: unknown-ness");

        if let Some(v) = exp.get("speed_mm_s") {
            let um = (v.as_f64().unwrap() * 1000.0).round() as u64;
            assert_eq!(rec.command, Command::Speed(Speed::from_um_per_s(um).unwrap()), "{rd}@{at:#x}");
        }
        if let Some(v) = exp.get("power_raw14") {
            let raw = v.as_u64().unwrap() as u16;
            assert!(matches!(rec.command, Command::LaserPower { value, .. } if value.raw() == raw), "{rd}@{at:#x}");
        }
        if let Some(v) = exp.get("modulation_raw14") {
            let raw = v.as_u64().unwrap() as u16;
            assert!(matches!(rec.command, Command::ImmediatePower { value, .. } if value.raw() == raw), "{rd}@{at:#x}");
        }
        if let Some(v) = exp.get("delta_um") {
            let d = v.as_i64().unwrap() as i32;
            let got = match rec.command {
                Command::MoveRelX(r) | Command::MoveRelY(r) | Command::CutRelX(r) | Command::CutRelY(r) => r.um(),
                ref other => panic!("{rd}@{at:#x}: expected single-axis delta, got {other:?}"),
            };
            assert_eq!(got, d, "{rd}@{at:#x}");
        }
        if let (Some(v), None) = (exp.get("xy_mm"), exp.get("delta_um")) {
            let mm: Vec<u64> =
                v.as_array().unwrap().iter().map(|c| (c.as_f64().unwrap() * 1000.0).round() as u64).collect();
            let (x, y) = match rec.command {
                Command::MoveAbs { x, y } | Command::CutAbs { x, y } => (x.um(), y.um()),
                ref other => panic!("{rd}@{at:#x}: expected absolute motion, got {other:?}"),
            };
            assert_eq!(vec![x, y], mm, "{rd}@{at:#x}");
        }
    }
}

#[test]
fn all_fixtures_match_independent_python_decode() {
    // File sums as recorded in 02-binaire-specificatie.md §6.
    assert_matches_independent_decode("rectangle-default.rd", "rectangle-default-decoded.json", 24_999);
    assert_matches_independent_decode("rectangle-fill.rd", "rectangle-fill-decoded.json", 142_815);
    assert_matches_independent_decode("gray-ramp-grayscale.rd", "gray-ramp-grayscale-decoded.json", 741_544);
}

#[test]
fn unknown_f2_01_record_is_preserved_not_dropped() {
    // 04-exportanalyse: `F2 01 00` at offset 240 has no registry label.
    let file = decode_rd_file(&fixture("rectangle-default.rd"), Magic::FIXTURE_0X88).unwrap();
    let rec = file.records.iter().find(|r| r.offset == 240).unwrap();
    assert_eq!(rec.command, Command::Unknown { opcode: 0xf2, bytes: vec![0xf2, 0x01, 0x00] });
}
