//! G6-machine, C05: an already dithered image burnt in "pass-through" (fixed pixel scale) mode is
//! only exact when every source pixel becomes a whole number of dots. Otherwise pixels are
//! duplicated or skipped unevenly and the preflight says so. It needs the source pixels, so
//! without them it says nothing rather than guess.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use burnburn_cam::{Finding, NoRasters, Severity, compile_with, preflight, preflight_with};
use burnburn_document::DocumentSession;
use burnburn_document::process::{
    DitherMode, ImageCrop, ImageRender, ImageSettings, OperationDraft, OperationMode,
};
use burnburn_image::SourceImage;
use burnburn_types::{CancellationToken, OperationId};
use common::*;

fn source(w: u32, h: u32) -> SourceImage {
    SourceImage::from_gray8(w, h, (0..w * h).map(|i| if i % 2 == 0 { 0 } else { 255 }).collect()).unwrap()
}

fn scene(src: &SourceImage, mode: DitherMode, dots_w: f64, dots_h: f64, crop: Option<ImageCrop>) -> (DocumentSession, OperationId) {
    let mut s = session();
    // 0.1 mm dots: a frame of dots_w x dots_h dots.
    let frame = rectangle(&mut s, 10.0, 10.0, dots_w / 10.0, dots_h / 10.0);
    let mut settings = ImageSettings::new(src.content_hash(), mm(0.1), mm(0.1), ImageRender::Dither(mode)).unwrap();
    if let Some(c) = crop {
        settings = settings.with_crop(Some(c)).unwrap();
    }
    let draft = OperationDraft {
        name: "Graveren".into(),
        mode: OperationMode::Image(settings),
        settings: settings_for(),
        output_enabled: true,
    };
    let op = add_op(&mut s, draft, vec![frame]);
    (s, op)
}

fn settings_for() -> burnburn_document::process::ProcessSettings {
    settings(100.0, 60.0)
}

fn findings(s: &DocumentSession, src: &SourceImage) -> Vec<Finding> {
    let profile = profile();
    let sources = vec![src.clone()];
    let plan = compile_with(&s.snapshot(), &profile, &options(), &sources, &CancellationToken::never()).unwrap();
    preflight_with(&s.snapshot(), &plan, &profile, &sources).findings
}

fn resampled(f: &[Finding]) -> Vec<&Finding> {
    f.iter().filter(|f| matches!(f, Finding::PassThroughResampled { .. })).collect()
}

#[test]
fn one_dot_per_source_pixel_and_whole_multiples_are_exact_and_say_nothing() {
    let src = source(12, 6);
    for (w, h) in [(12.0, 6.0), (24.0, 12.0), (36.0, 18.0)] {
        let (s, _) = scene(&src, DitherMode::PassThrough, w, h, None);
        assert!(resampled(&findings(&s, &src)).is_empty(), "{w} x {h} dots");
    }
}

#[test]
fn an_uneven_scale_is_a_warning_that_names_both_grids() {
    let src = source(12, 6);
    let (s, op) = scene(&src, DitherMode::PassThrough, 30.0, 15.0, None); // 2.5 dots per pixel
    let f = findings(&s, &src);
    let hits = resampled(&f);
    assert_eq!(hits.len(), 1, "{f:?}");
    assert_eq!(
        hits[0],
        &Finding::PassThroughResampled {
            operation: op,
            geometry: s.snapshot().objects().next().unwrap().0,
            source_px: (12, 6),
            dots: (30, 15),
        }
    );
    assert_eq!(hits[0].severity(), Severity::Warning, "the job can still run");
}

#[test]
fn shrinking_skips_pixels_and_is_warned_about_too() {
    let src = source(12, 6);
    let (s, _) = scene(&src, DitherMode::PassThrough, 6.0, 3.0, None);
    assert_eq!(resampled(&findings(&s, &src)).len(), 1, "every other pixel is skipped");
}

#[test]
fn the_crop_window_is_the_source_that_counts() {
    let src = source(12, 6);
    let crop = ImageCrop { x: 0, y: 0, width: 6, height: 3 };
    let (s, _) = scene(&src, DitherMode::PassThrough, 12.0, 6.0, Some(crop));
    assert!(resampled(&findings(&s, &src)).is_empty(), "6 x 3 pixels onto 12 x 6 dots is a whole multiple");
    let crop = ImageCrop { x: 0, y: 0, width: 5, height: 3 };
    let (s, _) = scene(&src, DitherMode::PassThrough, 12.0, 6.0, Some(crop));
    assert_eq!(resampled(&findings(&s, &src)).len(), 1, "5 pixels onto 12 dots is not");
}

#[test]
fn only_pass_through_is_judged_the_other_modes_resample_by_design() {
    let src = source(12, 6);
    let (s, _) = scene(&src, DitherMode::FloydSteinberg, 30.0, 15.0, None);
    assert!(resampled(&findings(&s, &src)).is_empty());
}

#[test]
fn without_the_source_pixels_the_preflight_cannot_know_and_says_nothing() {
    let src = source(12, 6);
    let (s, _) = scene(&src, DitherMode::PassThrough, 30.0, 15.0, None);
    let profile = profile();
    let plan =
        compile_with(&s.snapshot(), &profile, &options(), &vec![src.clone()], &CancellationToken::never()).unwrap();
    assert!(resampled(&preflight(&s.snapshot(), &plan, &profile).findings).is_empty());
    assert!(resampled(&preflight_with(&s.snapshot(), &plan, &profile, &NoRasters).findings).is_empty());
}
