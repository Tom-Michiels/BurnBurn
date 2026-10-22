//! G6-machine, C05: the grayscale image mode against an independent reference. The expected power
//! of every dot comes from `test-data/acceptance/image_grayscale_reference.py` (pure Python with
//! exact rationals, written from the published definition of the mode); the Rust side is read
//! back from the compiled plan's power runs, never from the engraver's intermediate levels.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test code

mod acc_cam;

use acc_cam::*;
use burnburn_cam::compile_with;
use burnburn_document::DocumentSession;
use burnburn_document::process::{
    ImageRender, ImageSettings, OperationDraft, OperationMode, Passes, PowerPercent, PowerRange, ProcessSettings, Speed,
};
use burnburn_image::SourceImage;
use burnburn_job::{Feature, JobPlan, PlanEvent};
use burnburn_test_support::fixtures::read_fixture;
use burnburn_types::CancellationToken;

/// The golden's source bitmap (12 x 6, gray8) and its cases: `(variant, scale, rows of cells)`.
fn golden() -> (SourceImage, Vec<(String, usize, Vec<Vec<String>>)>) {
    let text = String::from_utf8(read_fixture("acceptance", "image-grayscale-golden.txt").unwrap()).unwrap();
    let mut px = Vec::new();
    let mut cases = Vec::new();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("# ") {
            if rest.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                px.extend(rest.split(' ').map(|v| v.parse::<u8>().unwrap()));
            }
            continue;
        }
        let (head, body) = line.split_once(": ").unwrap();
        let (variant, scale) = head.split_once(' ').unwrap();
        let rows = body.split('/').map(|r| r.split(',').map(str::to_owned).collect()).collect();
        cases.push((variant.to_owned(), scale.parse::<usize>().unwrap(), rows));
    }
    (SourceImage::from_gray8(12, 6, px).unwrap(), cases)
}

/// The layer of the golden: 10 % to 80 % power at 100 mm/s.
fn layer() -> ProcessSettings {
    ProcessSettings {
        speed: Speed::mm_per_s(100.0).unwrap(),
        power: PowerRange::new(PowerPercent::new(10.0).unwrap(), PowerPercent::new(80.0).unwrap()).unwrap(),
        passes: Passes::new(1).unwrap(),
        air_assist: true,
    }
}

fn scene(settings: ImageSettings, w: f64, h: f64) -> DocumentSession {
    let mut s = session();
    let frame = rectangle(&mut s, 10.0, 10.0, w, h);
    let draft = OperationDraft {
        name: "Grijs".into(),
        mode: OperationMode::Image(settings),
        settings: layer(),
        output_enabled: true,
    };
    add_op(&mut s, draft, vec![frame]);
    s
}

fn plan(s: &DocumentSession, source: &SourceImage) -> JobPlan {
    // A profile that has proven grayscale modulation: on any other the mode is refused.
    let profile = supporting(&[Feature::GrayscaleModulation]);
    compile_with(&s.snapshot(), &profile, &options(), &vec![source.clone()], &CancellationToken::never()).unwrap()
}

/// The power of every dot of a `cols x rows` raster of 0.1 mm dots with its lower-left corner at
/// (10, 10), in thousandths of full power; `-` where no power run covers the dot. Top row first.
fn powers(plan: &JobPlan, cols: usize, rows: usize) -> Vec<Vec<String>> {
    let mut grid = vec![vec!["-".to_owned(); cols]; rows];
    for e in plan.events() {
        if let PlanEvent::PowerRun { line, power, .. } = *e {
            let (a, b) = (line.from.x_mm(), line.to.x_mm());
            let (x0, x1, y) = (a.min(b), a.max(b), line.from.y_mm());
            let row = rows - 1 - ((y - 10.0) / 0.1 - 0.5).round() as usize;
            let (c0, c1) = (((x0 - 10.0) / 0.1).round() as usize, ((x1 - 10.0) / 0.1).round() as usize);
            for cell in &mut grid[row][c0..c1] {
                assert_eq!(cell, "-", "a dot is covered by one power run only");
                *cell = format!("{}", (power.get() * 1000.0).round() as i64);
            }
        }
    }
    grid
}

#[test]
fn grayscale_power_follows_the_gray_of_every_dot_like_the_python_reference() {
    let (source, cases) = golden();
    assert_eq!(cases.len(), 4, "grayscale and its negative, at one and two times scale");
    let mut failures = Vec::new();
    for (variant, scale, expected) in &cases {
        let (cols, rows) = (12 * scale, 6 * scale);
        let settings = ImageSettings::new(source.content_hash(), mm(0.1), mm(0.1), ImageRender::Grayscale).unwrap();
        let settings = settings.with_negative(variant == "grayscale-negative");
        let s = scene(settings, cols as f64 / 10.0, rows as f64 / 10.0);
        let got = powers(&plan(&s, &source), cols, rows);
        if &got != expected {
            let show = |g: &[Vec<String>]| g.iter().map(|r| r.join(",")).collect::<Vec<_>>().join("/");
            failures.push(format!("{variant} x{scale}:\n  got      {}\n  expected {}", show(&got), show(expected)));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of 4 differ from the independent reference:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn a_white_dot_is_not_burnt_and_a_black_one_burns_at_the_layers_maximum() {
    let source = SourceImage::from_gray8(3, 1, vec![0, 255, 128]).unwrap();
    let settings = ImageSettings::new(source.content_hash(), mm(0.1), mm(0.1), ImageRender::Grayscale).unwrap();
    let s = scene(settings, 0.3, 0.1);
    let grid = powers(&plan(&s, &source), 3, 1);
    // 0 -> 800, 255 -> not burnt, 128 -> 100 + 700 * 127 / 255 = 448.6 -> 449 thousandths.
    assert_eq!(grid, vec![vec!["800".to_owned(), "-".to_owned(), "449".to_owned()]]);
}
