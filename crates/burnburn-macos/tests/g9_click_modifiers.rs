//! Golf-9 integration (bug found by G9-cam-werk: the native `layer_palette` test once saw a plain
//! palette click arrive as a Shift-click). The palette took the modifier flags of whatever event the
//! application processed last; only the click's own mouse event may decide Shift.

use burnburn_macos::adapter::{click_modifier_flags, flags};

const LEFT_DOWN: usize = 1;
const LEFT_UP: usize = 2;
const RIGHT_UP: usize = 4;
const KEY_DOWN: usize = 10;
const FLAGS_CHANGED: usize = 12;
const APP_KIT_DEFINED: usize = 13;
const SYSTEM_DEFINED: usize = 14;

#[test]
fn a_mouse_click_keeps_its_own_modifiers() {
    for t in [LEFT_DOWN, LEFT_UP, RIGHT_UP] {
        assert_eq!(click_modifier_flags(Some(t), flags::SHIFT), flags::SHIFT, "event type {t}");
        assert_eq!(click_modifier_flags(Some(t), 0), 0);
    }
}

#[test]
fn any_other_current_event_adds_no_modifiers() {
    for t in [KEY_DOWN, FLAGS_CHANGED, APP_KIT_DEFINED, SYSTEM_DEFINED] {
        assert_eq!(click_modifier_flags(Some(t), flags::SHIFT | flags::COMMAND), 0, "event type {t}");
    }
    assert_eq!(click_modifier_flags(None, flags::SHIFT), 0, "no current event (performClick)");
}
