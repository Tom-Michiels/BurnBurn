//! Which keys a control keeps for itself when the panel window runs the dialog manager
//! (`IsDialogMessage`: Tab moves the focus, Return presses the default button, Escape cancels).
//! Pure decisions, tested on every OS; the Win32 layer answers `WM_GETDLGCODE` with them.

use super::{ControlKind, Family};

pub const VK_TAB: u16 = 0x09;
pub const VK_RETURN: u16 = 0x0D;

/// Whether the control wants the key press `vk` itself instead of the dialog manager. Tab is never
/// kept, except while the shortcut editor records a key (then every key is the answer).
pub fn control_wants_key(family: Family, kind: &ControlKind, capturing: bool, vk: u16) -> bool {
    if capturing {
        return true;
    }
    if family == Family::Laser && super::laser::jog_control_for_vk(vk).is_some() {
        return true;
    }
    matches!(kind, ControlKind::Edit) && vk == VK_RETURN
}

/// Whether the canvas keeps the key press `vk` (arrows nudge, Return/Escape/Delete end or edit
/// something) instead of the dialog manager. Tab always moves the focus on.
pub fn canvas_wants_key(vk: u16) -> bool {
    vk != VK_TAB
}
