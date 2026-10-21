//! The inspector panel of the Windows window, headless (W4-L8a): nine fields two per row, the 3x3
//! reference point, the aspect lock, the flip buttons, the cut-order priority and the shape / tool
//! parameter rows. The rows come from the controller's view model; every control becomes the same
//! intent as on the macOS inspector (`tests/inspector_controls.rs`, `shape_panels.rs` there).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_presentation::{CommandKey, FieldKey, HeadlessEditorHarness, ParamKey, RefPoint, UiIntent};
use burnburn_types::{DocumentId, Unit};
use burnburn_windows::forms::{
    ControlId, ControlValue, Form, FormContext, FormKind, FormLine, FormOutcome, Placement, inspector,
};
use burnburn_windows::l10n::Locale;
use burnburn_windows::panels::ParamSection;

fn rectangle() -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (0.0, 0.0), (120.0, 80.0));
    (h, doc)
}

fn form(h: &HeadlessEditorHarness, doc: DocumentId, locale: Locale) -> Form {
    let vm = h.view();
    inspector::form(&FormContext { vm: &vm, document: doc, locale }).expect("an inspector form")
}

fn react(h: &HeadlessEditorHarness, doc: DocumentId, id: ControlId, value: ControlValue) -> FormOutcome {
    let vm = h.view();
    inspector::react(&id, &value, &FormContext { vm: &vm, document: doc, locale: Locale::En })
}

fn field(form: &Form, key: FieldKey) -> (String, String, bool, bool) {
    match form.find(&ControlId::Inspector(key)) {
        Some(FormLine::Field { caption, text, enabled, invalid, .. }) => {
            (caption.clone(), text.clone(), *enabled, *invalid)
        }
        other => panic!("{key:?}: {other:?}"),
    }
}

#[test]
fn the_inspector_docks_right_and_shows_the_nine_fields_two_per_row_with_their_unit() {
    let (h, doc) = rectangle();
    let form = form(&h, doc, Locale::En);
    assert_eq!(form.kind, FormKind::Inspector);
    assert_eq!(form.placement, Placement::DockRight);
    let rows: Vec<Vec<ControlId>> = form
        .lines
        .iter()
        .filter_map(|l| if let FormLine::Row(inner) = l { Some(inner.iter().flat_map(FormLine::ids).collect()) } else { None })
        .collect();
    use FieldKey::*;
    assert_eq!(
        rows.iter().take(4).cloned().collect::<Vec<_>>(),
        [
            vec![ControlId::Inspector(X), ControlId::Inspector(Y)],
            vec![ControlId::Inspector(Width), ControlId::Inspector(Height)],
            vec![ControlId::Inspector(ScaleX), ControlId::Inspector(ScaleY)],
            vec![ControlId::Inspector(ShearX), ControlId::Inspector(ShearY)],
        ]
    );
    let (caption, text, enabled, invalid) = field(&form, Width);
    assert_eq!((caption.as_str(), text.as_str(), enabled, invalid), ("Width (mm)", "120", true, false));
    assert_eq!(field(&form, Rotation).0, "Rotation (°)");
    assert_eq!(field(&form, ScaleX).0, "Scale X (%)");
    // Rotation shares its row with the two flip buttons, as on the Mac.
    let flips = rows.iter().find(|r| r.contains(&ControlId::Inspector(Rotation))).unwrap();
    assert!(flips.contains(&ControlId::Flip(0)) && flips.contains(&ControlId::Flip(1)));
}

#[test]
fn the_captions_follow_the_window_language_and_the_display_unit() {
    let (h, doc) = rectangle();
    assert_eq!(field(&form(&h, doc, Locale::Nl), FieldKey::Width).0, "Breedte (mm)");
    // The display-unit preference changes the unit after the length captions, never after angles.
    let mut vm = (*h.view()).clone();
    for f in &mut vm.documents[0].inspector.as_mut().unwrap().fields {
        f.unit = Unit::Inch;
    }
    let f = inspector::form(&FormContext { vm: &vm, document: doc, locale: Locale::En }).unwrap();
    assert_eq!(field(&f, FieldKey::Width).0, "Width (in)");
    assert_eq!(field(&f, FieldKey::Rotation).0, "Rotation (°)");
}

#[test]
fn without_a_selection_the_fields_are_empty_and_off_and_so_are_the_controls_beside_them() {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    let form = form(&h, doc, Locale::En);
    let (_, text, enabled, _) = field(&form, FieldKey::Width);
    assert_eq!((text.as_str(), enabled), ("", false));
    let Some(FormLine::Grid { buttons, .. }) = form.lines.iter().find_map(|l| match l {
        FormLine::Row(inner) => inner.iter().find(|i| matches!(i, FormLine::Grid { .. })),
        other @ FormLine::Grid { .. } => Some(other),
        _ => None,
    }) else {
        panic!("a reference point grid")
    };
    assert!(buttons.iter().all(|b| !b.enabled));
    assert!(matches!(form.find(&ControlId::AspectLock), Some(FormLine::Check { enabled: false, .. })));
}

#[test]
fn a_field_commit_sends_the_raw_text_once_and_a_retry_after_an_error_still_goes() {
    let (mut h, doc) = rectangle();
    let sent = react(&h, doc, ControlId::Inspector(FieldKey::Width), ControlValue::Text("200 mm".into()));
    assert_eq!(
        sent,
        FormOutcome::Intent(UiIntent::CommitField { document: doc, field: FieldKey::Width, text: "200 mm".into() })
    );
    // Enter and the following focus loss send the same text: only changed text is a commit.
    assert_eq!(react(&h, doc, ControlId::Inspector(FieldKey::Width), ControlValue::Text("120".into())), FormOutcome::Nothing);
    // A refused text keeps what was typed, and typing it again is a retry.
    h.send(UiIntent::CommitField { document: doc, field: FieldKey::Width, text: "abc".into() });
    let form = form(&h, doc, Locale::En);
    assert!(field(&form, FieldKey::Width).3, "the field is marked invalid");
    assert!(form.lines.iter().any(|l| matches!(l, FormLine::Problem(p) if !p.is_empty())), "the reason is said");
    assert!(matches!(
        react(&h, doc, ControlId::Inspector(FieldKey::Width), ControlValue::Text("abc".into())),
        FormOutcome::Intent(UiIntent::CommitField { .. })
    ));
}

#[test]
fn the_reference_point_buttons_show_the_chosen_point_and_a_click_chooses_another() {
    let (h, doc) = rectangle();
    let form = form(&h, doc, Locale::En);
    let ids = form.ids();
    for n in 0..9 {
        assert!(ids.contains(&ControlId::RefPoint(n)), "button {n}");
    }
    let pressed: Vec<usize> = ids
        .iter()
        .filter_map(|id| match (id, form.find(id)) {
            (ControlId::RefPoint(n), Some(FormLine::Grid { buttons, .. })) => {
                buttons.iter().find(|b| b.id == *id && b.pressed).map(|_| *n)
            }
            _ => None,
        })
        .collect();
    assert_eq!(pressed, [RefPoint::ALL.iter().position(|p| *p == RefPoint::default()).unwrap()]);
    let center = RefPoint::ALL.iter().position(|p| *p == RefPoint::Center).unwrap();
    assert_eq!(
        react(&h, doc, ControlId::RefPoint(center), ControlValue::Press),
        FormOutcome::Intent(UiIntent::SetReferencePoint { document: doc, point: RefPoint::Center })
    );
    assert_eq!(react(&h, doc, ControlId::RefPoint(9), ControlValue::Press), FormOutcome::Nothing);
}

#[test]
fn the_aspect_lock_flips_what_the_window_shows() {
    let (mut h, doc) = rectangle();
    assert_eq!(
        react(&h, doc, ControlId::AspectLock, ControlValue::Toggle(true)),
        FormOutcome::Intent(UiIntent::SetAspectLock { document: doc, locked: true })
    );
    h.send(UiIntent::SetAspectLock { document: doc, locked: true });
    assert!(matches!(form(&h, doc, Locale::En).find(&ControlId::AspectLock), Some(FormLine::Check { on: true, .. })));
    assert_eq!(
        react(&h, doc, ControlId::AspectLock, ControlValue::Toggle(false)),
        FormOutcome::Intent(UiIntent::SetAspectLock { document: doc, locked: false })
    );
}

#[test]
fn the_flip_buttons_invoke_the_flip_commands_for_this_window() {
    let (h, doc) = rectangle();
    assert_eq!(
        react(&h, doc, ControlId::Flip(0), ControlValue::Press),
        FormOutcome::Intent(UiIntent::Invoke { document: Some(doc), command: CommandKey::FlipHorizontal })
    );
    assert_eq!(
        react(&h, doc, ControlId::Flip(1), ControlValue::Press),
        FormOutcome::Intent(UiIntent::Invoke { document: Some(doc), command: CommandKey::FlipVertical })
    );
    assert_eq!(react(&h, doc, ControlId::Flip(2), ControlValue::Press), FormOutcome::Nothing);
    let form = form(&h, doc, Locale::En);
    assert!(matches!(
        form.find(&ControlId::Flip(0)),
        Some(FormLine::Buttons(b)) if b.iter().all(|b| b.enabled)
    ));
}

#[test]
fn the_priority_field_is_part_of_the_panel_and_commits_like_the_others() {
    let (h, doc) = rectangle();
    let form = form(&h, doc, Locale::En);
    let (caption, text, enabled, _) = field(&form, FieldKey::Priority);
    assert_eq!((caption.as_str(), text.as_str(), enabled), ("Priority", "0", true));
    assert!(
        matches!(form.find(&ControlId::Inspector(FieldKey::Priority)), Some(FormLine::Field { hint, .. }) if hint.contains("Optimization")),
        "the full sentence is the tooltip"
    );
    assert_eq!(
        react(&h, doc, ControlId::Inspector(FieldKey::Priority), ControlValue::Text("-3".into())),
        FormOutcome::Intent(UiIntent::CommitField { document: doc, field: FieldKey::Priority, text: "-3".into() })
    );
}

#[test]
fn shape_and_tool_parameters_are_rows_below_the_fields_and_commit_as_typed() {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    // The star tool has options (points, ratio) before anything is drawn.
    h.send(UiIntent::SelectTool { document: Some(doc), id: "shape.star".into() });
    let form = form(&h, doc, Locale::En);
    let tool_ids: Vec<ControlId> = form.ids().into_iter().filter(|i| matches!(i, ControlId::Param(ParamSection::Tool, _))).collect();
    assert!(!tool_ids.is_empty(), "tool options are shown: {:?}", form.ids());
    let vm = h.view();
    let first = &vm.document(doc).unwrap().tool_options[0];
    let shown = first.text.clone();
    let id = ControlId::Param(ParamSection::Tool, 0);
    // The same text is no change; a new one is one option, raw.
    if !first.toggle {
        assert_eq!(react(&h, doc, id.clone(), ControlValue::Text(shown)), FormOutcome::Nothing);
        assert_eq!(
            react(&h, doc, id, ControlValue::Text("7".into())),
            FormOutcome::Intent(UiIntent::SetToolOption { document: doc, key: first.key, text: "7".into() })
        );
    }
    assert_eq!(first.key, ParamKey::Points);
}

#[test]
fn a_selected_basic_shape_has_parameter_rows_that_commit_one_shape_parameter() {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.send(UiIntent::SelectTool { document: Some(doc), id: "shape.star".into() });
    h.drag(doc, (0.0, 0.0), (40.0, 40.0));
    let vm = h.view();
    let panel = vm.document(doc).unwrap().shape_panel.clone().expect("a star has parameters");
    let key = panel.rows[0].key;
    let form = form(&h, doc, Locale::En);
    assert!(form.ids().contains(&ControlId::Param(ParamSection::Shape, 0)));
    assert_eq!(
        react(&h, doc, ControlId::Param(ParamSection::Shape, 0), ControlValue::Text("3".into())),
        FormOutcome::Intent(UiIntent::CommitShapeParam { document: doc, key, text: "3".into() })
    );
}
