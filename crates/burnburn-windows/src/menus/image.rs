//! Beeld (Image), in the order of the macOS menu. The image window is Windows-only and sits last.

use super::{MenuNode, cmd, host, sep};
use crate::host_panels::menu::Item;
use burnburn_presentation::CommandKey;

pub(super) fn items() -> Vec<MenuNode> {
    vec![
        host(Item::ImportImage),
        cmd(CommandKey::AdjustImage),
        cmd(CommandKey::MaskImage),
        sep(),
        cmd(CommandKey::ReplaceImage),
        cmd(CommandKey::RefreshImage),
        cmd(CommandKey::SaveProcessedBitmap),
        sep(),
        host(Item::TraceImage),
        cmd(CommandKey::TraceEdit),
        sep(),
        host(Item::ImagePanel),
    ]
}
