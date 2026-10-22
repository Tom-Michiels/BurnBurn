//! Laser, in the order of the macOS menu: the laser window, then the CAM tests, Print and Cut and the
//! Center Finder. The camera window is Windows-only and sits last.

use super::{MenuNode, cmd, host, sep};
use crate::adapter::MenuItem;
use crate::host_panels::menu::Item;
use burnburn_presentation::{CamDialog, CommandKey};

pub(super) fn items() -> Vec<MenuNode> {
    vec![
        host(Item::LaserWindow),
        sep(),
        MenuNode::Item(MenuItem::CamDialog(CamDialog::MaterialTest)),
        MenuNode::Item(MenuItem::CamDialog(CamDialog::IntervalTest)),
        MenuNode::Item(MenuItem::CamDialog(CamDialog::FocusTest)),
        MenuNode::Item(MenuItem::CamDialog(CamDialog::Feeder)),
        MenuNode::Item(MenuItem::CamDialog(CamDialog::Preflight)),
        cmd(CommandKey::PrintAndCut),
        cmd(CommandKey::CenterFinder),
        sep(),
        host(Item::CameraWindow),
    ]
}
