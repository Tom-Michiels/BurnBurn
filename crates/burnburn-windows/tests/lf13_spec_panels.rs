//! LF13: the Windows panels that the Mac builds from a `FormSpec` (CAM, Cut Settings, files, golf 5,
//! Schikken, preferences, machine hub) are built from the same form on Windows. Per panel: the
//! description follows the form rules, the layout at 96, 144 and 192 dpi has no overlap and fits,
//! the buttons run primary first with the default button, and every control that takes input has a
//! route to what the Windows host acts on (parity with the old per-panel line lists).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_host_models::form::FormId;
use burnburn_host_models::l10n::Locale;
use burnburn_host_models::panels::{CutTop, G5Panel, cut_settings_panel, g5_panel, schikken_panel};
use burnburn_presentation::{CamDialog, CommandKey, HeadlessEditorHarness, PrefKey, SelectMode, UiIntent};
use burnburn_types::DocumentId;
use burnburn_windows::forms::spec_layout::{DluBase, SpecClass, SpecLayout, TextMeter};
use burnburn_windows::forms::spec_panels::{self, FilesSheet, SpecPanel, SpecRoute};
use burnburn_windows::forms::{ControlId, FormContext, PanelState, cam, cut_settings, g5, schikken};

struct Fixed(u32);

impl TextMeter for Fixed {
    fn width(&self, text: &str, bold: bool) -> i32 {
        let w = 6.0 * f64::from(self.0) / 96.0 * if bold { 1.1 } else { 1.0 };
        (text.chars().count() as f64 * w).round() as i32
    }
}

fn window() -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (0.0, 0.0), (40.0, 20.0));
    h.invoke(doc, CommandKey::ToolSelect);
    let id = h.document(doc).unwrap().objects[0].id;
    h.send(UiIntent::Select { document: doc, ids: vec![id], mode: SelectMode::Replace });
    h.send(UiIntent::AssignLayer { document: doc, slot: 1, add: false });
    (h, doc)
}

fn interactive(class: &SpecClass) -> bool {
    matches!(
        class,
        SpecClass::Edit { .. }
            | SpecClass::Combo
            | SpecClass::Check
            | SpecClass::Memo
            | SpecClass::Button
            | SpecClass::Tool
            | SpecClass::DialogButton { .. }
    )
}

/// The rules every panel follows, at every DPI. Returns the layout at 96 dpi.
fn check(name: &str, panel: &SpecPanel) -> SpecLayout {
    assert_eq!(panel.spec.problems(), Vec::new(), "{name}: form rules");
    assert!(panel.spec.sections.iter().any(|s| !s.rows.is_empty()), "{name}: the panel has rows");
    let mut first = None;
    for dpi in [96, 144, 192] {
        let l = panel.layout(DluBase::segoe_ui_9(dpi), &Fixed(dpi));
        let shown: Vec<_> = l.controls.iter().filter(|c| c.visible && c.class != SpecClass::Tabs).collect();
        for c in &shown {
            assert!(c.rect.x >= 0 && c.rect.y >= 0, "{name} {dpi}: {c:?}");
            assert!(
                c.rect.right() <= l.client.0 && c.rect.bottom() <= l.client.1,
                "{name} {dpi}: {c:?} in {:?}",
                l.client
            );
        }
        for (i, a) in shown.iter().enumerate() {
            for b in &shown[i + 1..] {
                let same_page = a.tab.is_none() || b.tab.is_none() || a.tab == b.tab;
                assert!(!(same_page && a.rect.overlaps(&b.rect)), "{name} {dpi}: {a:?} overlaps {b:?}");
            }
        }
        let buttons: Vec<_> = l.controls.iter().filter(|c| matches!(c.class, SpecClass::DialogButton { .. })).collect();
        if let (Some(p), Some(c)) = (&panel.spec.buttons.primary, &panel.spec.buttons.cancel) {
            let x = |id: FormId| buttons.iter().find(|b| b.id == Some(id)).map(|b| b.rect.x);
            assert!(x(p.id) < x(c.id), "{name} {dpi}: primary before cancel");
        }
        assert_eq!(
            l.default_button,
            panel.spec.buttons.primary.as_ref().filter(|p| p.enabled).map(|p| p.id),
            "{name} {dpi}: the default button"
        );
        for c in l.controls.iter().filter(|c| interactive(&c.class)) {
            let id = c.id.unwrap_or_else(|| panic!("{name}: {c:?} has no id"));
            assert!(panel.route(id).is_some(), "{name}: control {id:?} ({}) has no route", c.text);
        }
        first.get_or_insert(l);
    }
    first.unwrap()
}

fn routed(panel: &SpecPanel) -> Vec<ControlId> {
    panel
        .routes
        .iter()
        .filter_map(|(_, r)| if let SpecRoute::Control(c) = r { Some(c.clone()) } else { None })
        .collect()
}

#[test]
fn the_cam_dialogs_are_spec_panels_and_route_every_control_the_old_form_had() {
    let (mut h, doc) = window();
    for dialog in [CamDialog::Optimization, CamDialog::Device, CamDialog::Array] {
        h.send(UiIntent::OpenCamDialog { document: doc, dialog });
        let model = h.controller().cam_dialog(doc, dialog).expect("dialog");
        let panel = spec_panels::cam(&model, Locale::Nl);
        let l = check(&format!("{dialog:?}"), &panel);
        assert!(l.controls.iter().any(|c| matches!(c.class, SpecClass::DialogButton { .. })));
        let vm = h.view();
        let old = cam::form(&FormContext { vm: &vm, document: doc, locale: Locale::Nl }, dialog, &model).unwrap();
        let routes = routed(&panel);
        for id in old.ids() {
            assert!(routes.contains(&id), "{dialog:?}: the old form's {id:?} has no route");
        }
    }
}

#[test]
fn the_cut_settings_sheet_is_a_spec_panel_with_ok_first_and_every_old_control_routed() {
    let (mut h, doc) = window();
    let op = h.document(doc).unwrap().operations[0].id;
    h.send(UiIntent::OpenCutSettings { document: doc, operation: op });
    let sheet = h.document(doc).unwrap().cut_settings.expect("sheet");
    let vm = h.view();
    let panel_model = cut_settings_panel(&vm, doc, Locale::Nl).expect("panel");
    let panel = spec_panels::cut_settings(Some(&sheet), &CutTop::opening(&panel_model), sheet.mode, Locale::Nl);
    let l = check("cut settings", &panel);
    let buttons: Vec<&str> = l
        .controls
        .iter()
        .filter(|c| matches!(c.class, SpecClass::DialogButton { .. }))
        .map(|c| c.text.as_str())
        .collect();
    assert_eq!(buttons, vec!["OK", "Annuleer"]);
    let old = cut_settings::form(&FormContext { vm: &vm, document: doc, locale: Locale::Nl }, &[]).unwrap();
    let routes = routed(&panel);
    for id in old.ids() {
        assert!(routes.contains(&id), "the old sheet's {id:?} has no route");
    }
    assert!(routes.contains(&ControlId::CutOk) && routes.contains(&ControlId::CutCancel));
    // the unit of a number is the suffix after its field
    assert!(l.controls.iter().any(|c| c.class == SpecClass::Suffix && c.text == "mm/s"));
}

#[test]
fn the_golf_5_panels_and_schikken_are_spec_panels_with_their_old_controls_routed() {
    let (h, doc) = window();
    let vm = h.view();
    let ctx = FormContext { vm: &vm, document: doc, locale: Locale::Nl };
    for panel in [G5Panel::Offset, G5Panel::Structure] {
        let model = g5_panel(&vm, doc, panel, Locale::Nl, &[]).expect("panel");
        let spec = spec_panels::g5(&model);
        check(&format!("{panel:?}"), &spec);
        let state = PanelState { g5_shown: vec![panel], ..PanelState::default() };
        let old = g5::forms(&ctx, &state, &[]).into_iter().find(|f| f.title == model.title).expect("old form");
        let routes = routed(&spec);
        for id in old.ids() {
            assert!(routes.contains(&id), "{panel:?}: the old form's {id:?} has no route");
        }
    }
    let model = schikken_panel(&vm, doc, Locale::Nl).expect("schikken");
    let spec = spec_panels::schikken(&model);
    check("schikken", &spec);
    let state = PanelState { schikken_shown: true, ..PanelState::default() };
    let old = schikken::form(&ctx, &state).expect("old form");
    let routes = routed(&spec);
    for id in old.ids() {
        assert!(routes.contains(&id), "schikken: the old form's {id:?} has no route");
    }
}

#[test]
fn the_settings_window_has_three_tabs_a_route_per_preference_and_restore_defaults_apart() {
    let (mut h, _) = window();
    let panel = burnburn_host_models::app_view::prefs_panel(&h.controller().prefs_view(), Locale::Nl);
    let spec = spec_panels::prefs(&panel, Locale::Nl);
    let l = check("prefs", &spec);
    assert_eq!(l.tabs, vec!["Algemeen", "Tekenvlak", "Verplaatsen"]);
    for key in PrefKey::ALL {
        let routed = spec.routes.iter().any(|(_, r)| *r == SpecRoute::Pref(key));
        assert!(routed, "{key:?}");
    }
    assert!(spec.routes.iter().any(|(_, r)| *r == SpecRoute::PrefReset));
    // restore defaults is a dialog button after OK and Cancel
    let order: Vec<&str> = l
        .controls
        .iter()
        .filter(|c| matches!(c.class, SpecClass::DialogButton { .. }))
        .map(|c| c.text.as_str())
        .collect();
    assert_eq!(order.last().copied(), spec.spec.buttons.destructive.as_ref().map(|b| b.title.as_str()));
}

#[test]
fn the_machine_hub_is_a_spec_panel_with_four_tabs() {
    let (mut h, _) = window();
    let mut hub = h.controller().machine_hub_view();
    // LF17: without a machine the device tab shows an empty state instead of the pop-up.
    hub.devices.push(burnburn_presentation::DeviceRow {
        id: "ruida".into(),
        revision: 1,
        width_mm: 600.0,
        height_mm: 400.0,
        active: true,
        model: None,
        firmware: None,
    });
    let panel = burnburn_host_models::hub_panel::hub_panel(&hub, true, Locale::Nl);
    let spec = spec_panels::hub(&panel, Locale::Nl);
    let l = check("hub", &spec);
    assert_eq!(l.tabs.len(), 4, "{:?}", l.tabs);
    assert!(spec.routes.iter().any(|(_, r)| *r == SpecRoute::Hub(burnburn_host_models::hub_form::ID_DEVICE)));
}

#[test]
fn every_sheet_of_the_files_family_is_a_spec_panel() {
    for sheet in FilesSheet::ALL {
        let panel = spec_panels::files(sheet, Locale::Nl);
        check(&format!("{sheet:?}"), &panel);
        assert!(panel.routes.iter().all(|(_, r)| matches!(r, SpecRoute::Files(s, _) if *s == sheet)));
    }
}
