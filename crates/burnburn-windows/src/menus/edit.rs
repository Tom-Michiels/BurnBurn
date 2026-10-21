//! Wijzig (Edit): undo, clipboard, duplicate, delete, the selection commands and the two CAM
//! dialogs that belong to the document (Optimization Settings) and the machine (Device Settings).

use super::{MenuNode, cmd, cmds, sep};
use crate::adapter::MenuItem;
use burnburn_presentation::{CamDialog, CommandKey};

pub(super) fn items() -> Vec<MenuNode> {
    let mut items = cmds(&[CommandKey::Undo, CommandKey::Redo]);
    items.push(sep());
    items.extend(cmds(&[
        CommandKey::Cut,
        CommandKey::Copy,
        CommandKey::Paste,
        CommandKey::PasteInPlace,
        CommandKey::Duplicate,
        CommandKey::Delete,
    ]));
    items.push(sep());
    items.extend(cmds(&[
        CommandKey::SelectAll,
        CommandKey::InvertSelection,
        CommandKey::SelectOpenShapes,
        CommandKey::SelectInLayer,
        CommandKey::SelectContained,
        CommandKey::SelectSmaller,
        CommandKey::SelectCircles,
    ]));
    items.push(sep());
    items.push(cmd(CommandKey::DeleteDuplicates));
    items.push(sep());
    items.push(MenuNode::Item(MenuItem::CamDialog(CamDialog::Optimization)));
    items.push(MenuNode::Item(MenuItem::CamDialog(CamDialog::Device)));
    items
}
