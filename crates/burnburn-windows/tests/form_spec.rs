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

/// LF6a: the CAM dialogs and the Cut Settings sheet as forms order their buttons the Windows way:
/// the primary button (Apply, Generate, OK) first, then Close or Cancel. Written after the fact
/// (no red phase): it checks behaviour the shared form models already have.
#[test]
fn the_cam_and_cut_forms_put_the_primary_button_first_on_windows() {
    use burnburn_host_models::l10n::Locale;
    use burnburn_host_models::panels::{CutTop, cam_form, cut_form, cut_settings_panel};
    use burnburn_presentation::{CamDialog, CommandKey, HeadlessEditorHarness, SelectMode, UiIntent};

    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().expect("document");
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (0.0, 0.0), (40.0, 20.0));
    h.invoke(doc, CommandKey::ToolSelect);
    let id = h.document(doc).expect("window").objects[0].id;
    h.send(UiIntent::Select { document: doc, ids: vec![id], mode: SelectMode::Replace });
    h.send(UiIntent::AssignLayer { document: doc, slot: 1, add: false });
    let titles = |spec: &burnburn_host_models::form::FormSpec, p| {
        spec.buttons.order(p).into_iter().map(|(_, b)| b.title.clone()).collect::<Vec<_>>()
    };
    h.send(UiIntent::OpenCamDialog { document: doc, dialog: CamDialog::Optimization });
    let vm = h.controller().cam_dialog(doc, CamDialog::Optimization).expect("dialog");
    let cam = cam_form(&vm, Locale::Nl);
    assert_eq!(titles(&cam.spec, Platform::Windows), vec!["Pas toe", "Sluiten"]);
    assert_eq!(titles(&cam.spec, Platform::Mac), vec!["Sluiten", "Pas toe"]);
    assert_eq!(cam.spec.problems(), Vec::new());

    let op = h.document(doc).expect("window").operations[0].id;
    h.send(UiIntent::OpenCutSettings { document: doc, operation: op });
    let sheet = h.document(doc).expect("window").cut_settings.expect("sheet");
    let panel = cut_settings_panel(&h.view(), doc, Locale::Nl).expect("panel");
    let cut = cut_form(Some(&sheet), &CutTop::opening(&panel), &sheet.extras, sheet.mode, Locale::Nl);
    assert_eq!(titles(&cut.spec, Platform::Windows), vec!["OK", "Annuleer"]);
    assert_eq!(cut.spec.problems(), Vec::new());
}
