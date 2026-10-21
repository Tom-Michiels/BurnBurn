//! Acceptance plan `research/lightburn/05-acceptatieplan.md`, editing block: ED01-ED04.
//!
//! Everything goes through the public route: `UiIntent`s into the `HeadlessEditorHarness` (the
//! same controller the native hosts drive), then the saved project bytes or the scene. The
//! expectations are plain arithmetic on the squares drawn in each scenario.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test code

mod acc_rig;

use std::path::PathBuf;

use acc_rig::{bounds_of, fingerprint, opened_with, open_file, save_bytes, select, selected, square, view};
use burnburn_editor::Modifiers;
use burnburn_presentation::{CommandKey, FieldKey, HeadlessEditorHarness, RefPoint, UiIntent};
use burnburn_types::{DocumentId, GeometryId};
use proptest::prelude::*;

fn field(h: &HeadlessEditorHarness, doc: DocumentId, key: FieldKey) -> f64 {
    let text = view(h, doc).inspector.expect("inspector").field(key).expect("field").text.clone();
    text.replace(',', ".").parse().unwrap_or_else(|_| panic!("field {key:?} is not a number: {text}"))
}

fn commit(h: &mut HeadlessEditorHarness, doc: DocumentId, key: FieldKey, text: &str) {
    h.send(UiIntent::CommitField { document: doc, field: key, text: text.into() });
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

// ---------------------------------------------------------------------------------------------
// ED01 select, invert, group, nested group, locked objects
// ---------------------------------------------------------------------------------------------

/// Four 10 mm squares at x = 0, 30, 60, 90.
fn four() -> (HeadlessEditorHarness, DocumentId, Vec<GeometryId>) {
    opened_with(vec![square(0.0, 0.0, 10.0), square(30.0, 0.0, 10.0), square(60.0, 0.0, 10.0), square(90.0, 0.0, 10.0)])
}

#[test]
fn ed01_nested_group_is_picked_whole_and_its_bounds_and_move_follow_the_members() {
    let (mut h, doc, ids) = four();
    select(&mut h, doc, &ids[..2]);
    h.invoke(doc, CommandKey::Group);
    select(&mut h, doc, &[ids[0], ids[1], ids[2]]);
    h.invoke(doc, CommandKey::Group);
    let depths: Vec<usize> = view(&h, doc).objects.iter().map(|o| o.groups.len()).collect();
    assert_eq!(depths, vec![2, 2, 1, 0], "A and B sit in a group inside the group with C");

    // A click on the outermost member picks the whole outer group.
    select(&mut h, doc, &[]);
    h.click(doc, (5.0, 5.0), Modifiers::default());
    assert_eq!(selected(&h, doc), vec![ids[0], ids[1], ids[2]]);
    // The group is picked as one object whose bounds are the union: x 0..70, y 0..10.
    assert!(near(field(&h, doc, FieldKey::X), 0.0) && near(field(&h, doc, FieldKey::Width), 70.0));
    assert!(near(field(&h, doc, FieldKey::Y), 0.0) && near(field(&h, doc, FieldKey::Height), 10.0));

    // Moving the group by +5 mm in x moves every member and nothing else.
    commit(&mut h, doc, FieldKey::X, "5");
    assert_eq!(bounds_of(&h, doc, ids[0]).0, 5.0);
    assert_eq!(bounds_of(&h, doc, ids[2]).2, 75.0);
    assert_eq!(bounds_of(&h, doc, ids[3]), (90.0, 0.0, 100.0, 10.0), "the free square stays");
}

#[test]
fn ed01_invert_selection_skips_locked_objects_and_the_object_list_cannot_move_a_locked_object() {
    let (mut h, doc, ids) = four();
    select(&mut h, doc, &[ids[3]]);
    h.invoke(doc, CommandKey::LockSelected);
    select(&mut h, doc, &[]);
    h.invoke(doc, CommandKey::InvertSelection);
    assert_eq!(selected(&h, doc), vec![ids[0], ids[1], ids[2]], "invert of nothing = everything selectable");
    h.invoke(doc, CommandKey::SelectAll);
    assert_eq!(selected(&h, doc), vec![ids[0], ids[1], ids[2]]);

    // Selecting the locked object through the object list (to unlock it) must not let an edit
    // move, flip or delete it.
    let before = fingerprint(&h, doc);
    select(&mut h, doc, &[ids[3]]);
    commit(&mut h, doc, FieldKey::X, "500");
    h.invoke(doc, CommandKey::FlipHorizontal);
    h.invoke(doc, CommandKey::Delete);
    assert_eq!(fingerprint(&h, doc), before, "no lock-bypass through the inspector, arrange or delete");
}

#[test]
fn ed01_undo_of_a_selection_dependent_edit_restores_the_exact_bounds() {
    let (mut h, doc, ids) = four();
    select(&mut h, doc, &ids[..2]);
    h.invoke(doc, CommandKey::Group);
    let before = bounds_of(&h, doc, ids[1]);
    commit(&mut h, doc, FieldKey::Width, "40");
    assert!(near(field(&h, doc, FieldKey::Width), 40.0));
    h.invoke(doc, CommandKey::Undo);
    assert_eq!(bounds_of(&h, doc, ids[1]), before);
}

// ---------------------------------------------------------------------------------------------
// ED02 mm <-> in, locale comma/point, 9 anchors
// ---------------------------------------------------------------------------------------------

fn one_rect() -> (HeadlessEditorHarness, DocumentId, GeometryId) {
    let (mut h, doc, ids) = opened_with(vec![square(10.0, 20.0, 40.0)]);
    select(&mut h, doc, &ids);
    (h, doc, ids[0])
}

#[test]
fn ed02_comma_and_point_and_units_give_the_same_physical_width() {
    for text in ["25.4 mm", "25,4 mm", "1 in", "1,0 in", "1.0 in", "2.54 cm", "2,54cm"] {
        let (mut h, doc, id) = one_rect();
        commit(&mut h, doc, FieldKey::Width, text);
        let (x0, _, x1, _) = bounds_of(&h, doc, id);
        assert!(near(x1 - x0, 25.4), "{text:?} gave a width of {} mm", x1 - x0);
    }
}

#[test]
fn ed02_each_of_the_nine_anchors_stays_put_when_the_size_changes() {
    for anchor in RefPoint::ALL {
        let (mut h, doc, id) = one_rect(); // bounds x 10..50, y 20..60
        h.send(UiIntent::SetReferencePoint { document: doc, point: anchor });
        commit(&mut h, doc, FieldKey::Width, "20");
        commit(&mut h, doc, FieldKey::Height, "10");
        let (x0, y0, x1, y1) = bounds_of(&h, doc, id);
        assert!(near(x1 - x0, 20.0) && near(y1 - y0, 10.0), "{anchor:?}: size");
        // Expected position of the anchor point before: column 0/1/2 -> x 10/30/50, row 0 is the top (y 60).
        let ax = [10.0, 30.0, 50.0][anchor.column() as usize];
        let ay = [60.0, 40.0, 20.0][anchor.row() as usize];
        let got_x = [x0, (x0 + x1) / 2.0, x1][anchor.column() as usize];
        let got_y = [y1, (y0 + y1) / 2.0, y0][anchor.row() as usize];
        assert!(near(got_x, ax) && near(got_y, ay), "{anchor:?} moved: ({got_x}, {got_y}) vs ({ax}, {ay})");
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 48, failure_persistence: None, ..ProptestConfig::default() })]

    /// Any width in 0.5..500 mm typed as mm, as inches or with a decimal comma gives the same
    /// physical size, and the chosen anchor does not move.
    #[test]
    fn ed02_property_unit_locale_and_anchor_are_independent(
        width_um in 500_u32..500_000, anchor in 0_usize..9, unit in 0_usize..4, comma in any::<bool>(),
    ) {
        let width_mm = f64::from(width_um) / 1000.0;
        let (value, suffix) = match unit {
            0 => (width_mm, "mm"),
            1 => (width_mm / 10.0, "cm"),
            2 => (width_mm / 25.4, "in"),
            _ => (width_mm / 25.4, "\""),
        };
        let mut text = format!("{value:.9} {suffix}");
        if comma { text = text.replace('.', ","); }
        let (mut h, doc, id) = one_rect();
        let anchor = RefPoint::ALL[anchor];
        h.send(UiIntent::SetReferencePoint { document: doc, point: anchor });
        commit(&mut h, doc, FieldKey::Width, &text);
        let (x0, y0, x1, y1) = bounds_of(&h, doc, id);
        prop_assert!((x1 - x0 - width_mm).abs() < 1e-3, "{text:?}: width {} vs {width_mm}", x1 - x0);
        prop_assert!((y1 - y0 - 40.0).abs() < 1e-9, "height untouched");
        let ax = [10.0, 30.0, 50.0][anchor.column() as usize];
        let got_x = [x0, (x0 + x1) / 2.0, x1][anchor.column() as usize];
        prop_assert!((got_x - ax).abs() < 1e-3, "{anchor:?} anchor x moved {got_x} vs {ax}");
    }
}

// ---------------------------------------------------------------------------------------------
// ED03 100 operations, undo/redo, save/open
// ---------------------------------------------------------------------------------------------

/// One user action, chosen by a code; selection is derived from the code too. Every action is a
/// real command key or inspector commit; ones that do nothing in a state simply make no entry.
fn apply_op(h: &mut HeadlessEditorHarness, doc: DocumentId, code: u32) {
    let ids: Vec<GeometryId> = view(h, doc).objects.iter().map(|o| o.id).collect();
    if ids.is_empty() {
        h.invoke(doc, CommandKey::Paste);
        return;
    }
    let pick = |n: u32| ids[(n as usize) % ids.len()];
    let a = pick(code >> 8);
    let b = pick(code >> 16);
    select(h, doc, &[a, b]);
    match code % 14 {
        0 => commit(h, doc, FieldKey::X, &format!("{}", (code >> 4) % 200)),
        1 => commit(h, doc, FieldKey::Width, &format!("{}", 1 + (code >> 4) % 60)),
        2 => drop(h.invoke(doc, CommandKey::Group)),
        3 => drop(h.invoke(doc, CommandKey::Ungroup)),
        4 => drop(h.invoke(doc, CommandKey::FlipHorizontal)),
        5 => drop(h.invoke(doc, CommandKey::Rotate90Cw)),
        6 => drop(h.invoke(doc, CommandKey::Duplicate)),
        7 => {
            h.invoke(doc, CommandKey::Copy);
            drop(h.invoke(doc, CommandKey::PasteInPlace));
        }
        8 => drop(h.invoke(doc, CommandKey::LockSelected)),
        9 => drop(h.invoke(doc, CommandKey::UnlockAll)),
        10 => drop(h.invoke(doc, CommandKey::HideSelected)),
        11 => drop(h.invoke(doc, CommandKey::ShowAll)),
        12 => drop(h.invoke(doc, CommandKey::PushToFront)),
        _ => {
            // Keep the document from emptying out: delete only when more than three objects remain.
            if ids.len() > 3 {
                h.invoke(doc, CommandKey::Delete);
            }
        }
    }
}

fn undo_all(h: &mut HeadlessEditorHarness, doc: DocumentId) -> usize {
    let mut n = 0;
    while view(h, doc).undo.is_some() {
        h.invoke(doc, CommandKey::Undo);
        n += 1;
        assert!(n < 1000, "undo does not terminate");
    }
    n
}

fn redo_all(h: &mut HeadlessEditorHarness, doc: DocumentId) {
    let mut n = 0;
    while view(h, doc).redo.is_some() {
        h.invoke(doc, CommandKey::Redo);
        n += 1;
        assert!(n < 1000, "redo does not terminate");
    }
}

fn hundred_ops(codes: &[u32]) {
    let (mut h, doc, _) = opened_with(vec![
        square(0.0, 0.0, 10.0),
        square(30.0, 0.0, 12.0),
        square(60.0, 5.0, 8.0),
        square(0.0, 40.0, 15.0),
        square(30.0, 40.0, 10.0),
    ]);
    let start = fingerprint(&h, doc);
    for &c in codes {
        apply_op(&mut h, doc, c);
    }
    let end = fingerprint(&h, doc);
    let path = PathBuf::from("/work/ops.burnburn");
    let saved = save_bytes(&mut h, doc, &path);

    // Undo everything: the document is what it was; redo everything: it is what it ended as.
    let steps = undo_all(&mut h, doc);
    assert!(steps >= 25, "the script must really edit the document (only {steps} undo steps)");
    assert_ne!(start, end, "the script changes the document");
    assert_eq!(fingerprint(&h, doc), start, "after {steps} undo steps");
    redo_all(&mut h, doc);
    assert_eq!(fingerprint(&h, doc), end, "after redoing {steps} steps");
    let saved_again = save_bytes(&mut h, doc, &path);
    assert_eq!(saved, saved_again, "the project bytes after undo+redo equal the bytes before");

    // Open in a fresh app: structure (objects, outlines, group depth, flags) is identical, and a
    // second save is byte-identical to the first.
    let mut h2 = HeadlessEditorHarness::with_fs(h.shared_fs());
    let doc2 = open_file(&mut h2, &path);
    assert_eq!(fingerprint(&h2, doc2), end, "relation graph and content survive save + open");
    let resaved = save_bytes(&mut h2, doc2, &PathBuf::from("/work/ops2.burnburn"));
    assert_eq!(saved, resaved, "open + save is a fixed point");
}

#[test]
fn ed03_a_hundred_fixed_operations_roundtrip_through_undo_redo_save_open() {
    // A fixed pseudo-random script (LCG) so the failure, if any, reproduces exactly.
    let mut x: u32 = 0x2545_f491;
    let codes: Vec<u32> = (0..100)
        .map(|_| {
            x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            x >> 4
        })
        .collect();
    hundred_ops(&codes);
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 12, failure_persistence: None, ..ProptestConfig::default() })]

    #[test]
    fn ed03_property_any_hundred_operations_roundtrip(codes in proptest::collection::vec(0_u32..u32::MAX, 100)) {
        hundred_ops(&codes);
    }
}

// ---------------------------------------------------------------------------------------------
// ED04 duplicate vs paste vs paste-in-place
// ---------------------------------------------------------------------------------------------

#[test]
fn ed04_duplicate_paste_in_place_and_paste_each_have_their_own_ids_and_placement() {
    let (mut h, doc, ids) = opened_with(vec![square(100.0, 100.0, 10.0), square(130.0, 100.0, 10.0)]);
    select(&mut h, doc, &[ids[0]]);
    // Duplicate: same position, new identity, clipboard untouched.
    let up = h.invoke(doc, CommandKey::Duplicate);
    assert!(
        !up.effects.iter().any(|e| matches!(e, burnburn_presentation::HostEffect::SetClipboard { .. })),
        "duplicate leaves the clipboard alone"
    );
    let vm = view(&h, doc);
    assert_eq!(vm.objects.len(), 3);
    let dup = vm.selection[0];
    assert!(!ids.contains(&dup));
    assert_eq!(bounds_of(&h, doc, dup), bounds_of(&h, doc, ids[0]), "same place");

    // Copy then paste in place: another new identity at the original place.
    select(&mut h, doc, &[ids[1]]);
    h.invoke(doc, CommandKey::Copy);
    h.invoke(doc, CommandKey::PasteInPlace);
    let pip = view(&h, doc).selection[0];
    assert!(!ids.contains(&pip) && pip != dup);
    assert_eq!(bounds_of(&h, doc, pip), (130.0, 100.0, 140.0, 110.0));

    // Paste (not in place): new identity, placed at the view centre, same size, clipboard intact.
    h.invoke(doc, CommandKey::Paste);
    let vm = view(&h, doc);
    let pasted = vm.selection[0];
    assert!(![ids[0], ids[1], dup, pip].contains(&pasted));
    let c = vm.camera.points_to_world(&vm.viewport, [vm.viewport.width_pt() / 2.0, vm.viewport.height_pt() / 2.0]);
    let (x0, y0, x1, y1) = bounds_of(&h, doc, pasted);
    assert!(near(x1 - x0, 10.0) && near(y1 - y0, 10.0));
    assert!(near((x0 + x1) / 2.0, c.x) && near((y0 + y1) / 2.0, c.y), "centred on the view");
    // Five objects, all identities distinct.
    let mut all: Vec<_> = vm.objects.iter().map(|o| o.id).collect();
    all.sort();
    all.dedup();
    assert_eq!(all.len(), 5);
    // Each of the three was one undo entry.
    for expected in [5, 4, 3, 2] {
        assert_eq!(view(&h, doc).objects.len(), expected);
        h.invoke(doc, CommandKey::Undo);
    }
}
