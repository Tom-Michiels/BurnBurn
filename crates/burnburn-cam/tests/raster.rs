//! IM18 raster production in the compiler: image operations are dithered from the original
//! source pixels at the physical grid of the operation, at compile time.

#![allow(clippy::unwrap_used)]

mod common;

use burnburn_cam::{CompileError, compile, compile_with};
use burnburn_document::process::{DitherMode, ImageRender, ImageSettings, OperationDraft, OperationMode};
use burnburn_document::{Anchor, Axis, CommandPayload};
use burnburn_image::SourceImage;
use burnburn_job::{PlanEvent, PlanMode, ScanAxis, SourceRef};
use burnburn_types::{CancellationToken, GeometryId, OperationId};
use common::*;

fn image_op(source: &SourceImage, render: ImageRender) -> OperationDraft {
    let settings = ImageSettings::new(source.content_hash(), mm(0.1), mm(0.1), render).unwrap();
    OperationDraft {
        name: "Graveren".into(),
        mode: OperationMode::Image(settings),
        settings: settings_for_image(),
        output_enabled: true,
    }
}

fn settings_for_image() -> burnburn_document::process::ProcessSettings {
    settings(100.0, 60.0)
}

pub fn runs(events: &[PlanEvent]) -> Vec<(f64, f64, f64, Option<SourceRef>)> {
    events
        .iter()
        .filter_map(|e| match *e {
            PlanEvent::ScanRun { line, origin } => Some((line.from.x_mm(), line.to.x_mm(), line.from.y_mm(), origin)),
            _ => None,
        })
        .collect()
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

fn one_gray_pixel() -> (SourceImage, burnburn_document::DocumentSession, GeometryId, OperationId) {
    let source = SourceImage::from_gray8(1, 1, vec![128]).unwrap();
    let mut s = session();
    let frame = rectangle(&mut s, 10.0, 10.0, 0.1, 0.1);
    let op = add_op(&mut s, image_op(&source, ImageRender::Dither(DitherMode::FloydSteinberg)), vec![frame]);
    (source, s, frame, op)
}

#[test]
fn dither_runs_are_regenerated_from_source_after_physical_resize() {
    let (source, mut s, frame, op) = one_gray_pixel();
    let sources = vec![source];
    let small = compile_with(&s.snapshot(), &profile(), &options(), &sources, &CancellationToken::never()).unwrap();
    assert!(
        matches!(small.settings()[0].mode, PlanMode::Scan { interval_mm, axis: ScanAxis::X, .. } if interval_mm == 0.1)
    );
    // 0.1 mm at 0.1 mm pitch: one dot of gray 128, which Floyd–Steinberg leaves white.
    assert_eq!(runs(small.events()), vec![]);

    // Physical resize to 0.4 mm: four dots of 128 from the *source*. Floyd–Steinberg on
    // 128 128 128 128 gives white, burn, white, burn (error −127 → 73 → −96 → 86). Resampling
    // the earlier one-dot result would stay white everywhere.
    run(&mut s, CommandPayload::SetDimension { target: frame, axis: Axis::Width, value: mm(0.4), anchor: Anchor::Min });
    let big = compile_with(&s.snapshot(), &profile(), &options(), &sources, &CancellationToken::never()).unwrap();
    let r = runs(big.events());
    assert_eq!(r.len(), 2, "{r:?}");
    let src = Some(SourceRef { geometry: frame, operation: op });
    for ((x0, x1, y, origin), (e0, e1)) in r.iter().zip([(10.1, 10.2), (10.3, 10.4)]) {
        assert!(near(*x0, e0) && near(*x1, e1) && near(*y, 10.05), "run {x0}..{x1} at {y}");
        assert_eq!(*origin, src);
    }
}

#[test]
fn image_operation_without_its_source_pixels_is_an_error() {
    let (source, s, _, op) = one_gray_pixel();
    let err = compile(&s.snapshot(), &profile(), &options(), &CancellationToken::never()).unwrap_err();
    assert_eq!(err, CompileError::MissingImageSource { operation: op, source: source.content_hash() });
}

#[test]
fn image_rows_run_top_to_bottom_in_image_order_and_zig_zag() {
    // 2 × 2 source: top row black-white, bottom row white-black, threshold dither. The frame is
    // 0.2 × 0.2 mm at 0.1 mm: row centres y = 10.15 (image row 0) and 10.05 (row 1). The head
    // starts at the machine origin, so the bottom row goes first, left to right.
    let source = SourceImage::from_gray8(2, 2, vec![0, 255, 255, 0]).unwrap();
    let mut s = session();
    let frame = rectangle(&mut s, 10.0, 10.0, 0.2, 0.2);
    add_op(&mut s, image_op(&source, ImageRender::Dither(DitherMode::Threshold { level: 128 })), vec![frame]);
    let plan = compile_with(&s.snapshot(), &profile(), &options(), &vec![source], &CancellationToken::never()).unwrap();
    let r: Vec<(f64, f64, f64)> = runs(plan.events()).into_iter().map(|(a, b, y, _)| (a, b, y)).collect();
    assert_eq!(r.len(), 2, "{r:?}");
    assert!(near(r[0].0, 10.1) && near(r[0].1, 10.2) && near(r[0].2, 10.05), "bottom row: right dot {r:?}");
    assert!(near(r[1].0, 10.1) && near(r[1].1, 10.0) && near(r[1].2, 10.15), "top row: left dot, reversed {r:?}");
}
