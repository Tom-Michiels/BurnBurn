//! LF11, headless: the toolpath preview as a professional picture. Anti-aliased strokes, cuts in
//! their layer colour, travel dashed and grey, what is played dimmed (not green), a head marker,
//! a legend of drawn swatch plus word, lengths in the preferred unit and a scrub position that
//! maps to the right plan event. The pixels are checked with a tolerance, never against a golden.

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers outside #[test] fns

use std::collections::BTreeSet;
use std::sync::Arc;

use burnburn_host_models::format::{LengthPreference, format_length};
use burnburn_macos::app_view::{
    LegendEntry, PLAYED_OPACITY, PickNames, PreviewFrame, PreviewPanel, PreviewStyle, Swatch, event_at_seconds,
    preview_legend, preview_panel, preview_panel_units, render_preview, render_preview_styled, render_swatch,
};
use burnburn_macos::l10n::Locale;
use burnburn_macos::panels_image::BitmapImage;
use burnburn_presentation::{
    AppIntent, CommandKey, HeadlessEditorHarness, MachinePoint, MoveKind, PlanPreview, PreviewIntent, PreviewMove,
    PreviewViewModel, UiIntent,
};
use burnburn_types::{DocumentId, OperationId};

const BED: [f64; 3] = [245.0, 245.0, 245.0];

fn machine_profile() -> burnburn_job::MachineProfile {
    use burnburn_job::{MachineFrame, MachineProfile, OriginCorner, PowerRatio};
    MachineProfile::new(
        "research-1500x900",
        1,
        MachineFrame::new(1500.0, 900.0, OriginCorner::BottomLeft).unwrap(),
        1000.0,
        400.0,
        PowerRatio::new(0.0).unwrap(),
        PowerRatio::new(1.0).unwrap(),
    )
    .unwrap()
}

fn cut() -> burnburn_document::process::OperationDraft {
    use burnburn_document::process::{
        Kerf, OperationDraft, OperationMode, Passes, PowerPercent, PowerRange, ProcessSettings, Speed,
    };
    let p = PowerPercent::new(20.0).unwrap();
    OperationDraft {
        name: "Snijden".into(),
        mode: OperationMode::Line { kerf: Kerf::NONE },
        settings: ProcessSettings {
            speed: Speed::mm_per_s(100.0).unwrap(),
            power: PowerRange::new(p, p).unwrap(),
            passes: Passes::new(1).unwrap(),
            air_assist: true,
        },
        output_enabled: true,
    }
}

fn compiled() -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.send(UiIntent::SetMachineProfile {
        profile: Box::new(machine_profile()),
        ruida: burnburn_ruida::RuidaProfile {
            magic: burnburn_ruida::Magic::FIXTURE_0X88,
            overscan: burnburn_ruida::OverscanPolicy::ControllerManaged,
        },
    });
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (10.0, 10.0), (130.0, 90.0));
    let rect = h.document(doc).unwrap().objects[0].id;
    h.send(UiIntent::Select { document: doc, ids: vec![rect], mode: burnburn_presentation::SelectMode::Replace });
    h.send(UiIntent::AddOperation { document: doc, draft: cut() });
    h.invoke(doc, CommandKey::ToolSelect);
    h.invoke(doc, CommandKey::Compile);
    h.run_tasks();
    (h, doc)
}

fn mv(
    kind: MoveKind,
    from: [f64; 2],
    to: [f64; 2],
    event: usize,
    seconds: f64,
    op: Option<OperationId>,
) -> PreviewMove {
    PreviewMove {
        kind,
        from: MachinePoint::new(from[0], from[1]).unwrap(),
        to: MachinePoint::new(to[0], to[1]).unwrap(),
        event,
        operation: op,
        source: None,
        seconds,
        power: None,
    }
}

/// The compiled view with its moves replaced by `moves` (analytical geometry) and the clock after
/// `completed` of them. Operation 0 of the real plan keeps its layer slot.
fn synthetic(view: &PreviewViewModel, moves: Vec<PreviewMove>, completed: usize) -> PreviewViewModel {
    let mut v = view.clone();
    let mut preview: PlanPreview = (*v.preview).clone();
    preview.moves = moves;
    v.preview = Arc::new(preview);
    v.completed_moves = completed;
    v.partial = None;
    v.selected = None;
    v.head = [100.0, 800.0];
    v
}

fn first_op(view: &PreviewViewModel) -> OperationId {
    *view.layer_slots.keys().next().unwrap()
}

fn rgb(image: &BitmapImage, x: i64, y: i64) -> [f64; 3] {
    let i = (y as usize * image.width as usize + x as usize) * 4;
    [f64::from(image.rgba[i]), f64::from(image.rgba[i + 1]), f64::from(image.rgba[i + 2])]
}

fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// The pixel within one pixel of `at` that is closest to `want`: a stroke whose centre falls between
/// pixel rows does not cover the rounded pixel completely.
fn closest(image: &BitmapImage, at: [f64; 2], want: [f64; 3]) -> [f64; 3] {
    let (cx, cy) = (at[0].round() as i64, at[1].round() as i64);
    let mut best = rgb(image, cx, cy);
    for dy in -1..=1 {
        for dx in -1..=1 {
            let p = rgb(image, cx + dx, cy + dy);
            if dist(p, want) < dist(best, want) {
                best = p;
            }
        }
    }
    best
}

fn style(layer: [u8; 3]) -> PreviewStyle {
    PreviewStyle { layers: vec![layer], pixel_ratio: 1.0 }
}

#[test]
fn a_diagonal_move_is_antialiased_with_intermediate_tones_on_its_edges() {
    let (mut h, doc) = compiled();
    let base = h.controller().preview_view(doc).unwrap();
    let op = first_op(&base);
    let view = synthetic(&base, vec![mv(MoveKind::Cut, [200.0, 100.0], [1200.0, 700.0], 0, 5.0, Some(op))], 0);
    let frame = PreviewFrame::fit(view.bed_mm.unwrap(), [760, 456], 8);
    let image = render_preview_styled(&view, &frame, &style([0, 0, 255]));
    let ink = [0.0, 0.0, 255.0];
    // Walk the columns across the middle of the line and collect the tones between bed and ink.
    let (a, b) = (frame.to_px([200.0, 100.0]), frame.to_px([1200.0, 700.0]));
    let mut tones = BTreeSet::new();
    let mut full_ink = 0;
    for x in (a[0] as i64 + 20)..(b[0] as i64 - 20) {
        let yc = a[1] + (b[1] - a[1]) * (x as f64 - a[0]) / (b[0] - a[0]);
        for y in (yc as i64 - 3)..=(yc as i64 + 3) {
            let p = rgb(&image, x, y);
            if dist(p, ink) < 20.0 {
                full_ink += 1;
            } else if dist(p, BED) > 12.0 {
                tones.insert(p[0] as i64);
            }
        }
    }
    assert!(full_ink > 100, "the stroke has a solid core: {full_ink}");
    assert!(tones.len() >= 3, "a slanted edge has at least three intermediate tones, got {tones:?}");
}

#[test]
fn a_cut_is_drawn_in_the_colour_of_its_layer() {
    let (mut h, doc) = compiled();
    let base = h.controller().preview_view(doc).unwrap();
    let op = first_op(&base);
    let frame = PreviewFrame::fit(base.bed_mm.unwrap(), [760, 456], 8);
    // The line runs through the middle of a pixel row, so one pixel is fully covered.
    let y = frame.to_mm([0.0, 230.5])[1];
    let view = synthetic(&base, vec![mv(MoveKind::Cut, [200.0, y], [1000.0, y], 0, 4.0, Some(op))], 0);
    let mid = frame.to_px([600.0, y]);
    for layer in [[0, 160, 200], [190, 0, 190]] {
        let image = render_preview_styled(&view, &frame, &style(layer));
        let want = [f64::from(layer[0]), f64::from(layer[1]), f64::from(layer[2])];
        let got = closest(&image, mid, want);
        assert!(dist(got, want) < 25.0, "layer colour {layer:?} -> {got:?}");
    }
}

#[test]
fn what_is_played_is_dimmed_in_its_own_colour_and_never_green() {
    let (mut h, doc) = compiled();
    let base = h.controller().preview_view(doc).unwrap();
    let op = first_op(&base);
    let frame = PreviewFrame::fit(base.bed_mm.unwrap(), [760, 456], 8);
    let y = frame.to_mm([0.0, 230.5])[1];
    let moves = vec![mv(MoveKind::Cut, [200.0, y], [1000.0, y], 0, 4.0, Some(op))];
    let unplayed = synthetic(&base, moves.clone(), 0);
    let played = synthetic(&base, moves, 1);
    let mid = frame.to_px([600.0, y]);
    let at = |v: &PreviewViewModel, layer: [u8; 3], want: [f64; 3]| {
        let image = render_preview_styled(v, &frame, &style(layer));
        closest(&image, mid, want)
    };
    assert!((PLAYED_OPACITY - 0.35).abs() < 1e-9, "the plan's 35 %");
    for layer in [[255u8, 0, 0], [0, 0, 255]] {
        let want: [f64; 3] =
            std::array::from_fn(|c| PLAYED_OPACITY * f64::from(layer[c]) + (1.0 - PLAYED_OPACITY) * BED[c]);
        let got = at(&played, layer, want);
        assert!(dist(got, want) < 14.0, "played {layer:?} is the layer colour at 35 %: {got:?} vs {want:?}");
        let fresh = at(&unplayed, layer, layer.map(f64::from));
        assert!(got.iter().sum::<f64>() > fresh.iter().sum::<f64>() + 100.0, "played is lighter than not played");
        assert!(!(got[1] > got[0] + 30.0 && got[1] > got[2] + 30.0), "played is not green: {got:?}");
    }
}

#[test]
fn travel_is_dashed_grey_with_gaps_and_hidden_travel_is_not_drawn() {
    let (mut h, doc) = compiled();
    let base = h.controller().preview_view(doc).unwrap();
    let op = first_op(&base);
    let moves = vec![
        mv(MoveKind::Travel, [100.0, 450.0], [1400.0, 450.0], 0, 2.0, None),
        mv(MoveKind::Cut, [100.0, 200.0], [300.0, 200.0], 1, 1.0, Some(op)),
    ];
    let view = synthetic(&base, moves, 0);
    let frame = PreviewFrame::fit(view.bed_mm.unwrap(), [760, 456], 8);
    let image = render_preview_styled(&view, &frame, &style([255, 0, 0]));
    let yc = frame.to_px([0.0, 450.0])[1];
    let (x0, x1) = (frame.to_px([150.0, 0.0])[0] as i64, frame.to_px([1350.0, 0.0])[0] as i64);
    let inked = |x: i64| (-1..=1).any(|d| dist(rgb(&image, x, yc.round() as i64 + d), BED) > 25.0);
    let runs = (x0..x1).filter(|x| inked(*x) != inked(*x + 1)).count();
    assert!(runs >= 12, "a dashed line alternates ink and gap, got {runs} edges");
    let grey =
        (x0..x1).filter(|x| inked(*x)).map(|x| rgb(&image, x, yc.round() as i64)).filter(|p| dist(*p, BED) > 25.0);
    for p in grey.take(20) {
        assert!((p[0] - p[1]).abs() < 12.0 && (p[1] - p[2]).abs() < 12.0, "travel is grey: {p:?}");
    }
    let mut hidden_view = view.clone();
    hidden_view.show_travel = false;
    let hidden = render_preview_styled(&hidden_view, &frame, &style([255, 0, 0]));
    assert!(
        (x0..x1).all(|x| dist(rgb(&hidden, x, yc.round() as i64), BED) < 12.0),
        "hidden travel leaves the bed alone"
    );
    let _ = &mut h;
}

#[test]
fn the_head_is_a_cross_in_a_circle_at_the_current_position() {
    let (mut h, doc) = compiled();
    let base = h.controller().preview_view(doc).unwrap();
    let view = synthetic(&base, Vec::new(), 0);
    let frame = PreviewFrame::fit(view.bed_mm.unwrap(), [760, 456], 8);
    let image = render_preview_styled(&view, &frame, &PreviewStyle::default());
    let c = frame.to_px(view.head);
    let dark = |x: f64, y: f64| dist(rgb(&image, x.round() as i64, y.round() as i64), BED) > 90.0;
    assert!(dark(c[0], c[1] - 3.0) || dark(c[0] - 3.0, c[1]), "the cross has arms around the centre");
    let ring = (0..360).filter(|d| {
        let a = f64::from(*d).to_radians();
        (5..=8).any(|r| dark(c[0] + f64::from(r) * a.cos(), c[1] + f64::from(r) * a.sin()))
    });
    assert!(ring.count() > 300, "a closed ring surrounds the cross");
}

#[test]
fn a_higher_pixel_ratio_draws_proportionally_thicker_strokes() {
    let (mut h, doc) = compiled();
    let base = h.controller().preview_view(doc).unwrap();
    let op = first_op(&base);
    let view = synthetic(&base, vec![mv(MoveKind::Cut, [200.0, 450.0], [1000.0, 450.0], 0, 4.0, Some(op))], 0);
    let thickness = |ratio: f64| {
        let frame = PreviewFrame::fit(
            view.bed_mm.unwrap(),
            [(760.0 * ratio) as u32, (456.0 * ratio) as u32],
            (8.0 * ratio) as u32,
        );
        let image = render_preview_styled(&view, &frame, &PreviewStyle { layers: vec![[0, 0, 0]], pixel_ratio: ratio });
        let x = frame.to_px([600.0, 450.0])[0].round() as i64;
        let y = frame.to_px([600.0, 450.0])[1].round() as i64;
        (y - 12..y + 12).map(|y| (BED[0] - rgb(&image, x, y)[0]) / BED[0]).sum::<f64>()
    };
    let (one, two) = (thickness(1.0), thickness(2.0));
    assert!((two / one - 2.0).abs() < 0.3, "ink coverage doubles with the ratio: {one} -> {two}");
}

// ---- legend ----------------------------------------------------------------------------------

#[test]
fn the_legend_has_a_swatch_and_a_word_per_role_in_dutch_and_english() {
    let nl: Vec<&str> = preview_legend(Locale::Nl).iter().map(|e| e.word).collect();
    let en: Vec<&str> = preview_legend(Locale::En).iter().map(|e| e.word).collect();
    assert_eq!(nl, ["Snijden", "Verlaagd vermogen (tab)", "Verplaatsing", "Gespeeld", "Gekozen", "Positie"]);
    assert_eq!(en, ["Cut", "Lowered power (tab)", "Travel", "Played", "Picked", "Position"]);
    let swatches: Vec<Swatch> = preview_legend(Locale::En).iter().map(|e| e.swatch).collect();
    assert_eq!(
        swatches,
        [Swatch::Cut, Swatch::LoweredPower, Swatch::Travel, Swatch::Played, Swatch::Picked, Swatch::Head]
    );
    for entry in preview_legend(Locale::Nl) {
        let LegendEntry { word, .. } = entry;
        for colour in ["rood", "groen", "blauw", "oranje", "grijs"] {
            assert!(!word.to_lowercase().contains(colour), "colour is never the only carrier, nor the word: {word}");
        }
    }
}

#[test]
fn every_legend_swatch_is_drawn_and_none_is_green() {
    let style = style([0, 0, 255]);
    for entry in preview_legend(Locale::En) {
        let image = render_swatch(entry.swatch, [28, 14], &style);
        assert_eq!((image.width, image.height), (28, 14));
        let tones: BTreeSet<[i64; 3]> = (0..14)
            .flat_map(|y| (0..28).map(move |x| (x, y)))
            .map(|(x, y)| rgb(&image, x, y).map(|v| v as i64))
            .collect();
        assert!(tones.len() >= 2, "{:?} is drawn, not blank", entry.swatch);
        for t in tones {
            assert!(!(t[1] > t[0] + 40 && t[1] > t[2] + 40), "{:?} has a green pixel {t:?}", entry.swatch);
        }
    }
    // The cut swatch carries the layer colour.
    let image = render_swatch(Swatch::Cut, [28, 14], &style);
    assert!(dist(rgb(&image, 14, 7), [0.0, 0.0, 255.0]) < 25.0);
}

// ---- panel text ------------------------------------------------------------------------------

#[test]
fn the_panel_gives_the_lengths_in_the_preferred_unit_and_the_play_state() {
    let (mut h, doc) = compiled();
    let view = h.controller().preview_view(doc).unwrap();
    let cut_mm = view.preview.cut_length_mm;
    let travel_mm = view.preview.travel_length_mm;
    assert!(cut_mm > 100.0);
    let mm: PreviewPanel = preview_panel_units(&view, &PickNames::default(), Locale::En, LengthPreference::Millimeters);
    assert!(
        mm.lengths_text.contains(&format_length(cut_mm, LengthPreference::Millimeters, Locale::En)),
        "{}",
        mm.lengths_text
    );
    assert!(mm.lengths_text.contains(&format_length(travel_mm, LengthPreference::Millimeters, Locale::En)));
    assert!(mm.lengths_text.starts_with("Cut "), "{}", mm.lengths_text);
    let inch = preview_panel_units(&view, &PickNames::default(), Locale::Nl, LengthPreference::Inches);
    assert!(
        inch.lengths_text.contains(&format_length(cut_mm, LengthPreference::Inches, Locale::Nl)),
        "{}",
        inch.lengths_text
    );
    assert!(inch.lengths_text.starts_with("Snijden "), "{}", inch.lengths_text);
    assert!(!preview_panel(&view, Locale::En).playing);
    h.send(UiIntent::App(AppIntent::Preview(PreviewIntent::Play { document: doc, playing: true })));
    assert!(preview_panel(&h.controller().preview_view(doc).unwrap(), Locale::En).playing);
}

// ---- scrubbing -------------------------------------------------------------------------------

#[test]
fn a_scrub_position_maps_to_the_event_of_the_move_under_the_clock() {
    let (mut h, doc) = compiled();
    let base = h.controller().preview_view(doc).unwrap();
    // Three moves of 1, 2 and 3 seconds that belong to the plan events 4, 7 and 9.
    let moves = vec![
        mv(MoveKind::Travel, [0.0, 0.0], [10.0, 0.0], 4, 1.0, None),
        mv(MoveKind::Cut, [10.0, 0.0], [10.0, 10.0], 7, 2.0, None),
        mv(MoveKind::Cut, [10.0, 10.0], [0.0, 10.0], 9, 3.0, None),
    ];
    let preview = synthetic(&base, moves, 0).preview;
    let cases = [
        (0.0, Some(4)),
        (0.99, Some(4)),
        (1.0, Some(7)),
        (2.99, Some(7)),
        (3.0, Some(9)),
        (5.99, Some(9)),
        (6.0, Some(9)),
        (1e9, Some(9)),
        (-5.0, Some(4)),
        (f64::NAN, Some(4)),
    ];
    for (seconds, want) in cases {
        assert_eq!(event_at_seconds(&preview, seconds), want, "at {seconds} s");
    }
    let mut empty = (*preview).clone();
    empty.moves.clear();
    assert_eq!(event_at_seconds(&empty, 1.0), None);
}

#[test]
fn the_event_at_a_scrub_agrees_with_the_move_the_controller_draws_as_current() {
    let (mut h, doc) = compiled();
    let total = h.controller().preview_view(doc).unwrap().estimated_total_seconds;
    for step in 0..=20 {
        let seconds = total * f64::from(step) / 20.0;
        h.send(UiIntent::App(AppIntent::Preview(PreviewIntent::Seek { document: doc, seconds })));
        let view = h.controller().preview_view(doc).unwrap();
        let moves = &view.preview.moves;
        let current = view.partial.map_or(view.completed_moves, |p| p.index).min(moves.len() - 1);
        assert_eq!(event_at_seconds(&view.preview, seconds), Some(moves[current].event), "at {seconds:.3} s");
    }
}

#[test]
fn the_default_render_still_draws_cuts_and_the_bed() {
    let (mut h, doc) = compiled();
    let view = h.controller().preview_view(doc).unwrap();
    let frame = PreviewFrame::fit(view.bed_mm.unwrap(), [760, 456], 8);
    let image = render_preview(&view, &frame);
    assert_eq!((image.width, image.height), (760, 456));
    let some_ink = (0..456).any(|y| (0..760).any(|x| dist(rgb(&image, x, y), BED) > 100.0));
    assert!(some_ink);
}

/// Evidence for the dev note, not a check: with `BURNBURN_GUI_SNAPSHOT_DIR` set, a played, a partly
/// played, a picked and a dashed-travel scene is written as a binary PPM (`sips` makes a PNG).
#[test]
fn write_the_preview_scene_when_asked_to() {
    let Ok(dir) = std::env::var("BURNBURN_GUI_SNAPSHOT_DIR") else { return };
    let (mut h, doc) = compiled();
    let base = h.controller().preview_view(doc).unwrap();
    let op = first_op(&base);
    let moves = vec![
        mv(MoveKind::Travel, [0.0, 0.0], [200.0, 150.0], 0, 1.0, None),
        mv(MoveKind::Cut, [200.0, 150.0], [700.0, 150.0], 1, 2.0, Some(op)),
        mv(MoveKind::Cut, [700.0, 150.0], [1100.0, 600.0], 2, 2.0, Some(op)),
        mv(MoveKind::Travel, [1100.0, 600.0], [300.0, 700.0], 3, 1.0, None),
        mv(MoveKind::Cut, [300.0, 700.0], [900.0, 800.0], 4, 2.0, Some(op)),
        mv(MoveKind::Cut, [900.0, 800.0], [1400.0, 300.0], 5, 2.0, Some(op)),
    ];
    let mut view = synthetic(&base, moves, 2);
    view.partial = Some(burnburn_presentation::PartialMove { index: 2, fraction: 0.5 });
    view.head = [900.0, 375.0];
    view.selected =
        Some(burnburn_presentation::PreviewSelection { index: 5, kind: MoveKind::Cut, source: None, power: None });
    let frame = PreviewFrame::fit(view.bed_mm.unwrap(), [1520, 912], 16);
    let image = render_preview_styled(&view, &frame, &PreviewStyle { layers: vec![[0, 0, 255]], pixel_ratio: 2.0 });
    let mut out = format!("P6\n{} {}\n255\n", image.width, image.height).into_bytes();
    out.extend(image.rgba.as_chunks::<4>().0.iter().flat_map(|p| [p[0], p[1], p[2]]));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(std::path::Path::new(&dir).join("scene-played-picked.ppm"), out).unwrap();
}

/// LF-C follow-up (Q4 in the preview window): the preview's heading is the plan's compile moment
/// and duration, as in the inspector; the plan id is never the heading (it stays available as the
/// tooltip for support).
#[test]
fn the_preview_heading_names_the_compile_moment_not_the_plan_id() {
    let mut h = HeadlessEditorHarness::new();
    h.controller().attach_clock(Arc::new(|| burnburn_presentation::JobClock::new(1_781_751_845, 120).ok()));
    let doc = h.new_document().unwrap();
    h.send(UiIntent::SetMachineProfile {
        profile: Box::new(machine_profile()),
        ruida: burnburn_ruida::RuidaProfile {
            magic: burnburn_ruida::Magic::FIXTURE_0X88,
            overscan: burnburn_ruida::OverscanPolicy::ControllerManaged,
        },
    });
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (10.0, 10.0), (130.0, 90.0));
    let rect = h.document(doc).unwrap().objects[0].id;
    h.send(UiIntent::Select { document: doc, ids: vec![rect], mode: burnburn_presentation::SelectMode::Replace });
    h.send(UiIntent::AddOperation { document: doc, draft: cut() });
    h.invoke(doc, CommandKey::Compile);
    h.run_tasks();
    let view = h.controller().preview_view(doc).expect("a preview");
    let panel = preview_panel(&view, Locale::Nl);
    let id = view.plan.to_string();
    assert!(panel.plan_text.starts_with("Plan van "), "{}", panel.plan_text);
    assert!(!panel.plan_text.contains(&id[..8]), "no id in the heading: {}", panel.plan_text);
    assert_eq!(panel.plan_tooltip, id, "the id stays reachable");
}
