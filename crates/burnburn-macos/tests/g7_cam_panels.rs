//! G7-cam, headless: Tab Cut Power in the Cut Settings sheet (caption, greyed reason) and in the
//! preview (orange tab stretches, the pick says its power). The view models come from the real
//! controller; AppKit is not involved.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_job::{Feature, MachineCapabilities, MachineFrame, MachineProfile, OriginCorner, PowerRatio, Support};
use burnburn_macos::app_view::{PickNames, PreviewFrame, preview_panel_named, render_preview};
use burnburn_macos::l10n::Locale;
use burnburn_macos::panels::{OptionRow, cut_settings_panel, option_rows};
use burnburn_presentation::{
    AppIntent, CommandKey, CutExtras, CutField, HeadlessEditorHarness, OperationKind, PreviewIntent, SelectMode,
    UiIntent,
};
use burnburn_types::{DocumentId, GeometryId, OperationId};

fn profile(caps: MachineCapabilities) -> MachineProfile {
    MachineProfile::new(
        "research-1500x900",
        1,
        MachineFrame::new(1500.0, 900.0, OriginCorner::BottomLeft).unwrap(),
        1000.0,
        400.0,
        PowerRatio::new(0.0).unwrap(),
        PowerRatio::new(1.0).unwrap(),
    )
    .unwrap()
    .with_capabilities(caps)
    .unwrap()
}

fn ruida() -> burnburn_ruida::RuidaProfile {
    burnburn_ruida::RuidaProfile {
        magic: burnburn_ruida::Magic::FIXTURE_0X88,
        overscan: burnburn_ruida::OverscanPolicy::ControllerManaged,
    }
}

fn layer(h: &mut HeadlessEditorHarness, tab_power: Support) -> (DocumentId, OperationId) {
    let doc = h.new_document().unwrap();
    let caps = MachineCapabilities::conservative().with(Feature::TabCutPower, tab_power);
    h.send(UiIntent::SetMachineProfile { profile: Box::new(profile(caps)), ruida: ruida() });
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (0.0, 0.0), (40.0, 20.0));
    h.invoke(doc, CommandKey::ToolSelect);
    let id: GeometryId = h.document(doc).unwrap().objects[0].id;
    h.send(UiIntent::Select { document: doc, ids: vec![id], mode: SelectMode::Replace });
    h.send(UiIntent::AssignLayer { document: doc, slot: 1, add: false });
    (doc, h.document(doc).unwrap().operations[0].id)
}

fn row(rows: &[OptionRow], f: CutField) -> &OptionRow {
    rows.iter().find(|r| r.field == f).unwrap_or_else(|| panic!("no row for {f:?}"))
}

#[test]
fn the_sheet_has_a_captioned_tab_power_row_greyed_with_its_reason_without_proof() {
    let mut h = HeadlessEditorHarness::new();
    let (doc, op) = layer(&mut h, Support::Unverified { reason: "no RD form".into() });
    h.send(UiIntent::OpenCutSettings { document: doc, operation: op });
    let panel = cut_settings_panel(&h.view(), doc, Locale::En).unwrap();
    let r = row(&panel.options, CutField::TabsPower);
    assert!(r.caption.contains("Tab power"), "{}", r.caption);
    assert!(!r.enabled);
    assert!(
        r.note.to_lowercase().contains("missing capability") || r.note.to_lowercase().contains("tab cut power"),
        "{:?}",
        r.note
    );
    let nl = cut_settings_panel(&h.view(), doc, Locale::Nl).unwrap();
    assert!(row(&nl.options, CutField::TabsPower).caption.contains("Tabvermogen"));

    // With proof, and tabs on, the row is a plain text field with the stored value.
    let mut h = HeadlessEditorHarness::new();
    let (doc, op) = layer(&mut h, Support::Supported);
    h.send(UiIntent::OpenCutSettings { document: doc, operation: op });
    let sheet = h.document(doc).unwrap().cut_settings.unwrap();
    let live = sheet.extras.clone().merged(&CutExtras::new().with_flag(CutField::TabsEnabled, true));
    let rows = option_rows(&sheet, OperationKind::Line, &live, Locale::En);
    let r = row(&rows, CutField::TabsPower);
    assert!(r.enabled, "{r:?}");
}

fn tabbed_preview() -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let (doc, op) = layer(&mut h, Support::Supported);
    h.send(UiIntent::OpenCutSettings { document: doc, operation: op });
    let mut input = h.document(doc).unwrap().cut_settings.unwrap().input();
    input.extras = CutExtras::new()
        .with_flag(CutField::TabsEnabled, true)
        .with_text(CutField::TabsValue, "2")
        .with_text(CutField::TabsWidth, "6")
        .with_text(CutField::TabsPower, "25");
    h.send(UiIntent::ApplyCutSettings { document: doc, operation: op, settings: input });
    assert!(h.document(doc).unwrap().cut_settings.is_none(), "applied");
    h.invoke(doc, CommandKey::Compile);
    h.run_tasks();
    (h, doc)
}

fn near(image: &burnburn_macos::panels_image::BitmapImage, at: [f64; 2], want: [u8; 3]) -> bool {
    let (cx, cy) = (at[0].round() as i64, at[1].round() as i64);
    (-2..=2).any(|dy| {
        (-2..=2).any(|dx| {
            let (x, y) = (cx + dx, cy + dy);
            if x < 0 || y < 0 || x >= i64::from(image.width) || y >= i64::from(image.height) {
                return false;
            }
            let i = (y as usize * image.width as usize + x as usize) * 4;
            (0..3).all(|c| (i32::from(image.rgba[i + c]) - i32::from(want[c])).abs() <= 40)
        })
    })
}

#[test]
fn the_preview_draws_tab_stretches_orange_and_the_pick_names_the_power() {
    let (mut h, doc) = tabbed_preview();
    let view = h.controller().preview_view(doc).unwrap();
    let frame = PreviewFrame::fit(view.bed_mm.unwrap(), [760, 456], 8);
    let moves = &view.preview.moves;
    let tab = moves.iter().position(|m| m.power.is_some()).expect("a tab stretch");
    let plain = moves.iter().position(|m| m.kind == burnburn_presentation::MoveKind::Cut && m.power.is_none()).unwrap();
    let mid = |i: usize| {
        let m = moves[i];
        frame.to_px([(m.from.x_mm() + m.to.x_mm()) / 2.0, (m.from.y_mm() + m.to.y_mm()) / 2.0])
    };
    let (red, orange) = ([220, 40, 40], [235, 140, 20]);
    let image = render_preview(&view, &frame);
    assert!(near(&image, mid(tab), orange), "a tab at lowered power is orange");
    assert!(near(&image, mid(plain), red) && !near(&image, mid(plain), orange));

    // The pick names the power, in percent of the rated power: 25 % of the layer's power.
    h.send(UiIntent::App(AppIntent::Preview(PreviewIntent::SelectMove { document: doc, index: tab })));
    let view = h.controller().preview_view(doc).unwrap();
    let power = view.selected.unwrap().power.unwrap().get() * 100.0;
    let picked = preview_panel_named(&view, &PickNames::default(), Locale::En).picked.unwrap();
    assert!(picked.contains("cut") && picked.contains("power") && picked.contains(&format!("{power:.1}")), "{picked}");
    let legend = preview_panel_named(&view, &PickNames::default(), Locale::Nl).legend;
    assert!(legend.contains("oranje"), "{legend}");
    // A plain stretch says nothing about power.
    h.send(UiIntent::App(AppIntent::Preview(PreviewIntent::SelectMove { document: doc, index: plain })));
    let picked = preview_panel_named(&h.controller().preview_view(doc).unwrap(), &PickNames::default(), Locale::En)
        .picked
        .unwrap();
    assert!(!picked.contains("power"), "{picked}");
}

// ---- the nest panel: copies, holes and tight packing ------------------------------------------

mod nest_panel {
    use burnburn_macos::adapter::{NativeEvent, Reaction, translate};
    use burnburn_macos::l10n::Locale;
    use burnburn_macos::panels::{SchikkenControl as C, SchikkenLine, schikken_panel};
    use burnburn_presentation::{CommandKey, HeadlessEditorHarness, NestHoles};
    use burnburn_types::DocumentId;

    fn window() -> (HeadlessEditorHarness, DocumentId) {
        let mut h = HeadlessEditorHarness::new();
        let doc = h.new_document().unwrap();
        h.invoke(doc, CommandKey::ToolRectangle);
        h.drag(doc, (0.0, 0.0), (20.0, 10.0));
        h.drag(doc, (40.0, 0.0), (60.0, 10.0));
        h.invoke(doc, CommandKey::ToolSelect);
        h.invoke(doc, CommandKey::SelectAll);
        h.invoke(doc, CommandKey::QuickNest);
        (h, doc)
    }

    fn control(h: &HeadlessEditorHarness, doc: DocumentId, c: C) -> Option<SchikkenLine> {
        schikken_panel(&h.view(), doc, Locale::En).unwrap().lines.into_iter().find(|l| l.control() == Some(c))
    }

    fn press(h: &mut HeadlessEditorHarness, doc: DocumentId, c: C, text: &str) {
        let event = NativeEvent::SchikkenControl { document: doc, control: c, values: vec![(c, text.to_owned())] };
        let Reaction::Intent(i) = translate(event, &h.view()) else { panic!("{c:?} sends an intent") };
        h.send(i);
    }

    #[test]
    fn the_panel_has_a_copies_field_a_row_per_part_and_the_two_checks() {
        let (mut h, doc) = window();
        let Some(SchikkenLine::Field { text, caption, .. }) = control(&h, doc, C::NestCopies) else { panic!("copies") };
        assert_eq!((text.as_str(), caption.as_str()), ("1", "Copies per part"));
        let Some(SchikkenLine::Field { caption, text, .. }) = control(&h, doc, C::NestPartCopies(1)) else {
            panic!("a row for the second part")
        };
        assert!(caption.starts_with("Part 2:"), "{caption}");
        assert_eq!(text, "1");
        assert!(control(&h, doc, C::NestPartCopies(2)).is_none());
        let Some(SchikkenLine::Check { on, .. }) = control(&h, doc, C::NestHoles) else { panic!("holes") };
        assert!(on, "an inner contour is a hole of its part by default");
        let Some(SchikkenLine::Check { on, .. }) = control(&h, doc, C::NestTight) else { panic!("tight") };
        assert!(!on);

        press(&mut h, doc, C::NestCopies, "3");
        press(&mut h, doc, C::NestPartCopies(1), "5");
        press(&mut h, doc, C::NestTight, "1");
        press(&mut h, doc, C::NestHoles, "0");
        let input = h.document(doc).unwrap().schikken.nest.unwrap().input;
        assert_eq!(input.copies, "3");
        assert_eq!(input.part_copies.len(), 1);
        assert_eq!(input.part_copies[0].1, "5");
        assert!(input.tight);
        assert_eq!(input.holes, NestHoles::Separate);
        let Some(SchikkenLine::Field { text, .. }) = control(&h, doc, C::NestPartCopies(0)) else { panic!("row 1") };
        assert_eq!(text, "3", "a part without a number of its own shows the one of the panel");
        let Some(SchikkenLine::Field { text, .. }) = control(&h, doc, C::NestPartCopies(1)) else { panic!("row 2") };
        assert_eq!(text, "5");
        // The number of the panel again: the part follows the panel.
        press(&mut h, doc, C::NestPartCopies(1), "3");
        assert!(h.document(doc).unwrap().schikken.nest.unwrap().input.part_copies.is_empty());
    }

    #[test]
    fn objects_with_the_same_name_are_told_apart_in_the_choice_of_a_sheet() {
        // Copies carry the name of their source; a pop-up cannot hold two items with one title
        // (AppKit merges them, and the panel then reads past the end of the pop-up).
        let (mut h, doc) = window();
        press(&mut h, doc, C::NestCopies, "2");
        press(&mut h, doc, C::NestSheets, "2");
        press(&mut h, doc, C::NestStart, "");
        h.run_tasks();
        press(&mut h, doc, C::NestApply, "");
        let Some(SchikkenLine::Choice { options, selected, .. }) = control(&h, doc, C::NestSheet) else {
            panic!("a sheet choice")
        };
        assert_eq!(options.len(), 5, "the bed and four rectangles: {options:?}");
        let mut unique = options.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), options.len(), "every title is its own: {options:?}");
        assert_eq!(selected, 0);
    }

    #[test]
    fn common_line_and_grain_are_controls_and_the_report_names_the_shared_edge() {
        let (mut h, doc) = window();
        let Some(SchikkenLine::Check { on, caption, .. }) = control(&h, doc, C::NestCommonLine) else {
            panic!("common line check")
        };
        assert!(!on);
        assert_eq!(caption, "Common-line cutting (parts touch, shared edges are cut once)");
        let Some(SchikkenLine::Field { text, .. }) = control(&h, doc, C::NestGrain) else { panic!("grain field") };
        assert_eq!(text, "");

        press(&mut h, doc, C::NestGrain, "90");
        press(&mut h, doc, C::NestCommonLine, "1");
        let input = h.document(doc).unwrap().schikken.nest.unwrap().input;
        assert_eq!((input.grain.as_str(), input.common_line), ("90", true));
        press(&mut h, doc, C::NestStart, "");
        h.run_tasks();
        let texts: Vec<String> = schikken_panel(&h.view(), doc, Locale::En)
            .unwrap()
            .lines
            .into_iter()
            .filter_map(|l| if let SchikkenLine::Text(t) = l { Some(t) } else { None })
            .collect();
        assert!(texts.iter().any(|t| t.starts_with("Shared edge: ") && t.contains("mm")), "{texts:?}");
        press(&mut h, doc, C::NestGrain, "abc");
        let nl = schikken_panel(&h.view(), doc, Locale::Nl).unwrap().lines;
        assert!(nl.iter().any(|l| matches!(l, SchikkenLine::Problem(t) if t.contains("nerfrichting"))));
    }

    #[test]
    fn a_wrong_number_is_marked_on_its_field_and_said_in_words() {
        let (mut h, doc) = window();
        press(&mut h, doc, C::NestCopies, "0");
        let Some(SchikkenLine::Field { invalid, .. }) = control(&h, doc, C::NestCopies) else { panic!("copies") };
        assert!(invalid);
        let lines = schikken_panel(&h.view(), doc, Locale::Nl).unwrap().lines;
        assert!(
            lines
                .iter()
                .any(|l| matches!(l, SchikkenLine::Problem(t) if t.contains("geheel getal van 1 tot en met 1000")))
        );
    }
}
