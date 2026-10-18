//! The CLI against in-memory storage and a scripted fake controller (IM30, X05).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::sync::Arc;
use std::sync::atomic::Ordering;

use burnburn_cli::{CliError, profile_file::parse_profile};
use burnburn_machine::{ControllerIdentity, Reply, Request};
use burnburn_presentation::{CommandKey, HeadlessEditorHarness, UiIntent};
use burnburn_test_support::Step;
use burnburn_types::CancellationSource;
use common::*;

#[test]
fn cli_and_gui_compile_same_revision_to_same_normalized_plan() {
    let r = rig();
    with_project(&r);
    r.put("/work/plaat.burnburn", project_with_circle_bytes());
    let cli = r.run("compile /work/plaat.burnburn --profile /work/bench.json --out /work/plaat.rd").unwrap();

    // GUI route: the same file opened in the shared presentation controller, Compile invoked.
    let mut h = HeadlessEditorHarness::new();
    h.fs().insert(std::path::Path::new("/work/plaat.burnburn"), r.file("/work/plaat.burnburn").unwrap());
    h.send(UiIntent::Open { path: "/work/plaat.burnburn".into() });
    h.run_tasks();
    let doc = h.view().documents.iter().map(|d| d.id).next().expect("opened document");
    let pf = parse_profile(common::PROFILE_JSON.as_bytes()).unwrap();
    h.send(UiIntent::SetMachineProfile { profile: Box::new(pf.profile.clone()), ruida: pf.ruida });
    h.invoke(doc, CommandKey::Compile);
    h.run_tasks();
    let gui_plan = h.document(doc).unwrap().plan.expect("GUI compiled a plan").id;
    assert_eq!(cli.plan, Some(gui_plan), "CLI and GUI must produce the same JobPlanId");
}

#[test]
fn compile_writes_rd_and_plan_and_records_the_frozen_job() {
    let r = rig();
    with_project(&r);
    let out = r
        .run("compile /work/plaat.burnburn --profile /work/bench.json --out /work/plaat.rd --plan-json /work/plaat.plan.json --history /data/history")
        .unwrap();
    let rd = r.file("/work/plaat.rd").expect("RD written");
    assert!(r.file("/work/plaat.plan.json").is_some());
    let job = out.job.expect("frozen job recorded");
    assert!(out.stdout.contains(&format!("job {job}")), "{}", out.stdout);
    assert!(out.stdout.contains(&burnburn_machine::rd_hash(&rd).to_hex()), "{}", out.stdout);

    let inspect = r.run("inspect /work/plaat.rd").unwrap();
    assert!(inspect.stdout.contains("file sum ok"), "{}", inspect.stdout);
    assert!(inspect.stdout.contains("cut bounds: x 10.000..130.000 mm, y 10.000..90.000 mm"), "{}", inspect.stdout);

    let history = r.run("history --history /data/history").unwrap();
    assert!(history.stdout.contains(&format!("job {job}")), "{}", history.stdout);
}

#[test]
fn compile_refuses_on_preflight_errors_and_writes_nothing() {
    let r = rig();
    r.put("/work/te-groot.burnburn", project_bytes(&[(1400.0, 10.0, 200.0, 80.0)]));
    r.put("/work/bench.json", PROFILE_JSON.as_bytes().to_vec());
    let e = r.run("compile /work/te-groot.burnburn --profile /work/bench.json --out /work/x.rd").unwrap_err();
    assert!(matches!(e, CliError::Preflight { errors, .. } if errors > 0), "{e:?}");
    assert_eq!(e.exit_code(), 2);
    assert!(r.file("/work/x.rd").is_none());
    let pre = r.run("preflight /work/te-groot.burnburn --profile /work/bench.json").unwrap_err();
    assert!(matches!(pre, CliError::Preflight { .. }));
}

#[test]
fn cancelled_compile_writes_nothing() {
    let mut r = rig();
    with_project(&r);
    let source = CancellationSource::new();
    source.cancel();
    r.env.cancel = source.token();
    assert_eq!(
        r.run("compile /work/plaat.burnburn --profile /work/bench.json --out /work/plaat.rd"),
        Err(CliError::Cancelled)
    );
    assert!(r.file("/work/plaat.rd").is_none());
}

#[test]
fn upload_and_start_need_the_explicit_machine_contact_switch() {
    let r = rig();
    with_project(&r);
    let out = r.run("compile /work/plaat.burnburn --profile /work/bench.json --out /work/p.rd --history /h").unwrap();
    let job = out.job.unwrap();
    let up =
        r.run(&format!("upload --history /h --job {} --profile /work/bench.json --peer 192.0.2.10:50200", job.hash()));
    assert_eq!(up, Err(CliError::MachineContactNotAllowed));
    let st = r.run("start --history /h --execution 1 --profile /work/bench.json --peer 192.0.2.10:50200");
    assert_eq!(st, Err(CliError::MachineContactNotAllowed));
    assert_eq!(r.connector.connects.load(Ordering::SeqCst), 0, "nothing may be contacted");
    assert!(r.run("compile /work/plaat.burnburn --profile /work/bench.json --out /work/q.rd --and-start x").is_err());
}

fn identity() -> Reply {
    Reply::Identity(ControllerIdentity { model: Some("RDC6445G".into()), firmware: Some("fake-1".into()) })
}

#[test]
fn upload_then_start_are_separate_invocations_against_the_fake() {
    let r = rig();
    with_project(&r);
    let job = r
        .run("compile /work/plaat.burnburn --profile /work/bench.json --out /work/p.rd --history /h")
        .unwrap()
        .job
        .unwrap();
    let rd = r.file("/work/p.rd").unwrap();
    let peer = &r.connector.peer;
    peer.script(Step::expect(Request::QueryIdentity).reply(identity()));
    for (index, chunk) in rd.chunks(1470).enumerate() {
        peer.script(Step::expect(Request::UploadChunk { index, bytes: chunk.to_vec() }).reply(Reply::Ack));
    }
    peer.script(Step::expect(Request::QueryStored).reply(Reply::Stored(true)));
    let up = r
        .run(&format!(
            "upload --history /h --job {} --profile /work/bench.json --peer 127.0.0.1:50200 --allow-machine-contact",
            job.hash()
        ))
        .unwrap();
    let exec = up.execution.unwrap();
    assert!(up.stdout.contains("stored"), "{}", up.stdout);
    assert_eq!(peer.sent().iter().filter(|q| **q == Request::Start).count(), 0, "upload never starts");

    // A later invocation: identity + read-back again, then exactly one start.
    peer.script(Step::expect(Request::QueryIdentity).reply(identity()));
    peer.script(Step::expect(Request::QueryStored).reply(Reply::Stored(true)));
    peer.script(Step::expect(Request::Start).reply(Reply::Ack));
    let st = r
        .run(&format!(
            "start --history /h --execution {} --profile /work/bench.json --peer 127.0.0.1:50200 --allow-machine-contact",
            exec.raw()
        ))
        .unwrap();
    assert!(st.stdout.contains("start requested"), "{}", st.stdout);
    let again = r.run(&format!(
        "start --history /h --execution {} --profile /work/bench.json --peer 127.0.0.1:50200 --allow-machine-contact",
        exec.raw()
    ));
    assert!(again.is_err(), "a second start is refused: {again:?}");
    assert_eq!(peer.sent().iter().filter(|q| **q == Request::Start).count(), 1);
    peer.verify_complete().unwrap();

    let history = r.run("history --history /h --machine ruida-bench-1").unwrap();
    assert!(history.stdout.contains(&format!("execution {}", exec.raw())), "{}", history.stdout);
    // Each CLI invocation is its own process: afterwards the started execution is Unknown until
    // a status query or an operator settles it; the start intent is recorded exactly once.
    assert!(history.stdout.contains("interrupted by restart"), "{}", history.stdout);
    assert!(history.stdout.contains("intents [upload, confirm-stored, confirm-stored, start]"), "{}", history.stdout);
    let _ = Arc::clone(&r.connector);
}
