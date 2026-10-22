//! G7-editor panels, headless on the Mac: the fillet and chamfer options (E05). The captions come
//! in the window language and a typed value is one `SetToolOption` with the text unparsed (DR-05).
//! The Windows host shows the same rows (`burnburn-windows/tests/g7_editor_forms.rs`).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_macos::adapter::{NativeEvent, Reaction, translate};
use burnburn_macos::l10n::Locale;
use burnburn_macos::panels::{ParamSection, shapes_panel};
use burnburn_presentation::{HeadlessEditorHarness, ParamKey, UiIntent};
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
