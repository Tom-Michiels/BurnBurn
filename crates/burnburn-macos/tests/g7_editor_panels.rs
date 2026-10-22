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
    CommandKey, HeadlessEditorHarness, MessageKey, ModifierKind, PartShapeKind, PlateField, ShapeField, StructureEdit,
    StructureProblem, UiIntent,
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

// --- P06 / S07: plates, variants and the shapes of a part in the structure panel ---------------------

fn typed(h: &mut HeadlessEditorHarness, doc: DocumentId, c: E, values: &[(E, &str)]) -> Option<UiIntent> {
    press(h, doc, c, values)
}

fn window_with_part() -> (HeadlessEditorHarness, DocumentId, u64) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (100.0, 50.0), (130.0, 70.0));
    h.invoke(doc, CommandKey::ToolSelect);
    h.invoke(doc, CommandKey::SelectAll);
    h.send(UiIntent::Structure { document: doc, edit: StructureEdit::DefinePart { name: "Beugel".into() } });
    let part = h.document(doc).unwrap().structure.parts[0].id;
    h.send(UiIntent::Structure {
        document: doc,
        edit: StructureEdit::PlaceInstance { definition: part, x: "0".into(), y: "0".into() },
    });
    (h, doc, part)
}

fn make_plate(h: &mut HeadlessEditorHarness, doc: DocumentId) -> u64 {
    let values = [
        (E::NewPlateText(PlateField::Name), "Plaat B"),
        (E::NewPlateText(PlateField::Width), "600"),
        (E::NewPlateText(PlateField::Height), "400"),
        (E::NewPlateText(PlateField::Material), "Berken"),
        (E::NewPlateText(PlateField::Thickness), "3"),
    ];
    let sent = typed(h, doc, E::NewPlateMake, &values).unwrap();
    assert_eq!(
        sent,
        UiIntent::Structure {
            document: doc,
            edit: StructureEdit::CreatePlate {
                name: "Plaat B".into(),
                width: "600".into(),
                height: "400".into(),
                material: "Berken".into(),
                thickness: "3".into()
            }
        }
    );
    h.document(doc).unwrap().structure.plates[0].id
}

#[test]
fn a_plate_is_made_edited_given_the_selection_and_removed_from_the_panel() {
    let (mut h, doc, _) = window_with_part();
    let plate = make_plate(&mut h, doc);
    let all = lines(&h, doc, Locale::En);
    let width = all
        .iter()
        .find(|l| l.control() == Some(C::Editor(E::PlateText { plate, field: PlateField::Width })))
        .expect("the width field");
    assert!(matches!(width, G5Line::Field { caption, text, .. } if caption == "Plaat B: width (mm)" && text == "600"));

    let width_control = E::PlateText { plate, field: PlateField::Width };
    assert_eq!(typed(&mut h, doc, width_control, &[(width_control, "600")]), None, "the shown text again is no edit");
    let sent = typed(&mut h, doc, width_control, &[(width_control, "500 mm")]).unwrap();
    assert_eq!(
        sent,
        UiIntent::Structure {
            document: doc,
            edit: StructureEdit::SetPlateField { plate, field: PlateField::Width, text: "500 mm".into() }
        }
    );
    assert_eq!(h.document(doc).unwrap().structure.plates[0].width, "500");

    // The part's source rectangle (the first object) goes on the plate and off again; placing the
    // instance had selected its outlines, which follow their instance instead.
    let source = h.document(doc).unwrap().objects[0].id;
    h.send(UiIntent::Select { document: doc, ids: vec![source], mode: burnburn_presentation::SelectMode::Replace });
    let assign = typed(&mut h, doc, E::PlateAssign { plate }, &[]).unwrap();
    assert_eq!(
        assign,
        UiIntent::Structure { document: doc, edit: StructureEdit::AssignSelectionToPlate { plate: Some(plate) } }
    );
    assert_eq!(h.document(doc).unwrap().structure.plates[0].objects, 1);
    typed(&mut h, doc, E::PlateUnassign, &[]).unwrap();
    assert_eq!(h.document(doc).unwrap().structure.plates[0].objects, 0);

    typed(&mut h, doc, E::PlateRemove { plate }, &[]).unwrap();
    assert!(h.document(doc).unwrap().structure.plates.is_empty());
}

#[test]
fn the_plate_buttons_wait_for_a_selection() {
    let (mut h, doc, _) = window_with_part();
    let plate = make_plate(&mut h, doc);
    h.send(UiIntent::Select { document: doc, ids: Vec::new(), mode: burnburn_presentation::SelectMode::Replace });
    let all = lines(&h, doc, Locale::En);
    for c in [E::PlateAssign { plate }, E::PlateUnassign] {
        let line = all.iter().find(|l| l.control() == Some(C::Editor(c))).unwrap();
        assert!(matches!(line, G5Line::Button { enabled: false, .. }), "{c:?}");
    }
}

#[test]
fn an_instance_chooses_its_plate_from_a_list() {
    let (mut h, doc, _) = window_with_part();
    let plate = make_plate(&mut h, doc);
    let instance = h.document(doc).unwrap().structure.parts[0].instances[0].id;
    let c = E::InstancePlate { instance };
    let all = lines(&h, doc, Locale::Nl);
    let choice = all.iter().find(|l| l.control() == Some(C::Editor(c))).expect("a choice");
    assert!(
        matches!(choice, G5Line::Choice { options, selected: 0, .. } if options == &["Geen plaat".to_owned(), "Plaat B".to_owned()])
    );
    assert_eq!(typed(&mut h, doc, c, &[(c, "0")]), None, "none is already chosen");
    let sent = typed(&mut h, doc, c, &[(c, "1")]).unwrap();
    assert_eq!(
        sent,
        UiIntent::Structure { document: doc, edit: StructureEdit::SetInstancePlate { instance, plate: Some(plate) } }
    );
    assert_eq!(h.document(doc).unwrap().structure.plates[0].instances, 1);
    assert_eq!(typed(&mut h, doc, c, &[(c, "9")]), None, "a choice outside the list sends nothing");
}

#[test]
fn a_variant_is_renamed_and_removed_with_its_own_controls() {
    let (mut h, doc, part) = window_with_part();
    h.send(UiIntent::Structure {
        document: doc,
        edit: StructureEdit::AddVariant { definition: part, name: "groot".into() },
    });
    let rename = E::VariantRename { part, variant: 0 };
    assert_eq!(typed(&mut h, doc, rename, &[(rename, "groot")]), None);
    let sent = typed(&mut h, doc, rename, &[(rename, "extra groot")]).unwrap();
    assert_eq!(
        sent,
        UiIntent::Structure {
            document: doc,
            edit: StructureEdit::RenameVariant { definition: part, from: "groot".into(), to: "extra groot".into() }
        }
    );
    assert_eq!(h.document(doc).unwrap().structure.parts[0].variants, ["extra groot"]);
    typed(&mut h, doc, E::VariantRemove { part, variant: 0 }, &[]).unwrap();
    assert!(h.document(doc).unwrap().structure.parts[0].variants.is_empty());
}

#[test]
fn the_shapes_of_a_part_have_fields_a_name_a_remove_button_and_an_add_row() {
    let (mut h, doc, part) = window_with_part();
    let shape = h.document(doc).unwrap().structure.parts[0].shapes[0].clone();
    assert_eq!(shape.kind, PartShapeKind::Rectangle);
    let all = lines(&h, doc, Locale::En);
    for f in [ShapeField::X, ShapeField::Y, ShapeField::Width, ShapeField::Height] {
        let c = C::Editor(E::ShapeNumber { part, shape: shape.key, field: f });
        assert!(all.iter().any(|l| l.control() == Some(c)), "{f:?}");
    }
    assert!(
        !all.iter()
            .any(|l| l.control()
                == Some(C::Editor(E::ShapeNumber { part, shape: shape.key, field: ShapeField::Radius })))
    );

    let width = E::ShapeNumber { part, shape: shape.key, field: ShapeField::Width };
    let current = shape.fields.iter().find(|(f, _)| *f == ShapeField::Width).unwrap().1.clone();
    assert_eq!(typed(&mut h, doc, width, &[(width, &current)]), None);
    let sent = typed(&mut h, doc, width, &[(width, "55")]).unwrap();
    assert_eq!(
        sent,
        UiIntent::Structure {
            document: doc,
            edit: StructureEdit::SetPartShapeField {
                definition: part,
                shape: shape.key,
                field: ShapeField::Width,
                text: "55".into()
            }
        }
    );

    let before = h.document(doc).unwrap().structure.parts[0].shapes.len();
    let add = E::ShapeAdd { part, kind: PartShapeKind::Circle };
    typed(&mut h, doc, add, &[(E::ShapeAddName { part }, "gat")]).unwrap();
    assert_eq!(h.document(doc).unwrap().structure.parts[0].shapes.len(), before + 1);
    let added = h.document(doc).unwrap().structure.parts[0].shapes.last().unwrap().clone();
    assert_eq!((added.name.as_str(), added.kind), ("gat", PartShapeKind::Circle));

    let name = E::ShapeName { part, shape: added.key };
    let sent = typed(&mut h, doc, name, &[(name, "boorgat")]).unwrap();
    assert!(
        matches!(sent, UiIntent::Structure { edit: StructureEdit::RenamePartShape { ref name, .. }, .. } if name == "boorgat")
    );
    typed(&mut h, doc, E::ShapeRemove { part, shape: added.key }, &[]).unwrap();
    assert_eq!(h.document(doc).unwrap().structure.parts[0].shapes.len(), before);
}

#[test]
fn the_new_refusals_are_said_in_both_languages() {
    for p in [
        StructureProblem::UnknownPlate,
        StructureProblem::UnknownShape,
        StructureProblem::NoSuchField,
        StructureProblem::VariantInUse,
        StructureProblem::PartOutputSelected,
        StructureProblem::CannotAddFixed,
    ] {
        let (nl, en) = (
            message_or_key(MessageKey::Structure(p), Locale::Nl),
            message_or_key(MessageKey::Structure(p), Locale::En),
        );
        assert_ne!(nl, en, "{p:?}");
        assert!(!nl.contains("Structure"), "{p:?}: {nl}");
    }
}

// --- E07: several texts in the text toolbar ------------------------------------------------------------

mod several_texts {
    use std::sync::Arc;

    use burnburn_macos::adapter::{NativeEvent, Reaction, translate};
    use burnburn_macos::l10n::Locale;
    use burnburn_macos::panels::{Slot, SlotValue, text_panels};
    use burnburn_presentation::{CommandKey, HeadlessEditorHarness, SelectMode, TextEdit, UiIntent};
    use burnburn_test_support::fixtures::read_fixture;
    use burnburn_text::{FontData, FontRef, FontStyle, MemoryFontSource};
    use burnburn_types::DocumentId;

    fn fonts() -> (Arc<MemoryFontSource>, FontRef) {
        let mut src = MemoryFontSource::new();
        let data =
            Arc::new(FontData::from_bytes(read_fixture("fonts", "LiberationSans-Regular.ttf").unwrap()).unwrap());
        let regular = src.insert("Liberation Sans", FontStyle::Regular, data.clone());
        src.insert("Liberation Sans", FontStyle::Bold, data);
        let biz = Arc::new(FontData::from_bytes(read_fixture("fonts", "BIZUDPGothic-Regular.ttf").unwrap()).unwrap());
        src.insert("BIZ UDPGothic", FontStyle::Regular, biz);
        (Arc::new(src), regular)
    }

    /// Two texts; the second is 20 mm high, the first 10; both selected.
    fn window() -> (HeadlessEditorHarness, DocumentId) {
        let mut h = HeadlessEditorHarness::new();
        let doc = h.new_document().unwrap();
        let (src, font) = fonts();
        h.controller().attach_fonts(src, font);
        h.send(UiIntent::InsertText { document: doc, text: "eerste".into() });
        h.invoke(doc, CommandKey::ToolSelect);
        h.send(UiIntent::Select { document: doc, ids: vec![], mode: SelectMode::Replace });
        h.send(UiIntent::InsertText { document: doc, text: "tweede".into() });
        h.send(UiIntent::Text { document: doc, edit: TextEdit::Height("20".into()) });
        let all: Vec<_> = h.document(doc).unwrap().objects.iter().map(|o| o.id).collect();
        h.send(UiIntent::Select { document: doc, ids: all, mode: SelectMode::Replace });
        (h, doc)
    }

    fn intent(h: &HeadlessEditorHarness, doc: DocumentId, slot: Slot, value: SlotValue) -> Option<UiIntent> {
        match translate(NativeEvent::TextPanel { document: doc, slot, value }, &h.view()) {
            Reaction::Intent(i) => Some(i),
            _ => None,
        }
    }

    #[test]
    fn the_toolbar_counts_the_texts_and_marks_the_values_that_differ() {
        let (h, doc) = window();
        let bar = text_panels(&h.view(), doc, Locale::En).toolbar.expect("a toolbar");
        assert_eq!(bar.count, 2);
        assert_eq!(bar.mixed, [Slot::Height]);
        assert_eq!(bar.height, "", "a value that differs shows no value");
        assert_eq!(bar.h_space, "0", "a shared value is shown");
        assert!(bar.note.starts_with("2 texts"), "{}", bar.note);
        let nl = text_panels(&h.view(), doc, Locale::Nl).toolbar.unwrap();
        assert!(nl.note.starts_with("2 teksten"), "{}", nl.note);
    }

    #[test]
    fn a_mixed_field_takes_a_typed_value_for_all_and_an_empty_one_is_nothing() {
        let (h, doc) = window();
        assert_eq!(intent(&h, doc, Slot::Height, SlotValue::Text(String::new())), None);
        assert_eq!(
            intent(&h, doc, Slot::Height, SlotValue::Text("15".into())),
            Some(UiIntent::Text { document: doc, edit: TextEdit::Height("15".into()) })
        );
        // The shared letter spacing still sends nothing when it is only shown again.
        assert_eq!(intent(&h, doc, Slot::HSpace, SlotValue::Text("0".into())), None);
    }

    #[test]
    fn a_mixed_choice_applies_even_when_it_is_the_first_texts_value() {
        let (mut h, doc) = window();
        // Make the families differ: the second text gets the other family.
        let ids: Vec<_> = h.document(doc).unwrap().objects.iter().map(|o| o.id).collect();
        h.send(UiIntent::Select { document: doc, ids: vec![ids[1]], mode: SelectMode::Replace });
        h.send(UiIntent::Text { document: doc, edit: TextEdit::Family("BIZ UDPGothic".into()) });
        h.send(UiIntent::Select { document: doc, ids, mode: SelectMode::Replace });
        let bar = text_panels(&h.view(), doc, Locale::En).toolbar.unwrap();
        assert!(bar.mixed.contains(&Slot::Family));
        let first = bar.family;
        let sent = intent(&h, doc, Slot::Family, SlotValue::Choice(first)).expect("the pick goes to all");
        assert!(matches!(sent, UiIntent::Text { edit: TextEdit::Family(ref f), .. } if f == &bar.families[first]));
    }
}

// --- E08: text on a path as a live binding ---------------------------------------------------------------

mod text_on_path {
    use std::sync::Arc;

    use burnburn_macos::adapter::{NativeEvent, Reaction, translate};
    use burnburn_macos::l10n::Locale;
    use burnburn_macos::panels::{
        EditorControl as E, G5Control as C, G5Line, G5Panel, Slot, SlotValue, g5_intent, g5_opens_by_itself, g5_panel,
        text_panels,
    };
    use burnburn_presentation::{
        CommandKey, Distort, HeadlessEditorHarness, SelectMode, TextEdit, TextPathEdit, UiIntent,
    };
    use burnburn_test_support::fixtures::read_fixture;
    use burnburn_text::{FontData, FontRef, FontStyle, MemoryFontSource};
    use burnburn_types::DocumentId;

    fn fonts() -> (Arc<MemoryFontSource>, FontRef) {
        let mut src = MemoryFontSource::new();
        let data =
            Arc::new(FontData::from_bytes(read_fixture("fonts", "LiberationSans-Regular.ttf").unwrap()).unwrap());
        let regular = src.insert("Liberation Sans", FontStyle::Regular, data);
        (Arc::new(src), regular)
    }

    /// A text and a circle selected; the dialog is open.
    fn dialog() -> (HeadlessEditorHarness, DocumentId) {
        let mut h = HeadlessEditorHarness::new();
        let doc = h.new_document().unwrap();
        let (src, font) = fonts();
        h.controller().attach_fonts(src, font);
        h.send(UiIntent::InsertText { document: doc, text: "HIT".into() });
        let outlines: Vec<_> = h.document(doc).unwrap().objects.iter().map(|o| o.id).collect();
        h.invoke(doc, CommandKey::ToolCircle);
        h.drag(doc, (100.0, 100.0), (140.0, 100.0));
        h.invoke(doc, CommandKey::ToolSelect);
        let circle = h.document(doc).unwrap().objects.iter().map(|o| o.id).find(|i| !outlines.contains(i)).unwrap();
        let mut both = outlines;
        both.push(circle);
        h.send(UiIntent::Select { document: doc, ids: both, mode: SelectMode::Replace });
        h.invoke(doc, CommandKey::TextOnPath);
        (h, doc)
    }

    fn slot_intent(h: &HeadlessEditorHarness, doc: DocumentId, slot: Slot, value: SlotValue) -> Option<UiIntent> {
        match translate(NativeEvent::TextPanel { document: doc, slot, value }, &h.view()) {
            Reaction::Intent(i) => Some(i),
            _ => None,
        }
    }

    #[test]
    fn the_sheet_has_a_live_check_that_is_on_and_turns_it_off() {
        let (h, doc) = dialog();
        let sheet = text_panels(&h.view(), doc, Locale::En).path.expect("the sheet");
        assert!(sheet.live);
        assert_eq!(
            slot_intent(&h, doc, Slot::PathLive, SlotValue::Toggle(false)),
            Some(UiIntent::TextPath { document: doc, edit: TextPathEdit::Live(false) })
        );
        assert_eq!(slot_intent(&h, doc, Slot::PathLive, SlotValue::Toggle(true)), None, "already on");
    }

    /// A bound text selected, with its panel.
    fn bound() -> (HeadlessEditorHarness, DocumentId) {
        let (mut h, doc) = dialog();
        h.send(UiIntent::TextPath { document: doc, edit: TextPathEdit::Apply });
        let ids: Vec<_> = h.document(doc).unwrap().objects.iter().take(1).map(|o| o.id).collect();
        h.send(UiIntent::Select { document: doc, ids, mode: SelectMode::Replace });
        (h, doc)
    }

    fn path_lines(h: &HeadlessEditorHarness, doc: DocumentId, locale: Locale) -> Option<Vec<G5Line>> {
        g5_panel(&h.view(), doc, G5Panel::TextPath, locale, &[]).map(|m| m.lines)
    }

    #[test]
    fn the_path_panel_opens_by_itself_for_a_bound_text_only() {
        assert!(g5_opens_by_itself(G5Panel::TextPath));
        let (h, doc) = bound();
        assert!(path_lines(&h, doc, Locale::En).is_some());
        // A plain text, a rectangle or nothing: no panel.
        let (src, font) = fonts();
        let mut plain = HeadlessEditorHarness::new();
        let d = plain.new_document().unwrap();
        plain.controller().attach_fonts(src, font);
        plain.send(UiIntent::InsertText { document: d, text: "x".into() });
        assert!(path_lines(&plain, d, Locale::En).is_none());
    }

    #[test]
    fn the_path_panel_shows_the_binding_and_its_controls_send_text_edits() {
        let (mut h, doc) = bound();
        let lines = path_lines(&h, doc, Locale::En).unwrap();
        assert!(lines.iter().any(
            |l| matches!(l, G5Line::Text(t) if t.contains("follows") && t.contains("Pad") || t.contains("follows"))
        ));
        let field = |c: E| lines.iter().find(|l| l.control() == Some(C::Editor(c)));
        assert!(matches!(field(E::TextPathStart), Some(G5Line::Field { text, .. }) if text == "0"));
        assert!(matches!(field(E::TextPathOffset), Some(G5Line::Field { text, .. }) if text == "0"));
        assert!(matches!(field(E::TextPathReverse), Some(G5Line::Check { on: false, .. })));
        assert!(matches!(field(E::TextPathDistort), Some(G5Line::Choice { selected: 0, .. })));
        assert!(matches!(field(E::TextPathUnbind), Some(G5Line::Button { enabled: true, .. })));

        let send = |h: &mut HeadlessEditorHarness, c: E, v: &str| {
            let intent = g5_intent(&h.view(), doc, G5Panel::TextPath, C::Editor(c), &[(C::Editor(c), v.to_owned())]);
            if let Some(i) = &intent {
                h.send(i.clone());
            }
            intent
        };
        assert_eq!(send(&mut h, E::TextPathStart, "0"), None, "the shown text again is no edit");
        assert_eq!(
            send(&mut h, E::TextPathStart, "12,5 mm"),
            Some(UiIntent::Text { document: doc, edit: TextEdit::PathStart("12,5 mm".into()) })
        );
        assert_eq!(
            send(&mut h, E::TextPathReverse, "1"),
            Some(UiIntent::Text { document: doc, edit: TextEdit::PathReverse(true) })
        );
        assert_eq!(
            send(&mut h, E::TextPathDistort, "1"),
            Some(UiIntent::Text { document: doc, edit: TextEdit::PathDistort(Distort::Warp) })
        );
        assert_eq!(send(&mut h, E::TextPathDistort, "1"), None, "warp is already chosen");
        let b = h.controller().text_sources(doc)[0].path.unwrap();
        assert_eq!((b.start_mm, b.reverse, b.warp), (12.5, true, true));

        assert_eq!(
            send(&mut h, E::TextPathUnbind, ""),
            Some(UiIntent::Text { document: doc, edit: TextEdit::UnbindPath })
        );
        assert!(path_lines(&h, doc, Locale::En).is_none(), "unbound: the panel goes");
    }

    #[test]
    fn the_panel_speaks_dutch_and_names_a_lost_guide() {
        let (h, doc) = bound();
        let nl = path_lines(&h, doc, Locale::Nl).unwrap();
        assert!(nl.iter().any(|l| matches!(l, G5Line::Text(t) if t.contains("volgt"))));
        let model = g5_panel(&h.view(), doc, G5Panel::TextPath, Locale::Nl, &[]).unwrap();
        assert_eq!(model.title, "Tekst op pad");
    }
}
