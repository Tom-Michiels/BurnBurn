//! The golf-6 editor panels, headless (G6-editor): the generator panel (box, living hinge; E10) and
//! the project search panel (P08). What each shows comes from presentation's view model in the
//! window language; a control becomes at most one intent with its text unparsed (DR-05). The real
//! widgets are driven by `tests/native/macos/g6_panels.rs`; the Windows crate runs the same models
//! in `tests/g6_forms.rs`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod g6_fixtures;

use burnburn_macos::l10n::{Locale, message_or_key, undo_label_text};
use burnburn_macos::panels::{G5Control as C, G5Line, G5Panel, G5PanelModel, g5_intent, g5_opens_by_itself, g5_panel};
use burnburn_presentation::{
    CommandKey, FilesIntent, GeneratorEdit, GeneratorField, GeneratorKind, HeadlessEditorHarness, MessageKey, SelectMode,
    UiIntent,
};
use burnburn_types::DocumentId;
use g6_fixtures::*;

fn open() -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::GeneratorNew);
    (h, doc)
}

fn panel(h: &HeadlessEditorHarness, doc: DocumentId, locale: Locale) -> Option<G5PanelModel> {
    g5_panel(&h.view(), doc, G5Panel::Generator, locale, &[])
}

fn lines(h: &HeadlessEditorHarness, doc: DocumentId, locale: Locale) -> Vec<G5Line> {
    panel(h, doc, locale).expect("the panel is there").lines
}

fn line(h: &HeadlessEditorHarness, doc: DocumentId, c: C) -> G5Line {
    lines(h, doc, Locale::En).into_iter().find(|l| l.control() == Some(c)).unwrap_or_else(|| panic!("no control {c:?}"))
}

fn problems(h: &HeadlessEditorHarness, doc: DocumentId, locale: Locale) -> Vec<String> {
    lines(h, doc, locale)
        .into_iter()
        .filter_map(|l| match l {
            G5Line::Problem(t) => Some(t),
            _ => None,
        })
        .collect()
}

fn press(h: &mut HeadlessEditorHarness, doc: DocumentId, c: C, values: &[(C, &str)]) -> Option<UiIntent> {
    let values: Vec<(C, String)> = values.iter().map(|(c, v)| (*c, (*v).to_owned())).collect();
    let intent = g5_intent(&h.view(), doc, G5Panel::Generator, c, &values)?;
    h.send(intent.clone());
    Some(intent)
}

#[test]
fn the_menu_commands_and_the_generator_history_entries_have_dutch_and_english_titles() {
    assert_eq!(message_or_key(MessageKey::Command(CommandKey::GeneratorNew), Locale::Nl), "Doos of scharnier…");
    assert_eq!(message_or_key(MessageKey::Command(CommandKey::GeneratorNew), Locale::En), "Box or Hinge…");
    assert_eq!(message_or_key(MessageKey::Command(CommandKey::GeneratorEdit), Locale::Nl), "Bewerk generator…");
    assert_eq!(message_or_key(MessageKey::Command(CommandKey::GeneratorEdit), Locale::En), "Edit Generator…");
    use burnburn_document::UndoLabel;
    use burnburn_document::generator::GeneratorLabel;
    assert_eq!(undo_label_text(UndoLabel::Generator(GeneratorLabel::Create), Locale::Nl), Some("Generator maken"));
    assert_eq!(undo_label_text(UndoLabel::Generator(GeneratorLabel::Edit), Locale::En), Some("Edit Generator"));
}

#[test]
fn the_panel_exists_only_while_the_dialog_is_open_and_opens_by_itself() {
    assert!(g5_opens_by_itself(G5Panel::Generator));
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    assert!(panel(&h, doc, Locale::En).is_none(), "no dialog, no panel");
    h.invoke(doc, CommandKey::GeneratorNew);
    assert!(panel(&h, doc, Locale::En).is_some());
    h.send(UiIntent::Generator { document: doc, edit: GeneratorEdit::Cancel });
    assert!(panel(&h, doc, Locale::En).is_none());
}

#[test]
fn the_box_panel_shows_kind_fields_lid_the_kerf_note_and_the_preview_in_the_window_language() {
    let (h, doc) = open();
    let nl = panel(&h, doc, Locale::Nl).unwrap();
    assert_eq!(nl.title, "Doos of scharnier");
    let en = panel(&h, doc, Locale::En).unwrap();
    assert_eq!(en.title, "Box or Hinge");
    match line(&h, doc, C::GeneratorKind) {
        G5Line::Choice { options, selected, .. } => {
            assert_eq!(options, ["Finger-jointed box", "Living hinge"]);
            assert_eq!(selected, 0);
        }
        other => panic!("{other:?}"),
    }
    for (field, text) in [
        (GeneratorField::Width, "100"),
        (GeneratorField::Depth, "60"),
        (GeneratorField::Height, "40"),
        (GeneratorField::Thickness, "3"),
        (GeneratorField::FingerWidth, "10"),
        (GeneratorField::PanelSpacing, "5"),
    ] {
        match line(&h, doc, C::GeneratorField(field)) {
            G5Line::Field { text: t, caption, .. } => assert_eq!((t.as_str(), caption.is_empty()), (text, false), "{field:?}"),
            other => panic!("{other:?}"),
        }
    }
    assert!(matches!(line(&h, doc, C::GeneratorLid), G5Line::Check { on: false, .. }));
    let all: Vec<String> = lines(&h, doc, Locale::En)
        .into_iter()
        .filter_map(|l| match l {
            G5Line::Text(t) => Some(t),
            _ => None,
        })
        .collect();
    assert!(all.iter().any(|t| t.contains("kerf") && t.contains("once")), "the kerf note: {all:?}");
    assert!(all.iter().any(|t| t.contains("6 panels")), "the preview line: {all:?}");
    let nl_text: Vec<String> = lines(&h, doc, Locale::Nl)
        .into_iter()
        .filter_map(|l| match l {
            G5Line::Text(t) => Some(t),
            _ => None,
        })
        .collect();
    assert!(nl_text.iter().any(|t| t.contains("kerf") && t.contains("één keer")), "{nl_text:?}");
    assert!(matches!(line(&h, doc, C::GeneratorInsert), G5Line::Button { enabled: true, .. }));
}

#[test]
fn controls_become_generator_edits_and_an_unchanged_value_sends_nothing() {
    let (mut h, doc) = open();
    // Field text goes unparsed; the shown value sends nothing.
    assert_eq!(press(&mut h, doc, C::GeneratorField(GeneratorField::Width), &[(C::GeneratorField(GeneratorField::Width), "100")]), None);
    let intent = press(
        &mut h,
        doc,
        C::GeneratorField(GeneratorField::Width),
        &[(C::GeneratorField(GeneratorField::Width), "80,5 mm")],
    );
    assert_eq!(
        intent,
        Some(UiIntent::Generator { document: doc, edit: GeneratorEdit::Field(GeneratorField::Width, "80,5 mm".into()) })
    );
    // Four panels of 80.5, two of 60 and five gaps of 5 mm in the flat row: 4 * 80.5 + 120 + 25 = 467.
    assert_eq!(h.document(doc).unwrap().generator_dialog.unwrap().preview.unwrap().width, "467");
    // The lid check and the kind choice.
    assert_eq!(
        press(&mut h, doc, C::GeneratorLid, &[(C::GeneratorLid, "1")]),
        Some(UiIntent::Generator { document: doc, edit: GeneratorEdit::LidOpen(true) })
    );
    assert_eq!(press(&mut h, doc, C::GeneratorLid, &[(C::GeneratorLid, "1")]), None, "already open");
    assert_eq!(
        press(&mut h, doc, C::GeneratorKind, &[(C::GeneratorKind, "1")]),
        Some(UiIntent::Generator { document: doc, edit: GeneratorEdit::Kind(GeneratorKind::LivingHinge) })
    );
    assert!(!lines(&h, doc, Locale::En).iter().any(|l| l.control() == Some(C::GeneratorLid)), "a hinge has no lid");
    assert!(lines(&h, doc, Locale::En).iter().any(|l| l.control() == Some(C::GeneratorField(GeneratorField::CutLength))));
}

#[test]
fn a_refused_field_is_a_problem_line_in_words_and_insert_is_greyed() {
    let (mut h, doc) = open();
    press(&mut h, doc, C::GeneratorField(GeneratorField::FingerWidth), &[(C::GeneratorField(GeneratorField::FingerWidth), "2")]);
    let en = problems(&h, doc, Locale::En);
    assert_eq!(en.len(), 1, "{en:?}");
    assert!(en[0].to_lowercase().contains("finger") && en[0].contains('3') && en[0].contains('2'), "the limit is named: {en:?}");
    let nl = problems(&h, doc, Locale::Nl);
    assert!(nl[0].to_lowercase().contains("vinger"), "{nl:?}");
    assert!(matches!(line(&h, doc, C::GeneratorInsert), G5Line::Button { enabled: false, .. }));
    assert_eq!(press(&mut h, doc, C::GeneratorInsert, &[]), None, "a greyed button sends nothing");
    press(&mut h, doc, C::GeneratorField(GeneratorField::Width), &[(C::GeneratorField(GeneratorField::Width), "abc")]);
    assert!(problems(&h, doc, Locale::En).iter().any(|p| p.to_lowercase().contains("number")), "{:?}", problems(&h, doc, Locale::En));
}

#[test]
fn insert_makes_the_box_and_a_generated_object_opens_with_an_apply_button() {
    let (mut h, doc) = open();
    press(&mut h, doc, C::GeneratorInsert, &[]);
    assert!(panel(&h, doc, Locale::En).is_none(), "done: the panel closes");
    assert_eq!(h.document(doc).unwrap().objects.len(), 6);
    h.invoke(doc, CommandKey::GeneratorEdit);
    match line(&h, doc, C::GeneratorInsert) {
        G5Line::Button { title, .. } => assert_eq!(title, "Apply"),
        other => panic!("{other:?}"),
    }
    assert_eq!(press(&mut h, doc, C::GeneratorCancel, &[]), Some(UiIntent::Generator { document: doc, edit: GeneratorEdit::Cancel }));
    h.send(UiIntent::Select { document: doc, ids: vec![], mode: SelectMode::Replace });
    assert!(panel(&h, doc, Locale::En).is_none());
}

// ---------------------------------------------------------------------------------------------
// Project search (P08)
// ---------------------------------------------------------------------------------------------

fn search_panel(h: &HeadlessEditorHarness, doc: DocumentId, locale: Locale, draft: &[(C, &str)]) -> Option<G5PanelModel> {
    let draft: Vec<(C, String)> = draft.iter().map(|(c, v)| (*c, (*v).to_owned())).collect();
    g5_panel(&h.view(), doc, G5Panel::Search, locale, &draft)
}

fn search_lines(h: &HeadlessEditorHarness, doc: DocumentId, draft: &[(C, &str)]) -> Vec<G5Line> {
    search_panel(h, doc, Locale::En, draft).expect("the search panel").lines
}

fn search_press(
    h: &mut HeadlessEditorHarness,
    doc: DocumentId,
    c: C,
    values: &[(C, &str)],
) -> Option<UiIntent> {
    let values: Vec<(C, String)> = values.iter().map(|(c, v)| (*c, (*v).to_owned())).collect();
    let intent = g5_intent(&h.view(), doc, G5Panel::Search, c, &values)?;
    h.send(intent.clone());
    h.run_tasks();
    Some(intent)
}

#[test]
fn the_search_panel_opens_by_itself_with_the_search_and_titles_in_both_languages() {
    assert!(g5_opens_by_itself(G5Panel::Search));
    let mut h = HeadlessEditorHarness::new();
    let (deurbord, _) = werk(&h);
    let doc = open_window(&mut h, &deurbord);
    assert!(search_panel(&h, doc, Locale::En, &[]).is_none(), "no search, no panel");
    h.invoke(doc, CommandKey::SearchProjects);
    h.run_tasks();
    assert_eq!(search_panel(&h, doc, Locale::Nl, &[]).unwrap().title, "Projecten zoeken");
    assert_eq!(search_panel(&h, doc, Locale::En, &[]).unwrap().title, "Find Projects");
    assert_eq!(message_or_key(MessageKey::Command(CommandKey::SearchProjects), Locale::Nl), "Zoek projecten…");
    assert_eq!(message_or_key(MessageKey::Command(CommandKey::SearchProjects), Locale::En), "Find Projects…");
}

#[test]
fn the_panel_lists_the_projects_with_their_date_a_preview_and_what_could_not_be_read() {
    let (h, doc) = searching();
    let lines = search_lines(&h, doc, &[]);
    let field = |c: C| lines.iter().find_map(|l| match l {
        G5Line::Field { control, text, .. } if *control == c => Some(text.clone()),
        _ => None,
    });
    assert_eq!(field(C::SearchFolder).as_deref(), Some("/werk"));
    assert_eq!(field(C::SearchQuery).as_deref(), Some(""));
    let texts: Vec<&String> = lines.iter().filter_map(|l| if let G5Line::Text(t) = l { Some(t) } else { None }).collect();
    assert!(texts.iter().any(|t| t.contains("2 projects")), "{texts:?}");
    let problems: Vec<&String> = lines.iter().filter_map(|l| if let G5Line::Problem(t) = l { Some(t) } else { None }).collect();
    assert!(problems.iter().any(|t| t.contains("kapot.burnburn")), "a broken file is named: {problems:?}");
    match lines.iter().find(|l| l.control() == Some(C::SearchResult)).expect("the result list") {
        G5Line::Choice { options, selected, .. } => {
            assert_eq!(*selected, 0);
            assert_eq!(options[0], "deurbord.burnburn - 2026-06-18 14:30 UTC");
            assert_eq!(options[1], "onderzetters.burnburn", "an unknown time is not invented");
        }
        other => panic!("{other:?}"),
    }
    let image = lines.iter().find_map(|l| if let G5Line::Image(i) = l { Some(i) } else { None }).expect("a preview");
    assert_eq!((image.width, image.height), (64, 64));
    assert!(image.rgba.chunks(4).any(|p| p[0] < 128), "the outline is drawn dark on paper");
    assert!(texts.iter().any(|t| t.contains("2 objects") && t.contains("2026-06-18 14:30 UTC")), "{texts:?}");
}

#[test]
fn the_chosen_result_decides_the_preview_the_revisions_and_what_matched() {
    let (mut h, doc) = searching();
    let one = [(C::SearchResult, "1")];
    let lines = search_lines(&h, doc, &one);
    assert!(
        lines.iter().all(|l| l.control() != Some(C::SearchRevision)),
        "onderzetters has no frozen revision, so there is nothing to choose"
    );
    assert!(matches!(
        lines.iter().find(|l| l.control() == Some(C::SearchOpenRevision)).unwrap(),
        G5Line::Button { enabled: false, .. }
    ));
    let lines = search_lines(&h, doc, &[(C::SearchResult, "0")]);
    match lines.iter().find(|l| l.control() == Some(C::SearchRevision)).expect("deurbord has a frozen revision") {
        G5Line::Choice { options, .. } => assert_eq!(options, &["1: Order 4711".to_owned()]),
        other => panic!("{other:?}"),
    }
    // A query says what matched, in the window language.
    search_press(&mut h, doc, C::SearchQuery, &[(C::SearchQuery, "multiplex")]);
    let texts = |locale| -> Vec<String> {
        search_panel(&h, doc, locale, &[]).unwrap().lines.into_iter().filter_map(|l| if let G5Line::Text(t) = l { Some(t) } else { None }).collect()
    };
    assert!(texts(Locale::En).iter().any(|t| t.contains("Found in") && t.contains("material")), "{:?}", texts(Locale::En));
    assert!(texts(Locale::Nl).iter().any(|t| t.contains("Gevonden in") && t.contains("materiaal")), "{:?}", texts(Locale::Nl));
}

#[test]
fn search_controls_become_files_intents_and_an_unchanged_value_sends_nothing() {
    let (mut h, doc) = searching();
    assert_eq!(search_press(&mut h, doc, C::SearchFolder, &[(C::SearchFolder, "/werk")]), None);
    assert_eq!(
        search_press(&mut h, doc, C::SearchQuery, &[(C::SearchQuery, "4711")]),
        Some(UiIntent::Files(FilesIntent::SetSearchQuery { document: doc, query: "4711".into() }))
    );
    assert_eq!(
        search_press(&mut h, doc, C::SearchFolder, &[(C::SearchFolder, "/bestaat/niet")]),
        Some(UiIntent::Files(FilesIntent::SetSearchFolder { document: doc, folder: "/bestaat/niet".into() }))
    );
    assert!(
        search_lines(&h, doc, &[]).iter().any(|l| matches!(l, G5Line::Problem(t) if t.contains("cannot be read"))),
        "an unreadable folder is said"
    );
    search_press(&mut h, doc, C::SearchFolder, &[(C::SearchFolder, "/werk")]);
    assert_eq!(
        search_press(&mut h, doc, C::SearchRescan, &[]),
        Some(UiIntent::Files(FilesIntent::RescanSearch { document: doc }))
    );
    // The query is empty again after the folder change? No: it stays; clear it for the next steps.
    search_press(&mut h, doc, C::SearchQuery, &[(C::SearchQuery, "")]);
    assert_eq!(
        search_press(&mut h, doc, C::SearchOpenProject, &[(C::SearchResult, "1")]),
        Some(UiIntent::Files(FilesIntent::OpenFoundProject {
            document: doc,
            path: "/werk/onderzetters.burnburn".into()
        }))
    );
    assert_eq!(h.view().documents.len(), 2, "the found project is open next to the design");
    assert_eq!(
        search_press(&mut h, doc, C::SearchOpenRevision, &[(C::SearchResult, "0"), (C::SearchRevision, "0")]),
        Some(UiIntent::Files(FilesIntent::OpenFoundRevision {
            document: doc,
            path: "/werk/deurbord.burnburn".into(),
            number: 1
        }))
    );
    assert_eq!(h.view().documents.len(), 3, "and so is the old revision");
    assert_eq!(search_press(&mut h, doc, C::SearchOpenRevision, &[(C::SearchResult, "1")]), None, "none to open");
    assert_eq!(
        search_press(&mut h, doc, C::SearchClose, &[]),
        Some(UiIntent::Files(FilesIntent::CloseSearch { document: doc }))
    );
    assert!(search_panel(&h, doc, Locale::En, &[]).is_none());
}
