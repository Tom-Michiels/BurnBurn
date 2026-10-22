//! G7-werk, W07: the remnant library outlives the session. A remnant has material, thickness, contour
//! and exclusion zones; saving takes the selection (the largest closed outline is the sheet, every
//! other closed outline a zone), placing draws it in a window so a nest can use the contour as the
//! sheet, and an order can name the remnant it is made from, which changes no count.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test helpers

mod common;
mod edit_rig;

use std::path::{Path as FsPath, PathBuf};
use std::sync::Arc;

use burnburn_io::fs::FileSystem;
use burnburn_io::memory::MemoryFileSystem;
use burnburn_presentation::{
    CommandKey, FieldKey, HeadlessEditorHarness, OrderDraft, RemnantIntent, RemnantLinkStatus, RemnantProblem,
    RemnantsViewModel, SelectMode, UiIntent,
};
use burnburn_types::{DocumentId, GeometryId, StableId};
use common::{Rig, rig};

const FILE: &str = "/lib/remnants.json";

/// One run of the app on the shared disk.
fn start(fs: &Arc<MemoryFileSystem>) -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::with_fs(Arc::clone(fs));
    h.fs().create_dir_all_for_test(FsPath::new("/lib"));
    h.controller().set_library_dir(Some(PathBuf::from("/lib")));
    let doc = h.new_document().unwrap();
    h.run_tasks();
    (h, doc)
}

fn rectangle(h: &mut HeadlessEditorHarness, doc: DocumentId, from: (f64, f64), to: (f64, f64)) -> GeometryId {
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, from, to);
    h.invoke(doc, CommandKey::ToolSelect);
    h.document(doc).unwrap().objects.last().unwrap().id
}

fn select(h: &mut HeadlessEditorHarness, doc: DocumentId, ids: &[GeometryId]) {
    h.send(UiIntent::Select { document: doc, ids: ids.to_vec(), mode: SelectMode::Replace });
}

fn save(h: &mut HeadlessEditorHarness, doc: DocumentId, name: &str, material: &str, thickness: &str) {
    h.send(UiIntent::Remnant(RemnantIntent::SaveFromSelection {
        document: doc,
        name: name.into(),
        material: material.into(),
        thickness: thickness.into(),
        note: "rand verbrand".into(),
        date: "2026-10-22".into(),
    }));
    h.run_tasks();
}

fn remnants(h: &HeadlessEditorHarness, doc: DocumentId) -> RemnantsViewModel {
    h.document(doc).unwrap().remnants
}

/// A 200 x 100 sheet with a 20 x 20 zone inside, selected.
fn offcut(h: &mut HeadlessEditorHarness, doc: DocumentId) -> Vec<GeometryId> {
    let sheet = rectangle(h, doc, (10.0, 10.0), (210.0, 110.0));
    let zone = rectangle(h, doc, (30.0, 30.0), (50.0, 50.0));
    let both = vec![sheet, zone];
    select(h, doc, &both);
    both
}

#[test]
fn the_selection_becomes_a_remnant_the_largest_outline_is_the_sheet_and_the_rest_are_zones() {
    let fs = Arc::new(MemoryFileSystem::new());
    let (mut h, doc) = start(&fs);
    offcut(&mut h, doc);
    save(&mut h, doc, "Restplaat 1", "multiplex", "3");
    let v = remnants(&h, doc);
    assert_eq!(v.problem, None);
    assert_eq!(v.rows.len(), 1);
    let r = &v.rows[0];
    assert_eq!((r.name.as_str(), r.material.as_str(), r.thickness_mm), ("Restplaat 1", "multiplex", 3.0));
    assert!((r.width_mm - 200.0).abs() < 1e-6 && (r.height_mm - 100.0).abs() < 1e-6, "{r:?}");
    assert_eq!(r.zones, 1);
    assert!((r.usable_cm2 - 196.0).abs() < 1e-6, "200 x 100 mm minus a 20 x 20 mm zone: {}", r.usable_cm2);
    assert_eq!((r.note.as_str(), r.date.as_str()), ("rand verbrand", "2026-10-22"));
    assert!(fs.exists(FsPath::new(FILE)).unwrap(), "kept in the library folder");
}

#[test]
fn a_remnant_is_still_there_after_a_restart_and_a_removed_one_stays_removed_with_its_id_spent() {
    let fs = Arc::new(MemoryFileSystem::new());
    let (mut first, doc) = start(&fs);
    offcut(&mut first, doc);
    save(&mut first, doc, "A", "multiplex", "3");
    save(&mut first, doc, "B", "acryl", "5");
    let rows = remnants(&first, doc).rows;
    assert_eq!(rows.len(), 2);
    let (id_a, id_b) = (rows[0].id, rows[1].id);
    first.send(UiIntent::Remnant(RemnantIntent::Remove { id: id_b }));
    first.run_tasks();
    drop(first);

    let (mut second, doc2) = start(&fs);
    let rows = remnants(&second, doc2).rows;
    assert_eq!(rows.iter().map(|r| (r.id, r.name.as_str())).collect::<Vec<_>>(), vec![(id_a, "A")]);
    offcut(&mut second, doc2);
    save(&mut second, doc2, "C", "multiplex", "4");
    let ids: Vec<u64> = remnants(&second, doc2).rows.iter().map(|r| r.id).collect();
    assert!(!ids.contains(&id_b), "the id of a removed remnant is not handed out again: {ids:?}");
}

#[test]
fn what_cannot_be_kept_is_said_and_nothing_is_stored() {
    let fs = Arc::new(MemoryFileSystem::new());
    let (mut h, doc) = start(&fs);
    let problem = |h: &HeadlessEditorHarness| remnants(h, doc).problem;

    save(&mut h, doc, "Rest", "multiplex", "3");
    assert_eq!(problem(&h), Some(RemnantProblem::NeedsSelection));

    let ids = offcut(&mut h, doc);
    save(&mut h, doc, "  ", "multiplex", "3");
    assert_eq!(problem(&h), Some(RemnantProblem::NeedsName));
    save(&mut h, doc, "Rest", "", "3");
    assert_eq!(problem(&h), Some(RemnantProblem::NeedsMaterial));
    for bad in ["0", "-2", "dik", "500"] {
        save(&mut h, doc, "Rest", "multiplex", bad);
        assert_eq!(problem(&h), Some(RemnantProblem::InvalidThickness), "thickness {bad:?}");
    }
    h.send(UiIntent::Remnant(RemnantIntent::SaveFromSelection {
        document: doc,
        name: "Rest".into(),
        material: "multiplex".into(),
        thickness: "3".into(),
        note: String::new(),
        date: "22-10-2026".into(),
    }));
    assert_eq!(problem(&h), Some(RemnantProblem::InvalidDate));

    // A zone that does not lie on the sheet.
    let far = rectangle(&mut h, doc, (500.0, 500.0), (520.0, 520.0));
    select(&mut h, doc, &[ids[0], far]);
    save(&mut h, doc, "Rest", "multiplex", "3");
    assert_eq!(problem(&h), Some(RemnantProblem::ZoneOutsideSheet));

    // An open line is no outline.
    h.invoke(doc, CommandKey::SelectAll);
    h.invoke(doc, CommandKey::Delete);
    h.invoke(doc, CommandKey::ToolLine);
    h.drag(doc, (0.0, 0.0), (50.0, 50.0));
    h.invoke(doc, CommandKey::ToolSelect);
    let line = h.document(doc).unwrap().objects.last().unwrap().id;
    select(&mut h, doc, &[line]);
    save(&mut h, doc, "Rest", "multiplex", "3");
    assert_eq!(problem(&h), Some(RemnantProblem::NoClosedOutline));

    assert!(remnants(&h, doc).rows.is_empty());
    assert!(!fs.exists(FsPath::new(FILE)).unwrap(), "nothing was written");

    // The same name twice (case ignored).
    h.invoke(doc, CommandKey::SelectAll);
    h.invoke(doc, CommandKey::Delete);
    offcut(&mut h, doc);
    save(&mut h, doc, "Rest", "multiplex", "3");
    assert_eq!(problem(&h), None);
    save(&mut h, doc, " rest ", "multiplex", "3");
    assert_eq!(problem(&h), Some(RemnantProblem::NameTaken));
    assert_eq!(remnants(&h, doc).rows.len(), 1);
}

#[test]
fn placing_draws_the_sheet_and_its_zones_in_the_window_where_a_nest_can_use_them() {
    let fs = Arc::new(MemoryFileSystem::new());
    let (mut h, doc) = start(&fs);
    offcut(&mut h, doc);
    save(&mut h, doc, "Restplaat 1", "multiplex", "3");
    let id = remnants(&h, doc).rows[0].id;

    h.invoke(doc, CommandKey::SelectAll);
    h.invoke(doc, CommandKey::Delete);
    assert!(h.document(doc).unwrap().objects.is_empty());
    h.send(UiIntent::Remnant(RemnantIntent::Place { document: doc, id }));
    h.run_tasks();
    let objects = h.document(doc).unwrap().objects;
    assert_eq!(objects.len(), 2, "the sheet and its one zone");

    let size_of = |h: &mut HeadlessEditorHarness, object: GeometryId| {
        select(h, doc, &[object]);
        let inspector = h.document(doc).unwrap().inspector.expect("inspector");
        let text = |k| inspector.field(k).unwrap().text.clone();
        (text(FieldKey::Width), text(FieldKey::Height), text(FieldKey::X), text(FieldKey::Y))
    };
    let (w, hh, x0, y0) = size_of(&mut h, objects[0].id);
    assert_eq!((w.as_str(), hh.as_str()), ("200", "100"), "the contour has the remnant's size");
    let (zw, zh, x1, y1) = size_of(&mut h, objects[1].id);
    assert_eq!((zw.as_str(), zh.as_str()), ("20", "20"));
    let diff = |a: &str, b: &str| a.parse::<f64>().unwrap() - b.parse::<f64>().unwrap();
    assert!(
        (diff(&x1, &x0) - 20.0).abs() < 1e-6 && (diff(&y1, &y0) - 20.0).abs() < 1e-6,
        "zone 20 mm in from the corner"
    );

    // The contour is a closed object, so the nest offers it as a sheet.
    h.send(UiIntent::Remnant(RemnantIntent::Place { document: doc, id: 9999 }));
    assert_eq!(remnants(&h, doc).problem, Some(RemnantProblem::Unknown));
    assert_eq!(h.document(doc).unwrap().objects.len(), 2, "an unknown remnant draws nothing");
}

#[test]
fn a_damaged_library_file_is_left_alone_while_the_session_still_says_why() {
    let fs = Arc::new(MemoryFileSystem::new());
    fs.create_dir_all_for_test(FsPath::new("/lib"));
    fs.insert(FsPath::new(FILE), b"{ not json".to_vec());
    let (mut h, doc) = start(&fs);
    offcut(&mut h, doc);
    save(&mut h, doc, "Rest", "multiplex", "3");
    assert_eq!(remnants(&h, doc).problem, Some(RemnantProblem::Unreadable));
    assert_eq!(fs.read(FsPath::new(FILE), 1 << 20).unwrap(), b"{ not json".to_vec());
}

// --- orders ------------------------------------------------------------------------------------------

fn order_rig() -> (Rig, burnburn_presentation::OrderViewModel) {
    let mut r = rig();
    r.h.controller().attach_history(r.history.clone()).unwrap();
    let part = r.h.document(r.doc).unwrap().title;
    r.h.send(UiIntent::AddOrder { order: OrderDraft { name: "Klant 7".into(), part, quantity: 2, remnant: None } });
    let order = r.h.operator_view().orders[0].clone();
    (r, order)
}

fn keep_remnant(r: &mut Rig, name: &str, material: &str) {
    let sheet = rectangle(&mut r.h, r.doc, (10.0, 150.0), (210.0, 250.0));
    select(&mut r.h, r.doc, &[sheet]);
    save(&mut r.h, r.doc, name, material, "3");
}

#[test]
fn an_order_names_the_remnant_it_is_made_from_and_the_link_changes_no_count() {
    let (mut r, order) = order_rig();
    assert_eq!(order.remnant_link, None);
    keep_remnant(&mut r, "Restplaat 1", "multiplex");
    r.h.send(UiIntent::SetOrderRemnant { order: order.id, remnant: Some("Restplaat 1".into()) });
    let linked = r.h.operator_view().orders[0].clone();
    assert_eq!(linked.remnant.as_deref(), Some("Restplaat 1"));
    assert_eq!(
        linked.remnant_link,
        Some(RemnantLinkStatus::Known { material: "multiplex".into(), thickness_um: 3000, material_matches: None }),
        "the job's material is not known in this drawing, so no verdict"
    );
    assert_eq!((linked.completed, linked.quantity), (order.completed, order.quantity), "no count moved");

    // An unknown name is refused and the link stays.
    r.h.send(UiIntent::SetOrderRemnant { order: order.id, remnant: Some("bestaat niet".into()) });
    assert_eq!(r.h.operator_view().orders[0].remnant.as_deref(), Some("Restplaat 1"));

    // The remnant leaves the library: the order still says which one it named, and that it is gone.
    let id = remnants(&r.h, r.doc).rows[0].id;
    r.h.send(UiIntent::Remnant(RemnantIntent::Remove { id }));
    r.h.run_tasks();
    let gone = r.h.operator_view().orders[0].clone();
    assert_eq!((gone.remnant.as_deref(), gone.remnant_link), (Some("Restplaat 1"), Some(RemnantLinkStatus::Missing)));

    // Unlinking.
    r.h.send(UiIntent::SetOrderRemnant { order: order.id, remnant: None });
    let free = r.h.operator_view().orders[0].clone();
    assert_eq!((free.remnant, free.remnant_link), (None, None));
}

#[test]
fn the_link_is_kept_with_the_order_in_the_history() {
    let (mut r, order) = order_rig();
    keep_remnant(&mut r, "Restplaat 1", "multiplex");
    r.h.send(UiIntent::SetOrderRemnant { order: order.id, remnant: Some("Restplaat 1".into()) });
    let stored = r.history.orders.lock().unwrap().orders.clone();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].draft.remnant.as_deref(), Some("Restplaat 1"));
}

// --- the library feeds the nest ---------------------------------------------------------------------

/// A kept L-shaped remnant, drawn again from the library, is the sheet of a nest: the parts go inside
/// its contour and not into the corner that is gone (the nest of G6-werk, fed from the library).
#[test]
fn a_remnant_from_the_library_is_the_sheet_of_a_nest() {
    use burnburn_document::ObjectKind;
    use burnburn_presentation::{NestInput, NestRun, SchikkenIntent};

    let l_shape = burnburn_geometry::Path::polyline(
        &[
            burnburn_geometry::Point::new(0.0, 0.0),
            burnburn_geometry::Point::new(100.0, 0.0),
            burnburn_geometry::Point::new(100.0, 50.0),
            burnburn_geometry::Point::new(50.0, 50.0),
            burnburn_geometry::Point::new(50.0, 100.0),
            burnburn_geometry::Point::new(0.0, 100.0),
        ],
        true,
    )
    .unwrap();
    let (mut h, doc, ids) = edit_rig::opened_with_kinds(vec![(l_shape, ObjectKind::Path)]);
    edit_rig::select(&mut h, doc, &ids);
    save(&mut h, doc, "L-restje", "multiplex", "3");
    let id = remnants(&h, doc).rows[0].id;
    assert!((remnants(&h, doc).rows[0].usable_cm2 - 75.0).abs() < 1e-6, "the L's own area, not its bounding box");

    // The offcut leaves the drawing; the library draws it back, with three parts to cut from it.
    edit_rig::select(&mut h, doc, &ids);
    h.invoke(doc, CommandKey::Delete);
    h.send(UiIntent::Remnant(RemnantIntent::Place { document: doc, id }));
    let sheet = h.document(doc).unwrap().objects.last().unwrap().id;
    let mut parts = Vec::new();
    for x in [300.0, 340.0, 380.0] {
        let square = edit_rig::square(x, 300.0, 30.0);
        h.send(UiIntent::Select { document: doc, ids: Vec::new(), mode: SelectMode::Replace });
        parts.push(rectangle(&mut h, doc, (square_min(&square).0, 300.0), (square_min(&square).0 + 30.0, 330.0)));
    }
    edit_rig::select(&mut h, doc, &parts);
    h.invoke(doc, CommandKey::NestSelected);
    h.send(UiIntent::Schikken(SchikkenIntent::SetNestOptions {
        document: doc,
        input: Box::new(NestInput { sheet_object: Some(sheet.raw()), ..NestInput::default() }),
    }));
    h.send(UiIntent::Schikken(SchikkenIntent::StartNest { document: doc }));
    h.run_tasks();
    let panel = h.document(doc).unwrap().schikken.nest.expect("nest panel");
    assert_eq!(panel.run, NestRun::Done);
    let sheet_row = panel.sheet.expect("sheet");
    assert_eq!(sheet_row.contour, Some(sheet.raw()), "the drawn-back remnant is the sheet");
    let report = panel.report.expect("report");
    assert_eq!((report.placed, report.unplaced.len()), (3, 0));
}

fn square_min(path: &burnburn_geometry::Path) -> (f64, f64) {
    let b = path.bounds().unwrap();
    (b.min().x, b.min().y)
}
