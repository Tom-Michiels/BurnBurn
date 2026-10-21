//! X04: compile and nest are cancellable *while they run*, not only when the token is already
//! cancelled at the start. A second thread cancels shortly after the work began; the call must come
//! back with its typed `Cancelled` error and never with a partial result as `Ok`.
//!
//! The work size is calibrated: if a run finishes before the cancel arrives (fast machine), the
//! next, larger size is tried, so the test observes a real mid-run cancel without a time limit.

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers outside #[test] fns

mod common;

use std::time::Duration;

use burnburn_cam::nest::{
    NestError, NestParams, NestPart, PartKey, RotationSet, Stock, StockId, UtilisationBasis, nest,
};
use burnburn_cam::{CompileError, compile};
use burnburn_document::{DocumentSession, DocumentSnapshot, GeometryObject, GeometrySource, ObjectKind};
use burnburn_geometry::{Path, Point, ProductionTolerance, Rect};
use burnburn_types::{CancellationSource, CancellationToken, DocumentId, GeometryId, Length, StableId};
use common::*;

const SIZES: [usize; 4] = [1_500, 6_000, 24_000, 96_000];
const CANCEL_AFTER: Duration = Duration::from_millis(15);

/// Prepares an input of each size, then runs `work(input, token)` with a token that a second thread
/// cancels `CANCEL_AFTER` after the work started (preparation is outside that window). Returns the
/// first size at which the work reported "cancelled".
fn first_cancelled_size<I>(prepare: impl Fn(usize) -> I, work: impl Fn(&I, &CancellationToken) -> bool) -> usize {
    for size in SIZES {
        let input = prepare(size);
        let source = CancellationSource::new();
        let token = source.token();
        let cancelled = std::thread::scope(|scope| {
            scope.spawn(|| {
                std::thread::sleep(CANCEL_AFTER);
                source.cancel();
            });
            work(&input, &token)
        });
        if cancelled {
            return size;
        }
    }
    panic!("the work finished before the cancel for every size: it does not observe cancellation");
}

/// `n` closed squares in a grid, all cut by one line operation (containment ordering runs over all).
fn squares_document(n: usize) -> DocumentSession {
    let cols = (n as f64).sqrt().ceil() as usize;
    let objects: Vec<GeometryObject> = (0..n)
        .map(|i| {
            let (x, y) = (12.0 * (i % cols) as f64, 12.0 * (i / cols) as f64);
            let rect = Rect::new(Point::new(x, y), mm(8.0), mm(8.0)).unwrap();
            GeometryObject::new(
                GeometryId::from_raw(i as u64 + 1),
                String::new(),
                ObjectKind::Rectangle,
                GeometrySource::Drawn,
                Path::rectangle(rect),
            )
        })
        .collect();
    let snap = DocumentSnapshot::from_parts(objects, n as u64 + 1).unwrap();
    let mut s = DocumentSession::from_snapshot(DocumentId::from_raw(1), snap);
    let targets = (0..n).map(|i| GeometryId::from_raw(i as u64 + 1)).collect();
    add_op(&mut s, line_op("Snijden", 100.0, 20.0), targets);
    s
}

#[test]
fn a_compile_cancelled_while_running_returns_cancelled_and_no_plan() {
    let size = first_cancelled_size(
        |n| squares_document(n).snapshot(),
        |snapshot, token| match compile(snapshot, &profile(), &options(), token) {
            Err(CompileError::Cancelled) => true,
            Err(other) => panic!("unexpected compile error: {other}"),
            Ok(_) => false,
        },
    );
    println!("compile cancelled mid-run at {size} contours");
}

#[test]
fn a_nest_cancelled_while_running_returns_cancelled_and_no_placements() {
    let size = first_cancelled_size(
        |n| {
            let side = Length::from_mm(4.0).unwrap();
            let part = NestPart {
                key: PartKey(1),
                outline: Path::rectangle(Rect::new(Point::ORIGIN, side, side).unwrap()),
                count: n as u32,
                rotations: RotationSet::new(vec![0.0, 90.0]).unwrap(),
                grain_deg: None,
            };
            let edge = Length::from_mm(400.0).unwrap();
            let outline = Path::rectangle(Rect::new(Point::ORIGIN, edge, edge).unwrap());
            (part, Stock { id: StockId(1), outline, keep_out: vec![], grain_deg: None })
        },
        |(part, stock), token| {
            let params = NestParams {
                spacing: Length::from_mm(1.0).unwrap(),
                edge_margin: Length::from_mm(1.0).unwrap(),
                step: Length::from_mm(1.0).unwrap(),
                tolerance: ProductionTolerance::new(Length::from_mm(0.01).unwrap()).unwrap(),
                max_candidates: u64::MAX,
                basis: UtilisationBasis::StockArea,
            };
            match nest(std::slice::from_ref(part), std::slice::from_ref(stock), &params, token) {
                Err(NestError::Cancelled) => true,
                Err(other) => panic!("unexpected nest error: {other}"),
                Ok(_) => false,
            }
        },
    );
    println!("nest cancelled mid-run at {size} parts");
}
