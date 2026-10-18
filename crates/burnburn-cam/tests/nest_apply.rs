//! A nesting result becomes one document command on a plate (IM26 ↔ IM21): placements on the
//! chosen stock map to part instances by part key and copy number.

#![allow(clippy::unwrap_used, clippy::panic)]

use std::collections::BTreeMap;

use burnburn_cam::nest::{
    NestError, NestParams, NestPart, PartKey, RotationSet, Stock, StockId, UtilisationBasis, apply_nest_command, nest,
};
use burnburn_document::structure::{PartInstanceId, PlateId, StructureCommand};
use burnburn_geometry::{Path, Point, ProductionTolerance, Rect};
use burnburn_types::{CancellationToken, Length, StableId};

fn mm(v: f64) -> Length {
    Length::from_mm(v).unwrap()
}

fn rect(w: f64, h: f64) -> Path {
    Path::rectangle(Rect::new(Point::new(0.0, 0.0), mm(w), mm(h)).unwrap())
}

#[test]
fn nest_result_becomes_one_apply_nest_command_for_its_stock() {
    let parts = vec![NestPart {
        key: PartKey(3),
        outline: rect(20.0, 10.0),
        count: 2,
        rotations: RotationSet::new(vec![0.0]).unwrap(),
        grain_deg: None,
    }];
    let stock = vec![Stock { id: StockId(1), outline: rect(100.0, 50.0), keep_out: vec![], grain_deg: None }];
    let params = NestParams {
        spacing: mm(2.0),
        edge_margin: mm(1.0),
        step: mm(1.0),
        tolerance: ProductionTolerance::new(mm(0.01)).unwrap(),
        max_candidates: 1_000_000,
        basis: UtilisationBasis::StockArea,
    };
    let result = nest(&parts, &stock, &params, &CancellationToken::never()).unwrap();
    assert_eq!(result.placements.len(), 2);
    let ids = [PartInstanceId::from_raw(10), PartInstanceId::from_raw(11)];
    let mut instances = BTreeMap::new();
    instances.insert(PartKey(3), ids.to_vec());
    let plate = PlateId::from_raw(20);
    let cmd = apply_nest_command(&result, StockId(1), plate, &instances).unwrap();
    let StructureCommand::ApplyNest { plate: p, placements } = cmd else { panic!("ApplyNest") };
    assert_eq!(p, plate);
    let want: Vec<_> = result.placements.iter().map(|pl| (ids[pl.copy as usize], pl.transform)).collect();
    assert_eq!(placements, want);
    // Another stock has no placements; a missing instance is an error, never a guess.
    assert!(matches!(apply_nest_command(&result, StockId(2), plate, &instances), Err(NestError::InvalidInput { .. })));
    instances.insert(PartKey(3), ids[..1].to_vec());
    assert!(matches!(apply_nest_command(&result, StockId(1), plate, &instances), Err(NestError::InvalidInput { .. })));
}
