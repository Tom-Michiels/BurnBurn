//! Editor commit → document payload. This is the single place where an interaction result meets
//! the document command set (contract §4: the editor knows no `DocumentCommand`).

use burnburn_document::structure::{EvalMode, ModifierChain, ModifierStep, StructureCommand};
use burnburn_document::{CommandPayload, GeometryCommand};
use burnburn_editor::{EditCommit, ShapeKind};
use burnburn_geometry::{BooleanOptions, FillRule, ProductionTolerance, Shape};
use burnburn_types::Length;

/// Default object names (NL; the document stores user data, not localisation keys).
pub(crate) fn base_name(kind: NameKind) -> &'static str {
    match kind {
        NameKind::Rectangle => "Rechthoek",
        NameKind::Shape(ShapeKind::Ellipse) => "Ellips",
        NameKind::Shape(ShapeKind::Circle) => "Cirkel",
        NameKind::Shape(ShapeKind::Line) => "Lijn",
        NameKind::Shape(ShapeKind::Polyline) => "Polylijn",
        NameKind::Shape(_) => "Curve",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum NameKind {
    Rectangle,
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
        _ => None,
    }
}

/// The document payload for one editor commit (one command, one history entry).
pub(crate) fn payload(edit: EditCommit, name: String) -> Option<CommandPayload> {
    Some(match edit {
        EditCommit::CreateRectangle { rect } => CommandPayload::CreateRectangle { rect, name },
        EditCommit::Transform { targets, transform, .. } => CommandPayload::Transform { targets, transform },
        EditCommit::CreateShape { shape } => CommandPayload::Edit(GeometryCommand::CreateShape { shape, name }),
        EditCommit::CreatePath { path } => CommandPayload::Edit(GeometryCommand::CreatePath { path, name }),
        // The document re-applies the same domain edit; the preview path is not trusted input.
        EditCommit::EditNodes { target, edit, .. } => CommandPayload::Edit(GeometryCommand::EditNodes { target, edit }),
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
        // Mirrored copies need a document duplicate command; until it exists nothing is applied.
        EditCommit::Mirror { targets, transform, keep_original: false } => {
            CommandPayload::Transform { targets, transform }
        }
        // Several sketch commands would be several history entries; they wait for an atomic command.
        EditCommit::Sketch { mut commands, .. } if commands.len() == 1 => CommandPayload::Sketch(commands.remove(0)),
        // A chain has one source object: several targets would need an atomic batch.
        EditCommit::AddArray { targets, modifier } if targets.len() == 1 => {
            let tolerance = ProductionTolerance::new(Length::from_mm(BOOLEAN_TOLERANCE_MM).ok()?).ok()?;
            CommandPayload::Structure(StructureCommand::CreateModifier {
                chain: ModifierChain {
                    name: if name.is_empty() { "Patroon".to_owned() } else { name },
                    source: targets[0],
                    steps: vec![ModifierStep { modifier, enabled: true }],
                    tolerance,
                    mode: EvalMode::Inline,
                },
            })
        }
        _ => return None,
    })
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
    use burnburn_document::sketch::{EntityId, Relation, SketchCommand, SketchConstraint};
    use burnburn_document::structure::{Modifier, StructureCommand};
    use burnburn_editor::EditCommit;
    use burnburn_geometry::array::GridArray;
    use burnburn_geometry::{Affine, CornerCut, EndpointRef, Path, Point, Vector};
    use burnburn_types::{GeometryId, Length, StableId};

    use super::*;

    fn id(raw: u64) -> GeometryId {
        GeometryId::from_raw(raw)
    }

    fn line() -> Path {
        Path::line(Point::ORIGIN, Point::new(10.0, 0.0)).unwrap()
    }

    #[test]
    fn corner_trim_extend_and_break_commits_re_apply_the_same_domain_edit() {
        let cut = CornerCut::Fillet { radius: 2.0 };
        let reach = Length::from_mm(3.0).unwrap();
        let pick = Point::new(1.0, 1.0);
        let payload_of = |c| payload(c, String::new());
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
    fn mirror_in_place_is_a_transform_and_copies_wait_for_a_document_command() {
        let transform = Affine::mirror_x();
        assert_eq!(
            payload(EditCommit::Mirror { targets: vec![id(1)], transform, keep_original: false }, String::new()),
            Some(CommandPayload::Transform { targets: vec![id(1)], transform })
        );
        assert_eq!(
            payload(EditCommit::Mirror { targets: vec![id(1)], transform, keep_original: true }, String::new()),
            None,
            "no duplicate command exists yet: nothing is applied instead of the wrong thing"
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
            payload(EditCommit::Sketch { sketch, commands: vec![command.clone()] }, String::new()),
            Some(CommandPayload::Sketch(command.clone()))
        );
        assert_eq!(
            payload(EditCommit::Sketch { sketch, commands: vec![command.clone(), command] }, String::new()),
            None,
            "several commands need one atomic document command"
        );
        let grid = GridArray {
            columns: 2,
            rows: 2,
            pitch_x: Length::from_mm(5.0).unwrap(),
            pitch_y: Length::from_mm(5.0).unwrap(),
        };
        let Some(CommandPayload::Structure(StructureCommand::CreateModifier { chain })) = payload(
            EditCommit::AddArray { targets: vec![id(1)], modifier: Modifier::GridArray(grid) },
            "Raster".into(),
        ) else {
            panic!("a modifier chain")
        };
        assert_eq!((chain.name.as_str(), chain.source, chain.steps.len()), ("Raster", id(1), 1));
        assert_eq!(chain.steps[0].modifier, Modifier::GridArray(grid));
        assert!(chain.steps[0].enabled);
        let _ = Vector::ZERO;
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
                let payload = payload(commit, String::new()).expect("a document command");
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
