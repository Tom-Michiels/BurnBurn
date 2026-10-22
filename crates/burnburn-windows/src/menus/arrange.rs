//! Schikken (Arrange): grouping, flipping and rotating, align / distribute / dock / move together,
//! nesting, the Schikken panel, drawing order, lock and hide.

use super::{MenuNode, cmd, cmds, nested, sep};
use crate::adapter::MenuItem;
use crate::l10n::{HostText, Locale, host_text};
use crate::panels::{SchikkenText, schikken_text};
use burnburn_presentation::CommandKey;

pub(super) fn items(locale: Locale) -> Vec<MenuNode> {
    let mut items = cmds(&[CommandKey::Group, CommandKey::Ungroup, CommandKey::AutoGroup, CommandKey::BreakApart]);
    items.push(sep());
    items.extend(cmds(&[
        CommandKey::FlipHorizontal,
        CommandKey::FlipVertical,
        CommandKey::Rotate90Cw,
        CommandKey::Rotate90Ccw,
    ]));
    items.push(sep());
    items.push(nested(
        host_text(HostText::MenuAlign, locale),
        cmds(&[
            CommandKey::AlignLeft,
            CommandKey::AlignCenterX,
            CommandKey::AlignRight,
            CommandKey::AlignTop,
            CommandKey::AlignCenterY,
            CommandKey::AlignBottom,
        ]),
    ));
    items.push(nested(
        host_text(HostText::MenuDistribute, locale),
        cmds(&[CommandKey::DistributeHorizontal, CommandKey::DistributeVertical]),
    ));
    items.push(nested(
        schikken_text(SchikkenText::MenuDock, locale),
        cmds(&[CommandKey::DockLeft, CommandKey::DockRight, CommandKey::DockUp, CommandKey::DockDown]),
    ));
    items.push(nested(
        schikken_text(SchikkenText::MenuMoveTogether, locale),
        cmds(&[CommandKey::MoveTogetherHorizontal, CommandKey::MoveTogetherVertical]),
    ));
    items.push(sep());
    items.push(cmd(CommandKey::QuickNest));
    items.push(cmd(CommandKey::NestSelected));
    items.push(MenuNode::Item(MenuItem::SchikkenPanel));
    items.push(sep());
    items.extend(cmds(&[
        CommandKey::PushForward,
        CommandKey::PushBackward,
        CommandKey::PushToFront,
        CommandKey::PushToBack,
    ]));
    items.push(sep());
    items.extend(cmds(&[
        CommandKey::LockSelected,
        CommandKey::UnlockSelected,
        CommandKey::UnlockAll,
        CommandKey::HideSelected,
        CommandKey::IsolateSelection,
        CommandKey::ShowAll,
    ]));
    items
}
