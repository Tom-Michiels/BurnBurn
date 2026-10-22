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
