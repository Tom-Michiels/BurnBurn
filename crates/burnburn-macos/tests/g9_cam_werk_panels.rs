//! G9-cam-werk, headless: the grain of a part as a free angle in the nest panel. The view models come
//! from the real controller; AppKit is not involved.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, dead_code)]

#[path = "../../burnburn-presentation/tests/common/camera.rs"]
mod rig;

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

mod recognition {
    use std::sync::Arc;

    use burnburn_image::SourceImage;
    use burnburn_macos::l10n::Locale;
    use burnburn_macos::panels::{G5Control as C, G5Line, G5Panel, g5_intent, g5_panel};
    use burnburn_presentation::{CameraWorkspaceIntent, HeadlessEditorHarness, RecognitionIntent as R, UiIntent};
    use burnburn_types::DocumentId;

    use super::rig::*;

    const A: &str = "cam-A";
    const W: u32 = 160;
    const H: u32 = 120;

    /// A blob with three features of different strength and distance: handed, unlike its mirror image.
    fn product(u: f64, v: f64) -> f64 {
        90.0 * (-(u * u + v * v) / 32.0).exp() - 45.0 * (-((u + 10.0).powi(2) + (v - 6.0).powi(2)) / 18.0).exp()
            + 60.0 * (-((u - 11.0).powi(2) + (v + 8.0).powi(2)) / 12.5).exp()
            + 50.0 * (-((u - 4.0).powi(2) + (v - 12.0).powi(2)) / 8.0).exp()
    }

    /// Products at `(x, y, mirrored)`.
    fn photo(at: &[(f64, f64, bool)]) -> SourceImage {
        let mut rgba = Vec::with_capacity((W * H * 4) as usize);
        for y in 0..H {
            for x in 0..W {
                let mut v = 128.0 + ((x * 7 + y * 13) % 5) as f64 - 2.0;
                for (cx, cy, mirrored) in at {
                    let (u, w) = (f64::from(x) - cx, f64::from(y) - cy);
                    if u.abs() <= 24.0 && w.abs() <= 24.0 {
                        v += product(if *mirrored { -u } else { u }, w);
                    }
                }
                let g = v.clamp(0.0, 255.0).round() as u8;
                rgba.extend_from_slice(&[g, g, g, 255]);
            }
        }
        SourceImage::from_rgba8(W, H, rgba).unwrap()
    }

    struct Rig {
        h: HeadlessEditorHarness,
        doc: DocumentId,
        cam: Arc<FakeCam>,
    }

    fn rig() -> Rig {
        let cam = FakeCam::new(&[(A, &[mode(W, H)])]);
        cam.authorize();
        let mut h = HeadlessEditorHarness::new();
        h.controller().attach_camera(cam.clone());
        let doc = h.new_document().unwrap();
        Rig { h, doc, cam }
    }

    fn show(r: &mut Rig, at: &[(f64, f64, bool)]) {
        r.cam.show(A, mode(W, H), photo(at));
        r.h.send(UiIntent::CameraWorkspace(CameraWorkspaceIntent::UseCamera {
            device: dev(A),
            mode: mode(W, H),
            lens: "standard".into(),
        }));
    }

    fn texts(r: &Rig, locale: Locale) -> String {
        g5_panel(&r.h.view(), r.doc, G5Panel::Recognition, locale, &[])
            .expect("the recognition panel")
            .lines
            .into_iter()
            .filter_map(|l| match l {
                G5Line::Text(t) | G5Line::Problem(t) | G5Line::Heading(t) => Some(t),
                G5Line::Check { caption, on, .. } => Some(format!("({}) {caption}", if on { "x" } else { " " })),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn intent(r: &Rig, control: C, draft: &[(C, String)]) -> Option<UiIntent> {
        g5_intent(&r.h.view(), r.doc, G5Panel::Recognition, control, draft)
    }

    fn learn(r: &mut Rig) {
        r.h.send(UiIntent::Recognition(R::Learn {
            device: dev(A),
            x: "46".into(),
            y: "46".into(),
            width: "29".into(),
            height: "29".into(),
            name: "Kaartje".into(),
        }));
    }

    fn find(r: &mut Rig) {
        r.h.send(UiIntent::Recognition(R::Find { min_percent: "90".into() }));
        r.h.run_tasks();
    }

    #[test]
    fn mirror_perspective_and_partial_are_three_choices_that_start_off_and_send_all_three() {
        let mut r = rig();
        show(&mut r, &[(60.0, 60.0, false)]);
        let nl = texts(&r, Locale::Nl);
        for caption in [
            "( ) Gespiegelde producten herkennen",
            "( ) Producten onder een hoek herkennen (perspectief)",
            "( ) Producten die deels buiten het beeld liggen herkennen",
        ] {
            assert!(nl.contains(caption), "{caption}: {nl}");
        }
        let en = texts(&r, Locale::En);
        assert!(en.contains("( ) Find mirrored products") && en.contains("( ) Find products partly outside the picture"));

        let draft = vec![(C::RecogMirror, "1".to_owned())];
        assert_eq!(
            intent(&r, C::RecogMirror, &draft),
            Some(UiIntent::Recognition(R::SetSearchExtras { mirror: true, perspective: false, partial: false }))
        );
        r.h.send(intent(&r, C::RecogMirror, &draft).unwrap());
        assert!(texts(&r, Locale::En).contains("(x) Find mirrored products"));
        let draft = vec![(C::RecogMirror, "1".to_owned()), (C::RecogPartial, "1".to_owned())];
        assert_eq!(
            intent(&r, C::RecogPartial, &draft),
            Some(UiIntent::Recognition(R::SetSearchExtras { mirror: true, perspective: false, partial: true }))
        );
        let draft = vec![(C::RecogMirror, "1".to_owned()), (C::RecogPerspective, "1".to_owned())];
        assert_eq!(
            intent(&r, C::RecogPerspective, &draft),
            Some(UiIntent::Recognition(R::SetSearchExtras { mirror: true, perspective: true, partial: false }))
        );
    }

    #[test]
    fn a_partly_visible_match_is_marked_so_in_both_languages_and_a_whole_one_is_not() {
        let mut r = rig();
        show(&mut r, &[(60.0, 60.0, false)]);
        learn(&mut r);
        r.h.send(UiIntent::Recognition(R::SetSearchExtras { mirror: false, perspective: false, partial: true }));
        show(&mut r, &[(6.0, 70.0, false), (110.0, 60.0, false)]);
        find(&mut r);
        let nl = texts(&r, Locale::Nl);
        assert_eq!(nl.matches("DEELS ZICHTBAAR").count(), 1, "only the product at the edge: {nl}");
        assert!(nl.contains("% van het sjabloon ligt in het beeld"), "{nl}");
        let en = texts(&r, Locale::En);
        assert_eq!(en.matches("PARTLY VISIBLE").count(), 1, "{en}");
    }

    #[test]
    fn a_mirrored_match_says_that_its_copy_will_be_mirrored() {
        let mut r = rig();
        show(&mut r, &[(60.0, 60.0, false)]);
        learn(&mut r);
        r.h.send(UiIntent::Recognition(R::SetSearchExtras { mirror: true, perspective: false, partial: false }));
        show(&mut r, &[(100.0, 70.0, true)]);
        find(&mut r);
        let en = texts(&r, Locale::En);
        assert!(en.contains("mirrored product (the copy is mirrored too)"), "{en}");
        assert!(texts(&r, Locale::Nl).contains("gespiegeld product (de kopie wordt ook gespiegeld)"));
    }
}
