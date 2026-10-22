//! G5-grbl-ui, the Mac host's headless side: Archief ▸ Exporteer G-code and its save sheet, the
//! GRBL refusals at the plan (with reason; choosing one goes to its object), and the words of the
//! Device Settings backend choice. The AppKit widgets themselves are in the native tests.

#![allow(clippy::unwrap_used, clippy::panic, clippy::expect_used)]

use std::path::PathBuf;

use burnburn_job::{MachineFrame, MachineProfile, OriginCorner, PowerRatio};
use burnburn_macos::adapter::{
    MenuItem, NativeEvent, Reaction, menu_enabled, menu_item, menu_tag, menu_title, translate,
};
use burnburn_macos::l10n::{Locale, message};
use burnburn_macos::panels::{cam_note, grbl_refusal_text, plan_panel};
use burnburn_presentation::{
    CommandKey, DialogNote, GrblProfile, GrblRefusalKind, HeadlessEditorHarness, MessageKey, SelectMode, UiIntent,
    grbl_capabilities,
};
use burnburn_types::{DocumentId, StableId};

fn grbl_rig(air: bool) -> (HeadlessEditorHarness, DocumentId, burnburn_types::GeometryId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    let grbl = GrblProfile { air_assist: air, ..GrblProfile::grbl_1_1_laser() };
    let profile = MachineProfile::new(
        "grbl-400",
        1,
        MachineFrame::new(400.0, 400.0, OriginCorner::BottomLeft).unwrap(),
        1000.0,
        400.0,
        PowerRatio::new(0.0).unwrap(),
        PowerRatio::new(1.0).unwrap(),
    )
    .unwrap()
    .with_capabilities(grbl_capabilities(&grbl))
    .unwrap();
    h.send(UiIntent::SetGrblMachineProfile { profile: Box::new(profile), grbl: Box::new(grbl) });
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (10.0, 10.0), (50.0, 30.0));
    h.invoke(doc, CommandKey::ToolSelect);
    let id = h.document(doc).unwrap().objects[0].id;
    h.send(UiIntent::Select { document: doc, ids: vec![id], mode: SelectMode::Replace });
    h.send(UiIntent::AssignLayer { document: doc, slot: 1, add: false });
    h.invoke(doc, CommandKey::Compile);
    h.run_tasks();
    (h, doc, id)
}

#[test]
fn export_gcode_is_a_menu_item_that_is_on_for_a_compiled_grbl_plan_only() {
    let item = MenuItem::Command(CommandKey::ExportGcode);
    assert!(burnburn_macos::adapter::menu_items().contains(&item));
    assert_eq!(menu_item(menu_tag(item)), Some(item));
    let (h, _doc, _) = grbl_rig(true);
    assert!(menu_enabled(item, &h.view()));
    assert!(!menu_enabled(MenuItem::Command(CommandKey::ExportRd), &h.view()), "a GRBL machine has no RD file");
    assert_eq!(menu_title(item, &h.view(), Locale::Nl), "Exporteer G-code…");
    assert_eq!(menu_title(item, &h.view(), Locale::En), "Export G-code…");
    let empty = HeadlessEditorHarness::new();
    assert!(!menu_enabled(item, &empty.view()));
}

#[test]
fn the_gcode_sheet_sends_the_export_and_a_cancel_changes_nothing() {
    let (h, doc, _) = grbl_rig(true);
    let vm = h.view();
    let path = PathBuf::from("/tmp/plaat.gcode");
    assert_eq!(
        translate(NativeEvent::GcodePanelFinished { document: doc, path: Some(path.clone()) }, &vm),
        Reaction::Intent(UiIntent::ExportGcode { document: doc, path })
    );
    assert_eq!(translate(NativeEvent::GcodePanelFinished { document: doc, path: None }, &vm), Reaction::Nothing);
    assert_eq!(
        translate(
            NativeEvent::GcodePanelFinished { document: DocumentId::from_raw(999_999), path: Some("/x".into()) },
            &vm
        ),
        Reaction::Nothing,
        "a closed window gets nothing"
    );
}

#[test]
fn the_plan_panel_lists_each_grbl_refusal_in_words_and_choosing_one_selects_its_object() {
    let (mut h, doc, object) = grbl_rig(false);
    let vm = h.view();
    let panel = plan_panel(&vm, doc, Locale::Nl);
    assert!(!panel.grbl.is_empty(), "air assist is refused on a machine without M8");
    let row = panel.grbl.iter().find(|r| r.text.contains("M8")).unwrap_or_else(|| panic!("{:?}", panel.grbl));
    assert_eq!(row.geometry, Some(object));
    assert_eq!(
        panel.findings.split(' ').next().unwrap(),
        panel.grbl.len().to_string(),
        "counted as errors: {}",
        panel.findings
    );
    // Choosing the row selects its object, like clicking a Ruida finding's source would.
    h.send(UiIntent::Select { document: doc, ids: Vec::new(), mode: SelectMode::Replace });
    let intent = panel.select_intent(doc, 0).expect("a row with an object");
    h.send(intent);
    assert_eq!(h.document(doc).unwrap().selection, vec![object]);
    assert_eq!(panel.select_intent(doc, 99), None);
}

#[test]
fn a_ruida_plan_has_no_grbl_rows() {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (10.0, 10.0), (50.0, 30.0));
    assert!(plan_panel(&h.view(), doc, Locale::Nl).grbl.is_empty());
}

#[test]
fn every_grbl_refusal_and_new_text_is_said_in_both_languages() {
    use burnburn_presentation::Feature;
    let kinds = [
        GrblRefusalKind::LaserModeRequired,
        GrblRefusalKind::AirAssistUnavailable,
        GrblRefusalKind::OutsideBed,
        GrblRefusalKind::InvalidProfile("decimals"),
        GrblRefusalKind::Feature(Feature::DotMode),
        GrblRefusalKind::Rotary,
        GrblRefusalKind::NotRepresentable("coordinate"),
    ];
    for locale in [Locale::Nl, Locale::En] {
        for kind in &kinds {
            assert!(grbl_refusal_text(kind, locale).trim().len() > 8, "{kind:?} {locale:?}");
        }
        assert!(!cam_note(DialogNote::GrblUnverified, locale).is_empty());
        for key in [MessageKey::GcodeRefused, MessageKey::RdNotForGrbl, MessageKey::GcodeNotForRuida] {
            assert!(message(key, locale).is_some_and(|t| !t.trim().is_empty()), "{key:?} {locale:?}");
        }
    }
    assert_ne!(
        grbl_refusal_text(&GrblRefusalKind::AirAssistUnavailable, Locale::Nl),
        grbl_refusal_text(&GrblRefusalKind::AirAssistUnavailable, Locale::En)
    );
}
