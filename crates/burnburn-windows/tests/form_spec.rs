//! The shared `FormSpec` (LF5) compiles in the Windows host and orders the dialog buttons the
//! Windows way: OK (the default button, Return), then Cancel (Escape). The Windows layout that
//! draws it is LF13; this checks only what the host can already rely on.

use burnburn_host_models::form::{ButtonRole, Key, Platform};
use burnburn_host_models::panels::{G5Control, G5Line, g5_form};

#[test]
fn the_notes_form_orders_save_before_close_on_windows_and_close_before_save_on_the_mac() {
    let lines = vec![
        G5Line::Memo { control: G5Control::NotesText, text: String::new(), max_chars: 20_000 },
        G5Line::Button { control: G5Control::NotesSave, title: "Bewaar notities".into(), enabled: true },
        G5Line::Button { control: G5Control::NotesClose, title: "Sluit".into(), enabled: true },
    ];
    let form = g5_form("Notities", &lines);
    let roles = |p| form.spec.buttons.order(p).into_iter().map(|(r, _)| r).collect::<Vec<_>>();
    assert_eq!(roles(Platform::Windows), vec![ButtonRole::Primary, ButtonRole::Cancel]);
    assert_eq!(roles(Platform::Mac), vec![ButtonRole::Cancel, ButtonRole::Primary]);
    assert_eq!(ButtonRole::Primary.key(), Some(Key::Return));
    assert_eq!(ButtonRole::Cancel.key(), Some(Key::Escape));
    assert_eq!(form.spec.problems(), Vec::new());
}
