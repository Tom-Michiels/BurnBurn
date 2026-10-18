//! 100k-segment benchmark (IM30, X04). Ignored by default; run with
//! `cargo test -p burnburn-cli --release --test bench -- --ignored --nocapture`.
//! Prints measured times; no budget is asserted before a first measurement exists.
#![allow(clippy::unwrap_used)]

mod common;

use std::time::Instant;

use burnburn_document::process::ProcessCommand;
use burnburn_document::{CommandPayload, DocumentCommand, DocumentSession, GeometryCommand};
use burnburn_geometry::{Point, Shape};
use burnburn_io::{Project, ProjectUuid, encode_container};
use burnburn_types::{Angle, DocumentId, RequestId, StableId};
use common::*;

#[test]
#[ignore = "benchmark: run explicitly with --ignored --nocapture"]
fn compile_and_lower_100k_segments() {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let mut ids = Vec::new();
    for i in 0..100 {
        let center = Point::new(50.0 + f64::from(i % 10) * 70.0, 50.0 + f64::from(i / 10) * 70.0);
        let shape =
            Shape::Polygon { center, radius: mm(30.0), sides: 1000, rotation: Angle::from_degrees(0.0).unwrap() };
        let payload = CommandPayload::Edit(GeometryCommand::CreateShape { shape, name: String::new() });
        let cmd =
            DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload };
        ids.push(s.apply(cmd).unwrap().changes.created[0]);
    }
    let payload = CommandPayload::Process(ProcessCommand::AddOperation { draft: cut_op(50.0), targets: ids });
    let cmd = DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload };
    s.apply(cmd).unwrap();
    let bytes = encode_container(&Project::new(ProjectUuid::from_bytes([9; 16]), (*s.snapshot()).clone())).unwrap();

    let r = rig();
    r.put("/work/big.burnburn", bytes);
    r.put("/work/bench.json", PROFILE_JSON.as_bytes().to_vec());
    let t = Instant::now();
    let out = r.run("compile /work/big.burnburn --profile /work/bench.json --out /work/big.rd").unwrap();
    let total = t.elapsed();
    let rd = r.file("/work/big.rd").unwrap();
    let inspect = r.run("inspect /work/big.rd").unwrap();
    let cuts: usize = inspect
        .stdout
        .lines()
        .find_map(|l| {
            l.strip_prefix("moves ").and_then(|l| l.split(" travel, ").nth(1)?.split(' ').next()?.parse().ok())
        })
        .unwrap();
    eprintln!("BENCH 100k: {cuts} cut records, {} RD bytes; load+compile+preflight+lower+write {:?}", rd.len(), total);
    eprintln!("{}", out.stdout);
    assert!(cuts >= 100_000, "expected ≥ 100k cut segments, got {cuts}");
}
