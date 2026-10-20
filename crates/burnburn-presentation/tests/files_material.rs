//! W3-UI-bestanden, Material Library panel (M217, C08): search by material and thickness, apply an
//! entry to a layer, see which settings are copied and which are linked, and never let a library
//! update change a layer by itself.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

use std::path::{Path as FsPath, PathBuf};
use std::sync::Arc;

use burnburn_document::UndoLabel;
use burnburn_document::process::ProcessLabel;
use burnburn_io::fs::FileSystem;
use burnburn_presentation::{
    CommandKey, FilesIntent, FilesMessage, HeadlessEditorHarness, MaterialLinkStatus, MaterialOrigin,
    MaterialPanelViewModel, MessageKey, OperationField, OperationKind, SelectMode, UiIntent,
};
use burnburn_types::{DocumentId, OperationId, StableId};

const LIBRARY_FILE: &str = "/lib/material-library.json";

fn harness() -> HeadlessEditorHarness {
    let mut h = HeadlessEditorHarness::new();
    h.fs().create_dir_all_for_test(FsPath::new("/lib"));
    h.controller().set_library_dir(Some(PathBuf::from("/lib")));
    h
}

/// A window with two 20 × 10 rectangles on layers 1 and 2: two Line operations.
fn two_layers(h: &mut HeadlessEditorHarness) -> (DocumentId, OperationId, OperationId) {
    let doc = h.new_document().expect("window");
    for (i, x) in [0.0, 40.0].into_iter().enumerate() {
        h.invoke(doc, CommandKey::ToolRectangle);
        h.drag(doc, (x, 0.0), (x + 20.0, 10.0));
        h.invoke(doc, CommandKey::ToolSelect);
        let id = h.document(doc).unwrap().objects[i].id;
        h.send(UiIntent::Select { document: doc, ids: vec![id], mode: SelectMode::Replace });
        h.send(UiIntent::AssignLayer { document: doc, slot: i as u8 + 1, add: false });
    }
    let ops = h.document(doc).unwrap().operations;
    (doc, ops[0].id, ops[1].id)
}

fn set(h: &mut HeadlessEditorHarness, doc: DocumentId, op: OperationId, field: OperationField, text: &str) {
    h.send(UiIntent::SetOperationField { document: doc, operation: op, field, text: text.into() });
}

fn open_panel(h: &mut HeadlessEditorHarness, doc: DocumentId) -> MaterialPanelViewModel {
    h.send(UiIntent::Files(FilesIntent::OpenMaterialLibrary { document: doc }));
    h.run_tasks();
    panel(h, doc)
}

fn panel(h: &HeadlessEditorHarness, doc: DocumentId) -> MaterialPanelViewModel {
    h.document(doc).unwrap().files.material.expect("the material panel is open")
}

fn save_entry(h: &mut HeadlessEditorHarness, doc: DocumentId, op: OperationId, name: &str, material: &str, mm: &str) {
    h.send(UiIntent::Files(FilesIntent::SaveMaterial {
        document: doc,
        operation: op,
        name: name.into(),
        material: material.into(),
        thickness: mm.into(),
        notes: String::new(),
    }));
    h.run_tasks();
}

fn row(h: &HeadlessEditorHarness, doc: DocumentId, op: OperationId) -> burnburn_presentation::OperationRow {
    h.document(doc).unwrap().operations.into_iter().find(|o| o.id == op).unwrap()
}

/// The panel opens loading; a library file that does not exist yet is an empty library, not an error.
#[test]
fn opening_the_panel_reads_the_library_on_a_worker_and_a_missing_file_is_an_empty_library() {
    let mut h = harness();
    let (doc, ..) = two_layers(&mut h);
    h.send(UiIntent::Files(FilesIntent::OpenMaterialLibrary { document: doc }));
    let p = panel(&h, doc);
    assert!(p.loading, "the read is a task");
    h.run_tasks();
    let p = panel(&h, doc);
    assert!(!p.loading && p.issue.is_none() && p.persisted);
    assert!(p.entries.is_empty() && p.materials.is_empty());
    assert_eq!(p.operations.len(), 2);
    assert!(p.operations.iter().all(|o| o.origin == MaterialOrigin::Manual && o.applicable));
    h.send(UiIntent::Files(FilesIntent::CloseMaterialLibrary { document: doc }));
    assert!(h.document(doc).unwrap().files.material.is_none());
}

/// A layer's mode and settings become an entry for a material and thickness, are written to the
/// library file and are found again by a new session.
#[test]
fn a_layer_is_stored_as_an_entry_that_survives_a_restart() {
    let mut h = harness();
    let (doc, a, _) = two_layers(&mut h);
    set(&mut h, doc, a, OperationField::Speed, "12");
    set(&mut h, doc, a, OperationField::Power, "65");
    open_panel(&mut h, doc);
    save_entry(&mut h, doc, a, "Snijden", "Berken", "3");
    let p = panel(&h, doc);
    assert_eq!(p.entries.len(), 1);
    let e = &p.entries[0];
    assert_eq!((e.name.as_str(), e.material.as_str(), e.thickness_mm, e.revision), ("Snijden", "Berken", 3.0, 1));
    assert_eq!((e.speed.as_str(), e.power.as_str(), e.mode), ("12", "65", OperationKind::Line));
    assert_eq!(p.materials, ["Berken"]);
    assert!(h.fs().exists(FsPath::new(LIBRARY_FILE)).unwrap(), "written next to the user's other data");

    let mut again = HeadlessEditorHarness::with_fs(h.shared_fs());
    again.controller().set_library_dir(Some(PathBuf::from("/lib")));
    let doc2 = again.new_document().unwrap();
    let p = open_panel(&mut again, doc2);
    assert_eq!(p.entries.len(), 1, "found again after a restart");
    assert_eq!(p.entries[0].name, "Snijden");
}

#[test]
fn an_entry_needs_a_name_a_material_and_a_positive_thickness_and_never_comes_from_an_image_layer() {
    let mut h = harness();
    let (doc, a, _) = two_layers(&mut h);
    open_panel(&mut h, doc);
    for (name, material, mm) in
        [("", "Berken", "3"), ("Snijden", " ", "3"), ("Snijden", "Berken", "0"), ("Snijden", "Berken", "drie")]
    {
        save_entry(&mut h, doc, a, name, material, mm);
        let p = panel(&h, doc);
        assert!(p.entries.is_empty(), "{name:?} {material:?} {mm:?}");
        assert_eq!(p.issue, Some(MessageKey::Files(FilesMessage::MaterialInvalid)), "{name:?} {material:?} {mm:?}");
    }
    save_entry(&mut h, doc, OperationId::from_raw(999), "Snijden", "Berken", "3");
    assert_eq!(panel(&h, doc).issue, Some(MessageKey::Files(FilesMessage::MaterialUnknown)));
}

fn apply(h: &mut HeadlessEditorHarness, doc: DocumentId, entry: u64, ops: &[OperationId]) {
    h.send(UiIntent::Files(FilesIntent::ApplyMaterial { document: doc, entry, operations: ops.to_vec() }));
    h.run_tasks();
}

/// Applying copies mode and settings into the layer; the layer then shows the recipe and library
/// entry it came from. The other layer stays hand-made.
#[test]
fn applying_an_entry_copies_its_settings_and_shows_the_link() {
    let mut h = harness();
    let (doc, a, b) = two_layers(&mut h);
    set(&mut h, doc, a, OperationField::Speed, "12");
    set(&mut h, doc, a, OperationField::Power, "65");
    open_panel(&mut h, doc);
    save_entry(&mut h, doc, a, "Snijden", "Berken", "3");
    let entry = panel(&h, doc).entries[0].id;
    let before = row(&h, doc, b);
    assert_ne!(before.speed, "12");

    apply(&mut h, doc, entry, &[b]);
    let after = row(&h, doc, b);
    assert_eq!((after.speed.as_str(), after.power.as_str()), ("12", "65"));
    assert_eq!(h.document(doc).unwrap().undo, Some(UndoLabel::Process(ProcessLabel::ApplyRecipe)));
    let p = panel(&h, doc);
    let origin = &p.operations.iter().find(|o| o.id == b).unwrap().origin;
    let MaterialOrigin::Linked { entry: linked, revision, status, .. } = origin else { panic!("{origin:?}") };
    assert_eq!((*linked, *revision, *status), (entry, 1, MaterialLinkStatus::UpToDate));
    assert_eq!(p.operations.iter().find(|o| o.id == a).unwrap().origin, MaterialOrigin::Manual);
    // Applying the same entry again reuses the project's recipe instead of adding another.
    apply(&mut h, doc, entry, &[a]);
    let recipes: std::collections::BTreeSet<u64> = panel(&h, doc)
        .operations
        .iter()
        .filter_map(|o| match &o.origin {
            MaterialOrigin::Linked { recipe, .. } => Some(*recipe),
            _ => None,
        })
        .collect();
    assert_eq!(recipes.len(), 1, "both layers share one recipe: {recipes:?}");
}

/// C08: a library update is reported and offered, never applied. The layer keeps the settings it
/// had until the user updates the recipe and re-applies it, step by step.
#[test]
fn library_update_does_not_change_existing_job_settings() {
    let mut h = harness();
    let (doc, a, b) = two_layers(&mut h);
    set(&mut h, doc, a, OperationField::Speed, "12");
    open_panel(&mut h, doc);
    save_entry(&mut h, doc, a, "Snijden", "Berken", "3");
    let entry = panel(&h, doc).entries[0].id;
    apply(&mut h, doc, entry, &[b]);
    let recipe = match &panel(&h, doc).operations.iter().find(|o| o.id == b).unwrap().origin {
        MaterialOrigin::Linked { recipe, .. } => *recipe,
        other => panic!("{other:?}"),
    };

    // The library entry changes (a better speed measured): revision 2.
    set(&mut h, doc, a, OperationField::Speed, "20");
    h.send(UiIntent::Files(FilesIntent::UpdateMaterialFromOperation { document: doc, entry, operation: a }));
    h.run_tasks();
    let p = panel(&h, doc);
    assert_eq!((p.entries[0].revision, p.entries[0].speed.as_str()), (2, "20"));
    assert_eq!(row(&h, doc, b).speed, "12", "the layer keeps its copied settings");
    let origin = &p.operations.iter().find(|o| o.id == b).unwrap().origin;
    let MaterialOrigin::Linked { status, revision, .. } = origin else { panic!("{origin:?}") };
    assert_eq!((*status, *revision), (MaterialLinkStatus::UpdateAvailable { applied: 1, latest: 2 }, 1));

    // Bijwerken without re-applying: the recipe is refreshed, the layer still is not.
    h.send(UiIntent::Files(FilesIntent::UpdateLinkedRecipe { document: doc, recipe, reapply: false }));
    h.run_tasks();
    assert_eq!(row(&h, doc, b).speed, "12", "refreshing the recipe does not touch the layer");
    let origin = &panel(&h, doc).operations.iter().find(|o| o.id == b).unwrap().origin.clone();
    let MaterialOrigin::Linked { status, .. } = origin else { panic!("{origin:?}") };
    assert_eq!(*status, MaterialLinkStatus::UpToDate, "the link now names revision 2");

    // Only the explicit re-apply changes the layer.
    h.send(UiIntent::Files(FilesIntent::UpdateLinkedRecipe { document: doc, recipe, reapply: true }));
    h.run_tasks();
    assert_eq!(row(&h, doc, b).speed, "20");
}

/// An image layer is never replaced by a library entry (its pixels would be lost).
#[test]
fn a_layer_that_cannot_take_an_entry_says_so_and_nothing_changes() {
    let mut h = harness();
    let (doc, a, b) = two_layers(&mut h);
    open_panel(&mut h, doc);
    save_entry(&mut h, doc, a, "Snijden", "Berken", "3");
    let entry = panel(&h, doc).entries[0].id;
    let before = h.document(doc).unwrap().revision;
    apply(&mut h, doc, entry + 100, &[b]);
    assert_eq!(h.document(doc).unwrap().revision, before);
    assert_eq!(panel(&h, doc).issue, Some(MessageKey::Files(FilesMessage::MaterialUnknown)));
    apply(&mut h, doc, entry, &[OperationId::from_raw(999)]);
    assert_eq!(h.document(doc).unwrap().revision, before);
}

/// The recipe links go into the project file with Save and come back with Open (provenance).
#[test]
fn the_links_travel_with_the_project_file() {
    let mut h = harness();
    let (doc, a, b) = two_layers(&mut h);
    open_panel(&mut h, doc);
    save_entry(&mut h, doc, a, "Snijden", "Berken", "3");
    let entry = panel(&h, doc).entries[0].id;
    apply(&mut h, doc, entry, &[b]);
    let path = PathBuf::from("/projects/a.burnburn");
    h.fs().create_dir_all_for_test(FsPath::new("/projects"));
    h.send(UiIntent::Save { document: doc, path: Some(path.clone()) });
    h.run_tasks();

    let mut again = HeadlessEditorHarness::with_fs(h.shared_fs());
    again.controller().set_library_dir(Some(PathBuf::from("/lib")));
    again.send(UiIntent::Open { path });
    again.run_tasks();
    let opened = again.view().active.expect("the project opened");
    let p = open_panel(&mut again, opened);
    let linked = p.operations.iter().filter(|o| matches!(o.origin, MaterialOrigin::Linked { .. })).count();
    assert_eq!(linked, 1, "{:?}", p.operations);
}

#[test]
fn material_and_thickness_narrow_the_entries() {
    let mut h = harness();
    let (doc, a, b) = two_layers(&mut h);
    open_panel(&mut h, doc);
    save_entry(&mut h, doc, a, "Snijden", "Berken", "3");
    save_entry(&mut h, doc, b, "Snijden", "Berken", "6");
    save_entry(&mut h, doc, a, "Graveren", "Acryl", "3");
    let filter = |h: &mut HeadlessEditorHarness, material: Option<&str>, thickness: &str| {
        h.send(UiIntent::Files(FilesIntent::SetMaterialFilter {
            document: doc,
            material: material.map(str::to_owned),
            thickness: thickness.into(),
            machine: None,
        }));
        panel(h, doc)
    };
    assert_eq!(filter(&mut h, None, "").entries.len(), 3);
    let p = filter(&mut h, Some("Berken"), "");
    assert_eq!(p.entries.len(), 2);
    assert_eq!(p.thicknesses_mm, [3.0, 6.0]);
    assert_eq!(filter(&mut h, Some("Berken"), "6").entries.len(), 1);
    assert_eq!(filter(&mut h, None, "3 mm").entries.len(), 2, "any material at 3 mm");
    let p = filter(&mut h, Some("Berken"), "veel");
    assert_eq!(p.issue, Some(MessageKey::Files(FilesMessage::MaterialInvalid)), "a bad thickness keeps the old result");
    assert_eq!(p.entries.len(), 2, "still the result of the last good search (any material at 3 mm)");
    assert_eq!(p.filter.thickness_text, "3 mm");
}

/// A library file that cannot be read is reported and never overwritten by an empty one.
#[test]
fn a_damaged_library_file_is_reported_and_left_alone() {
    let mut h = harness();
    h.fs().insert(FsPath::new(LIBRARY_FILE), b"{ not a library".to_vec());
    let (doc, a, _) = two_layers(&mut h);
    let p = open_panel(&mut h, doc);
    assert_eq!(p.issue, Some(MessageKey::Files(FilesMessage::LibraryUnreadable)));
    assert!(!p.persisted);
    save_entry(&mut h, doc, a, "Snijden", "Berken", "3");
    assert_eq!(panel(&h, doc).entries.len(), 1, "usable in memory");
    assert_eq!(h.fs().read(FsPath::new(LIBRARY_FILE), 1 << 20).unwrap(), b"{ not a library", "the file is untouched");
    let _ = Arc::clone(&h.shared_fs());
}
