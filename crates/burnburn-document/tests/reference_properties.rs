//! DR-20 invariant: inserting nodes anywhere in a drawn source never moves a reference to
//! another edge; a reference to the top edge always resolves to segments on the top edge.

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers outside #[test] fns; let-else on expected variants

use burnburn_document::structure::{AssocReference, RefStatus, Resolved, StructureChange, StructureCommand};
use burnburn_document::{CommandPayload, DocumentCommand, DocumentSession, GeometryCommand};
use burnburn_geometry::{NodeEdit, Point, Rect};
use burnburn_types::{DocumentId, Length, RequestId, StableId};
use proptest::prelude::*;

fn run(s: &mut DocumentSession, payload: CommandPayload) -> burnburn_document::CommandOutcome {
    let cmd = DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload };
    s.apply(cmd).unwrap()
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, ..ProptestConfig::default() })]

    #[test]
    fn node_insertions_never_rebind_a_reference(splits in proptest::collection::vec((0usize..64, 0.05f64..0.95), 1..12)) {
        let mut s = DocumentSession::new(DocumentId::from_raw(1));
        let rect = Rect::new(Point::new(0.0, 0.0), Length::from_mm(100.0).unwrap(), Length::from_mm(50.0).unwrap()).unwrap();
        let plate = run(&mut s, CommandPayload::CreateRectangle { rect, name: "Plaat".into() }).changes.created[0];
        let snap = s.snapshot();
        let at = snap.object(plate).unwrap().path().nearest(Point::new(50.0, 50.0)).unwrap().at;
        let target = snap.edge_ref(plate, at).unwrap();
        let out = run(&mut s, CommandPayload::Structure(StructureCommand::AddReference {
            reference: AssocReference { name: "boven".into(), target },
        }));
        let StructureChange::Reference(r) = out.changes.structure[0] else { panic!() };
        for (seg, t) in splits {
            let n = s.snapshot().object(plate).unwrap().path().subpaths()[0].segments().len();
            let edit = NodeEdit::InsertNode { subpath: 0, segment: seg % n, t };
            run(&mut s, CommandPayload::Edit(GeometryCommand::EditNodes { target: plate, edit }));
            let snap = s.snapshot();
            let Some(RefStatus::Bound(Resolved::Edge { segments, .. })) = snap.reference_status(r) else {
                return Err(TestCaseError::fail("reference must stay bound"));
            };
            let sp = &snap.object(plate).unwrap().path().subpaths()[0];
            let mut from = sp.start();
            for (i, g) in sp.segments().iter().enumerate() {
                if segments.contains(&i) {
                    prop_assert!((from.y - 50.0).abs() < 1e-9 && (g.end().y - 50.0).abs() < 1e-9);
                }
                from = g.end();
            }
        }
    }
}
