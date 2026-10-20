//! IM28 (P07/P08): machine-profile snapshots frozen in the project, frozen production revisions
//! ("opnieuw maken als revisie X") and the project search over a folder.

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers outside #[test] fns; let-else on expected variants

use std::path::Path;

use burnburn_document::{CommandPayload, DocumentCommand, DocumentSession};
use burnburn_geometry::{Point, Rect};
use burnburn_io::memory::MemoryFileSystem;
use burnburn_io::search::{MatchField, index_projects};
use burnburn_io::{ProfileComparison, Project, ProjectUuid, compare_profile, load_project, project_hash, save_project};
use burnburn_job::{Feature, MachineCapabilities, MachineFrame, MachineProfile, OriginCorner, PowerRatio, Support};
use burnburn_types::{DocumentId, Length, RequestId, StableId};

fn profile(id: &str, speed: f64) -> MachineProfile {
    let frame = MachineFrame::new(1500.0, 900.0, OriginCorner::TopRight).unwrap();
    let caps = MachineCapabilities::conservative()
        .with(Feature::ZStep, Support::Unverified { reason: "nog niet gemeten".into() });
    MachineProfile::new(id, 3, frame, speed, 300.0, PowerRatio::new(0.1).unwrap(), PowerRatio::new(0.8).unwrap())
        .unwrap()
        .with_capabilities(caps)
        .unwrap()
}

fn session(name: &str) -> DocumentSession {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let rect = Rect::new(Point::new(0.0, 0.0), Length::from_mm(10.0).unwrap(), Length::from_mm(10.0).unwrap()).unwrap();
    edit(&mut s, CommandPayload::CreateRectangle { rect, name: name.into() });
    s
}

fn edit(s: &mut DocumentSession, payload: CommandPayload) {
    let cmd = DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload };
    s.apply(cmd).unwrap();
}

fn roundtrip(p: &Project) -> Project {
    let fs = MemoryFileSystem::new();
    fs.create_dir_all_for_test(Path::new("/p"));
    save_project(&fs, Path::new("/p/x.burnburn"), p).unwrap();
    load_project(&fs, Path::new("/p/x.burnburn")).unwrap()
}

#[test]
fn profile_snapshot_is_frozen_in_project_and_survives_reopen() {
    let mut p = Project::new(ProjectUuid::from_bytes(*b"burnburn-profile"), (*session("A").snapshot()).clone());
    let without = project_hash(&p).unwrap();
    p.profiles.push(profile("ruida-1500x900", 400.0));
    assert_ne!(project_hash(&p).unwrap(), without, "the frozen profile is part of the meaning");
    let back = roundtrip(&p);
    let (a, b) = (&back.profiles[0], &p.profiles[0]);
    assert_eq!(a.revision(), b.revision(), "same id, counter and content hash after reopen");
    assert_eq!(compare_profile(a, b), ProfileComparison::Identical);
    for f in Feature::ALL {
        assert_eq!(a.capabilities().support(f), b.capabilities().support(f));
    }
    assert_eq!(project_hash(&back).unwrap(), project_hash(&p).unwrap());
}

#[test]
fn embedded_profile_is_compared_with_the_local_one_never_applied() {
    let frozen = profile("ruida-1500x900", 400.0);
    assert_eq!(compare_profile(&frozen, &profile("ruida-1500x900", 400.0)), ProfileComparison::Identical);
    assert_eq!(
        compare_profile(&frozen, &profile("ruida-1500x900", 350.0)),
        ProfileComparison::Differs { fields: vec!["max_speed_mm_s"] }
    );
    assert_eq!(compare_profile(&frozen, &profile("andere-laser", 400.0)), ProfileComparison::OtherMachine);
}

#[test]
fn frozen_revision_survives_later_edits_and_reopen() {
    let mut s = session("Plaat");
    let mut p = Project::new(ProjectUuid::from_bytes(*b"burnburn-revisio"), (*s.snapshot()).clone());
    let n = p.freeze_revision("Productie 18 okt").unwrap();
    assert_eq!(n, 1);
    let frozen_hash = project_hash(&p.recreate_revision_project(n).unwrap()).unwrap();
    // The design moves on; the frozen revision does not.
    let rect = Rect::new(Point::new(50.0, 0.0), Length::from_mm(5.0).unwrap(), Length::from_mm(5.0).unwrap()).unwrap();
    edit(&mut s, CommandPayload::CreateRectangle { rect, name: "Extra".into() });
    p.document = (*s.snapshot()).clone();
    let back = roundtrip(&p);
    assert_eq!(back.document.len(), 2, "current design");
    let rev = back.revision(1).unwrap();
    assert_eq!((rev.number, rev.label.as_str(), rev.project_hash), (1, "Productie 18 okt", frozen_hash));
    let again = back.recreate_revision_project(1).unwrap();
    assert_eq!(again.document.len(), 1, "the old production revision, next to the current design");
    assert_eq!(project_hash(&again).unwrap(), frozen_hash, "reproduces the frozen meaning exactly");
    assert!(back.recreate_revision_project(2).is_none());
}

#[test]
fn project_search_finds_by_file_name_object_material_and_revision() {
    let fs = MemoryFileSystem::new();
    fs.create_dir_all_for_test(Path::new("/werk"));
    let mut sa = session("Deurbord");
    let plate = burnburn_document::structure::Plate {
        name: "Plaat 1".into(),
        width: Length::from_mm(600.0).unwrap(),
        height: Length::from_mm(400.0).unwrap(),
        material: "Berken multiplex".into(),
        thickness: None,
    };
    edit(&mut sa, CommandPayload::Structure(burnburn_document::structure::StructureCommand::CreatePlate { plate }));
    let mut a = Project::new(ProjectUuid::from_bytes(*b"burnburn-search1"), (*sa.snapshot()).clone());
    a.freeze_revision("Order 4711").unwrap();
    save_project(&fs, Path::new("/werk/deurbord.burnburn"), &a).unwrap();
    let b = Project::new(ProjectUuid::from_bytes(*b"burnburn-search2"), (*session("Onderzetter").snapshot()).clone());
    save_project(&fs, Path::new("/werk/onderzetters.burnburn"), &b).unwrap();
    fs.insert(Path::new("/werk/kapot.burnburn"), b"not a zip".to_vec());
    fs.insert(Path::new("/werk/notities.txt"), b"populier".to_vec());

    let index = index_projects(&fs, Path::new("/werk")).unwrap();
    assert_eq!(index.entries().len(), 2);
    assert_eq!(index.issues().len(), 1, "an unreadable project is reported, not skipped silently");
    assert_eq!(index.issues()[0].path, Path::new("/werk/kapot.burnburn"));

    let hits = index.search("DEURBORD");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].fields, vec![MatchField::FileName, MatchField::Object]);
    let hits = index.search("4711");
    assert_eq!((hits.len(), hits[0].fields.clone()), (1, vec![MatchField::Revision]));
    assert_eq!(index.search("multiplex")[0].fields, vec![MatchField::Material]);
    assert_eq!(index.search("onderzet")[0].entry.path, Path::new("/werk/onderzetters.burnburn"));
    assert!(index.search("populier").is_empty(), "only projects are indexed");
    assert!(index.search("").is_empty());
}

/// A frozen revision whose stored document no longer matches its recorded hash is refused on
/// load: an old production revision is reproduced exactly or not at all.
#[test]
fn tampered_frozen_revision_is_refused() {
    use std::io::{Read, Write};
    let mut p = Project::new(ProjectUuid::from_bytes(*b"burnburn-revisio"), (*session("Plaat").snapshot()).clone());
    p.freeze_revision("Productie").unwrap();
    let bytes = burnburn_io::encode_container(&p).unwrap();
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).unwrap();
        let name = f.name().unwrap().into_owned();
        let mut data = Vec::new();
        f.read_to_end(&mut data).unwrap();
        if name == "revisions.json" {
            data = String::from_utf8(data).unwrap().replace("\"Plaat\"", "\"Plaaq\"").into_bytes();
        }
        out.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        out.write_all(&data).unwrap();
    }
    let tampered = out.finish().unwrap().into_inner();
    let err = burnburn_io::decode_container(&tampered).unwrap_err();
    assert!(matches!(err, burnburn_io::LoadError::InvalidValue { ref path, .. } if path == "revisions[0]"), "{err:?}");
}

fn rotary_profile(kind: burnburn_job::RotaryKind, mirror: bool) -> burnburn_job::MachineProfile {
    let rotary = burnburn_job::RotaryConfig::new(kind, 8000, 20.0, mirror).unwrap();
    profile("ruida-1500x900", 400.0).with_rotary(rotary).unwrap()
}

#[test]
fn a_frozen_profile_with_a_rotary_survives_reopen_and_can_return_to_the_flat_bed() {
    use burnburn_job::RotaryKind;
    for (kind, mirror) in [
        (RotaryKind::Chuck { object_diameter_mm: 63.5 }, false),
        (RotaryKind::Roller { roller_diameter_mm: 30.0 }, true),
    ] {
        let mut p = Project::new(ProjectUuid::from_bytes(*b"burnburn-rotary!"), (*session("A").snapshot()).clone());
        let turned = rotary_profile(kind, mirror);
        p.profiles.push(turned.clone());
        let back = roundtrip(&p);
        let got = &back.profiles[0];
        assert_eq!(got, &turned, "rotary, flat-bed frame and content hash all come back");
        assert_eq!(got.clone().without_rotary(), profile("ruida-1500x900", 400.0));
        assert_eq!(project_hash(&back).unwrap(), project_hash(&p).unwrap());
    }
}

#[test]
fn comparing_profiles_names_the_rotary() {
    let flat = profile("ruida-1500x900", 400.0);
    let turned = rotary_profile(burnburn_job::RotaryKind::Roller { roller_diameter_mm: 30.0 }, false);
    assert_eq!(compare_profile(&turned, &flat), ProfileComparison::Differs { fields: vec!["rotary"] });
    let mirrored = rotary_profile(burnburn_job::RotaryKind::Roller { roller_diameter_mm: 30.0 }, true);
    assert_eq!(compare_profile(&turned, &mirrored), ProfileComparison::Differs { fields: vec!["rotary"] });
}
