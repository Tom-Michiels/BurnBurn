//! Wijzig (Edit): undo, clipboard, duplicate, delete and the selection commands (the less common
//! ones in the Select submenu). The CAM dialogs moved to Laser ▸ Operations (LF16).

use super::{MenuNode, cmd, cmds, nested, sep};
use crate::l10n::{HostText, Locale, host_text};
use burnburn_presentation::CommandKey;

pub(super) fn items(locale: Locale) -> Vec<MenuNode> {
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
    items.extend(cmds(&[CommandKey::SelectAll, CommandKey::InvertSelection]));
    items.push(nested(
        host_text(HostText::MenuSelect, locale),
        cmds(&[
            CommandKey::SelectOpenShapes,
            CommandKey::SelectInLayer,
            CommandKey::SelectContained,
            CommandKey::SelectSmaller,
            CommandKey::SelectCircles,
            CommandKey::SelectSameType,
        ]),
    ));
    items.push(sep());
    items.push(cmd(CommandKey::DeleteDuplicates));
    items
}
