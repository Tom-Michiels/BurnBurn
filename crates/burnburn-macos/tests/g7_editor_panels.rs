//! G7-editor panels, headless on the Mac: the fillet and chamfer options (E05). The captions come
//! in the window language and a typed value is one `SetToolOption` with the text unparsed (DR-05).
//! The Windows host shows the same rows (`burnburn-windows/tests/g7_editor_forms.rs`).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_macos::adapter::{NativeEvent, Reaction, translate};
use burnburn_macos::l10n::{Locale, message_or_key};
use burnburn_macos::panels::{
    EditorControl as E, G5Control as C, G5Line, G5Panel, ParamSection, g5_intent, g5_panel, shapes_panel,
};
use burnburn_presentation::ParamKey;
use burnburn_presentation::{
    CommandKey, HeadlessEditorHarness, MessageKey, ModifierKind, StructureEdit, StructureProblem, UiIntent,
};
use burnburn_types::DocumentId;

fn with_tool(id: &str) -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.send(UiIntent::SelectTool { document: Some(doc), id: id.into() });
    (h, doc)
}

#[test]
fn the_fillet_tool_shows_a_radius_row_in_both_languages() {
    let (h, doc) = with_tool("sketch.fillet");
    let nl = shapes_panel(&h.view(), doc, Locale::Nl);
    assert_eq!(nl.tool.len(), 1);
    assert_eq!(
        (nl.tool[0].key, nl.tool[0].label.as_str(), nl.tool[0].text.as_str()),
        (ParamKey::FilletRadius, "Straal (mm)", "2")
    );
    let en = shapes_panel(&h.view(), doc, Locale::En);
    assert_eq!(en.tool[0].label, "Radius (mm)");
}

#[test]
fn a_typed_radius_is_one_intent_with_the_text_unparsed() {
    let (h, doc) = with_tool("sketch.fillet");
    let event =
        NativeEvent::ParamCommitted { document: doc, section: ParamSection::Tool, slot: 0, text: "3,5 mm".into() };
    let Reaction::Intent(i) = translate(event, &h.view()) else { panic!("an intent") };
    assert_eq!(i, UiIntent::SetToolOption { document: doc, key: ParamKey::FilletRadius, text: "3,5 mm".into() });
}

#[test]
fn the_three_chamfer_tools_show_their_own_rows() {
    let rows = |tool: &str| -> Vec<(ParamKey, String)> {
        let (h, doc) = with_tool(tool);
        shapes_panel(&h.view(), doc, Locale::En).tool.iter().map(|r| (r.key, r.label.clone())).collect()
    };
    assert_eq!(rows("sketch.chamfer.equal_distance"), [(ParamKey::ChamferDistance, "Distance (mm)".to_owned())]);
    assert_eq!(
        rows("sketch.chamfer.two_distance"),
        [
            (ParamKey::ChamferDistance, "Distance (mm)".to_owned()),
            (ParamKey::ChamferDistance2, "Second distance (mm)".to_owned())
        ]
    );
    assert_eq!(
        rows("sketch.chamfer.distance_angle"),
        [
            (ParamKey::ChamferDistance, "Distance (mm)".to_owned()),
            (ParamKey::ChamferAngle, "Angle (degrees)".to_owned())
        ]
    );
}

// --- E06: the modifier stack in the structure panel -----------------------------------------------

fn window_with_chain() -> (HeadlessEditorHarness, DocumentId, u64) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (100.0, 50.0), (130.0, 70.0));
    h.invoke(doc, CommandKey::ToolSelect);
    h.invoke(doc, CommandKey::SelectAll);
    // The way to a chain from the panel: type a distance, press the button.
    let values = [(C::Editor(E::ModOffsetDistance), "2 mm".to_owned())];
    let make = g5_intent(&h.view(), doc, G5Panel::Structure, C::Editor(E::ModOffsetMake), &values).unwrap();
    h.send(make);
    let id = h.document(doc).unwrap().structure.modifiers[0].id;
    (h, doc, id)
}

fn lines(h: &HeadlessEditorHarness, doc: DocumentId, locale: Locale) -> Vec<G5Line> {
    g5_panel(&h.view(), doc, G5Panel::Structure, locale, &[]).expect("the structure panel").lines
}

fn press(h: &mut HeadlessEditorHarness, doc: DocumentId, c: E, values: &[(E, &str)]) -> Option<UiIntent> {
    let values: Vec<(C, String)> = values.iter().map(|(c, v)| (C::Editor(*c), (*v).to_owned())).collect();
    let intent = g5_intent(&h.view(), doc, G5Panel::Structure, C::Editor(c), &values)?;
    h.send(intent.clone());
    Some(intent)
}

#[test]
fn without_a_chain_the_panel_says_so_and_the_offset_button_waits_for_a_selection() {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    let all = lines(&h, doc, Locale::En);
    assert!(all.iter().any(|l| matches!(l, G5Line::Heading(t) if t == "Modifier stack")));
    assert!(all.iter().any(|l| matches!(l, G5Line::Text(t) if t.starts_with("No modifier chains"))));
    let make = all.iter().find(|l| l.control() == Some(C::Editor(E::ModOffsetMake))).unwrap();
    assert!(matches!(make, G5Line::Button { enabled: false, .. }), "nothing is selected");
    let nl = lines(&h, doc, Locale::Nl);
    assert!(nl.iter().any(|l| matches!(l, G5Line::Heading(t) if t == "Modifierstapel")));
}

#[test]
fn a_chain_shows_its_step_with_a_check_a_number_and_the_buttons() {
    let (h, doc, m) = window_with_chain();
    let all = lines(&h, doc, Locale::En);
    let on = all.iter().find(|l| l.control() == Some(C::Editor(E::ModStepOn { modifier: m, step: 0 }))).unwrap();
    assert!(matches!(on, G5Line::Check { caption, on: true, .. } if caption == "Step 1: Offset on"));
    let value = all.iter().find(|l| l.control() == Some(C::Editor(E::ModStepValue { modifier: m, step: 0 }))).unwrap();
    assert!(matches!(value, G5Line::Field { caption, text, .. } if caption == "Step 1: distance (mm)" && text == "2"));
    for (control, enabled) in [
        (E::ModStepUp { modifier: m, step: 0 }, false),
        (E::ModStepDown { modifier: m, step: 0 }, false),
        (E::ModStepRemove { modifier: m, step: 0 }, false),
        (E::ModBake { modifier: m }, true),
        (E::ModRemove { modifier: m }, true),
    ] {
        let line = all.iter().find(|l| l.control() == Some(C::Editor(control))).expect("the button");
        assert!(matches!(line, G5Line::Button { enabled: e, .. } if *e == enabled), "{control:?}");
    }
    let nl = lines(&h, doc, Locale::Nl);
    assert!(nl.iter().any(|l| matches!(l, G5Line::Check { caption, .. } if caption == "Stap 1: Offset actief")));
}

#[test]
fn the_controls_send_one_edit_each_and_an_untouched_field_sends_nothing() {
    let (mut h, doc, m) = window_with_chain();
    let value = E::ModStepValue { modifier: m, step: 0 };
    assert_eq!(press(&mut h, doc, value, &[(value, "2")]), None, "the shown text again is no edit");
    let sent = press(&mut h, doc, value, &[(value, "3,5 mm")]).unwrap();
    assert_eq!(
        sent,
        UiIntent::Structure {
            document: doc,
            edit: StructureEdit::SetModifierStepValue { modifier: m, step: 0, text: "3,5 mm".into() }
        }
    );
    assert_eq!(h.document(doc).unwrap().structure.modifiers[0].steps[0].value.as_ref().unwrap().1, "3.5");

    let on = E::ModStepOn { modifier: m, step: 0 };
    assert_eq!(press(&mut h, doc, on, &[(on, "1")]), None, "already on");
    let off = press(&mut h, doc, on, &[(on, "0")]).unwrap();
    assert_eq!(
        off,
        UiIntent::Structure {
            document: doc,
            edit: StructureEdit::ToggleModifierStep { modifier: m, step: 0, enabled: false }
        }
    );
    assert!(!h.document(doc).unwrap().structure.modifiers[0].steps[0].enabled);

    let bake = press(&mut h, doc, E::ModBake { modifier: m }, &[]).unwrap();
    assert_eq!(bake, UiIntent::Structure { document: doc, edit: StructureEdit::BakeModifier { modifier: m } });
    assert!(h.document(doc).unwrap().structure.modifiers.is_empty());
}

#[test]
fn a_move_beyond_the_end_is_refused_in_words() {
    let (mut h, doc, m) = window_with_chain();
    let up = press(&mut h, doc, E::ModStepUp { modifier: m, step: 0 }, &[]).unwrap();
    assert_eq!(
        up,
        UiIntent::Structure { document: doc, edit: StructureEdit::MoveModifierStep { modifier: m, step: 0, up: true } }
    );
    let problem = h.document(doc).unwrap().problem.map(|p| p.message);
    assert_eq!(problem, Some(MessageKey::Structure(StructureProblem::CannotMoveStep)));
    assert_eq!(message_or_key(problem.unwrap(), Locale::En), "The step is already at that end.");
    assert_eq!(h.document(doc).unwrap().structure.modifiers[0].steps[0].kind, ModifierKind::Offset);
}

#[test]
fn every_golf_7_editor_text_has_both_languages_with_the_same_placeholders() {
    use burnburn_macos::l10n_g7_editor::{G7Text, g7_text};
    fn placeholders(s: &str) -> Vec<String> {
        let mut out: Vec<String> = s.match_indices('{').map(|(i, _)| s[i..].chars().take(3).collect()).collect();
        out.sort();
        out
    }
    for t in G7Text::ALL {
        let (nl, en) = (g7_text(*t, Locale::Nl), g7_text(*t, Locale::En));
        assert!(!nl.is_empty() && !en.is_empty(), "{t:?}");
        assert_eq!(placeholders(nl), placeholders(en), "{t:?}");
    }
}
