//! Archief (File): the basic items. W4-L8b adds import, export, bundles, templates and recent
//! projects here, between Open and Close, as the macOS menu has them.

use super::{MenuNode, cmd, sep};
use crate::adapter::MenuItem;
use burnburn_presentation::CommandKey;

pub(super) fn items() -> Vec<MenuNode> {
    vec![
        cmd(CommandKey::New),
        MenuNode::Item(MenuItem::Open),
        // W4-L8b: Importeer, Exporteren, Bundels, Recente projecten, Nieuw van template.
        // G5-grbl-ui: Exporteer G-code (on for a compiled plan of a GRBL machine), as on the Mac.
        cmd(CommandKey::ExportGcode),
        sep(),
        cmd(CommandKey::Close),
        cmd(CommandKey::Save),
        cmd(CommandKey::SaveAs),
    ]
}
