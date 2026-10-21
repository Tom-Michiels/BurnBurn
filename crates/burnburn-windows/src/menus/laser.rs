//! Laser: the CAM tests, Print and Cut and the Center Finder. W4-L8b adds the laser window as the
//! first item (followed by a separator), as the macOS menu has it.

use super::{MenuNode, cmd};
use crate::adapter::MenuItem;
use burnburn_presentation::{CamDialog, CommandKey};

pub(super) fn items() -> Vec<MenuNode> {
    vec![
        // W4-L8b: Laservenster, separator.
        MenuNode::Item(MenuItem::CamDialog(CamDialog::MaterialTest)),
        MenuNode::Item(MenuItem::CamDialog(CamDialog::IntervalTest)),
        MenuNode::Item(MenuItem::CamDialog(CamDialog::FocusTest)),
        cmd(CommandKey::PrintAndCut),
        cmd(CommandKey::CenterFinder),
    ]
}
