//! C08 (G7-cam): material recipes in the operation rows of a project that already exists. The
//! fixture `test-data/projects/v2/recipes-in-rows.burnburn` is schema 2 from before library links.
//! Opened in the window, its rows say which recipe they were copied from, no library link is
//! invented, a library entry applied afterwards gets a recipe id that collides with nothing, and a
//! save and reopen keeps every row as it was.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

use std::path::{Path as FsPath, PathBuf};

use burnburn_io::fs::FileSystem;
use burnburn_presentation::{
    FilesIntent, HeadlessEditorHarness, HostEffect, MaterialOrigin, MaterialPanelViewModel, OperationField, UiIntent,
};
use burnburn_types::{DocumentId, OperationId, StableId};

const LIBRARY_DIR: &str = "/lib";

fn fixture_bytes() -> Vec<u8> {
    std::fs::read(FsPath::new(env!("CARGO_MANIFEST_DIR")).join("../../test-data/projects/v2/recipes-in-rows.burnburn"))
        .unwrap()
}

fn open_fixture() -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    h.fs().create_dir_all_for_test(FsPath::new("/work"));
    h.fs().create_dir_all_for_test(FsPath::new(LIBRARY_DIR));
    h.controller().set_library_dir(Some(PathBuf::from(LIBRARY_DIR)));
    let path = PathBuf::from("/work/recipes.burnburn");
    h.fs().write_all(&path, &fixture_bytes()).unwrap();
    h.send(UiIntent::Open { path });
    let doc = h
        .run_tasks()
        .iter()
        .flat_map(|u| u.effects.clone())
        .find_map(|e| match e {
            HostEffect::OpenWindow { document } => Some(document),
            _ => None,
        })
        .expect("the project opened");
    (h, doc)
}

fn panel(h: &mut HeadlessEditorHarness, doc: DocumentId) -> MaterialPanelViewModel {
    h.document(doc).unwrap().files.material.expect("the material panel is open")
}

fn open_panel(h: &mut HeadlessEditorHarness, doc: DocumentId) -> MaterialPanelViewModel {
    h.send(UiIntent::Files(FilesIntent::OpenMaterialLibrary { document: doc }));
    h.run_tasks();
    panel(h, doc)
}

fn origin(p: &MaterialPanelViewModel, op: u64) -> MaterialOrigin {
    p.operations.iter().find(|o| o.id == OperationId::from_raw(op)).unwrap().origin.clone()
}

#[test]
fn the_rows_of_an_existing_project_say_which_recipe_they_were_copied_from() {
    let (mut h, doc) = open_fixture();
    let p = open_panel(&mut h, doc);
    let names: Vec<&str> = p.operations.iter().map(|o| o.name.as_str()).collect();
    assert_eq!(names, ["Binnen", "Buiten", "Graveren", "Handwerk"]);
    let copied = |recipe: u64, name: &str, revision: u32| MaterialOrigin::Copied {
        recipe,
        recipe_name: name.to_owned(),
        revision,
    };
    assert_eq!(origin(&p, 5), copied(7, "Berken 3 mm snijden", 2));
    assert_eq!(origin(&p, 6), copied(7, "Berken 3 mm snijden", 1), "the older row names the revision it has");
    assert_eq!(origin(&p, 10), copied(8, "Berken graveren", 1));
    assert_eq!(origin(&p, 11), MaterialOrigin::Manual);

    // The numbers in the layer table are the rows' own, not the recipe's.
    let rows = h.document(doc).unwrap().operations;
    let speed = |op: u64| rows.iter().find(|r| r.id == OperationId::from_raw(op)).unwrap().speed.clone();
    assert_eq!((speed(5).as_str(), speed(6).as_str()), ("22", "25"));
}

#[test]
fn a_library_entry_applied_later_gets_a_recipe_of_its_own_and_the_old_rows_stay() {
    let (mut h, doc) = open_fixture();
    open_panel(&mut h, doc);
    // Store the hand-made row as an entry, then apply that entry to a row that had no recipe yet.
    h.send(UiIntent::Files(FilesIntent::SaveMaterial {
        document: doc,
        operation: OperationId::from_raw(11),
        name: "Handwerk".into(),
        material: "Berken".into(),
        thickness: "3".into(),
        notes: String::new(),
    }));
    h.run_tasks();
    let entry = panel(&mut h, doc).entries[0].id;
    h.send(UiIntent::SetOperationField {
        document: doc,
        operation: OperationId::from_raw(5),
        field: OperationField::Speed,
        text: "40".into(),
    });
    h.send(UiIntent::Files(FilesIntent::ApplyMaterial {
        document: doc,
        entry,
        operations: vec![OperationId::from_raw(5)],
    }));
    h.run_tasks();
    let p = panel(&mut h, doc);
    let MaterialOrigin::Linked { recipe, .. } = origin(&p, 5) else { panic!("{:?}", origin(&p, 5)) };
    assert!(![7, 8, 9].contains(&recipe), "the new recipe is a new recipe, not one of the old: {recipe}");
    assert!(recipe >= 12, "it takes an id from the project's counter (next id 12): {recipe}");
    // The other rows are untouched.
    assert_eq!(
        origin(&p, 6),
        MaterialOrigin::Copied { recipe: 7, recipe_name: "Berken 3 mm snijden".into(), revision: 1 }
    );
    assert_eq!(
        origin(&p, 10),
        MaterialOrigin::Copied { recipe: 8, recipe_name: "Berken graveren".into(), revision: 1 }
    );
}

#[test]
fn saving_and_reopening_keeps_the_rows_as_they_were() {
    let (mut h, doc) = open_fixture();
    let before = open_panel(&mut h, doc);
    let path = PathBuf::from("/work/saved.burnburn");
    h.send(UiIntent::Save { document: doc, path: Some(path.clone()) });
    h.run_tasks();
    h.send(UiIntent::Open { path });
    let reopened = h
        .run_tasks()
        .iter()
        .flat_map(|u| u.effects.clone())
        .find_map(|e| match e {
            HostEffect::OpenWindow { document } => Some(document),
            _ => None,
        })
        .expect("the saved project opened");
    let after = open_panel(&mut h, reopened);
    let origins =
        |p: &MaterialPanelViewModel| p.operations.iter().map(|o| (o.id, o.origin.clone())).collect::<Vec<_>>();
    assert_eq!(origins(&after), origins(&before));
}
