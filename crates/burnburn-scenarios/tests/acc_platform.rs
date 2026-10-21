//! Acceptance plan PL01: translations, display scale (DPI), hotkeys and file dialogs. The headless
//! half: command identifiers stay what they were, and neither language, unit preference, decimal
//! separator nor window scale reaches the physical output. The native half (real OS dialogs, key
//! events, real DPI) belongs to the macOS/Windows native tests and is listed as such in
//! `docs/user-flows/acceptatie.md`.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test code

mod common;

use std::collections::BTreeSet;
use std::path::PathBuf;

use burnburn_document::process::{Kerf, OperationDraft, OperationMode, Passes, PowerPercent, PowerRange, ProcessSettings, Speed};
use burnburn_editor::{Camera, Viewport};
use burnburn_presentation::{
    AppIntent, CameraIntent, CommandKey, FieldKey, HeadlessEditorHarness, PrefKey, SelectMode, UiIntent,
};
use burnburn_test_support::fixtures::read_fixture;
use burnburn_types::DocumentId;

#[test]
fn pl01_every_command_and_shortcut_id_of_golf_3_still_exists() {
    let golden = String::from_utf8(read_fixture("acceptance", "command-ids.txt").unwrap()).unwrap();
    let mut h = HeadlessEditorHarness::new();
    let mut now: BTreeSet<String> = CommandKey::ALL.iter().map(|k| format!("command {k:?}")).collect();
    now.extend(h.controller().shortcuts_view().rows.iter().map(|r| format!("shortcut {}", r.id)));
    let missing: Vec<&str> = golden.lines().filter(|l| !now.contains(*l)).collect();
    assert!(missing.is_empty(), "ids were renamed or removed (stored settings and scripts name them): {missing:?}");
    assert!(golden.lines().count() > 300, "the golden list is the whole list");
}

#[test]
fn pl01_command_ids_do_not_depend_on_the_language() {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    let ids = |h: &HeadlessEditorHarness| -> Vec<CommandKey> { h.view().commands.keys().copied().collect() };
    let system = ids(&h);
    for lang in ["nl", "en", "system"] {
        h.send(UiIntent::App(AppIntent::SetPreference { key: PrefKey::Language, text: lang.into() }));
        assert_eq!(ids(&h), system, "language {lang}");
        let rows: Vec<String> = h.controller().shortcuts_view().rows.iter().map(|r| r.id.clone()).collect();
        assert!(rows.iter().any(|r| r.contains("edit") || !r.is_empty()), "shortcuts exist in {lang}");
    }
    let _ = doc;
}

fn cut() -> OperationDraft {
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

/// A 12.5 x 8.25 mm rectangle at (20, 30), typed through the inspector in the given notation, cut and
/// exported: the RD bytes.
fn rd_with(language: &str, units: &str, decimal_comma: bool, scale: f64) -> Vec<u8> {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.send(UiIntent::SetMachineProfile { profile: Box::new(common::profile()), ruida: common::RUIDA });
    h.send(UiIntent::App(AppIntent::SetPreference { key: PrefKey::Language, text: language.into() }));
    h.send(UiIntent::App(AppIntent::SetPreference { key: PrefKey::Units, text: units.into() }));
    // The window's backing scale (Retina / Windows DPI) changes the viewport, never the document.
    h.send(UiIntent::Camera {
        document: doc,
        intent: CameraIntent::Resized { viewport: Viewport::new(1200.0, 800.0, scale).unwrap() },
    });
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (20.0, 30.0), (60.0, 60.0));
    h.invoke(doc, CommandKey::ToolSelect);
    let id = h.document(doc).unwrap().objects[0].id;
    h.send(UiIntent::Select { document: doc, ids: vec![id], mode: SelectMode::Replace });
    // Values are typed with an explicit unit so that the display unit cannot change their meaning.
    let n = |mm: f64| {
        let t = format!("{mm} mm");
        if decimal_comma { t.replace('.', ",") } else { t }
    };
    h.send(UiIntent::CommitField { document: doc, field: FieldKey::Width, text: n(12.5) });
    h.send(UiIntent::CommitField { document: doc, field: FieldKey::Height, text: n(8.25) });
    h.send(UiIntent::CommitField { document: doc, field: FieldKey::X, text: n(20.0) });
    h.send(UiIntent::CommitField { document: doc, field: FieldKey::Y, text: n(30.0) });
    h.send(UiIntent::AddOperation { document: doc, draft: cut() });
    h.invoke(doc, CommandKey::Compile);
    h.run_tasks();
    h.fs().create_dir_all_for_test(std::path::Path::new("/out"));
    let path = PathBuf::from("/out/pl.rd");
    h.send(UiIntent::ExportRd { document: doc, path: path.clone() });
    h.run_tasks();
    burnburn_io::fs::FileSystem::read(h.fs(), &path, u64::MAX).expect("exported RD")
}

#[test]
fn pl01_language_units_decimal_separator_and_window_scale_do_not_change_a_single_output_byte() {
    let reference = rd_with("en", "mm", false, 1.0);
    assert!(reference.len() > 100);
    // The cut itself is what was typed: 12.5 x 8.25 mm at (20, 30).
    let rd = common::decode(&reference);
    let (lo, hi) = common::cut_extent(&common::cuts(&rd));
    assert!((lo.x - 20.0).abs() < 0.002 && (lo.y - 30.0).abs() < 0.002);
    assert!((hi.x - 32.5).abs() < 0.002 && (hi.y - 38.25).abs() < 0.002, "{hi:?}");
    for (language, units, comma, scale) in [
        ("nl", "mm", true, 1.0),
        ("nl", "inch", true, 2.0),
        ("en", "inch", false, 1.5),
        ("system", "mm", false, 3.0),
        ("nl", "mm", false, 1.25),
    ] {
        assert_eq!(
            rd_with(language, units, comma, scale),
            reference,
            "language {language}, units {units}, decimal comma {comma}, scale {scale}"
        );
    }
}

#[test]
fn pl01_the_canvas_camera_follows_the_window_scale_but_not_the_document() {
    let mut h = HeadlessEditorHarness::new();
    let doc: DocumentId = h.new_document().unwrap();
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (10.0, 10.0), (50.0, 40.0));
    let before = h.document(doc).unwrap();
    let mut points_per_mm: Option<f64> = None;
    for scale in [1.0, 2.0, 3.0] {
        h.send(UiIntent::Camera {
            document: doc,
            intent: CameraIntent::Resized { viewport: Viewport::new(1000.0, 700.0, scale).unwrap() },
        });
        let vm = h.document(doc).unwrap();
        assert_eq!((vm.revision, vm.undo), (before.revision, before.undo), "scale {scale}: no document change");
        assert_eq!(vm.viewport.size_px(), [(1000.0 * scale) as u32, (700.0 * scale) as u32]);
        let cam: Camera = vm.camera;
        // One millimetre is the same number of points whatever the backing scale.
        let p = cam.world_to_points(&vm.viewport, burnburn_geometry::Point::new(10.0, 10.0));
        let q = cam.world_to_points(&vm.viewport, burnburn_geometry::Point::new(11.0, 10.0));
        let per_mm = q[0] - p[0];
        assert!(per_mm > 0.0);
        let first = *points_per_mm.get_or_insert(per_mm);
        assert!((per_mm - first).abs() < 1e-9, "scale {scale}: {per_mm} pt/mm vs {first}");
    }
}
