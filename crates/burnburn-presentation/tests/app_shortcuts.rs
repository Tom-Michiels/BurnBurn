//! X02 (W3-UI-app): the shortcut editor's model over the command register. Menu commands, catalogue
//! tools and host actions share one namespace of keys: a key has one meaning, a conflict is
//! reported (never silently resolved), and the user's choices live in the user file.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

use std::path::{Path, PathBuf};
use std::sync::Arc;

use burnburn_io::fs::FileSystem;
use burnburn_io::memory::MemoryFileSystem;
use burnburn_presentation::{
    AppIntent, CommandKey, HeadlessEditorHarness, KeyBinding, ShortcutProblem, ShortcutTarget, UiIntent,
};
use burnburn_types::DocumentId;

const USER_DIR: &str = "/Users/ik/Library/Application Support/BurnBurn";

fn user_file() -> PathBuf {
    Path::new(USER_DIR).join("preferences.conf")
}

fn started(fs: Arc<MemoryFileSystem>) -> HeadlessEditorHarness {
    let mut h = HeadlessEditorHarness::with_fs(fs);
    h.controller().set_user_dir(PathBuf::from(USER_DIR));
    h
}

fn key(text: &str) -> KeyBinding {
    KeyBinding::parse(text).unwrap_or_else(|| panic!("{text:?} should parse"))
}

fn assign(h: &mut HeadlessEditorHarness, id: &str, binding: &str, replace: bool) {
    h.send(UiIntent::App(AppIntent::SetShortcut { id: id.into(), binding: key(binding), replace }));
    h.run_tasks();
}

fn binding_of(h: &mut HeadlessEditorHarness, id: &str) -> Option<String> {
    h.controller().shortcuts_view().row(id).expect("row").binding.as_ref().map(KeyBinding::display)
}

fn text_of_user_file(fs: &MemoryFileSystem) -> String {
    String::from_utf8(fs.read(&user_file(), u64::MAX).unwrap()).unwrap()
}

#[test]
fn binding_text_round_trips_and_accepts_any_order_case_and_the_alt_alias() {
    assert_eq!(key("shift+cmd+z"), key("Cmd+Shift+Z"));
    assert_eq!(key("Alt+P"), key("Opt+P"));
    assert_eq!(key("Cmd+Shift+Z").display(), "Cmd+Shift+Z");
    assert_eq!(key("opt+cmd+i").display(), "Cmd+Opt+I");
    assert_eq!(key("Cmd++").key, "+");
    assert_eq!(key("Ctrl+Left").key, "left".replace("left", "Left"));
    for text in ["Cmd+Shift+Z", "Opt+P", "Shift+F", "V", "Cmd+]", "Cmd+0", "Cmd+Tab", "."] {
        assert_eq!(key(text).display(), key(&key(text).display()).display(), "{text}");
    }
    for bad in ["", "Cmd+", "Cmd+Shift", "Foo+Z", "Cmd+Z+X", "  "] {
        assert_eq!(KeyBinding::parse(bad), None, "{bad:?}");
    }
}

#[test]
fn the_defaults_match_what_the_menu_and_the_tooltips_have_shown_so_far_and_collide_nowhere() {
    let mut h = HeadlessEditorHarness::new();
    let view = h.controller().shortcuts_view();
    let default =
        |id: &str| view.row(id).unwrap_or_else(|| panic!("no row {id}")).default.as_ref().map(|b| b.display());
    assert_eq!(default("cmd.Undo").as_deref(), Some("Cmd+Z"));
    assert_eq!(default("cmd.Redo").as_deref(), Some("Cmd+Shift+Z"));
    assert_eq!(default("cmd.Group").as_deref(), Some("Cmd+G"));
    assert_eq!(default("cmd.Compile").as_deref(), Some("Cmd+B"));
    assert_eq!(default("tool.select").as_deref(), Some("V"));
    assert_eq!(default("tool.sketch.chamfer.equal_distance").as_deref(), Some("Shift+F"));
    assert_eq!(default("host.preferences").as_deref(), Some("Cmd+,"));
    let mut seen = std::collections::BTreeMap::new();
    for row in &view.rows {
        if let Some(b) = &row.default {
            assert_eq!(
                seen.insert(b.clone(), row.id.clone()),
                None,
                "{} and {} share {}",
                row.id,
                seen[b],
                b.display()
            );
        }
        assert_eq!(row.binding, row.default, "{}: nothing is customized in a fresh profile", row.id);
        assert!(!row.customized);
    }
    assert!(view.rows.iter().any(|r| matches!(r.target, ShortcutTarget::Host(_))));
    assert!(
        CommandKey::ALL.iter().all(|k| view.row(&format!("cmd.{k:?}")).is_some()),
        "every command is in the register"
    );
}

#[test]
fn a_free_key_can_be_assigned_and_survives_a_restart() {
    let fs = Arc::new(MemoryFileSystem::new());
    let mut h = started(Arc::clone(&fs));
    assign(&mut h, "cmd.FlipHorizontal", "Cmd+Opt+F", false);
    assert_eq!(binding_of(&mut h, "cmd.FlipHorizontal").as_deref(), Some("Cmd+Opt+F"));
    assert!(h.controller().shortcuts_view().row("cmd.FlipHorizontal").unwrap().customized);
    assert!(text_of_user_file(&fs).contains("shortcut.cmd.FlipHorizontal = Cmd+Opt+F"));
    let mut second = started(Arc::clone(&fs));
    assert_eq!(binding_of(&mut second, "cmd.FlipHorizontal").as_deref(), Some("Cmd+Opt+F"));
}

#[test]
fn a_taken_key_is_a_conflict_that_names_the_owner_and_changes_nothing_until_replace_is_chosen() {
    let fs = Arc::new(MemoryFileSystem::new());
    let mut h = started(Arc::clone(&fs));
    assign(&mut h, "cmd.Save", "Cmd+Z", false);
    let error = h.controller().shortcuts_view().error.expect("conflict reported");
    assert_eq!(error.id, "cmd.Save");
    assert_eq!(error.problem, ShortcutProblem::Conflict { with: "cmd.Undo".into() });
    assert_eq!(binding_of(&mut h, "cmd.Save").as_deref(), Some("Cmd+S"));
    assert_eq!(binding_of(&mut h, "cmd.Undo").as_deref(), Some("Cmd+Z"));
    assert!(fs.files().is_empty(), "a refused change writes nothing");

    assign(&mut h, "cmd.Save", "Cmd+Z", true);
    assert_eq!(h.controller().shortcuts_view().error, None);
    assert_eq!(binding_of(&mut h, "cmd.Save").as_deref(), Some("Cmd+Z"));
    assert_eq!(binding_of(&mut h, "cmd.Undo"), None, "the previous owner is left without a shortcut");
    assert!(h.controller().shortcuts_view().row("cmd.Undo").unwrap().customized);
    let text = text_of_user_file(&fs);
    assert!(text.contains("shortcut.cmd.Undo = none"), "{text}");
    // Pressing the key now means Save, and only Save.
    let doc = h.new_document().expect("window");
    assert_eq!(
        h.controller().shortcut_intent(doc, &key("Cmd+Z")),
        Some(UiIntent::Invoke { document: Some(doc), command: CommandKey::Save })
    );
}

#[test]
fn assigning_the_key_an_entry_already_has_is_not_a_conflict() {
    let mut h = started(Arc::new(MemoryFileSystem::new()));
    assign(&mut h, "cmd.Undo", "Cmd+Z", false);
    assert_eq!(h.controller().shortcuts_view().error, None);
}

#[test]
fn keys_the_system_or_the_canvas_own_are_refused_and_commands_need_a_modifier() {
    let mut h = started(Arc::new(MemoryFileSystem::new()));
    for (id, text, why) in [
        ("cmd.Save", "Cmd+Q", ShortcutProblem::Reserved),
        ("cmd.Save", "Cmd+H", ShortcutProblem::Reserved),
        ("cmd.Save", "Cmd+Opt+H", ShortcutProblem::Reserved),
        ("cmd.Save", "Cmd+Tab", ShortcutProblem::Reserved),
        ("tool.select", "Escape", ShortcutProblem::Reserved),
        ("tool.select", "Return", ShortcutProblem::Reserved),
        ("tool.select", "Delete", ShortcutProblem::Reserved),
        ("cmd.FlipHorizontal", "H", ShortcutProblem::NeedsModifier),
        ("cmd.FlipHorizontal", "Shift+H", ShortcutProblem::NeedsModifier),
        ("cmd.Nonexistent", "Cmd+K", ShortcutProblem::UnknownId),
    ] {
        assign(&mut h, id, text, false);
        assert_eq!(h.controller().shortcuts_view().error.map(|e| e.problem), Some(why), "{id} = {text}");
    }
    // A tool may use a plain key.
    assign(&mut h, "tool.select", "H", false);
    assert_eq!(h.controller().shortcuts_view().error, None);
    assert_eq!(binding_of(&mut h, "tool.select").as_deref(), Some("H"));
}

#[test]
fn clearing_resetting_one_and_resetting_all_return_to_the_defaults_without_leaving_lines_behind() {
    let fs = Arc::new(MemoryFileSystem::new());
    let mut h = started(Arc::clone(&fs));
    assign(&mut h, "cmd.FlipHorizontal", "Cmd+Opt+F", false);
    assign(&mut h, "tool.select", "H", false);
    h.send(UiIntent::App(AppIntent::ClearShortcut { id: "cmd.Undo".into() }));
    h.run_tasks();
    assert_eq!(binding_of(&mut h, "cmd.Undo"), None);
    h.send(UiIntent::App(AppIntent::ResetShortcut { id: "cmd.Undo".into() }));
    h.run_tasks();
    assert_eq!(binding_of(&mut h, "cmd.Undo").as_deref(), Some("Cmd+Z"));
    assert!(!h.controller().shortcuts_view().row("cmd.Undo").unwrap().customized);
    assert!(!text_of_user_file(&fs).contains("cmd.Undo"));
    h.send(UiIntent::App(AppIntent::ResetAllShortcuts));
    h.run_tasks();
    assert!(h.controller().shortcuts_view().rows.iter().all(|r| !r.customized));
    assert!(!text_of_user_file(&fs).contains("shortcut."), "{}", text_of_user_file(&fs));
}

#[test]
fn an_override_for_an_id_this_version_does_not_know_is_ignored_but_kept_and_a_bad_binding_is_reported() {
    let fs = Arc::new(MemoryFileSystem::new());
    fs.insert(
        &user_file(),
        b"burnburn-preferences 1\nshortcut.cmd.FutureCommand = Cmd+9\nshortcut.cmd.Save = Cmd+Banana\nshortcut.tool.select = H\n"
            .to_vec(),
    );
    let mut h = started(Arc::clone(&fs));
    assert_eq!(
        binding_of(&mut h, "cmd.Save").as_deref(),
        Some("Cmd+S"),
        "an unreadable binding falls back to the default"
    );
    assert_eq!(binding_of(&mut h, "tool.select").as_deref(), Some("H"));
    assert!(h.controller().prefs_view().load_problem, "the unreadable line is reported");
    assign(&mut h, "cmd.FlipHorizontal", "Cmd+Opt+F", false);
    assert!(text_of_user_file(&fs).contains("shortcut.cmd.FutureCommand = Cmd+9"), "{}", text_of_user_file(&fs));
}

fn in_window() -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().expect("window");
    (h, doc)
}

#[test]
fn a_tool_key_selects_the_tool_through_the_same_path_as_its_button() {
    let (mut h, doc) = in_window();
    assert_eq!(
        h.controller().shortcut_intent(doc, &key("R")),
        Some(UiIntent::SelectTool { document: Some(doc), id: "sketch.rectangle.two_point".into() })
    );
    let intent = h.controller().shortcut_intent(doc, &key("R")).unwrap();
    h.send(intent);
    assert_eq!(h.document(doc).unwrap().tool, burnburn_editor::ToolKind::Rectangle);
}

#[test]
fn plain_keys_are_text_while_the_text_tool_is_active_and_unavailable_tools_do_nothing() {
    let (mut h, doc) = in_window();
    h.invoke(doc, CommandKey::ToolSelect);
    let text = h.controller().shortcut_intent(doc, &key("X")).expect("the text tool has X");
    h.send(text);
    assert_eq!(h.document(doc).unwrap().tool, burnburn_editor::ToolKind::Text);
    assert_eq!(h.controller().shortcut_intent(doc, &key("R")), None, "typing text is not a tool switch");
    assert!(h.controller().shortcut_intent(doc, &key("Cmd+Z")).is_some(), "Cmd combinations still work");
    assert_eq!(h.controller().shortcut_intent(doc, &key(".")), None, "the point tool does not exist yet");
}

#[test]
fn a_command_key_without_a_menu_item_still_works_on_the_canvas() {
    let (mut h, doc) = in_window();
    assign(&mut h, "cmd.Measure", "Cmd+Opt+M", false);
    assert_eq!(
        h.controller().shortcut_intent(doc, &key("Cmd+Opt+M")),
        Some(UiIntent::Invoke { document: Some(doc), command: CommandKey::Measure })
    );
    assert_eq!(h.controller().shortcut_intent(doc, &key("Cmd+Opt+J")), None);
}

#[test]
fn the_operator_cannot_switch_tools_with_a_key() {
    let (mut h, doc) = in_window();
    h.controller().set_role(burnburn_presentation::Role::Operator);
    assert_eq!(h.controller().shortcut_intent(doc, &key("R")), None);
}

#[test]
fn the_shortcut_revision_moves_on_every_real_change_and_only_then() {
    let mut h = started(Arc::new(MemoryFileSystem::new()));
    let r0 = h.controller().shortcuts_revision();
    assign(&mut h, "cmd.FlipHorizontal", "Cmd+Opt+F", false);
    let r1 = h.controller().shortcuts_revision();
    assert_ne!(r0, r1);
    assign(&mut h, "cmd.Save", "Cmd+Z", false); // a conflict: nothing changes
    assert_eq!(h.controller().shortcuts_revision(), r1);
    assign(&mut h, "cmd.FlipHorizontal", "Cmd+Opt+F", false); // the same again
    assert_eq!(h.controller().shortcuts_revision(), r1, "assigning what it already is changes nothing");
    h.send(UiIntent::App(AppIntent::ResetAllShortcuts));
    assert_ne!(h.controller().shortcuts_revision(), r1);
}
