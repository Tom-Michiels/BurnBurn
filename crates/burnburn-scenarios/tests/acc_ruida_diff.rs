//! Differential Ruida export corpus (acceptance plan, "Differentieel Ruida-exportcorpus"): one factor
//! at a time, document -> compile -> RD, and the RD read back by an INDEPENDENT decoder.
//!
//! The independent decoder is the research tool `research/lightburn/tools/rd_inspect.py` (pure
//! Python 3 standard library, run as `python3 -I`, no installs). It only splits and unscrambles the
//! records; the numbers are then read from its hex strings here, straight from the field layout of
//! `research/lightburn/ruida/02-binaire-specificatie.md` (7-bit groups, 5-byte absolute and 2-byte
//! signed relative coordinates, 2-byte power, 5-byte speed). Neither BurnBurn's encoder nor its
//! decoder takes part in reading the motion back.
//!
//! If `python3` cannot be started the tests say so and pass vacuously unless
//! `BURNBURN_REQUIRE_PYTHON=1` is set (then they fail). The hardware bench (the controller really
//! accepting and burning these files) is not part of this corpus.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test code

mod acc_cam;
mod common;

use std::path::PathBuf;
use std::process::Command;

use burnburn_cam::{JobAnchor, Placement, compile};
use burnburn_document::process::{
    FillSettings, Kerf, OperationDraft, OperationMode, OptimizationSettings, Passes,
    PowerPercent, PowerRange, ProcessCommand, ProcessSettings, ScanAxis, ScanDirection, Speed,
};
use burnburn_document::{CommandPayload, DocumentSession, GeometryCommand};
use burnburn_geometry::{PathBuilder, Point};
use burnburn_job::{JobPlan, MachineFrame, MachineProfile, OriginCorner, PlanEvent, PowerRatio};
use burnburn_test_support::fixtures::repo_root;
use burnburn_types::CancellationToken;

// ---------------------------------------------------------------------------------------------
// the independent decoder
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct Rec {
    offset: usize,
    hex: Vec<u8>,
    names: Vec<String>,
}

fn python_available() -> bool {
    Command::new("python3").arg("-I").arg("--version").output().is_ok_and(|o| o.status.success())
}

/// `None` when python3 is not there (and the run is allowed to skip).
fn oracle(rd: &[u8], tag: &str) -> Option<Vec<Rec>> {
    if !python_available() {
        assert!(std::env::var("BURNBURN_REQUIRE_PYTHON").is_err(), "python3 is required (BURNBURN_REQUIRE_PYTHON=1)");
        eprintln!("SKIPPED {tag}: python3 is not available, the independent decoder did not run");
        return None;
    }
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join(format!("acc-ruida-{tag}.rd"));
    std::fs::write(&file, rd).unwrap();
    let tool = repo_root().join("research/lightburn/tools/rd_inspect.py");
    let out = Command::new("python3").arg("-I").arg(tool).arg("--json").arg(&file).output().expect("python3 runs");
    assert!(out.status.success(), "rd_inspect.py failed: {}", String::from_utf8_lossy(&out.stderr));
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let recs = json["records"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| Rec {
            offset: r["offset"].as_u64().unwrap() as usize,
            hex: r["hex"]
                .as_str()
                .unwrap()
                .split(' ')
                .map(|h| u8::from_str_radix(h, 16).unwrap())
                .collect(),
            names: r["names"].as_array().unwrap().iter().map(|n| n.as_str().unwrap().to_owned()).collect(),
        })
        .collect();
    Some(recs)
}

fn unpack7(b: &[u8]) -> i64 {
    assert!(b.iter().all(|v| *v < 128), "not a 7-bit payload: {b:02x?}");
    b.iter().fold(0_i64, |acc, v| (acc << 7) | i64::from(*v))
}

fn signed14(b: &[u8]) -> i64 {
    let v = unpack7(b);
    if v & 0x2000 != 0 { v - 0x4000 } else { v }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Seg {
    cut: bool,
    from: (i64, i64),
    to: (i64, i64),
    opcode: u8,
}

#[derive(Debug, Default)]
struct Read {
    segs: Vec<Seg>,
    /// (min/max, raw14) of laser 1 powers in file order.
    powers: Vec<(u8, u16)>,
    speeds: Vec<i64>,
    doc_min: Option<(i64, i64)>,
    doc_max: Option<(i64, i64)>,
}

fn read(recs: &[Rec]) -> Read {
    let mut r = Read::default();
    let mut at: Option<(i64, i64)> = None;
    for rec in recs {
        let h = &rec.hex;
        let abs = |h: &[u8]| (unpack7(&h[1..6]), unpack7(&h[6..11]));
        let (cut, to, op) = match h[0] {
            0x88 => (false, abs(h), 0x88),
            0xa8 => (true, abs(h), 0xa8),
            0x89 | 0xa9 => {
                let p = at.expect("relative move before an absolute one");
                (h[0] == 0xa9, (p.0 + signed14(&h[1..3]), p.1 + signed14(&h[3..5])), h[0])
            }
            0x8a | 0xaa => {
                let p = at.expect("relative move before an absolute one");
                (h[0] == 0xaa, (p.0 + signed14(&h[1..3]), p.1), h[0])
            }
            0x8b | 0xab => {
                let p = at.expect("relative move before an absolute one");
                (h[0] == 0xab, (p.0, p.1 + signed14(&h[1..3])), h[0])
            }
            0xc6 if h[1] == 0x01 || h[1] == 0x02 => {
                r.powers.push((h[1], unpack7(&h[2..4]) as u16));
                continue;
            }
            0xc9 if h[1] == 0x02 => {
                r.speeds.push(unpack7(&h[2..7]));
                continue;
            }
            0xe7 if h[1] == 0x50 => {
                r.doc_min = Some((unpack7(&h[2..7]), unpack7(&h[7..12])));
                continue;
            }
            0xe7 if h[1] == 0x51 => {
                r.doc_max = Some((unpack7(&h[2..7]), unpack7(&h[7..12])));
                continue;
            }
            _ => continue,
        };
        // The tool's own registry names must agree with the opcode class we read the numbers by.
        let wanted = if cut { "CUT" } else { "MOVE" };
        assert!(rec.names.iter().any(|n| n.contains(wanted)), "record {:02x?} is named {:?}", rec.hex, rec.names);
        let from = at.unwrap_or(to);
        r.segs.push(Seg { cut, from, to, opcode: op });
        at = Some(to);
    }
    r
}

impl Read {
    fn cuts(&self) -> Vec<Seg> {
        self.segs.iter().copied().filter(|s| s.cut).collect()
    }

    fn cut_extent(&self) -> ((i64, i64), (i64, i64)) {
        let pts: Vec<(i64, i64)> = self.cuts().iter().flat_map(|s| [s.from, s.to]).collect();
        (
            (pts.iter().map(|p| p.0).min().unwrap(), pts.iter().map(|p| p.1).min().unwrap()),
            (pts.iter().map(|p| p.0).max().unwrap(), pts.iter().map(|p| p.1).max().unwrap()),
        )
    }
}

fn um(mm: f64) -> i64 {
    (mm * 1000.0).round() as i64
}

// ---------------------------------------------------------------------------------------------
// building the corpus
// ---------------------------------------------------------------------------------------------

fn never() -> CancellationToken {
    CancellationToken::never()
}

fn open_path(s: &mut DocumentSession, pts: &[(f64, f64)], closed: bool) -> burnburn_types::GeometryId {
    let mut b = PathBuilder::new().move_to(Point::new(pts[0].0, pts[0].1));
    for p in &pts[1..] {
        b = b.line_to(Point::new(p.0, p.1));
    }
    if closed {
        b = b.close();
    }
    acc_cam::run(s, CommandPayload::Edit(GeometryCommand::CreatePath { path: b.build().unwrap(), name: "p".into() })).changes.created[0]
}

/// Keeps the drawn start and direction (no best-start / best-direction search).
fn keep_drawn(s: &mut DocumentSession) {
    let o = OptimizationSettings::default().with_best_start(false).with_best_direction(false);
    acc_cam::run(s, CommandPayload::Process(ProcessCommand::SetOptimization { settings: o }));
}

fn rd_of(plan: &JobPlan) -> Vec<u8> {
    common::rd_bytes(plan)
}

fn compile_with(s: &DocumentSession, profile: &MachineProfile, placement: Placement) -> JobPlan {
    let mut o = common::options();
    o.placement = placement;
    compile(&s.snapshot(), profile, &o, &never()).unwrap()
}

/// Laser-on segments of the plan, in machine micrometres.
fn plan_cuts(plan: &JobPlan) -> Vec<((i64, i64), (i64, i64))> {
    let mut at = plan.start();
    let mut out = Vec::new();
    for e in plan.events() {
        match *e {
            PlanEvent::Travel { to } => at = to,
            PlanEvent::Cut { to, .. } => {
                out.push(((um(at.x_mm()), um(at.y_mm())), (um(to.x_mm()), um(to.y_mm()))));
                at = to;
            }
            PlanEvent::ScanRun { line, .. } => {
                out.push(((um(line.from.x_mm()), um(line.from.y_mm())), (um(line.to.x_mm()), um(line.to.y_mm()))));
                at = line.to;
            }
            _ => {}
        }
    }
    out
}

/// Plan semantics and the independent reading of the file must describe the same laser-on motion.
fn assert_rd_matches_plan(plan: &JobPlan, tag: &str) -> Option<Read> {
    let rd = rd_of(plan);
    let recs = oracle(&rd, tag)?;
    // The independent splitter and BurnBurn's own decoder agree on the record structure.
    let ours = common::decode(&rd);
    assert_eq!(ours.records.len(), recs.len(), "{tag}: record count");
    for (a, b) in ours.records.iter().zip(&recs) {
        assert_eq!(a.offset, b.offset, "{tag}: record offset");
        assert_eq!(a.len, b.hex.len(), "{tag}: record length at {}", a.offset);
    }
    let got = read(&recs);
    let expected = plan_cuts(plan);
    let actual: Vec<_> = got.cuts().iter().map(|s| (s.from, s.to)).collect();
    assert_eq!(actual.len(), expected.len(), "{tag}: number of laser-on segments");
    for (i, (a, e)) in actual.iter().zip(&expected).enumerate() {
        assert!(
            (a.0 .0 - e.0 .0).abs() <= 1 && (a.0 .1 - e.0 .1).abs() <= 1 && (a.1 .0 - e.1 .0).abs() <= 1 && (a.1 .1 - e.1 .1).abs() <= 1,
            "{tag}: segment {i}: file {a:?} vs plan {e:?}"
        );
    }
    Some(got)
}

// ---------------------------------------------------------------------------------------------
// 1. directions
// ---------------------------------------------------------------------------------------------

#[test]
fn rd_diff_01_directions_x_y_diagonals_and_closed_contours_are_read_back_as_drawn() {
    let cases: Vec<(&str, Vec<(f64, f64)>, bool)> = vec![
        ("x-plus", vec![(100.0, 100.0), (150.0, 100.0)], false),
        ("x-minus", vec![(150.0, 100.0), (100.0, 100.0)], false),
        ("y-plus", vec![(100.0, 100.0), (100.0, 150.0)], false),
        ("y-minus", vec![(100.0, 150.0), (100.0, 100.0)], false),
        ("diag-ne", vec![(100.0, 100.0), (130.0, 140.0)], false),
        ("diag-sw", vec![(130.0, 140.0), (100.0, 100.0)], false),
        ("diag-nw", vec![(130.0, 100.0), (100.0, 140.0)], false),
        ("diag-se", vec![(100.0, 140.0), (130.0, 100.0)], false),
        ("square-ccw", vec![(100.0, 100.0), (120.0, 100.0), (120.0, 120.0), (100.0, 120.0)], true),
        ("square-cw", vec![(100.0, 100.0), (100.0, 120.0), (120.0, 120.0), (120.0, 100.0)], true),
    ];
    for (tag, pts, closed) in cases {
        let mut s = acc_cam::session();
        let g = open_path(&mut s, &pts, closed);
        acc_cam::add_op(&mut s, acc_cam::line_op("Snijden", 20.0, 50.0), vec![g]);
        keep_drawn(&mut s);
        let plan = compile_with(&s, &common::profile(), Placement::Absolute);
        let Some(got) = assert_rd_matches_plan(&plan, tag) else { return };
        // Analytic: the laser-on motion is the drawn vertices in order (closed contours return to the start).
        let mut want: Vec<(i64, i64)> = pts.iter().map(|p| (um(p.0), um(p.1))).collect();
        if closed {
            want.push(want[0]);
        }
        let cuts = got.cuts();
        let mut visited = vec![cuts[0].from];
        visited.extend(cuts.iter().map(|c| c.to));
        assert_eq!(visited, want, "{tag}: drawn direction is kept");
    }
}

// ---------------------------------------------------------------------------------------------
// 2. relative14 limits
// ---------------------------------------------------------------------------------------------

#[test]
fn rd_diff_02_relative14_boundaries_choose_the_opcode_by_the_signed_14_bit_range() {
    // Horizontal cut of dx micrometres starting well inside the bed. The signed 14-bit relative field
    // covers -8192..=8191 um (spec 02, section 2): AA (relative X cut) inside, A8 (absolute) outside.
    for dx in [8190_i64, 8191, 8192, -8191, -8192, -8193, 1, -1] {
        let start = 700.0;
        let end = start + dx as f64 / 1000.0;
        let mut s = acc_cam::session();
        let g = open_path(&mut s, &[(start, 300.0), (end, 300.0)], false);
        acc_cam::add_op(&mut s, acc_cam::line_op("Snijden", 20.0, 50.0), vec![g]);
        keep_drawn(&mut s);
        let plan = compile_with(&s, &common::profile(), Placement::Absolute);
        let tag = format!("rel14-{dx}");
        let Some(got) = assert_rd_matches_plan(&plan, &tag) else { return };
        let cuts = got.cuts();
        assert_eq!(cuts.len(), 1, "{tag}");
        assert_eq!((cuts[0].to.0 - cuts[0].from.0), dx, "{tag}: the displacement is exact to the micrometre");
        let relative = (-8192..=8191).contains(&dx);
        let expected_opcode = if relative { 0xaa } else { 0xa8 };
        assert_eq!(cuts[0].opcode, expected_opcode, "{tag}: opcode {:#x}", cuts[0].opcode);
    }
}

// ---------------------------------------------------------------------------------------------
// 3. origins and anchors
// ---------------------------------------------------------------------------------------------

fn profile_with(origin: OriginCorner) -> MachineProfile {
    MachineProfile::new(
        "research-1500x900",
        1,
        MachineFrame::new(1500.0, 900.0, origin).unwrap(),
        1000.0,
        400.0,
        PowerRatio::new(0.0).unwrap(),
        PowerRatio::new(1.0).unwrap(),
    )
    .unwrap()
}

#[test]
fn rd_diff_03_each_bed_corner_origin_maps_the_same_design_to_its_own_machine_coordinates() {
    // Design (bed view, y up): rectangle (100, 50)-(140, 80). Machine coordinates count from the
    // origin corner of the profile.
    for (origin, expect) in [
        (OriginCorner::BottomLeft, ((100, 50), (140, 80))),
        (OriginCorner::TopLeft, ((100, 900 - 80), (140, 900 - 50))),
        (OriginCorner::TopRight, ((1500 - 140, 900 - 80), (1500 - 100, 900 - 50))),
        (OriginCorner::BottomRight, ((1500 - 140, 50), (1500 - 100, 80))),
    ] {
        let mut s = acc_cam::session();
        let g = acc_cam::rectangle(&mut s, 100.0, 50.0, 40.0, 30.0);
        acc_cam::add_op(&mut s, acc_cam::line_op("Snijden", 20.0, 50.0), vec![g]);
        let plan = compile_with(&s, &profile_with(origin), Placement::Absolute);
        let tag = format!("origin-{origin:?}");
        let Some(got) = assert_rd_matches_plan(&plan, &tag) else { return };
        let (lo, hi) = got.cut_extent();
        let want = ((um(f64::from(expect.0 .0)), um(f64::from(expect.0 .1))), (um(f64::from(expect.1 .0)), um(f64::from(expect.1 .1))));
        assert_eq!((lo, hi), want, "{tag}");
        // And the header's document bounds (E7 50 / E7 51) agree with the motion (they may be a few um wider).
        let (dmin, dmax) = (got.doc_min.unwrap(), got.doc_max.unwrap());
        assert!(dmin.0 <= lo.0 && dmin.1 <= lo.1 && dmax.0 >= hi.0 && dmax.1 >= hi.1 && dmin.0 >= lo.0 - 5 && dmax.1 <= hi.1 + 5, "{tag}: header {dmin:?} {dmax:?}");
    }
}

#[test]
fn rd_diff_03_all_nine_job_anchors_land_on_the_chosen_point() {
    // Rectangle (100, 100)-(140, 120) moved so that its anchor lands on (500, 400).
    let (w, h) = (40.0, 20.0);
    for (anchor, fx, fy) in [
        (JobAnchor::BottomLeft, 0.0, 0.0),
        (JobAnchor::Bottom, 0.5, 0.0),
        (JobAnchor::BottomRight, 1.0, 0.0),
        (JobAnchor::Left, 0.0, 0.5),
        (JobAnchor::Center, 0.5, 0.5),
        (JobAnchor::Right, 1.0, 0.5),
        (JobAnchor::TopLeft, 0.0, 1.0),
        (JobAnchor::Top, 0.5, 1.0),
        (JobAnchor::TopRight, 1.0, 1.0),
    ] {
        let mut s = acc_cam::session();
        let g = acc_cam::rectangle(&mut s, 100.0, 100.0, w, h);
        acc_cam::add_op(&mut s, acc_cam::line_op("Snijden", 20.0, 50.0), vec![g]);
        let plan = compile_with(&s, &common::profile(), Placement::Anchored { anchor, at: Point::new(500.0, 400.0) });
        let tag = format!("anchor-{anchor:?}");
        let Some(got) = assert_rd_matches_plan(&plan, &tag) else { return };
        let (lo, hi) = got.cut_extent();
        let (min_x, min_y) = (500.0 - fx * w, 400.0 - fy * h);
        assert_eq!((lo, hi), ((um(min_x), um(min_y)), (um(min_x + w), um(min_y + h))), "{tag}");
    }
}

// ---------------------------------------------------------------------------------------------
// 4. power, 5. speed
// ---------------------------------------------------------------------------------------------

fn settings_pct(speed: f64, min: f64, max: f64) -> ProcessSettings {
    ProcessSettings {
        speed: Speed::mm_per_s(speed).unwrap(),
        power: PowerRange::new(PowerPercent::new(min).unwrap(), PowerPercent::new(max).unwrap()).unwrap(),
        passes: Passes::new(1).unwrap(),
        air_assist: true,
    }
}

fn rect_job(settings: ProcessSettings) -> JobPlan {
    let mut s = acc_cam::session();
    let g = acc_cam::rectangle(&mut s, 100.0, 100.0, 20.0, 10.0);
    let draft = OperationDraft {
        name: "Snijden".into(),
        mode: OperationMode::Line { kerf: Kerf::NONE },
        settings,
        output_enabled: true,
    };
    acc_cam::add_op(&mut s, draft, vec![g]);
    compile_with(&s, &common::profile(), Placement::Absolute)
}

#[test]
fn rd_diff_04_power_values_reach_the_file_as_the_scaled_14_bit_value_and_min_max_stay_apart() {
    // Evidence: LightBurn writes 20 % as 3277 (fixture rectangle-default). The other raw values below
    // are the same scaling, round(percent x 16383 / 100), which is a hypothesis for every percentage
    // except 20 % until a bench run compares them.
    for pct in [0.0, 0.1, 1.0, 10.0, 20.0, 50.0, 99.9, 100.0] {
        let plan = rect_job(settings_pct(20.0, pct, pct));
        let tag = format!("power-{pct}");
        let Some(got) = assert_rd_matches_plan(&plan, &tag) else { return };
        let raw = (pct / 100.0 * 16383.0).round() as u16;
        assert!(got.powers.contains(&(1, raw)) && got.powers.contains(&(2, raw)), "{tag}: {:?} vs raw {raw}", got.powers);
        if pct == 20.0 {
            assert_eq!(raw, 3277, "the hardware-evidenced pair");
        }
    }
    // min != max: the two records differ and the maximum is not below the minimum.
    let plan = rect_job(settings_pct(20.0, 10.0, 60.0));
    let Some(got) = assert_rd_matches_plan(&plan, "power-min-max") else { return };
    let min_raw = got.powers.iter().find(|p| p.0 == 1).unwrap().1;
    let max_raw = got.powers.iter().find(|p| p.0 == 2).unwrap().1;
    assert!(min_raw < max_raw, "min {min_raw} max {max_raw}");
    assert_eq!((min_raw, max_raw), (1638, 9830));
}

#[test]
fn rd_diff_05_speed_reaches_the_file_in_micrometres_per_second() {
    for mm_s in [0.1, 1.0, 100.0, 1000.0] {
        let plan = rect_job(settings_pct(mm_s, 20.0, 20.0));
        let tag = format!("speed-{mm_s}");
        let Some(got) = assert_rd_matches_plan(&plan, &tag) else { return };
        assert!(got.speeds.contains(&((mm_s * 1000.0_f64).round() as i64)), "{tag}: {:?}", got.speeds);
    }
    // The unit is mm/s only; a per-minute figure must be converted by the caller (/60) and then lands on
    // the same record: 6000 mm/min == 100 mm/s.
    let a = rect_job(settings_pct(6000.0 / 60.0, 20.0, 20.0));
    let b = rect_job(settings_pct(100.0, 20.0, 20.0));
    assert_eq!(rd_of(&a), rd_of(&b));
}

// ---------------------------------------------------------------------------------------------
// 6. fill angles and directions
// ---------------------------------------------------------------------------------------------

fn fill_job(axis: ScanAxis, direction: ScanDirection, angle: Option<f64>, interval: f64) -> JobPlan {
    let mut s = acc_cam::session();
    let g = acc_cam::rectangle(&mut s, 100.0, 100.0, 20.0, 10.0);
    let mut f = FillSettings::new(acc_cam::mm(interval), axis, direction, acc_cam::mm(0.0)).unwrap();
    let mut profile = common::profile();
    if let Some(a) = angle {
        f = f.with_angle(a).unwrap();
        profile = acc_cam::supporting(&[burnburn_job::Feature::ScanAngle]);
    }
    let draft = OperationDraft {
        name: "Vullen".into(),
        mode: OperationMode::Fill(f),
        settings: acc_cam::settings(100.0, 20.0),
        output_enabled: true,
    };
    acc_cam::add_op(&mut s, draft, vec![g]);
    compile_with(&s, &profile, Placement::Absolute)
}

fn lowered(plan: &JobPlan) -> Result<Vec<u8>, burnburn_ruida::LowerError> {
    let program = burnburn_ruida::lower(plan, &common::RUIDA)?;
    Ok(program.to_rd_file(common::RUIDA.magic).unwrap())
}

#[test]
fn rd_diff_06_fill_rows_sit_at_half_interval_centres_and_alternate_only_when_bidirectional() {
    let interval = 1.0;
    for dir in [ScanDirection::Bidirectional, ScanDirection::Unidirectional] {
        let plan = fill_job(ScanAxis::X, dir, None, interval);
        let tag = format!("fill-X-{dir:?}");
        let Some(got) = assert_rd_matches_plan(&plan, &tag) else { return };
        let cuts = got.cuts();
        // 20 x 10 mm at 1 mm: 10 rows along X, each 20 mm, centred in their interval from y = 100 mm.
        assert_eq!(cuts.len(), 10, "{tag}");
        for (i, c) in cuts.iter().enumerate() {
            assert_eq!((c.to.0 - c.from.0).abs(), 20_000, "{tag}: row {i} length");
            assert_eq!(c.from.1, 100_000 + 500 + 1000 * i as i64, "{tag}: row {i} is centred in its interval");
        }
        let signs: Vec<i64> = cuts.iter().map(|c| (c.to.0 - c.from.0).signum()).collect();
        match dir {
            ScanDirection::Bidirectional => assert!(signs.windows(2).all(|w| w[0] == -w[1]), "{tag}: alternates {signs:?}"),
            ScanDirection::Unidirectional => assert!(signs.windows(2).all(|w| w[0] == w[1]), "{tag}: same way {signs:?}"),
        }
    }
}

/// GAP (see docs/user-flows/acceptatie.md, plan section 6 and 13): the RD lowering only carries scans
/// along X. A vertical (Y) scan or an angled scan compiles to a plan, but exporting it is refused with
/// a typed `Unsupported`; nothing wrong is written. This test pins that refusal.
#[test]
fn rd_diff_06_vertical_and_angled_fills_are_refused_at_export_not_written_wrong() {
    let cases = [
        ("axis-y", fill_job(ScanAxis::Y, ScanDirection::Bidirectional, None, 1.0)),
        ("angle-90", fill_job(ScanAxis::X, ScanDirection::Bidirectional, Some(90.0), 1.0)),
        ("angle-45", fill_job(ScanAxis::X, ScanDirection::Bidirectional, Some(45.0), 1.0)),
    ];
    for (tag, plan) in cases {
        let err = lowered(&plan).expect_err(tag);
        assert!(matches!(err, burnburn_ruida::LowerError::Unsupported { .. }), "{tag}: {err:?}");
    }
    // 180 degrees is 0 (angles are taken modulo 180 in the document), so that one exports and is identical.
    let plain = fill_job(ScanAxis::X, ScanDirection::Bidirectional, Some(0.0), 1.0);
    let half_turn = fill_job(ScanAxis::X, ScanDirection::Bidirectional, Some(180.0), 1.0);
    assert_eq!(lowered(&plain).unwrap(), lowered(&half_turn).unwrap());
}

// ---------------------------------------------------------------------------------------------
// 9. job: passes, array of copies and a very large file with its checksum
// ---------------------------------------------------------------------------------------------

#[test]
fn rd_diff_12_the_file_checksum_is_the_byte_sum_of_everything_before_it_also_for_a_very_large_file() {
    // 6000 short zig-zag cuts: a file of several hundred kilobytes; the sum is far beyond 16 bits.
    let mut s = acc_cam::session();
    let pts: Vec<(f64, f64)> = (0..6000).map(|i| (100.0 + f64::from(i % 2) * 5.0, 100.0 + f64::from(i) * 0.1)).collect();
    let g = open_path(&mut s, &pts, false);
    acc_cam::add_op(&mut s, acc_cam::line_op("Snijden", 100.0, 50.0), vec![g]);
    keep_drawn(&mut s);
    let plan = compile_with(&s, &common::profile(), Placement::Absolute);
    let rd = rd_of(&plan);
    assert!(rd.len() > 50_000, "a large file: {} bytes", rd.len());
    let Some(recs) = oracle(&rd, "large") else { return };
    // Last two records: E5 05 <sum5> and D7. The sum covers the unscrambled bytes before E5 05, plus 0xD7.
    let n = recs.len();
    let sum_rec = &recs[n - 2];
    assert_eq!(&sum_rec.hex[..2], &[0xe5, 0x05]);
    let declared = unpack7(&sum_rec.hex[2..7]);
    let raw_before: u64 = recs[..n - 2].iter().flat_map(|r| r.hex.iter()).map(|b| u64::from(*b)).sum();
    assert_eq!(declared as u64, raw_before + 0xd7, "file sum field");
    assert!(declared > 65_535, "beyond 16 bits: {declared}");
    let got = read(&recs);
    assert_eq!(got.cuts().len(), 5999);
}

// ---------------------------------------------------------------------------------------------
// 13. output route: Save RD and Send carry the same payload (envelope differs, payload does not)
// ---------------------------------------------------------------------------------------------

#[test]
fn rd_diff_13_save_and_send_differ_in_envelope_but_not_in_payload() {
    use burnburn_machine::RdArtifact;
    use burnburn_ruida::udp::{frame, parse_frame};
    let plan = rect_job(settings_pct(100.0, 20.0, 20.0));
    let file = rd_of(&plan);
    // "Send": the artifact is cut into chunks; every chunk is framed with a 2-byte checksum and the
    // concatenation of the chunk payloads is the saved file, byte for byte.
    let artifact = RdArtifact::from_rd_file(plan.id(), &file, 128).unwrap();
    let mut rebuilt = Vec::new();
    for chunk in artifact.chunks() {
        let datagram = frame(chunk).unwrap();
        assert_eq!(datagram.len(), chunk.len() + 2);
        rebuilt.extend_from_slice(parse_frame(&datagram).unwrap());
    }
    assert_eq!(rebuilt, file, "the uploaded bytes are the saved bytes");
    assert_eq!(artifact.rd_hash(), burnburn_types::ContentHash::of(burnburn_types::HashDomain::Rd, &file));
}

// ---------------------------------------------------------------------------------------------
// 8. images, 11. axes, 12. passes
// ---------------------------------------------------------------------------------------------

#[test]
fn rd_diff_08_a_dithered_image_is_read_back_from_the_file_as_the_same_burn_pattern() {
    use burnburn_document::process::{DitherMode, ImageRender, ImageSettings};
    use burnburn_image::SourceImage;
    // 8 x 4 gradient, Atkinson: the pattern is the independent reference of burnburn-cam's image tests.
    let px: Vec<u8> = (0..4).flat_map(|y| (0..8).map(move |x| 30 + 27 * x + 10 * y)).collect();
    let source = SourceImage::from_gray8(8, 4, px).unwrap();
    let settings = ImageSettings::new(
        source.content_hash(),
        acc_cam::mm(0.1),
        acc_cam::mm(0.1),
        ImageRender::Dither(DitherMode::Atkinson),
    )
    .unwrap();
    let mut s = acc_cam::session();
    let frame = acc_cam::rectangle(&mut s, 10.0, 10.0, 0.8, 0.4);
    let draft = OperationDraft {
        name: "Graveren".into(),
        mode: OperationMode::Image(settings),
        settings: acc_cam::settings(100.0, 60.0),
        output_enabled: true,
    };
    acc_cam::add_op(&mut s, draft, vec![frame]);
    let plan = burnburn_cam::compile_with(&s.snapshot(), &common::profile(), &common::options(), &vec![source], &never()).unwrap();
    let Some(recs) = oracle(&rd_of(&plan), "image") else { return };
    let got = read(&recs);
    // Rebuild the pattern from the laser-on segments of the FILE (rows top to bottom, 0.1 mm dots).
    let mut grid = vec![vec!['.'; 8]; 4];
    for c in got.cuts() {
        let row = 3 - (((c.from.1 - 10_000) as f64 / 100.0) - 0.5).round() as usize;
        let (a, b) = (c.from.0.min(c.to.0), c.from.0.max(c.to.0));
        for col in ((a - 10_000) / 100)..((b - 10_000) / 100) {
            grid[row][col as usize] = '#';
        }
    }
    let rows: Vec<String> = grid.into_iter().map(|r| r.into_iter().collect()).collect();
    assert_eq!(rows, ["###.....", "####....", "##..##..", "##......"]);
}

#[test]
fn rd_diff_11_a_focus_offset_is_refused_at_export_and_two_passes_repeat_the_motion() {
    use burnburn_document::process::{AdvancedOptions, ZOffset};
    // Z/U axes: the plan can carry a focus offset (with the capability), the RD lowering has no
    // verified record for it and refuses with a typed error instead of dropping it silently.
    let mut s = acc_cam::session();
    let g = acc_cam::rectangle(&mut s, 100.0, 100.0, 20.0, 10.0);
    let op = acc_cam::add_op(&mut s, acc_cam::line_op("Snijden", 20.0, 50.0), vec![g]);
    let advanced = AdvancedOptions { z_offset: Some(ZOffset::new(acc_cam::mm(1.5)).unwrap()), ..Default::default() };
    acc_cam::run(&mut s, CommandPayload::Process(ProcessCommand::SetAdvanced { operation: op, advanced }));
    let z_profile = acc_cam::supporting(&[burnburn_job::Feature::ZStep]);
    let plan = compile_with(&s, &z_profile, Placement::Absolute);
    assert!(plan.events().iter().any(|e| matches!(e, PlanEvent::FocusOffset { .. })));
    assert!(matches!(lowered(&plan), Err(burnburn_ruida::LowerError::Unsupported { what: "focus offset", .. })));

    // Passes: two passes of one contour are the same four cuts twice in the file.
    let mut s = acc_cam::session();
    let g = acc_cam::rectangle(&mut s, 100.0, 100.0, 20.0, 10.0);
    let draft = OperationDraft {
        name: "Snijden".into(),
        mode: OperationMode::Line { kerf: Kerf::NONE },
        settings: ProcessSettings { passes: Passes::new(2).unwrap(), ..acc_cam::settings(20.0, 50.0) },
        output_enabled: true,
    };
    acc_cam::add_op(&mut s, draft, vec![g]);
    let plan = compile_with(&s, &common::profile(), Placement::Absolute);
    let Some(got) = assert_rd_matches_plan(&plan, "two-passes") else { return };
    let cuts = got.cuts();
    assert_eq!(cuts.len(), 8, "2 passes x 4 sides");
    assert_eq!(cuts[..4].iter().map(|c| (c.from, c.to)).collect::<Vec<_>>(), cuts[4..].iter().map(|c| (c.from, c.to)).collect::<Vec<_>>());
}
