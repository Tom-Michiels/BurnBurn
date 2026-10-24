//! Gereedschap (Tools) with the Vormen (Shapes), Knopen (Nodes), Schets (Sketch) and Beeld (Image)
//! submenus (LF16), and Tekst (Text).

use super::{MenuNode, cmd, cmds, image, nested, sep};
use crate::l10n::{HostText, Locale, host_text};
use burnburn_presentation::CommandKey;

pub(super) fn tools_items(locale: Locale) -> Vec<MenuNode> {
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
    items.push(sep());
    items.push(nested(host_text(HostText::MenuShapes, locale), shapes_items()));
    items.push(nested(host_text(HostText::MenuNodes, locale), node_items()));
    items.push(nested(host_text(HostText::MenuSketch, locale), sketch_items()));
    items.push(nested(host_text(HostText::ImageMenu, locale), image::items()));
    items
}

fn shapes_items() -> Vec<MenuNode> {
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
    items
}

fn node_items() -> Vec<MenuNode> {
    cmds(&[
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
    ])
}

/// Tekst: text on a path, then the generators. (Convert to Path is in Gereedschap ▸ Vormen.)
pub(super) fn text_items() -> Vec<MenuNode> {
    vec![
        cmd(CommandKey::TextOnPath),
        sep(),
        cmd(CommandKey::BarcodeNew),
        cmd(CommandKey::GeneratorNew),
        cmd(CommandKey::GeneratorEdit),
    ]
}

fn sketch_items() -> Vec<MenuNode> {
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
