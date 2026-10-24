//! Which Win32 window class and style a control of the form builder becomes (LF13), as plain data.
//! `win32/spec_form.rs` feeds it to `CreateWindowExW`; the mapping is tested on every host.

use burnburn_host_models::form::FormId;

use super::spec_layout::SpecClass;

const WS_TABSTOP: u32 = 0x0001_0000;
const WS_VSCROLL: u32 = 0x0020_0000;
const WS_BORDER: u32 = 0x0080_0000;
const WS_DISABLED: u32 = 0x0800_0000;
const WS_CLIPSIBLINGS: u32 = 0x0400_0000;
const BS_PUSHBUTTON: u32 = 0x0000;
const BS_DEFPUSHBUTTON: u32 = 0x0001;
const BS_AUTOCHECKBOX: u32 = 0x0003;
const ES_RIGHT: u32 = 0x0002;
const ES_MULTILINE: u32 = 0x0004;
const ES_AUTOVSCROLL: u32 = 0x0040;
const ES_AUTOHSCROLL: u32 = 0x0080;
const ES_READONLY: u32 = 0x0800;
const ES_WANTRETURN: u32 = 0x1000;
const CBS_DROPDOWNLIST: u32 = 0x0003;
const SS_LEFT: u32 = 0x0000;
const SS_NOTIFY: u32 = 0x0100;

/// The id of the first child window of a form; control `n` is `CONTROL_BASE + n`.
const CONTROL_BASE: usize = 2000;
/// Ids of controls without a form id (captions, headings) start here, above every form id.
const DECOR_BASE: usize = 100_000;

/// The window class and style bits of one control (without `WS_CHILD` and `WS_VISIBLE`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WinSpec {
    pub class: &'static str,
    pub style: u32,
}

/// The class and style of a control; `enabled` is false for a greyed one.
pub fn win_spec(class: SpecClass, enabled: bool) -> WinSpec {
    let (name, style) = match class {
        SpecClass::Tabs => ("SysTabControl32", WS_TABSTOP),
        SpecClass::Heading
        | SpecClass::Caption
        | SpecClass::Suffix
        | SpecClass::Help
        | SpecClass::Error
        | SpecClass::Value
        | SpecClass::Note
        | SpecClass::Notice => ("STATIC", WS_CLIPSIBLINGS | SS_LEFT),
        SpecClass::Preview | SpecClass::Picture => ("STATIC", WS_CLIPSIBLINGS | SS_NOTIFY),
        SpecClass::Edit { numeric } => {
            let right = if numeric { ES_RIGHT } else { 0 };
            ("EDIT", WS_CLIPSIBLINGS | WS_TABSTOP | WS_BORDER | ES_AUTOHSCROLL | right)
        }
        SpecClass::Combo => ("COMBOBOX", WS_CLIPSIBLINGS | WS_TABSTOP | WS_VSCROLL | CBS_DROPDOWNLIST),
        SpecClass::Check => ("BUTTON", WS_CLIPSIBLINGS | WS_TABSTOP | BS_AUTOCHECKBOX),
        SpecClass::Memo => (
            "EDIT",
            WS_CLIPSIBLINGS | WS_TABSTOP | WS_BORDER | WS_VSCROLL | ES_MULTILINE | ES_AUTOVSCROLL | ES_WANTRETURN,
        ),
        SpecClass::Report => (
            "EDIT",
            WS_CLIPSIBLINGS | WS_TABSTOP | WS_BORDER | WS_VSCROLL | ES_MULTILINE | ES_AUTOVSCROLL | ES_READONLY,
        ),
        SpecClass::Button | SpecClass::Tool => ("BUTTON", WS_CLIPSIBLINGS | WS_TABSTOP | BS_PUSHBUTTON),
        SpecClass::DialogButton { default } => {
            let kind = if default { BS_DEFPUSHBUTTON } else { BS_PUSHBUTTON };
            ("BUTTON", WS_CLIPSIBLINGS | WS_TABSTOP | kind)
        }
    };
    let off = if enabled { 0 } else { WS_DISABLED };
    WinSpec { class: name, style: style | off }
}

/// The id of the child window of control `index` of the layout.
pub fn control_id(id: Option<FormId>, index: usize) -> usize {
    match id {
        Some(id) => CONTROL_BASE + id.0 as usize,
        None => DECOR_BASE + index,
    }
}
