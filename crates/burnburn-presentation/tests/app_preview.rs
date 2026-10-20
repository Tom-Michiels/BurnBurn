//! C07 / IM15 (W3-UI-app): the preview window's model over the compiled plan: play, scrub, pick a
//! move to select its source and operation, travel on and off, and the read-only RD inspector.
//! The clock is the plan's estimate; the preview is a view of exactly one plan.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

mod common;

use burnburn_job::MoveKind;
use burnburn_presentation::{
    AppIntent, CommandKey, HeadlessEditorHarness, PlaybackSpeed, PreviewIntent, PreviewViewModel, UiIntent,
};
use burnburn_types::{DocumentId, StableId};

/// A 120 × 80 rectangle with one cut operation, compiled for the 1500 × 900 research bed.
fn compiled() -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().expect("window");
    common::design(&mut h, doc);
    h.invoke(doc, CommandKey::Compile);
    h.run_tasks();
    (h, doc)
}

fn preview(h: &mut HeadlessEditorHarness, doc: DocumentId) -> PreviewViewModel {
    h.controller().preview_view(doc).expect("a compiled plan has a preview")
}

fn send(h: &mut HeadlessEditorHarness, intent: PreviewIntent) {
    h.send(UiIntent::App(AppIntent::Preview(intent)));
}

fn midpoint(m: &burnburn_job::PreviewMove) -> [f64; 2] {
    [(m.from.x_mm() + m.to.x_mm()) / 2.0, (m.from.y_mm() + m.to.y_mm()) / 2.0]
}

#[test]
fn clicking_plan_move_selects_source_and_operation() {
    let (mut h, doc) = compiled();
    let view = preview(&mut h, doc);
    let rect = h.document(doc).unwrap().objects[0].id;
    let operation = h.document(doc).unwrap().operations[0].id;
    h.send(UiIntent::Select { document: doc, ids: vec![], mode: burnburn_presentation::SelectMode::Replace });
    assert!(h.document(doc).unwrap().selection.is_empty());

    let burning = view.preview.moves.iter().position(|m| m.kind == MoveKind::Cut).expect("a cut move");
    let at = midpoint(&view.preview.moves[burning]);
    send(&mut h, PreviewIntent::Pick { document: doc, at_mm: at, tolerance_mm: 0.5 });

    let picked = preview(&mut h, doc).selected.expect("the click picked a move");
    assert_eq!(picked.index, burning);
    let source = picked.source.expect("a cut move knows its source");
    assert_eq!((source.geometry, source.operation), (rect, operation));
    assert_eq!(h.document(doc).unwrap().selection, vec![rect], "the source shape is selected in the document");
    assert_eq!(preview(&mut h, doc).selected_operation, Some(operation), "and its operation is the picked one");
}

#[test]
fn a_pick_chooses_the_nearest_move_within_the_radius_and_a_miss_picks_nothing() {
    let (mut h, doc) = compiled();
    let view = preview(&mut h, doc);
    let cut = view.preview.moves.iter().position(|m| m.kind == MoveKind::Cut).unwrap();
    let [x, y] = midpoint(&view.preview.moves[cut]);
    // 0.4 mm off the line, inside a 0.5 mm radius; the picked move is the one this point is on.
    let m = view.preview.moves[cut];
    let (dx, dy) = (m.to.x_mm() - m.from.x_mm(), m.to.y_mm() - m.from.y_mm());
    let norm = dx.hypot(dy);
    let off = [x - dy / norm * 0.4, y + dx / norm * 0.4];
    send(&mut h, PreviewIntent::Pick { document: doc, at_mm: off, tolerance_mm: 0.5 });
    assert_eq!(preview(&mut h, doc).selected.map(|s| s.index), Some(cut));
    // The same click with a radius smaller than its distance misses; the pick is cleared.
    send(&mut h, PreviewIntent::Pick { document: doc, at_mm: off, tolerance_mm: 0.3 });
    assert_eq!(preview(&mut h, doc).selected, None);
    // Far from everything.
    send(&mut h, PreviewIntent::Pick { document: doc, at_mm: [1400.0, 850.0], tolerance_mm: 1.0 });
    assert_eq!(preview(&mut h, doc).selected, None);
    // Garbage input changes nothing.
    send(&mut h, PreviewIntent::Pick { document: doc, at_mm: [f64::NAN, 0.0], tolerance_mm: 1.0 });
    send(&mut h, PreviewIntent::Pick { document: doc, at_mm: [x, y], tolerance_mm: -1.0 });
    assert_eq!(preview(&mut h, doc).selected, None);
}

#[test]
fn travel_can_be_picked_but_has_no_source_and_hidden_travel_cannot_be_picked() {
    let (mut h, doc) = compiled();
    let view = preview(&mut h, doc);
    let travel = view.preview.moves.iter().position(|m| m.kind == MoveKind::Travel).expect("a travel move");
    let m = view.preview.moves[travel];
    assert!(m.from.distance(m.to) > 1.0, "the rig travels from the origin to the shape");
    let at = midpoint(&m);
    // Selection of the document stays as it is for a move without source.
    let before = h.document(doc).unwrap().selection;
    send(&mut h, PreviewIntent::Pick { document: doc, at_mm: at, tolerance_mm: 0.5 });
    let picked = preview(&mut h, doc).selected.expect("travel is picked while it is shown");
    assert_eq!((picked.index, picked.kind, picked.source), (travel, MoveKind::Travel, None));
    assert_eq!(h.document(doc).unwrap().selection, before);
    assert_eq!(preview(&mut h, doc).selected_operation, None);

    send(&mut h, PreviewIntent::ShowTravel { document: doc, show: false });
    assert!(!preview(&mut h, doc).show_travel);
    send(&mut h, PreviewIntent::Pick { document: doc, at_mm: at, tolerance_mm: 0.5 });
    assert_eq!(preview(&mut h, doc).selected, None, "what is not drawn cannot be clicked");
}

#[test]
fn scrubbing_puts_the_head_on_the_plan_at_that_estimated_time() {
    let (mut h, doc) = compiled();
    let view = preview(&mut h, doc);
    let moves = &view.preview.moves;
    assert!((view.estimated_total_seconds - view.preview.estimated_seconds).abs() < 1e-12, "one clock: the plan's");
    assert_eq!(view.estimated_position_seconds, 0.0);
    assert_eq!((view.completed_moves, view.partial), (0, None));
    assert_eq!(view.head, [moves[0].from.x_mm(), moves[0].from.y_mm()], "the head starts where the plan starts");

    // Halfway through the second move (independent arithmetic on the plan's own move times).
    let t = moves[0].seconds + moves[1].seconds / 2.0;
    send(&mut h, PreviewIntent::Seek { document: doc, seconds: t });
    let view = preview(&mut h, doc);
    assert_eq!(view.completed_moves, 1);
    let partial = view.partial.expect("inside the second move");
    assert_eq!(partial.index, 1);
    assert!((partial.fraction - 0.5).abs() < 1e-9, "{partial:?}");
    let [mx, my] = midpoint(&moves[1]);
    assert!((view.head[0] - mx).abs() < 1e-9 && (view.head[1] - my).abs() < 1e-9, "{:?}", view.head);

    // The end, and values outside the plan are clamped.
    send(&mut h, PreviewIntent::Seek { document: doc, seconds: 1e9 });
    let view = preview(&mut h, doc);
    assert_eq!(view.completed_moves, moves.len());
    assert_eq!(view.partial, None);
    let last = moves.last().unwrap();
    assert_eq!(view.head, [last.to.x_mm(), last.to.y_mm()]);
    assert_eq!(view.estimated_position_seconds, view.estimated_total_seconds);
    send(&mut h, PreviewIntent::Seek { document: doc, seconds: -5.0 });
    assert_eq!(preview(&mut h, doc).estimated_position_seconds, 0.0);
    send(&mut h, PreviewIntent::Seek { document: doc, seconds: 1.0 });
    send(&mut h, PreviewIntent::Seek { document: doc, seconds: f64::NAN });
    assert_eq!(preview(&mut h, doc).estimated_position_seconds, 1.0, "not-a-number is ignored");
}

#[test]
fn playing_advances_by_the_timer_ticks_at_the_chosen_speed_and_stops_at_the_end() {
    let (mut h, doc) = compiled();
    let total = preview(&mut h, doc).estimated_total_seconds;
    send(&mut h, PreviewIntent::Tick { document: doc, seconds: 0.2 });
    assert_eq!(preview(&mut h, doc).estimated_position_seconds, 0.0, "paused: ticks do nothing");

    send(&mut h, PreviewIntent::SetSpeed { document: doc, speed: PlaybackSpeed::X5 });
    send(&mut h, PreviewIntent::Play { document: doc, playing: true });
    send(&mut h, PreviewIntent::Tick { document: doc, seconds: 0.1 });
    let view = preview(&mut h, doc);
    assert!(view.playing);
    assert!((view.estimated_position_seconds - 0.5).abs() < 1e-12, "0.1 s × 5");

    send(&mut h, PreviewIntent::Tick { document: doc, seconds: total });
    let view = preview(&mut h, doc);
    assert_eq!(view.estimated_position_seconds, total, "never beyond the end");
    assert!(!view.playing, "the end stops the playback");

    // Play at the end starts over.
    send(&mut h, PreviewIntent::Play { document: doc, playing: true });
    let view = preview(&mut h, doc);
    assert!(view.playing);
    assert_eq!(view.estimated_position_seconds, 0.0);
    // Scrubbing while playing keeps playing from the new place.
    send(&mut h, PreviewIntent::Seek { document: doc, seconds: 1.0 });
    assert!(preview(&mut h, doc).playing);
    send(&mut h, PreviewIntent::Play { document: doc, playing: false });
    assert!(!preview(&mut h, doc).playing);
}

#[test]
fn an_edit_makes_the_preview_stale_but_it_stays_the_old_plan_until_a_new_compile_replaces_it() {
    let (mut h, doc) = compiled();
    let first = preview(&mut h, doc);
    assert!(!first.stale);
    assert_eq!(h.document(doc).unwrap().plan.expect("plan view").id, first.plan, "preview and plan panel: one plan");
    send(&mut h, PreviewIntent::Seek { document: doc, seconds: 0.5 });
    send(&mut h, PreviewIntent::SelectMove { document: doc, index: 2 });

    h.invoke(doc, CommandKey::SelectAll);
    h.send(UiIntent::App(AppIntent::Nudge {
        document: doc,
        direction: burnburn_presentation::NudgeDirection::Right,
        step: burnburn_presentation::NudgeStep::Large,
    }));
    let stale = preview(&mut h, doc);
    assert!(stale.stale);
    assert_eq!(stale.plan, first.plan, "an edit never rewrites the plan the preview shows");
    assert_eq!(stale.estimated_position_seconds, 0.5, "the clock position is kept while only the plan is stale");

    h.invoke(doc, CommandKey::Compile);
    h.run_tasks();
    let fresh = preview(&mut h, doc);
    assert!(!fresh.stale);
    assert_ne!(fresh.plan, first.plan);
    assert_eq!(fresh.estimated_position_seconds, 0.0, "a new plan starts at the beginning");
    assert_eq!(fresh.selected, None, "and its pick is gone");
}

#[test]
fn no_plan_no_preview_and_an_unknown_window_has_none() {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().expect("window");
    assert_eq!(h.controller().preview_view(doc), None);
    send(&mut h, PreviewIntent::Play { document: doc, playing: true });
    assert_eq!(h.controller().preview_view(doc), None);
    assert_eq!(h.controller().preview_view(DocumentId::from_raw(99)), None);
    assert_eq!(h.controller().rd_inspect_view(doc), None);
}

#[test]
fn the_rd_inspector_lists_the_records_the_export_would_contain_and_nothing_can_be_edited() {
    let (mut h, doc) = compiled();
    assert_eq!(h.controller().rd_inspect_view(doc), None, "nothing is decoded until it is asked for");
    send(&mut h, PreviewIntent::InspectRd { document: doc });
    let view = h.controller().rd_inspect_view(doc).expect("inspected");
    assert!(view.current && view.error.is_none(), "{:?}", view.error);
    assert_eq!(view.plan, preview(&mut h, doc).plan, "same plan as the preview");

    // Independent oracle: export the same plan to a file and decode that file with the codec.
    h.fs().create_dir_all_for_test(std::path::Path::new("/out"));
    h.send(UiIntent::ExportRd { document: doc, path: std::path::PathBuf::from("/out/job.rd") });
    h.run_tasks();
    let bytes =
        burnburn_io::fs::FileSystem::read(h.fs(), std::path::Path::new("/out/job.rd"), u64::MAX).expect("export");
    let decoded = burnburn_ruida::decode_rd_file(&bytes, common::RUIDA.magic).expect("valid rd");
    assert_eq!(view.bytes, bytes.len());
    assert_eq!(view.records.len() + view.truncated, decoded.records.len());
    for (row, record) in view.records.iter().zip(&decoded.records) {
        assert_eq!(row.offset, record.offset);
        assert_eq!(row.hex.split(' ').count(), record.len, "one hex pair per byte");
        assert_eq!(row.decoded, format!("{:?}", record.command));
    }
    assert!(view.records.iter().any(|r| r.label.is_some()), "known opcodes carry their registry name");
    assert!(view.records.iter().all(|r| !r.hex.is_empty() && !r.decoded.is_empty()));
}

#[test]
fn the_rd_inspector_keeps_what_it_decoded_and_says_when_it_is_not_the_current_plan_any_more() {
    let (mut h, doc) = compiled();
    send(&mut h, PreviewIntent::InspectRd { document: doc });
    let before = h.controller().rd_inspect_view(doc).unwrap();
    h.invoke(doc, CommandKey::SelectAll);
    h.invoke(doc, CommandKey::FlipHorizontal);
    h.invoke(doc, CommandKey::Compile);
    h.run_tasks();
    let after = h.controller().rd_inspect_view(doc).unwrap();
    assert_eq!(after.plan, before.plan, "an older listing is not silently replaced");
    assert!(!after.current, "and it says it is of an older plan");
    send(&mut h, PreviewIntent::InspectRd { document: doc });
    let again = h.controller().rd_inspect_view(doc).unwrap();
    assert!(again.current);
    assert_ne!(again.plan, before.plan);
}
