//! W3-UI-bestanden, Art Library panel (M210): groups, thumbnails, adding files and the selection,
//! search, and inserting a drawing into the document in one history entry.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

use std::path::{Path as FsPath, PathBuf};

use burnburn_document::{EditLabel, UndoLabel};
use burnburn_io::fs::FileSystem;
use burnburn_presentation::{
    ArtPanelViewModel, CommandKey, FilesIntent, FilesMessage, HeadlessEditorHarness, ImportUnit, MessageKey,
    SelectMode, UiIntent,
};
use burnburn_test_support::fixtures::read_fixture;
use burnburn_types::DocumentId;

const ART_FILE: &str = "/lib/art-library.json";
/// A 10 × 10 mm square outline.
const SQUARE_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="10mm" height="10mm" viewBox="0 0 10 10"><rect id="sq" x="0" y="0" width="10" height="10" fill="none" stroke="black"/></svg>"#;

fn harness() -> HeadlessEditorHarness {
    let mut h = HeadlessEditorHarness::new();
    h.fs().create_dir_all_for_test(FsPath::new("/lib"));
    h.fs().create_dir_all_for_test(FsPath::new("/in"));
    h.controller().set_library_dir(Some(PathBuf::from("/lib")));
    h
}

fn put(h: &HeadlessEditorHarness, name: &str, bytes: &[u8]) -> PathBuf {
    let path = PathBuf::from(format!("/in/{name}"));
    h.fs().insert(&path, bytes.to_vec());
    path
}

fn panel(h: &HeadlessEditorHarness, doc: DocumentId) -> ArtPanelViewModel {
    h.document(doc).unwrap().files.art.expect("the art panel is open")
}

fn open(h: &mut HeadlessEditorHarness, doc: DocumentId) -> ArtPanelViewModel {
    h.send(UiIntent::Files(FilesIntent::OpenArtLibrary { document: doc }));
    h.run_tasks();
    panel(h, doc)
}

fn add_file(h: &mut HeadlessEditorHarness, doc: DocumentId, path: PathBuf, group: &str, name: &str) {
    let unit = None;
    h.send(UiIntent::Files(FilesIntent::AddArtFromFile {
        document: doc,
        path,
        group: group.into(),
        name: name.into(),
        dxf_unit: unit,
    }));
    h.run_tasks();
}

fn group(h: &mut HeadlessEditorHarness, doc: DocumentId, name: &str) {
    h.send(UiIntent::Files(FilesIntent::AddArtGroup { document: doc, name: name.into() }));
}

#[test]
fn the_panel_reads_the_library_on_a_worker_and_starts_empty() {
    let mut h = harness();
    let doc = h.new_document().unwrap();
    h.send(UiIntent::Files(FilesIntent::OpenArtLibrary { document: doc }));
    assert!(panel(&h, doc).loading);
    h.run_tasks();
    let p = panel(&h, doc);
    assert!(!p.loading && p.persisted && p.issue.is_none());
    assert!(p.groups.is_empty() && p.items.is_empty());
    h.send(UiIntent::Files(FilesIntent::CloseArtLibrary { document: doc }));
    assert!(h.document(doc).unwrap().files.art.is_none());
}

#[test]
fn groups_are_added_renamed_and_removed_but_never_with_items_in_them() {
    let mut h = harness();
    let doc = h.new_document().unwrap();
    open(&mut h, doc);
    group(&mut h, doc, "Dieren");
    group(&mut h, doc, "Letters");
    assert_eq!(panel(&h, doc).groups.iter().map(|g| g.name.as_str()).collect::<Vec<_>>(), ["Dieren", "Letters"]);
    group(&mut h, doc, "Dieren");
    assert_eq!(panel(&h, doc).issue, Some(MessageKey::Files(FilesMessage::ArtGroupExists)));
    group(&mut h, doc, "  ");
    assert_eq!(panel(&h, doc).issue, Some(MessageKey::Files(FilesMessage::ArtNameInvalid)));

    h.send(UiIntent::Files(FilesIntent::RenameArtGroup { document: doc, old: "Dieren".into(), new: "Beesten".into() }));
    assert!(panel(&h, doc).groups.iter().any(|g| g.name == "Beesten"));
    let path = put(&h, "vierkant.svg", SQUARE_SVG.as_bytes());
    add_file(&mut h, doc, path, "Beesten", "");
    assert_eq!(panel(&h, doc).groups[0].items, 1);
    h.send(UiIntent::Files(FilesIntent::RemoveArtGroup { document: doc, name: "Beesten".into() }));
    assert_eq!(panel(&h, doc).issue, Some(MessageKey::Files(FilesMessage::ArtGroupNotEmpty)));
    assert_eq!(panel(&h, doc).groups.len(), 2, "nothing was lost");
    h.send(UiIntent::Files(FilesIntent::RemoveArtGroup { document: doc, name: "Letters".into() }));
    assert_eq!(panel(&h, doc).groups.len(), 1);
    h.send(UiIntent::Files(FilesIntent::RemoveArtGroup { document: doc, name: "Bestaat niet".into() }));
    assert_eq!(panel(&h, doc).issue, Some(MessageKey::Files(FilesMessage::ArtGroupUnknown)));
}

/// A file becomes an item named after the file, with a thumbnail made on the worker; it is
/// written to the library file and found again by a new session.
#[test]
fn a_file_becomes_an_item_with_a_thumbnail_that_survives_a_restart() {
    let mut h = harness();
    let doc = h.new_document().unwrap();
    open(&mut h, doc);
    group(&mut h, doc, "Vormen");
    let path = put(&h, "vierkant.svg", SQUARE_SVG.as_bytes());
    add_file(&mut h, doc, path, "Vormen", "");
    let p = panel(&h, doc);
    assert_eq!(p.items.len(), 1);
    assert_eq!((p.items[0].name.as_str(), p.items[0].group.as_str()), ("vierkant", "Vormen"));
    assert!(!p.busy);
    let t = p.items[0].thumbnail.as_ref().expect("a thumbnail");
    assert_eq!((t.width, t.height), (64, 64));
    // A 10 mm square fitted into 64 px with a 3 px margin spans pixels 3 … 61 (58 px, centred):
    // the top and left edges are drawn on row 3 and column 3, the margin and the middle are empty.
    let at = |x: u32, y: u32| t.coverage[(y * t.width + x) as usize];
    assert_eq!(at(32, 32), 0, "inside of the square is empty");
    assert!((3..=61).all(|x| at(x, 3) > 0) && (3..=61).all(|y| at(3, y) > 0), "the outline is drawn");
    assert!((0..64).all(|x| at(x, 1) == 0 && at(1, x) == 0), "the margin stays empty");
    assert!(h.fs().exists(FsPath::new(ART_FILE)).unwrap());

    let mut again = HeadlessEditorHarness::with_fs(h.shared_fs());
    again.controller().set_library_dir(Some(PathBuf::from("/lib")));
    let doc2 = again.new_document().unwrap();
    let p = open(&mut again, doc2);
    assert_eq!(p.items.len(), 1);
    assert!(p.items[0].thumbnail.is_some(), "thumbnails are made when the library is read");
}

#[test]
fn a_file_that_does_not_import_or_is_no_drawing_changes_nothing() {
    let mut h = harness();
    let doc = h.new_document().unwrap();
    open(&mut h, doc);
    group(&mut h, doc, "Vormen");
    let bad = put(&h, "kapot.svg", b"<svg");
    add_file(&mut h, doc, bad, "Vormen", "");
    assert_eq!(panel(&h, doc).issue, Some(MessageKey::Files(FilesMessage::ArtFileUnreadable)));
    let other = put(&h, "foto.png", b"png");
    add_file(&mut h, doc, other, "Vormen", "");
    assert_eq!(panel(&h, doc).issue, Some(MessageKey::Files(FilesMessage::ArtFileFormat)));
    let good = put(&h, "vierkant.svg", SQUARE_SVG.as_bytes());
    add_file(&mut h, doc, good, "Onbekend", "");
    assert_eq!(panel(&h, doc).issue, Some(MessageKey::Files(FilesMessage::ArtGroupUnknown)));
    assert!(panel(&h, doc).items.is_empty());
}

/// A DXF without a unit is not guessed: the panel asks, and the file is added with the choice.
#[test]
fn a_dxf_without_a_unit_waits_for_the_choice() {
    let mut h = harness();
    let doc = h.new_document().unwrap();
    open(&mut h, doc);
    group(&mut h, doc, "CAD");
    let bytes = read_fixture("import", "unitless-lines.dxf").unwrap();
    let path = put(&h, "plan.dxf", &bytes);
    add_file(&mut h, doc, path.clone(), "CAD", "");
    let p = panel(&h, doc);
    assert_eq!(p.needs_scale, Some(path.clone()));
    assert!(p.items.is_empty());
    h.send(UiIntent::Files(FilesIntent::AddArtFromFile {
        document: doc,
        path,
        group: "CAD".into(),
        name: "Plan".into(),
        dxf_unit: Some(ImportUnit::Inch),
    }));
    h.run_tasks();
    let p = panel(&h, doc);
    assert!(p.needs_scale.is_none());
    assert_eq!(p.items.len(), 1);
    assert_eq!(p.items[0].name, "Plan");
}

/// Inserting puts the drawing in the document in one history entry, centred on the view (or the drop point).
#[test]
fn placing_inserts_the_drawing_centred_in_one_history_entry() {
    let mut h = harness();
    let doc = h.new_document().unwrap();
    open(&mut h, doc);
    group(&mut h, doc, "Vormen");
    let path = put(&h, "vierkant.svg", SQUARE_SVG.as_bytes());
    add_file(&mut h, doc, path, "Vormen", "");
    let item = panel(&h, doc).items[0].id;
    let centre = h.document(doc).unwrap().camera.center_mm();

    h.send(UiIntent::Files(FilesIntent::PlaceArt { document: doc, item, at: None }));
    h.run_tasks();
    let vm = h.document(doc).unwrap();
    assert_eq!(vm.objects.len(), 1);
    assert_eq!(vm.undo, Some(UndoLabel::Edit(EditLabel::Import)));
    h.send(UiIntent::Select { document: doc, ids: vec![vm.objects[0].id], mode: SelectMode::Replace });
    let inspector = h.document(doc).unwrap().inspector.unwrap();
    let field = |k| inspector.fields.iter().find(|f| f.key == k).unwrap().text.clone();
    assert_eq!(
        (field(burnburn_presentation::FieldKey::Width), field(burnburn_presentation::FieldKey::Height)),
        ("10".into(), "10".into())
    );
    // The reference point of the inspector is the object's bottom-left corner: centre − 5 mm.
    let x: f64 = field(burnburn_presentation::FieldKey::X).replace(',', ".").parse().unwrap();
    assert!((x - (centre.x - 5.0)).abs() < 0.01, "{x} vs {}", centre.x);

    h.invoke(doc, CommandKey::Undo);
    assert!(h.document(doc).unwrap().objects.is_empty(), "one undo removes the whole drawing");
}

/// The selection goes in as the SVG it exports to and comes back at the same size.
#[test]
fn the_selection_can_be_added_and_placed_back() {
    let mut h = harness();
    let doc = h.new_document().unwrap();
    open(&mut h, doc);
    group(&mut h, doc, "Eigen");
    h.send(UiIntent::Files(FilesIntent::AddArtFromSelection {
        document: doc,
        name: "Niets".into(),
        group: "Eigen".into(),
    }));
    assert_eq!(panel(&h, doc).issue, Some(MessageKey::Files(FilesMessage::ArtNothingSelected)));
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (0.0, 0.0), (40.0, 20.0));
    h.invoke(doc, CommandKey::ToolSelect);
    let id = h.document(doc).unwrap().objects[0].id;
    h.send(UiIntent::Select { document: doc, ids: vec![id], mode: SelectMode::Replace });
    h.send(UiIntent::Files(FilesIntent::AddArtFromSelection {
        document: doc,
        name: "Plaat".into(),
        group: "Eigen".into(),
    }));
    h.run_tasks();
    let p = panel(&h, doc);
    assert_eq!(p.items.len(), 1);
    assert_eq!(p.items[0].format, burnburn_io::art_library::ArtFormat::Svg);
    h.send(UiIntent::Files(FilesIntent::PlaceArt { document: doc, item: p.items[0].id, at: None }));
    h.run_tasks();
    assert_eq!(h.document(doc).unwrap().objects.len(), 2);
}

#[test]
fn search_and_group_narrow_the_items_and_items_can_be_renamed_moved_and_removed() {
    let mut h = harness();
    let doc = h.new_document().unwrap();
    open(&mut h, doc);
    group(&mut h, doc, "A");
    group(&mut h, doc, "B");
    let one = put(&h, "ster.svg", SQUARE_SVG.as_bytes());
    let two = put(&h, "maan.svg", SQUARE_SVG.as_bytes());
    add_file(&mut h, doc, one, "A", "");
    add_file(&mut h, doc, two, "B", "");
    h.send(UiIntent::Files(FilesIntent::SelectArtGroup { document: doc, group: Some("B".into()) }));
    assert_eq!(panel(&h, doc).items.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(), ["maan"]);
    h.send(UiIntent::Files(FilesIntent::SelectArtGroup { document: doc, group: None }));
    h.send(UiIntent::Files(FilesIntent::SetArtSearch { document: doc, text: "ST".into() }));
    assert_eq!(panel(&h, doc).items.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(), ["ster"]);
    h.send(UiIntent::Files(FilesIntent::SetArtSearch { document: doc, text: String::new() }));
    let ster = panel(&h, doc).items.iter().find(|i| i.name == "ster").unwrap().id;
    h.send(UiIntent::Files(FilesIntent::RenameArtItem { document: doc, item: ster, name: "Ster".into() }));
    h.send(UiIntent::Files(FilesIntent::MoveArtItem { document: doc, item: ster, group: "B".into() }));
    let item = panel(&h, doc).items.into_iter().find(|i| i.id == ster).unwrap();
    assert_eq!((item.name.as_str(), item.group.as_str()), ("Ster", "B"));
    h.send(UiIntent::Files(FilesIntent::RemoveArtItem { document: doc, item: ster }));
    assert_eq!(panel(&h, doc).items.len(), 1);
    h.send(UiIntent::Files(FilesIntent::RemoveArtItem { document: doc, item: ster }));
    assert_eq!(panel(&h, doc).issue, Some(MessageKey::Files(FilesMessage::ArtItemUnknown)));
}

#[test]
fn a_damaged_art_file_is_reported_and_left_alone() {
    let mut h = harness();
    h.fs().insert(FsPath::new(ART_FILE), b"garbage".to_vec());
    let doc = h.new_document().unwrap();
    let p = open(&mut h, doc);
    assert_eq!(p.issue, Some(MessageKey::Files(FilesMessage::LibraryUnreadable)));
    assert!(!p.persisted);
    group(&mut h, doc, "Vormen");
    assert_eq!(panel(&h, doc).groups.len(), 1, "usable in memory");
    assert_eq!(h.fs().read(FsPath::new(ART_FILE), 1 << 20).unwrap(), b"garbage");
}
