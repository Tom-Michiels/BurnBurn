//! The golf-5 panels as Windows forms, headless (G5-native): project notes, offset and repair,
//! centre-line decisions and the structure panel. Same lines, words and intents as the macOS panels
//! (`tests/g5_panels.rs` there): a control reports its value, presentation decides what changes.
//! The Win32 realisation (`win32/forms.rs`) is only type-checked from macOS, never run.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, dead_code)]

#[path = "../../burnburn-macos/tests/g5_fixtures/mod.rs"]
mod g5_fixtures;

use burnburn_presentation::{
    CenterlineEdit, CommandKey, HeadlessEditorHarness, OffsetJoin, OffsetOptions, StructureEdit, UiIntent,
};
use burnburn_types::DocumentId;
use burnburn_windows::adapter::{MenuItem, Reaction, command_id, menu_item, menu_items, menu_title};
use burnburn_windows::forms::g5;
use burnburn_windows::forms::layout::{Item, layout};
use burnburn_windows::forms::{
    ControlId, ControlValue, Form, FormContext, FormKind, FormLine, FormOutcome, PanelState, Placement,
};
use burnburn_windows::l10n::Locale;
use burnburn_windows::panels::{G5Control as C, G5Panel as P};
use g5_fixtures::*;

type Values = Vec<(ControlId, ControlValue)>;

fn asked(panels: &[P]) -> PanelState {
    PanelState { g5_shown: panels.to_vec(), ..PanelState::default() }
}

fn forms(h: &HeadlessEditorHarness, doc: DocumentId, state: &PanelState, values: &Values) -> Vec<Form> {
    let vm = h.view();
    g5::forms(&FormContext { vm: &vm, document: doc, locale: Locale::En }, state, values)
}

fn form(h: &HeadlessEditorHarness, doc: DocumentId, p: P) -> Form {
    forms(h, doc, &asked(&[P::Offset, P::Structure]), &Vec::new())
        .into_iter()
        .find(|f| f.kind == FormKind::G5(p))
        .unwrap_or_else(|| panic!("no form {p:?}"))
}

fn line(h: &HeadlessEditorHarness, doc: DocumentId, p: P, c: C) -> FormLine {
    form(h, doc, p).find(&ControlId::G5(c)).cloned().unwrap_or_else(|| panic!("no control {c:?}"))
}

fn flat(f: &Form) -> Vec<&FormLine> {
    f.lines
        .iter()
        .flat_map(|l| match l {
            FormLine::Row(inner) => inner.iter().collect::<Vec<_>>(),
            other => vec![other],
        })
        .collect()
}

fn texts(h: &HeadlessEditorHarness, doc: DocumentId, p: P) -> Vec<String> {
    flat(&form(h, doc, p))
        .into_iter()
        .filter_map(|l| match l {
            FormLine::Text(t) | FormLine::Problem(t) | FormLine::Heading(t) => Some(t.clone()),
            _ => None,
        })
        .collect()
}

/// Uses a control like the host does: reads every control, reports one, sends what comes back.
fn press(
    h: &mut HeadlessEditorHarness,
    doc: DocumentId,
    p: P,
    c: C,
    _value: ControlValue,
    overrides: &[(C, ControlValue)],
) -> Option<UiIntent> {
    let mut values = form(h, doc, p).values();
    for (c, v) in overrides {
        match values.iter_mut().find(|(id, _)| *id == ControlId::G5(*c)) {
            Some(slot) => slot.1 = v.clone(),
            None => values.push((ControlId::G5(*c), v.clone())),
        }
    }
    let vm = h.view();
    let outcome =
        g5::react(p, &ControlId::G5(c), &values, &FormContext { vm: &vm, document: doc, locale: Locale::En });
    match outcome {
        FormOutcome::Intent(i) => {
            h.send(i.clone());
            Some(i)
        }
        FormOutcome::Nothing => None,
        other => panic!("{other:?}"),
    }
}

fn text(s: &str) -> ControlValue {
    ControlValue::Text(s.to_owned())
}

fn choice(l: &FormLine) -> (Vec<String>, usize) {
    match l {
        FormLine::Choice { items, selected, .. } => (items.iter().map(|i| i.title.clone()).collect(), *selected),
        other => panic!("not a choice: {other:?}"),
    }
}

fn button_enabled(l: &FormLine, c: C) -> bool {
    match l {
        FormLine::Buttons(b) => b.iter().find(|b| b.id == ControlId::G5(c)).unwrap().enabled,
        other => panic!("not buttons: {other:?}"),
    }
}

// ---------------------------------------------------------------------------------------------
// Which forms exist
// ---------------------------------------------------------------------------------------------

#[test]
fn notes_and_centreline_open_by_themselves_and_offset_and_structure_wait_for_the_menu() {
    let (mut h, doc, _) = two_rects();
    let none = forms(&h, doc, &PanelState::default(), &Vec::new());
    assert!(none.is_empty(), "nothing is asked for yet: {:?}", none.iter().map(|f| f.kind).collect::<Vec<_>>());
    h.invoke(doc, CommandKey::ShowNotes);
    let now = forms(&h, doc, &PanelState::default(), &Vec::new());
    assert_eq!(now.iter().map(|f| (f.kind, f.placement)).collect::<Vec<_>>(), [(FormKind::G5(P::Notes), Placement::Floating)]);
    let both = forms(&h, doc, &asked(&[P::Offset, P::Structure]), &Vec::new());
    let kinds: Vec<FormKind> = both.iter().map(|f| f.kind).collect();
    assert_eq!(kinds, [FormKind::G5(P::Notes), FormKind::G5(P::Offset), FormKind::G5(P::Structure)]);
    assert!(both.iter().all(|f| f.placement == Placement::Floating));
    assert_eq!(both[1].title, "Offset and Repair");
    let nl = g5::forms(
        &FormContext { vm: &h.view(), document: doc, locale: Locale::Nl },
        &asked(&[P::Structure]),
        &Vec::new(),
    );
    assert_eq!(nl.iter().find(|f| f.kind == FormKind::G5(P::Structure)).unwrap().title, "Structuur");
}

// ---------------------------------------------------------------------------------------------
// Notes
// ---------------------------------------------------------------------------------------------

#[test]
fn the_notes_form_saves_one_undo_step_and_refuses_a_note_beyond_the_limit() {
    let (mut h, doc, _) = two_rects();
    h.invoke(doc, CommandKey::ShowNotes);
    let FormLine::Memo { text: stored, max_chars, .. } = line(&h, doc, P::Notes, C::NotesText) else { panic!("memo") };
    assert_eq!((stored.as_str(), max_chars), ("", 20_000));

    let sent = press(&mut h, doc, P::Notes, C::NotesSave, ControlValue::Press, &[(C::NotesText, text("Klant Jansen"))]);
    assert_eq!(sent, Some(UiIntent::SetNotes { document: doc, text: "Klant Jansen".into() }));
    let FormLine::Memo { text: stored, .. } = line(&h, doc, P::Notes, C::NotesText) else { panic!() };
    assert_eq!(stored, "Klant Jansen");
    h.invoke(doc, CommandKey::Undo);
    let FormLine::Memo { text: stored, .. } = line(&h, doc, P::Notes, C::NotesText) else { panic!() };
    assert_eq!(stored, "", "undo takes the note back");

    // What is typed is counted: beyond the limit Save is off and the form says why.
    let typed: Values = vec![(ControlId::G5(C::NotesText), text(&"a".repeat(20_001)))];
    let f = forms(&h, doc, &PanelState::default(), &typed).remove(0);
    let save = flat(&f).into_iter().find(|l| l.ids().contains(&ControlId::G5(C::NotesSave))).unwrap();
    assert!(!button_enabled(save, C::NotesSave));
    assert!(flat(&f).iter().any(|l| matches!(l, FormLine::Problem(t) if t.contains("20000"))));
    let vm = h.view();
    let ctx = FormContext { vm: &vm, document: doc, locale: Locale::En };
    assert_eq!(g5::react(P::Notes, &ControlId::G5(C::NotesSave), &typed, &ctx), FormOutcome::Nothing);

    let closed = press(&mut h, doc, P::Notes, C::NotesClose, ControlValue::Press, &[]);
    assert_eq!(closed, Some(UiIntent::CloseNotes { document: doc }));
    assert!(forms(&h, doc, &PanelState::default(), &Vec::new()).is_empty());
}

#[test]
fn a_memo_is_laid_out_as_one_multi_line_edit_inside_the_width() {
    let (mut h, doc, _) = two_rects();
    h.invoke(doc, CommandKey::ShowNotes);
    let f = form(&h, doc, P::Notes);
    assert_eq!(f.ids(), [ControlId::G5(C::NotesText), ControlId::G5(C::NotesSave), ControlId::G5(C::NotesClose)]);
    for width in [260, 380, 520] {
        let l = layout(&f, width, 1.0);
        let memo = l.items.iter().find(|p| matches!(p.item, Item::Memo { .. })).expect("a memo item");
        assert!(memo.rect.h >= 100, "tall enough to write in: {:?}", memo.rect);
        assert!(memo.rect.x >= 0 && memo.rect.right() <= width);
        for (i, a) in l.items.iter().enumerate() {
            for b in l.items.iter().skip(i + 1) {
                assert!(!a.rect.overlaps(&b.rect), "{:?} / {:?}", a.rect, b.rect);
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Offset and repair
// ---------------------------------------------------------------------------------------------

#[test]
fn the_offset_section_previews_applies_and_sends_the_options() {
    let (mut h, doc, [a, _]) = two_rects();
    select(&mut h, doc, &[]);
    assert_eq!(choice(&line(&h, doc, P::Offset, C::OffsetJoin)), (vec!["Round".into(), "Mitred".into(), "Bevelled".into()], 0));
    assert!(!button_enabled(&line(&h, doc, P::Offset, C::OffsetPreview), C::OffsetPreview), "needs a selection");
    select(&mut h, doc, &[a]);

    let sent = press(&mut h, doc, P::Offset, C::OffsetPreview, ControlValue::Press, &[(C::OffsetDistance, text("2 mm"))]);
    assert_eq!(sent, Some(UiIntent::PreviewOffset { document: doc, distance: "2 mm".into() }));
    h.run_tasks();
    assert!(texts(&h, doc, P::Offset).iter().any(|t| t.contains("24 × 14 mm")), "{:?}", texts(&h, doc, P::Offset));

    let sent = press(
        &mut h,
        doc,
        P::Offset,
        C::OffsetJoin,
        ControlValue::Choice(1),
        &[(C::OffsetJoin, ControlValue::Choice(1)), (C::OffsetKeepSources, ControlValue::Toggle(false))],
    );
    assert_eq!(
        sent,
        Some(UiIntent::SetOffsetOptions {
            document: doc,
            options: OffsetOptions { join: OffsetJoin::Miter, keep_sources: false }
        })
    );
    h.run_tasks();
    assert!(texts(&h, doc, P::Offset).iter().any(|t| t.contains("336")), "recomputed mitred");

    let sent = press(&mut h, doc, P::Offset, C::OffsetApply, ControlValue::Press, &[(C::OffsetDistance, text("2 mm"))]);
    assert_eq!(sent, Some(UiIntent::Offset { document: doc, distance: "2 mm".into() }));
    assert_eq!(vm(&h, doc).objects.len(), 2, "the source was replaced by its offset");
}

#[test]
fn the_repair_section_says_what_closes_and_what_stays_before_it_repairs() {
    let (mut h, doc, ids) = opened_with(vec![open_path(0.0, 0.05), open_path(20.0, 0.15)]);
    let issues = vm(&h, doc).issues;
    let seam = issues.iter().position(|i| i.objects == vec![ids[0]]).unwrap();
    let slot = issues.iter().position(|i| i.objects == vec![ids[1]]).unwrap();
    let (options, _) = choice(&line(&h, doc, P::Offset, C::RepairIssue));
    assert_eq!(options.len(), 2);
    assert!(options[seam].contains("open contour") && options[seam].contains("0.05"), "{options:?}");
    let revision = vm(&h, doc).revision;

    let set = |i: usize| [(C::RepairIssue, ControlValue::Choice(i)), (C::RepairTolerance, text("0.1 mm"))];
    let sent = press(&mut h, doc, P::Offset, C::RepairPreview, ControlValue::Press, &set(seam));
    assert_eq!(sent, Some(UiIntent::PreviewRepair { document: doc, issue: seam, tolerance: "0.1 mm".into() }));
    h.run_tasks();
    assert!(texts(&h, doc, P::Offset).iter().any(|t| t.contains("1 gaps close, 0 stay open")));
    press(&mut h, doc, P::Offset, C::RepairPreview, ControlValue::Press, &set(slot));
    h.run_tasks();
    assert!(texts(&h, doc, P::Offset).iter().any(|t| t.contains("0 gaps close, 1 stay open")));
    assert_eq!(vm(&h, doc).revision, revision, "nothing changed yet");

    let sent = press(&mut h, doc, P::Offset, C::RepairApply, ControlValue::Press, &set(seam));
    assert_eq!(sent, Some(UiIntent::RepairIssue { document: doc, issue: seam, tolerance: "0.1 mm".into() }));
    assert!(vm(&h, doc).revision != revision);
}

// ---------------------------------------------------------------------------------------------
// Centre-line decisions
// ---------------------------------------------------------------------------------------------

#[test]
fn the_centreline_form_opens_with_the_trace_and_a_pairing_decides_the_junction() {
    let (h0, doc0, _) = two_rects();
    assert!(forms(&h0, doc0, &PanelState::default(), &Vec::new()).is_empty());

    let (mut h, doc) = centre_lines_of(&t_png());
    let f = forms(&h, doc, &PanelState::default(), &Vec::new());
    assert!(f.iter().any(|f| f.kind == FormKind::G5(P::Centerline)), "opens by itself");
    let node = vm(&h, doc).centerline.unwrap().junctions[0].node;
    let pairing = C::Junction { node, candidate: 1 };
    assert!(matches!(line(&h, doc, P::Centerline, pairing), FormLine::Buttons(_)));
    let sent = press(&mut h, doc, P::Centerline, C::Junction { node, candidate: 0 }, ControlValue::Press, &[]);
    assert_eq!(
        sent,
        Some(UiIntent::CenterlineEdit { document: doc, edit: CenterlineEdit::ResolveJunction { node, candidate: 0 } })
    );
    assert!(texts(&h, doc, P::Centerline).iter().any(|t| t.contains("decided")));
}

#[test]
fn a_ring_offers_a_circle_that_only_the_operator_confirms() {
    let (mut h, doc) = centre_lines_of(&ring_png());
    let chain = vm(&h, doc).centerline.unwrap().chains[0].chain;
    assert!(texts(&h, doc, P::Centerline).iter().any(|t| t.contains("circle") && t.contains("within tolerance")));
    let sent = press(&mut h, doc, P::Centerline, C::Primitive { chain }, ControlValue::Press, &[]);
    assert_eq!(sent, Some(UiIntent::CenterlineEdit { document: doc, edit: CenterlineEdit::ConfirmPrimitive { chain } }));
    assert!(texts(&h, doc, P::Centerline).iter().any(|t| t.contains("confirmed as circle")));
}

// ---------------------------------------------------------------------------------------------
// Structure
// ---------------------------------------------------------------------------------------------

#[test]
fn the_structure_form_lists_plates_instances_and_the_broken_reference() {
    let (h, doc, [_, kapot, _]) = structure_project();
    let shown = texts(&h, doc, P::Structure);
    assert!(shown.iter().any(|t| t == "Plaat A: 600 × 400 mm, Berken, 3 mm thick, 2 parts, 0 objects"), "{shown:?}");
    assert!(shown.iter().any(|t| t.contains("Rand: broken") && t.contains("no longer exists")), "{shown:?}");
    let failed: Vec<String> = flat(&form(&h, doc, P::Structure))
        .into_iter()
        .filter_map(|l| match l {
            FormLine::Problem(t) => Some(t.clone()),
            _ => None,
        })
        .collect();
    assert!(failed.iter().any(|t| t.contains(&format!("Instance {kapot}")) && t.contains("failed at parameter breedte")));
}

#[test]
fn a_variant_an_override_and_the_plate_go_through_presentation_and_unchanged_values_send_nothing() {
    let (mut h, doc, [_, _, plain]) = structure_project();
    let variant = C::Variant { instance: plain };
    let (options, selected) = choice(&line(&h, doc, P::Structure, variant));
    assert_eq!(selected, 0);
    let klein = options.iter().position(|o| o == "klein").unwrap();
    assert_eq!(press(&mut h, doc, P::Structure, variant, ControlValue::Choice(0), &[(variant, ControlValue::Choice(0))]), None);
    let sent = press(&mut h, doc, P::Structure, variant, ControlValue::Choice(klein), &[(variant, ControlValue::Choice(klein))]);
    assert_eq!(
        sent,
        Some(UiIntent::Structure {
            document: doc,
            edit: StructureEdit::SetVariant { instance: plain, variant: Some("klein".into()) }
        })
    );
    assert_eq!(choice(&line(&h, doc, P::Structure, variant)).1, klein);

    let over = C::Override { instance: plain, parameter: 0 };
    assert_eq!(press(&mut h, doc, P::Structure, over, text(""), &[(over, text(""))]), None, "unchanged");
    let sent = press(&mut h, doc, P::Structure, over, text("80 mm"), &[(over, text("80 mm"))]);
    assert_eq!(
        sent,
        Some(UiIntent::Structure {
            document: doc,
            edit: StructureEdit::SetOverride { instance: plain, parameter: "breedte".into(), text: "80 mm".into() }
        })
    );
    assert!(matches!(line(&h, doc, P::Structure, over), FormLine::Field { text, .. } if text == "80"));
    press(&mut h, doc, P::Structure, over, text("-3"), &[(over, text("-3"))]);
    assert!(matches!(line(&h, doc, P::Structure, over), FormLine::Field { text, .. } if text == "80"), "refused");

    let plate = vm(&h, doc).structure.plates[0].id;
    let sent = press(&mut h, doc, P::Structure, C::Plate, ControlValue::Choice(1), &[(C::Plate, ControlValue::Choice(1))]);
    assert_eq!(sent, Some(UiIntent::ChoosePlate { document: doc, plate: Some(plate) }));
    assert_eq!(choice(&line(&h, doc, P::Structure, C::Plate)).1, 1);
}

#[test]
fn a_structure_form_with_many_instances_lays_out_without_overlap() {
    let (h, doc, _) = structure_project();
    let f = form(&h, doc, P::Structure);
    let want = f.ids();
    for width in [300, 420] {
        let l = layout(&f, width, 1.0);
        let got: Vec<ControlId> = l
            .items
            .iter()
            .filter_map(|p| match &p.item {
                Item::Field { id, .. } | Item::Check { id, .. } | Item::Choice { id, .. } | Item::Memo { id, .. } => {
                    Some(id.clone())
                }
                Item::Button(b) => Some(b.id.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(got.len(), want.len());
        for (i, a) in l.items.iter().enumerate() {
            assert!(a.rect.x >= 0 && a.rect.right() <= width && a.rect.bottom() <= l.height);
            for b in l.items.iter().skip(i + 1) {
                assert!(!a.rect.overlaps(&b.rect), "{:?} / {:?}", a.rect, b.rect);
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Menu
// ---------------------------------------------------------------------------------------------

#[test]
fn the_panel_items_share_the_macos_tags_and_toggle_their_panels() {
    use burnburn_windows::adapter::{Control, MessageContext, msg, notify, on_message};
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    let vm = h.view();
    assert_eq!(command_id(MenuItem::OffsetPanel), 1030);
    assert_eq!(command_id(MenuItem::StructurePanel), 1031);
    for item in [MenuItem::OffsetPanel, MenuItem::StructurePanel] {
        assert!(menu_items().contains(&item));
        assert_eq!(menu_item(command_id(item)), Some(item));
    }
    assert_eq!(menu_title(MenuItem::OffsetPanel, &vm, Locale::En), "Offset and Repair…");
    assert_eq!(menu_title(MenuItem::StructurePanel, &vm, Locale::Nl), "Structuur…");
    let ctx = MessageContext {
        document: doc,
        control: Control::Frame,
        dpi: 96,
        text: "",
        selected_rows: &[],
        alt: false,
        cursor_px: [0, 0],
        time_ms: 0,
    };
    let wm = |id: u16| (id as usize) | ((notify::MENU as usize) << 16);
    assert_eq!(
        on_message(&ctx, msg::WM_COMMAND, wm(command_id(MenuItem::OffsetPanel)), 0, &vm),
        Reaction::ToggleG5Panel(P::Offset)
    );
    assert_eq!(
        on_message(&ctx, msg::WM_COMMAND, wm(command_id(MenuItem::StructurePanel)), 0, &vm),
        Reaction::ToggleG5Panel(P::Structure)
    );
}
