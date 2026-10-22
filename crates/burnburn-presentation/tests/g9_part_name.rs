//! G9-cam-werk, W01/W02: an explicit part name in the project. It is set in the notes panel, is
//! saved with the project (and makes it unsaved until then), travels with the released job, and an
//! order finds the job by that name instead of by the names of the drawn objects. Several jobs with the
//! same name still wait for the operator's choice.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

mod common;

use std::path::PathBuf;
use std::sync::Arc;

use burnburn_presentation::{
    CommandKey, HeadlessEditorHarness, HostEffect, JobCardViewModel, OrderDraft, Role, UiIntent,
};
use burnburn_test_support::FakeClock;
use burnburn_types::DocumentId;
use common::*;

fn part_name_of(h: &HeadlessEditorHarness, doc: DocumentId) -> Option<String> {
    h.document(doc).unwrap().notes.map(|n| n.part_name)
}

fn show_notes(h: &mut HeadlessEditorHarness, doc: DocumentId) {
    if h.document(doc).unwrap().notes.is_none() {
        h.invoke(doc, CommandKey::ShowNotes);
    }
}

fn set(h: &mut HeadlessEditorHarness, doc: DocumentId, name: &str) {
    h.send(UiIntent::SetPartName { document: doc, name: name.to_owned() });
}

// --- the name itself ----------------------------------------------------------------------------------------------

#[test]
fn the_part_name_is_shown_in_the_notes_panel_and_makes_the_document_unsaved_until_it_is_saved() {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    show_notes(&mut h, doc);
    assert_eq!(part_name_of(&h, doc), Some(String::new()), "no name yet");
    h.fs().create_dir_all_for_test(std::path::Path::new("/work"));
    let path = PathBuf::from("/work/houder.burnburn");
    h.send(UiIntent::Save { document: doc, path: Some(path.clone()) });
    h.run_tasks();
    assert!(!h.document(doc).unwrap().dirty);

    set(&mut h, doc, "  Houder links ");
    assert_eq!(part_name_of(&h, doc), Some("Houder links".to_owned()), "kept trimmed");
    assert!(h.document(doc).unwrap().dirty, "a changed name is unsaved work");
    // The same name again changes nothing.
    h.send(UiIntent::Save { document: doc, path: Some(path.clone()) });
    h.run_tasks();
    assert!(!h.document(doc).unwrap().dirty);
    set(&mut h, doc, "Houder links");
    assert!(!h.document(doc).unwrap().dirty, "the same name is no change");
    // It is not an undo step.
    assert!(h.document(doc).unwrap().undo.is_none());
}

#[test]
fn the_part_name_is_saved_and_comes_back_when_the_project_is_opened() {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.fs().create_dir_all_for_test(std::path::Path::new("/work"));
    let path = PathBuf::from("/work/houder.burnburn");
    set(&mut h, doc, "Houder");
    h.send(UiIntent::Save { document: doc, path: Some(path.clone()) });
    h.run_tasks();

    h.send(UiIntent::Open { path });
    let opened = h
        .run_tasks()
        .iter()
        .flat_map(|u| u.effects.clone())
        .find_map(|e| match e {
            HostEffect::OpenWindow { document } => Some(document),
            _ => None,
        })
        .expect("opened");
    show_notes(&mut h, opened);
    assert_eq!(part_name_of(&h, opened), Some("Houder".to_owned()));
    assert!(!h.document(opened).unwrap().dirty);
}

#[test]
fn a_blank_name_clears_it_and_a_name_beyond_the_limit_is_refused() {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    show_notes(&mut h, doc);
    set(&mut h, doc, "Houder");
    set(&mut h, doc, "   ");
    assert_eq!(part_name_of(&h, doc), Some(String::new()));
    set(&mut h, doc, "Houder");
    set(&mut h, doc, &"a".repeat(201));
    assert_eq!(part_name_of(&h, doc), Some("Houder".to_owned()), "refused, not cut");
    set(&mut h, doc, &"a".repeat(200));
    assert_eq!(part_name_of(&h, doc).unwrap().chars().count(), 200);
}

// --- orders ------------------------------------------------------------------------------------------------------------

/// The designer's run over the shared history with `name` as the explicit part name: the job card,
/// the file name and the name of the drawn object.
fn designer_releases(
    history: &Arc<RecordingHistory>,
    name: Option<&str>,
    width: Option<&str>,
) -> (JobCardViewModel, String, String) {
    let mut r = rig();
    r.h.controller().attach_history(history.clone()).unwrap();
    if let Some(width) = width {
        r.h.send(UiIntent::CommitField {
            document: r.doc,
            field: burnburn_presentation::FieldKey::Width,
            text: width.into(),
        });
    }
    if let Some(name) = name {
        set(&mut r.h, r.doc, name);
    }
    prepare(&mut r.h, r.doc);
    r.h.invoke(r.doc, CommandKey::ApproveJob);
    let job = r.h.document(r.doc).unwrap().job.unwrap();
    assert!(job.approved);
    let title = r.h.document(r.doc).unwrap().title;
    let object = r.h.document(r.doc).unwrap().objects[0].label.clone();
    (job, title, object)
}

fn operator(history: &Arc<RecordingHistory>) -> HeadlessEditorHarness {
    let clock = FakeClock::new();
    let peer = Peer::new(clock.clone());
    let service = service(&clock, &peer, history.clone());
    let mut h = HeadlessEditorHarness::new();
    h.controller().attach_machine(service);
    h.controller().attach_history(history.clone()).unwrap();
    h.controller().set_role(Role::Operator);
    h
}

fn order_for(part: &str) -> UiIntent {
    UiIntent::AddOrder { order: OrderDraft { name: "Klant 9".into(), part: part.into(), quantity: 2, remnant: None } }
}

#[test]
fn an_order_finds_the_job_by_the_explicit_part_name_and_no_longer_by_the_name_of_a_drawn_object() {
    let shared = history(&Peer::new(FakeClock::new()));
    let (job, title, object) = designer_releases(&shared, Some("Houder links"), None);
    assert_ne!(object, "Houder links");

    let mut op = operator(&shared);
    op.send(order_for("houder LINKS "));
    assert_eq!(op.operator_view().orders[0].job, Some(job.plan), "the name of the product, without case and spaces");
    op.send(order_for(&object));
    let orders = op.operator_view().orders;
    assert_eq!(orders[1].job, None, "the drawn object is not what this project makes");
    assert!(orders[1].candidates.is_empty());
    op.send(order_for(&title));
    assert_eq!(op.operator_view().orders[2].job, Some(job.plan), "the file name still finds it");
}

#[test]
fn without_an_explicit_name_the_drawn_objects_still_find_the_job() {
    let shared = history(&Peer::new(FakeClock::new()));
    let (job, _, object) = designer_releases(&shared, None, None);
    let mut op = operator(&shared);
    op.send(order_for(&object));
    assert_eq!(op.operator_view().orders[0].job, Some(job.plan));
}

#[test]
fn the_same_explicit_name_in_two_released_jobs_waits_for_the_operators_choice() {
    let shared = history(&Peer::new(FakeClock::new()));
    let (first, _, _) = designer_releases(&shared, Some("Houder"), None);
    let (second, _, _) = designer_releases(&shared, Some("Houder"), Some("100"));
    assert_ne!(first.plan, second.plan);

    let mut op = operator(&shared);
    op.send(order_for("Houder"));
    let order = op.operator_view().orders[0].clone();
    assert!(order.needs_choice, "two jobs make the product: nothing is picked automatically");
    assert_eq!((order.job, order.candidates.len()), (None, 2));
    assert!(order.upload.is_none(), "no upload before the choice");
    op.send(UiIntent::ChooseOrderJob { order: order.id, plan: second.plan });
    let order = op.operator_view().orders[0].clone();
    assert_eq!((order.job, order.needs_choice), (Some(second.plan), false));
}

#[test]
fn a_window_that_holds_the_job_finds_it_by_the_explicit_name_too() {
    let mut r = rig();
    set(&mut r.h, r.doc, "Houder");
    prepare(&mut r.h, r.doc);
    r.h.invoke(r.doc, CommandKey::ApproveJob);
    let job = r.h.document(r.doc).unwrap().job.unwrap();
    let object = r.h.document(r.doc).unwrap().objects[0].label.clone();
    r.h.controller().set_role(Role::Operator);
    r.h.send(order_for("Houder"));
    assert_eq!(r.h.operator_view().orders[0].job, Some(job.plan));
    r.h.send(order_for(&object));
    assert_eq!(r.h.operator_view().orders[1].job, None);
}
