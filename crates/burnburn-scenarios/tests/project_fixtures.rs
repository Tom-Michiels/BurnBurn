//! Schema-2 project fixture (request CAM, docs/coordination/cam.md): operations, an order
//! constraint and a material recipe. Written once by the ignored writer below, reviewed by hand
//! (`unzip -p … document.json`), then frozen in `test-data/projects/manifest.json`. The load test
//! checks hand-written expectations, never values read back from the encoder.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::path::{Path, PathBuf};

use burnburn_document::process::{
    FillSettings, Kerf, OperationDraft, OperationMode, Passes, PowerPercent, PowerRange, ProcessCommand,
    ProcessSettings, RecipeDraft, RecipeId, RecipeRef, ScanAxis, ScanDirection, Speed,
};
use burnburn_document::{CommandPayload, GeometryCommand, ObjectKind};
use burnburn_geometry::{Point, Rect, Shape};
use burnburn_io::std_fs::StdFileSystem;
use burnburn_io::{Project, ProjectUuid, load_project, save_project};
use burnburn_test_support::fixtures::read_fixture;
use burnburn_types::{GeometryId, OperationId, StableId};
use common::*;

const V2_PLATE: &str = "v2/plate-operations.burnburn";

fn settings(speed: f64, min: f64, max: f64, passes: u32, air: bool) -> ProcessSettings {
    ProcessSettings {
        speed: Speed::mm_per_s(speed).unwrap(),
        power: PowerRange::new(PowerPercent::new(min).unwrap(), PowerPercent::new(max).unwrap()).unwrap(),
        passes: Passes::new(passes).unwrap(),
        air_assist: air,
    }
}

fn fill() -> OperationMode {
    OperationMode::Fill(FillSettings::new(mm(0.1), ScanAxis::X, ScanDirection::Bidirectional, mm(0.0)).unwrap())
}

/// Writes the fixture. Run only to (re)create it on purpose, then review and update the manifest:
/// `cargo test -p burnburn-scenarios --test project_fixtures -- --ignored write_v2_plate_fixture`
#[test]
#[ignore = "writes test-data/projects/v2; run deliberately and review the result"]
fn write_v2_plate_fixture() {
    let mut s = new_session();
    let rect = Rect::new(Point::new(0.0, 0.0), mm(120.0), mm(80.0)).unwrap();
    let plate = apply(&mut s, CommandPayload::CreateRectangle { rect, name: "Plaat".into() }, &[]).changes.created[0];
    let shape = Shape::Circle { center: Point::new(60.0, 40.0), radius: mm(10.0) };
    let hole = apply(&mut s, CommandPayload::Edit(GeometryCommand::CreateShape { shape, name: "Gat".into() }), &[])
        .changes
        .created[0];
    let logo_rect = Rect::new(Point::new(10.0, 60.0), mm(20.0), mm(10.0)).unwrap();
    let logo =
        apply(&mut s, CommandPayload::CreateRectangle { rect: logo_rect, name: "Logo".into() }, &[]).changes.created[0];
    let op = |s: &mut burnburn_document::DocumentSession, name: &str, mode, st, targets| {
        let draft = OperationDraft { name: name.into(), mode, settings: st, output_enabled: true };
        apply(s, CommandPayload::Process(ProcessCommand::AddOperation { draft, targets }), &[]).changes.operations[0]
    };
    let engrave = op(&mut s, "Graveren", fill(), settings(300.0, 10.0, 25.0, 1, false), vec![logo]);
    let inner =
        op(&mut s, "Binnen", OperationMode::Line { kerf: Kerf::NONE }, settings(20.0, 60.0, 60.0, 1, true), vec![hole]);
    let outer = op(
        &mut s,
        "Buiten",
        OperationMode::Line { kerf: Kerf::NONE },
        settings(20.0, 60.0, 60.0, 1, true),
        vec![plate],
    );
    apply(&mut s, CommandPayload::Process(ProcessCommand::AddOrderConstraint { before: inner, after: outer }), &[]);
    let draft = RecipeDraft {
        name: "Multiplex 3 mm snijden".into(),
        material: "Berken multiplex".into(),
        thickness: mm(3.0),
        machine: "research-1500x900".into(),
        notes: "Testfixture, geen gemeten waarden".into(),
        mode: OperationMode::Line { kerf: Kerf::NONE },
        settings: settings(15.0, 55.0, 65.0, 2, true),
    };
    let recipe = apply(&mut s, CommandPayload::Process(ProcessCommand::AddRecipe { draft }), &[]).changes.recipes[0];
    apply(&mut s, CommandPayload::Process(ProcessCommand::ApplyRecipe { recipe, operation: outer }), &[]);
    let _ = engrave;

    let path = fixture_path();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let uuid = ProjectUuid::from_bytes([0x0b, 0x0b, 0x0b, 0x0b, 0, 2, 0x40, 0, 0x80, 0, 0, 0, 0, 0, 0x01, 0x20]);
    save_project(&StdFileSystem, &path, &Project::new(uuid, (*s.snapshot()).clone())).unwrap();
}

fn fixture_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-data/projects").join(V2_PLATE)
}

/// Hand-written from the reviewed document.json (see the manifest `content`).
#[test]
fn v2_plate_fixture_loads_with_operations_constraint_and_recipe() {
    read_fixture("projects", V2_PLATE).unwrap(); // hash- and size-checked against the manifest
    let p = load_project(&StdFileSystem, &fixture_path()).unwrap();
    assert_eq!(p.uuid.to_string(), "0b0b0b0b-0002-4000-8000-000000000120");
    let d = &p.document;
    assert_eq!(d.len(), 3);
    let obj = |id: u64| d.object(GeometryId::from_raw(id)).unwrap();
    assert_eq!((obj(1).name(), obj(1).kind()), ("Plaat", ObjectKind::Rectangle));
    let b = obj(1).bounds().unwrap();
    assert_eq!((b.min(), b.max()), (Point::new(0.0, 0.0), Point::new(120.0, 80.0)));
    assert_eq!(obj(2).name(), "Gat");
    let b = obj(2).bounds().unwrap();
    assert!(
        (b.center().x - 60.0).abs() < 1e-12 && (b.center().y - 40.0).abs() < 1e-12 && (b.width() - 20.0).abs() < 1e-12
    );
    assert_eq!(obj(3).name(), "Logo");

    let process = d.process();
    let names: Vec<_> = process.operations().map(|o| (o.id(), o.name().to_owned(), o.targets().to_vec())).collect();
    assert_eq!(
        names,
        vec![
            (OperationId::from_raw(4), "Graveren".to_owned(), vec![GeometryId::from_raw(3)]),
            (OperationId::from_raw(5), "Binnen".to_owned(), vec![GeometryId::from_raw(2)]),
            (OperationId::from_raw(6), "Buiten".to_owned(), vec![GeometryId::from_raw(1)]),
        ]
    );
    let engrave = process.operation(OperationId::from_raw(4)).unwrap();
    assert_eq!(engrave.mode(), fill());
    assert_eq!(engrave.settings(), settings(300.0, 10.0, 25.0, 1, false));
    assert_eq!(engrave.recipe(), None);
    let outer = process.operation(OperationId::from_raw(6)).unwrap();
    // ApplyRecipe copied the recipe's settings (15 mm/s, 55–65 %, 2 passes) and its revision.
    assert_eq!(outer.settings(), settings(15.0, 55.0, 65.0, 2, true));
    assert_eq!(outer.recipe(), Some(RecipeRef { recipe: RecipeId::from_raw(7), revision: 1 }));
    assert_eq!(process.constraints().len(), 1);
    let c = process.constraints()[0];
    assert_eq!((c.before, c.after), (OperationId::from_raw(5), OperationId::from_raw(6)));
    assert_eq!(
        process.execution_order().unwrap(),
        vec![OperationId::from_raw(4), OperationId::from_raw(5), OperationId::from_raw(6)]
    );
    let recipes: Vec<_> = process.recipes().collect();
    assert_eq!(recipes.len(), 1);
    let r = recipes[0];
    assert_eq!(
        (r.name(), r.material(), r.machine(), r.revision()),
        ("Multiplex 3 mm snijden", "Berken multiplex", "research-1500x900", 1)
    );
    assert_eq!(r.thickness().mm(), 3.0);
    assert_eq!(d.next_id(), 8);
}
