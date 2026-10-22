//! W3-UI-bestanden, bundles (M032/M033, P07): choose what goes in, see what a bundle would change
//! before it changes anything, and take explicit decisions for conflicts and machine profiles.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

use burnburn_test_support::host::rig as common;

use std::path::{Path as FsPath, PathBuf};

use burnburn_document::process::{RecipeDraft, Speed};
use burnburn_io::art_library::{ArtFormat, ArtLibrary, NewArt};
use burnburn_io::bundle::{BundleContents, BundleTemplate, decode_bundle, encode_bundle};
use burnburn_io::fs::FileSystem;
use burnburn_io::material_library::MaterialLibrary;
use burnburn_job::{MachineFrame, MachineProfile, OriginCorner, PowerRatio};
use burnburn_presentation::{
    BundleChange, BundleConflict, BundleDialogViewModel, BundleItem, BundleKind, BundleMode, BundleProfileChange,
    CommandKey, FilesIntent, FilesMessage, HeadlessEditorHarness, MessageKey, OperationField, SelectMode,
    TemplateScope, UiIntent,
};
use burnburn_types::{DocumentId, Length, OperationId};
use common::{RUIDA, profile};

const SQUARE_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="10mm" height="10mm" viewBox="0 0 10 10"><rect id="sq" x="0" y="0" width="10" height="10" fill="none" stroke="black"/></svg>"#;

fn harness() -> HeadlessEditorHarness {
    let mut h = HeadlessEditorHarness::new();
    for dir in ["/lib", "/in", "/out", "/templates"] {
        h.fs().create_dir_all_for_test(FsPath::new(dir));
    }
    h.controller().set_library_dir(Some(PathBuf::from("/lib")));
    h.send(UiIntent::SetMachineProfile { profile: Box::new(profile(1)), ruida: RUIDA });
    h
}

fn layer(h: &mut HeadlessEditorHarness) -> (DocumentId, OperationId) {
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (0.0, 0.0), (20.0, 10.0));
    h.invoke(doc, CommandKey::ToolSelect);
    let id = h.document(doc).unwrap().objects[0].id;
    h.send(UiIntent::Select { document: doc, ids: vec![id], mode: SelectMode::Replace });
    h.send(UiIntent::AssignLayer { document: doc, slot: 1, add: false });
    let op = h.document(doc).unwrap().operations[0].id;
    h.send(UiIntent::SetOperationField {
        document: doc,
        operation: op,
        field: OperationField::Speed,
        text: "12".into(),
    });
    (doc, op)
}

fn dialog(h: &HeadlessEditorHarness, doc: DocumentId) -> BundleDialogViewModel {
    h.document(doc).unwrap().files.bundle.expect("the bundle dialog is open")
}

/// A local setup: two material entries, one art item, one template, the active profile.
fn filled(h: &mut HeadlessEditorHarness) -> (DocumentId, OperationId) {
    let (doc, op) = layer(h);
    h.send(UiIntent::Files(FilesIntent::OpenMaterialLibrary { document: doc }));
    h.send(UiIntent::Files(FilesIntent::OpenArtLibrary { document: doc }));
    h.run_tasks();
    for (name, mm) in [("Snijden", "3"), ("Graveren", "3")] {
        h.send(UiIntent::Files(FilesIntent::SaveMaterial {
            document: doc,
            operation: op,
            name: name.into(),
            material: "Berken".into(),
            thickness: mm.into(),
            notes: String::new(),
        }));
    }
    h.send(UiIntent::Files(FilesIntent::AddArtGroup { document: doc, name: "Vormen".into() }));
    h.fs().insert(FsPath::new("/in/vierkant.svg"), SQUARE_SVG.as_bytes().to_vec());
    h.send(UiIntent::Files(FilesIntent::AddArtFromFile {
        document: doc,
        path: PathBuf::from("/in/vierkant.svg"),
        group: "Vormen".into(),
        name: String::new(),
        dxf_unit: None,
    }));
    h.send(UiIntent::Files(FilesIntent::SaveAsTemplate {
        document: doc,
        path: PathBuf::from("/templates/plaat.burnburntemplate"),
        scope: TemplateScope::Everything,
    }));
    h.run_tasks();
    (doc, op)
}

#[test]
fn the_export_dialog_lists_what_can_go_in_and_nothing_is_selected_at_first() {
    let mut h = harness();
    let (doc, _) = filled(&mut h);
    h.send(UiIntent::Files(FilesIntent::OpenBundleExport { document: doc }));
    h.run_tasks();
    let BundleMode::Export(view) = dialog(&h, doc).mode else { panic!("export mode") };
    assert_eq!(view.materials.len(), 2);
    assert_eq!(view.art.len(), 1);
    assert_eq!(view.profiles.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), ["research-1500x900"]);
    assert_eq!(view.templates.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), ["plaat"]);
    assert!(view.materials.iter().chain(&view.art).all(|c| !c.selected), "the user chooses");
    h.send(UiIntent::Files(FilesIntent::ExportBundle {
        document: doc,
        path: PathBuf::from("/out/leeg.burnburnbundle"),
    }));
    h.run_tasks();
    assert_eq!(dialog(&h, doc).issue, Some(MessageKey::Files(FilesMessage::BundleNothingSelected)));
    assert!(!h.fs().exists(FsPath::new("/out/leeg.burnburnbundle")).unwrap());
}

/// The bundle holds exactly the selection.
#[test]
fn exporting_writes_exactly_the_chosen_items() {
    let mut h = harness();
    let (doc, _) = filled(&mut h);
    h.send(UiIntent::Files(FilesIntent::OpenBundleExport { document: doc }));
    h.run_tasks();
    let BundleMode::Export(view) = dialog(&h, doc).mode else { panic!() };
    h.send(UiIntent::Files(FilesIntent::ToggleBundleItem {
        document: doc,
        item: BundleItem::Material(view.materials[0].id),
        selected: true,
    }));
    h.send(UiIntent::Files(FilesIntent::SetBundleKind { document: doc, kind: BundleKind::Templates, selected: true }));
    h.send(UiIntent::Files(FilesIntent::SetBundleKind { document: doc, kind: BundleKind::Profiles, selected: true }));
    h.send(UiIntent::Files(FilesIntent::ExportBundle { document: doc, path: PathBuf::from("/out/a.burnburnbundle") }));
    h.run_tasks();
    let BundleMode::Export(view) = dialog(&h, doc).mode else { panic!() };
    assert_eq!(view.written, Some(PathBuf::from("/out/a.burnburnbundle")));
    let bytes = h.fs().read(FsPath::new("/out/a.burnburnbundle"), 1 << 28).unwrap();
    let bundle = decode_bundle(&bytes).unwrap();
    assert_eq!(bundle.materials.as_ref().map(|m| m.entries().len()), Some(1));
    assert!(bundle.art.is_none(), "art was not chosen");
    assert_eq!(bundle.profiles.len(), 1);
    assert_eq!(bundle.templates.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["plaat"]);
}

fn entry(name: &str, speed: f64) -> RecipeDraft {
    let cut = common::cut();
    let mut settings = cut.settings;
    settings.speed = Speed::mm_per_s(speed).unwrap();
    RecipeDraft {
        name: name.into(),
        material: "Berken".into(),
        thickness: Length::from_mm(3.0).unwrap(),
        // The same machine as the active profile, which is what a local entry is stored for.
        machine: "research-1500x900".into(),
        notes: String::new(),
        mode: cut.mode,
        settings,
    }
}

fn other_profile() -> MachineProfile {
    MachineProfile::new(
        "research-1500x900",
        2,
        MachineFrame::new(1500.0, 900.0, OriginCorner::BottomLeft).unwrap(),
        600.0,
        400.0,
        PowerRatio::new(0.0).unwrap(),
        PowerRatio::new(1.0).unwrap(),
    )
    .unwrap()
}

/// A bundle with `Snijden` at another speed (conflict), a new `Boren`, one new art item and a
/// profile of the active machine with another maximum speed.
fn incoming(h: &HeadlessEditorHarness) -> PathBuf {
    let mut materials = MaterialLibrary::new("bundel");
    materials.add(entry("Snijden", 30.0)).unwrap();
    materials.add(entry("Boren", 5.0)).unwrap();
    let mut art = ArtLibrary::new("bundel-art");
    art.add_group("Nieuw").unwrap();
    art.add_item(NewArt {
        name: "Vierkant",
        group: "Nieuw",
        tags: vec![],
        format: ArtFormat::Svg,
        bytes: SQUARE_SVG.as_bytes(),
        dxf_scale: None,
    })
    .unwrap();
    let profiles = [other_profile()];
    let bytes = encode_bundle(&BundleContents {
        materials: Some(&materials),
        art: Some(&art),
        profiles: &profiles,
        templates: &[],
    })
    .unwrap();
    let path = PathBuf::from("/in/bundel.burnburnbundle");
    h.fs().insert(&path, bytes);
    path
}

/// P07: the profile difference is shown with its field names before anything is replaced, and
/// the profile stays as it is unless the user chose to replace it.
#[test]
fn package_import_shows_profile_diff_before_replace() {
    let mut h = harness();
    let (doc, op) = layer(&mut h);
    h.send(UiIntent::Files(FilesIntent::OpenMaterialLibrary { document: doc }));
    h.run_tasks();
    h.send(UiIntent::Files(FilesIntent::SaveMaterial {
        document: doc,
        operation: op,
        name: "Snijden".into(),
        material: "Berken".into(),
        thickness: "3".into(),
        notes: String::new(),
    }));
    let path = incoming(&h);

    h.send(UiIntent::Files(FilesIntent::OpenBundleImport { document: doc, path }));
    assert!(dialog(&h, doc).loading, "the bundle is read on a worker");
    h.run_tasks();
    let BundleMode::Import(view) = dialog(&h, doc).mode else { panic!("import mode") };
    // The profile: same machine, other maximum speed — named, not applied.
    assert_eq!(view.profiles.len(), 1);
    let BundleProfileChange::Differs { fields } = &view.profiles[0].change else { panic!("{:?}", view.profiles[0]) };
    assert!(fields.iter().any(|f| f == "max_speed_mm_s"), "{fields:?}");
    assert_eq!((view.profiles[0].bundle_revision, view.profiles[0].local_revision), (2, Some(1)));
    assert!(!view.profiles[0].replace, "replacing is never the default");
    // Materials: `Snijden` conflicts (other speed), `Boren` is new.
    let changes: Vec<_> = view.materials.iter().map(|m| (m.label.contains("Snijden"), m.change)).collect();
    assert!(
        changes.contains(&(true, BundleChange::Conflict)) && changes.contains(&(false, BundleChange::New)),
        "{changes:?}"
    );
    assert!(view.has_conflicts);
    assert_eq!(view.art.len(), 1);
    // Nothing happened yet.
    assert_eq!(h.controller().known_profiles()[0].max_speed_mm_s(), 1000.0);
    let materials = h.document(doc).unwrap().files.material.expect("panel");
    assert_eq!(materials.entries.len(), 1, "the library is unchanged until Apply");

    // Apply with the defaults: new things come in, the conflicting entry and the profile stay.
    h.send(UiIntent::Files(FilesIntent::ApplyBundleImport { document: doc }));
    h.run_tasks();
    let BundleMode::Import(view) = dialog(&h, doc).mode else { panic!() };
    let report = view.report.expect("a report");
    assert_eq!((report.materials_added, report.materials_replaced, report.skipped), (1, 0, 1));
    assert_eq!(report.art_added, 1);
    assert_eq!(report.profiles_pending, ["research-1500x900"]);
    assert_eq!(h.controller().known_profiles()[0].max_speed_mm_s(), 1000.0, "the differing profile was not replaced");
}

#[test]
fn replacing_a_profile_is_an_explicit_choice_and_takes_effect_on_the_active_machine() {
    let mut h = harness();
    let (doc, _) = layer(&mut h);
    let path = incoming(&h);
    h.send(UiIntent::Files(FilesIntent::OpenBundleImport { document: doc, path }));
    h.run_tasks();
    h.send(UiIntent::Files(FilesIntent::SetBundleProfileReplace {
        document: doc,
        profile: "research-1500x900".into(),
        replace: true,
    }));
    let BundleMode::Import(view) = dialog(&h, doc).mode else { panic!() };
    assert!(view.profiles[0].replace);
    h.send(UiIntent::Files(FilesIntent::ApplyBundleImport { document: doc }));
    h.run_tasks();
    assert_eq!(h.controller().known_profiles().len(), 1);
    assert_eq!(h.controller().known_profiles()[0].max_speed_mm_s(), 600.0);
    let BundleMode::Import(view) = dialog(&h, doc).mode else { panic!() };
    assert_eq!(view.report.unwrap().profiles_replaced, 1);
}

#[test]
fn the_conflict_choice_decides_what_happens_to_a_differing_entry() {
    for (choice, speeds) in [
        (BundleConflict::Skip, vec!["12"]),
        (BundleConflict::Replace, vec!["30"]),
        (BundleConflict::KeepBoth, vec!["12", "30"]),
    ] {
        let mut h = harness();
        let (doc, op) = layer(&mut h);
        h.send(UiIntent::Files(FilesIntent::OpenMaterialLibrary { document: doc }));
        h.run_tasks();
        h.send(UiIntent::Files(FilesIntent::SaveMaterial {
            document: doc,
            operation: op,
            name: "Snijden".into(),
            material: "Berken".into(),
            thickness: "3".into(),
            notes: String::new(),
        }));
        let path = incoming(&h);
        h.send(UiIntent::Files(FilesIntent::OpenBundleImport { document: doc, path }));
        h.run_tasks();
        h.send(UiIntent::Files(FilesIntent::SetBundleConflict { document: doc, choice }));
        h.send(UiIntent::Files(FilesIntent::ApplyBundleImport { document: doc }));
        h.run_tasks();
        let p = h.document(doc).unwrap().files.material.expect("panel");
        let mut got: Vec<String> =
            p.entries.iter().filter(|e| e.name.starts_with("Snijden")).map(|e| e.speed.clone()).collect();
        got.sort();
        assert_eq!(got, speeds, "{choice:?}");
    }
}

#[test]
fn a_file_that_is_no_bundle_changes_nothing() {
    let mut h = harness();
    let (doc, _) = layer(&mut h);
    h.fs().insert(FsPath::new("/in/kapot.burnburnbundle"), b"PK nonsense".to_vec());
    h.send(UiIntent::Files(FilesIntent::OpenBundleImport {
        document: doc,
        path: PathBuf::from("/in/kapot.burnburnbundle"),
    }));
    h.run_tasks();
    assert_eq!(dialog(&h, doc).issue, Some(MessageKey::Files(FilesMessage::BundleUnreadable)));
    h.send(UiIntent::Files(FilesIntent::ApplyBundleImport { document: doc }));
    h.run_tasks();
    assert!(h.controller().known_profiles().len() <= 1);
    h.send(UiIntent::Files(FilesIntent::CloseBundle { document: doc }));
    assert!(h.document(doc).unwrap().files.bundle.is_none());
}

/// Templates from a bundle are written to the templates folder only when the user applies the
/// import, and a different local one needs the conflict choice.
#[test]
fn bundle_templates_are_written_to_the_templates_folder_on_apply() {
    let mut h = harness();
    let (doc, _) = layer(&mut h);
    h.send(UiIntent::Files(FilesIntent::SaveAsTemplate {
        document: doc,
        path: PathBuf::from("/templates/bron.burnburntemplate"),
        scope: TemplateScope::Everything,
    }));
    h.run_tasks();
    let template = h.fs().read(FsPath::new("/templates/bron.burnburntemplate"), 1 << 24).unwrap();
    let bytes = encode_bundle(&BundleContents {
        templates: &[BundleTemplate { name: "uit-bundel".into(), bytes: template }],
        ..BundleContents::default()
    })
    .unwrap();
    h.fs().insert(FsPath::new("/in/t.burnburnbundle"), bytes);
    h.send(UiIntent::Files(FilesIntent::OpenBundleImport {
        document: doc,
        path: PathBuf::from("/in/t.burnburnbundle"),
    }));
    h.run_tasks();
    let BundleMode::Import(view) = dialog(&h, doc).mode else { panic!() };
    assert_eq!(view.templates.len(), 1);
    assert!(!h.fs().exists(FsPath::new("/lib/templates/uit-bundel.burnburntemplate")).unwrap(), "not before Apply");
    h.send(UiIntent::Files(FilesIntent::ApplyBundleImport { document: doc }));
    h.run_tasks();
    assert!(h.fs().exists(FsPath::new("/lib/templates/uit-bundel.burnburntemplate")).unwrap());
    let BundleMode::Import(view) = dialog(&h, doc).mode else { panic!() };
    assert_eq!(view.report.unwrap().templates_written, 1);
}
