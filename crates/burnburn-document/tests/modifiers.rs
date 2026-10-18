//! IM21 non-destructive modifier chains and associative references (E06, S06).

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers outside #[test] fns; let-else on expected variants

use burnburn_document::structure::{
    AssocReference, BreakReason, EvalMode, Modifier, ModifierChain, ModifierStep, RefHolder, RefStatus,
    StructureChange, StructureCommand,
};
use burnburn_document::{CommandOutcome, CommandPayload, CommandWarning, DocumentCommand, DocumentSession};
use burnburn_geometry::{Affine, BooleanOp, Point, ProductionTolerance, Rect, Vector};
use burnburn_types::{DocumentId, GeometryId, Length, RequestId, StableId};

pub fn mm(v: f64) -> Length {
    Length::from_mm(v).unwrap()
}

pub fn run(s: &mut DocumentSession, payload: CommandPayload) -> CommandOutcome {
    let cmd = DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload };
    s.apply(cmd).unwrap()
}

pub fn rect(s: &mut DocumentSession, x: f64, y: f64, w: f64, h: f64, name: &str) -> GeometryId {
    let rect = Rect::new(Point::new(x, y), mm(w), mm(h)).unwrap();
    run(s, CommandPayload::CreateRectangle { rect, name: name.into() }).changes.created[0]
}

pub fn chain(source: GeometryId, steps: Vec<Modifier>) -> ModifierChain {
    ModifierChain {
        name: "Resultaat".into(),
        source,
        steps: steps.into_iter().map(|modifier| ModifierStep { modifier, enabled: true }).collect(),
        tolerance: ProductionTolerance::new(mm(0.01)).unwrap(),
        mode: EvalMode::Inline,
    }
}

fn structure(cmd: StructureCommand) -> CommandPayload {
    CommandPayload::Structure(cmd)
}

fn area(s: &DocumentSession, id: GeometryId) -> f64 {
    s.snapshot().object(id).unwrap().path().signed_area().abs()
}

/// A plate minus a slot rectangle; a reference names the plate's right edge through the
/// boolean output. Moving the slot over that edge removes it: the reference must report
/// `Broken` and must not silently move to the new right edge (which comes from the slot).
#[test]
fn broken_reference_after_boolean_is_reported_without_rebinding_neighbour() {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let plate = rect(&mut s, 0.0, 0.0, 100.0, 50.0, "Plaat");
    let slot = rect(&mut s, 120.0, -10.0, 40.0, 70.0, "Sleuf");
    let out = run(
        &mut s,
        structure(StructureCommand::CreateModifier {
            chain: chain(plate, vec![Modifier::Boolean { op: BooleanOp::Difference, with: vec![slot] }]),
        }),
    );
    let result = out.changes.created[0];
    assert!(matches!(out.changes.structure[0], StructureChange::Modifier(_)));
    assert!((area(&s, result) - 5000.0).abs() < 1e-6, "slot does not overlap yet");

    // Pick the right edge (x = 100) of the boolean output and make a named reference.
    let snap = s.snapshot();
    let pick = snap.object(result).unwrap().path().nearest(Point::new(100.0, 25.0)).unwrap().at;
    let target = snap.edge_ref(result, pick).expect("an edge with an origin");
    let added = run(
        &mut s,
        structure(StructureCommand::AddReference { reference: AssocReference { name: "rechts".into(), target } }),
    );
    let StructureChange::Reference(reference) = added.changes.structure[0] else { panic!("reference created") };
    assert!(s.snapshot().reference_status(reference).unwrap().is_bound());

    // Move the slot so it removes the plate's right part (x 60..100).
    let moved = run(
        &mut s,
        CommandPayload::Transform {
            targets: vec![slot],
            transform: Affine::translate(Vector::new(-60.0, 0.0)).unwrap(),
        },
    );
    assert!((area(&s, result) - 3000.0).abs() < 1e-6, "the chain followed the operand in the same command");
    assert!(moved.changes.modified.contains(&result));
    assert_eq!(
        moved.warnings,
        vec![CommandWarning::ReferenceBroken {
            holder: RefHolder::Reference(reference),
            reason: BreakReason::OriginGone
        }]
    );
    assert_eq!(
        s.snapshot().reference_status(reference),
        Some(RefStatus::Broken { reason: BreakReason::OriginGone }),
        "not rebound to the neighbouring edge at x = 60"
    );
    // The new right edge exists and is nameable, but it is a different edge (origin: the slot).
    let snap = s.snapshot();
    let new_right = snap.object(result).unwrap().path().nearest(Point::new(60.0, 25.0)).unwrap().at;
    let new_name = snap.edge_ref(result, new_right).unwrap();
    assert_ne!(new_name, target);

    // Undo restores geometry and binding together.
    s.undo(s.revision()).unwrap();
    assert!(s.snapshot().reference_status(reference).unwrap().is_bound());
}

fn edit_nodes(s: &mut DocumentSession, target: GeometryId, edit: burnburn_geometry::NodeEdit) -> CommandOutcome {
    run(s, CommandPayload::Edit(burnburn_document::GeometryCommand::EditNodes { target, edit }))
}

/// A reference to the top edge of a rectangle (directly, and through a modifier output) stays on
/// the top edge after a node is inserted in an *earlier* edge (DR-20: not a segment index).
#[test]
fn inserting_node_in_source_does_not_rebind_reference_to_later_edge() {
    use burnburn_document::structure::Resolved;
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let plate = rect(&mut s, 0.0, 0.0, 100.0, 50.0, "Plaat");
    let far = rect(&mut s, 500.0, 500.0, 10.0, 10.0, "Ver");
    let out = run(
        &mut s,
        structure(StructureCommand::CreateModifier {
            chain: chain(plate, vec![Modifier::Boolean { op: BooleanOp::Difference, with: vec![far] }]),
        }),
    );
    let result = out.changes.created[0];
    let snap = s.snapshot();
    let top_of = |snap: &burnburn_document::DocumentSnapshot, id| {
        let at = snap.object(id).unwrap().path().nearest(Point::new(50.0, 50.0)).unwrap().at;
        snap.edge_ref(id, at).unwrap()
    };
    let direct = top_of(&snap, plate);
    let derived = top_of(&snap, result);
    let mut refs = Vec::new();
    for target in [direct, derived] {
        let o = run(
            &mut s,
            structure(StructureCommand::AddReference { reference: AssocReference { name: "boven".into(), target } }),
        );
        let StructureChange::Reference(r) = o.changes.structure[0] else { panic!() };
        refs.push(r);
    }
    // Split the right edge (segment 1, before the top edge) in the source.
    let edited = edit_nodes(&mut s, plate, burnburn_geometry::NodeEdit::InsertNode { subpath: 0, segment: 1, t: 0.5 });
    assert!(edited.warnings.is_empty(), "{:?}", edited.warnings);
    let snap = s.snapshot();
    for (r, obj) in refs.iter().zip([plate, result]) {
        let Some(RefStatus::Bound(Resolved::Edge { object, subpath, segments })) = snap.reference_status(*r) else {
            panic!("reference {r} must stay bound: {:?}", snap.reference_status(*r));
        };
        assert_eq!(object, obj);
        let path = snap.object(obj).unwrap().path();
        let sp = &path.subpaths()[subpath];
        // Every resolved segment lies on y = 50 (the top edge), not on the split right edge.
        let mut from = sp.start();
        for (i, seg) in sp.segments().iter().enumerate() {
            if segments.contains(&i) {
                assert!((from.y - 50.0).abs() < 1e-9 && (seg.end().y - 50.0).abs() < 1e-9, "segment {i} of {obj}");
            }
            from = seg.end();
        }
    }
}

/// Splitting an edge keeps one edge (two segments, same key); deleting the node between two
/// edges merges them into a new edge: references to either are reported broken, never moved.
#[test]
fn deleting_node_breaks_reference_to_merged_edge() {
    use burnburn_geometry::{NodeEdit, NodeRef};
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let plate = rect(&mut s, 0.0, 0.0, 100.0, 50.0, "Plaat");
    let snap = s.snapshot();
    let at = snap.object(plate).unwrap().path().nearest(Point::new(100.0, 25.0)).unwrap().at;
    let right = snap.edge_ref(plate, at).unwrap();
    let at = snap.object(plate).unwrap().path().nearest(Point::new(50.0, 0.0)).unwrap().at;
    let bottom = snap.edge_ref(plate, at).unwrap();
    let mut refs = Vec::new();
    for target in [right, bottom] {
        let o = run(
            &mut s,
            structure(StructureCommand::AddReference { reference: AssocReference { name: "r".into(), target } }),
        );
        let StructureChange::Reference(r) = o.changes.structure[0] else { panic!() };
        refs.push(r);
    }
    // Node 1 joins the bottom edge (segment 0) and the right edge (segment 1).
    let edited = edit_nodes(&mut s, plate, NodeEdit::DeleteNode { node: NodeRef { subpath: 0, node: 1 } });
    for r in &refs {
        assert_eq!(s.snapshot().reference_status(*r), Some(RefStatus::Broken { reason: BreakReason::OriginGone }));
        assert!(edited.warnings.contains(&CommandWarning::ReferenceBroken {
            holder: RefHolder::Reference(*r),
            reason: BreakReason::OriginGone
        }));
    }
}

/// An output edge that lies on two coincident input edges (both bottoms of an intersection)
/// has two origins: a name for it resolves `Ambiguous`; edges on one input resolve normally.
#[test]
fn edge_on_coincident_operand_edges_is_ambiguous() {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let a = rect(&mut s, 0.0, 0.0, 100.0, 50.0, "A");
    let b = rect(&mut s, 50.0, 0.0, 100.0, 50.0, "B");
    let out = run(
        &mut s,
        structure(StructureCommand::CreateModifier {
            chain: chain(a, vec![Modifier::Boolean { op: BooleanOp::Intersection, with: vec![b] }]),
        }),
    );
    let result = out.changes.created[0];
    let snap = s.snapshot();
    let name_at = |x: f64, y: f64| {
        let at = snap.object(result).unwrap().path().nearest(Point::new(x, y)).unwrap().at;
        snap.edge_ref(result, at).unwrap()
    };
    assert!(matches!(snap.resolve(&name_at(75.0, 0.0)), RefStatus::Ambiguous { .. }), "bottom lies on A and B");
    let right = name_at(100.0, 25.0);
    assert!(snap.resolve(&right).is_bound(), "x = 100 lies on A only");
    let left = name_at(50.0, 25.0);
    assert!(snap.resolve(&left).is_bound(), "x = 50 lies on B only");
    let origin = |r: burnburn_document::structure::GeometryRef| match r.sub {
        Some(burnburn_document::structure::SubRef::Edge(e)) => e.origin.source,
        _ => panic!(),
    };
    assert_eq!((origin(right), origin(left)), (a, b));
}

fn create_chain(s: &mut DocumentSession, c: ModifierChain) -> (burnburn_document::structure::ModifierId, GeometryId) {
    let out = run(s, structure(StructureCommand::CreateModifier { chain: c }));
    let StructureChange::Modifier(m) = out.changes.structure[0] else { panic!() };
    (m, out.changes.created[0])
}

/// E06: source → offset → boolean → array stays inspectable; disabling one step skips only that
/// step and never touches the source; one command, one undo step.
#[test]
fn disabled_step_is_skipped_without_touching_source_or_other_steps() {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let plate = rect(&mut s, 0.0, 0.0, 20.0, 10.0, "Plaat");
    let hole = rect(&mut s, 5.0, 2.0, 5.0, 5.0, "Gat");
    let steps = vec![
        Modifier::Offset { distance: mm(1.0), join: burnburn_geometry::OffsetJoin::Miter },
        Modifier::Boolean { op: BooleanOp::Difference, with: vec![hole] },
        Modifier::LinearArray { count: 3, step: Vector::new(30.0, 0.0) },
    ];
    let (m, result) = create_chain(&mut s, chain(plate, steps));
    let source_before = s.snapshot().object(plate).unwrap().clone();
    // (22 × 12 − 25) per copy, three copies.
    assert!((area(&s, result) - 3.0 * (264.0 - 25.0)).abs() < 1e-6, "{}", area(&s, result));
    let out = run(&mut s, structure(StructureCommand::SetStepEnabled { modifier: m, step: 1, enabled: false }));
    assert_eq!(out.changes.modified, vec![result]);
    assert!((area(&s, result) - 3.0 * 264.0).abs() < 1e-6, "boolean skipped, offset and array kept");
    assert_eq!(s.snapshot().object(plate), Some(&source_before), "source unchanged");
    assert_eq!(s.snapshot().structure().modifier(m).unwrap().steps.len(), 3, "the step is still there");
    s.undo(s.revision()).unwrap();
    assert!((area(&s, result) - 3.0 * (264.0 - 25.0)).abs() < 1e-6, "undo restores step flag and output together");
}

/// Deleting a modifier's source keeps the last valid output, marks it stale (production
/// blocked) and warns once; nothing is rebound to another object.
#[test]
fn deleted_source_keeps_last_valid_output_and_blocks_production() {
    use burnburn_document::{EvalDiagnosis, EvalStatus};
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let plate = rect(&mut s, 0.0, 0.0, 20.0, 10.0, "Plaat");
    let _other = rect(&mut s, 0.0, 0.0, 20.0, 10.0, "Zelfde plek");
    let (m, result) = create_chain(&mut s, chain(plate, vec![]));
    assert!(!s.snapshot().production_blocked(result));
    let out = run(&mut s, CommandPayload::Delete { targets: vec![plate] });
    assert_eq!(
        out.warnings,
        vec![CommandWarning::ReferenceBroken {
            holder: RefHolder::ModifierSource { modifier: m, geometry: plate },
            reason: BreakReason::ObjectMissing
        }]
    );
    let snap = s.snapshot();
    assert!((area(&s, result) - 200.0).abs() < 1e-9, "last valid output stays");
    assert!(snap.production_blocked(result));
    assert_eq!(
        snap.derived_record(burnburn_document::DerivedOwner::Modifier(m)).unwrap().status,
        EvalStatus::Failed { diagnosis: EvalDiagnosis::MissingSources { objects: vec![plate] } }
    );
    let again = run(&mut s, CommandPayload::Rename { target: result, name: "x".into() });
    assert!(again.warnings.is_empty(), "the break is reported once, not on every command");
}

/// Contract §11: a background evaluation's patch applies after an unrelated edit, and is refused
/// as stale once the chain's own inputs changed.
#[test]
fn stale_patch_against_old_inputs_is_refused() {
    use burnburn_document::structure::ModifierEvaluator;
    use burnburn_document::{CommandError, DerivedOwner, EvalStatus, Evaluator};
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let plate = rect(&mut s, 0.0, 0.0, 20.0, 10.0, "Plaat");
    let unrelated = rect(&mut s, 100.0, 0.0, 5.0, 5.0, "Elders");
    let mut c = chain(plate, vec![Modifier::Offset { distance: mm(1.0), join: burnburn_geometry::OffsetJoin::Miter }]);
    c.mode = EvalMode::Background;
    let out = run(&mut s, structure(StructureCommand::CreateModifier { chain: c }));
    let StructureChange::Modifier(m) = out.changes.structure[0] else { panic!() };
    let owner = DerivedOwner::Modifier(m);
    assert!(out.changes.created.is_empty(), "background chains wait for their task");
    assert_eq!(s.snapshot().derived_record(owner).unwrap().status, EvalStatus::Pending);

    // The task evaluates the snapshot it was given; meanwhile an unrelated object moves.
    let patch = ModifierEvaluator.evaluate(&s.snapshot(), owner);
    let mv = Affine::translate(Vector::new(1.0, 0.0)).unwrap();
    run(&mut s, CommandPayload::Transform { targets: vec![unrelated], transform: mv });
    let applied = run(&mut s, CommandPayload::ApplyPatch(patch.clone()));
    assert_eq!(applied.changes.created.len(), 1, "not revision-bound: an edit elsewhere is no reason");
    assert_eq!(s.snapshot().derived_record(owner).unwrap().status, EvalStatus::Valid);

    // Now the source changes: the same (old) patch is stale.
    run(&mut s, CommandPayload::Transform { targets: vec![plate], transform: mv });
    assert_eq!(s.snapshot().derived_record(owner).unwrap().status, EvalStatus::Pending);
    let cmd = DocumentCommand {
        document: s.id(),
        expected: s.revision(),
        request: RequestId::from_raw(2),
        payload: CommandPayload::ApplyPatch(patch),
    };
    let before = s.revision();
    assert_eq!(s.apply(cmd), Err(CommandError::StalePatch { owner }));
    assert_eq!(s.revision(), before, "a refused patch changes nothing");
}

/// Contract IM21: a modifier may read another modifier's output (evaluated after it, in the
/// same transaction); a chain that would read itself (directly or via another) is refused.
#[test]
fn modifier_on_modifier_output_follows_and_cycles_are_refused() {
    use burnburn_document::CommandError;
    use burnburn_document::structure::StructureError;
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let plate = rect(&mut s, 0.0, 0.0, 20.0, 10.0, "Plaat");
    let grow = |d: f64| Modifier::Offset { distance: mm(d), join: burnburn_geometry::OffsetJoin::Miter };
    let (m1, first) = create_chain(&mut s, chain(plate, vec![grow(1.0)]));
    let (_m2, second) = create_chain(&mut s, chain(first, vec![grow(1.0)]));
    assert!((area(&s, second) - 24.0 * 14.0).abs() < 1e-6);
    let mv = Affine::translate(Vector::new(5.0, 0.0)).unwrap();
    run(&mut s, CommandPayload::Transform { targets: vec![plate], transform: mv });
    let x = s.snapshot().object(second).unwrap().bounds().unwrap().min().x;
    assert!((x - 3.0).abs() < 1e-6, "both followed in one command: {x}");
    // m1 reading m2's output would close a cycle.
    let mut c = chain(second, vec![]);
    c.name = "lus".into();
    let cmd = DocumentCommand {
        document: s.id(),
        expected: s.revision(),
        request: RequestId::from_raw(3),
        payload: structure(StructureCommand::SetModifier { modifier: m1, chain: c }),
    };
    assert_eq!(s.apply(cmd), Err(CommandError::Structure(StructureError::ReferenceCycle(m1))));
}

/// C-8: direct edits of derived geometry are refused with the owner, so the user edits the
/// definition instead; nothing changes.
#[test]
fn direct_edit_of_derived_output_is_refused_with_its_owner() {
    use burnburn_document::{Anchor, Axis, CommandError, DerivedOwner};
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let plate = rect(&mut s, 0.0, 0.0, 20.0, 10.0, "Plaat");
    let (m, result) = create_chain(&mut s, chain(plate, vec![]));
    let owner = DerivedOwner::Modifier(m);
    let mv = Affine::translate(Vector::new(1.0, 0.0)).unwrap();
    let payloads = vec![
        CommandPayload::Transform { targets: vec![plate, result], transform: mv },
        CommandPayload::SetDimension { target: result, axis: Axis::Width, value: mm(5.0), anchor: Anchor::Min },
        CommandPayload::Edit(burnburn_document::GeometryCommand::EditNodes {
            target: result,
            edit: burnburn_geometry::NodeEdit::ReverseSubpath { subpath: 0 },
        }),
    ];
    for payload in payloads {
        let before = s.revision();
        let cmd = DocumentCommand { document: s.id(), expected: before, request: RequestId::from_raw(9), payload };
        assert_eq!(s.apply(cmd), Err(CommandError::DerivedObject { owner }));
        assert_eq!(s.revision(), before);
    }
}
