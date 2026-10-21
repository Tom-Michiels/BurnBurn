//! W4-L8b, headless: the lane's menu entries (ids 6000..6500, titles, enablement, reactions).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashSet;
use std::path::PathBuf;

use burnburn_presentation::{CommandKey, DiagnosticOptions, FilesIntent, HeadlessEditorHarness, UiIntent};
use burnburn_windows::app_menu::AppAction;
use burnburn_windows::host_panels::menu::{Item, MenuContext, RECENT_SLOTS, items, reaction};
use burnburn_windows::host_panels::{DialogRequest, Family, FilePurpose, HostRequest, PanelReaction, ids};
use burnburn_windows::l10n::Locale;

fn all_items() -> Vec<Item> {
    let mut v = items();
    v.extend((0..RECENT_SLOTS).map(Item::RecentProject));
    v.extend((0..RECENT_SLOTS).map(Item::RecentTemplate));
    v
}

fn ctx<'a>(
    vm: &'a burnburn_presentation::AppViewModel,
    recent: &'a [PathBuf],
    templates: &'a [PathBuf],
    locale: Locale,
) -> MenuContext<'a> {
    MenuContext { vm, locale, manual_known: true, recent_projects: recent, recent_templates: templates }
}

#[test]
fn ids_are_unique_inside_the_menu_range_belong_to_their_family_and_round_trip() {
    let mut seen = HashSet::new();
    for item in all_items() {
        let id = item.id();
        assert!((6000..6500).contains(&id), "{item:?} {id}");
        assert!(seen.insert(id), "{item:?} shares {id}");
        assert_eq!(Item::from_id(id), Some(item), "{item:?}");
    }
    assert_eq!(Item::LaserWindow.id(), ids::menu_base(Family::Laser));
    assert_eq!(ids::menu_owner(Item::Material.id()).map(|(f, _)| f), Some(Family::Files));
    assert_eq!(ids::menu_owner(Item::CameraWindow.id()).map(|(f, _)| f), Some(Family::Camera));
    assert_eq!(ids::menu_owner(Item::TraceImage.id()).map(|(f, _)| f), Some(Family::Image));
    for action in AppAction::ALL {
        assert_eq!(Item::App(action).id(), action.id());
        assert!(items().contains(&Item::App(action)));
    }
    assert_eq!(Item::from_id(101), None);
    assert_eq!(Item::from_id(6499), None);
    assert!(items().len() >= 19);
}

#[test]
fn titles_exist_in_both_languages_and_recents_are_named_by_their_file() {
    let h = HeadlessEditorHarness::new();
    let vm = h.view();
    let recent = [PathBuf::from("C:/werk/plaat.burnburn")];
    let templates = [PathBuf::from("C:/t/vorm.burnburntemplate")];
    for item in items() {
        for locale in [Locale::Nl, Locale::En] {
            assert!(!item.title(&ctx(&vm, &recent, &templates, locale)).trim().is_empty(), "{item:?} {locale:?}");
        }
    }
    let c = ctx(&vm, &recent, &templates, Locale::En);
    assert_eq!(Item::RecentProject(0).title(&c), "plaat.burnburn");
    assert_eq!(Item::RecentTemplate(0).title(&c), "vorm.burnburntemplate");
    assert_eq!(Item::LaserWindow.title(&ctx(&vm, &[], &[], Locale::Nl)), "Laservenster tonen");
    assert_eq!(Item::ExportDiagnostics.title(&ctx(&vm, &[], &[], Locale::Nl)), "Diagnose exporteren…");
}

#[test]
fn entries_that_need_a_window_or_a_stored_entry_are_off_without_it() {
    let mut h = HeadlessEditorHarness::new();
    let none: [PathBuf; 0] = [];
    let vm = h.view();
    let c = ctx(&vm, &none, &none, Locale::En);
    for item in [Item::LaserWindow, Item::ImportImage, Item::TraceImage, Item::ImagePanel, Item::FilesPanel, Item::Import, Item::CameraWindow] {
        assert!(!item.enabled(&c), "{item:?} needs a window");
    }
    assert!(Item::ExportDiagnostics.enabled(&c), "the journal replay needs no window");
    assert!(Item::App(AppAction::Preferences).enabled(&c));
    assert!(!Item::RecentProject(0).enabled(&c) && !Item::RecentTemplate(0).enabled(&c));
    h.new_document().unwrap();
    let vm = h.view();
    let recent = [PathBuf::from("C:/a.burnburn")];
    let c = ctx(&vm, &recent, &none, Locale::En);
    for item in [Item::LaserWindow, Item::ImportImage, Item::TraceImage, Item::ImagePanel, Item::FilesPanel, Item::Import, Item::CameraWindow] {
        assert!(item.enabled(&c), "{item:?} with a window");
    }
    assert!(Item::RecentProject(0).enabled(&c) && !Item::RecentProject(1).enabled(&c), "only stored entries");
}

#[test]
fn choosing_an_entry_opens_the_right_panel_dialog_or_command_for_the_active_window() {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    let vm = h.view();
    let recent = [PathBuf::from("C:/a.burnburn"), PathBuf::from("C:/b.burnburn")];
    let templates = [PathBuf::from("C:/t/x.burnburntemplate")];
    let c = ctx(&vm, &recent, &templates, Locale::En);

    assert_eq!(reaction(Item::LaserWindow, &c), PanelReaction::Host(HostRequest::ShowSheet(Family::Laser, 0)));
    assert_eq!(reaction(Item::CameraWindow, &c), PanelReaction::Host(HostRequest::ShowSheet(Family::Camera, 0)));
    assert_eq!(reaction(Item::ImagePanel, &c), PanelReaction::Host(HostRequest::ShowSheet(Family::Image, 0)));
    assert_eq!(reaction(Item::FilesPanel, &c), PanelReaction::Host(HostRequest::ShowSheet(Family::Files, 0)));
    match reaction(Item::ExportDiagnostics, &c) {
        PanelReaction::Dialog(DialogRequest::Save { purpose, .. }) => {
            assert_eq!(purpose, FilePurpose::Diagnostics { options: DiagnosticOptions::default() }, "private data only when ticked");
        }
        other => panic!("{other:?}"),
    }
    match reaction(Item::ImportImage, &c) {
        PanelReaction::Dialog(DialogRequest::Open { purpose, filters, .. }) => {
            assert_eq!(purpose, FilePurpose::ImportDrawing { document: doc });
            assert_eq!(filters[0].extensions[0], "png");
        }
        other => panic!("{other:?}"),
    }
    match reaction(Item::Import, &c) {
        PanelReaction::Dialog(DialogRequest::Open { filters, .. }) => assert_eq!(filters[0].extensions[0], "dxf"),
        other => panic!("{other:?}"),
    }
    match reaction(Item::TraceImage, &c) {
        PanelReaction::Dialog(DialogRequest::Open { purpose, .. }) => assert_eq!(purpose, FilePurpose::TraceImage { document: doc }),
        other => panic!("{other:?}"),
    }
    // Command entries are enabled by the command state of the window; an off command sends nothing.
    for (item, command) in [
        (Item::Material, CommandKey::ShowMaterialLibrary),
        (Item::Art, CommandKey::ShowArtLibrary),
        (Item::ExportSvg, CommandKey::ExportSvg),
        (Item::ExportDxf, CommandKey::ExportDxf),
        (Item::SaveTemplate, CommandKey::SaveAsTemplate),
        (Item::NewTemplate, CommandKey::NewFromTemplate),
        (Item::BundleExport, CommandKey::ExportBundle),
        (Item::BundleImport, CommandKey::ImportBundle),
    ] {
        let on = vm.commands.get(&command).is_some_and(|s| s.enabled);
        assert_eq!(item.enabled(&c), on, "{item:?}");
        let want = if on { PanelReaction::Intent(UiIntent::Invoke { document: Some(doc), command }) } else { PanelReaction::Nothing };
        assert_eq!(reaction(item, &c), want, "{item:?}");
    }
    assert_eq!(reaction(Item::RecentProject(1), &c), PanelReaction::Intent(UiIntent::Open { path: recent[1].clone() }));
    assert_eq!(reaction(Item::RecentProject(7), &c), PanelReaction::Nothing, "a stale entry");
    assert_eq!(
        reaction(Item::RecentTemplate(0), &c),
        PanelReaction::Intent(UiIntent::Files(FilesIntent::NewFromTemplate { path: templates[0].clone() }))
    );
    assert_eq!(
        reaction(Item::ClearRecentTemplates, &c),
        PanelReaction::Intent(UiIntent::Files(FilesIntent::ClearRecentTemplates))
    );
    assert_eq!(
        reaction(Item::App(AppAction::Preferences), &c),
        PanelReaction::Host(HostRequest::ShowSheet(Family::App, 0))
    );

    // Without a window the window entries do nothing.
    let empty = HeadlessEditorHarness::new().view();
    let c = ctx(&empty, &recent, &templates, Locale::En);
    assert_eq!(reaction(Item::LaserWindow, &c), PanelReaction::Nothing);
    assert_eq!(reaction(Item::Import, &c), PanelReaction::Nothing);
}

#[test]
fn every_file_purpose_is_answered_by_exactly_one_family() {
    use burnburn_presentation::TemplateScope;
    use burnburn_windows::host_panels::on_file_chosen;
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    let vm = h.view();
    let path = PathBuf::from("C:/x/file.bin");
    let options = DiagnosticOptions { include_project_paths: true, include_machine_ids: false };
    let purposes = [
        FilePurpose::OpenProject,
        FilePurpose::ImportDrawing { document: doc },
        FilePurpose::ReplaceImage { document: doc },
        FilePurpose::TraceImage { document: doc },
        FilePurpose::SaveProcessedBitmap { document: doc },
        FilePurpose::ExportRd { document: doc },
        FilePurpose::ExportDrawing { document: doc },
        FilePurpose::TemplateSave { document: doc, scope: TemplateScope::Everything },
        FilePurpose::TemplateOpen,
        FilePurpose::BundleOpen { document: doc },
        FilePurpose::BundleWrite { document: doc },
        FilePurpose::ArtAddFile { document: doc, name: String::new(), unit_item: 0 },
        FilePurpose::Diagnostics { options },
        FilePurpose::SupportBundle { options },
    ];
    for purpose in &purposes {
        let answered = on_file_chosen(&vm, purpose, Some(path.clone()));
        // The art flow needs its library open to know the group; every other purpose sends at once.
        if !matches!(purpose, FilePurpose::ArtAddFile { .. }) {
            assert!(!answered.intents().is_empty(), "{purpose:?} sends an intent: {answered:?}");
        }
        assert_eq!(on_file_chosen(&vm, purpose, None), PanelReaction::Nothing, "cancel sends nothing: {purpose:?}");
    }
}
