//! LF13: which Win32 window class and style each control of the form builder becomes. Pure data
//! (`forms::spec_win32`), so the mapping is checked on every host; `win32/spec_form.rs` only feeds
//! it to `CreateWindowExW`. Real window creation is not run here (no Windows machine).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_windows::forms::spec_layout::SpecClass;
use burnburn_windows::forms::spec_win32::{WinSpec, control_id, win_spec};

const WS_TABSTOP: u32 = 0x0001_0000;
const WS_DISABLED: u32 = 0x0800_0000;
const WS_VSCROLL: u32 = 0x0020_0000;
const BS_DEFPUSHBUTTON: u32 = 0x0001;
const BS_AUTOCHECKBOX: u32 = 0x0003;
const ES_RIGHT: u32 = 0x0002;
const ES_MULTILINE: u32 = 0x0004;
const ES_READONLY: u32 = 0x0800;
const CBS_DROPDOWNLIST: u32 = 0x0003;
const SS_LEFT: u32 = 0x0000;

fn spec(class: SpecClass, enabled: bool) -> WinSpec {
    win_spec(class, enabled)
}

#[test]
fn input_controls_are_tab_stops_and_decoration_is_not() {
    let stops = [
        SpecClass::Edit { numeric: false },
        SpecClass::Edit { numeric: true },
        SpecClass::Combo,
        SpecClass::Check,
        SpecClass::Memo,
        SpecClass::Button,
        SpecClass::Tool,
        SpecClass::Tabs,
        SpecClass::DialogButton { default: false },
        SpecClass::DialogButton { default: true },
    ];
    for class in stops {
        assert_ne!(spec(class, true).style & WS_TABSTOP, 0, "{class:?}");
    }
    let still = [SpecClass::Heading, SpecClass::Caption, SpecClass::Suffix, SpecClass::Help, SpecClass::Error];
    for class in still {
        let s = spec(class, true);
        assert_eq!((s.style & WS_TABSTOP, s.class), (0, "STATIC"), "{class:?}");
    }
}

#[test]
fn each_class_gets_the_standard_control_and_style() {
    assert_eq!(spec(SpecClass::Check, true).class, "BUTTON");
    assert_eq!(spec(SpecClass::Check, true).style & 0xF, BS_AUTOCHECKBOX);
    assert_eq!(spec(SpecClass::Combo, true).class, "COMBOBOX");
    assert_eq!(spec(SpecClass::Combo, true).style & 0xF, CBS_DROPDOWNLIST);
    assert_eq!(spec(SpecClass::Tabs, true).class, "SysTabControl32");
    assert_eq!(spec(SpecClass::Edit { numeric: false }, true).class, "EDIT");
    assert_eq!(spec(SpecClass::Edit { numeric: true }, true).style & ES_RIGHT, ES_RIGHT);
    assert_eq!(spec(SpecClass::Edit { numeric: false }, true).style & ES_RIGHT, 0);
    let memo = spec(SpecClass::Memo, true);
    assert_ne!(memo.style & ES_MULTILINE, 0);
    assert_ne!(memo.style & WS_VSCROLL, 0);
    let report = spec(SpecClass::Report, true);
    assert_eq!((report.class, report.style & (ES_MULTILINE | ES_READONLY | WS_VSCROLL)), ("EDIT", ES_MULTILINE | ES_READONLY | WS_VSCROLL));
    assert_eq!(spec(SpecClass::Caption, true).style & 0xF, SS_LEFT);
}

#[test]
fn only_the_default_dialog_button_is_a_default_push_button_and_a_disabled_control_is_disabled() {
    assert_eq!(spec(SpecClass::DialogButton { default: true }, true).style & 0xF, BS_DEFPUSHBUTTON);
    assert_eq!(spec(SpecClass::DialogButton { default: false }, true).style & 0xF, 0);
    assert_eq!(spec(SpecClass::Button, true).style & 0xF, 0);
    assert_ne!(spec(SpecClass::Combo, false).style & WS_DISABLED, 0);
    assert_eq!(spec(SpecClass::Combo, true).style & WS_DISABLED, 0);
}

#[test]
fn a_form_id_is_the_child_window_id_and_decoration_gets_unique_ids_above_them() {
    use burnburn_host_models::form::FormId;
    assert_eq!(control_id(Some(FormId(7)), 0), 2000 + 7);
    // a control without a form id (a caption) still gets a unique id above every form id
    assert!(control_id(None, 3) > control_id(Some(FormId(60_000)), 0));
    assert_ne!(control_id(None, 3), control_id(None, 4));
}
