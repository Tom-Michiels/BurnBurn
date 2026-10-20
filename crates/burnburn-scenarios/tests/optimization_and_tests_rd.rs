//! W2-L3: optimization settings and generated tests reach the Ruida file as the preview shows
//! them. Expectations are derived from the drawings (square positions, priorities, grid values),
//! and read back through the RD decoder, not from the plan.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use burnburn_cam::test_job::{
    IntervalTestSpec, MaterialTestSpec, TestAxis, TestCellMode, TestParameter, generate_interval_test,
    generate_material_test,
};
use burnburn_document::process::{
    CutPriority, OptimizationSettings, OrderKey, Passes, PowerPercent, PowerRange, ProcessCommand, ProcessSettings,
    Speed,
};
use burnburn_document::{CommandPayload, DocumentSession};
use burnburn_geometry::{Point, Rect};
use burnburn_job::preview;
use burnburn_ruida::Command;
use burnburn_types::{CancellationToken, GeometryId};
use common::*;

fn square(s: &mut DocumentSession, x: f64) -> GeometryId {
    let rect = Rect::new(Point::new(x, 10.0), mm(10.0), mm(10.0)).unwrap();
    apply(s, CommandPayload::CreateRectangle { rect, name: String::new() }, &[]).changes.created[0]
}

fn first_cut_x(plan: &burnburn_job::JobPlan) -> f64 {
    cuts(&decode(&rd_bytes(plan)))[0].from.x
}

#[test]
fn the_order_by_priority_reaches_the_rd_file() {
    let mut s = new_session();
    let near = square(&mut s, 10.0);
    let far = square(&mut s, 100.0);
    add_cut(&mut s, vec![near, far]);
    // Nearest neighbour from the origin: the square at x = 10 first.
    assert!((first_cut_x(&compile_plan(&s.snapshot())) - 10.0).abs() < 1e-6);
    let priority = CutPriority::new(-1).unwrap();
    apply(&mut s, CommandPayload::Process(ProcessCommand::SetPriority { geometry: far, priority }), &[]);
    let by_priority = OptimizationSettings::default().with_order(vec![OrderKey::Layer, OrderKey::Priority]).unwrap();
    apply(&mut s, CommandPayload::Process(ProcessCommand::SetOptimization { settings: by_priority }), &[]);
    // Priority −1 is cut first, 90 mm further away.
    assert!((first_cut_x(&compile_plan(&s.snapshot())) - 100.0).abs() < 1e-6);
}

#[test]
fn removed_overlapping_lines_are_missing_from_the_rd_as_well() {
    // Two squares 0.005 mm apart share an edge within the 0.01 mm tolerance: 80 mm − 10 mm of
    // the shared edge − 2 · 0.005 mm of the corner stadiums = 69.99 mm, in the preview and in the RD.
    let mut s = new_session();
    let a = square(&mut s, 10.0);
    let b = square(&mut s, 20.005);
    add_cut(&mut s, vec![a, b]);
    let tolerance = OptimizationSettings::default().with_remove_overlapping(Some(mm(0.01))).unwrap();
    apply(&mut s, CommandPayload::Process(ProcessCommand::SetOptimization { settings: tolerance }), &[]);
    let plan = compile_plan(&s.snapshot());
    let pv = preview(&plan, &profile());
    let rd = cuts(&decode(&rd_bytes(&plan))).iter().map(|c| c.from.distance(c.to)).sum::<f64>();
    assert!((pv.cut_length_mm - 69.99).abs() < 1e-6, "{}", pv.cut_length_mm);
    assert!((rd - 69.99).abs() < 0.01, "{rd}");
    // The source geometry still holds both squares.
    assert_eq!(s.snapshot().objects().count(), 2);
}

fn base() -> ProcessSettings {
    let p = PowerPercent::new(20.0).unwrap();
    ProcessSettings {
        speed: Speed::mm_per_s(50.0).unwrap(),
        power: PowerRange::new(p, p).unwrap(),
        passes: Passes::new(1).unwrap(),
        air_assist: false,
    }
}

#[test]
fn a_material_test_lowers_to_one_rd_part_per_cell_with_its_own_speed_and_power() {
    let spec = MaterialTestSpec {
        mode: TestCellMode::Line,
        columns: TestAxis { parameter: TestParameter::Speed, min: 100.0, max: 300.0, count: 3 },
        rows: Some(TestAxis { parameter: TestParameter::Power, min: 10.0, max: 40.0, count: 2 }),
        base: base(),
        cell_width: mm(20.0),
        cell_height: mm(10.0),
        gap: mm(2.0),
        labels: None,
        border: None,
    };
    let job = generate_material_test(&spec).unwrap();
    let plan = job.compile(&profile(), &options(), &CancellationToken::never()).unwrap();
    let file = decode(&rd_bytes(&plan));
    let speeds: Vec<u64> = file
        .commands()
        .filter_map(|c| match c {
            Command::PartSpeed { speed, .. } => Some(speed.um_per_s()),
            _ => None,
        })
        .collect();
    assert_eq!(speeds, vec![100_000, 200_000, 300_000, 100_000, 200_000, 300_000]);
    // Power raw = round(ratio × 16383): 10 % → 1638, 40 % → 6553 (the maximum of each cell).
    let max_powers: Vec<u16> = file
        .commands()
        .filter_map(|c| match c {
            Command::PartPower { bound: burnburn_ruida::PowerBound::Max, value, .. } => Some(value.raw()),
            _ => None,
        })
        .collect();
    assert_eq!(max_powers, vec![1638, 1638, 1638, 6553, 6553, 6553]);
    // Cell outlines: 6 cells × 4 edges.
    assert_eq!(cuts(&file).len(), 24);
}

#[test]
fn an_interval_test_lowers_to_scan_parts_with_the_row_count_of_each_interval() {
    let spec = IntervalTestSpec {
        min_interval: mm(0.5),
        max_interval: mm(2.0),
        count: 4,
        overscan: mm(0.0),
        base: base(),
        cell_width: mm(10.0),
        cell_height: mm(10.0),
        gap: mm(2.0),
        labels: None,
        border: None,
    };
    let job = generate_interval_test(&spec).unwrap();
    let plan = job.compile(&profile(), &options(), &CancellationToken::never()).unwrap();
    let file = decode(&rd_bytes(&plan));
    let scan_parts =
        file.commands().filter(|c| matches!(c, Command::WorkModePart { mode, .. } if mode.get() == 2)).count();
    assert_eq!(scan_parts, 4);
    // Scan rows per cell: 10 mm / interval = 20, 10 (0.5 → 20; 1.0 → 10; 1.5 → 6; 2.0 → 5), one
    // laser-on run per row of a rectangle: the sum of the decoded runs.
    let runs = cuts(&file).len();
    assert_eq!(runs, 20 + 10 + 6 + 5);
}
