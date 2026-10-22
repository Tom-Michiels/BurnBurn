//! The golf-5 panels, headless (G5-native): notes, offset and repair, centre-line decisions and
//! the structure panel. What each shows comes from presentation's view model in the window
//! language; a control becomes one intent with its text unparsed (DR-05). The real widgets are
//! driven by `tests/native/macos/g5_panels.rs`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod g5_fixtures;

use burnburn_macos::l10n::Locale;
use burnburn_macos::l10n_g5::{G5Text, g5_text};
use burnburn_macos::panels::{G5Control as C, G5Line, G5Panel, G5PanelModel, g5_intent, g5_opens_by_itself, g5_panel};
use burnburn_presentation::{
    CenterlineEdit, CommandKey, HeadlessEditorHarness, OffsetJoin, OffsetOptions, StructureEdit, UiIntent,
};
use burnburn_types::DocumentId;
use g5_fixtures::*;

fn panel(h: &HeadlessEditorHarness, doc: DocumentId, p: G5Panel) -> Option<G5PanelModel> {
    g5_panel(&h.view(), doc, p, Locale::En, &[])
}

fn lines(h: &HeadlessEditorHarness, doc: DocumentId, p: G5Panel) -> Vec<G5Line> {
    panel(h, doc, p).expect("the panel is there").lines
}

fn line(h: &HeadlessEditorHarness, doc: DocumentId, p: G5Panel, c: C) -> G5Line {
    lines(h, doc, p).into_iter().find(|l| l.control() == Some(c)).unwrap_or_else(|| panic!("no control {c:?}"))
}

fn has_control(h: &HeadlessEditorHarness, doc: DocumentId, p: G5Panel, c: C) -> bool {
    lines(h, doc, p).iter().any(|l| l.control() == Some(c))
}

fn texts(h: &HeadlessEditorHarness, doc: DocumentId, p: G5Panel) -> Vec<String> {
    lines(h, doc, p)
        .into_iter()
        .filter_map(|l| match l {
            G5Line::Text(t) | G5Line::Problem(t) | G5Line::Heading(t) => Some(t),
            _ => None,
        })
        .collect()
}

fn problems(h: &HeadlessEditorHarness, doc: DocumentId, p: G5Panel) -> Vec<String> {
    lines(h, doc, p)
        .into_iter()
        .filter_map(|l| match l {
            G5Line::Problem(t) => Some(t),
            _ => None,
        })
        .collect()
}

/// Uses a control like the host does: the intent goes to the controller.
fn press(h: &mut HeadlessEditorHarness, doc: DocumentId, p: G5Panel, c: C, values: &[(C, &str)]) -> Option<UiIntent> {
    let values: Vec<(C, String)> = values.iter().map(|(c, v)| (*c, (*v).to_owned())).collect();
    let intent = g5_intent(&h.view(), doc, p, c, &values)?;
    h.send(intent.clone());
    Some(intent)
}

fn choice(l: &G5Line) -> (&[String], usize) {
    match l {
        G5Line::Choice { options, selected, .. } => (options, *selected),
        other => panic!("not a choice: {other:?}"),
    }
}

fn field_text(l: &G5Line) -> &str {
    match l {
        G5Line::Field { text, .. } => text,
        other => panic!("not a field: {other:?}"),
    }
}

fn enabled(l: &G5Line) -> bool {
    match l {
        G5Line::Button { enabled, .. } => *enabled,
        other => panic!("not a button: {other:?}"),
    }
}

// ---------------------------------------------------------------------------------------------
// Notes (P04)
// ---------------------------------------------------------------------------------------------

/// Show Notes opens the panel with the stored text; Save is one undo step; undo takes the note back
/// and the panel shows it; Close ends the panel.
#[test]
fn the_notes_panel_follows_show_notes_and_saving_is_one_undo_step() {
    let (mut h, doc, _) = two_rects();
    assert!(panel(&h, doc, G5Panel::Notes).is_none(), "no panel until Show Notes");
    assert!(g5_opens_by_itself(G5Panel::Notes));
    h.invoke(doc, CommandKey::ShowNotes);
    let G5Line::Memo { text, max_chars, .. } = line(&h, doc, G5Panel::Notes, C::NotesText) else { panic!("a memo") };
    assert_eq!((text.as_str(), max_chars), ("", 20_000));

    let intent = press(&mut h, doc, G5Panel::Notes, C::NotesSave, &[(C::NotesText, "Klant Jansen")]);
    assert_eq!(intent, Some(UiIntent::SetNotes { document: doc, text: "Klant Jansen".into() }));
    let G5Line::Memo { text, .. } = line(&h, doc, G5Panel::Notes, C::NotesText) else { panic!() };
    assert_eq!(text, "Klant Jansen");

    h.invoke(doc, CommandKey::Undo);
    let G5Line::Memo { text, .. } = line(&h, doc, G5Panel::Notes, C::NotesText) else { panic!() };
    assert_eq!(text, "", "undo takes the note back");

    let closed = press(&mut h, doc, G5Panel::Notes, C::NotesClose, &[]);
    assert_eq!(closed, Some(UiIntent::CloseNotes { document: doc }));
    assert!(panel(&h, doc, G5Panel::Notes).is_none());
}

/// A note beyond the limit is not sent: the panel says so and Save is off; the stored note stays.
#[test]
fn a_note_beyond_the_limit_is_not_sent_and_the_panel_says_why() {
    let (mut h, doc, _) = two_rects();
    h.invoke(doc, CommandKey::ShowNotes);
    let long = "a".repeat(20_001);
    let draft = [(C::NotesText, long.clone())];
    let model = g5_panel(&h.view(), doc, G5Panel::Notes, Locale::En, &draft).unwrap();
    let save = model.lines.iter().find(|l| l.control() == Some(C::NotesSave)).unwrap();
    assert!(!enabled(save), "Save is off");
    assert!(model.lines.iter().any(|l| matches!(l, G5Line::Problem(t) if t.contains("20000"))), "{:?}", model.lines);
    assert_eq!(g5_intent(&h.view(), doc, G5Panel::Notes, C::NotesSave, &draft), None);
    // Inside the limit it is fine, also in the other language.
    let ok = [(C::NotesText, "kort".to_owned())];
    let nl = g5_panel(&h.view(), doc, G5Panel::Notes, Locale::Nl, &ok).unwrap();
    assert_eq!(nl.title, "Notities");
    assert!(g5_intent(&h.view(), doc, G5Panel::Notes, C::NotesSave, &ok).is_some());
}

// ---------------------------------------------------------------------------------------------
// Offset (E05)
// ---------------------------------------------------------------------------------------------

/// The offset section offers the distance, the corner shape and "keep the source" with the options
/// in force; with nothing selected neither Preview nor Apply works.
#[test]
fn the_offset_section_shows_the_options_in_force_and_needs_a_selection() {
    let (mut h, doc, [a, _]) = two_rects();
    assert!(!g5_opens_by_itself(G5Panel::Offset));
    select(&mut h, doc, &[]);
    let join = line(&h, doc, G5Panel::Offset, C::OffsetJoin);
    assert_eq!(choice(&join), (&["Round".to_owned(), "Mitred".to_owned(), "Bevelled".to_owned()][..], 0));
    assert!(matches!(line(&h, doc, G5Panel::Offset, C::OffsetKeepSources), G5Line::Check { on: true, .. }));
    assert!(!enabled(&line(&h, doc, G5Panel::Offset, C::OffsetPreview)));
    assert!(!enabled(&line(&h, doc, G5Panel::Offset, C::OffsetApply)));
    select(&mut h, doc, &[a]);
    assert!(enabled(&line(&h, doc, G5Panel::Offset, C::OffsetPreview)));
    assert!(enabled(&line(&h, doc, G5Panel::Offset, C::OffsetApply)));
}

/// Preview computes on a worker and shows the size of the result; the document is untouched. A
/// changed corner shape or keep-source choice is sent as the option set and recomputes the preview.
#[test]
fn the_offset_preview_and_the_options_go_through_presentation() {
    let (mut h, doc, [a, _]) = two_rects();
    select(&mut h, doc, &[a]);
    let (revision, undo) = (vm(&h, doc).revision, vm(&h, doc).undo);
    let intent = press(&mut h, doc, G5Panel::Offset, C::OffsetPreview, &[(C::OffsetDistance, "2 mm")]);
    assert_eq!(intent, Some(UiIntent::PreviewOffset { document: doc, distance: "2 mm".into() }));
    h.run_tasks();
    let shown = texts(&h, doc, G5Panel::Offset);
    assert!(shown.iter().any(|t| t.contains("24 × 14 mm") && t.contains("area 332.")), "{shown:?}");
    assert_eq!((vm(&h, doc).revision, vm(&h, doc).undo), (revision, undo), "the document is untouched");

    let intent =
        press(&mut h, doc, G5Panel::Offset, C::OffsetJoin, &[(C::OffsetJoin, "1"), (C::OffsetKeepSources, "0")]);
    assert_eq!(
        intent,
        Some(UiIntent::SetOffsetOptions {
            document: doc,
            options: OffsetOptions { join: OffsetJoin::Miter, keep_sources: false }
        })
    );
    h.run_tasks();
    assert!(texts(&h, doc, G5Panel::Offset).iter().any(|t| t.contains("336")), "recomputed with mitred corners");
    assert_eq!(choice(&line(&h, doc, G5Panel::Offset, C::OffsetJoin)).1, 1);
    assert!(matches!(line(&h, doc, G5Panel::Offset, C::OffsetKeepSources), G5Line::Check { on: false, .. }));
}

/// Apply sends the distance as typed with the chosen options; the source is replaced when it is
/// not kept, and the preview is gone.
#[test]
fn applying_the_offset_uses_the_typed_distance_and_the_chosen_options() {
    let (mut h, doc, [a, _]) = two_rects();
    select(&mut h, doc, &[a]);
    press(&mut h, doc, G5Panel::Offset, C::OffsetKeepSources, &[(C::OffsetJoin, "1"), (C::OffsetKeepSources, "0")]);
    press(&mut h, doc, G5Panel::Offset, C::OffsetPreview, &[(C::OffsetDistance, "2 mm")]);
    h.run_tasks();
    let intent = press(&mut h, doc, G5Panel::Offset, C::OffsetApply, &[(C::OffsetDistance, "2 mm")]);
    assert_eq!(intent, Some(UiIntent::Offset { document: doc, distance: "2 mm".into() }));
    assert_eq!(vm(&h, doc).objects.len(), 2, "the source was replaced");
    assert!(!texts(&h, doc, G5Panel::Offset).iter().any(|t| t.starts_with("Preview")), "applied: no preview");
}

/// An offset that cannot be made names the problem in the panel and shows no preview.
#[test]
fn an_impossible_offset_names_the_problem_in_the_panel() {
    let (mut h, doc, [a, _]) = two_rects();
    select(&mut h, doc, &[a]);
    assert!(problems(&h, doc, G5Panel::Offset).is_empty());
    press(&mut h, doc, G5Panel::Offset, C::OffsetPreview, &[(C::OffsetDistance, "-6 mm")]);
    h.run_tasks();
    assert_eq!(problems(&h, doc, G5Panel::Offset).len(), 1, "{:?}", texts(&h, doc, G5Panel::Offset));
    assert!(!texts(&h, doc, G5Panel::Offset).iter().any(|t| t.starts_with("Preview")));
}

// ---------------------------------------------------------------------------------------------
// Repair before and after (E09)
// ---------------------------------------------------------------------------------------------

/// The repair section lists the problems; Before and After says how many gaps close at the typed
/// tolerance and how many stay open, without changing the document; Repair then does it.
#[test]
fn the_repair_section_shows_what_closes_and_what_stays_before_it_repairs() {
    let (mut h, doc, ids) = opened_with(vec![open_path(0.0, 0.05), open_path(20.0, 0.15)]);
    let issues = vm(&h, doc).issues;
    let seam = issues.iter().position(|i| i.objects == vec![ids[0]]).unwrap();
    let slot = issues.iter().position(|i| i.objects == vec![ids[1]]).unwrap();
    let issue_line = line(&h, doc, G5Panel::Offset, C::RepairIssue);
    let (options, selected) = choice(&issue_line);
    assert_eq!(options.len(), 2);
    assert_eq!(selected, 0);
    assert!(options[seam].contains("open contour") && options[seam].contains("0.05"), "{options:?}");
    let (revision, undo) = (vm(&h, doc).revision, vm(&h, doc).undo);

    let values = |i: usize| [(C::RepairIssue, i.to_string()), (C::RepairTolerance, "0.1 mm".to_owned())];
    let call = |h: &mut HeadlessEditorHarness, c: C, i: usize| {
        let values: Vec<(C, String)> = values(i).to_vec();
        let intent = g5_intent(&h.view(), doc, G5Panel::Offset, c, &values)?;
        h.send(intent.clone());
        Some(intent)
    };
    assert_eq!(
        call(&mut h, C::RepairPreview, seam),
        Some(UiIntent::PreviewRepair { document: doc, issue: seam, tolerance: "0.1 mm".into() })
    );
    h.run_tasks();
    assert!(texts(&h, doc, G5Panel::Offset).iter().any(|t| t.contains("1 gaps close, 0 stay open")));
    assert_eq!(choice(&line(&h, doc, G5Panel::Offset, C::RepairIssue)).1, seam, "the choice follows the preview");

    call(&mut h, C::RepairPreview, slot);
    h.run_tasks();
    assert!(texts(&h, doc, G5Panel::Offset).iter().any(|t| t.contains("0 gaps close, 1 stay open")));
    assert_eq!((vm(&h, doc).revision, vm(&h, doc).undo), (revision, undo), "nothing changed yet");

    assert_eq!(
        call(&mut h, C::RepairApply, seam),
        Some(UiIntent::RepairIssue { document: doc, issue: seam, tolerance: "0.1 mm".into() })
    );
    assert!(vm(&h, doc).revision != revision, "now the document changed");
    assert!(!texts(&h, doc, G5Panel::Offset).iter().any(|t| t.contains("gaps close")), "the preview is gone");
}

/// A tolerance that is not a number starts nothing and the panel names the field error.
#[test]
fn a_bad_tolerance_shows_a_problem_and_a_clean_document_says_so() {
    let (mut h, doc, _) = opened_with(vec![open_path(0.0, 0.05)]);
    press(&mut h, doc, G5Panel::Offset, C::RepairPreview, &[(C::RepairIssue, "0"), (C::RepairTolerance, "abc")]);
    assert_eq!(problems(&h, doc, G5Panel::Offset).len(), 1, "{:?}", texts(&h, doc, G5Panel::Offset));
    let (h2, doc2, _) = two_rects();
    assert!(texts(&h2, doc2, G5Panel::Offset).iter().any(|t| t == "No problems found."));
    assert!(!has_control(&h2, doc2, G5Panel::Offset, C::RepairIssue));
}

// ---------------------------------------------------------------------------------------------
// Centre-line decisions (T04, T08)
// ---------------------------------------------------------------------------------------------

/// A drawing without a centre-line trace has no decisions panel.
#[test]
fn there_are_no_centreline_decisions_without_a_centreline_trace() {
    let (h, doc, _) = two_rects();
    assert!(panel(&h, doc, G5Panel::Centerline).is_none());
    assert!(g5_opens_by_itself(G5Panel::Centerline));
}

/// At a T three pairings are offered as buttons; picking one decides the junction, and the panel
/// then says so and counts the decision.
#[test]
fn an_ambiguous_junction_offers_its_pairings_and_picking_one_decides_it() {
    let (mut h, doc) = centre_lines_of(&t_png());
    let node = vm(&h, doc).centerline.unwrap().junctions[0].node;
    for candidate in 0..3 {
        assert!(has_control(&h, doc, G5Panel::Centerline, C::Junction { node, candidate }), "pairing {candidate}");
    }
    assert!(!has_control(&h, doc, G5Panel::Centerline, C::Junction { node, candidate: 3 }));
    let pair = line(&h, doc, G5Panel::Centerline, C::Junction { node, candidate: 1 });
    assert!(matches!(&pair, G5Line::Button { title, enabled: true, .. } if title.starts_with("Pairing 2 of junction")));

    let intent = press(&mut h, doc, G5Panel::Centerline, C::Junction { node, candidate: 0 }, &[]);
    assert_eq!(
        intent,
        Some(UiIntent::CenterlineEdit { document: doc, edit: CenterlineEdit::ResolveJunction { node, candidate: 0 } })
    );
    assert!(!has_control(&h, doc, G5Panel::Centerline, C::Junction { node, candidate: 0 }), "decided");
    let shown = texts(&h, doc, G5Panel::Centerline);
    assert!(shown.iter().any(|t| t.contains("decided")), "{shown:?}");
    assert!(shown.iter().any(|t| t == "Decisions taken: 1"), "{shown:?}");
}

/// A stored decision whose place is gone after another resolution is said in the panel as a problem
/// (G6-werk); with the same resolution no such line shows.
#[test]
fn a_stored_decision_that_no_longer_fits_is_said_in_the_panel() {
    let (mut h, doc) = centre_lines_of(&t_png());
    let node = vm(&h, doc).centerline.unwrap().junctions[0].node;
    h.send(UiIntent::CenterlineEdit { document: doc, edit: CenterlineEdit::ResolveJunction { node, candidate: 0 } });
    assert!(problems(&h, doc, G5Panel::Centerline).is_empty());
    let mut input = h.document(doc).unwrap().tracer.unwrap().form.input;
    input.dpi = "508".into();
    h.send(UiIntent::SetTraceForm { document: doc, input: Box::new(input) });
    h.run_tasks();
    let shown = problems(&h, doc, G5Panel::Centerline);
    assert_eq!(shown.len(), 1, "{shown:?}");
    assert!(shown[0].contains("1 stored decision(s) no longer fit"), "{shown:?}");
}

/// A ring offers an exact circle; nothing is replaced until the operator accepts, and then the panel
/// says which shape was confirmed.
#[test]
fn a_ring_offers_a_circle_and_confirming_it_is_the_operators_choice() {
    let (mut h, doc) = centre_lines_of(&ring_png());
    let chain = vm(&h, doc).centerline.unwrap().chains[0].chain;
    let confirm = line(&h, doc, G5Panel::Centerline, C::Primitive { chain });
    assert!(matches!(&confirm, G5Line::Button { title, enabled: true, .. } if title.contains("circle")), "{confirm:?}");
    assert!(texts(&h, doc, G5Panel::Centerline).iter().any(|t| t.contains("circle") && t.contains("within tolerance")));

    let intent = press(&mut h, doc, G5Panel::Centerline, C::Primitive { chain }, &[]);
    assert_eq!(
        intent,
        Some(UiIntent::CenterlineEdit { document: doc, edit: CenterlineEdit::ConfirmPrimitive { chain } })
    );
    assert!(!has_control(&h, doc, G5Panel::Centerline, C::Primitive { chain }), "confirmed");
    assert!(texts(&h, doc, G5Panel::Centerline).iter().any(|t| t.contains("confirmed as circle")));
}

// ---------------------------------------------------------------------------------------------
// Structure (S06, S07, P06)
// ---------------------------------------------------------------------------------------------

/// The structure panel lists the plate with its size and material, every instance with its own
/// status (the invalid variant fails alone) and the broken reference.
#[test]
fn the_structure_panel_lists_plates_instances_and_the_broken_reference() {
    let (h, doc, [_, kapot, _]) = structure_project();
    let shown = texts(&h, doc, G5Panel::Structure);
    assert!(shown.iter().any(|t| t == "Plaat A: 600 × 400 mm, Berken, 3 mm thick, 2 parts, 0 objects"), "{shown:?}");
    assert!(shown.iter().any(|t| t.contains("Beugel")), "{shown:?}");
    let failed = problems(&h, doc, G5Panel::Structure);
    assert!(
        failed.iter().any(|t| t.contains(&format!("Instance {kapot}")) && t.contains("failed at parameter breedte")),
        "{failed:?}"
    );
    assert_eq!(failed.iter().filter(|t| t.contains("failed")).count(), 1, "only the invalid variant fails");
    assert!(
        shown.iter().any(|t| t.contains("Rand: broken") && t.contains("no longer exists")),
        "a visible broken link: {shown:?}"
    );
}

/// A variant is chosen per instance: one undo step through the public route; an unchanged choice
/// sends nothing.
#[test]
fn choosing_a_variant_goes_through_presentation_and_the_panel_follows() {
    let (mut h, doc, [klein, _, plain]) = structure_project();
    let c = C::Variant { instance: plain };
    let (options, selected) = {
        let l = line(&h, doc, G5Panel::Structure, c);
        let (o, s) = choice(&l);
        (o.to_vec(), s)
    };
    assert_eq!(options.first().map(String::as_str), Some("(no variant)"));
    assert_eq!(selected, 0);
    let klein_at = options.iter().position(|o| o == "klein").expect("variant klein");
    assert_eq!(press(&mut h, doc, G5Panel::Structure, c, &[(c, "0")]), None, "unchanged");
    let intent = press(&mut h, doc, G5Panel::Structure, c, &[(c, &klein_at.to_string())]);
    assert_eq!(
        intent,
        Some(UiIntent::Structure {
            document: doc,
            edit: StructureEdit::SetVariant { instance: plain, variant: Some("klein".into()) }
        })
    );
    assert_eq!(choice(&line(&h, doc, G5Panel::Structure, c)).1, klein_at);
    // The instance that already had "klein" shows it selected.
    assert_eq!(choice(&line(&h, doc, G5Panel::Structure, C::Variant { instance: klein })).1, klein_at);
    h.invoke(doc, CommandKey::Undo);
    assert_eq!(choice(&line(&h, doc, G5Panel::Structure, c)).1, 0, "one undo step");
}

/// A local override is typed per instance and never written into the part: the field shows it,
/// an empty text removes it, a refused value (not a positive length) changes nothing.
#[test]
fn a_local_override_is_typed_per_instance_and_a_refused_value_changes_nothing() {
    let (mut h, doc, [_, _, plain]) = structure_project();
    let c = C::Override { instance: plain, parameter: 0 };
    let caption = match line(&h, doc, G5Panel::Structure, c) {
        G5Line::Field { caption, text, .. } => {
            assert_eq!(text, "", "no override yet");
            caption
        }
        other => panic!("{other:?}"),
    };
    assert!(caption.contains("breedte") && caption.contains("this instance only"), "{caption}");
    assert_eq!(press(&mut h, doc, G5Panel::Structure, c, &[(c, "")]), None, "unchanged");
    let intent = press(&mut h, doc, G5Panel::Structure, c, &[(c, "80 mm")]);
    assert_eq!(
        intent,
        Some(UiIntent::Structure {
            document: doc,
            edit: StructureEdit::SetOverride { instance: plain, parameter: "breedte".into(), text: "80 mm".into() }
        })
    );
    assert_eq!(field_text(&line(&h, doc, G5Panel::Structure, c)), "80");
    assert!(texts(&h, doc, G5Panel::Structure).iter().any(|t| t.contains("In effect") && t.contains("breedte = 80")));
    // The part itself keeps its own value.
    assert_eq!(vm(&h, doc).structure.parts[0].parameters[0].value, "40");

    press(&mut h, doc, G5Panel::Structure, c, &[(c, "-3")]);
    assert_eq!(field_text(&line(&h, doc, G5Panel::Structure, c)), "80", "a refused override is not stored");
    press(&mut h, doc, G5Panel::Structure, c, &[(c, "")]);
    assert_eq!(field_text(&line(&h, doc, G5Panel::Structure, c)), "", "empty removes it");
}

/// The plate Compile uses is chosen here; "the whole design" is the first choice.
#[test]
fn the_plate_for_compile_is_chosen_in_the_panel() {
    let (mut h, doc, _) = structure_project();
    let plate_id = vm(&h, doc).structure.plates[0].id;
    let (options, selected) = {
        let l = line(&h, doc, G5Panel::Structure, C::Plate);
        let (o, s) = choice(&l);
        (o.to_vec(), s)
    };
    assert_eq!(options, ["The whole design", "Plaat A"]);
    assert_eq!(selected, 0);
    assert_eq!(press(&mut h, doc, G5Panel::Structure, C::Plate, &[(C::Plate, "0")]), None, "unchanged");
    let intent = press(&mut h, doc, G5Panel::Structure, C::Plate, &[(C::Plate, "1")]);
    assert_eq!(intent, Some(UiIntent::ChoosePlate { document: doc, plate: Some(plate_id) }));
    assert_eq!(choice(&line(&h, doc, G5Panel::Structure, C::Plate)).1, 1);
    let intent = press(&mut h, doc, G5Panel::Structure, C::Plate, &[(C::Plate, "0")]);
    assert_eq!(intent, Some(UiIntent::ChoosePlate { document: doc, plate: None }));
}

/// A document without structure shows an honest empty panel.
#[test]
fn a_document_without_structure_says_so() {
    let (h, doc, _) = two_rects();
    let shown = texts(&h, doc, G5Panel::Structure);
    for empty in ["No plates.", "No parts.", "No references."] {
        assert!(shown.iter().any(|t| t == empty), "{empty}: {shown:?}");
    }
}

// ---------------------------------------------------------------------------------------------
// Texts
// ---------------------------------------------------------------------------------------------

/// Every text exists in both languages with the same placeholders.
#[test]
fn every_text_has_both_languages_with_the_same_placeholders() {
    fn placeholders(s: &str) -> Vec<String> {
        let mut out: Vec<String> =
            s.split('{').skip(1).filter_map(|p| p.split_once('}').map(|(n, _)| n.to_owned())).collect();
        out.sort();
        out
    }
    for t in G5Text::ALL {
        let (nl, en) = (g5_text(*t, Locale::Nl), g5_text(*t, Locale::En));
        assert!(!nl.is_empty() && !en.is_empty(), "{t:?}");
        assert_eq!(placeholders(nl), placeholders(en), "{t:?}");
    }
}

// ---------------------------------------------------------------------------------------------
// The menu and the event route (adapter)
// ---------------------------------------------------------------------------------------------

mod adapter {
    use super::*;
    use burnburn_macos::adapter::{
        MenuItem, NativeEvent, Reaction, menu_enabled, menu_item, menu_items, menu_tag, menu_title, translate,
    };

    /// The two panels that wait for a menu item have their own tags, titles and reactions; they need a
    /// window; ShowNotes is an ordinary command and lives in the File menu of both hosts.
    #[test]
    fn the_panel_menu_items_have_tags_titles_and_reactions_and_need_a_window() {
        let mut h = HeadlessEditorHarness::new();
        let empty = h.view();
        let items = [(MenuItem::OffsetPanel, G5Panel::Offset), (MenuItem::StructurePanel, G5Panel::Structure)];
        for (item, panel) in items {
            assert!(menu_items().contains(&item));
            assert_eq!(menu_item(menu_tag(item)), Some(item), "{item:?}");
            assert!(!menu_enabled(item, &empty), "{item:?} needs a window");
            assert_eq!(translate(NativeEvent::Menu(item), &empty), Reaction::ToggleG5Panel(panel));
        }
        h.new_document().unwrap();
        let vm = h.view();
        assert!(items.iter().all(|(i, _)| menu_enabled(*i, &vm)));
        assert_eq!(menu_title(MenuItem::OffsetPanel, &vm, Locale::En), "Offset and Repair…");
        assert_eq!(menu_title(MenuItem::StructurePanel, &vm, Locale::Nl), "Structuur…");
        let mut tags: Vec<isize> = menu_items().iter().map(|i| menu_tag(*i)).collect();
        let before = tags.len();
        tags.sort_unstable();
        tags.dedup();
        assert_eq!(tags.len(), before, "every menu tag is unique");
        assert!(menu_enabled(MenuItem::Command(CommandKey::ShowNotes), &vm), "Show Notes works with a window");
        assert_eq!(menu_title(MenuItem::Command(CommandKey::ShowNotes), &vm, Locale::Nl), "Notities tonen");
    }

    /// A control event becomes the intent of the headless model; an unchanged value becomes nothing.
    #[test]
    fn a_control_event_becomes_the_intent_of_the_model_and_nothing_when_unchanged() {
        let (h, doc, [_, _, plain]) = structure_project();
        let c = C::Variant { instance: plain };
        let event = |value: &str| NativeEvent::G5Control {
            document: doc,
            panel: G5Panel::Structure,
            control: c,
            values: vec![(c, value.to_owned())],
        };
        assert_eq!(translate(event("0"), &h.view()), Reaction::Nothing);
        assert!(matches!(translate(event("1"), &h.view()), Reaction::Intent(UiIntent::Structure { .. })));
    }
}
