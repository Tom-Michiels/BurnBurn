//! G8-cam-werk, W07: the quantity of an order goes to the nest. The pieces still to make (the
//! quantity less the recorded completions) become the number of copies of the parts of the nest that
//! carry the order's part name; other parts keep their numbers, and a name nobody has is said.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

mod common;

use burnburn_machine::{ControllerStatus, Reply, Request};
use burnburn_test_support::Step;
use burnburn_presentation::{
    CommandKey, MachineAction, NestFailure, NestPanelViewModel, OrderDraft, OrderViewModel, SchikkenIntent,
    SchikkenMessage, UiIntent,
};
use common::*;

/// The window of the designer: the rectangle of the rig and a second one, the nest panel open on both.
fn nest_rig() -> (Rig, String, String) {
    let mut r = rig();
    r.h.invoke(r.doc, CommandKey::ToolRectangle);
    r.h.drag(r.doc, (200.0, 10.0), (240.0, 40.0));
    r.h.invoke(r.doc, CommandKey::ToolSelect);
    let names: Vec<String> = r.h.document(r.doc).unwrap().objects.iter().map(|o| o.label.clone()).collect();
    assert_eq!(names.len(), 2);
    assert_ne!(names[0], names[1]);
    r.h.invoke(r.doc, CommandKey::SelectAll);
    r.h.invoke(r.doc, CommandKey::QuickNest);
    (r, names[0].clone(), names[1].clone())
}

fn panel(r: &Rig) -> NestPanelViewModel {
    r.h.document(r.doc).unwrap().schikken.nest.expect("the nest panel is open")
}

fn add_order(r: &mut Rig, part: &str, quantity: u32) -> OrderViewModel {
    r.h.send(UiIntent::AddOrder {
        order: OrderDraft { name: "Klant 8".into(), part: part.into(), quantity, remnant: None },
    });
    r.h.operator_view().orders.last().cloned().expect("the order")
}

fn take(r: &mut Rig, order: &OrderViewModel) {
    r.h.send(UiIntent::Schikken(SchikkenIntent::TakeOrderQuantities { document: r.doc, order: order.id }));
}

#[test]
fn the_panel_lists_the_orders_with_what_is_left_and_how_many_parts_carry_their_name() {
    let (mut r, first, _) = nest_rig();
    assert!(panel(&r).orders.is_empty(), "no orders, no rows");
    let order = add_order(&mut r, &first, 5);
    let rows = panel(&r).orders;
    assert_eq!(rows.len(), 1);
    assert_eq!((rows[0].id, rows[0].remaining, rows[0].matching_parts), (order.id, 5, 1));
    let none = add_order(&mut r, "Een onderdeel dat nergens in zit", 2);
    let rows = panel(&r).orders;
    assert_eq!(rows.iter().find(|row| row.id == none.id).map(|row| row.matching_parts), Some(0));
}

#[test]
fn taking_an_order_makes_its_quantity_the_copies_of_its_part_and_leaves_the_others() {
    let (mut r, first, second) = nest_rig();
    let order = add_order(&mut r, &first, 3);
    take(&mut r, &order);
    let p = panel(&r);
    assert_eq!(p.error, None);
    let rows: Vec<(&str, &str, bool)> =
        p.part_rows.iter().map(|row| (row.name.as_str(), row.copies.as_str(), row.own)).collect();
    assert!(rows.contains(&(first.as_str(), "3", true)), "{rows:?}");
    assert!(rows.contains(&(second.as_str(), "1", false)), "the other part keeps the number of the panel: {rows:?}");

    r.h.send(UiIntent::Schikken(SchikkenIntent::StartNest { document: r.doc }));
    r.h.run_tasks();
    let report = panel(&r).report.expect("report");
    let mut counts: Vec<u32> = report.parts.iter().map(|row| row.ordered).collect();
    counts.sort_unstable();
    assert_eq!(counts, vec![1, 3], "three of the ordered part, one of the other");
}

#[test]
fn a_name_without_a_part_and_an_order_that_is_gone_are_said_and_change_nothing() {
    let (mut r, first, _) = nest_rig();
    let before = panel(&r).input;
    let nothing = add_order(&mut r, "Een onderdeel dat nergens in zit", 2);
    take(&mut r, &nothing);
    assert_eq!(panel(&r).error, Some(SchikkenMessage::Nest(NestFailure::OrderMatchesNothing)));
    assert_eq!(panel(&r).input, before);

    let order = add_order(&mut r, &first, 4);
    let gone = burnburn_presentation::OrderId(order.id.0 + 100);
    r.h.send(UiIntent::Schikken(SchikkenIntent::TakeOrderQuantities { document: r.doc, order: gone }));
    assert_eq!(panel(&r).error, Some(SchikkenMessage::Nest(NestFailure::UnknownOrder)));
    assert_eq!(panel(&r).input, before);
}

#[test]
fn only_the_pieces_still_to_make_are_taken_and_a_finished_order_is_said() {
    let mut r = rig();
    // The designer releases the job; the operator makes one piece of an order for two.
    prepare(&mut r.h, r.doc);
    r.h.invoke(r.doc, CommandKey::ApproveJob);
    let job = r.h.document(r.doc).unwrap().job.unwrap();
    let part = r.h.document(r.doc).unwrap().objects[0].label.clone();
    let order = add_order(&mut r, &part, 2);
    assert_eq!(order.job, Some(job.plan));
    script_upload_acks(&r.peer, &job);
    r.h.send(UiIntent::UploadForOrder { order: order.id, action: order.upload.clone().unwrap().action, seen: None });
    pump(&r);
    let press = |r: &mut Rig, kind: MachineAction| {
        let button = r.h.operator_view().machine.expect("machine").button(kind).cloned().expect("control");
        r.h.press(None, &button);
        pump(r);
    };
    r.peer.script(Step::expect(Request::QueryStored).reply(Reply::Stored(true)));
    press(&mut r, MachineAction::ConfirmStored);
    r.peer.script(Step::expect(Request::Start).reply(Reply::Ack));
    press(&mut r, MachineAction::Start);
    r.peer.script(Step::expect(Request::QueryStatus).reply(Reply::Status(ControllerStatus::Running)));
    press(&mut r, MachineAction::QueryStatus);
    r.peer.script(Step::expect(Request::QueryStatus).reply(Reply::Status(ControllerStatus::Finished)));
    press(&mut r, MachineAction::QueryStatus);
    assert_eq!(r.h.operator_view().orders[0].completed, 1);

    r.h.invoke(r.doc, CommandKey::SelectAll);
    r.h.invoke(r.doc, CommandKey::QuickNest);
    let rows = panel(&r).orders;
    assert_eq!((rows[0].remaining, rows[0].matching_parts), (1, 1), "two ordered, one made");
    take(&mut r, &order);
    assert_eq!(panel(&r).part_rows[0].copies, "1");

    // The second piece is made as well: nothing is left to nest.
    let order = r.h.operator_view().orders[0].clone();
    script_upload_acks(&r.peer, &job);
    r.h.send(UiIntent::UploadForOrder {
        order: order.id,
        action: order.upload.clone().unwrap().action,
        seen: order.upload.clone().unwrap().seen,
    });
    pump(&r);
    r.peer.script(Step::expect(Request::QueryStored).reply(Reply::Stored(true)));
    press(&mut r, MachineAction::ConfirmStored);
    r.peer.script(Step::expect(Request::Start).reply(Reply::Ack));
    press(&mut r, MachineAction::Start);
    r.peer.script(Step::expect(Request::QueryStatus).reply(Reply::Status(ControllerStatus::Running)));
    press(&mut r, MachineAction::QueryStatus);
    r.peer.script(Step::expect(Request::QueryStatus).reply(Reply::Status(ControllerStatus::Finished)));
    press(&mut r, MachineAction::QueryStatus);
    assert_eq!(r.h.operator_view().orders[0].completed, 2);
    take(&mut r, &order);
    assert_eq!(panel(&r).error, Some(SchikkenMessage::Nest(NestFailure::OrderDone)));
}
