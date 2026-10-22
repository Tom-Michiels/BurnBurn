//! Shared fixtures for burnburn-io integration tests.

#![allow(clippy::unwrap_used, dead_code)]

use std::io::{Cursor, Read, Write};

use burnburn_document::process::{
    Kerf, Operation, OperationDraft, OperationMode, Passes, PowerPercent, PowerRange, ProcessLayer, ProcessSettings,
    Speed,
};
use burnburn_document::{DocumentSnapshot, GeometryObject, GeometrySource, ObjectKind};
use burnburn_geometry::{EllipticalArc, PathBuilder, Point, Rect};
use burnburn_io::{AssetStore, Project, ProjectUuid};
use burnburn_types::{Angle, GeometryId, Length, OperationId, StableId};

/// Rectangle + an imported logo with a cubic and an arc; two operations share the rectangle.
pub fn sample_project() -> Project {
    let mut assets = AssetStore::default();
    let svg = assets.add("image/svg+xml", b"<svg xmlns='http://www.w3.org/2000/svg'/>");
    let rect =
        Rect::new(Point::new(10.0, 20.0), Length::from_mm(120.0).unwrap(), Length::from_mm(80.0).unwrap()).unwrap();
    let arc = EllipticalArc::circle(
        Point::new(50.0, 50.0),
        0.1 + 0.2,
        Angle::from_degrees(-30.0).unwrap(),
        Angle::from_degrees(250.0).unwrap(),
    )
    .unwrap();
    let logo = PathBuilder::new()
        .move_to(Point::new(1.0 / 3.0, -0.0))
        .cubic_to(Point::new(0.0, 10.0), Point::new(10.0, 10.0), Point::new(10.0, 0.0))
        .close()
        .move_to(arc.start_point())
        .arc(arc)
        .build()
        .unwrap();
    let objects = vec![
        GeometryObject::new(
            GeometryId::from_raw(1),
            "Plaat".into(),
            ObjectKind::Rectangle,
            GeometrySource::Drawn,
            burnburn_geometry::Path::rectangle(rect),
        ),
        GeometryObject::new(
            GeometryId::from_raw(2),
            "Logo".into(),
            ObjectKind::Path,
            GeometrySource::Imported { asset: svg, entity: Some("path#outline".into()) },
            logo,
        ),
    ];
    let operations = vec![
        sample_operation(3, "Graveren", vec![GeometryId::from_raw(1)]),
        sample_operation(4, "Snijden", vec![GeometryId::from_raw(1), GeometryId::from_raw(2)]),
    ];
    let process = ProcessLayer::from_parts(operations, vec![], vec![]).unwrap();
    let document = DocumentSnapshot::from_parts(objects, 5).unwrap().with_process(process).unwrap();
    Project {
        uuid: ProjectUuid::from_bytes(*b"burnburn-fixture"),
        document,
        assets,
        profiles: Vec::new(),
        revisions: Vec::new(),
        library_links: Vec::new(),
        production: None,
        layer_slots: Vec::new(),
        part_name: None,
    }
}

/// A Line operation (0.1 + 0.2 mm/s keeps a float that needs exact roundtrip).
pub fn sample_operation(id: u64, name: &str, targets: Vec<GeometryId>) -> Operation {
    let p = PowerPercent::new(33.3).unwrap();
    let draft = OperationDraft {
        name: name.into(),
        mode: OperationMode::Line { kerf: Kerf::NONE },
        settings: ProcessSettings {
            speed: Speed::mm_per_s(0.1 + 0.2).unwrap(),
            power: PowerRange::new(p, p).unwrap(),
            passes: Passes::new(1).unwrap(),
            air_assist: true,
        },
        output_enabled: true,
    };
    Operation::new(OperationId::from_raw(id), draft, targets, None).unwrap()
}

/// Rewrites one entry of a container with `edit`, using the zip crate directly (not our encoder).
pub fn rewrite_entry(container: &[u8], entry: &str, edit: impl Fn(Vec<u8>) -> Vec<u8>) -> Vec<u8> {
    let mut src = zip::ZipArchive::new(Cursor::new(container)).unwrap();
    let mut out = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for i in 0..src.len() {
        let mut f = src.by_index(i).unwrap();
        let name = f.name().unwrap().into_owned();
        let mut data = Vec::new();
        f.read_to_end(&mut data).unwrap();
        if name == entry {
            data = edit(data);
        }
        out.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        out.write_all(&data).unwrap();
    }
    out.finish().unwrap().into_inner()
}

/// Edits a JSON entry as a `serde_json::Value`.
pub fn rewrite_json(container: &[u8], entry: &str, edit: impl Fn(&mut serde_json::Value)) -> Vec<u8> {
    rewrite_entry(container, entry, |bytes| {
        let mut v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        edit(&mut v);
        serde_json::to_vec(&v).unwrap()
    })
}

/// Adds an extra entry.
pub fn add_entry(container: &[u8], name: &str, data: &[u8]) -> Vec<u8> {
    let mut src = zip::ZipArchive::new(Cursor::new(container)).unwrap();
    let mut out = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for i in 0..src.len() {
        out.raw_copy_file(src.by_index(i).unwrap()).unwrap();
    }
    out.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
    out.write_all(data).unwrap();
    out.finish().unwrap().into_inner()
}
