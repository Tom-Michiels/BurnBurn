//! Weergave (View): zoom and the wireframe toggle. (The Mac adds Show Toolbar and Customize
//! Toolbar… here; the Windows tool strip has no such switch.)

use super::{MenuNode, cmd, cmds, sep};
use burnburn_presentation::CommandKey;

pub(super) fn items() -> Vec<MenuNode> {
    let mut items = cmds(&[
        CommandKey::ZoomPage,
        CommandKey::ZoomBed,
        CommandKey::ZoomSelection,
        CommandKey::ZoomIn,
        CommandKey::ZoomOut,
    ]);
    items.push(sep());
    items.push(cmd(CommandKey::ViewWireframe));
    items
}
