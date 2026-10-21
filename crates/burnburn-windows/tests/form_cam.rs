//! The CAM dialogs of the Windows window, headless (W4-L8a): Optimization Settings, Material /
//! Interval / Focus Test and Device Settings as one generic form each. Rows, words, notes, reasons
//! and intents come from the same view model and panel modules as the macOS dialogs
//! (`cam_panels.rs` there); a changed control is one `SetCamField`, a button one
//! `CamDialogAction`, and Save RD waits for a path.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_document::process::OrderKey;
use burnburn_job::{Feature, MachineCapabilities, MachineFrame, MachineProfile, OriginCorner, PowerRatio, Support};
use burnburn_presentation::{
    CamDialog, DialogAction, DialogActionKind, DialogField, DialogValue, DialogViewModel, HeadlessEditorHarness,
    UiIntent,
};
use burnburn_ruida::{Magic, OverscanPolicy, RuidaProfile};
use burnburn_types::DocumentId;
use burnburn_windows::forms::{
    ControlId, ControlValue, FilePick, Form, FormContext, FormKind, FormLine, FormOutcome, Placement, cam,
};
use burnburn_windows::l10n::Locale;
use burnburn_windows::panels::{cam_caption, cam_note, cam_title};

fn rig(z: Support) -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    let profile = MachineProfile::new(
        "research-1500x900",
        1,
        MachineFrame::new(1500.0, 900.0, OriginCorner::BottomLeft).unwrap(),
        1000.0,
        400.0,
        PowerRatio::new(0.0).unwrap(),
        PowerRatio::new(1.0).unwrap(),
    )
    .unwrap()
    .with_capabilities(MachineCapabilities::conservative().with(Feature::ZStep, z))
    .unwrap();
    h.send(UiIntent::SetMachineProfile {
        profile: Box::new(profile),
        ruida: RuidaProfile { magic: Magic::FIXTURE_0X88, overscan: OverscanPolicy::ControllerManaged },
    });
    (h, doc)
}

fn open(h: &mut HeadlessEditorHarness, doc: DocumentId, dialog: CamDialog) -> DialogViewModel {
    h.send(UiIntent::OpenCamDialog { document: doc, dialog });
    h.controller().cam_dialog(doc, dialog).unwrap()
}

fn form(h: &HeadlessEditorHarness, doc: DocumentId, dialog: CamDialog, model: &DialogViewModel, locale: Locale) -> Form {
    let vm = h.view();
    cam::form(&FormContext { vm: &vm, document: doc, locale }, dialog, model).expect("a dialog form")
}

fn react(
    h: &HeadlessEditorHarness,
    doc: DocumentId,
    dialog: CamDialog,
    model: Option<&DialogViewModel>,
    id: ControlId,
    value: ControlValue,
) -> FormOutcome {
    let vm = h.view();
    cam::react(dialog, model, &id, &value, &FormContext { vm: &vm, document: doc, locale: Locale::En })
}

fn buttons(form: &Form) -> Vec<(ControlId, String, bool)> {
    form.lines
        .iter()
        .filter_map(|l| if let FormLine::Buttons(b) = l { Some(b) } else { None })
        .flatten()
        .map(|b| (b.id.clone(), b.title.clone(), b.enabled))
        .collect()
}

#[test]
fn every_dialog_is_a_floating_form_with_its_title_its_visible_rows_and_its_buttons() {
    let (mut h, doc) = rig(Support::Supported);
    for dialog in CamDialog::ALL {
        let model = open(&mut h, doc, dialog);
        for locale in [Locale::Nl, Locale::En] {
            let f = form(&h, doc, dialog, &model, locale);
            assert_eq!((f.kind, f.placement), (FormKind::Cam(dialog), Placement::Floating));
            assert_eq!(f.title, cam_title(dialog, locale));
            // Every row that is not hidden has a control with its caption in the window language.
            for row in &model.rows {
                let id = ControlId::Cam(row.field);
                let hidden = matches!(row.state, burnburn_presentation::FieldState::Hidden);
                assert_eq!(f.ids().contains(&id), !hidden, "{:?} in {dialog:?}", row.field);
                if !hidden {
                    let caption = cam_caption(row.field, &model, locale);
                    let shown = match f.find(&id) {
                        Some(FormLine::Field { caption, .. } | FormLine::Check { caption, .. } | FormLine::Choice { caption, .. }) => {
                            caption.clone()
                        }
                        other => panic!("{other:?}"),
                    };
                    assert_eq!(shown, caption);
                }
            }
            // Every note is said.
            for note in &model.notes {
                let text = cam_note(*note, locale);
                if !text.is_empty() {
                    assert!(f.lines.iter().any(|l| matches!(l, FormLine::Text(t) if *t == text)), "{note:?}");
                }
            }
            assert!(!buttons(&f).is_empty(), "{dialog:?} has buttons");
        }
        h.send(UiIntent::CamDialogAction { document: doc, dialog, action: DialogAction::Close });
    }
}

#[test]
fn a_changed_control_becomes_one_set_field_and_a_repeat_becomes_none() {
    let (mut h, doc) = rig(Support::Unsupported);
    let model = open(&mut h, doc, CamDialog::Device);
    let id = ControlId::Cam(DialogField::BedWidth);
    assert_eq!(
        react(&h, doc, CamDialog::Device, Some(&model), id.clone(), ControlValue::Text("1500".into())),
        FormOutcome::Nothing
    );
    assert_eq!(
        react(&h, doc, CamDialog::Device, Some(&model), id.clone(), ControlValue::Text("900".into())),
        FormOutcome::Intent(UiIntent::SetCamField {
            document: doc,
            dialog: CamDialog::Device,
            field: DialogField::BedWidth,
            value: DialogValue::Text("900".into())
        })
    );
    // A pop-up choice and a check use their own value kinds.
    let origin = ControlId::Cam(DialogField::BedOrigin);
    assert!(matches!(
        react(&h, doc, CamDialog::Device, Some(&model), origin, ControlValue::Choice(1)),
        FormOutcome::Intent(UiIntent::SetCamField { value: DialogValue::Choice(1), .. })
    ));
    assert!(matches!(
        react(&h, doc, CamDialog::Device, Some(&model), ControlId::Cam(DialogField::RotaryEnabled), ControlValue::Toggle(true)),
        FormOutcome::Intent(UiIntent::SetCamField { value: DialogValue::Flag(true), .. })
    ));
    assert_eq!(react(&h, doc, CamDialog::Device, None, id.clone(), ControlValue::Text("1".into())), FormOutcome::Nothing);
    // A row in error sends its text again, so a retry of the same text goes through; the reason is
    // under the row.
    h.send(UiIntent::SetCamField {
        document: doc,
        dialog: CamDialog::Device,
        field: DialogField::BedWidth,
        value: DialogValue::Text("abc".into()),
    });
    h.send(UiIntent::CamDialogAction { document: doc, dialog: CamDialog::Device, action: DialogAction::Apply });
    let model = h.controller().cam_dialog(doc, CamDialog::Device).unwrap();
    assert!(matches!(
        react(&h, doc, CamDialog::Device, Some(&model), id.clone(), ControlValue::Text("abc".into())),
        FormOutcome::Intent(_)
    ));
    let f = form(&h, doc, CamDialog::Device, &model, Locale::En);
    assert!(matches!(f.find(&id), Some(FormLine::Field { invalid: true, .. })));
    assert!(f.lines.iter().any(|l| matches!(l, FormLine::Problem(p) if !p.is_empty())));
}

#[test]
fn buttons_send_their_action_only_when_enabled_and_save_waits_for_a_path() {
    let (mut h, doc) = rig(Support::Unsupported);
    let device = open(&mut h, doc, CamDialog::Device);
    let press = |h: &HeadlessEditorHarness, d, m: &DialogViewModel, kind| {
        react(h, doc, d, Some(m), ControlId::CamAction(kind), ControlValue::Press)
    };
    assert_eq!(
        press(&h, CamDialog::Device, &device, DialogActionKind::Apply),
        FormOutcome::Intent(UiIntent::CamDialogAction { document: doc, dialog: CamDialog::Device, action: DialogAction::Apply })
    );
    assert_eq!(press(&h, CamDialog::Device, &device, DialogActionKind::ReturnToFlatbed), FormOutcome::Nothing, "no rotary to leave");
    assert_eq!(press(&h, CamDialog::Device, &device, DialogActionKind::Generate), FormOutcome::Nothing, "not a Device button");
    let focus = open(&mut h, doc, CamDialog::FocusTest);
    assert_eq!(press(&h, CamDialog::FocusTest, &focus, DialogActionKind::Generate), FormOutcome::Nothing, "a focus test without Z does not start");
    // The generate button of a test is on the form, greyed when it cannot start.
    let f = form(&h, doc, CamDialog::FocusTest, &focus, Locale::En);
    assert!(buttons(&f).iter().any(|(id, _, enabled)| *id == ControlId::CamAction(DialogActionKind::Generate) && !*enabled));

    let material = open(&mut h, doc, CamDialog::MaterialTest);
    assert_eq!(press(&h, CamDialog::MaterialTest, &material, DialogActionKind::ExportRd), FormOutcome::Nothing, "nothing to save yet");
    h.send(UiIntent::CamDialogAction { document: doc, dialog: CamDialog::MaterialTest, action: DialogAction::Generate });
    h.run_tasks();
    let material = h.controller().cam_dialog(doc, CamDialog::MaterialTest).unwrap();
    assert!(material.result.is_some());
    // Save RD asks for a path first; the chosen path completes the intent.
    assert_eq!(
        press(&h, CamDialog::MaterialTest, &material, DialogActionKind::ExportRd),
        FormOutcome::PickFile(FilePick::CamExportRd(CamDialog::MaterialTest))
    );
    let vm = h.view();
    let ctx = FormContext { vm: &vm, document: doc, locale: Locale::En };
    assert_eq!(
        cam::file_chosen(CamDialog::MaterialTest, Some(&material), "/x.rd".into(), &ctx),
        FormOutcome::Intent(UiIntent::CamDialogAction {
            document: doc,
            dialog: CamDialog::MaterialTest,
            action: DialogAction::ExportRd("/x.rd".into())
        })
    );
    // The result is on the form in words.
    let f = form(&h, doc, CamDialog::MaterialTest, &material, Locale::En);
    assert!(f.lines.iter().any(|l| matches!(l, FormLine::Text(t) if t.contains("25") && t.contains("plan-"))));
}

#[test]
fn the_order_levels_have_arrows_that_follow_the_row_and_move_the_level() {
    let (mut h, doc) = rig(Support::Unsupported);
    let model = open(&mut h, doc, CamDialog::Optimization);
    let f = form(&h, doc, CamDialog::Optimization, &model, Locale::En);
    let arrow = |key, up| {
        buttons(&f).into_iter().find(|(id, _, _)| *id == ControlId::CamMove(key, up)).map(|(_, _, enabled)| enabled)
    };
    assert_eq!(arrow(OrderKey::Layer, true), Some(false), "the first level cannot go up");
    assert_eq!(arrow(OrderKey::Priority, false), Some(false), "the last cannot go down");
    assert_eq!(arrow(OrderKey::Layer, false), Some(true));
    assert_eq!(
        react(&h, doc, CamDialog::Optimization, Some(&model), ControlId::CamMove(OrderKey::Layer, true), ControlValue::Press),
        FormOutcome::Nothing
    );
    let down = react(&h, doc, CamDialog::Optimization, Some(&model), ControlId::CamMove(OrderKey::Layer, false), ControlValue::Press);
    assert_eq!(
        down,
        FormOutcome::Intent(UiIntent::CamDialogAction {
            document: doc,
            dialog: CamDialog::Optimization,
            action: DialogAction::MoveDown(OrderKey::Layer)
        })
    );
    if let FormOutcome::Intent(i) = down {
        h.send(i);
    }
    let model = h.controller().cam_dialog(doc, CamDialog::Optimization).unwrap();
    assert_eq!(model.order()[1].0, OrderKey::Layer);
}

#[test]
fn the_device_dialog_lists_what_the_profile_supports() {
    let (mut h, doc) = rig(Support::Unsupported);
    let model = open(&mut h, doc, CamDialog::Device);
    assert!(!model.capabilities.is_empty());
    let f = form(&h, doc, CamDialog::Device, &model, Locale::En);
    let z = f
        .lines
        .iter()
        .find_map(|l| match l {
            FormLine::Text(t) if t.contains("Z axis") => Some(t.clone()),
            _ => None,
        })
        .expect("the Z axis line");
    assert!(z.contains("not supported"), "{z}");
}
