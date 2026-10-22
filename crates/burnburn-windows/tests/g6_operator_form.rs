//! The compact operator view as a Windows form, headless (G6-werk, W08): same lines, words and
//! intents as the macOS panel (`tests/g6_werk_operator.rs` there). The machine is a fake service.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, dead_code)]

#[path = "../../burnburn-presentation/tests/common/mod.rs"]
mod operator_rig;

use burnburn_presentation::{CommandKey, Role, UiIntent};
use burnburn_windows::adapter::{Control, MenuItem, MessageContext, Reaction, command_id, msg, notify, on_message};
use burnburn_windows::forms::g5;
use burnburn_windows::forms::{ControlId, FormContext, FormKind, FormLine, FormOutcome, PanelState};
use burnburn_windows::l10n::Locale;
use burnburn_windows::panels::{G5Control as C, G5Panel as P};
use operator_rig::*;

fn shown() -> PanelState {
    PanelState { g5_shown: vec![P::Operator], ..PanelState::default() }
}

#[test]
fn the_operator_form_lists_the_job_and_its_upload_sends_the_machine_button_unchanged() {
    let mut r = rig();
    prepare(&mut r.h, r.doc);
    r.h.invoke(r.doc, CommandKey::ApproveJob);
    r.h.controller().set_role(Role::Operator);
    let vm = r.h.view();
    let context = FormContext { vm: &vm, document: r.doc, locale: Locale::En };
    let form = g5::forms(&context, &shown(), &Vec::new())
        .into_iter()
        .find(|f| f.kind == FormKind::G5(P::Operator))
        .expect("the operator form");
    let texts: Vec<String> = form
        .lines
        .iter()
        .flat_map(|l| match l {
            FormLine::Row(inner) => inner.iter().collect::<Vec<_>>(),
            other => vec![other],
        })
        .filter_map(|l| match l {
            FormLine::Text(t) | FormLine::Heading(t) | FormLine::Problem(t) => Some(t.clone()),
            _ => None,
        })
        .collect();
    assert!(texts.iter().any(|t| t.contains("120 × 80 mm") && t.contains("revision")), "{texts:?}");

    let row = r.h.operator_view().jobs[0].clone();
    let upload = row.upload.clone().unwrap();
    let outcome = g5::react(P::Operator, &ControlId::G5(C::OperatorUpload { index: 0 }), &form.values(), &context);
    let FormOutcome::Intent(intent) = outcome else { panic!("{outcome:?}") };
    assert_eq!(intent, UiIntent::Machine { document: Some(row.document), action: upload.action, seen: upload.seen });
}

#[test]
fn the_menu_item_has_the_macos_tag_and_toggles_the_operator_form() {
    let r = rig();
    let vm = r.h.view();
    assert_eq!(command_id(MenuItem::OperatorPanel), 1033);
    let ctx = MessageContext {
        document: r.doc,
        control: Control::Frame,
        dpi: 96,
        text: "",
        selected_rows: &[],
        alt: false,
        cursor_px: [0, 0],
        time_ms: 0,
    };
    let wm = |id: u16| (id as usize) | ((notify::MENU as usize) << 16);
    assert_eq!(
        on_message(&ctx, msg::WM_COMMAND, wm(command_id(MenuItem::OperatorPanel)), 0, &vm),
        Reaction::ToggleG5Panel(P::Operator)
    );
}
