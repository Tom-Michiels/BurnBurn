//! Cut Settings at LightBurn level (W2-L1-cut, C03–C05): Line tabs/leads/overcut/perforation/dot,
//! Fill scan options and Image modes as raw field text (DR-05) through the real controller.
//! Every accepted change is one document command with one undo step; an unchanged value sends no
//! command; what the machine profile does not support is visible with its reason and never
//! replaced by another value.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_test_support::host::rig as common;

use burnburn_document::UndoLabel;
use burnburn_document::process::{
    ImageRender, ImageSettings, OperationDraft, OperationMode, Passes, PowerPercent, PowerRange, ProcessLabel,
    ProcessSettings, Speed,
};
use burnburn_job::{Feature, MachineCapabilities, Support};
use burnburn_presentation::{
    CommandKey, CutChoice, CutExtras, CutField, CutSection, CutSettingsViewModel, DocumentViewModel, FieldState,
    HeadlessEditorHarness, MessageKey, OperationKind, SelectMode, UiIntent,
};
use burnburn_types::{ContentHash, DocumentId, GeometryId, HashDomain, Length, OperationId};

use common::{RUIDA, profile};

/// A document with a 40 × 20 rectangle in layer colour 1 (a Line operation): perimeter 120 mm.
fn line_layer(h: &mut HeadlessEditorHarness) -> (DocumentId, OperationId) {
    let doc = h.new_document().expect("window");
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (0.0, 0.0), (40.0, 20.0));
    h.invoke(doc, CommandKey::ToolSelect);
    let id: GeometryId = h.document(doc).unwrap().objects[0].id;
    h.send(UiIntent::Select { document: doc, ids: vec![id], mode: SelectMode::Replace });
    h.send(UiIntent::AssignLayer { document: doc, slot: 1, add: false });
    let op = h.document(doc).unwrap().operations[0].id;
    (doc, op)
}

fn vm(h: &HeadlessEditorHarness, doc: DocumentId) -> DocumentViewModel {
    h.document(doc).unwrap()
}

fn rev(h: &HeadlessEditorHarness, doc: DocumentId) -> u64 {
    vm(h, doc).revision.raw()
}

fn open(h: &mut HeadlessEditorHarness, doc: DocumentId, op: OperationId) -> CutSettingsViewModel {
    h.send(UiIntent::OpenCutSettings { document: doc, operation: op });
    vm(h, doc).cut_settings.expect("sheet open")
}

/// OK with `change` applied to what the sheet shows.
fn ok(
    h: &mut HeadlessEditorHarness,
    doc: DocumentId,
    op: OperationId,
    change: impl FnOnce(&mut burnburn_presentation::CutSettingsInput),
) {
    let sheet = vm(h, doc).cut_settings.expect("sheet open");
    let mut input = sheet.input();
    change(&mut input);
    h.send(UiIntent::ApplyCutSettings { document: doc, operation: op, settings: input });
}

/// Open, change extras, press OK; returns the reopened sheet when it closed, else the open one.
fn edit(
    h: &mut HeadlessEditorHarness,
    doc: DocumentId,
    op: OperationId,
    change: impl FnOnce(&mut CutExtras),
) -> CutSettingsViewModel {
    open(h, doc, op);
    ok(h, doc, op, |i| change(&mut i.extras));
    match vm(h, doc).cut_settings {
        Some(still_open) => still_open,
        None => open(h, doc, op),
    }
}

fn close(h: &mut HeadlessEditorHarness, doc: DocumentId) {
    h.send(UiIntent::CancelCutSettings { document: doc });
}

/// The sheet stays open with this error and nothing was committed.
fn refused(
    h: &mut HeadlessEditorHarness,
    doc: DocumentId,
    op: OperationId,
    change: impl FnOnce(&mut CutExtras),
) -> MessageKey {
    let before = rev(h, doc);
    let sheet = edit(h, doc, op, change);
    assert_eq!(rev(h, doc), before, "refused input sends no command");
    let error = sheet.error.expect("the sheet stays open with its error");
    close(h, doc);
    error
}

fn with_profile(h: &mut HeadlessEditorHarness, caps: MachineCapabilities) {
    let p = profile(1).with_capabilities(caps).unwrap();
    h.send(UiIntent::SetMachineProfile { profile: Box::new(p), ruida: RUIDA });
}

fn cut_length(h: &mut HeadlessEditorHarness, doc: DocumentId) -> f64 {
    h.invoke(doc, CommandKey::Compile);
    h.run_tasks();
    vm(h, doc).plan.expect("compiled plan").preview.cut_length_mm
}

fn supported(features: &[Feature]) -> MachineCapabilities {
    features.iter().fold(MachineCapabilities::conservative(), |c, f| c.with(*f, Support::Supported))
}

fn undo_label(h: &HeadlessEditorHarness, doc: DocumentId) -> Option<UndoLabel> {
    vm(h, doc).undo
}

#[test]
fn a_line_sheet_lists_every_line_option_off_and_no_fill_or_image_field() {
    let mut h = HeadlessEditorHarness::new();
    let (doc, op) = line_layer(&mut h);
    let s = open(&mut h, doc, op);
    for f in CutField::ALL {
        assert_eq!(
            s.extras.get(f).is_some(),
            f.section() != CutSection::Image,
            "{f:?}: a non-image sheet lists the Line and Fill fields (the Fill ones as defaults) and no image field"
        );
        if f.section() == CutSection::Fill {
            assert_eq!(
                s.extras.state(f, OperationKind::Line, &s.gates),
                FieldState::Hidden,
                "{f:?} is not part of Line"
            );
        }
    }
    for f in [
        CutField::TabsEnabled,
        CutField::LeadInEnabled,
        CutField::LeadOutEnabled,
        CutField::PerforationEnabled,
        CutField::DotEnabled,
    ] {
        assert!(!s.extras.flag(f), "{f:?} starts off");
    }
    assert_eq!(s.extras.text(CutField::Overcut), "0");
    assert_eq!(s.extras.choice(CutField::TabsMode), Some(CutChoice::TabsByCount));
    assert_eq!(s.extras.choice(CutField::LeadInShape), Some(CutChoice::LeadLine));
}

#[test]
fn tabs_by_count_and_by_spacing_are_one_command_each_and_cut_the_bridges_out() {
    let mut h = HeadlessEditorHarness::new();
    let (doc, op) = line_layer(&mut h);
    h.send(UiIntent::SetMachineProfile { profile: Box::new(profile(1)), ruida: RUIDA });
    let full = cut_length(&mut h, doc);
    assert!((full - 120.0).abs() < 1e-6, "the 40 × 20 rectangle is 120 mm of cut, got {full}");

    let r0 = rev(&h, doc);
    let s = edit(&mut h, doc, op, |e| {
        e.set_flag(CutField::TabsEnabled, true);
        e.set_text(CutField::TabsValue, "4");
        e.set_text(CutField::TabsWidth, "0,5 mm");
    });
    assert_eq!(rev(&h, doc), r0 + 1, "tabs are one command");
    assert_eq!(undo_label(&h, doc), Some(UndoLabel::Process(ProcessLabel::SetAdvanced)));
    assert!(s.extras.flag(CutField::TabsEnabled));
    assert_eq!(s.extras.choice(CutField::TabsMode), Some(CutChoice::TabsByCount));
    assert_eq!((s.extras.text(CutField::TabsValue), s.extras.text(CutField::TabsWidth)), ("4", "0.5"));
    let with_tabs = cut_length(&mut h, doc);
    assert!((with_tabs - 118.0).abs() < 1e-6, "4 tabs of 0.5 mm leave 120 − 2 = 118 mm, got {with_tabs}");

    // The same values again: nothing.
    edit(&mut h, doc, op, |_| {});
    assert_eq!(rev(&h, doc), r0 + 1, "unchanged: no command");

    // One tab per 20 mm of contour length: 6 tabs of 2 mm.
    let s = edit(&mut h, doc, op, |e| {
        e.set_choice(CutField::TabsMode, CutChoice::TabsBySpacing);
        e.set_text(CutField::TabsValue, "20");
        e.set_text(CutField::TabsWidth, "2");
    });
    assert_eq!(rev(&h, doc), r0 + 2);
    assert_eq!(s.extras.choice(CutField::TabsMode), Some(CutChoice::TabsBySpacing));
    assert_eq!(s.extras.text(CutField::TabsValue), "20");
    let spaced = cut_length(&mut h, doc);
    assert!((spaced - 108.0).abs() < 1e-6, "6 tabs of 2 mm leave 108 mm, got {spaced}");

    close(&mut h, doc);
    h.invoke(doc, CommandKey::Undo);
    let s = open(&mut h, doc, op);
    assert_eq!(s.extras.choice(CutField::TabsMode), Some(CutChoice::TabsByCount), "one undo = one change");
    assert_eq!(s.extras.text(CutField::TabsValue), "4");

    // Switching the option off removes it in one command.
    let r = rev(&h, doc);
    let s = edit(&mut h, doc, op, |e| e.set_flag(CutField::TabsEnabled, false));
    assert_eq!((rev(&h, doc), s.extras.flag(CutField::TabsEnabled)), (r + 1, false));
    let plain = cut_length(&mut h, doc);
    assert!((plain - 120.0).abs() < 1e-6);
}

#[test]
fn leads_overcut_and_perforation_show_their_stored_values_and_change_the_cut() {
    let mut h = HeadlessEditorHarness::new();
    let (doc, op) = line_layer(&mut h);
    h.send(UiIntent::SetMachineProfile { profile: Box::new(profile(1)), ruida: RUIDA });

    let r0 = rev(&h, doc);
    let s = edit(&mut h, doc, op, |e| {
        e.set_flag(CutField::LeadInEnabled, true);
        e.set_text(CutField::LeadInLength, "2");
        e.set_choice(CutField::LeadInShape, CutChoice::LeadLine);
        e.set_text(CutField::LeadInAngle, "45");
        e.set_flag(CutField::LeadOutEnabled, true);
        e.set_text(CutField::LeadOutLength, "3");
        e.set_choice(CutField::LeadOutShape, CutChoice::LeadArc);
    });
    assert_eq!(rev(&h, doc), r0 + 1, "both leads are one command");
    assert!(s.extras.flag(CutField::LeadInEnabled) && s.extras.flag(CutField::LeadOutEnabled));
    assert_eq!(s.extras.text(CutField::LeadInAngle), "45");
    assert_eq!(s.extras.choice(CutField::LeadOutShape), Some(CutChoice::LeadArc));
    assert_eq!(s.extras.text(CutField::LeadOutLength), "3");
    // Lead-out: a quarter circle of radius 3 (3π/2); lead-in: 2 mm.
    let leads = cut_length(&mut h, doc);
    assert!((leads - (120.0 + 2.0 + 3.0 * std::f64::consts::FRAC_PI_2)).abs() < 0.05, "got {leads}");

    let s = edit(&mut h, doc, op, |e| {
        e.set_flag(CutField::LeadInEnabled, false);
        e.set_flag(CutField::LeadOutEnabled, false);
        e.set_text(CutField::Overcut, "3 mm");
    });
    assert_eq!(rev(&h, doc), r0 + 2);
    assert_eq!(s.extras.text(CutField::Overcut), "3");
    let overcut = cut_length(&mut h, doc);
    assert!((overcut - 123.0).abs() < 1e-6, "overcut adds 3 mm, got {overcut}");

    let s = edit(&mut h, doc, op, |e| {
        e.set_text(CutField::Overcut, "0");
        e.set_flag(CutField::PerforationEnabled, true);
        e.set_text(CutField::PerforationOn, "3");
        e.set_text(CutField::PerforationOff, "2");
    });
    assert!(s.extras.flag(CutField::PerforationEnabled));
    assert_eq!((s.extras.text(CutField::PerforationOn), s.extras.text(CutField::PerforationOff)), ("3", "2"));
    let perforated = cut_length(&mut h, doc);
    assert!((perforated - 72.0).abs() < 1e-6, "3 on / 2 off keeps 60 % of 120 mm, got {perforated}");
}

#[test]
fn dot_mode_excludes_the_continuous_line_options_and_greys_them_with_a_reason() {
    let mut h = HeadlessEditorHarness::new();
    let (doc, op) = line_layer(&mut h);
    with_profile(&mut h, supported(&[Feature::DotMode]));

    // Tabs and dot together are refused by the sheet, not by luck of the document.
    let r0 = rev(&h, doc);
    let error = refused(&mut h, doc, op, |e| {
        e.set_flag(CutField::TabsEnabled, true);
        e.set_flag(CutField::DotEnabled, true);
    });
    assert_eq!(error, MessageKey::CutOptionsConflict);
    assert_eq!(rev(&h, doc), r0);

    let s = edit(&mut h, doc, op, |e| {
        e.set_flag(CutField::DotEnabled, true);
        e.set_text(CutField::DotSpacing, "5");
        e.set_text(CutField::DotDwell, "12,5");
    });
    assert_eq!(rev(&h, doc), r0 + 1);
    assert!(s.extras.flag(CutField::DotEnabled));
    assert_eq!((s.extras.text(CutField::DotSpacing), s.extras.text(CutField::DotDwell)), ("5", "12.5"));

    // The live values decide which fields are greyed.
    let kind = OperationKind::Line;
    for f in
        [CutField::TabsEnabled, CutField::TabsValue, CutField::LeadInLength, CutField::Overcut, CutField::PerforationOn]
    {
        assert_eq!(
            s.extras.state(f, kind, &s.gates),
            FieldState::Disabled(Some(MessageKey::CutOptionsConflict)),
            "{f:?}"
        );
    }
    assert_eq!(s.extras.state(CutField::DotSpacing, kind, &s.gates), FieldState::Editable);
    let mut off = s.extras.clone();
    off.set_flag(CutField::DotEnabled, false);
    assert_eq!(off.state(CutField::TabsEnabled, kind, &s.gates), FieldState::Editable);
    assert_eq!(
        off.state(CutField::TabsValue, kind, &s.gates),
        FieldState::Disabled(None),
        "off until its switch is on"
    );
    assert_eq!(off.state(CutField::DotSpacing, kind, &s.gates), FieldState::Disabled(None));
    assert_eq!(off.state(CutField::FillAngle, kind, &s.gates), FieldState::Hidden);

    // Turning dot off and tabs on in one OK is fine.
    let s = edit(&mut h, doc, op, |e| {
        e.set_flag(CutField::DotEnabled, false);
        e.set_flag(CutField::TabsEnabled, true);
    });
    assert!(s.extras.flag(CutField::TabsEnabled) && !s.extras.flag(CutField::DotEnabled));
}

#[test]
fn invalid_line_option_text_keeps_the_sheet_open_with_the_field_named() {
    let mut h = HeadlessEditorHarness::new();
    let (doc, op) = line_layer(&mut h);
    with_profile(&mut h, supported(&[Feature::DotMode]));
    let cases: [(CutField, &str, &[(CutField, &str)]); 14] = [
        (CutField::TabsValue, "abc", &[]),
        (CutField::TabsValue, "0", &[]),
        (CutField::TabsValue, "65", &[]),
        (CutField::TabsValue, "2,5", &[]),
        (CutField::TabsWidth, "0", &[]),
        (CutField::TabsWidth, "-1", &[]),
        (CutField::LeadInLength, "-1", &[]),
        (CutField::LeadInAngle, "0", &[]),
        (CutField::LeadInAngle, "91", &[]),
        (CutField::Overcut, "-1", &[]),
        (CutField::PerforationOff, "0", &[]),
        (CutField::DotSpacing, "0", &[]),
        (CutField::DotDwell, "0", &[]),
        (CutField::DotDwell, "10001", &[]),
    ];
    for (field, text, also) in cases {
        let error = refused(&mut h, doc, op, |e| {
            for sw in [CutField::TabsEnabled, CutField::LeadInEnabled, CutField::PerforationEnabled] {
                if field.switch() == Some(sw) {
                    e.set_flag(sw, true);
                }
            }
            if matches!(field, CutField::DotSpacing | CutField::DotDwell) {
                e.set_flag(CutField::DotEnabled, true);
            }
            e.set_text(field, text);
            for (f, t) in also {
                e.set_text(*f, *t);
            }
        });
        // The overcut has no switch; the dot fields are checked with Dot on.
        assert_eq!(error, MessageKey::InvalidCutField(field), "{field:?} = {text:?}");
    }
    // Tab spacing at or below the tab width cannot leave a bridge.
    let error = refused(&mut h, doc, op, |e| {
        e.set_flag(CutField::TabsEnabled, true);
        e.set_choice(CutField::TabsMode, CutChoice::TabsBySpacing);
        e.set_text(CutField::TabsValue, "2");
        e.set_text(CutField::TabsWidth, "2");
    });
    assert_eq!(error, MessageKey::InvalidCutField(CutField::TabsValue));
}

#[test]
fn mode_settings_and_line_options_in_one_ok_are_two_commands_and_leaving_line_drops_its_options() {
    let mut h = HeadlessEditorHarness::new();
    let (doc, op) = line_layer(&mut h);
    let r0 = rev(&h, doc);
    open(&mut h, doc, op);
    ok(&mut h, doc, op, |i| {
        i.speed = "250".into();
        i.extras.set_flag(CutField::TabsEnabled, true);
    });
    // The document has no combined command: UpdateOperation + SetAdvanced (two undo steps).
    assert_eq!(rev(&h, doc), r0 + 2);

    // Line options are only legal on a Line operation, so Fill takes them away with it.
    let r = rev(&h, doc);
    open(&mut h, doc, op);
    ok(&mut h, doc, op, |i| i.mode = OperationKind::Fill);
    assert_eq!(rev(&h, doc), r + 2);
    let s = open(&mut h, doc, op);
    assert_eq!(s.mode, OperationKind::Fill);
    assert!(!s.extras.flag(CutField::TabsEnabled), "the Fill sheet's Line options are the defaults: off");
    assert_eq!(s.extras.state(CutField::TabsEnabled, OperationKind::Fill, &s.gates), FieldState::Hidden);
    close(&mut h, doc);
    ok_mode(&mut h, doc, op, OperationKind::Line);
    let s = open(&mut h, doc, op);
    assert!(!s.extras.flag(CutField::TabsEnabled), "the tabs did not come back");
}

fn ok_mode(h: &mut HeadlessEditorHarness, doc: DocumentId, op: OperationId, mode: OperationKind) {
    open(h, doc, op);
    ok(h, doc, op, |i| i.mode = mode);
}

#[test]
fn fill_options_are_read_and_written_as_one_update_with_unchanged_input_sending_nothing() {
    let mut h = HeadlessEditorHarness::new();
    let (doc, op) = line_layer(&mut h);
    with_profile(&mut h, supported(&[Feature::RampLength, Feature::ScanAngle]));
    ok_mode(&mut h, doc, op, OperationKind::Fill);
    let s = open(&mut h, doc, op);
    assert_eq!(s.extras.text(CutField::FillInterval), "0.1");
    assert_eq!(s.extras.text(CutField::FillAngle), "0");
    assert!(s.extras.flag(CutField::FillBidirectional));
    assert!(!s.extras.flag(CutField::FillCrossHatch) && !s.extras.flag(CutField::FillFloodFill));
    assert_eq!(s.extras.choice(CutField::FillGrouping), Some(CutChoice::GroupAllShapes));
    assert_eq!(s.extras.text(CutField::FillRamp), "0");
    assert_eq!(s.extras.state(CutField::TabsEnabled, OperationKind::Fill, &s.gates), FieldState::Hidden);
    close(&mut h, doc);

    let r0 = rev(&h, doc);
    let s = edit(&mut h, doc, op, |e| {
        e.set_text(CutField::FillInterval, "0,2");
        e.set_text(CutField::FillAngle, "-30");
        e.set_flag(CutField::FillBidirectional, false);
        e.set_text(CutField::FillOverscan, "2 mm");
        e.set_flag(CutField::FillCrossHatch, true);
        e.set_flag(CutField::FillFloodFill, true);
        e.set_choice(CutField::FillGrouping, CutChoice::GroupIndividual);
        e.set_text(CutField::FillRamp, "1,5");
    });
    assert_eq!(rev(&h, doc), r0 + 1, "all fill options in one command");
    assert_eq!(undo_label(&h, doc), Some(UndoLabel::Process(ProcessLabel::UpdateOperation)));
    assert_eq!(s.extras.text(CutField::FillInterval), "0.2");
    assert_eq!(s.extras.text(CutField::FillAngle), "150", "−30° is normalised to [0, 180)");
    assert!(!s.extras.flag(CutField::FillBidirectional));
    assert_eq!(s.extras.text(CutField::FillOverscan), "2");
    assert!(s.extras.flag(CutField::FillCrossHatch) && s.extras.flag(CutField::FillFloodFill));
    assert_eq!(s.extras.choice(CutField::FillGrouping), Some(CutChoice::GroupIndividual));
    assert_eq!(s.extras.text(CutField::FillRamp), "1.5");

    edit(&mut h, doc, op, |_| {});
    assert_eq!(rev(&h, doc), r0 + 1, "same values: no command");
    // Equal after normalisation is equal: 150° typed again is not a change.
    edit(&mut h, doc, op, |e| e.set_text(CutField::FillAngle, "150°"));
    assert_eq!(rev(&h, doc), r0 + 1);

    close(&mut h, doc);
    h.invoke(doc, CommandKey::Undo);
    let s = open(&mut h, doc, op);
    assert_eq!(s.extras.text(CutField::FillInterval), "0.1", "one undo restores every fill option");
    assert!(!s.extras.flag(CutField::FillCrossHatch));
}

#[test]
fn invalid_fill_option_text_is_refused_with_the_field_named() {
    let mut h = HeadlessEditorHarness::new();
    let (doc, op) = line_layer(&mut h);
    ok_mode(&mut h, doc, op, OperationKind::Fill);
    close(&mut h, doc);
    for (field, text) in [
        (CutField::FillInterval, "0"),
        (CutField::FillInterval, "snel"),
        (CutField::FillAngle, "abc"),
        (CutField::FillOverscan, "-1"),
        (CutField::FillRamp, "-0,5"),
    ] {
        let error = refused(&mut h, doc, op, |e| e.set_text(field, text));
        assert_eq!(error, MessageKey::InvalidCutField(field), "{field:?} = {text:?}");
    }
}

#[test]
fn fill_options_the_profile_does_not_support_are_visible_greyed_and_never_replaced() {
    let mut h = HeadlessEditorHarness::new();
    let (doc, op) = line_layer(&mut h);
    ok_mode(&mut h, doc, op, OperationKind::Fill);
    // No profile: the conservative matrix. Ramp and scan angle are unverified, cross-hatch is fine.
    let s = open(&mut h, doc, op);
    assert_eq!(
        s.gates.blocked.get(&CutField::FillRamp),
        Some(&MessageKey::MissingCapability(Feature::RampLength)),
        "visible with its reason"
    );
    assert_eq!(s.gates.advisory.get(&CutField::FillAngle), Some(&MessageKey::MissingCapability(Feature::ScanAngle)));
    assert!(!s.gates.blocked.contains_key(&CutField::FillCrossHatch));
    assert_eq!(
        s.extras.state(CutField::FillRamp, OperationKind::Fill, &s.gates),
        FieldState::Disabled(Some(MessageKey::MissingCapability(Feature::RampLength)))
    );
    assert_eq!(s.extras.state(CutField::FillAngle, OperationKind::Fill, &s.gates), FieldState::Editable);
    close(&mut h, doc);

    let r0 = rev(&h, doc);
    let e = refused(&mut h, doc, op, |e| e.set_text(CutField::FillRamp, "2"));
    assert_eq!(e, MessageKey::MissingCapability(Feature::RampLength));
    let e = refused(&mut h, doc, op, |e| e.set_text(CutField::FillAngle, "45"));
    assert_eq!(e, MessageKey::MissingCapability(Feature::ScanAngle));
    assert_eq!(rev(&h, doc), r0);
    // 0° and 90° need no capability.
    let s = edit(&mut h, doc, op, |e| e.set_text(CutField::FillAngle, "90"));
    assert_eq!((rev(&h, doc), s.extras.text(CutField::FillAngle)), (r0 + 1, "90"));
    close(&mut h, doc);

    // A profile that lacks cross-hatch blocks that checkbox.
    with_profile(&mut h, MachineCapabilities::conservative().with(Feature::CrossHatch, Support::Unsupported));
    let s = open(&mut h, doc, op);
    assert_eq!(
        s.gates.blocked.get(&CutField::FillCrossHatch),
        Some(&MessageKey::MissingCapability(Feature::CrossHatch))
    );
    close(&mut h, doc);
    let e = refused(&mut h, doc, op, |e| e.set_flag(CutField::FillCrossHatch, true));
    assert_eq!(e, MessageKey::MissingCapability(Feature::CrossHatch));
}

#[test]
fn a_stored_option_the_profile_lacks_stays_and_does_not_block_unrelated_edits() {
    let mut h = HeadlessEditorHarness::new();
    let (doc, op) = line_layer(&mut h);
    edit(&mut h, doc, op, |e| e.set_flag(CutField::TabsEnabled, true));
    close(&mut h, doc);
    with_profile(&mut h, MachineCapabilities::conservative().with(Feature::Tabs, Support::Unsupported));
    let s = open(&mut h, doc, op);
    assert!(s.extras.flag(CutField::TabsEnabled), "the stored tabs are shown, not hidden");
    assert!(s.gates.blocked.contains_key(&CutField::TabsEnabled));
    let r = rev(&h, doc);
    ok(&mut h, doc, op, |i| i.speed = "80".into());
    assert_eq!(rev(&h, doc), r + 1, "an unrelated edit is accepted");
    let s = open(&mut h, doc, op);
    assert!(s.extras.flag(CutField::TabsEnabled), "and the tabs are still there");
}

fn image_layer(h: &mut HeadlessEditorHarness) -> (DocumentId, OperationId) {
    let (doc, _) = line_layer(h);
    let source = ContentHash::of(HashDomain::SourcePixels, b"pixels");
    let pitch = Length::from_mm(0.1).unwrap();
    let image = ImageSettings::new(
        source,
        pitch,
        pitch,
        ImageRender::Dither(burnburn_document::process::DitherMode::Threshold { level: 128 }),
    )
    .unwrap();
    let p = PowerPercent::new(30.0).unwrap();
    let draft = OperationDraft {
        name: "Foto".into(),
        mode: OperationMode::Image(image),
        settings: ProcessSettings {
            speed: Speed::mm_per_s(200.0).unwrap(),
            power: PowerRange::new(p, p).unwrap(),
            passes: Passes::new(1).unwrap(),
            air_assist: true,
        },
        output_enabled: true,
    };
    h.send(UiIntent::AddOperation { document: doc, draft });
    let op = vm(h, doc).operations.iter().find(|o| o.name == "Foto").expect("image layer").id;
    (doc, op)
}

#[test]
fn an_image_sheet_offers_only_the_image_mode_and_lists_all_eleven_render_modes() {
    let mut h = HeadlessEditorHarness::new();
    let (doc, op) = image_layer(&mut h);
    assert_eq!(vm(&h, doc).operations.iter().find(|o| o.id == op).unwrap().kind, OperationKind::Image);
    let s = open(&mut h, doc, op);
    assert_eq!(s.mode, OperationKind::Image);
    assert_eq!(s.modes.iter().map(|m| m.kind).collect::<Vec<_>>(), vec![OperationKind::Image]);
    assert_eq!(
        CutField::ImageMode.choices(),
        [
            CutChoice::Threshold,
            CutChoice::Ordered,
            CutChoice::Atkinson,
            CutChoice::Dither,
            CutChoice::Stucki,
            CutChoice::Jarvis,
            CutChoice::Newsprint,
            CutChoice::Halftone,
            CutChoice::Sketch,
            CutChoice::Grayscale,
            CutChoice::PassThrough,
        ]
    );
    assert_eq!(s.extras.choice(CutField::ImageMode), Some(CutChoice::Threshold));
    assert_eq!(s.extras.text(CutField::ImageLevel), "128");
    assert_eq!(s.extras.text(CutField::ImageInterval), "0.1");
    assert!(!s.extras.flag(CutField::ImageNegative) && s.extras.flag(CutField::ImageBidirectional));
    assert!(s.extras.get(CutField::TabsEnabled).is_none() && s.extras.get(CutField::FillAngle).is_none());
    // Level belongs to Threshold/Sketch; the matrix to Ordered; the screen to Halftone/Newsprint.
    let kind = OperationKind::Image;
    assert!(CutField::ImageLevel.applies(kind, &s.extras));
    assert!(!CutField::ImageMatrix.applies(kind, &s.extras) && !CutField::ScreenCell.applies(kind, &s.extras));
    let halftone = s.extras.clone().with_choice(CutField::ImageMode, CutChoice::Halftone);
    assert!(CutField::ScreenCell.applies(kind, &halftone) && CutField::ScreenAngle.applies(kind, &halftone));
    let newsprint = s.extras.clone().with_choice(CutField::ImageMode, CutChoice::Newsprint);
    assert!(CutField::ScreenCell.applies(kind, &newsprint) && !CutField::ScreenAngle.applies(kind, &newsprint));
    let pass = s.extras.clone().with_choice(CutField::ImageMode, CutChoice::PassThrough);
    assert!(CutField::ImageAngle.applies(kind, &pass), "the angle stays editable so it can be set back to 0");
}

#[test]
fn every_image_mode_is_one_command_and_comes_back_in_the_sheet() {
    let mut h = HeadlessEditorHarness::new();
    let (doc, op) = image_layer(&mut h);
    with_profile(&mut h, supported(&[Feature::GrayscaleModulation]));
    let mut expected = rev(&h, doc);
    for mode in CutField::ImageMode.choices().iter().copied().filter(|m| *m != CutChoice::Threshold) {
        let s = edit(&mut h, doc, op, |e| e.set_choice(CutField::ImageMode, mode));
        expected += 1;
        assert_eq!(rev(&h, doc), expected, "{mode:?}: one command");
        assert_eq!(s.extras.choice(CutField::ImageMode), Some(mode), "{mode:?} is stored");
        close(&mut h, doc);
    }
    let s = edit(&mut h, doc, op, |e| {
        e.set_choice(CutField::ImageMode, CutChoice::Ordered);
        e.set_text(CutField::ImageMatrix, "4");
    });
    assert_eq!(s.extras.text(CutField::ImageMatrix), "4");
    let s = edit(&mut h, doc, op, |e| {
        e.set_choice(CutField::ImageMode, CutChoice::Sketch);
        e.set_text(CutField::ImageLevel, "200");
    });
    assert_eq!(
        (s.extras.choice(CutField::ImageMode), s.extras.text(CutField::ImageLevel)),
        (Some(CutChoice::Sketch), "200")
    );
    close(&mut h, doc);
    let r = rev(&h, doc);
    edit(&mut h, doc, op, |e| e.set_text(CutField::ImageLevel, "200"));
    assert_eq!(rev(&h, doc), r, "unchanged: no command");
}

#[test]
fn image_options_negative_dot_width_angle_interval_direction_overscan_and_screen() {
    let mut h = HeadlessEditorHarness::new();
    let (doc, op) = image_layer(&mut h);
    with_profile(&mut h, supported(&[Feature::ScanAngle]));
    let r0 = rev(&h, doc);
    let s = edit(&mut h, doc, op, |e| {
        e.set_choice(CutField::ImageMode, CutChoice::Halftone);
        e.set_flag(CutField::ImageNegative, true);
        e.set_text(CutField::ImageDotWidth, "0,05");
        e.set_text(CutField::ImageAngle, "30");
        e.set_text(CutField::ImageInterval, "254 dpi");
        e.set_text(CutField::ImageDotPitch, "0,2 mm");
        e.set_flag(CutField::ImageBidirectional, false);
        e.set_text(CutField::ImageOverscan, "3");
        e.set_text(CutField::ScreenCell, "0,4");
        e.set_text(CutField::ScreenAngle, "15");
    });
    assert_eq!(rev(&h, doc), r0 + 1, "all image options are one command");
    assert_eq!(undo_label(&h, doc), Some(UndoLabel::Process(ProcessLabel::UpdateOperation)));
    assert!(s.extras.flag(CutField::ImageNegative) && !s.extras.flag(CutField::ImageBidirectional));
    assert_eq!(s.extras.text(CutField::ImageDotWidth), "0.05");
    assert_eq!(s.extras.text(CutField::ImageAngle), "30");
    assert_eq!(s.extras.text(CutField::ImageInterval), "0.1", "254 dpi is 0.1 mm");
    assert_eq!(s.extras.text(CutField::ImageDotPitch), "0.2");
    assert_eq!(s.extras.text(CutField::ImageOverscan), "3");
    assert_eq!((s.extras.text(CutField::ScreenCell), s.extras.text(CutField::ScreenAngle)), ("0.4", "15"));
    close(&mut h, doc);
    h.invoke(doc, CommandKey::Undo);
    let s = open(&mut h, doc, op);
    assert_eq!(s.extras.choice(CutField::ImageMode), Some(CutChoice::Threshold), "one undo restores the image options");
    assert!(!s.extras.flag(CutField::ImageNegative));
}

#[test]
fn invalid_image_text_and_options_that_do_not_fit_the_mode_are_refused() {
    let mut h = HeadlessEditorHarness::new();
    let (doc, op) = image_layer(&mut h);
    with_profile(&mut h, supported(&[Feature::ScanAngle]));
    for (field, text) in [
        (CutField::ImageLevel, "256"),
        (CutField::ImageLevel, "-1"),
        (CutField::ImageLevel, "12,5"),
        (CutField::ImageDotWidth, "-0,1"),
        (CutField::ImageAngle, "abc"),
        (CutField::ImageInterval, "0"),
        (CutField::ImageInterval, "0 dpi"),
        (CutField::ImageDotPitch, "snel"),
        (CutField::ImageOverscan, "-2"),
    ] {
        let error = refused(&mut h, doc, op, |e| e.set_text(field, text));
        assert_eq!(error, MessageKey::InvalidCutField(field), "{field:?} = {text:?}");
    }
    let error = refused(&mut h, doc, op, |e| {
        e.set_choice(CutField::ImageMode, CutChoice::Ordered);
        e.set_text(CutField::ImageMatrix, "3");
    });
    assert_eq!(error, MessageKey::InvalidCutField(CutField::ImageMatrix));
    let error = refused(&mut h, doc, op, |e| {
        e.set_choice(CutField::ImageMode, CutChoice::Halftone);
        e.set_text(CutField::ScreenCell, "0");
    });
    assert_eq!(error, MessageKey::InvalidCutField(CutField::ScreenCell));
    // Pass-through scans the source pixels as they are: an angle is refused, not ignored.
    let error = refused(&mut h, doc, op, |e| {
        e.set_choice(CutField::ImageMode, CutChoice::PassThrough);
        e.set_text(CutField::ImageAngle, "45");
    });
    assert_eq!(error, MessageKey::CutOptionNotForMode(CutField::ImageAngle));
    // Grayscale, too, only scans along the axes (a 90° angle is not 0°).
    with_profile(&mut h, supported(&[Feature::ScanAngle, Feature::GrayscaleModulation]));
    let error = refused(&mut h, doc, op, |e| {
        e.set_choice(CutField::ImageMode, CutChoice::Grayscale);
        e.set_text(CutField::ImageAngle, "90");
    });
    assert_eq!(error, MessageKey::CutOptionNotForMode(CutField::ImageAngle));
}

#[test]
fn grayscale_and_oblique_image_angles_need_their_capability() {
    let mut h = HeadlessEditorHarness::new();
    let (doc, op) = image_layer(&mut h);
    let s = open(&mut h, doc, op);
    assert_eq!(
        s.gates.blocked_choices.get(&(CutField::ImageMode, CutChoice::Grayscale)),
        Some(&MessageKey::MissingCapability(Feature::GrayscaleModulation)),
        "Grayscale stays in the list with its reason"
    );
    assert!(!s.gates.blocked_choices.contains_key(&(CutField::ImageMode, CutChoice::Atkinson)));
    assert_eq!(s.gates.advisory.get(&CutField::ImageAngle), Some(&MessageKey::MissingCapability(Feature::ScanAngle)));
    close(&mut h, doc);
    let r = rev(&h, doc);
    let e = refused(&mut h, doc, op, |e| e.set_choice(CutField::ImageMode, CutChoice::Grayscale));
    assert_eq!(e, MessageKey::MissingCapability(Feature::GrayscaleModulation));
    let e = refused(&mut h, doc, op, |e| e.set_text(CutField::ImageAngle, "30"));
    assert_eq!(e, MessageKey::MissingCapability(Feature::ScanAngle));
    assert_eq!(rev(&h, doc), r);
    // Without dither support no dither mode can be chosen either, and the reason says so.
    with_profile(&mut h, MachineCapabilities::conservative().with(Feature::ImageDither, Support::Unsupported));
    let s = open(&mut h, doc, op);
    assert_eq!(
        s.gates.blocked_choices.get(&(CutField::ImageMode, CutChoice::Atkinson)),
        Some(&MessageKey::MissingCapability(Feature::ImageDither))
    );
}

#[test]
fn a_mode_change_on_an_image_layer_is_refused() {
    let mut h = HeadlessEditorHarness::new();
    let (doc, op) = image_layer(&mut h);
    open(&mut h, doc, op);
    let r = rev(&h, doc);
    ok(&mut h, doc, op, |i| i.mode = OperationKind::Line);
    assert_eq!(rev(&h, doc), r, "an image keeps its mode");
    assert_eq!(vm(&h, doc).cut_settings.unwrap().error, Some(MessageKey::CommandRejected));
}
