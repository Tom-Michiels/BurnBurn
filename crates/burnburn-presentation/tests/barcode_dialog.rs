//! The barcode dialog (W3-UI-tekst, E10), headless: the type list over all 29 symbologies with
//! the unsupported ones named and reasoned, parameters that follow the type, field-level errors
//! (character index, check-digit proposal), the readability warning with its limit, and Create as
//! one path in the view. Known values: EAN-13 4006381333931 (GS1), ISBN-10 0-306-40615-2 =
//! 978-0-306-40615-7, QR levels L/M/Q/H = 7/15/25/30 %.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_document::UndoLabel;
use burnburn_generators::{SymbologyParams, encode_symbol, symbology_catalog};
use burnburn_presentation::{
    BarcodeEdit, BarcodeField, BarcodeIssue, CommandKey, DocumentViewModel, FieldKey, HeadlessEditorHarness,
    SelectMode, SymbolInputMode, SymbolKind, Symbology, UiIntent,
};
use burnburn_types::DocumentId;

fn open() -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::BarcodeNew);
    (h, doc)
}

fn edit(h: &mut HeadlessEditorHarness, doc: DocumentId, edit: BarcodeEdit) {
    h.send(UiIntent::Barcode { document: doc, edit });
}

fn vm(h: &HeadlessEditorHarness, doc: DocumentId) -> DocumentViewModel {
    h.document(doc).unwrap()
}

fn problem(h: &HeadlessEditorHarness, doc: DocumentId, field: BarcodeField) -> Option<BarcodeIssue> {
    vm(h, doc).barcode_dialog.unwrap().problems.iter().find(|p| p.field == field).map(|p| p.issue)
}

#[test]
fn the_dialog_opens_from_the_menu_command_and_cancel_closes_it() {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    assert!(vm(&h, doc).barcode_dialog.is_none());
    assert!(h.view().commands[&CommandKey::BarcodeNew].enabled);
    h.invoke(doc, CommandKey::BarcodeNew);
    let d = vm(&h, doc).barcode_dialog.expect("open");
    assert_eq!(d.symbology, Symbology::QrCode, "QR is the first choice");
    edit(&mut h, doc, BarcodeEdit::Cancel);
    assert!(vm(&h, doc).barcode_dialog.is_none());
    let rev = vm(&h, doc).revision;
    edit(&mut h, doc, BarcodeEdit::Create);
    assert_eq!(vm(&h, doc).revision, rev, "no dialog, no command");
}

#[test]
fn all_29_types_are_listed_and_the_unsupported_ones_say_why() {
    let (h, doc) = open();
    let d = vm(&h, doc).barcode_dialog.unwrap();
    assert_eq!(d.types.len(), 29);
    let unsupported: Vec<_> = d.types.iter().filter(|t| !t.supported).map(|t| t.symbology).collect();
    assert_eq!(
        unsupported,
        [Symbology::ArucoMip36h11, Symbology::GridMatrix],
        "27 of 29 are implemented (W3-robuust added 2 of 5 and Han Xin)"
    );
    for t in d.types.iter() {
        assert!(!t.name.is_empty());
        if t.supported {
            assert!(t.reason.is_none() && t.source.is_some(), "{}", t.name);
        } else {
            assert!(t.reason.is_some_and(|r| !r.is_empty()) && t.source.is_none(), "{}", t.name);
        }
    }
    // The rows are the domain's catalogue, in its order.
    let catalog: Vec<_> = symbology_catalog().into_iter().map(|e| e.symbology).collect();
    assert_eq!(d.types.iter().map(|t| t.symbology).collect::<Vec<_>>(), catalog);
}

#[test]
fn an_unsupported_type_can_be_chosen_to_read_its_reason_but_never_makes_a_pattern() {
    let (mut h, doc) = open();
    edit(&mut h, doc, BarcodeEdit::Symbology(Symbology::GridMatrix));
    edit(&mut h, doc, BarcodeEdit::Payload("123".into()));
    let d = vm(&h, doc).barcode_dialog.unwrap();
    assert!(!d.can_create && d.size.is_none() && d.canonical.is_none());
    assert_eq!(problem(&h, doc, BarcodeField::Payload), Some(BarcodeIssue::Unsupported));
    let rev = vm(&h, doc).revision;
    edit(&mut h, doc, BarcodeEdit::Create);
    assert_eq!(vm(&h, doc).revision, rev);
    assert!(vm(&h, doc).barcode_dialog.is_some(), "the dialog stays");
}

#[test]
fn the_parameters_follow_the_chosen_type() {
    let (mut h, doc) = open();
    // Long enough that the error-correction level changes the symbol's version.
    edit(&mut h, doc, BarcodeEdit::Payload("BURNBURN LASER 0123456789 ABCDEFGHIJ".into()));
    // QR: error correction L/M/Q/H, no height, no columns.
    let qr = vm(&h, doc).barcode_dialog.unwrap();
    assert_eq!(qr.kind, SymbolKind::Matrix);
    let ecc = qr.ecc.expect("QR has error correction steps");
    assert_eq!(
        ecc.steps.iter().map(|s| (s.label, s.recovery_percent)).collect::<Vec<_>>(),
        [("L", Some(7)), ("M", Some(15)), ("Q", Some(25)), ("H", Some(30))]
    );
    assert_eq!(ecc.selected, Symbology::QrCode.capabilities().default_ecc_step.unwrap());
    assert!(qr.height.is_none() && qr.columns.is_none() && qr.marker_count.is_none());

    edit(&mut h, doc, BarcodeEdit::Ecc(3));
    assert_eq!(vm(&h, doc).barcode_dialog.unwrap().ecc.unwrap().selected, 3);
    let h_size = vm(&h, doc).barcode_dialog.unwrap().size.unwrap();
    edit(&mut h, doc, BarcodeEdit::Ecc(0));
    assert_ne!(vm(&h, doc).barcode_dialog.unwrap().size.unwrap(), h_size, "level H makes a bigger symbol than L");
    edit(&mut h, doc, BarcodeEdit::Ecc(9));
    assert_eq!(problem(&h, doc, BarcodeField::Ecc), Some(BarcodeIssue::EccStepUnavailable));

    // A linear type: height, no error correction; the quiet zone minimum comes from the type.
    edit(&mut h, doc, BarcodeEdit::Symbology(Symbology::Code128));
    let linear = vm(&h, doc).barcode_dialog.unwrap();
    assert_eq!(linear.kind, SymbolKind::Linear);
    assert!(linear.height.is_some() && linear.ecc.is_none());
    assert_eq!(linear.quiet_minimum, Symbology::Code128.capabilities().min_quiet_modules);
    assert!(linear.ecc.is_none() && problem(&h, doc, BarcodeField::Ecc).is_none(), "the old ECC step is gone");

    // PDF417: columns. A marker family: the payload is the id.
    edit(&mut h, doc, BarcodeEdit::Symbology(Symbology::Pdf417));
    assert!(vm(&h, doc).barcode_dialog.unwrap().columns.is_some());
    edit(&mut h, doc, BarcodeEdit::Symbology(Symbology::AprilTag36h11));
    let marker = vm(&h, doc).barcode_dialog.unwrap();
    assert_eq!(marker.kind, SymbolKind::Marker);
    assert_eq!(marker.marker_count, Symbology::AprilTag36h11.capabilities().marker_count);
    assert_eq!(marker.payload, "0", "a marker starts at id 0");
    assert!(marker.can_create);
    edit(&mut h, doc, BarcodeEdit::Payload("999999".into()));
    assert!(matches!(problem(&h, doc, BarcodeField::Payload), Some(BarcodeIssue::MarkerId { id: 999_999, .. })));

    // Input modes come from the type: the first is the default.
    edit(&mut h, doc, BarcodeEdit::Symbology(Symbology::DataMatrix));
    let dm = vm(&h, doc).barcode_dialog.unwrap();
    assert_eq!(dm.input_modes, Symbology::DataMatrix.capabilities().input_modes);
    assert_eq!(dm.input_mode, dm.input_modes[0]);
    if let Some(other) = dm.input_modes.iter().copied().find(|m| *m != dm.input_mode) {
        edit(&mut h, doc, BarcodeEdit::InputMode(other));
        assert_eq!(vm(&h, doc).barcode_dialog.unwrap().input_mode, other);
    }
    edit(&mut h, doc, BarcodeEdit::InputMode(SymbolInputMode::Gs1));
    if !Symbology::DataMatrix.capabilities().input_modes.contains(&SymbolInputMode::Gs1) {
        assert_eq!(problem(&h, doc, BarcodeField::InputMode), Some(BarcodeIssue::InputModeUnavailable));
    }
}

#[test]
fn payload_errors_name_the_character_and_propose_the_check_digit() {
    let (mut h, doc) = open();
    edit(&mut h, doc, BarcodeEdit::Symbology(Symbology::Ean));
    edit(&mut h, doc, BarcodeEdit::Payload("400638133393".into()));
    let d = vm(&h, doc).barcode_dialog.unwrap();
    assert_eq!(d.canonical.as_deref(), Some("4006381333931"), "the check digit is computed and shown");
    assert!(d.can_create && d.problems.is_empty());

    edit(&mut h, doc, BarcodeEdit::Payload("4006381333932".into()));
    assert_eq!(
        problem(&h, doc, BarcodeField::Payload),
        Some(BarcodeIssue::CheckDigit { expected: '1', found: '2' }),
        "the proposal is the right digit"
    );
    edit(&mut h, doc, BarcodeEdit::Payload("40063A133393".into()));
    assert_eq!(problem(&h, doc, BarcodeField::Payload), Some(BarcodeIssue::InvalidChar { ch: 'A', index: 5 }));
    edit(&mut h, doc, BarcodeEdit::Payload("".into()));
    assert_eq!(problem(&h, doc, BarcodeField::Payload), Some(BarcodeIssue::EmptyPayload));
    assert!(!vm(&h, doc).barcode_dialog.unwrap().can_create);

    edit(&mut h, doc, BarcodeEdit::Symbology(Symbology::Isbn));
    edit(&mut h, doc, BarcodeEdit::Payload("0-306-40615-2".into()));
    assert_eq!(
        vm(&h, doc).barcode_dialog.unwrap().canonical.as_deref(),
        Some("9780306406157"),
        "ISBN-10 becomes ISBN-13"
    );
}

#[test]
fn a_module_below_the_minimum_warns_with_the_limit_and_is_never_rounded_up() {
    let (mut h, doc) = open();
    edit(&mut h, doc, BarcodeEdit::Payload("HELLO".into()));
    edit(&mut h, doc, BarcodeEdit::MinModule("0,2".into()));
    edit(&mut h, doc, BarcodeEdit::Module("0,5".into()));
    let ok = vm(&h, doc).barcode_dialog.unwrap();
    assert!(ok.readability.is_none() && ok.can_create);

    edit(&mut h, doc, BarcodeEdit::Module("0,1".into()));
    let d = vm(&h, doc).barcode_dialog.unwrap();
    let r = d.readability.expect("below the minimum");
    assert_eq!((r.module_mm, r.min_module_mm), (0.1, 0.2));
    assert_eq!(d.module, "0,1", "the typed module stays as it is");
    assert!(matches!(
        problem(&h, doc, BarcodeField::Module),
        Some(BarcodeIssue::ModuleTooSmall { min_mm, value_mm }) if min_mm == 0.2 && value_mm == 0.1
    ));
    assert!(!d.can_create);

    // A quiet zone below the standard minimum of the type is refused with that minimum.
    edit(&mut h, doc, BarcodeEdit::Module("0,5".into()));
    edit(&mut h, doc, BarcodeEdit::Symbology(Symbology::Code128));
    edit(&mut h, doc, BarcodeEdit::Payload("BURN-1".into()));
    edit(&mut h, doc, BarcodeEdit::Quiet("2".into()));
    let min = Symbology::Code128.capabilities().min_quiet_modules;
    assert_eq!(
        problem(&h, doc, BarcodeField::Quiet),
        Some(BarcodeIssue::QuietZoneTooSmall { min_modules: min, got_modules: 2 })
    );
    edit(&mut h, doc, BarcodeEdit::Quiet(String::new()));
    assert!(problem(&h, doc, BarcodeField::Quiet).is_none(), "empty means the standard minimum");

    // Typed numbers are parsed here.
    for (e, field) in [
        (BarcodeEdit::Module("breed".into()), BarcodeField::Module),
        (BarcodeEdit::MinModule("-1".into()), BarcodeField::MinModule),
        (BarcodeEdit::Height("0".into()), BarcodeField::Height),
        (BarcodeEdit::Quiet("1,5".into()), BarcodeField::Quiet),
        (BarcodeEdit::Width("veel".into()), BarcodeField::Width),
    ] {
        edit(&mut h, doc, e.clone());
        assert_eq!(problem(&h, doc, field), Some(BarcodeIssue::InvalidNumber), "{e:?}");
    }
}

#[test]
fn width_sets_the_module_and_a_small_width_warns_instead_of_clamping() {
    let (mut h, doc) = open();
    edit(&mut h, doc, BarcodeEdit::Payload("HELLO".into()));
    edit(&mut h, doc, BarcodeEdit::MinModule("0,2".into()));
    edit(&mut h, doc, BarcodeEdit::Width("30".into()));
    let d = vm(&h, doc).barcode_dialog.unwrap();
    let (w, _) = d.size.clone().expect("encodes");
    assert_eq!(w, "30", "the symbol is as wide as asked, quiet zone included");
    assert!(d.readability.is_none());
    let module: f64 = d.module.parse().unwrap();
    let direct = encode_symbol(Symbology::QrCode, "HELLO", &SymbologyParams::new(module, 0.2)).unwrap();
    assert!((direct.size_mm.0 - 30.0).abs() < 1e-4, "the module the dialog chose gives 30 mm: {}", direct.size_mm.0);

    edit(&mut h, doc, BarcodeEdit::Width("4".into()));
    let d = vm(&h, doc).barcode_dialog.unwrap();
    assert!(d.readability.is_some_and(|r| r.module_mm < r.min_module_mm && r.min_module_mm == 0.2));
    assert!(!d.can_create);
}

#[test]
fn create_makes_one_path_in_the_middle_of_the_view_in_one_undo_step() {
    let (mut h, doc) = open();
    edit(&mut h, doc, BarcodeEdit::Payload("BURNBURN".into()));
    edit(&mut h, doc, BarcodeEdit::Module("0,5".into()));
    edit(&mut h, doc, BarcodeEdit::MinModule("0,2".into()));
    let rev = vm(&h, doc).revision;
    edit(&mut h, doc, BarcodeEdit::Create);
    let after = vm(&h, doc);
    assert!(after.barcode_dialog.is_none(), "creating closes the dialog");
    assert_eq!(after.revision.raw(), rev.raw() + 1);
    assert_eq!(after.undo, Some(UndoLabel::Edit(burnburn_document::EditLabel::CreatePath)));
    assert_eq!(after.objects.len(), 1);
    assert_eq!(after.objects[0].label, "QR Code: BURNBURN");
    assert_eq!(after.selection, [after.objects[0].id], "the new symbol is selected");

    let direct = encode_symbol(Symbology::QrCode, "BURNBURN", &SymbologyParams::new(0.5, 0.2)).unwrap();
    let scene = h.scene(doc).unwrap();
    let path = &scene.items.iter().find(|i| i.source == after.objects[0].id).unwrap().path;
    let b = path.bounds().unwrap();
    let want = direct.path.bounds().unwrap();
    // Dark modules only: the path covers the symbol without the quiet zone, centred on the view centre (0, 0).
    assert!((b.width() - want.width()).abs() < 1e-9 && (b.height() - want.height()).abs() < 1e-9);
    assert_eq!(path.subpaths().len(), direct.path.subpaths().len(), "the same dark modules");
    assert!(b.center().x.abs() < 1e-6 && b.center().y.abs() < 1e-6);

    h.invoke(doc, CommandKey::Undo);
    assert!(vm(&h, doc).objects.is_empty());
}

#[test]
fn scaling_a_created_barcode_below_the_minimum_module_warns_with_the_limit() {
    let (mut h, doc) = open();
    edit(&mut h, doc, BarcodeEdit::Payload("BURNBURN".into()));
    edit(&mut h, doc, BarcodeEdit::Module("0,4".into()));
    edit(&mut h, doc, BarcodeEdit::MinModule("0,25".into()));
    edit(&mut h, doc, BarcodeEdit::Create);
    let id = vm(&h, doc).objects[0].id;
    h.send(UiIntent::Select { document: doc, ids: vec![id], mode: SelectMode::Replace });
    assert!(vm(&h, doc).barcode_scale_warning.is_none(), "as made it is readable");
    let width: f64 =
        vm(&h, doc).inspector.unwrap().fields.iter().find(|f| f.key == FieldKey::Width).unwrap().text.parse().unwrap();

    // 80 % of the width: module 0.32 mm, still above 0.25.
    h.send(UiIntent::CommitField { document: doc, field: FieldKey::Width, text: format!("{}", width * 0.8) });
    assert!(vm(&h, doc).barcode_scale_warning.is_none());
    // 50 % of the original: module 0.2 mm.
    h.send(UiIntent::CommitField { document: doc, field: FieldKey::Width, text: format!("{}", width * 0.5) });
    let w = vm(&h, doc).barcode_scale_warning.expect("below the minimum");
    assert!((w.module_mm - 0.2).abs() < 0.01 && w.min_module_mm == 0.25, "{w:?}");

    // Selecting something else, or nothing, shows no warning.
    h.send(UiIntent::Select { document: doc, ids: vec![], mode: SelectMode::Replace });
    assert!(vm(&h, doc).barcode_scale_warning.is_none());
}
