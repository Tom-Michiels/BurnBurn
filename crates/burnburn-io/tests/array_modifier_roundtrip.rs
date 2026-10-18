//! Array modifier steps survive save and reopen (never dropped: a reopened file would make
//! fewer copies). Regression for the silent `_ => return None` in the structure mapping.

#![allow(clippy::unwrap_used)]

use burnburn_document::structure::{EvalMode, Modifier, ModifierChain, ModifierStep, StructureCommand};
use burnburn_document::{CommandPayload, DocumentCommand, DocumentSession};
use burnburn_geometry::array::{CircularArray, GridArray};
use burnburn_geometry::{Point, ProductionTolerance, Rect};
use burnburn_io::{Project, ProjectUuid, decode_container, encode_container, project_hash};
use burnburn_types::{DocumentId, Length, RequestId, StableId};

fn mm(v: f64) -> Length {
    Length::from_mm(v).unwrap()
}

#[test]
fn grid_and_circular_array_steps_survive_save_and_reopen() {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let run = |s: &mut DocumentSession, payload| {
        let cmd =
            DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload };
        s.apply(cmd).unwrap()
    };
    let rect = Rect::new(Point::new(0.0, 0.0), mm(10.0), mm(10.0)).unwrap();
    let src = run(&mut s, CommandPayload::CreateRectangle { rect, name: "Bron".into() }).changes.created[0];
    let steps = vec![
        ModifierStep {
            modifier: Modifier::GridArray(GridArray { columns: 3, rows: 2, pitch_x: mm(20.0), pitch_y: mm(1.0 / 3.0) }),
            enabled: true,
        },
        ModifierStep {
            modifier: Modifier::CircularArray(CircularArray {
                count: 5,
                center: Point::new(-40.0, 0.5),
                sweep_deg: 135.0,
                rotate_items: false,
                reference: Point::new(5.0, 5.0),
            }),
            enabled: false,
        },
    ];
    let chain = ModifierChain {
        name: "Reeks".into(),
        source: src,
        steps,
        tolerance: ProductionTolerance::new(mm(0.01)).unwrap(),
        mode: EvalMode::Inline,
    };
    run(&mut s, CommandPayload::Structure(StructureCommand::CreateModifier { chain }));
    let p = Project::new(ProjectUuid::from_bytes([2; 16]), (*s.snapshot()).clone());
    let back = decode_container(&encode_container(&p).unwrap()).unwrap();
    assert_eq!(back.document.structure(), p.document.structure());
    let a: Vec<_> = p.document.objects().map(|(_, o)| o.clone()).collect();
    let b: Vec<_> = back.document.objects().map(|(_, o)| o.clone()).collect();
    assert_eq!(a, b);
    assert_eq!(project_hash(&back).unwrap(), project_hash(&p).unwrap());
}
