//! Gereedschap (Tools), Vormen (Shapes, with the node commands), Tekst (Text) and Schets (Sketch).

use super::{MenuNode, cmd, cmds, sep};
use burnburn_presentation::CommandKey;

pub(super) fn tools_items() -> Vec<MenuNode> {
    let mut items = cmds(&[
        CommandKey::ToolSelect,
        CommandKey::ToolRectangle,
        CommandKey::ToolEllipse,
        CommandKey::ToolLine,
        CommandKey::ToolPen,
        CommandKey::ToolNode,
        CommandKey::ToolTrim,
        CommandKey::ToolWarp,
        CommandKey::ToolDeform,
        CommandKey::ResizeSlots,
    ]);
    items.push(sep());
    items.extend(cmds(&[CommandKey::BooleanUnion, CommandKey::BooleanDifference, CommandKey::BooleanIntersection]));
    items.push(sep());
    items.extend(cmds(&[CommandKey::SnapToObjects, CommandKey::SnapToGrid]));
    items.push(sep());
    items.push(cmd(CommandKey::Measure));
    items
}

pub(super) fn shapes_items() -> Vec<MenuNode> {
    let mut items = cmds(&[
        CommandKey::ShapesJoin,
        CommandKey::ShapesCut,
        CommandKey::ShapesRoundCorners,
        CommandKey::ShapesOptimize,
    ]);
    items.push(sep());
    items.extend(cmds(&[CommandKey::ShapesReverse, CommandKey::ShapesClockwise, CommandKey::ShapesCounterClockwise]));
    items.push(sep());
    items.extend(cmds(&[CommandKey::LockPathStart, CommandKey::LockPathDirection]));
    items.push(sep());
    items.push(cmd(CommandKey::ShapesConvertToPath));
    items.push(sep());
    items.extend(cmds(&[
        CommandKey::NodeDelete,
        CommandKey::NodeBreak,
        CommandKey::NodeSmooth,
        CommandKey::NodeSymmetric,
        CommandKey::NodeCorner,
        CommandKey::NodeToLine,
        CommandKey::NodeToCurve,
        CommandKey::NodeClosePath,
        CommandKey::NodeJoinEnds,
        CommandKey::NodeStartPoint,
    ]));
    items
}

pub(super) fn text_items() -> Vec<MenuNode> {
    vec![
        cmd(CommandKey::TextOnPath),
        cmd(CommandKey::ShapesConvertToPath),
        sep(),
        cmd(CommandKey::BarcodeNew),
        cmd(CommandKey::GeneratorNew),
        cmd(CommandKey::GeneratorEdit),
    ]
}

pub(super) fn sketch_items() -> Vec<MenuNode> {
    let mut items = cmds(&[
        CommandKey::SketchNew,
        CommandKey::SketchFromSelection,
        CommandKey::SketchEdit,
        CommandKey::SketchFinish,
    ]);
    items.push(sep());
    items.extend(cmds(&[
        CommandKey::SketchCoincident,
        CommandKey::SketchHorizontal,
        CommandKey::SketchVertical,
        CommandKey::SketchParallel,
        CommandKey::SketchPerpendicular,
        CommandKey::SketchEqual,
        CommandKey::SketchTangent,
        CommandKey::SketchFix,
    ]));
    items.push(sep());
    items.extend(cmds(&[CommandKey::SketchDimension, CommandKey::SketchRemoveConstraint]));
    items
}
