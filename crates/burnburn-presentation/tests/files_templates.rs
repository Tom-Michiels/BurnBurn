//! W3-UI-bestanden, templates (M024/M018): Save As Template, new from a template and the recent
//! templates list, through the real controller and the in-memory file system.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

use std::path::{Path as FsPath, PathBuf};

use burnburn_io::fs::FileSystem;
use burnburn_presentation::{
    AppUpdate, CommandKey, FilesIntent, FilesMessage, HeadlessEditorHarness, HostEffect, MessageKey, StatusMessage,
    TemplateScope, UiIntent,
};
use burnburn_types::DocumentId;

fn template_path(name: &str) -> PathBuf {
    PathBuf::from(format!("/templates/{name}.burnburntemplate"))
}

/// A window with a 40 × 20 rectangle on layer 1 (one Line operation).
fn drawn(h: &mut HeadlessEditorHarness) -> DocumentId {
    let doc = h.new_document().expect("window");
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (0.0, 0.0), (40.0, 20.0));
    h.invoke(doc, CommandKey::ToolSelect);
    let id = h.document(doc).unwrap().objects[0].id;
    h.send(UiIntent::Select { document: doc, ids: vec![id], mode: burnburn_presentation::SelectMode::Replace });
    h.send(UiIntent::AssignLayer { document: doc, slot: 1, add: false });
    doc
}

fn save_template(
    h: &mut HeadlessEditorHarness,
    doc: DocumentId,
    path: PathBuf,
    scope: TemplateScope,
) -> Vec<AppUpdate> {
    h.fs().create_dir_all_for_test(FsPath::new("/templates"));
    let mut updates = vec![h.send(UiIntent::Files(FilesIntent::SaveAsTemplate { document: doc, path, scope }))];
    updates.extend(h.run_tasks());
    updates
}

fn effects(updates: &[AppUpdate]) -> Vec<HostEffect> {
    updates.iter().flat_map(|u| u.effects.clone()).collect()
}

#[test]
fn the_save_as_template_command_asks_the_host_for_a_template_file() {
    let mut h = HeadlessEditorHarness::new();
    let doc = drawn(&mut h);
    let update = h.invoke(doc, CommandKey::SaveAsTemplate);
    assert!(
        update
            .effects
            .iter()
            .any(|e| matches!(e, HostEffect::ShowTemplateSavePanel { document, .. } if *document == doc)),
        "{:?}",
        update.effects
    );
    assert!(h.view().commands[&CommandKey::SaveAsTemplate].enabled);
    let update = h.invoke(doc, CommandKey::NewFromTemplate);
    assert!(update.effects.contains(&HostEffect::ShowTemplateOpenPanel));
}

/// The template file appears, the project stays untitled and dirty: a template is never the
/// document's own file.
#[test]
fn saving_a_template_writes_the_template_and_leaves_the_document_unsaved() {
    let mut h = HeadlessEditorHarness::new();
    let doc = drawn(&mut h);
    let path = template_path("plaat");
    let updates = save_template(&mut h, doc, path.clone(), TemplateScope::Everything);
    assert!(h.fs().exists(&path).unwrap(), "the template is written");
    let vm = h.document(doc).unwrap();
    assert!(vm.dirty, "the project itself is still unsaved");
    assert!(vm.title.starts_with("Naamloos"), "{}", vm.title);
    assert_eq!(h.controller().recent_templates(), [path.clone()]);
    assert!(effects(&updates).contains(&HostEffect::RecentTemplatesChanged { paths: vec![path] }));
}

#[test]
fn a_file_that_is_not_a_template_is_refused_and_nothing_is_written() {
    let mut h = HeadlessEditorHarness::new();
    let doc = drawn(&mut h);
    let path = PathBuf::from("/templates/plaat.burnburn");
    save_template(&mut h, doc, path.clone(), TemplateScope::Everything);
    assert!(!h.fs().exists(&path).unwrap());
    assert!(h.controller().recent_templates().is_empty());
    let vm = h.document(doc).unwrap();
    assert!(vm.messages.contains(&StatusMessage { key: MessageKey::Files(FilesMessage::TemplateWrongExtension) }));
}

/// New from a template: an untitled, unsaved window with the template's content; Save asks for a
/// location, and the template file is byte for byte what it was.
#[test]
fn new_from_a_template_opens_an_untitled_copy_and_never_writes_the_template() {
    let mut h = HeadlessEditorHarness::new();
    let doc = drawn(&mut h);
    let path = template_path("plaat");
    save_template(&mut h, doc, path.clone(), TemplateScope::Everything);
    let before = h.fs().read(&path, 1 << 20).unwrap();

    let mut updates = vec![h.send(UiIntent::Files(FilesIntent::NewFromTemplate { path: path.clone() }))];
    updates.extend(h.run_tasks());
    let opened = effects(&updates)
        .into_iter()
        .find_map(|e| match e {
            HostEffect::OpenWindow { document } if document != doc => Some(document),
            _ => None,
        })
        .expect("a new window");
    let vm = h.document(opened).unwrap();
    assert!(vm.title.starts_with("Naamloos"), "{}", vm.title);
    assert_eq!(vm.objects.len(), 1);
    assert_eq!(vm.operations.len(), 1);
    assert!(vm.dirty, "the copy is unsaved until it has a file of its own");

    let save = h.invoke(opened, CommandKey::Save);
    assert!(save.effects.iter().any(|e| matches!(e, HostEffect::ShowSavePanel { .. })), "Save asks for a location");
    assert_eq!(h.fs().read(&path, 1 << 20).unwrap(), before, "the template was only read");
    assert_eq!(h.controller().recent_templates(), [path], "using it keeps it in the list");
}

#[test]
fn a_settings_only_template_keeps_the_operations_and_no_objects() {
    let mut h = HeadlessEditorHarness::new();
    let doc = drawn(&mut h);
    let path = template_path("instellingen");
    save_template(&mut h, doc, path.clone(), TemplateScope::SettingsOnly);
    let mut updates = vec![h.send(UiIntent::Files(FilesIntent::NewFromTemplate { path }))];
    updates.extend(h.run_tasks());
    let opened = effects(&updates)
        .into_iter()
        .find_map(|e| match e {
            HostEffect::OpenWindow { document } if document != doc => Some(document),
            _ => None,
        })
        .expect("a new window");
    let vm = h.document(opened).unwrap();
    assert_eq!(vm.objects.len(), 0);
    assert_eq!(vm.operations.len(), 1);
    assert_eq!(vm.operations[0].targets, 0, "the layer is kept, without objects");
}

/// A template that is gone or damaged opens nothing and drops out of the list.
#[test]
fn a_missing_template_opens_nothing_and_is_forgotten() {
    let mut h = HeadlessEditorHarness::new();
    let doc = drawn(&mut h);
    let gone = template_path("weg");
    h.send(UiIntent::Files(FilesIntent::SetRecentTemplates { paths: vec![gone.clone()] }));
    assert_eq!(h.controller().recent_templates(), [gone.clone()]);
    let mut updates = vec![h.send(UiIntent::Files(FilesIntent::NewFromTemplate { path: gone }))];
    updates.extend(h.run_tasks());
    assert!(!effects(&updates).iter().any(|e| matches!(e, HostEffect::OpenWindow { .. })));
    assert!(effects(&updates).contains(&HostEffect::RecentTemplatesChanged { paths: Vec::new() }));
    assert!(h.controller().recent_templates().is_empty());
    assert!(h.document(doc).is_some(), "the first window is untouched");
}

#[test]
fn the_recent_templates_list_is_ordered_deduplicated_and_clearable() {
    let mut h = HeadlessEditorHarness::new();
    let doc = drawn(&mut h);
    let (a, b) = (template_path("a"), template_path("b"));
    save_template(&mut h, doc, a.clone(), TemplateScope::Everything);
    save_template(&mut h, doc, b.clone(), TemplateScope::Everything);
    save_template(&mut h, doc, a.clone(), TemplateScope::Everything);
    assert_eq!(h.controller().recent_templates(), [a.clone(), b.clone()], "most recent first, no duplicates");
    h.send(UiIntent::Files(FilesIntent::SetRecentTemplates { paths: vec![b.clone(), b.clone(), a.clone()] }));
    assert_eq!(h.controller().recent_templates(), [b, a]);
    let update = h.send(UiIntent::Files(FilesIntent::ClearRecentTemplates));
    assert!(update.effects.contains(&HostEffect::RecentTemplatesChanged { paths: Vec::new() }));
    assert!(h.controller().recent_templates().is_empty());
}
