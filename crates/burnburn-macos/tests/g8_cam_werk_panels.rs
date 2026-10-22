//! G8-cam-werk, headless: the nest panel lists every part (pages of eight) and has a grain rule
//! per part. The view models come from the real controller; AppKit is not involved.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod nest_panel {
    use std::collections::BTreeSet;

    use burnburn_macos::adapter::{NativeEvent, Reaction, translate};
    use burnburn_macos::l10n::Locale;
    use burnburn_macos::panels::{SchikkenControl as C, SchikkenLine, schikken_panel};
    use burnburn_presentation::{CommandKey, HeadlessEditorHarness, NestPartGrain};
    use burnburn_types::DocumentId;

    /// `count` rectangles of 20 x 10 in a row.
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

    fn lines(h: &HeadlessEditorHarness, doc: DocumentId, locale: Locale) -> Vec<SchikkenLine> {
        schikken_panel(&h.view(), doc, locale).unwrap().lines
    }

    fn control(h: &HeadlessEditorHarness, doc: DocumentId, c: C) -> Option<SchikkenLine> {
        lines(h, doc, Locale::En).into_iter().find(|l| l.control() == Some(c))
    }

    fn press(h: &mut HeadlessEditorHarness, doc: DocumentId, c: C, text: &str) {
        let event = NativeEvent::SchikkenControl { document: doc, control: c, values: vec![(c, text.to_owned())] };
        let Reaction::Intent(i) = translate(event, &h.view()) else { panic!("{c:?} sends an intent") };
        h.send(i);
    }

    /// The rows with a copies field, as part numbers counted from 0.
    fn rows(h: &HeadlessEditorHarness, doc: DocumentId) -> Vec<usize> {
        lines(h, doc, Locale::En)
            .iter()
            .filter_map(|l| match l.control() {
                Some(C::NestPartCopies(i)) => Some(i),
                _ => None,
            })
            .collect()
    }

    fn texts(h: &HeadlessEditorHarness, doc: DocumentId, locale: Locale) -> Vec<String> {
        lines(h, doc, locale)
            .into_iter()
            .filter_map(|l| if let SchikkenLine::Text(t) = l { Some(t) } else { None })
            .collect()
    }

    #[test]
    fn a_long_list_of_parts_is_paged_and_every_part_can_be_reached() {
        let (mut h, doc) = window(30);
        assert_eq!(rows(&h, doc), (0..8).collect::<Vec<_>>(), "the first page has eight rows");
        assert!(texts(&h, doc, Locale::En).iter().any(|t| t == "Parts 1-8 of 30"), "{:?}", texts(&h, doc, Locale::En));
        assert!(texts(&h, doc, Locale::Nl).iter().any(|t| t == "Onderdelen 1-8 van 30"));
        let Some(SchikkenLine::Button { enabled: prev, .. }) = control(&h, doc, C::NestPartsPrev) else {
            panic!("a previous button")
        };
        assert!(!prev, "there is no page before the first");

        let mut seen: BTreeSet<usize> = BTreeSet::new();
        for page in 0..4 {
            seen.extend(rows(&h, doc));
            assert_eq!(rows(&h, doc).len(), if page == 3 { 6 } else { 8 }, "page {page}");
            press(&mut h, doc, C::NestPartsNext, "");
        }
        assert_eq!(seen, (0..30).collect(), "every part is on some page");
        // After the last page the list stays on it.
        assert_eq!(rows(&h, doc), (24..30).collect::<Vec<_>>());
        assert!(texts(&h, doc, Locale::En).iter().any(|t| t == "Parts 25-30 of 30"));
        let Some(SchikkenLine::Button { enabled: next, .. }) = control(&h, doc, C::NestPartsNext) else {
            panic!("a next button")
        };
        assert!(!next);

        press(&mut h, doc, C::NestPartsPrev, "");
        assert_eq!(rows(&h, doc), (16..24).collect::<Vec<_>>());
    }

    #[test]
    fn a_few_parts_need_no_paging_and_the_row_keeps_the_number_of_the_part() {
        let (mut h, doc) = window(8);
        assert_eq!(rows(&h, doc).len(), 8);
        assert!(control(&h, doc, C::NestPartsNext).is_none(), "one page has no page buttons");
        assert!(!texts(&h, doc, Locale::En).iter().any(|t| t.starts_with("Parts ")));

        // A number typed on a row of a later page belongs to that part.
        let (mut h2, doc2) = window(12);
        press(&mut h2, doc2, C::NestPartsNext, "");
        assert_eq!(rows(&h2, doc2), vec![8, 9, 10, 11]);
        press(&mut h2, doc2, C::NestPartCopies(10), "7");
        let nest = h2.document(doc2).unwrap().schikken.nest.unwrap();
        assert_eq!(nest.input.part_copies.len(), 1);
        assert_eq!(nest.part_rows[10].copies, "7");
        assert_eq!(nest.part_rows[10].object, nest.input.part_copies[0].0);
        let _ = &mut h;
    }

    #[test]
    fn the_page_is_kept_in_range_when_there_are_fewer_parts_than_the_page_asks_for() {
        let (mut h, doc) = window(20);
        for _ in 0..2 {
            press(&mut h, doc, C::NestPartsNext, "");
        }
        assert_eq!(rows(&h, doc), (16..20).collect::<Vec<_>>());
        // Previous goes to the page that is shown minus one, not to a page that is not there.
        press(&mut h, doc, C::NestPartsPrev, "");
        assert_eq!(rows(&h, doc), (8..16).collect::<Vec<_>>());
    }

    #[test]
    fn a_grain_rule_per_part_appears_only_with_a_grain_on_the_sheet() {
        let (mut h, doc) = window(2);
        assert!(control(&h, doc, C::NestPartGrain(0)).is_none(), "no grain, no rules");
        press(&mut h, doc, C::NestGrain, "0");
        for i in 0..2 {
            let Some(SchikkenLine::Choice { options, selected, caption, .. }) = control(&h, doc, C::NestPartGrain(i))
            else {
                panic!("a grain rule for part {i}")
            };
            assert_eq!((options.len(), selected), (3, 0), "follows the sheet by default");
            assert!(caption.contains(&format!("Part {}", i + 1)), "{caption}");
        }
        let nl = lines(&h, doc, Locale::Nl);
        assert!(nl.iter().any(|l| matches!(l, SchikkenLine::Choice { caption, .. } if caption.contains("nerf"))));

        press(&mut h, doc, C::NestPartGrain(1), "1");
        let nest = h.document(doc).unwrap().schikken.nest.unwrap();
        assert_eq!(nest.input.part_grain, vec![(nest.part_rows[1].object, NestPartGrain::Across)]);
        let Some(SchikkenLine::Choice { selected, .. }) = control(&h, doc, C::NestPartGrain(1)) else { panic!("row") };
        assert_eq!(selected, 1);

        press(&mut h, doc, C::NestPartGrain(1), "2");
        let nest = h.document(doc).unwrap().schikken.nest.unwrap();
        assert_eq!(nest.input.part_grain[0].1, NestPartGrain::Free);
        // The rule of the sheet again: the part follows it and the list has no entry for it.
        press(&mut h, doc, C::NestPartGrain(1), "0");
        assert!(h.document(doc).unwrap().schikken.nest.unwrap().input.part_grain.is_empty());
    }
}
