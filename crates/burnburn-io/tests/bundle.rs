//! W2-L6, Bundles (File ▸ Bundles ▸ Export/Import Bundle, P07): selected materials, art, machine
//! profiles and templates travel in one file. Importing first *shows* what would change; machine
//! profiles are never replaced unless the user names them.

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers and let-else in tests

mod common;

use burnburn_document::process::{
    Kerf, OperationMode, Passes, PowerPercent, PowerRange, ProcessSettings, RecipeDraft, Speed,
};
use burnburn_io::art_library::{ArtFormat, ArtLibrary, NewArt};
use burnburn_io::bundle::{
    BUNDLE_SCHEMA, Bundle, BundleContents, BundleTemplate, Change, ConflictChoice, ImportChoices, ImportTarget,
    LocalState, ProfileChange, apply_import, decode_bundle, encode_bundle, preview_import,
};
use burnburn_io::material_library::MaterialLibrary;
use burnburn_io::template::{TemplateContent, template_of};
use burnburn_io::{LoadError, encode_container};
use burnburn_job::{Feature, MachineCapabilities, MachineFrame, MachineProfile, OriginCorner, PowerRatio, Support};
use burnburn_types::Length;

const STAR: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="20mm" height="20mm" viewBox="0 0 20 20"><path d="M10 0 L20 20 L0 20 Z"/></svg>"#;
const STAR2: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="20mm" height="20mm" viewBox="0 0 20 20"><path d="M10 0 L20 10 L10 20 L0 10 Z"/></svg>"#;

fn profile(id: &str, speed: f64) -> MachineProfile {
    let frame = MachineFrame::new(1500.0, 900.0, OriginCorner::TopRight).unwrap();
    let caps = MachineCapabilities::conservative()
        .with(Feature::ZStep, Support::Unverified { reason: "nog niet gemeten".into() });
    MachineProfile::new(id, 3, frame, speed, 300.0, PowerRatio::new(0.1).unwrap(), PowerRatio::new(0.8).unwrap())
        .unwrap()
        .with_capabilities(caps)
        .unwrap()
}

fn recipe(material: &str, thickness: f64, speed: f64) -> RecipeDraft {
    let p = PowerPercent::new(60.0).unwrap();
    RecipeDraft {
        name: format!("{material} {thickness}"),
        material: material.into(),
        thickness: Length::from_mm(thickness).unwrap(),
        machine: "omtech-60w".into(),
        notes: String::new(),
        mode: OperationMode::Line { kerf: Kerf::NONE },
        settings: ProcessSettings {
            speed: Speed::mm_per_s(speed).unwrap(),
            power: PowerRange::new(p, p).unwrap(),
            passes: Passes::new(1).unwrap(),
            air_assist: false,
        },
    }
}

fn materials(speed: f64) -> MaterialLibrary {
    let mut lib = MaterialLibrary::new("werkplaats");
    lib.add(recipe("Berken", 3.0, speed)).unwrap();
    lib.add(recipe("Acryl", 3.0, 10.0)).unwrap();
    lib
}

fn art(svg: &str) -> ArtLibrary {
    let mut lib = ArtLibrary::new("kunst");
    lib.add_group("Vormen").unwrap();
    lib.add_item(NewArt {
        name: "Driehoek",
        group: "Vormen",
        tags: vec![],
        format: ArtFormat::Svg,
        bytes: svg.as_bytes(),
        dxf_scale: None,
    })
    .unwrap();
    lib
}

fn template() -> BundleTemplate {
    let t = template_of(&common::sample_project(), TemplateContent::SettingsOnly).unwrap();
    BundleTemplate { name: "basis".into(), bytes: encode_container(&t).unwrap() }
}

fn local<'a>(materials: &'a mut MaterialLibrary, art: &'a mut ArtLibrary) -> ImportTarget<'a> {
    ImportTarget { materials, art, profiles: &[], templates: &[] }
}

fn with_profiles<'a>(
    materials: &'a mut MaterialLibrary,
    art: &'a mut ArtLibrary,
    profiles: &'a [MachineProfile],
) -> ImportTarget<'a> {
    ImportTarget { materials, art, profiles, templates: &[] }
}

fn everything() -> (MaterialLibrary, ArtLibrary, Vec<MachineProfile>, Vec<BundleTemplate>) {
    (materials(12.0), art(STAR), vec![profile("omtech-60w", 400.0)], vec![template()])
}

#[test]
fn a_bundle_of_selected_resources_roundtrips_exactly() {
    let (m, a, p, t) = everything();
    // Only the first material entry and the one art item are selected.
    let chosen_m = m.subset(&[m.entries()[0].id()]);
    let bytes =
        encode_bundle(&BundleContents { materials: Some(&chosen_m), art: Some(&a), profiles: &p, templates: &t })
            .unwrap();
    let bundle = decode_bundle(&bytes).unwrap();
    assert_eq!(bundle.materials.as_ref().unwrap().entries().len(), 1, "only the selection travels");
    assert_eq!(bundle.materials, Some(chosen_m.clone()));
    assert_eq!(bundle.art, Some(a.clone()));
    assert_eq!(bundle.profiles, p);
    assert_eq!(bundle.templates, t);
    assert_eq!(
        encode_bundle(&BundleContents { materials: Some(&chosen_m), art: Some(&a), profiles: &p, templates: &t })
            .unwrap(),
        bytes,
        "deterministic"
    );
    // A bundle may hold just one kind.
    let only_profiles = encode_bundle(&BundleContents { profiles: &p, ..BundleContents::default() }).unwrap();
    let b = decode_bundle(&only_profiles).unwrap();
    assert!(b.materials.is_none() && b.art.is_none() && b.templates.is_empty() && b.profiles.len() == 1);
}

/// P07: `package_import_shows_profile_diff_before_replace`.
#[test]
fn package_import_shows_profile_diff_before_replace() {
    let (_, _, _, _) = everything();
    let bundle_profiles = vec![profile("omtech-60w", 400.0), profile("laser-b", 250.0), profile("fiber", 500.0)];
    let bytes = encode_bundle(&BundleContents { profiles: &bundle_profiles, ..BundleContents::default() }).unwrap();
    let bundle = decode_bundle(&bytes).unwrap();
    // Locally: the same omtech with another max speed, and fiber identical; laser-b is unknown here.
    let local_profiles = vec![profile("omtech-60w", 300.0), profile("fiber", 500.0)];
    let local = LocalState { profiles: &local_profiles, ..LocalState::default() };
    let preview = preview_import(&bundle, &local);
    let by_id = |id: &str| preview.profiles.iter().find(|p| p.id == id).unwrap();
    assert_eq!(
        by_id("omtech-60w").change,
        ProfileChange::Differs { fields: vec!["max_speed_mm_s"] },
        "the diff names the field"
    );
    assert_eq!(by_id("laser-b").change, ProfileChange::New);
    assert_eq!(by_id("fiber").change, ProfileChange::Identical);
    assert_eq!((by_id("omtech-60w").bundle_revision, by_id("omtech-60w").local_revision), (3, Some(3)));

    // Applying with no explicit choice never replaces the local profile: it stays pending.
    let (mut m, mut a) = (MaterialLibrary::new("l"), ArtLibrary::new("k"));
    let outcome =
        apply_import(&bundle, with_profiles(&mut m, &mut a, &local_profiles), &ImportChoices::default()).unwrap();
    assert!(outcome.profiles_to_replace.is_empty(), "no silent replacement");
    assert_eq!(outcome.profiles_pending, ["omtech-60w"]);
    assert_eq!(outcome.profiles_to_add.iter().map(|p| p.revision().id.as_str()).collect::<Vec<_>>(), ["laser-b"]);
    // Only after the user names it is the profile handed over for replacement.
    let choices = ImportChoices { replace_profiles: ["omtech-60w".to_owned()].into(), ..ImportChoices::default() };
    let outcome = apply_import(&bundle, with_profiles(&mut m, &mut a, &local_profiles), &choices).unwrap();
    assert_eq!(outcome.profiles_to_replace.len(), 1);
    assert_eq!(outcome.profiles_to_replace[0].max_speed_mm_s(), 400.0);
    assert!(outcome.profiles_pending.is_empty());
}

#[test]
fn the_preview_classifies_materials_art_and_templates_against_the_local_state() {
    let (m, a, _, t) = everything();
    let bytes = encode_bundle(&BundleContents {
        materials: Some(&m),
        art: Some(&a),
        templates: &t,
        ..BundleContents::default()
    })
    .unwrap();
    let bundle = decode_bundle(&bytes).unwrap();
    // Local: Berken 3 mm with another speed (conflict), Acryl missing (new); the star exists identically;
    // the template exists with other content.
    let local_m = materials(20.0);
    let mut local_m = {
        let mut l = MaterialLibrary::new("lokaal");
        l.add(local_m.entries()[0].draft().clone()).unwrap();
        l
    };
    let local_a = art(STAR);
    let other_template =
        BundleTemplate { name: "basis".into(), bytes: encode_container(&common::sample_project()).unwrap() };
    let local = LocalState {
        materials: Some(&local_m),
        art: Some(&local_a),
        profiles: &[],
        templates: std::slice::from_ref(&other_template),
    };
    let preview = preview_import(&bundle, &local);
    let mat = |name: &str| preview.materials.iter().find(|p| p.material == name).unwrap();
    assert_eq!((mat("Berken").change, mat("Berken").local_entry), (Change::Conflict, Some(local_m.entries()[0].id())));
    assert_eq!((mat("Acryl").change, mat("Acryl").local_entry), (Change::New, None));
    assert_eq!(preview.art[0].change, Change::Identical);
    assert_eq!(preview.templates[0].change, Change::Conflict);
    // Same content locally → identical.
    local_m.update(local_m.entries()[0].id(), recipe("Berken", 3.0, 12.0)).unwrap();
    let local = LocalState { materials: Some(&local_m), ..LocalState::default() };
    assert_eq!(
        preview_import(&bundle, &local).materials.iter().find(|p| p.material == "Berken").unwrap().change,
        Change::Identical
    );
}

#[test]
fn conflicts_follow_the_chosen_policy_and_new_items_are_always_added() {
    let (m, a, _, t) = everything();
    let bundle = decode_bundle(
        &encode_bundle(&BundleContents {
            materials: Some(&m),
            art: Some(&a),
            templates: &t,
            ..BundleContents::default()
        })
        .unwrap(),
    )
    .unwrap();
    let setup = || {
        let mut lm = MaterialLibrary::new("lokaal");
        lm.add(recipe("Berken", 3.0, 99.0)).unwrap(); // conflicts with the bundle's Berken 3 mm
        let mut la = ArtLibrary::new("lokaal-kunst");
        la.add_group("Vormen").unwrap();
        la.add_item(NewArt {
            name: "Driehoek",
            group: "Vormen",
            tags: vec![],
            format: ArtFormat::Svg,
            bytes: STAR2.as_bytes(),
            dxf_scale: None,
        })
        .unwrap();
        (lm, la)
    };
    // Skip (default): conflicts stay as they are, new things come in.
    let (mut lm, mut la) = setup();
    let out = apply_import(&bundle, local(&mut lm, &mut la), &ImportChoices::default()).unwrap();
    assert_eq!(lm.entries().len(), 2, "Acryl added, Berken kept");
    assert_eq!(
        lm.find(&Default::default())
            .iter()
            .find(|e| e.draft().material == "Berken")
            .unwrap()
            .draft()
            .settings
            .speed
            .get_mm_per_s(),
        99.0
    );
    assert_eq!((out.materials_added.len(), out.materials_replaced.len(), la.items().len(), out.skipped), (1, 0, 1, 2));
    assert_eq!(out.templates_to_write.len(), 1, "templates are handed to the caller to write");
    // Replace: the local entry takes the bundle's content (its revision goes up).
    let (mut lm, mut la) = setup();
    let local_id = lm.entries()[0].id();
    let out = apply_import(
        &bundle,
        local(&mut lm, &mut la),
        &ImportChoices { conflicts: ConflictChoice::Replace, ..ImportChoices::default() },
    )
    .unwrap();
    assert_eq!(lm.get(local_id).unwrap().draft().settings.speed.get_mm_per_s(), 12.0);
    assert_eq!(lm.get(local_id).unwrap().revision(), 2);
    assert_eq!(out.materials_replaced, [local_id]);
    assert_eq!(la.items().len(), 1);
    assert_eq!(la.items()[0].hash(), burnburn_io::asset_hash(STAR.as_bytes()));
    // KeepBoth: both stay, the incoming one with a visible suffix.
    let (mut lm, mut la) = setup();
    apply_import(
        &bundle,
        local(&mut lm, &mut la),
        &ImportChoices { conflicts: ConflictChoice::KeepBoth, ..ImportChoices::default() },
    )
    .unwrap();
    assert_eq!(lm.entries().len(), 3);
    assert!(lm.entries().iter().any(|e| e.draft().name.contains("(bundel)")));
    assert_eq!(la.items().len(), 2);
    assert!(la.items().iter().any(|i| i.name().contains("(bundel)")));
    // Art groups the bundle needs are created locally.
    let mut empty_a = ArtLibrary::new("leeg");
    let mut empty_m = MaterialLibrary::new("leeg");
    apply_import(&bundle, local(&mut empty_m, &mut empty_a), &ImportChoices::default()).unwrap();
    assert_eq!(
        (empty_a.groups(), empty_a.items().len(), empty_m.entries().len()),
        (["Vormen".to_owned()].as_slice(), 1, 2)
    );
}

#[test]
fn damaged_tampered_and_newer_bundles_are_typed_errors() {
    let (m, a, p, t) = everything();
    let good =
        encode_bundle(&BundleContents { materials: Some(&m), art: Some(&a), profiles: &p, templates: &t }).unwrap();
    let tampered = common::rewrite_entry(&good, "materials.json", |mut b| {
        b.push(b'\n');
        b
    });
    assert!(matches!(decode_bundle(&tampered), Err(LoadError::AssetHashMismatch(_))));
    let newer = common::rewrite_json(&good, "bundle.json", |v| v["schema"] = (BUNDLE_SCHEMA + 1).into());
    assert_eq!(
        decode_bundle(&newer),
        Err(LoadError::NewerSchema { found: BUNDLE_SCHEMA + 1, supported: BUNDLE_SCHEMA })
    );
    assert!(matches!(decode_bundle(&common::add_entry(&good, "../evil", b"x")), Err(LoadError::UnexpectedEntry(_))));
    assert!(
        matches!(
            decode_bundle(&common::add_entry(&good, "templates/extra.burnburntemplate", b"x")),
            Err(LoadError::UnexpectedEntry(_))
        ),
        "an entry the manifest does not list"
    );
    let unlisted = common::rewrite_json(&good, "bundle.json", |v| {
        v["entries"].as_array_mut().unwrap().pop();
    });
    assert!(matches!(decode_bundle(&unlisted), Err(LoadError::UnexpectedEntry(_))), "a file without a manifest line");
    let traversal = common::rewrite_json(&good, "bundle.json", |v| {
        v["entries"][3]["path"] = "templates/../../x.burnburntemplate".into()
    });
    assert!(decode_bundle(&traversal).is_err());
    let unknown = common::rewrite_json(&good, "bundle.json", |v| v["extra"] = 1.into());
    assert!(matches!(decode_bundle(&unknown), Err(LoadError::UnknownField { .. })));
    // A template that is not a project container is refused on both sides.
    let bad_template = [BundleTemplate { name: "kapot".into(), bytes: b"not a zip".to_vec() }];
    assert!(encode_bundle(&BundleContents { templates: &bad_template, ..BundleContents::default() }).is_err());
    let bad_name = [BundleTemplate { name: "../x".into(), ..template() }];
    assert!(encode_bundle(&BundleContents { templates: &bad_name, ..BundleContents::default() }).is_err());
    assert!(decode_bundle(b"nope").is_err() && decode_bundle(b"").is_err());
    let _: Option<Bundle> = None;
}
