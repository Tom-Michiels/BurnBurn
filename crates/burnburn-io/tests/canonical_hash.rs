//! Public contract: the project hash is over canonical meaning, not over file bytes.

#![allow(clippy::unwrap_used)] // test helpers outside #[test] fns

mod common;

use burnburn_document::{DocumentSnapshot, GeometryObject, GeometrySource, ObjectKind};
use burnburn_geometry::{Affine, Path, Point, Vector};
use burnburn_io::{Project, ProjectUuid, decode_container, encode_container, project_hash};
use burnburn_types::{GeometryId, StableId};
use common::{rewrite_json, sample_project};
use sha2::{Digest, Sha256};

#[test]
fn project_hash_matches_hand_encoded_bytes() {
    let line = Path::line(Point::new(0.0, 0.0), Point::new(1.0, 0.0)).unwrap();
    let obj = GeometryObject::new(GeometryId::from_raw(1), "a".into(), ObjectKind::Path, GeometrySource::Drawn, line);
    let project = Project::new(ProjectUuid::from_bytes([1; 16]), DocumentSnapshot::from_parts(vec![obj], 2).unwrap());

    // Hand-written projection (layout documented in burnburn-io/src/hash.rs).
    let tag = |t: &str| [&(t.len() as u32).to_le_bytes()[..], t.as_bytes()].concat();
    let u64le = |v: u64| v.to_le_bytes().to_vec();
    let f64le = |v: f64| v.to_bits().to_le_bytes().to_vec();
    let mut bytes = Vec::new();
    bytes.extend(b"burnburn-v1\0");
    bytes.extend(tag("project"));
    bytes.extend(tag("objects"));
    bytes.extend(u64le(1)); //            one object
    bytes.extend(u64le(1)); //              id 1
    bytes.extend(u64le(1));
    bytes.extend(b"a"); //                  name "a" (u64 length + bytes)
    bytes.extend(tag("path")); //           kind
    bytes.extend(tag("drawn")); //          source
    bytes.extend(u64le(1)); //              one subpath
    bytes.extend(f64le(0.0));
    bytes.extend(f64le(0.0)); //              start
    bytes.extend(u64le(0)); //                open
    bytes.extend(u64le(1)); //                one segment
    bytes.extend(tag("line"));
    bytes.extend(f64le(1.0));
    bytes.extend(f64le(0.0));
    bytes.extend(tag("operations"));
    bytes.extend(u64le(0));
    bytes.extend(tag("assets"));
    bytes.extend(u64le(0));
    let expected: [u8; 32] = Sha256::digest(&bytes).into();
    assert_eq!(project_hash(&project).unwrap().as_bytes(), &expected);
}

#[test]
fn canonical_hash_ignores_formatting_and_identity_but_tracks_meaning() {
    let p = sample_project();
    let h = project_hash(&p).unwrap();

    let bytes = encode_container(&p).unwrap();
    assert_eq!(project_hash(&decode_container(&bytes).unwrap()).unwrap(), h, "save/load keeps the hash");
    let compact = rewrite_json(&bytes, "document.json", |_| {});
    assert_ne!(compact, bytes, "re-serialised JSON differs in bytes");
    assert_eq!(project_hash(&decode_container(&compact).unwrap()).unwrap(), h, "formatting is not meaning");

    let mut other_uuid = p.clone();
    other_uuid.uuid = ProjectUuid::from_bytes([9; 16]);
    assert_eq!(project_hash(&other_uuid).unwrap(), h, "file identity is not content");

    // The sample logo starts at y = -0.0; +0.0 is the same meaning.
    let flipped_zero =
        rewrite_json(&bytes, "document.json", |v| v["objects"][1]["subpaths"][0]["start"][1] = 0.0.into());
    assert_eq!(project_hash(&decode_container(&flipped_zero).unwrap()).unwrap(), h);

    let moved = {
        let objs: Vec<_> = p
            .document
            .objects()
            .map(|(_, o)| {
                let m = Affine::translate(Vector::new(1e-9, 0.0)).unwrap();
                GeometryObject::new(o.id(), o.name().into(), o.kind(), o.source().clone(), o.path().transform(&m))
            })
            .collect();
        Project { document: DocumentSnapshot::from_parts(objs, p.document.next_id()).unwrap(), ..p.clone() }
    };
    assert_ne!(project_hash(&moved).unwrap(), h, "geometry is meaning");
    let retargeted = {
        let ops = vec![
            common::sample_operation(3, "Graveren", vec![GeometryId::from_raw(1), GeometryId::from_raw(2)]),
            common::sample_operation(4, "Snijden", vec![GeometryId::from_raw(1), GeometryId::from_raw(2)]),
        ];
        let layer = burnburn_document::process::ProcessLayer::from_parts(ops, vec![], vec![]).unwrap();
        let objs: Vec<_> = p.document.objects().map(|(_, o)| o.clone()).collect();
        let doc = DocumentSnapshot::from_parts(objs, p.document.next_id()).unwrap().with_process(layer).unwrap();
        Project { document: doc, ..p.clone() }
    };
    assert_ne!(project_hash(&retargeted).unwrap(), h, "operation references are meaning");
}

/// DR-17 guard: asset keys written before the hash consolidation stay valid.
#[test]
fn asset_key_unchanged_after_hash_consolidation() {
    let bytes = b"<svg xmlns='http://www.w3.org/2000/svg'/>";
    let mut h = Sha256::new();
    h.update(b"burnburn-v1\0");
    h.update(5u32.to_le_bytes());
    h.update(b"asset");
    h.update(bytes);
    let expected: [u8; 32] = h.finalize().into();
    assert_eq!(burnburn_io::asset_hash(bytes).as_bytes(), &expected);
}
