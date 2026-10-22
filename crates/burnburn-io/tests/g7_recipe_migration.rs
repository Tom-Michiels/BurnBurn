//! C08 (G7-cam): material recipes in the operation rows of existing projects. The fixture
//! `test-data/projects/v2/recipes-in-rows.burnburn` is schema 2 as it was before library links,
//! written by hand: rows that reference one recipe at different revisions, a recipe nobody uses and
//! a hand-made row. It must keep loading with the same meaning, re-save without inventing anything,
//! and editing a recipe afterwards must never change a row (a row keeps the numbers it copied).
//! Expectations are written from the fixture's reviewed `document.json`, not read back from the encoder.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

use burnburn_document::process::{
    OperationMode, ProcessCommand, RecipeDraft, RecipeId, RecipeRef,
};
use burnburn_document::{CommandPayload, DocumentCommand, DocumentSession};
use burnburn_io::std_fs::StdFileSystem;
use burnburn_io::{Project, decode_container, encode_container, load_project, project_hash};
use burnburn_types::{DocumentId, Length, OperationId, RequestId, StableId};
use sha2::{Digest, Sha256};

const FIXTURE: &str = "v2/recipes-in-rows.burnburn";

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-data/projects")
}

fn bytes() -> Vec<u8> {
    std::fs::read(fixtures().join(FIXTURE)).unwrap()
}

fn entry(container: &[u8], name: &str) -> Vec<u8> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(container)).unwrap();
    let mut out = Vec::new();
    std::io::Read::read_to_end(&mut zip.by_name(name).unwrap(), &mut out).unwrap();
    out
}

fn op(p: &Project, id: u64) -> burnburn_document::process::Operation {
    p.document.process().operation(OperationId::from_raw(id)).unwrap().clone()
}

#[test]
fn the_fixture_is_the_recorded_file() {
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(fixtures().join("manifest.json")).unwrap()).unwrap();
    let entry = manifest["fixtures"].as_array().unwrap().iter().find(|f| f["file"] == FIXTURE).expect("in the manifest");
    let digest: [u8; 32] = Sha256::digest(bytes()).into();
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(entry["sha256"].as_str().unwrap(), hex, "fixture bytes changed: review, do not regenerate silently");
}

#[test]
fn rows_keep_their_recipe_reference_and_their_own_numbers() {
    let p = load_project(&StdFileSystem, &fixtures().join(FIXTURE)).unwrap();
    assert_eq!(p.uuid.to_string(), "0b0b0b0b-0003-4000-8000-000000000120");
    assert_eq!(p.document.next_id(), 12);
    assert!(p.library_links.is_empty(), "a project from before library links has none, and none are invented");

    let recipe = |row: u64| op(&p, row).recipe();
    assert_eq!(recipe(5), Some(RecipeRef { recipe: RecipeId::from_raw(7), revision: 2 }));
    assert_eq!(recipe(6), Some(RecipeRef { recipe: RecipeId::from_raw(7), revision: 1 }));
    assert_eq!(recipe(10), Some(RecipeRef { recipe: RecipeId::from_raw(8), revision: 1 }));
    assert_eq!(recipe(11), None, "a hand-made row stays hand-made");

    // The older row keeps what it copied (25 mm/s, 70 %), though recipe 7 now says 22 mm/s, 60 %.
    let s = |row: u64| {
        let o = op(&p, row);
        let s = o.settings();
        (s.speed.get_mm_per_s(), s.power.min().get(), s.power.max().get(), s.passes.get(), s.air_assist)
    };
    assert_eq!(s(5), (22.0, 60.0, 60.0, 1, true));
    assert_eq!(s(6), (25.0, 70.0, 70.0, 1, true));
    assert_eq!(s(10), (300.0, 10.0, 25.0, 1, false));
    assert!(matches!(op(&p, 10).mode(), OperationMode::Fill(_)));

    let seven = p.document.process().recipe(RecipeId::from_raw(7)).unwrap();
    assert_eq!((seven.name(), seven.material(), seven.revision(), seven.thickness().mm()), ("Berken 3 mm snijden", "Berken multiplex", 2, 3.0));
    assert_eq!(seven.settings().speed.get_mm_per_s(), 22.0);
    let ids: Vec<u64> = p.document.process().recipes().map(|r| r.id().raw()).collect();
    assert_eq!(ids, vec![7, 8, 9], "the recipe nobody uses is kept");
}

#[test]
fn saving_again_writes_the_same_meaning_and_no_library_file() {
    let original = bytes();
    let p = decode_container(&original).unwrap();
    let again = encode_container(&p).unwrap();
    let json = |b: &[u8]| serde_json::from_slice::<serde_json::Value>(&entry(b, "document.json")).unwrap();
    // The hand-written file spells out an empty `constraints`; the writer omits what is empty.
    let mut before = json(&original);
    assert_eq!(before["constraints"], serde_json::json!([]));
    before.as_object_mut().unwrap().remove("constraints");
    assert_eq!(json(&again), before, "nothing added, nothing lost, nothing renamed");
    let zip = zip::ZipArchive::new(std::io::Cursor::new(again)).unwrap();
    assert!(zip.file_names().all(|n| n.ok().is_none_or(|n| n != "library-links.json")), "no library behind these recipes");
    assert_eq!(project_hash(&decode_container(&encode_container(&p).unwrap()).unwrap()).unwrap(), project_hash(&p).unwrap());
}

fn run(s: &mut DocumentSession, payload: CommandPayload) -> burnburn_document::CommandOutcome {
    let cmd = DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload };
    s.apply(cmd).unwrap()
}

#[test]
fn editing_a_recipe_afterwards_changes_no_row_and_survives_save_and_reopen() {
    let p = decode_container(&bytes()).unwrap();
    let mut s = DocumentSession::from_snapshot(DocumentId::from_raw(1), p.document.clone());
    let seven = p.document.process().recipe(RecipeId::from_raw(7)).unwrap();
    let draft = RecipeDraft {
        name: seven.name().to_owned(),
        material: seven.material().to_owned(),
        thickness: Length::from_mm(3.0).unwrap(),
        machine: seven.machine().to_owned(),
        notes: "Derde meting".into(),
        mode: seven.mode(),
        settings: {
            let mut settings = seven.settings();
            settings.passes = burnburn_document::process::Passes::new(2).unwrap();
            settings
        },
    };
    run(&mut s, CommandPayload::Process(ProcessCommand::UpdateRecipe { recipe: RecipeId::from_raw(7), draft }));
    let edited = Project { document: (*s.snapshot()).clone(), ..p.clone() };
    let now = |q: &Project, row: u64| (op(q, row).recipe().map(|r| r.revision), op(q, row).settings().passes.get());
    assert_eq!(p.document.process().recipe(RecipeId::from_raw(7)).unwrap().revision(), 2);
    assert_eq!(edited.document.process().recipe(RecipeId::from_raw(7)).unwrap().revision(), 3);
    for row in [5, 6, 10, 11] {
        assert_eq!(now(&edited, row), now(&p, row), "row {row} keeps the revision and numbers it copied");
    }
    let back = decode_container(&encode_container(&edited).unwrap()).unwrap();
    assert_eq!(back.document.process(), edited.document.process());
    assert_eq!(back.document.process().recipe(RecipeId::from_raw(7)).unwrap().notes(), "Derde meting");
    assert_ne!(project_hash(&back).unwrap(), project_hash(&p).unwrap());
}
