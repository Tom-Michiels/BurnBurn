//! The `WM_COMMAND` id space of lane W4-L8b: `6000..=7999`.
//!
//! * `6000..6499`: main-menu items that open a panel or dialog of a family, `100` per family.
//! * `7000..7999`: controls inside the panel windows, `200` per family.
//!
//! Other lanes must stay out of `6000..=7999` (the macOS tag ranges are different and unrelated).

use super::Family;

pub const RANGE: std::ops::RangeInclusive<u16> = 6000..=7999;

pub const MENU_BASE: u16 = 6000;
/// Width of a family's menu-id block.
pub const MENU_BLOCK: u16 = 100;

pub const CONTROL_BASE: u16 = 7000;
/// Width of a family's control-id block.
pub const CONTROL_BLOCK: u16 = 200;

/// First control id of each family's panel (`CONTROL_BASE + n * CONTROL_BLOCK`).
pub const LASER_CONTROLS: u16 = CONTROL_BASE;
pub const IMAGE_CONTROLS: u16 = CONTROL_BASE + CONTROL_BLOCK;
pub const FILES_CONTROLS: u16 = CONTROL_BASE + 2 * CONTROL_BLOCK;
pub const CAMERA_CONTROLS: u16 = CONTROL_BASE + 3 * CONTROL_BLOCK;
pub const APP_CONTROLS: u16 = CONTROL_BASE + 4 * CONTROL_BLOCK;

const FAMILIES: [Family; 5] = [Family::Laser, Family::Image, Family::Files, Family::Camera, Family::App];

fn slot(family: Family) -> u16 {
    FAMILIES.iter().position(|f| *f == family).map_or(0, |i| i as u16)
}

/// First menu id of a family.
pub fn menu_base(family: Family) -> u16 {
    MENU_BASE + slot(family) * MENU_BLOCK
}

/// First control id of a family's panel.
pub fn control_base(family: Family) -> u16 {
    CONTROL_BASE + slot(family) * CONTROL_BLOCK
}

/// The family that owns a menu id, and the item number inside its block.
pub fn menu_owner(id: u16) -> Option<(Family, u16)> {
    owner(id, MENU_BASE, MENU_BLOCK)
}

/// The family that owns a control id, and the control number inside its block.
pub fn control_owner(id: u16) -> Option<(Family, u16)> {
    owner(id, CONTROL_BASE, CONTROL_BLOCK)
}

fn owner(id: u16, base: u16, block: u16) -> Option<(Family, u16)> {
    let offset = id.checked_sub(base)?;
    let family = *FAMILIES.get(usize::from(offset / block))?;
    Some((family, offset % block))
}
