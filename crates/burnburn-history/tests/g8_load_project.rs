//! G8-cam-werk, W01/W02: a frozen job gives its project back, so the names of its parts can be
//! read from the history (an order finds a job by part name). The project is the frozen blob,
//! checked against the hash recorded with the job; an unknown job is a typed error.
#![allow(clippy::unwrap_used)]

mod common;

use burnburn_document::{CommandPayload, DocumentSession};
use burnburn_geometry::{Point, Rect};
use burnburn_history::HistoryError;
use burnburn_types::{DocumentId, StableId};
use common::*;

fn named_plate() -> DocumentSession {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let mut ids = Vec::new();
    for (i, name) in ["Beugel", "Plaatje"].iter().enumerate() {
        let rect = Rect::new(Point::new(10.0 + 60.0 * i as f64, 10.0), mm(50.0), mm(40.0)).unwrap();
        ids.push(run(&mut s, CommandPayload::CreateRectangle { rect, name: (*name).to_owned() }).changes.created[0]);
    }
    run(
        &mut s,
        CommandPayload::Process(burnburn_document::process::ProcessCommand::AddOperation {
            draft: cut_op(),
            targets: ids,
        }),
    );
    s
}

#[test]
fn the_project_of_a_frozen_job_comes_back_with_the_names_of_its_parts() {
    let fs = Fs::new();
    let (mut h, _) = open(&fs);
    let c = compile_session(&named_plate(), &profile(1));
    let job = record(&mut h, &fs, &c, &profile(1), t(0));
    let project = h.load_project(&fs, job).unwrap();
    let snapshot = &project.document;
    let mut names: Vec<String> =
        snapshot.draw_order().into_iter().filter_map(|id| snapshot.object(id)).map(|o| o.name().to_owned()).collect();
    names.sort();
    assert_eq!(names, vec!["Beugel".to_owned(), "Plaatje".to_owned()]);
    assert_eq!(burnburn_io::project_hash(&project).unwrap(), h.job(job).unwrap().project_hash);
}

#[test]
fn it_survives_a_restart_and_an_unknown_job_is_a_typed_error() {
    let fs = Fs::new();
    let (mut h, _) = open(&fs);
    let c = compile_session(&named_plate(), &profile(1));
    let job = record(&mut h, &fs, &c, &profile(1), t(0));
    drop(h);
    let (h, report) = open(&fs);
    assert_eq!(report.corrupt, None);
    assert!(h.load_project(&fs, job).is_ok());
    let other = burnburn_history::compiled_job_id(
        &burnburn_types::ContentHash::of(burnburn_types::HashDomain::Plan, b"nope"),
        &burnburn_types::ContentHash::of(burnburn_types::HashDomain::Plan, b"nope2"),
        profile(1).revision(),
    );
    assert!(matches!(h.load_project(&fs, other), Err(HistoryError::UnknownJob(_))));
}
