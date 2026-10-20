//! W2-L6, Material Library (C08): materiaal + dikte → bewerkingsinstellingen; a recipe copied into
//! a project is a snapshot, a library update never changes existing job settings.

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers and let-else in tests

mod common;

use std::path::PathBuf;

use burnburn_document::process::{
    FillSettings, Kerf, OperationMode, Passes, PowerPercent, PowerRange, ProcessCommand, ProcessSettings, RecipeDraft,
    ScanAxis, ScanDirection, Speed,
};
use burnburn_document::{CommandPayload, DocumentCommand, DocumentSession};
use burnburn_io::material_library::{
    LibraryError, LibraryQuery, LinkStatus, MATERIAL_LIBRARY_SCHEMA, MaterialLibrary, decode_material_library,
    encode_material_library, link_for, link_status, load_material_library, refresh_command, save_material_library,
};
use burnburn_io::memory::MemoryFileSystem;
use burnburn_io::{LibraryLink, load_project, save_project};
use burnburn_types::{DocumentId, Length, RequestId, StableId};

fn settings(speed: f64, power: f64) -> ProcessSettings {
    ProcessSettings {
        speed: Speed::mm_per_s(speed).unwrap(),
        power: PowerRange::new(PowerPercent::new(power).unwrap(), PowerPercent::new(power).unwrap()).unwrap(),
        passes: Passes::new(1).unwrap(),
        air_assist: true,
    }
}

fn cut(material: &str, thickness: f64, machine: &str, speed: f64, power: f64) -> RecipeDraft {
    RecipeDraft {
        name: format!("{material} {thickness} mm snijden"),
        material: material.into(),
        thickness: Length::from_mm(thickness).unwrap(),
        machine: machine.into(),
        notes: "gemeten 2026-10-20".into(),
        mode: OperationMode::Line { kerf: Kerf::NONE },
        settings: settings(speed, power),
    }
}

fn library() -> MaterialLibrary {
    let mut lib = MaterialLibrary::new("werkplaats");
    lib.add(cut("Berken multiplex", 3.0, "omtech-60w", 12.0, 70.0)).unwrap();
    lib.add(cut("Berken multiplex", 6.0, "omtech-60w", 6.0, 85.0)).unwrap();
    lib.add(cut("Acryl", 3.0, "omtech-60w", 10.0, 60.0)).unwrap();
    lib.add(cut("Acryl", 3.0, "laser-b", 14.0, 45.0)).unwrap();
    lib
}

fn run(s: &mut DocumentSession, payload: CommandPayload) -> burnburn_document::CommandOutcome {
    let cmd = DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload };
    s.apply(cmd).unwrap()
}

#[test]
fn entries_are_found_by_material_thickness_and_machine_in_a_stable_order() {
    let lib = library();
    assert_eq!(lib.materials(), ["Acryl", "Berken multiplex"], "sorted, unique");
    let names = |q: &LibraryQuery<'_>| -> Vec<(String, f64, String)> {
        lib.find(q)
            .iter()
            .map(|e| (e.draft().material.clone(), e.draft().thickness.mm(), e.draft().machine.clone()))
            .collect()
    };
    let birch = names(&LibraryQuery { material: Some("Berken multiplex"), ..LibraryQuery::default() });
    assert_eq!(
        birch,
        [("Berken multiplex".into(), 3.0, "omtech-60w".into()), ("Berken multiplex".into(), 6.0, "omtech-60w".into())]
    );
    // Thickness matches within 0.05 mm; the machine narrows further.
    let acrylic = names(&LibraryQuery { material: Some("Acryl"), thickness_mm: Some(3.02), machine: Some("laser-b") });
    assert_eq!(acrylic, [("Acryl".into(), 3.0, "laser-b".into())]);
    assert!(names(&LibraryQuery { thickness_mm: Some(4.0), ..LibraryQuery::default() }).is_empty());
    // No query: everything, ordered by material, thickness, machine (not insertion order).
    assert_eq!(names(&LibraryQuery::default()).len(), 4);
    assert_eq!(names(&LibraryQuery::default())[0].0, "Acryl");
}

#[test]
fn update_bumps_the_revision_and_remove_forgets_the_entry() {
    let mut lib = library();
    let id = lib.entries()[0].id();
    assert_eq!(lib.get(id).unwrap().revision(), 1);
    assert_eq!(lib.update(id, cut("Berken multiplex", 3.0, "omtech-60w", 14.0, 72.0)).unwrap(), 2);
    assert_eq!(lib.get(id).unwrap().draft().settings.speed.get_mm_per_s(), 14.0);
    assert_eq!(lib.update(999, cut("x", 1.0, "m", 1.0, 1.0)), Err(LibraryError::UnknownEntry(999)));
    assert!(lib.remove(id) && !lib.remove(id));
    // Ids are never reused: a new entry does not take the removed entry's id.
    let again = lib.add(cut("Berken multiplex", 3.0, "omtech-60w", 12.0, 70.0)).unwrap();
    assert_ne!(again, id);
}

#[test]
fn image_mode_and_invalid_drafts_are_refused() {
    let mut lib = MaterialLibrary::new("x");
    let mut d = cut("Acryl", 3.0, "m", 10.0, 50.0);
    d.name = String::new();
    assert!(matches!(lib.add(d), Err(LibraryError::InvalidDraft(_))), "an entry needs a name");
    let mut d = cut("", 3.0, "m", 10.0, 50.0);
    d.material.clear();
    assert!(matches!(lib.add(d), Err(LibraryError::InvalidDraft(_))), "an entry needs a material");
    let image = burnburn_document::process::ImageSettings::new(
        burnburn_io::asset_hash(b"x"),
        Length::from_mm(0.1).unwrap(),
        Length::from_mm(0.1).unwrap(),
        burnburn_document::process::ImageRender::Grayscale,
    )
    .unwrap();
    let mut d = cut("Acryl", 3.0, "m", 10.0, 50.0);
    d.mode = OperationMode::Image(image);
    assert_eq!(lib.add(d), Err(LibraryError::ImageModeNotAllowed), "an image source belongs to one project");
}

/// C08: `library_update_does_not_change_existing_job_settings`.
#[test]
fn library_update_does_not_change_existing_job_settings() {
    let mut lib = library();
    let entry_id = lib.find(&LibraryQuery {
        material: Some("Berken multiplex"),
        thickness_mm: Some(3.0),
        ..LibraryQuery::default()
    })[0]
        .id();

    // A project with one operation; the recipe is copied from the library entry (revision 1) and applied.
    let project = common::sample_project();
    let mut session = DocumentSession::from_snapshot(DocumentId::from_raw(1), project.document.clone());
    let operation = project.document.process().operations().next().unwrap().id();
    let out = run(&mut session, CommandPayload::Process(lib.get(entry_id).unwrap().add_recipe_command()));
    let recipe = out.changes.recipes[0];
    let link = link_for(&lib, entry_id, recipe).unwrap();
    assert_eq!(
        (link.library.as_str(), link.entry, link.revision, link.recipe),
        ("werkplaats", entry_id, 1, recipe.raw())
    );
    run(&mut session, CommandPayload::Process(ProcessCommand::ApplyRecipe { recipe, operation }));
    let applied = session.snapshot().process().operation(operation).unwrap().clone();
    assert_eq!(applied.settings().speed.get_mm_per_s(), 12.0);
    assert_eq!(link_status(&link, &lib), LinkStatus::UpToDate);
    let before = session.snapshot();

    // The library entry is improved: the library says an update exists, the project does not change.
    lib.update(entry_id, cut("Berken multiplex", 3.0, "omtech-60w", 20.0, 55.0)).unwrap();
    assert_eq!(link_status(&link, &lib), LinkStatus::UpdateAvailable { applied: 1, latest: 2 });
    assert!(session.snapshot().same_content(&before), "the document is untouched by a library update");
    let now = session.snapshot().process().operation(operation).unwrap().clone();
    assert_eq!((now.settings(), now.mode()), (applied.settings(), applied.mode()), "job settings are bit-identical");

    // Only explicit commands bring the new values in: update the project recipe, then apply it again.
    let ProcessCommand::UpdateRecipe { recipe: target, .. } = refresh_command(&link, &lib, recipe).unwrap() else {
        panic!("expected an UpdateRecipe")
    };
    assert_eq!(target, recipe);
    run(&mut session, CommandPayload::Process(refresh_command(&link, &lib, recipe).unwrap()));
    let untouched = session.snapshot().process().operation(operation).unwrap().settings();
    assert_eq!(untouched, applied.settings(), "updating the recipe alone does not rewrite operations either");
    run(&mut session, CommandPayload::Process(ProcessCommand::ApplyRecipe { recipe, operation }));
    assert_eq!(session.snapshot().process().operation(operation).unwrap().settings().speed.get_mm_per_s(), 20.0);

    // Removing the entry leaves the project alone; the link just reports it.
    let doc_before = session.snapshot();
    lib.remove(entry_id);
    assert_eq!(link_status(&link, &lib), LinkStatus::EntryMissing);
    assert!(session.snapshot().same_content(&doc_before));
    assert_eq!(link_status(&link, &MaterialLibrary::new("andere")), LinkStatus::OtherLibrary);
}

#[test]
fn libraries_roundtrip_exactly_through_the_filesystem() {
    let mut lib = library();
    let fill = FillSettings::new(
        Length::from_mm(0.1 + 0.2).unwrap(),
        ScanAxis::X,
        ScanDirection::Bidirectional,
        Length::from_mm(1.5).unwrap(),
    )
    .unwrap();
    lib.add(RecipeDraft {
        mode: OperationMode::Fill(fill),
        name: "Graveren".into(),
        settings: settings(0.1 + 0.2, 33.3),
        ..cut("Anodiseeraluminium", 1.0, "fiber", 1.0, 1.0)
    })
    .unwrap();
    let fs = MemoryFileSystem::new();
    let path = PathBuf::from("/user/materialen.json");
    save_material_library(&fs, &path, &lib).unwrap();
    let loaded = load_material_library(&fs, &path).unwrap();
    assert_eq!(loaded, lib, "exact roundtrip, including 0.1 + 0.2 and the revision counters");
    assert_eq!(
        encode_material_library(&loaded).unwrap(),
        encode_material_library(&lib).unwrap(),
        "deterministic bytes"
    );
}

#[test]
fn newer_unknown_and_damaged_libraries_are_typed_errors() {
    let good = encode_material_library(&library()).unwrap();
    let edit = |f: &dyn Fn(&mut serde_json::Value)| -> Vec<u8> {
        let mut v: serde_json::Value = serde_json::from_slice(&good).unwrap();
        f(&mut v);
        serde_json::to_vec(&v).unwrap()
    };
    use burnburn_io::LoadError;
    assert_eq!(
        decode_material_library(&edit(&|v| v["schema"] = (MATERIAL_LIBRARY_SCHEMA + 1).into())),
        Err(LoadError::NewerSchema { found: MATERIAL_LIBRARY_SCHEMA + 1, supported: MATERIAL_LIBRARY_SCHEMA })
    );
    assert!(matches!(decode_material_library(&edit(&|v| v["schema"] = 0.into())), Err(LoadError::InvalidValue { .. })));
    assert!(matches!(
        decode_material_library(&edit(&|v| v["format"] = "iets-anders".into())),
        Err(LoadError::NotAProject)
    ));
    assert!(matches!(
        decode_material_library(&edit(&|v| v["entries"][0]["mystery"] = 1.into())),
        Err(LoadError::UnknownField { .. })
    ));
    assert!(
        matches!(
            decode_material_library(&edit(&|v| v["entries"][1]["id"] = v["entries"][0]["id"].clone())),
            Err(LoadError::InvalidValue { .. })
        ),
        "duplicate ids"
    );
    assert!(matches!(
        decode_material_library(&edit(&|v| v["entries"][0]["thickness_mm"] = (-1.0).into())),
        Err(LoadError::InvalidValue { .. })
    ));
    assert!(
        matches!(decode_material_library(&edit(&|v| v["next_id"] = 1.into())), Err(LoadError::InvalidValue { .. })),
        "next_id must exceed every entry id"
    );
    assert!(decode_material_library(b"not json").is_err() && decode_material_library(b"").is_err());
}

#[test]
fn a_project_keeps_its_library_links_and_a_project_without_links_gains_no_entry() {
    let fs = MemoryFileSystem::new();
    let mut project = common::sample_project();
    let plain = burnburn_io::encode_container(&project).unwrap();
    assert!(!String::from_utf8_lossy(&plain).contains("library-links.json"));
    // Add a recipe so the link has something to point at.
    let mut session = DocumentSession::from_snapshot(DocumentId::from_raw(1), project.document.clone());
    let out = run(&mut session, CommandPayload::Process(library().entries()[0].add_recipe_command()));
    project.document = (*session.snapshot()).clone();
    let recipe = out.changes.recipes[0];
    project.library_links.push(LibraryLink {
        recipe: recipe.raw(),
        library: "werkplaats".into(),
        entry: 1,
        revision: 3,
    });
    let hash_with = burnburn_io::project_hash(&project).unwrap();
    let path = PathBuf::from("/w/p.burnburn");
    save_project(&fs, &path, &project).unwrap();
    let loaded = load_project(&fs, &path).unwrap();
    assert_eq!(loaded.library_links, project.library_links);
    // Provenance is not part of the canonical project hash.
    project.library_links.clear();
    assert_eq!(burnburn_io::project_hash(&project).unwrap(), hash_with);
    // A link to a recipe the project does not have is rejected on load.
    let mut bad = loaded.clone();
    bad.library_links[0].recipe = 999_999;
    let bytes = burnburn_io::encode_container(&bad).unwrap();
    assert!(matches!(burnburn_io::decode_container(&bytes), Err(burnburn_io::LoadError::InvalidValue { .. })));
}

#[test]
fn a_library_restored_from_a_backup_is_reported_as_older_not_as_up_to_date() {
    let mut lib = library();
    let id = lib.entries()[0].id();
    lib.update(id, cut("Berken multiplex", 3.0, "omtech-60w", 13.0, 71.0)).unwrap();
    let link = link_for(&lib, id, burnburn_types::StableId::from_raw(7)).unwrap();
    assert_eq!(link.revision, 2);
    assert_eq!(link_status(&link, &library()), LinkStatus::LibraryOlder { applied: 2, latest: 1 });
}
