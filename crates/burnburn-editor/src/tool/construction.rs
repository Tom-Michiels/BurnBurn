//! The construction tool: toggles the construction flag of a sketch entity.

use burnburn_document::sketch::SketchCommand;
use burnburn_geometry::Point;

use super::modify::{Action, Rejection, Unavailable, outline_hit, rejected};
use super::sketch_pick::pick_entity;
use super::{CLICK_TOLERANCE_PT, EditCommit, ToolContext, ToolStep};
use crate::input::PointerEvent;

#[derive(Debug, Clone, Default)]
pub(super) struct ConstructionAction;

impl Action for ConstructionAction {
    fn press(&mut self, ctx: &ToolContext<'_>, _at: Point, ev: &PointerEvent) -> ToolStep {
        if let Some(pick) = pick_entity(ctx.document, ev.position, ctx.tolerance_mm(CLICK_TOLERANCE_PT)) {
            let current =
                ctx.document.sketch(pick.sketch).and_then(|s| s.entity(pick.entity)).is_some_and(|e| e.construction);
            return ToolStep::Commit(EditCommit::Sketch {
                sketch: pick.sketch,
                commands: vec![SketchCommand::SetConstruction {
                    sketch: pick.sketch,
                    entity: pick.entity,
                    construction: !current,
                }],
            });
        }
        if outline_hit(ctx, ev.position).is_some() {
            return rejected(Rejection::Unavailable(Unavailable::ConstructionOutsideSketch));
        }
        rejected(Rejection::NothingPicked)
    }

    fn reset(&mut self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use burnburn_document::sketch::{EntityId, Sketch, SketchCommand};

    use super::super::fixture::*;
    use super::super::modify::kit::*;
    use super::super::modify::{Rejection, Unavailable};
    use super::super::{EditCommit, ToolKind};

    fn sketch() -> Sketch {
        let mut s = Sketch::new("S");
        let a = s.point(0.0, 0.0).unwrap();
        let b = s.point(10.0, 0.0).unwrap();
        s.line(a, b).unwrap();
        let (c, d) = (s.point(0.0, 10.0).unwrap(), s.point(10.0, 10.0).unwrap());
        s.construction_line(c, d).unwrap();
        s
    }

    fn set(entity: u32, construction: bool) -> EditCommit {
        EditCommit::Sketch {
            sketch: sketch_id(),
            commands: vec![SketchCommand::SetConstruction {
                sketch: sketch_id(),
                entity: EntityId::from_raw(entity),
                construction,
            }],
        }
    }

    #[test]
    fn a_click_flips_the_flag_of_the_entity() {
        let f = with_sketch(vec![], sketch());
        let mut ed = editor(ToolKind::Construction);
        assert_eq!(only(&click(&mut ed, &f, 5.0, 0.2)), set(2, true));
        assert_eq!(only(&click(&mut ed, &f, 5.0, 10.2)), set(5, false));
    }

    #[test]
    fn drawn_objects_have_no_construction_flag_yet_and_the_tool_says_so() {
        let f = with_objects(vec![rect(1, 50.0, 50.0, 10.0, 10.0)]);
        let mut ed = editor(ToolKind::Construction);
        let steps = click(&mut ed, &f, 55.0, 50.0);
        assert!(commits(&steps).is_empty());
        assert_eq!(rejection(&steps), Some(Rejection::Unavailable(Unavailable::ConstructionOutsideSketch)));
        assert_eq!(rejection(&click(&mut ed, &f, 0.0, 200.0)), Some(Rejection::NothingPicked));
    }
}
