//! Laser, in the order of the macOS menu: the laser window, compile and RD export (Productie was
//! merged into this menu, LF16), the CAM settings in the Operations submenu, then the CAM tests,
//! Print and Cut and the Center Finder. The camera window is Windows-only and sits last.

use super::{MenuNode, cmd, cmds, host, nested, sep};
use crate::adapter::MenuItem;
use crate::host_panels::menu::Item;
use crate::l10n::{HostText, Locale, host_text};
use burnburn_presentation::{CamDialog, CommandKey};

pub(super) fn items(locale: Locale) -> Vec<MenuNode> {
    let mut items = vec![host(Item::LaserWindow), sep()];
    items.extend(cmds(&[CommandKey::Compile, CommandKey::ExportRd]));
    items.push(nested(
        host_text(HostText::MenuOperations, locale),
        [CamDialog::Optimization, CamDialog::Order, CamDialog::Sublayers, CamDialog::Device]
            .into_iter()
            .map(|d| MenuNode::Item(MenuItem::CamDialog(d)))
            .collect(),
    ));
    items.push(sep());
    items.extend([
        MenuNode::Item(MenuItem::CamDialog(CamDialog::MaterialTest)),
        MenuNode::Item(MenuItem::CamDialog(CamDialog::IntervalTest)),
        MenuNode::Item(MenuItem::CamDialog(CamDialog::FocusTest)),
        MenuNode::Item(MenuItem::CamDialog(CamDialog::Feeder)),
        MenuNode::Item(MenuItem::CamDialog(CamDialog::Preflight)),
        cmd(CommandKey::PrintAndCut),
        cmd(CommandKey::CenterFinder),
        sep(),
        host(Item::CameraWindow),
    ]);
    items
}
