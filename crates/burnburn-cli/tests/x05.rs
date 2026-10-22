//! X05 (G5-tekst-api): GUI and CLI agree on the compiled plan and the RD bytes; compile and
//! export are separate from upload and start; a machine action names its target machine and its
//! intent; unknown options are refused instead of ignored. No socket, no laser.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::sync::atomic::Ordering;

use burnburn_cli::{CliError, profile_file::parse_profile};
use burnburn_io::fs::FileSystem;
use burnburn_presentation::{CommandKey, HeadlessEditorHarness, UiIntent};
use common::*;

const UP: &str = "upload --history /h --job";

fn compiled_job(r: &Rig) -> String {
    with_project(r);
    let out = r.run("compile /work/plaat.burnburn --profile /work/bench.json --out /work/p.rd --history /h").unwrap();
    out.job.unwrap().hash().to_hex()
}

// ---- GUI and CLI: one plan, one RD ------------------------------------------------------------

#[test]
fn gui_and_cli_give_the_same_plan_id_and_the_same_rd_bytes_for_the_same_input() {
    let r = rig();
    with_project(&r);
    let cli = r.run("compile /work/plaat.burnburn --profile /work/bench.json --out /work/plaat.rd").unwrap();
    let cli_rd = r.file("/work/plaat.rd").expect("RD written by the CLI");

    let mut h = HeadlessEditorHarness::new();
    h.fs().insert(std::path::Path::new("/work/plaat.burnburn"), r.file("/work/plaat.burnburn").unwrap());
    h.send(UiIntent::Open { path: "/work/plaat.burnburn".into() });
    h.run_tasks();
    let doc = h.view().documents.iter().map(|d| d.id).next().expect("opened document");
    let pf = parse_profile(PROFILE_JSON.as_bytes()).unwrap();
    h.send(UiIntent::SetMachineProfile { profile: Box::new(pf.profile.clone()), ruida: pf.ruida });
    h.invoke(doc, CommandKey::Compile);
    h.run_tasks();
    let gui_plan = h.document(doc).unwrap().plan.expect("GUI compiled a plan").id;
    h.send(UiIntent::ExportRd { document: doc, path: "/out/job.rd".into() });
    h.run_tasks();
    let gui_rd = h.fs().read(std::path::Path::new("/out/job.rd"), u64::MAX).expect("GUI wrote the RD");

    assert_eq!(cli.plan, Some(gui_plan), "the same normalized plan id");
    assert!(cli.stdout.contains(&format!("plan {gui_plan}")), "the CLI prints the id the GUI shows: {}", cli.stdout);
    assert_eq!(
        burnburn_machine::rd_hash(&cli_rd),
        burnburn_machine::rd_hash(&gui_rd),
        "the exported bytes hash the same"
    );
    assert_eq!(cli_rd, gui_rd);
}

// ---- compile/export never contact a machine ---------------------------------------------------

#[test]
fn compile_preflight_inspect_and_history_never_touch_the_machine_connector() {
    let r = rig();
    with_project(&r);
    r.run("compile /work/plaat.burnburn --profile /work/bench.json --out /work/p.rd --history /h").unwrap();
    r.run("preflight /work/plaat.burnburn --profile /work/bench.json").unwrap();
    r.run("inspect /work/p.rd").unwrap();
    r.run("history --history /h").unwrap();
    // Even the switch that allows machine contact is no reason for compile to contact one.
    let e = r.run("compile /work/plaat.burnburn --profile /work/bench.json --out /work/q.rd --allow-machine-contact");
    assert!(e.is_ok(), "the switch is harmless on compile: {e:?}");
    assert_eq!(r.connector.connects.load(Ordering::SeqCst), 0);
    assert!(r.connector.peer.sent().is_empty());
}

// ---- machine actions: explicit target and intent ----------------------------------------------

fn upload_line(job: &str, extra: &str) -> String {
    format!("{UP} {job} --profile /work/bench.json --peer 127.0.0.1:50200 --allow-machine-contact {extra}")
}

#[test]
fn an_upload_without_a_named_target_machine_or_intent_contacts_nothing() {
    let r = rig();
    let job = compiled_job(&r);
    for line in
        [upload_line(&job, ""), upload_line(&job, "--machine ruida-bench-1"), upload_line(&job, "--intent upload")]
    {
        let e = r.run(&line).unwrap_err();
        assert!(matches!(e, CliError::Usage(_)), "{line}: {e:?}");
        assert_eq!(e.exit_code(), 2);
    }
    let start =
        "start --history /h --execution 1 --profile /work/bench.json --peer 127.0.0.1:50200 --allow-machine-contact";
    for extra in ["", "--machine ruida-bench-1", "--intent start"] {
        let e = r.run(&format!("{start} {extra}")).unwrap_err();
        assert!(matches!(e, CliError::Usage(_)), "{extra}: {e:?}");
    }
    assert_eq!(r.connector.connects.load(Ordering::SeqCst), 0, "nothing may be contacted");
}

#[test]
fn the_intent_must_match_the_command_and_the_machine_must_be_the_profiles() {
    let r = rig();
    let job = compiled_job(&r);
    // `start` intent on `upload`, and the other way round.
    let e = r.run(&upload_line(&job, "--machine ruida-bench-1 --intent start")).unwrap_err();
    assert!(matches!(e, CliError::Usage(_)), "{e:?}");
    let e = r
        .run("start --history /h --execution 1 --profile /work/bench.json --peer 127.0.0.1:50200 --allow-machine-contact --machine ruida-bench-1 --intent upload")
        .unwrap_err();
    assert!(matches!(e, CliError::Usage(_)), "{e:?}");
    // A target other than the machine the profile describes.
    let e = r.run(&upload_line(&job, "--machine ruida-andere --intent upload")).unwrap_err();
    assert_eq!(e, CliError::MachineTargetMismatch { expected: "ruida-bench-1".into(), given: "ruida-andere".into() });
    assert_eq!(e.exit_code(), 2);
    assert_eq!(r.connector.connects.load(Ordering::SeqCst), 0);
}

#[test]
fn a_named_target_and_intent_still_need_the_machine_contact_switch() {
    let r = rig();
    let job = compiled_job(&r);
    let line =
        format!("{UP} {job} --profile /work/bench.json --peer 127.0.0.1:50200 --machine ruida-bench-1 --intent upload");
    assert_eq!(r.run(&line), Err(CliError::MachineContactNotAllowed));
    assert_eq!(r.connector.connects.load(Ordering::SeqCst), 0);
}

// ---- options ----------------------------------------------------------------------------------

#[test]
fn an_unknown_option_is_refused_and_writes_nothing() {
    let r = rig();
    with_project(&r);
    for extra in ["--frobnicate 1", "--and-start now", "--upload yes"] {
        let e = r
            .run(&format!("compile /work/plaat.burnburn --profile /work/bench.json --out /work/x.rd {extra}"))
            .unwrap_err();
        assert!(matches!(e, CliError::Usage(_)), "{extra}: {e:?}");
    }
    assert!(r.file("/work/x.rd").is_none());
}

#[test]
fn the_variant_option_is_refused_with_the_reason_instead_of_being_ignored() {
    let r = rig();
    with_project(&r);
    let e = r.run("compile /work/plaat.burnburn --profile /work/bench.json --out /work/x.rd --variant A").unwrap_err();
    match &e {
        CliError::Usage(m) => assert!(m.contains("--variant") && m.contains("not available"), "{m}"),
        other => panic!("{other:?}"),
    }
    assert!(r.file("/work/x.rd").is_none(), "no RD for a variant that was not honoured");
}
