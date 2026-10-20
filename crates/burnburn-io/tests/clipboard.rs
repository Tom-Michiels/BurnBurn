//! W2-L1-bewerken: the clipboard payload that travels through the OS pasteboard (own UTI).
//! It is untrusted input from any other app: it either decodes to validated geometry or is
//! refused with a typed error, and it never panics.

#![allow(clippy::unwrap_used)] // test helpers outside #[test] fns

use burnburn_document::{ObjectKind, PasteItem};
use burnburn_geometry::{ParametricShape, Path, Point, Rect, ShapeParams, Subpath};
use burnburn_io::clipboard::{ClipboardError, MAX_CLIPBOARD_BYTES, decode_clipboard, encode_clipboard};
use burnburn_io::ProjectUuid;
use burnburn_types::{Length, OperationId, StableId};
use proptest::prelude::*;

fn rect(x: f64, y: f64, w: f64, h: f64) -> Path {
    Path::rectangle(Rect::new(Point::new(x, y), Length::from_mm(w).unwrap(), Length::from_mm(h).unwrap()).unwrap())
}

fn project() -> ProjectUuid {
    ProjectUuid::from_bytes([9; 16])
}

fn sample() -> Vec<PasteItem> {
    let star = ParametricShape::fit(
        ShapeParams::Star { points: 5, ratio: 0.5 },
        rect(0.0, 0.0, 20.0, 20.0).bounds().unwrap(),
    )
    .unwrap();
    vec![
        PasteItem {
            name: "Plaat".into(),
            kind: ObjectKind::Rectangle,
            path: rect(1.0 / 3.0, -0.0, 120.0, 80.0),
            parametric: None,
            group: vec![0],
            operations: vec![OperationId::from_raw(7)],
        },
        PasteItem {
            name: "Ster".into(),
            kind: ObjectKind::Star,
            path: star.to_path().unwrap(),
            parametric: Some(star),
            group: vec![0, 1],
            operations: vec![],
        },
        PasteItem {
            name: "Twee".into(),
            kind: ObjectKind::Path,
            path: Path::from_subpaths(vec![
                rect(0.0, 0.0, 5.0, 5.0).subpaths()[0].clone(),
                rect(10.0, 0.0, 5.0, 5.0).subpaths()[0].clone(),
            ]),
            parametric: None,
            group: vec![],
            operations: vec![],
        },
    ]
}

#[test]
fn clipboard_roundtrips_geometry_groups_layers_and_shape_definitions_exactly() {
    let bytes = encode_clipboard(project(), &sample()).unwrap();
    let back = decode_clipboard(&bytes).unwrap();
    assert_eq!(back.project, project());
    assert_eq!(back.items, sample());
}

#[test]
fn the_payload_names_its_format_and_version_for_other_readers() {
    let bytes = encode_clipboard(project(), &sample()).unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["format"], "burnburn-clipboard");
    assert_eq!(json["version"], 1);
}

#[test]
fn foreign_future_and_damaged_payloads_are_refused_with_a_typed_error() {
    let good = encode_clipboard(project(), &sample()).unwrap();
    let edit = |f: &dyn Fn(&mut serde_json::Value)| {
        let mut v: serde_json::Value = serde_json::from_slice(&good).unwrap();
        f(&mut v);
        serde_json::to_vec(&v).unwrap()
    };
    assert!(matches!(decode_clipboard(b"not json"), Err(ClipboardError::Malformed(_))));
    assert!(matches!(decode_clipboard(b""), Err(ClipboardError::Malformed(_))));
    assert_eq!(decode_clipboard(&edit(&|v| v["format"] = "lightburn".into())), Err(ClipboardError::NotOurs));
    assert_eq!(
        decode_clipboard(&edit(&|v| v["version"] = 2.into())),
        Err(ClipboardError::UnsupportedVersion(2))
    );
    // A coordinate that is not a finite number names its place.
    let bad = edit(&|v| v["items"][0]["subpaths"][0]["start"][0] = serde_json::Value::Null);
    assert!(matches!(decode_clipboard(&bad), Err(ClipboardError::Invalid { .. })), "{:?}", decode_clipboard(&bad));
    // A definition that disagrees with its outline is refused, not silently preferred.
    let bad = edit(&|v| v["items"][1]["parametric"]["points"] = 7.into());
    assert!(matches!(decode_clipboard(&bad), Err(ClipboardError::Invalid { .. })));
    // Empty geometry cannot be pasted.
    let bad = edit(&|v| v["items"][0]["subpaths"] = serde_json::json!([]));
    assert!(matches!(decode_clipboard(&bad), Err(ClipboardError::Invalid { .. })));
}

#[test]
fn an_oversized_payload_is_refused_before_it_is_parsed() {
    let big = vec![b' '; MAX_CLIPBOARD_BYTES + 1];
    assert_eq!(decode_clipboard(&big), Err(ClipboardError::TooLarge));
}

#[test]
fn an_empty_selection_is_not_encoded() {
    assert_eq!(encode_clipboard(project(), &[]), Err(ClipboardError::Empty));
}

#[test]
fn a_subpath_with_non_finite_geometry_cannot_be_encoded() {
    // Domain constructors keep NaN out; the encoder still must not write `null` for a number.
    let sub = Subpath::new(Point::new(0.0, 0.0), vec![], false).unwrap();
    let item = PasteItem {
        name: "x".into(),
        kind: ObjectKind::Path,
        path: Path::from_subpaths(vec![sub]),
        parametric: None,
        group: vec![],
        operations: vec![],
    };
    let bytes = encode_clipboard(project(), &[item]).unwrap();
    assert!(decode_clipboard(&bytes).is_ok());
}

proptest! {
    /// Any byte soup, and any mutation of a good payload, is `Ok` or a typed error; never a panic.
    #[test]
    fn decoding_arbitrary_bytes_never_panics(noise in proptest::collection::vec(any::<u8>(), 0..512)) {
        let _ = decode_clipboard(&noise);
    }

    #[test]
    fn mutated_payloads_never_panic(edits in proptest::collection::vec((any::<prop::sample::Index>(), any::<u8>()), 1..16)) {
        let mut bytes = encode_clipboard(project(), &sample()).unwrap();
        for (at, byte) in edits {
            let i = at.index(bytes.len());
            bytes[i] = byte;
        }
        let _ = decode_clipboard(&bytes);
    }
}
