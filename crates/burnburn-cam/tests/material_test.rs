//! W2-L3: Material Test, Interval Test and Focus Test. The generators build a private document
//! and compile it with the ordinary compiler; the oracles are the analytic grid layout and the
//! plan's own bounds and settings blocks.
#![allow(clippy::unwrap_used)]

mod common;

use burnburn_cam::CompileError;
use burnburn_cam::test_job::{
    FocusTestSpec, IntervalTestSpec, MaterialTestSpec, TestAxis, TestBorder, TestCellMode, TestError, TestJob,
    TestLabels, TestParameter, generate_focus_test, generate_interval_test, generate_material_test,
};
use burnburn_document::process::{OperationMode, ProcessSettings};
use burnburn_job::{Feature, JobPlan, PlanEvent, PlanMode, Support};
use burnburn_types::CancellationToken;
use common::*;

fn axis(parameter: TestParameter, min: f64, max: f64, count: u32) -> TestAxis {
    TestAxis { parameter, min, max, count }
}

fn grid() -> MaterialTestSpec {
    MaterialTestSpec {
        mode: TestCellMode::Line,
        columns: axis(TestParameter::Speed, 100.0, 300.0, 3),
        rows: Some(axis(TestParameter::Power, 10.0, 40.0, 2)),
        base: settings(50.0, 20.0),
        cell_width: mm(20.0),
        cell_height: mm(10.0),
        gap: mm(2.0),
        labels: None,
        border: None,
    }
}

fn compiled(job: &TestJob) -> JobPlan {
    job.compile(&profile(), &options(), &CancellationToken::never()).unwrap()
}

fn operation_modes(plan: &JobPlan) -> Vec<(f64, f64, f64)> {
    plan.events()
        .iter()
        .filter_map(|e| match e {
            PlanEvent::OperationBoundary { settings, .. } => {
                let s = plan.settings()[settings.get()];
                Some((s.speed_mm_s, s.min_power.get(), s.max_power.get()))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn a_material_test_is_a_grid_of_cells_with_one_value_per_column_and_row() {
    let job = generate_material_test(&grid()).unwrap();
    assert_eq!(job.cells().len(), 6);
    // Row-major, first row at the top: speed varies along the columns, power along the rows.
    let values: Vec<(f64, Option<f64>)> = job.cells().iter().map(|c| (c.column_value, c.row_value)).collect();
    assert_eq!(
        values,
        vec![
            (100.0, Some(10.0)),
            (200.0, Some(10.0)),
            (300.0, Some(10.0)),
            (100.0, Some(40.0)),
            (200.0, Some(40.0)),
            (300.0, Some(40.0)),
        ]
    );
    for c in job.cells() {
        let op = job.snapshot().process().operation(c.operation).unwrap();
        let s = op.settings();
        assert_eq!(s.speed.get_mm_per_s(), c.column_value);
        assert_eq!(s.power.max().get(), c.row_value.unwrap());
        assert_eq!(op.targets(), &[c.geometry]);
    }
    // Layout oracle: 3 × 20 + 2 × 2 wide, 2 × 10 + 2 high, cut outlines only.
    let plan = compiled(&job);
    let b = plan.bounds().unwrap();
    assert!((b.max().x - b.min().x - 64.0).abs() < 1e-9, "{b:?}");
    assert!((b.max().y - b.min().y - 22.0).abs() < 1e-9, "{b:?}");
    // The settings blocks follow the cells; the minimum power never exceeds the maximum.
    let speeds: Vec<f64> = operation_modes(&plan).iter().map(|m| m.0).collect();
    assert_eq!(speeds, vec![100.0, 200.0, 300.0, 100.0, 200.0, 300.0]);
    assert!(operation_modes(&plan).iter().all(|m| m.1 <= m.2));
}

#[test]
fn the_first_row_is_the_top_row_and_cells_are_one_pitch_apart() {
    let job = generate_material_test(&grid()).unwrap();
    let origin = |i: usize| job.snapshot().object(job.cells()[i].geometry).unwrap().bounds().unwrap().min();
    assert!((origin(1).x - origin(0).x - 22.0).abs() < 1e-9, "column pitch = width + gap");
    assert!((origin(0).y - origin(3).y - 12.0).abs() < 1e-9, "row 0 lies one pitch above row 1");
    assert!((origin(0).x - origin(3).x).abs() < 1e-9);
}

#[test]
fn a_test_is_separate_from_the_running_document() {
    let mut s = session();
    let g = rectangle(&mut s, 0.0, 0.0, 10.0, 10.0);
    add_op(&mut s, line_op("Snijden", 10.0, 50.0), vec![g]);
    let before = burnburn_cam::source_hash(&s.snapshot()).unwrap();
    let job = generate_material_test(&grid()).unwrap();
    let plan = compiled(&job);
    assert_eq!(burnburn_cam::source_hash(&s.snapshot()).unwrap(), before, "the document is untouched");
    assert_ne!(plan.source().source_hash, before);
    assert_eq!(job.snapshot().objects().count(), 6, "only the cells live in the private document");
}

#[test]
fn passes_and_the_fill_interval_are_swept_like_any_other_parameter() {
    let mut spec = grid();
    spec.mode = TestCellMode::Fill { interval: mm(0.1), overscan: mm(0.0) };
    spec.columns = axis(TestParameter::Interval, 0.05, 0.2, 4);
    spec.rows = Some(axis(TestParameter::Passes, 1.0, 3.0, 3));
    let job = generate_material_test(&spec).unwrap();
    let intervals: Vec<f64> = job
        .cells()
        .iter()
        .map(|c| match job.snapshot().process().operation(c.operation).unwrap().mode() {
            OperationMode::Fill(f) => f.interval().mm(),
            other => panic!("{other:?}"),
        })
        .collect();
    for (got, want) in intervals.iter().zip([0.05, 0.1, 0.15, 0.2].iter().cycle()) {
        assert!((got - want).abs() < 1e-12, "{got} {want}");
    }
    let passes: Vec<u32> = job
        .cells()
        .iter()
        .map(|c| job.snapshot().process().operation(c.operation).unwrap().settings().passes.get())
        .collect();
    assert_eq!(passes, [1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3]);
}

#[test]
fn an_interval_test_scans_each_cell_at_its_own_interval() {
    let spec = IntervalTestSpec {
        min_interval: mm(0.05),
        max_interval: mm(0.2),
        count: 4,
        overscan: mm(0.0),
        base: settings(100.0, 20.0),
        cell_width: mm(10.0),
        cell_height: mm(10.0),
        gap: mm(2.0),
        labels: None,
        border: None,
    };
    let job = generate_interval_test(&spec).unwrap();
    assert_eq!(job.cells().len(), 4);
    let plan = compiled(&job);
    // A 10 mm tall cell has one scan row per interval (row centres): 200, 100, 66/67, 50.
    let runs = |settings_of_cell: usize| {
        let mut n = 0usize;
        let mut current: Option<usize> = None;
        let mut boundary = 0usize;
        for e in plan.events() {
            match e {
                PlanEvent::OperationBoundary { .. } => {
                    current = Some(boundary);
                    boundary += 1;
                }
                PlanEvent::ScanRun { .. } if current == Some(settings_of_cell) => n += 1,
                _ => {}
            }
        }
        n
    };
    for (cell, interval) in [0.05, 0.1, 0.15, 0.2].iter().enumerate() {
        let expected = (10.0f64 / interval).floor() as usize;
        assert_eq!(runs(cell), expected, "interval {interval}");
        match plan.settings()[cell].mode {
            PlanMode::Scan { interval_mm, .. } => assert!((interval_mm - interval).abs() < 1e-12),
            other => panic!("{other:?}"),
        }
    }
}

#[test]
fn labels_and_a_border_enlarge_the_job_around_the_cells() {
    let mut spec = grid();
    let label_settings = settings(30.0, 15.0);
    spec.labels = Some(TestLabels { height: mm(3.0), settings: label_settings });
    spec.border = Some(TestBorder { margin: mm(2.0), settings: label_settings });
    let job = generate_material_test(&spec).unwrap();
    // 3 column labels + 2 row labels + border: six cells' operations come first.
    let ops: Vec<_> = job.snapshot().process().operations().collect();
    assert_eq!(ops.len(), 6 + 1 + 1, "cells, one label operation, one border operation");
    assert_eq!(job.snapshot().objects().count(), 6 + 5 + 1);
    let plan = compiled(&job);
    let b = plan.bounds().unwrap();
    let cells = generate_material_test(&grid()).unwrap();
    let inner = compiled(&cells).bounds().unwrap();
    // The border lies 2 mm outside everything; the content is larger than the bare grid.
    assert!(b.max().x - b.min().x > inner.max().x - inner.min().x + 4.0);
    assert!(b.max().y - b.min().y >= inner.max().y - inner.min().y + 3.0 + 4.0);
    assert!(b.min().x >= -1e-9 && b.min().y >= -1e-9, "everything lies in the positive quadrant");
}

#[test]
fn invalid_specs_are_typed_errors() {
    let fails = |f: &dyn Fn(&mut MaterialTestSpec)| {
        let mut s = grid();
        f(&mut s);
        generate_material_test(&s).unwrap_err()
    };
    assert!(matches!(fails(&|s| s.columns.count = 0), TestError::Count { .. }));
    assert!(matches!(fails(&|s| s.columns.count = 26), TestError::Count { .. }));
    assert!(matches!(fails(&|s| s.columns.min = 400.0), TestError::Range { .. }));
    assert!(matches!(fails(&|s| s.rows = Some(axis(TestParameter::Power, 10.0, 120.0, 2))), TestError::Range { .. }));
    assert!(matches!(fails(&|s| s.rows = Some(axis(TestParameter::Passes, 1.0, 2.0, 3))), TestError::NotWhole { .. }));
    assert_eq!(
        fails(&|s| s.rows = Some(axis(TestParameter::Interval, 0.1, 0.2, 2))),
        TestError::ParameterNotForMode { parameter: TestParameter::Interval }
    );
    assert_eq!(
        fails(&|s| s.rows = Some(axis(TestParameter::Speed, 10.0, 20.0, 2))),
        TestError::SameParameter(TestParameter::Speed)
    );
    assert!(matches!(fails(&|s| s.cell_width = mm(0.0)), TestError::Size { .. }));
    assert!(matches!(
        fails(&|s| {
            s.columns = axis(TestParameter::Speed, 10.0, 20.0, 25);
            s.rows = Some(axis(TestParameter::Power, 10.0, 20.0, 5));
        }),
        TestError::TooManyCells { cells: 125 }
    ));
}

#[test]
fn a_single_value_axis_uses_the_minimum() {
    let mut spec = grid();
    spec.columns = axis(TestParameter::Speed, 120.0, 500.0, 1);
    spec.rows = None;
    let job = generate_material_test(&spec).unwrap();
    assert_eq!(job.cells().len(), 1);
    assert_eq!(job.cells()[0].column_value, 120.0);
    assert_eq!(job.cells()[0].row_value, None);
}

fn focus() -> FocusTestSpec {
    FocusTestSpec {
        z_start: -1.0,
        z_step: 0.5,
        count: 5,
        line_length: mm(20.0),
        spacing: mm(3.0),
        base: settings(30.0, 40.0),
        labels: None,
    }
}

#[test]
fn a_focus_test_needs_the_z_capability() {
    let err = generate_focus_test(&focus(), &profile()).unwrap_err();
    assert_eq!(err, TestError::Unsupported { feature: Feature::ZStep, support: Support::Unsupported });
}

#[test]
fn a_focus_test_cuts_one_line_per_focus_offset() {
    let z = supporting(&[Feature::ZStep]);
    let job = generate_focus_test(&focus(), &z).unwrap();
    assert_eq!(job.cells().len(), 5);
    let plan = job.compile(&z, &options(), &CancellationToken::never()).unwrap();
    let offsets: Vec<f64> = plan
        .events()
        .iter()
        .filter_map(|e| match e {
            PlanEvent::FocusOffset { z_mm } => Some(*z_mm),
            _ => None,
        })
        .collect();
    // The line at offset 0 needs no explicit offset of its own: it returns the focus to the start.
    assert_eq!(offsets, vec![-1.0, -0.5, 0.0, 0.5, 1.0]);
    let cut_lines = plan.events().iter().filter(|e| matches!(e, PlanEvent::Cut { .. })).count();
    assert_eq!(cut_lines, 5);
    // The same job on a profile without Z is refused by the compiler too.
    let err = job.compile(&profile(), &options(), &CancellationToken::never()).unwrap_err();
    assert!(matches!(err, CompileError::UnsupportedFeature { feature: Feature::ZStep, .. }));
    let _: Option<ProcessSettings> = None;
}
