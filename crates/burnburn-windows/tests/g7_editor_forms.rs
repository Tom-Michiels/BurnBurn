//! G7-editor forms, headless on the Windows side: the fillet and chamfer options (E05) are rows of
//! the inspector, taken from the same model as the Mac panel (`burnburn-macos/tests/g7_editor_panels.rs`).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_presentation::{HeadlessEditorHarness, ParamKey, UiIntent};
use burnburn_types::DocumentId;
use burnburn_windows::forms::{ControlId, ControlValue, FormContext, FormLine, FormOutcome, inspector};
use burnburn_windows::l10n::Locale;
use burnburn_windows::panels::ParamSection;

fn with_tool(id: &str) -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.send(UiIntent::SelectTool { document: Some(doc), id: id.into() });
    (h, doc)
}

#[test]
fn the_fillet_radius_is_an_inspector_field_that_sends_the_text_unparsed() {
    let (h, doc) = with_tool("sketch.fillet");
    let vm = h.view();
    let ctx = FormContext { vm: &vm, document: doc, locale: Locale::Nl };
    let form = inspector::form(&ctx).expect("an inspector form");
    let id = ControlId::Param(ParamSection::Tool, 0);
    match form.find(&id) {
        Some(FormLine::Field { caption, text, invalid: false, .. }) => {
            assert_eq!((caption.as_str(), text.as_str()), ("Straal (mm)", "2"));
        }
        other => panic!("{other:?}"),
    }
    let outcome = inspector::react(&id, &ControlValue::Text("3,5 mm".into()), &ctx);
    assert_eq!(
        outcome,
        FormOutcome::Intent(UiIntent::SetToolOption {
            document: doc,
            key: ParamKey::FilletRadius,
            text: "3,5 mm".into()
        })
    );
}

#[test]
fn the_distance_angle_chamfer_has_two_rows() {
    let (h, doc) = with_tool("sketch.chamfer.distance_angle");
    let vm = h.view();
    let form = inspector::form(&FormContext { vm: &vm, document: doc, locale: Locale::En }).expect("a form");
    assert!(form.find(&ControlId::Param(ParamSection::Tool, 0)).is_some());
    assert!(form.find(&ControlId::Param(ParamSection::Tool, 1)).is_some());
    assert!(form.find(&ControlId::Param(ParamSection::Tool, 2)).is_none());
}

// --- E06: the modifier stack in the structure form ---------------------------------------------------

mod stack {
    use burnburn_presentation::{CommandKey, HeadlessEditorHarness, StructureEdit, UiIntent};
    use burnburn_types::DocumentId;
    use burnburn_windows::forms::g5;
    use burnburn_windows::forms::{
        ControlId, ControlValue, Form, FormContext, FormKind, FormLine, FormOutcome, PanelState,
    };
    use burnburn_windows::l10n::Locale;
    use burnburn_windows::panels::{EditorControl as E, G5Control as C, G5Panel as P};

    fn shown() -> PanelState {
        PanelState { g5_shown: vec![P::Structure], ..PanelState::default() }
    }

    fn window_with_chain() -> (HeadlessEditorHarness, DocumentId, u64) {
        let mut h = HeadlessEditorHarness::new();
        let doc = h.new_document().unwrap();
        h.invoke(doc, CommandKey::ToolRectangle);
        h.drag(doc, (100.0, 50.0), (130.0, 70.0));
        h.invoke(doc, CommandKey::ToolSelect);
        h.invoke(doc, CommandKey::SelectAll);
        h.send(UiIntent::Structure {
            document: doc,
            edit: StructureEdit::CreateOffsetModifier { distance: "2".into() },
        });
        let id = h.document(doc).unwrap().structure.modifiers[0].id;
        (h, doc, id)
    }

    fn form(h: &HeadlessEditorHarness, doc: DocumentId, locale: Locale) -> Form {
        let vm = h.view();
        g5::forms(&FormContext { vm: &vm, document: doc, locale }, &shown(), &Vec::new())
            .into_iter()
            .find(|f| f.kind == FormKind::G5(P::Structure))
            .expect("the structure form")
    }

    fn react(h: &HeadlessEditorHarness, doc: DocumentId, c: E, values: Vec<(ControlId, ControlValue)>) -> FormOutcome {
        let vm = h.view();
        g5::react(
            P::Structure,
            &ControlId::G5(C::Editor(c)),
            &values,
            &FormContext { vm: &vm, document: doc, locale: Locale::En },
        )
    }

    #[test]
    fn the_form_has_the_check_the_number_and_the_buttons_of_the_mac_panel() {
        let (h, doc, m) = window_with_chain();
        let f = form(&h, doc, Locale::En);
        assert!(matches!(
            f.find(&ControlId::G5(C::Editor(E::ModStepOn { modifier: m, step: 0 }))),
            Some(FormLine::Check { caption, on: true, .. }) if caption == "Step 1: Offset on"
        ));
        assert!(matches!(
            f.find(&ControlId::G5(C::Editor(E::ModStepValue { modifier: m, step: 0 }))),
            Some(FormLine::Field { text, .. }) if text == "2"
        ));
        for c in [E::ModBake { modifier: m }, E::ModRemove { modifier: m }, E::ModOffsetMake] {
            assert!(f.find(&ControlId::G5(C::Editor(c))).is_some(), "{c:?}");
        }
        let nl = form(&h, doc, Locale::Nl);
        assert!(matches!(
            nl.find(&ControlId::G5(C::Editor(E::ModStepOn { modifier: m, step: 0 }))),
            Some(FormLine::Check { caption, .. }) if caption == "Stap 1: Offset actief"
        ));
    }

    #[test]
    fn a_check_and_a_button_send_the_same_intents_as_the_mac_panel() {
        let (mut h, doc, m) = window_with_chain();
        let on = ControlId::G5(C::Editor(E::ModStepOn { modifier: m, step: 0 }));
        let off = react(&h, doc, E::ModStepOn { modifier: m, step: 0 }, vec![(on, ControlValue::Toggle(false))]);
        assert_eq!(
            off,
            FormOutcome::Intent(UiIntent::Structure {
                document: doc,
                edit: StructureEdit::ToggleModifierStep { modifier: m, step: 0, enabled: false }
            })
        );
        let bake = react(&h, doc, E::ModBake { modifier: m }, Vec::new());
        assert_eq!(
            bake,
            FormOutcome::Intent(UiIntent::Structure {
                document: doc,
                edit: StructureEdit::BakeModifier { modifier: m }
            })
        );
        if let FormOutcome::Intent(i) = bake {
            h.send(i);
        }
        assert!(h.document(doc).unwrap().structure.modifiers.is_empty());
    }
}

// --- P06 / S07: plates, variants and shapes in the structure form ------------------------------------

mod plates_parts {
    use burnburn_presentation::{
        CommandKey, HeadlessEditorHarness, PartShapeKind, PlateField, ShapeField, StructureEdit, UiIntent,
    };
    use burnburn_types::DocumentId;
    use burnburn_windows::forms::g5;
    use burnburn_windows::forms::{
        ChoiceItem, ControlId, ControlValue, Form, FormContext, FormKind, FormLine, FormOutcome, PanelState,
    };
    use burnburn_windows::l10n::Locale;
    use burnburn_windows::panels::{EditorControl as E, G5Control as C, G5Panel as P};

    fn window_with_part() -> (HeadlessEditorHarness, DocumentId, u64, u64) {
        let mut h = HeadlessEditorHarness::new();
        let doc = h.new_document().unwrap();
        h.invoke(doc, CommandKey::ToolRectangle);
        h.drag(doc, (100.0, 50.0), (130.0, 70.0));
        h.invoke(doc, CommandKey::ToolSelect);
        h.invoke(doc, CommandKey::SelectAll);
        let send = |h: &mut HeadlessEditorHarness, edit| h.send(UiIntent::Structure { document: doc, edit });
        send(&mut h, StructureEdit::DefinePart { name: "Beugel".into() });
        let part = h.document(doc).unwrap().structure.parts[0].id;
        send(&mut h, StructureEdit::PlaceInstance { definition: part, x: "0".into(), y: "0".into() });
        send(
            &mut h,
            StructureEdit::CreatePlate {
                name: "Plaat B".into(),
                width: "600".into(),
                height: "400".into(),
                material: String::new(),
                thickness: String::new(),
            },
        );
        let plate = h.document(doc).unwrap().structure.plates[0].id;
        (h, doc, part, plate)
    }

    fn form(h: &HeadlessEditorHarness, doc: DocumentId) -> Form {
        let vm = h.view();
        let state = PanelState { g5_shown: vec![P::Structure], ..PanelState::default() };
        g5::forms(&FormContext { vm: &vm, document: doc, locale: Locale::En }, &state, &Vec::new())
            .into_iter()
            .find(|f| f.kind == FormKind::G5(P::Structure))
            .expect("the structure form")
    }

    fn react(h: &HeadlessEditorHarness, doc: DocumentId, c: E, values: Vec<(ControlId, ControlValue)>) -> FormOutcome {
        let vm = h.view();
        g5::react(
            P::Structure,
            &ControlId::G5(C::Editor(c)),
            &values,
            &FormContext { vm: &vm, document: doc, locale: Locale::En },
        )
    }

    #[test]
    fn the_form_has_the_plate_fields_and_the_new_plate_row_of_the_mac_panel() {
        let (h, doc, _, plate) = window_with_part();
        let f = form(&h, doc);
        for field in
            [PlateField::Name, PlateField::Width, PlateField::Height, PlateField::Material, PlateField::Thickness]
        {
            assert!(f.find(&ControlId::G5(C::Editor(E::PlateText { plate, field }))).is_some(), "{field:?}");
            assert!(f.find(&ControlId::G5(C::Editor(E::NewPlateText(field)))).is_some(), "{field:?}");
        }
        for c in [E::NewPlateMake, E::PlateUnassign, E::PlateRemove { plate }, E::PlateAssign { plate }] {
            assert!(f.find(&ControlId::G5(C::Editor(c))).is_some(), "{c:?}");
        }
    }

    #[test]
    fn typed_text_makes_a_plate_and_a_choice_moves_an_instance() {
        let (h, doc, _, plate) = window_with_part();
        let id = |c: E| ControlId::G5(C::Editor(c));
        let values = vec![
            (id(E::NewPlateText(PlateField::Name)), ControlValue::Text("Plaat C".into())),
            (id(E::NewPlateText(PlateField::Width)), ControlValue::Text("300".into())),
            (id(E::NewPlateText(PlateField::Height)), ControlValue::Text("200".into())),
        ];
        assert_eq!(
            react(&h, doc, E::NewPlateMake, values),
            FormOutcome::Intent(UiIntent::Structure {
                document: doc,
                edit: StructureEdit::CreatePlate {
                    name: "Plaat C".into(),
                    width: "300".into(),
                    height: "200".into(),
                    material: String::new(),
                    thickness: String::new()
                }
            })
        );

        let instance = h.document(doc).unwrap().structure.parts[0].instances[0].id;
        let c = E::InstancePlate { instance };
        let f = form(&h, doc);
        assert!(matches!(
            f.find(&id(c)),
            Some(FormLine::Choice { items, selected: 0, .. }) if items == &vec![ChoiceItem::plain("No plate".to_owned()), ChoiceItem::plain("Plaat B".to_owned())]
        ));
        assert_eq!(
            react(&h, doc, c, vec![(id(c), ControlValue::Choice(1))]),
            FormOutcome::Intent(UiIntent::Structure {
                document: doc,
                edit: StructureEdit::SetInstancePlate { instance, plate: Some(plate) }
            })
        );
    }

    #[test]
    fn the_part_section_has_variant_and_shape_controls() {
        let (mut h, doc, part, _) = window_with_part();
        h.send(UiIntent::Structure {
            document: doc,
            edit: StructureEdit::AddVariant { definition: part, name: "groot".into() },
        });
        let shape = h.document(doc).unwrap().structure.parts[0].shapes[0].key;
        let f = form(&h, doc);
        let id = |c: E| ControlId::G5(C::Editor(c));
        for c in [
            E::VariantRename { part, variant: 0 },
            E::VariantRemove { part, variant: 0 },
            E::ShapeName { part, shape },
            E::ShapeNumber { part, shape, field: ShapeField::Width },
            E::ShapeRemove { part, shape },
            E::ShapeAddName { part },
            E::ShapeAdd { part, kind: PartShapeKind::Rectangle },
            E::ShapeAdd { part, kind: PartShapeKind::Circle },
        ] {
            assert!(f.find(&id(c)).is_some(), "{c:?}");
        }
        let rename = E::VariantRename { part, variant: 0 };
        assert_eq!(
            react(&h, doc, rename, vec![(id(rename), ControlValue::Text("reuze".into()))]),
            FormOutcome::Intent(UiIntent::Structure {
                document: doc,
                edit: StructureEdit::RenameVariant { definition: part, from: "groot".into(), to: "reuze".into() }
            })
        );
    }
}
