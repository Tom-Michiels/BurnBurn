//! Editor commit → document payload. This is the single place where an interaction result meets
//! the document command set (contract §4: the editor knows no `DocumentCommand`).

use burnburn_document::sketch::{SketchCommand, SketchStep};
use burnburn_document::structure::{EvalMode, ModifierChain, ModifierStep, StructureCommand};
use burnburn_document::{CommandPayload, DocumentSnapshot, GeometryCommand, ShapeCommand};
use burnburn_editor::{EditCommit, ShapeKind};
use burnburn_geometry::{BooleanOptions, FillRule, ProductionTolerance, Shape, ShapeParams};
use burnburn_types::{GeometryId, Length};

/// Default object names (NL; the document stores user data, not localisation keys).
pub(crate) fn base_name(kind: NameKind) -> &'static str {
    match kind {
        NameKind::Rectangle => "Rechthoek",
        NameKind::Triangle => "Driehoek",
        NameKind::Pentagon => "Vijfhoek",
        NameKind::Octagon => "Achthoek",
        NameKind::Shape(ShapeKind::Ellipse) => "Ellips",
        NameKind::Shape(ShapeKind::Circle) => "Cirkel",
        NameKind::Shape(ShapeKind::Line) => "Lijn",
        NameKind::Shape(ShapeKind::Polyline) => "Polylijn",
        NameKind::Shape(ShapeKind::Polygon) => "Veelhoek",
        NameKind::Shape(ShapeKind::Star) => "Ster",
        NameKind::Shape(ShapeKind::DoubleStar) => "Dubbele ster",
        NameKind::Shape(ShapeKind::RoundedRectangle) => "Afgeronde rechthoek",
        NameKind::Shape(_) => "Curve",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum NameKind {
    Rectangle,
    /// Regular polygons with a name of their own (3, 5 and 8 sides); the others are `Polygon`.
    Triangle,
    Pentagon,
    Octagon,
    Shape(ShapeKind),
}

/// What a commit creates, for its default name.
pub(crate) fn name_kind(edit: &EditCommit) -> Option<NameKind> {
    match edit {
        EditCommit::CreateRectangle { .. } => Some(NameKind::Rectangle),
        EditCommit::CreateShape { shape } => Some(NameKind::Shape(match shape {
            Shape::Ellipse { .. } => ShapeKind::Ellipse,
            Shape::Circle { .. } => ShapeKind::Circle,
            Shape::Line { .. } => ShapeKind::Line,
            Shape::Polyline { .. } => ShapeKind::Polyline,
            _ => ShapeKind::Curve,
        })),
        EditCommit::CreatePath { .. } => Some(NameKind::Shape(ShapeKind::Curve)),
        EditCommit::CreateParametric { shape } => Some(match *shape.params() {
            ShapeParams::Polygon { sides: 3 } => NameKind::Triangle,
            ShapeParams::Polygon { sides: 5 } => NameKind::Pentagon,
            ShapeParams::Polygon { sides: 8 } => NameKind::Octagon,
            ShapeParams::Polygon { .. } => NameKind::Shape(ShapeKind::Polygon),
            ShapeParams::Star { .. } => NameKind::Shape(ShapeKind::Star),
            ShapeParams::DoubleStar { .. } => NameKind::Shape(ShapeKind::DoubleStar),
            ShapeParams::RoundedRectangle { .. } => NameKind::Shape(ShapeKind::RoundedRectangle),
            _ => NameKind::Shape(ShapeKind::Curve),
        }),
        _ => None,
    }
}

/// Why a commit has no document command. Both are told to the user; neither is silent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Refusal {
    /// The document has no atomic command for this gesture on several objects at once.
    SingleObjectOnly,
    /// The editor result has no document equivalent (yet).
    Unsupported,
}

/// The document payload for one editor commit (one command, one history entry). `snapshot` is the
/// document the commit was made against (a copy needs its original).
pub(crate) fn payload(edit: EditCommit, name: String, snapshot: &DocumentSnapshot) -> Result<CommandPayload, Refusal> {
    Ok(match edit {
        EditCommit::CreateRectangle { rect } => CommandPayload::CreateRectangle { rect, name },
        EditCommit::Transform { targets, transform, .. } => CommandPayload::Transform { targets, transform },
        EditCommit::CreateShape { shape } => CommandPayload::Edit(GeometryCommand::CreateShape { shape, name }),
        EditCommit::CreatePath { path } => CommandPayload::Edit(GeometryCommand::CreatePath { path, name }),
        EditCommit::CreateParametric { shape } => CommandPayload::Shape(ShapeCommand::Create { shape, name }),
        // The document re-applies the same domain edit; the preview path is not trusted input.
        EditCommit::EditNodes { target, edit, .. } => CommandPayload::Edit(GeometryCommand::EditNodes { target, edit }),
        EditCommit::Ops(ops) => CommandPayload::Edit(GeometryCommand::Ops(ops)),
        EditCommit::CutCorner { target, pick, reach, cut, .. } => {
            CommandPayload::Edit(GeometryCommand::CutCorner { target, pick, reach, cut })
        }
        EditCommit::Trim { target, pick, cutters, .. } => {
            CommandPayload::Edit(GeometryCommand::TrimAt { target, pick, cutters })
        }
        EditCommit::Extend { target, end, boundaries, .. } => {
            CommandPayload::Edit(GeometryCommand::ExtendEnd { target, end, boundaries })
        }
        EditCommit::Break { target, pick, .. } => CommandPayload::Edit(GeometryCommand::BreakAt { target, pick }),
        EditCommit::Mirror { targets, transform, keep_original: false } => {
            CommandPayload::Transform { targets, transform }
        }
        EditCommit::Mirror { targets, transform, keep_original: true } => {
            let [target] = single(&targets)?;
            let original = snapshot.object(target).ok_or(Refusal::Unsupported)?;
            let name = format!("{} kopie", original.name());
            // A basic shape stays editable in its copy; everything else becomes a plain path.
            match original.parametric().and_then(|shape| shape.transformed(&transform)) {
                Some(shape) => CommandPayload::Shape(ShapeCommand::Create { shape, name }),
                None => CommandPayload::Edit(GeometryCommand::CreatePath {
                    path: original.path().transform(&transform),
                    name,
                }),
            }
        }
        EditCommit::Sketch { sketch, commands } => match <[SketchCommand; 1]>::try_from(commands) {
            Ok([only]) => CommandPayload::Sketch(only),
            // Several commands are one transaction (one history entry) or none.
            Err(commands) => {
                let steps = commands.into_iter().map(|c| step_of(c, sketch)).collect::<Option<Vec<_>>>();
                CommandPayload::Sketch(SketchCommand::Batch { sketch, steps: steps.ok_or(Refusal::Unsupported)? })
            }
        },
        // A chain has one source object: several targets would need an atomic batch.
        EditCommit::AddArray { targets, modifier } => {
            let [source] = single(&targets)?;
            let tolerance = boolean_options().ok_or(Refusal::Unsupported)?.tolerance;
            CommandPayload::Structure(StructureCommand::CreateModifier {
                chain: ModifierChain {
                    name: if name.is_empty() { "Patroon".to_owned() } else { name },
                    source,
                    steps: vec![ModifierStep { modifier, enabled: true }],
                    tolerance,
                    mode: EvalMode::Inline,
                },
            })
        }
        _ => return Err(Refusal::Unsupported),
    })
}

/// The one target of a gesture that the document can only apply to one object at a time.
fn single(targets: &[GeometryId]) -> Result<[GeometryId; 1], Refusal> {
    match targets {
        [one] => Ok([*one]),
        [] => Err(Refusal::Unsupported),
        _ => Err(Refusal::SingleObjectOnly),
    }
}

/// One step of a sketch batch: only entity and constraint edits of that same sketch are steps.
fn step_of(command: SketchCommand, sketch: burnburn_document::sketch::SketchId) -> Option<SketchStep> {
    match command {
        SketchCommand::AddEntity { sketch: s, entity } if s == sketch => Some(SketchStep::AddEntity(entity)),
        SketchCommand::AddConstraint { sketch: s, constraint } if s == sketch => {
            Some(SketchStep::AddConstraint(constraint))
        }
        SketchCommand::RemoveConstraint { sketch: s, constraint } if s == sketch => {
            Some(SketchStep::RemoveConstraint(constraint))
        }
        _ => None,
    }
}

/// Chord tolerance for flattening curves in booleans (the result is polygonal).
const BOOLEAN_TOLERANCE_MM: f64 = 0.01;

/// The options used for both the boolean preview task and the applied command, so the applied
/// result is the previewed one.
pub(crate) fn boolean_options() -> Option<BooleanOptions> {
    let tolerance = ProductionTolerance::new(Length::from_mm(BOOLEAN_TOLERANCE_MM).ok()?).ok()?;
    Some(BooleanOptions { fill: FillRule::EvenOdd, tolerance })
}

#[cfg(test)]
mod tests {
    use burnburn_document::sketch::{EntityId, Relation, SketchCommand, SketchConstraint, SketchId, SketchStep};
    use burnburn_document::structure::{Modifier, StructureCommand};
    use burnburn_document::{GeometryObject, GeometrySource, ObjectKind, OpsCommand, ShapeCommand};
    use burnburn_editor::EditCommit;
    use burnburn_geometry::array::GridArray;
    use burnburn_geometry::{
        Affine, Bounds, CornerCut, EndpointRef, ParametricShape, Path, PathOp, Point, ShapeParams, Vector,
    };
    use burnburn_types::{GeometryId, Length, StableId};

    use super::*;

    fn id(raw: u64) -> GeometryId {
        GeometryId::from_raw(raw)
    }

    fn line() -> Path {
        Path::line(Point::ORIGIN, Point::new(10.0, 0.0)).unwrap()
    }

    fn empty() -> DocumentSnapshot {
        DocumentSnapshot::from_parts(Vec::new(), 1).unwrap()
    }

    /// The payload against an empty document, as an option (the refusal is tested separately).
    fn payload_of(edit: EditCommit) -> Option<CommandPayload> {
        payload(edit, String::new(), &empty()).ok()
    }

    #[test]
    fn corner_trim_extend_and_break_commits_re_apply_the_same_domain_edit() {
        let cut = CornerCut::Fillet { radius: 2.0 };
        let reach = Length::from_mm(3.0).unwrap();
        let pick = Point::new(1.0, 1.0);
        assert_eq!(
            payload_of(EditCommit::CutCorner { target: id(1), pick, reach, cut, path: line() }),
            Some(CommandPayload::Edit(GeometryCommand::CutCorner { target: id(1), pick, reach, cut }))
        );
        assert_eq!(
            payload_of(EditCommit::Trim { target: id(1), pick, cutters: vec![id(2)], path: line() }),
            Some(CommandPayload::Edit(GeometryCommand::TrimAt { target: id(1), pick, cutters: vec![id(2)] }))
        );
        assert_eq!(
            payload_of(EditCommit::Extend {
                target: id(1),
                end: EndpointRef::End(0),
                boundaries: vec![id(2)],
                path: line()
            }),
            Some(CommandPayload::Edit(GeometryCommand::ExtendEnd {
                target: id(1),
                end: EndpointRef::End(0),
                boundaries: vec![id(2)]
            }))
        );
        assert_eq!(
            payload_of(EditCommit::Break { target: id(1), pick, path: line() }),
            Some(CommandPayload::Edit(GeometryCommand::BreakAt { target: id(1), pick }))
        );
    }

    #[test]
    fn mirror_in_place_is_a_transform() {
        let transform = Affine::mirror_x();
        assert_eq!(
            payload_of(EditCommit::Mirror { targets: vec![id(1)], transform, keep_original: false }),
            Some(CommandPayload::Transform { targets: vec![id(1)], transform })
        );
    }

    #[test]
    fn one_sketch_command_and_a_single_target_array_map_one_to_one() {
        let sketch = burnburn_document::sketch::SketchId::from_raw(5);
        let command = SketchCommand::AddConstraint {
            sketch,
            constraint: SketchConstraint::Relation(Relation::Horizontal(EntityId::from_raw(2))),
        };
        assert_eq!(
            payload_of(EditCommit::Sketch { sketch, commands: vec![command.clone()] }),
            Some(CommandPayload::Sketch(command.clone()))
        );
        let grid = GridArray {
            columns: 2,
            rows: 2,
            pitch_x: Length::from_mm(5.0).unwrap(),
            pitch_y: Length::from_mm(5.0).unwrap(),
        };
        let Ok(CommandPayload::Structure(StructureCommand::CreateModifier { chain })) = payload(
            EditCommit::AddArray { targets: vec![id(1)], modifier: Modifier::GridArray(grid) },
            "Raster".into(),
            &empty(),
        ) else {
            panic!("a modifier chain")
        };
        assert_eq!((chain.name.as_str(), chain.source, chain.steps.len()), ("Raster", id(1), 1));
        assert_eq!(chain.steps[0].modifier, Modifier::GridArray(grid));
        assert!(chain.steps[0].enabled);
        let _ = Vector::ZERO;
    }

    fn snapshot_of(objects: Vec<GeometryObject>) -> DocumentSnapshot {
        let next = objects.iter().map(|o| o.id().raw()).max().unwrap_or(0) + 1;
        DocumentSnapshot::from_parts(objects, next).unwrap()
    }

    fn drawn(raw: u64, name: &str, path: Path) -> GeometryObject {
        GeometryObject::new(id(raw), name.to_owned(), ObjectKind::Path, GeometrySource::Drawn, path)
    }

    fn bounds(w: f64, h: f64) -> Bounds {
        Bounds::from_corners(Point::ORIGIN, Point::new(w, h))
    }

    fn shape(params: ShapeParams) -> ParametricShape {
        ParametricShape::fit(params, bounds(40.0, 40.0)).unwrap()
    }

    #[test]
    fn ops_commits_are_plain_document_commands() {
        let ops = OpsCommand::PathOp { targets: vec![id(1)], op: PathOp::Reverse { subpaths: None } };
        let edit = EditCommit::Ops(ops.clone());
        assert_eq!(name_kind(&edit), None, "an operation creates nothing to name");
        assert_eq!(payload_of(edit), Some(CommandPayload::Edit(GeometryCommand::Ops(ops))));
    }

    #[test]
    fn a_parametric_commit_creates_a_shape_that_stays_editable() {
        let hexagon = shape(ShapeParams::Polygon { sides: 6 });
        assert_eq!(
            payload(EditCommit::CreateParametric { shape: hexagon }, "Veelhoek 1".into(), &empty()),
            Ok(CommandPayload::Shape(ShapeCommand::Create { shape: hexagon, name: "Veelhoek 1".into() }))
        );
    }

    #[test]
    fn basic_shapes_are_named_after_what_they_are() {
        let named = |params| {
            let kind = name_kind(&EditCommit::CreateParametric { shape: shape(params) }).expect("a creating commit");
            base_name(kind)
        };
        assert_eq!(named(ShapeParams::Polygon { sides: 3 }), "Driehoek");
        assert_eq!(named(ShapeParams::Polygon { sides: 5 }), "Vijfhoek");
        assert_eq!(named(ShapeParams::Polygon { sides: 8 }), "Achthoek");
        assert_eq!(named(ShapeParams::Polygon { sides: 7 }), "Veelhoek");
        assert_eq!(named(ShapeParams::Star { points: 5, ratio: 0.4 }), "Ster");
        assert_eq!(named(ShapeParams::DoubleStar { points: 6, ratio: 0.5 }), "Dubbele ster");
        assert_eq!(
            named(ShapeParams::RoundedRectangle { width: 1.0, height: 1.0, radius: 0.2 }),
            "Afgeronde rechthoek"
        );
    }

    fn horizontal(sketch: SketchId, line: u32) -> SketchCommand {
        SketchCommand::AddConstraint {
            sketch,
            constraint: SketchConstraint::Relation(Relation::Horizontal(EntityId::from_raw(line))),
        }
    }

    #[test]
    fn several_sketch_commands_become_one_atomic_batch() {
        let sketch = SketchId::from_raw(5);
        let step = |line: u32| {
            SketchStep::AddConstraint(SketchConstraint::Relation(Relation::Horizontal(EntityId::from_raw(line))))
        };
        assert_eq!(
            payload_of(EditCommit::Sketch { sketch, commands: vec![horizontal(sketch, 2), horizontal(sketch, 3)] }),
            Some(CommandPayload::Sketch(SketchCommand::Batch { sketch, steps: vec![step(2), step(3)] }))
        );
        // Only entity and constraint steps can be one transaction; anything else is refused whole.
        let parameter = SketchCommand::SetParameter { sketch, name: "a".into(), text: "1 mm".into() };
        assert_eq!(
            payload(
                EditCommit::Sketch { sketch, commands: vec![horizontal(sketch, 2), parameter] },
                String::new(),
                &empty()
            ),
            Err(Refusal::Unsupported)
        );
        let other = SketchId::from_raw(6);
        assert_eq!(
            payload(
                EditCommit::Sketch { sketch, commands: vec![horizontal(sketch, 2), horizontal(other, 3)] },
                String::new(),
                &empty()
            ),
            Err(Refusal::Unsupported),
            "a batch belongs to one sketch"
        );
    }

    #[test]
    fn a_mirrored_copy_is_a_new_object_next_to_the_original() {
        let snapshot = snapshot_of(vec![drawn(1, "Pad 1", line())]);
        let mirror = Affine::mirror_x();
        assert_eq!(
            payload(
                EditCommit::Mirror { targets: vec![id(1)], transform: mirror, keep_original: true },
                String::new(),
                &snapshot
            ),
            Ok(CommandPayload::Edit(GeometryCommand::CreatePath {
                path: line().transform(&mirror),
                name: "Pad 1 kopie".into()
            }))
        );
    }

    #[test]
    fn a_mirrored_copy_of_a_basic_shape_stays_a_basic_shape() {
        let star = shape(ShapeParams::Star { points: 5, ratio: 0.4 });
        let object = GeometryObject::new_parametric(
            id(1),
            "Ster 1".into(),
            ObjectKind::of_shape(star.params()),
            GeometrySource::Drawn,
            star,
        )
        .unwrap();
        let mirror = Affine::mirror_x();
        let Ok(CommandPayload::Shape(ShapeCommand::Create { shape: copy, name })) = payload(
            EditCommit::Mirror { targets: vec![id(1)], transform: mirror, keep_original: true },
            String::new(),
            &snapshot_of(vec![object]),
        ) else {
            panic!("a parametric copy")
        };
        assert_eq!(name, "Ster 1 kopie");
        assert_eq!(Some(copy), star.transformed(&mirror));
    }

    #[test]
    fn copies_and_arrays_of_several_objects_are_refused_with_a_reason() {
        let snapshot = snapshot_of(vec![drawn(1, "A", line()), drawn(2, "B", line())]);
        assert_eq!(
            payload(
                EditCommit::Mirror { targets: vec![id(1), id(2)], transform: Affine::mirror_x(), keep_original: true },
                String::new(),
                &snapshot
            ),
            Err(Refusal::SingleObjectOnly),
            "two copies would be two history entries"
        );
        let grid = GridArray {
            columns: 2,
            rows: 2,
            pitch_x: Length::from_mm(5.0).unwrap(),
            pitch_y: Length::from_mm(5.0).unwrap(),
        };
        assert_eq!(
            payload(
                EditCommit::AddArray { targets: vec![id(1), id(2)], modifier: Modifier::GridArray(grid) },
                String::new(),
                &snapshot
            ),
            Err(Refusal::SingleObjectOnly)
        );
        assert_eq!(
            payload(
                EditCommit::Mirror { targets: vec![id(9)], transform: Affine::mirror_x(), keep_original: true },
                String::new(),
                &snapshot
            ),
            Err(Refusal::Unsupported),
            "a copy of nothing"
        );
    }

    // End to end: a real editor tool on a real document session, mapped by `payload`.
    mod end_to_end {
        use std::f64::consts::PI;
        use std::time::Duration;

        use burnburn_document::sketch::{Sketch, SketchCommand};
        use burnburn_document::{CommandOutcome, DocumentCommand, DocumentSession, Evaluator, UndoLabel};
        use burnburn_editor::{
            CanvasInput, Editor, PointerEvent, PointerPhase, Selection, SnapSettings, ToolContext, ToolKind, ToolParam,
            ToolStep,
        };
        use burnburn_geometry::{Point, Rect, Shape};
        use burnburn_sketch::SketchEvaluator;
        use burnburn_sketch::solver::SketchSolverAdapter;
        use burnburn_types::{DocumentId, MonotonicTime, RequestId, StableId};

        use super::super::*;
        use burnburn_types::GeometryId;

        struct World {
            session: DocumentSession,
            editor: Editor,
        }

        impl World {
            fn new(tool: ToolKind) -> Self {
                let mut editor = Editor::new();
                editor.select_tool(tool);
                World { session: DocumentSession::new(DocumentId::from_raw(1)), editor }
            }

            fn apply(&mut self, payload: CommandPayload) -> CommandOutcome {
                let cmd = DocumentCommand {
                    document: self.session.id(),
                    expected: self.session.revision(),
                    request: RequestId::from_raw(1),
                    payload,
                };
                let sketches = SketchEvaluator::new(SketchSolverAdapter);
                let evaluators: [&dyn Evaluator; 1] = [&sketches];
                self.session.apply_with(cmd, &evaluators).unwrap()
            }

            fn shape(&mut self, shape: Shape) -> GeometryId {
                self.apply(CommandPayload::Edit(GeometryCommand::CreateShape { shape, name: String::new() }))
                    .changes
                    .created[0]
            }

            fn click(&mut self, x: f64, y: f64) -> Vec<ToolStep> {
                let snapshot = self.session.snapshot();
                let selection = Selection::default();
                let snap = SnapSettings { objects: false, ..SnapSettings::default() };
                let ctx = ToolContext { document: &snapshot, selection: &selection, mm_per_point: 0.5, snap: &snap };
                let t = MonotonicTime::from_offset(Duration::from_millis(1));
                [PointerPhase::Down, PointerPhase::Up]
                    .into_iter()
                    .map(|phase| {
                        self.editor.input(&CanvasInput::Pointer(PointerEvent::mouse(phase, Point::new(x, y), t)), &ctx)
                    })
                    .collect()
            }

            /// Applies the gesture's commit the way the controller does.
            fn commit(&mut self, steps: Vec<ToolStep>) -> CommandOutcome {
                let commit = steps
                    .into_iter()
                    .find_map(|s| if let ToolStep::Commit(c) = s { Some(c) } else { None })
                    .expect("the gesture commits");
                let payload = payload(commit, String::new(), &self.session.snapshot()).expect("a document command");
                self.apply(payload)
            }
        }

        #[test]
        fn a_fillet_click_rounds_the_corner_in_one_undoable_step() {
            let mut w = World::new(ToolKind::Fillet);
            let rect =
                Rect::new(Point::ORIGIN, Length::from_mm(10.0).unwrap(), Length::from_mm(10.0).unwrap()).unwrap();
            let id = w.shape(Shape::Rectangle(rect));
            assert!(w.editor.set_param(ToolParam::Modify(burnburn_editor::ModifyParam::Radius(2.0))));
            let steps = w.click(0.4, 0.4);
            let out = w.commit(steps);
            assert_eq!(out.changes.modified, vec![id]);
            assert_eq!(w.session.undo_label(), Some(UndoLabel::Edit(burnburn_document::EditLabel::CutCorner)));
            let area = w.session.snapshot().object(id).unwrap().path().signed_area();
            assert!((area - (100.0 - (4.0 - PI))).abs() < 1e-9, "{area}");
            w.session.undo(w.session.revision()).unwrap();
            assert!((w.session.snapshot().object(id).unwrap().path().signed_area() - 100.0).abs() < 1e-12);
        }

        #[test]
        fn a_trim_click_cuts_a_circle_at_a_line() {
            let mut w = World::new(ToolKind::Trim);
            let circle = w.shape(Shape::Circle { center: Point::ORIGIN, radius: Length::from_mm(10.0).unwrap() });
            w.shape(Shape::Line { from: Point::new(0.0, -20.0), to: Point::new(0.0, 20.0) });
            let steps = w.click(10.0, 0.0);
            w.commit(steps);
            let path = w.session.snapshot().object(circle).unwrap().path().clone();
            assert!(!path.subpaths()[0].is_closed());
            assert!((path.bounds().unwrap().max().x).abs() < 1e-9, "only the left half remains");
        }

        #[test]
        fn the_construction_tool_removes_a_sketch_line_from_the_output() {
            let mut w = World::new(ToolKind::Construction);
            let mut sketch = Sketch::new("S");
            let a = sketch.point(0.0, 0.0).unwrap();
            let b = sketch.point(10.0, 0.0).unwrap();
            sketch.line(a, b).unwrap();
            w.apply(CommandPayload::Sketch(SketchCommand::Create { sketch: Box::new(sketch) }));
            assert_eq!(w.session.snapshot().len(), 1, "the line is sketch output");
            let steps = w.click(5.0, 0.1);
            w.commit(steps);
            assert_eq!(w.session.snapshot().len(), 0, "construction geometry is not output");
            w.session.undo(w.session.revision()).unwrap();
            assert_eq!(w.session.snapshot().len(), 1);
        }
    }
}
