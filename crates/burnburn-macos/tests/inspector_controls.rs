//! W2-L1 (bewerken), headless: what the inspector offers for a selection: the nine fields with
//! their units, the 3×3 reference point, the aspect lock and the flip buttons. The view model comes
//! from the real `AppController`; the host only forwards clicks and raw field text.

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers outside #[test] fns

use burnburn_macos::adapter::{CommitCause, NativeEvent, Reaction, translate};
use burnburn_macos::l10n::Locale;
use burnburn_macos::panels::{
    INSPECTOR_FIELDS, INSPECTOR_FLIP_BUTTONS, INSPECTOR_ROWS, aspect_lock_intent, field_unit, flip_intent,
    inspector_controls, reference_point_intent, reference_point_label,
};
use burnburn_presentation::{CommandKey, FieldKey, HeadlessEditorHarness, RefPoint, SelectMode, UiIntent};
use burnburn_types::{DocumentId, StableId};

fn rectangles(count: u32) -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::ToolRectangle);
    for i in 0..count {
        let x = f64::from(i) * 60.0;
        h.drag(doc, (x, 0.0), (x + 40.0, 30.0));
    }
    (h, doc)
}

#[test]
fn the_inspector_lists_position_size_and_the_transform_actions_in_that_order() {
    assert_eq!(
        INSPECTOR_FIELDS,
        [
            FieldKey::X,
            FieldKey::Y,
            FieldKey::Width,
            FieldKey::Height,
            FieldKey::Rotation,
            FieldKey::ScaleX,
            FieldKey::ScaleY,
            FieldKey::ShearX,
            FieldKey::ShearY,
        ]
    );
}

#[test]
fn every_listed_field_exists_for_one_object_and_for_a_multi_selection() {
    for count in [1, 2] {
        let (mut h, doc) = rectangles(count);
        h.invoke(doc, CommandKey::SelectAll);
        let inspector = h.document(doc).unwrap().inspector.expect("a selection has an inspector");
        for key in INSPECTOR_FIELDS {
            assert!(inspector.field(key).is_some(), "{count} object(s): presentation offers {key:?}");
        }
    }
}

#[test]
fn units_follow_the_field_not_the_document() {
    for locale in [Locale::Nl, Locale::En] {
        for key in [FieldKey::X, FieldKey::Y, FieldKey::Width, FieldKey::Height] {
            assert_eq!(field_unit(key, locale), "mm", "{key:?}");
        }
        for key in [FieldKey::Rotation, FieldKey::ShearX, FieldKey::ShearY] {
            assert_eq!(field_unit(key, locale), "°", "{key:?}");
        }
        for key in [FieldKey::ScaleX, FieldKey::ScaleY] {
            assert_eq!(field_unit(key, locale), "%", "{key:?}");
        }
    }
}

/// A transform action shows its neutral value; typing another amount is a commit with the raw
/// text (never parsed in the host), and putting the neutral value back is no commit.
#[test]
fn a_scale_commit_forwards_the_raw_text_and_the_neutral_value_is_not_a_commit() {
    let (mut h, doc) = rectangles(1);
    let commit = |h: &HeadlessEditorHarness, field, text: &str| {
        translate(
            NativeEvent::FieldCommitted { document: doc, field, text: text.into(), cause: CommitCause::Return },
            &h.view(),
        )
    };
    let shown = h.document(doc).unwrap().inspector.unwrap().field(FieldKey::ScaleX).unwrap().text.clone();
    assert_eq!(shown, "100");
    assert_eq!(commit(&h, FieldKey::ScaleX, "100"), Reaction::Nothing);
    let Reaction::Intent(intent) = commit(&h, FieldKey::ScaleX, "150,5") else { panic!("a new amount is a commit") };
    assert_eq!(intent, UiIntent::CommitField { document: doc, field: FieldKey::ScaleX, text: "150,5".into() });
    let before = h.document(doc).unwrap().inspector.unwrap().field(FieldKey::Width).unwrap().text.clone();
    h.send(intent);
    let after = h.document(doc).unwrap().inspector.unwrap().field(FieldKey::Width).unwrap().text.clone();
    assert_ne!(before, after, "the width changed by the typed percentage");
    let again = h.document(doc).unwrap().inspector.unwrap().field(FieldKey::ScaleX).unwrap().text.clone();
    assert_eq!(again, "100", "the action field shows the neutral value again");
}

#[test]
fn the_reference_grid_has_nine_labelled_points_and_a_click_sends_that_point() {
    let (mut h, doc) = rectangles(1);
    let mut labels = Vec::new();
    for (index, point) in RefPoint::ALL.iter().enumerate() {
        let index = isize::try_from(index).unwrap();
        assert_eq!(
            reference_point_intent(doc, index),
            Some(UiIntent::SetReferencePoint { document: doc, point: *point }),
            "button {index}"
        );
        for locale in [Locale::Nl, Locale::En] {
            let label = reference_point_label(*point, locale);
            assert!(!label.is_empty());
            labels.push((locale, label));
        }
    }
    labels.sort();
    labels.dedup();
    assert_eq!(labels.len(), 18, "every point has its own label in both languages");
    assert_eq!(reference_point_intent(doc, 9), None);
    assert_eq!(reference_point_intent(doc, -1), None);

    assert_eq!(inspector_controls(&h.view(), doc).reference_point, RefPoint::BottomLeft, "the default");
    h.send(reference_point_intent(doc, 4).unwrap());
    assert_eq!(inspector_controls(&h.view(), doc).reference_point, RefPoint::Center);
}

#[test]
fn the_aspect_lock_button_toggles_the_view_model_state() {
    let (mut h, doc) = rectangles(1);
    assert!(!inspector_controls(&h.view(), doc).aspect_locked);
    let on = aspect_lock_intent(&h.view(), doc).unwrap();
    assert_eq!(on, UiIntent::SetAspectLock { document: doc, locked: true });
    h.send(on);
    assert!(inspector_controls(&h.view(), doc).aspect_locked);
    assert_eq!(aspect_lock_intent(&h.view(), doc), Some(UiIntent::SetAspectLock { document: doc, locked: false }));
    assert_eq!(aspect_lock_intent(&h.view(), <DocumentId as StableId>::from_raw(404)), None, "a closed window");
}

#[test]
fn the_controls_are_off_without_a_selection_and_the_flip_buttons_invoke_the_flip_commands() {
    let (mut h, doc) = rectangles(1);
    assert_eq!(INSPECTOR_FLIP_BUTTONS, [CommandKey::FlipHorizontal, CommandKey::FlipVertical]);
    let c = inspector_controls(&h.view(), doc);
    assert!(c.enabled);
    assert_eq!(c.flip_enabled, [true, true]);
    assert_eq!(
        flip_intent(doc, 0),
        Some(UiIntent::Invoke { document: Some(doc), command: CommandKey::FlipHorizontal })
    );
    assert_eq!(flip_intent(doc, 1), Some(UiIntent::Invoke { document: Some(doc), command: CommandKey::FlipVertical }));
    assert_eq!(flip_intent(doc, 2), None);

    h.send(UiIntent::Select { document: doc, ids: vec![], mode: SelectMode::Replace });
    let c = inspector_controls(&h.view(), doc);
    assert!(!c.enabled, "nothing selected, nothing to transform");
    assert_eq!(c.flip_enabled, [false, false]);
}

/// The widgets only report which control was clicked; the adapter turns that into the same
/// intents the headless helpers build (and drops late callbacks of a closed window).
#[test]
fn clicks_on_the_inspector_controls_become_intents_for_open_windows_only() {
    let (h, doc) = rectangles(1);
    let vm = h.view();
    assert_eq!(
        translate(NativeEvent::ReferencePointChosen { document: doc, index: 2 }, &vm),
        Reaction::Intent(UiIntent::SetReferencePoint { document: doc, point: RefPoint::TopRight })
    );
    assert_eq!(translate(NativeEvent::ReferencePointChosen { document: doc, index: 12 }, &vm), Reaction::Nothing);
    assert_eq!(
        translate(NativeEvent::AspectLockToggled { document: doc }, &vm),
        Reaction::Intent(UiIntent::SetAspectLock { document: doc, locked: true })
    );
    let gone = <DocumentId as StableId>::from_raw(404);
    assert_eq!(translate(NativeEvent::ReferencePointChosen { document: gone, index: 0 }, &vm), Reaction::Nothing);
    assert_eq!(translate(NativeEvent::AspectLockToggled { document: gone }, &vm), Reaction::Nothing);
}

/// The window lays the fields out two per row: every field is on exactly one row, X beside Y,
/// width beside height, so nine fields fit the inspector's height next to the plan and machine panels.
#[test]
fn the_rows_hold_every_field_once_with_pairs_that_belong_together() {
    let in_rows: Vec<FieldKey> = INSPECTOR_ROWS.iter().flat_map(|(a, b)| std::iter::once(*a).chain(*b)).collect();
    let mut sorted = in_rows.clone();
    sorted.sort();
    sorted.dedup();
    let mut expected = INSPECTOR_FIELDS.to_vec();
    expected.sort();
    assert_eq!(sorted, expected, "every inspector field is on a row, none twice: {in_rows:?}");
    assert_eq!(INSPECTOR_ROWS[0], (FieldKey::X, Some(FieldKey::Y)));
    assert_eq!(INSPECTOR_ROWS[1], (FieldKey::Width, Some(FieldKey::Height)));
    assert!(INSPECTOR_ROWS.contains(&(FieldKey::ScaleX, Some(FieldKey::ScaleY))));
    assert!(INSPECTOR_ROWS.contains(&(FieldKey::ShearX, Some(FieldKey::ShearY))));
}
