//! Common-line cutting (C12, G7-cam): an option of a Line operation. Where the toolpaths of
//! different shapes coincide within a tolerance over at least a minimal length, the stretch is cut
//! once. The option is validated at construction, excludes the options that do not follow
//! contours, is part of the canonical hash only when set, and never touches a tool layer.

#![allow(clippy::unwrap_used)]

use burnburn_document::process::{
    AdvancedOptions, CommonLine, DotMode, Kerf, LineOptions, Operation, OperationDraft, OperationMode, Passes,
    PowerPercent, PowerRange, ProcessError, ProcessSettings, Speed, ToolLayer,
};
use burnburn_types::{CanonicalEncode, CanonicalWriter, GeometryId, Length, OperationId, StableId};

fn mm(v: f64) -> Length {
    Length::from_mm(v).unwrap()
}

fn draft(mode: OperationMode) -> OperationDraft {
    let p = PowerPercent::new(20.0).unwrap();
    OperationDraft {
        name: "Op".into(),
        mode,
        settings: ProcessSettings {
            speed: Speed::mm_per_s(100.0).unwrap(),
            power: PowerRange::new(p, p).unwrap(),
            passes: Passes::new(1).unwrap(),
            air_assist: true,
        },
        output_enabled: true,
    }
}

fn line_op() -> Operation {
    Operation::new(
        OperationId::from_raw(5),
        draft(OperationMode::Line { kerf: Kerf::NONE }),
        vec![GeometryId::from_raw(1), GeometryId::from_raw(2)],
        None,
    )
    .unwrap()
}

fn bytes(op: &Operation) -> Vec<u8> {
    let mut w = CanonicalWriter::new();
    op.encode(&mut w);
    w.finish().unwrap()
}

#[test]
fn the_tolerance_must_be_positive_and_small_and_the_overlap_not_negative() {
    let ok = CommonLine::new(mm(0.05), mm(2.0)).unwrap();
    assert_eq!((ok.tolerance_mm(), ok.min_overlap_mm()), (0.05, 2.0));
    assert!(CommonLine::new(mm(0.05), mm(0.0)).is_ok(), "no minimum is allowed");
    for bad in [0.0, -0.1, 5.5] {
        assert_eq!(
            CommonLine::new(mm(bad), mm(1.0)),
            Err(ProcessError::OutOfRange { field: "common-line tolerance" }),
            "{bad}"
        );
    }
    assert_eq!(
        CommonLine::new(mm(0.05), mm(-1.0)),
        Err(ProcessError::OutOfRange { field: "common-line overlap" })
    );
}

#[test]
fn it_belongs_to_line_operations_and_excludes_dot_mode_and_tool_layers() {
    let cl = CommonLine::new(mm(0.05), mm(1.0)).unwrap();
    let with = |advanced: AdvancedOptions| line_op().with_advanced(advanced);
    assert!(with(AdvancedOptions { common_line: Some(cl), ..Default::default() }).is_ok());

    let dot = LineOptions { dot: Some(DotMode::new(mm(1.0), 2.0).unwrap()), ..Default::default() };
    assert_eq!(
        with(AdvancedOptions { line: dot, common_line: Some(cl), ..Default::default() }).unwrap_err(),
        ProcessError::ConflictingOptions { first: "dot mode", second: "common line" }
    );
    let tool = AdvancedOptions { common_line: Some(cl), tool: Some(ToolLayer::T1), ..Default::default() };
    assert_eq!(
        with(tool).unwrap_err(),
        ProcessError::ConflictingOptions { first: "tool layer", second: "common line" }
    );
}

#[test]
fn the_option_is_in_the_hash_with_its_values_and_an_operation_without_it_keeps_its_hash() {
    let plain = bytes(&line_op());
    let with = |tol: f64, min: f64| {
        bytes(
            &line_op()
                .with_advanced(AdvancedOptions {
                    common_line: Some(CommonLine::new(mm(tol), mm(min)).unwrap()),
                    ..Default::default()
                })
                .unwrap(),
        )
    };
    assert_ne!(plain, with(0.05, 1.0));
    assert_ne!(with(0.05, 1.0), with(0.06, 1.0));
    assert_ne!(with(0.05, 1.0), with(0.05, 2.0));
    assert_eq!(with(0.05, 1.0), with(0.05, 1.0));
    let off = line_op().with_advanced(AdvancedOptions { common_line: None, ..Default::default() }).unwrap();
    assert_eq!(bytes(&off), plain);
}
