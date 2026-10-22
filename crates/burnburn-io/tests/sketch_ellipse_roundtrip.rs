//! S01: an ellipse in a sketch survives save/reopen (centre, major end, minor radius, construction
//! flag) and the project hash covers its minor radius.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test helpers outside #[test] fns

use std::path::Path;

use burnburn_document::sketch::{Entity, Relation, Sketch, SketchCommand};
use burnburn_document::{CommandPayload, DocumentCommand, DocumentSession};
use burnburn_geometry::Point;
use burnburn_io::memory::MemoryFileSystem;
use burnburn_io::{Project, ProjectUuid, load_project, project_hash, save_project};
use burnburn_types::{DocumentId, RequestId, StableId};

fn sketch_with_ellipse(minor: f64) -> Sketch {
    let mut k = Sketch::new("Ellips");
    let c = k.point(10.0, 5.0).unwrap();
    let m = k.point(14.0, 5.0).unwrap();
    let e = k.ellipse(c, m, minor).unwrap();
    let hidden = k.ellipse(c, m, 1.0).unwrap();
    k.set_construction(hidden, true).unwrap();
    k.relate(Relation::Horizontal(e)).unwrap();
    k.relate(Relation::Concentric(e, hidden)).unwrap();
    k
}

fn project_with(minor: f64) -> Project {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let c = DocumentCommand {
        document: s.id(),
        expected: s.revision(),
        request: RequestId::from_raw(1),
        payload: CommandPayload::Sketch(SketchCommand::Create { sketch: Box::new(sketch_with_ellipse(minor)) }),
    };
    s.apply(c).unwrap();
    Project::new(ProjectUuid::from_bytes([3; 16]), (*s.snapshot()).clone())
}

#[test]
fn an_ellipse_survives_save_and_reopen() {
    let fs = MemoryFileSystem::new();
    fs.create_dir_all_for_test(Path::new("/p"));
    let path = Path::new("/p/ellipse.burnburn");
    let original = project_with(2.0);
    save_project(&fs, path, &original).unwrap();
    let back = load_project(&fs, path).unwrap();
    let sketch = back.document.sketch_layer().sketches().next().unwrap().1;
    let ellipses: Vec<_> =
        sketch.entities().filter(|(_, e)| matches!(e.entity, Entity::Ellipse { .. })).map(|(_, e)| *e).collect();
    assert_eq!(ellipses.len(), 2, "both ellipses came back");
    assert!(ellipses.iter().any(|e| e.construction));
    assert!(ellipses.iter().any(|e| matches!(e.entity, Entity::Ellipse { minor, .. } if minor == 2.0) && !e.construction));
    assert_eq!(back.document.sketch_layer(), original.document.sketch_layer());
    assert_eq!(sketch.constraints().len(), 2);
    let _ = Point::new(0.0, 0.0);
}

#[test]
fn the_project_hash_covers_the_minor_radius() {
    assert_ne!(project_hash(&project_with(2.0)).unwrap(), project_hash(&project_with(2.5)).unwrap());
}
