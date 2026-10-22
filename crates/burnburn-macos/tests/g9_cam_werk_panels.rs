//! G9-cam-werk, headless: the grain of a part as a free angle in the nest panel. The view models come
//! from the real controller; AppKit is not involved.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod nest_panel {
    use burnburn_macos::adapter::{NativeEvent, Reaction, translate};
    use burnburn_macos::l10n::Locale;
    use burnburn_macos::panels::{SchikkenControl as C, SchikkenLine, schikken_panel};
    use burnburn_presentation::{CommandKey, HeadlessEditorHarness, NestPartGrain};
    use burnburn_types::DocumentId;

    fn window(count: usize) -> (HeadlessEditorHarness, DocumentId) {
        let mut h = HeadlessEditorHarness::new();
        let doc = h.new_document().unwrap();
        h.invoke(doc, CommandKey::ToolRectangle);
        for i in 0..count {
            let x = i as f64 * 30.0;
            h.drag(doc, (x, 0.0), (x + 20.0, 10.0));
        }
        h.invoke(doc, CommandKey::ToolSelect);
        h.invoke(doc, CommandKey::SelectAll);
        h.invoke(doc, CommandKey::QuickNest);
        (h, doc)
    }

    fn control(h: &HeadlessEditorHarness, doc: DocumentId, locale: Locale, c: C) -> Option<SchikkenLine> {
        schikken_panel(&h.view(), doc, locale).unwrap().lines.into_iter().find(|l| l.control() == Some(c))
    }

    fn press(h: &mut HeadlessEditorHarness, doc: DocumentId, c: C, text: &str) {
        let event = NativeEvent::SchikkenControl { document: doc, control: c, values: vec![(c, text.to_owned())] };
        let Reaction::Intent(i) = translate(event, &h.view()) else { panic!("{c:?} sends an intent") };
        h.send(i);
    }

    fn rule(h: &HeadlessEditorHarness, doc: DocumentId, row: usize) -> Vec<NestPartGrain> {
        let nest = h.document(doc).unwrap().schikken.nest.unwrap();
        nest.input.part_grain.iter().filter(|(o, _)| *o == nest.part_rows[row].object).map(|(_, g)| *g).collect()
    }

    #[test]
    fn the_fourth_grain_choice_is_an_angle_with_a_field_that_appears_with_it() {
        let (mut h, doc) = window(2);
        press(&mut h, doc, C::NestGrain, "0");
        let Some(SchikkenLine::Choice { options, selected, .. }) = control(&h, doc, Locale::En, C::NestPartGrain(0))
        else {
            panic!("a grain rule")
        };
        assert_eq!((options.len(), selected), (4, 0));
        assert!(options[3].contains("angle"), "{options:?}");
        assert!(control(&h, doc, Locale::En, C::NestPartGrainAngle(0)).is_none(), "no angle without the choice");

        press(&mut h, doc, C::NestPartGrain(0), "3");
        assert_eq!(rule(&h, doc, 0), vec![NestPartGrain::Angle(450)], "a first angle of 45 degrees");
        let Some(SchikkenLine::Choice { selected, .. }) = control(&h, doc, Locale::En, C::NestPartGrain(0)) else {
            panic!("row")
        };
        assert_eq!(selected, 3);
        let Some(SchikkenLine::Field { text, caption, .. }) = control(&h, doc, Locale::En, C::NestPartGrainAngle(0))
        else {
            panic!("an angle field")
        };
        assert_eq!(text, "45");
        assert!(caption.contains("Part 1"), "{caption}");
        assert!(control(&h, doc, Locale::En, C::NestPartGrainAngle(1)).is_none(), "only on the part that asks");
        let Some(SchikkenLine::Field { caption, .. }) = control(&h, doc, Locale::Nl, C::NestPartGrainAngle(0)) else {
            panic!("an angle field")
        };
        assert!(caption.contains("hoek"), "{caption}");
    }

    #[test]
    fn typing_an_angle_sets_it_to_a_tenth_and_a_bad_text_changes_nothing() {
        let (mut h, doc) = window(1);
        press(&mut h, doc, C::NestGrain, "0");
        press(&mut h, doc, C::NestPartGrain(0), "3");
        press(&mut h, doc, C::NestPartGrainAngle(0), "22,5");
        assert_eq!(rule(&h, doc, 0), vec![NestPartGrain::Angle(225)], "a decimal comma is accepted");
        press(&mut h, doc, C::NestPartGrainAngle(0), "-30");
        assert_eq!(rule(&h, doc, 0), vec![NestPartGrain::Angle(1500)], "-30 degrees is the same grain as 150");
        press(&mut h, doc, C::NestPartGrainAngle(0), "veel");
        assert_eq!(rule(&h, doc, 0), vec![NestPartGrain::Angle(1500)], "unreadable text keeps the angle");
        let Some(SchikkenLine::Field { text, .. }) = control(&h, doc, Locale::En, C::NestPartGrainAngle(0)) else {
            panic!("field")
        };
        assert_eq!(text, "150");

        // Back to a rule without an angle removes the field again.
        press(&mut h, doc, C::NestPartGrain(0), "1");
        assert_eq!(rule(&h, doc, 0), vec![NestPartGrain::Across]);
        assert!(control(&h, doc, Locale::En, C::NestPartGrainAngle(0)).is_none());
    }
}
