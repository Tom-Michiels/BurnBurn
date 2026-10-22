//! Shared fixtures of the golf-5 panel tests (macOS and Windows hosts include this file): small
//! documents that make presentation show notes, an offset, open contours, a centre-line trace and
//! a structure with parts, plates and a broken reference. Built through the public route only.

#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::path::{Path as FsPath, PathBuf};

use burnburn_document::structure::{
    AssocReference, Dim, GeometryRef, NamedShape, PartDefinition, PartInstance, PartShape, Plate, StructureChange,
    StructureCommand,
};
use burnburn_document::{
    CommandOutcome, CommandPayload, DocumentCommand, DocumentSession, DocumentSnapshot, GeometryObject,
    GeometrySource, ObjectKind,
};
use burnburn_geometry::{Affine, Path, PathBuilder, Point, Rect, Vector};
use burnburn_image::encode_png_gray8;
use burnburn_io::{Project, ProjectUuid, save_project};
use burnburn_presentation::{
    CommandKey, DocumentViewModel, HeadlessEditorHarness, HostEffect, SelectMode, TraceFormInput, TraceMode, UiIntent,
};
use burnburn_types::{DocumentId, GeometryId, Length, RequestId, StableId};

pub fn vm(h: &HeadlessEditorHarness, doc: DocumentId) -> DocumentViewModel {
    h.document(doc).unwrap()
}

/// A window with two rectangles, 20 x 10 mm at x = 0 and x = 40.
pub fn two_rects() -> (HeadlessEditorHarness, DocumentId, [GeometryId; 2]) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (0.0, 0.0), (20.0, 10.0));
    h.drag(doc, (40.0, 0.0), (60.0, 10.0));
    h.invoke(doc, CommandKey::ToolSelect);
    let ids: Vec<GeometryId> = vm(&h, doc).objects.iter().map(|o| o.id).collect();
    (h, doc, [ids[0], ids[1]])
}

pub fn select(h: &mut HeadlessEditorHarness, doc: DocumentId, ids: &[GeometryId]) {
    h.send(UiIntent::Select { document: doc, ids: ids.to_vec(), mode: SelectMode::Replace });
}

fn open_window(h: &mut HeadlessEditorHarness, path: PathBuf) -> DocumentId {
    h.send(UiIntent::Open { path });
    h.run_tasks()
        .iter()
        .flat_map(|u| u.effects.clone())
        .find_map(|e| match e {
            HostEffect::OpenWindow { document } => Some(document),
            _ => None,
        })
        .expect("opened")
}

/// A saved project made of `paths` (named "Pad n"), opened in a window.
pub fn opened_with(paths: Vec<Path>) -> (HeadlessEditorHarness, DocumentId, Vec<GeometryId>) {
    let mut h = HeadlessEditorHarness::new();
    h.fs().create_dir_all_for_test(FsPath::new("/work"));
    let objects: Vec<_> = paths
        .into_iter()
        .enumerate()
        .map(|(i, p)| {
            let id = GeometryId::from_raw(i as u64 + 1);
            GeometryObject::new(id, format!("Pad {}", i + 1), ObjectKind::Path, GeometrySource::Drawn, p)
        })
        .collect();
    let next = objects.len() as u64 + 1;
    let snapshot = DocumentSnapshot::from_parts(objects, next).unwrap();
    let path = PathBuf::from("/work/fixture.burnburn");
    save_project(h.fs(), &path, &Project::new(ProjectUuid::from_bytes([7; 16]), snapshot)).unwrap();
    let doc = open_window(&mut h, path);
    let ids = vm(&h, doc).objects.iter().map(|o| o.id).collect();
    (h, doc, ids)
}

/// An open contour: a gap of `gap` mm between its two ends.
pub fn open_path(x: f64, gap: f64) -> Path {
    PathBuilder::new()
        .move_to(Point::new(x, 0.0))
        .line_to(Point::new(x + 10.0, 0.0))
        .line_to(Point::new(x + 10.0, 10.0))
        .line_to(Point::new(x, gap))
        .build()
        .unwrap()
}

// --- Centre-line traces -----------------------------------------------------------------------

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 == 1 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

/// A grey image as a PNG at 254 dpi (0.1 mm per pixel).
fn png_of(gray: &[u8], w: u32, h: u32) -> Vec<u8> {
    let plain = encode_png_gray8(w, h, gray).unwrap();
    let mut data = 10_000u32.to_be_bytes().to_vec();
    data.extend_from_slice(&10_000u32.to_be_bytes());
    data.push(1);
    let mut body = b"pHYs".to_vec();
    body.extend_from_slice(&data);
    let mut chunk = (data.len() as u32).to_be_bytes().to_vec();
    chunk.extend_from_slice(&body);
    chunk.extend_from_slice(&crc32(&body).to_be_bytes());
    let mut out = plain[..33].to_vec();
    out.extend_from_slice(&chunk);
    out.extend_from_slice(&plain[33..]);
    out
}

/// A T of 6 px thick strokes: a bar over the top and a stem down from its middle.
pub fn t_png() -> Vec<u8> {
    let mut gray = vec![255u8; 64 * 64];
    for y in 0..64usize {
        for x in 0..64usize {
            let bar = (10..54).contains(&x) && (10..16).contains(&y);
            let stem = (29..35).contains(&x) && (10..56).contains(&y);
            if bar || stem {
                gray[y * 64 + x] = 0;
            }
        }
    }
    png_of(&gray, 64, 64)
}

/// A ring: centre (32, 32), outer radius 20 px, inner 14 px.
pub fn ring_png() -> Vec<u8> {
    let mut gray = vec![255u8; 64 * 64];
    for y in 0..64i32 {
        for x in 0..64i32 {
            let d2 = (x - 32).pow(2) + (y - 32).pow(2);
            if (14 * 14..=20 * 20).contains(&d2) {
                gray[(y * 64 + x) as usize] = 0;
            }
        }
    }
    png_of(&gray, 64, 64)
}

/// The tracer open on `bytes` in centre-line mode.
pub fn centre_lines_of(bytes: &[u8]) -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    h.fs().create_dir_all_for_test(FsPath::new("/work"));
    burnburn_io::fs::FileSystem::write_all(h.fs(), FsPath::new("/work/in.png"), bytes).unwrap();
    let doc = h.new_document().expect("window");
    h.send(UiIntent::OpenTraceSource { document: doc, path: PathBuf::from("/work/in.png") });
    h.run_tasks();
    let mut input: TraceFormInput = h.document(doc).unwrap().tracer.expect("tracer panel").form.input;
    input.mode = TraceMode::Centerline;
    h.send(UiIntent::SetTraceForm { document: doc, input: Box::new(input) });
    h.run_tasks();
    (h, doc)
}

// --- Structure --------------------------------------------------------------------------------

fn mm(v: f64) -> Length {
    Length::from_mm(v).unwrap()
}

fn run(s: &mut DocumentSession, payload: CommandPayload) -> CommandOutcome {
    let cmd = DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload };
    s.apply(cmd).unwrap()
}

fn st(cmd: StructureCommand) -> CommandPayload {
    CommandPayload::Structure(cmd)
}

/// "Beugel": `breedte x 20`, variants "klein" (25) and "kapot" (-5, invalid on purpose).
fn bracket() -> PartDefinition {
    let mut shapes = BTreeMap::new();
    shapes.insert(
        1,
        NamedShape {
            name: "omtrek".into(),
            shape: PartShape::Rectangle {
                x: Dim::Mm(0.0),
                y: Dim::Mm(0.0),
                width: Dim::Param("breedte".into()),
                height: Dim::Mm(20.0),
            },
        },
    );
    let mut variants = BTreeMap::new();
    variants.insert("klein".to_owned(), BTreeMap::from([("breedte".to_owned(), mm(25.0))]));
    variants.insert("kapot".to_owned(), BTreeMap::from([("breedte".to_owned(), mm(-5.0))]));
    PartDefinition {
        name: "Beugel".into(),
        parameters: BTreeMap::from([("breedte".to_owned(), mm(40.0))]),
        shapes,
        variants,
    }
}

/// A saved project with the part "Beugel", three instances (klein on plate A, kapot on plate A,
/// plain), plate A (600 x 400 mm) and a reference to an object that was deleted afterwards.
pub fn structure_project() -> (HeadlessEditorHarness, DocumentId, [u64; 3]) {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let out = run(&mut s, st(StructureCommand::DefinePart { definition: bracket() }));
    let StructureChange::Definition(def) = out.changes.structure[0] else { panic!() };
    let out = run(
        &mut s,
        st(StructureCommand::CreatePlate {
            plate: Plate {
                name: "Plaat A".into(),
                width: mm(600.0),
                height: mm(400.0),
                material: "Berken".into(),
                thickness: Some(mm(3.0)),
            },
        }),
    );
    let StructureChange::Plate(plate) = out.changes.structure[0] else { panic!() };
    let place = |s: &mut DocumentSession, x: f64, variant: Option<&str>, plate| {
        let instance = PartInstance {
            definition: def,
            variant: variant.map(str::to_owned),
            overrides: BTreeMap::new(),
            transform: Affine::translate(Vector::new(x, 0.0)).unwrap(),
            plate,
        };
        let out = run(s, st(StructureCommand::PlaceInstance { instance }));
        let StructureChange::Instance(i) = out.changes.structure[0] else { panic!() };
        StableId::raw(i)
    };
    let klein = place(&mut s, 0.0, Some("klein"), Some(plate));
    let kapot = place(&mut s, 100.0, Some("kapot"), Some(plate));
    let plain = place(&mut s, 200.0, None, None);

    let rect = Rect::new(Point::new(0.0, 0.0), mm(10.0), mm(10.0)).unwrap();
    let object = run(&mut s, CommandPayload::CreateRectangle { rect, name: "Bron".into() }).changes.created[0];
    run(
        &mut s,
        st(StructureCommand::AddReference {
            reference: AssocReference { name: "Rand".into(), target: GeometryRef { object, sub: None } },
        }),
    );
    run(&mut s, CommandPayload::Delete { targets: vec![object] });

    let mut h = HeadlessEditorHarness::new();
    h.fs().create_dir_all_for_test(FsPath::new("/work"));
    let path = PathBuf::from("/work/onderdelen.burnburn");
    save_project(h.fs(), &path, &Project::new(ProjectUuid::from_bytes([3; 16]), (*s.snapshot()).clone())).unwrap();
    let doc = open_window(&mut h, path);
    (h, doc, [klein, kapot, plain])
}
