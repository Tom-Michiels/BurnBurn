//! Schema fixtures: every released schema version keeps loading with the same meaning.
//! Expectations are written by hand from the fixture's reviewed `document.json`, not read back
//! from the encoder. See `test-data/projects/README.md`.

#![allow(clippy::unwrap_used)] // test helpers outside #[test] fns

use std::path::{Path, PathBuf};

use burnburn_document::process::{Kerf, OperationMode};
use burnburn_document::{GeometrySource, ObjectKind};
use burnburn_geometry::Point;
use burnburn_io::std_fs::StdFileSystem;
use burnburn_io::{MIGRATED_V1_POWER_PCT, MIGRATED_V1_SPEED_MM_S, load_project};
use burnburn_types::{GeometryId, OperationId, StableId};
use sha2::{Digest, Sha256};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-data/projects")
}

const V1_RECTANGLE: &str = "v1/rectangle-120x80.burnburn";

/// Recorded in test-data/projects/manifest.json; regenerating the file must be a reviewed change.
const V1_RECTANGLE_SHA256: &str = "see manifest";

#[test]
fn v1_fixture_still_loads_with_expected_meaning() {
    let path = fixtures().join(V1_RECTANGLE);
    let bytes = std::fs::read(&path).unwrap();
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(fixtures().join("manifest.json")).unwrap()).unwrap();
    let entry = manifest["fixtures"].as_array().unwrap().iter().find(|f| f["file"] == V1_RECTANGLE).unwrap();
    let digest: [u8; 32] = Sha256::digest(&bytes).into();
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(entry["sha256"].as_str().unwrap(), hex, "fixture bytes changed: review, do not regenerate silently");
    let _ = V1_RECTANGLE_SHA256;

    let p = load_project(&StdFileSystem, &path).unwrap();
    assert_eq!(p.uuid.to_string(), "0b0b0b0b-0001-4000-8000-000000000120");
    assert_eq!(p.document.len(), 1);
    let obj = p.document.object(GeometryId::from_raw(1)).unwrap();
    assert_eq!((obj.name(), obj.kind(), obj.source()), ("Plaat", ObjectKind::Rectangle, &GeometrySource::Drawn));
    let b = obj.bounds().unwrap();
    assert_eq!((b.min(), b.max()), (Point::new(0.0, 0.0), Point::new(120.0, 80.0)));
    assert_eq!(p.document.next_id(), 3);
    // Schema 1 → 2 migration (IM13): the placeholder operation becomes a Line operation on the
    // same geometry, **disabled** and with inert settings, so an old file never fires the laser
    // before the user reviewed it.
    let ops: Vec<_> = p.document.process().operations().collect();
    assert_eq!(ops.len(), 1);
    let op = ops[0];
    assert_eq!(
        (op.id(), op.name(), op.targets()),
        (OperationId::from_raw(2), "Snijden", &[GeometryId::from_raw(1)][..])
    );
    assert_eq!(op.mode(), OperationMode::Line { kerf: Kerf::NONE });
    assert!(!op.output_enabled(), "migrated placeholder produces no output");
    let s = op.settings();
    assert_eq!(
        (s.speed.get_mm_per_s(), s.power.max().get(), s.passes.get()),
        (MIGRATED_V1_SPEED_MM_S, MIGRATED_V1_POWER_PCT, 1)
    );
    assert!(p.document.process().constraints().is_empty() && p.document.process().recipes().len() == 0);
}

// The schema-1 fixture writer was removed with schema 2 (IM13): this build can no longer write
// schema 1, and old-schema fixtures are frozen (test-data/projects/manifest.json golden policy).

const V2_PLATE: &str = "v2/plate-operations.burnburn";

/// Testing's schema-2 fixture (manifest entry, written before IM21) loads under the build with
/// the structure layer and edge keys: additive fields stay absent, the meaning is unchanged, and
/// re-encoding writes no IM21 field (C-3).
#[test]
fn v2_fixture_loads_unchanged_with_structure_build() {
    let path = fixtures().join(V2_PLATE);
    let bytes = std::fs::read(&path).unwrap();
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(fixtures().join("manifest.json")).unwrap()).unwrap();
    let entry = manifest["fixtures"].as_array().unwrap().iter().find(|f| f["file"] == V2_PLATE).unwrap();
    let digest: [u8; 32] = Sha256::digest(&bytes).into();
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(entry["sha256"].as_str().unwrap(), hex, "fixture bytes changed: review, do not regenerate silently");

    let p = load_project(&StdFileSystem, &path).unwrap();
    assert_eq!(p.uuid.to_string(), "0b0b0b0b-0002-4000-8000-000000000120");
    let names: Vec<&str> = p.document.objects().map(|(_, o)| o.name()).collect();
    assert_eq!(names, ["Plaat", "Gat", "Logo"]);
    let ops: Vec<&str> = p.document.process().operations().map(|o| o.name()).collect();
    assert_eq!(ops, ["Graveren", "Binnen", "Buiten"]);
    assert_eq!(p.document.next_id(), 8);
    assert!(p.document.structure().is_empty());
    for (_, o) in p.document.objects() {
        assert!(o.stored_edge_keys().is_none(), "default edge keys 0..n");
    }
    let doc = burnburn_io::encode_container(&p).unwrap();
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(doc)).unwrap();
    let mut json = String::new();
    std::io::Read::read_to_string(&mut zip.by_name("document.json").unwrap(), &mut json).unwrap();
    assert!(!json.contains("structure") && !json.contains("edge_keys"));
}

fn entry(container: &[u8], name: &str) -> Vec<u8> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(container)).unwrap();
    let mut out = Vec::new();
    std::io::Read::read_to_end(&mut zip.by_name(name).unwrap(), &mut out).unwrap();
    out
}

/// Every additive schema-2 field since (IM21 structure, texts, traces/generators, IM25 process
/// options) is omitted when unused: re-saving the schema-2 fixture writes its `document.json`
/// byte for byte (integratie-datamodel).
#[test]
fn v2_fixture_document_reencodes_byte_identically() {
    let bytes = std::fs::read(fixtures().join(V2_PLATE)).unwrap();
    let p = burnburn_io::decode_container(&bytes).unwrap();
    let again = burnburn_io::encode_container(&p).unwrap();
    assert_eq!(
        String::from_utf8(entry(&again, "document.json")).unwrap(),
        String::from_utf8(entry(&bytes, "document.json")).unwrap()
    );
}
