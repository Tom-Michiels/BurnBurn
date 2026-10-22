//! G8-editor P06: automatic distribution of part instances over plates. Instances are rows on a
//! plate (shelves): the plan is worked out by hand for each case, and the invariants (everything
//! placed or reported once, never more area than the plate) hold for generated inputs.

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers

use burnburn_document::structure::{Footprint, PartInstanceId, PlateId, PlateRoom, distribute_over_plates};
use burnburn_types::StableId;

fn plate(n: u64, w: f64, h: f64) -> PlateRoom {
    PlateRoom { id: PlateId::from_raw(n), width: w, height: h }
}

fn item(n: u64, w: f64, h: f64, on: Option<u64>) -> Footprint {
    Footprint { id: PartInstanceId::from_raw(n), width: w, height: h, plate: on.map(PlateId::from_raw) }
}

fn on(d: &burnburn_document::structure::Distribution, n: u64) -> Option<u64> {
    d.assigned.iter().find(|(i, _)| i.raw() == n).map(|(_, p)| p.raw())
}

#[test]
fn rows_of_equal_parts_fill_a_plate_row_by_row() {
    // 100 x 50: two 40 x 20 parts per row (80 <= 100), two rows (40 <= 50); the fifth finds no room.
    let plates = [plate(1, 100.0, 50.0)];
    let four: Vec<_> = (1..=4).map(|n| item(n, 40.0, 20.0, None)).collect();
    let d = distribute_over_plates(&plates, &four, 0.0);
    assert!(d.unplaced.is_empty() && d.assigned.len() == 4, "{d:?}");
    assert!((1..=4).all(|n| on(&d, n) == Some(1)));
    let five: Vec<_> = (1..=5).map(|n| item(n, 40.0, 20.0, None)).collect();
    let d = distribute_over_plates(&plates, &five, 0.0);
    assert_eq!(d.unplaced.len(), 1, "a third row would reach 60 > 50: {d:?}");
    assert_eq!(d.assigned.len(), 4);
}

#[test]
fn a_gap_between_parts_and_rows_is_kept() {
    let plates = [plate(1, 100.0, 50.0)];
    // 40 + 5 + 40 = 85 <= 100, rows at 0 and 25 (20 + 5): the second row ends at 45 <= 50.
    let four: Vec<_> = (1..=4).map(|n| item(n, 40.0, 20.0, None)).collect();
    assert!(distribute_over_plates(&plates, &four, 5.0).unplaced.is_empty());
    // 40 + 10 + 40 = 90 fits, the second row starts at 30 and ends at 50: just fits.
    assert!(distribute_over_plates(&plates, &four, 10.0).unplaced.is_empty());
    // 40 + 12 + 40 = 92 fits but the second row would end at 20 + 12 + 20 = 52 > 50: two are left over.
    assert_eq!(distribute_over_plates(&plates, &four, 12.0).unplaced.len(), 2);
    // A gap of 25: only one part per row (40 + 25 + 40 = 105 > 100), and two rows need 65 > 50.
    assert_eq!(distribute_over_plates(&plates, &four, 25.0).unplaced.len(), 3);
}

#[test]
fn a_part_that_only_fits_turned_is_assigned_anyway() {
    let plates = [plate(1, 100.0, 50.0)];
    let d = distribute_over_plates(&plates, &[item(1, 20.0, 80.0, None)], 0.0);
    assert_eq!(on(&d, 1), Some(1), "80 x 20 fits where 20 x 80 does not");
    let d = distribute_over_plates(&plates, &[item(1, 120.0, 80.0, None)], 0.0);
    assert_eq!(d.unplaced.len(), 1, "too big either way");
}

#[test]
fn the_next_plate_takes_what_the_first_cannot() {
    let plates = [plate(1, 100.0, 50.0), plate(2, 100.0, 50.0)];
    let parts: Vec<_> = (1..=6).map(|n| item(n, 40.0, 20.0, None)).collect();
    let d = distribute_over_plates(&plates, &parts, 0.0);
    assert!(d.unplaced.is_empty());
    let first = (1..=6).filter(|n| on(&d, *n) == Some(1)).count();
    assert_eq!(first, 4, "plate 1 is filled first");
    assert_eq!((1..=6).filter(|n| on(&d, *n) == Some(2)).count(), 2);
}

#[test]
fn parts_already_on_a_plate_use_up_its_room_and_are_not_moved() {
    let plates = [plate(1, 100.0, 50.0), plate(2, 100.0, 50.0)];
    // Plate 1 already carries four 40 x 20 parts: its 50 mm are used up to 40 and a row of 20 no longer fits.
    let mut parts: Vec<_> = (1..=4).map(|n| item(n, 40.0, 20.0, Some(1))).collect();
    parts.push(item(5, 40.0, 20.0, None));
    let d = distribute_over_plates(&plates, &parts, 0.0);
    assert_eq!(on(&d, 5), Some(2));
    assert!((1..=4).all(|n| on(&d, n).is_none()), "the ones that were placed stay out of the plan");
}

#[test]
fn the_order_of_the_input_does_not_change_the_plan() {
    let plates = [plate(1, 100.0, 40.0), plate(2, 100.0, 40.0)];
    let parts = [
        item(1, 45.0, 10.0, None),
        item(2, 45.0, 10.0, None),
        item(3, 90.0, 30.0, None),
        item(4, 60.0, 25.0, None),
        item(5, 30.0, 25.0, None),
    ];
    let mut reversed = parts.to_vec();
    reversed.reverse();
    let sorted = |d: burnburn_document::structure::Distribution| {
        let mut a: Vec<_> = d.assigned.iter().map(|(i, p)| (i.raw(), p.raw())).collect();
        a.sort();
        let mut u: Vec<_> = d.unplaced.iter().map(|i| i.raw()).collect();
        u.sort();
        (a, u)
    };
    let forward = sorted(distribute_over_plates(&plates, &parts, 0.0));
    assert_eq!(forward, sorted(distribute_over_plates(&plates, &reversed, 0.0)));
    assert!(forward.1.is_empty(), "two plates of 100 x 40 take all of these: {forward:?}");
}

#[test]
fn invalid_sizes_and_no_plates_are_reported_not_assumed() {
    let parts = [item(1, 0.0, 10.0, None), item(2, f64::NAN, 10.0, None), item(3, 10.0, -1.0, None)];
    let d = distribute_over_plates(&[plate(1, 100.0, 100.0)], &parts, 0.0);
    assert_eq!(d.unplaced.len(), 3);
    let d = distribute_over_plates(&[], &[item(1, 10.0, 10.0, None)], 0.0);
    assert_eq!((d.assigned.len(), d.unplaced.len()), (0, 1));
    // A negative or non-finite gap is no gap at all.
    let d = distribute_over_plates(&[plate(1, 100.0, 100.0)], &[item(1, 10.0, 10.0, None)], f64::NAN);
    assert_eq!(on(&d, 1), Some(1));
}

#[test]
fn generated_inputs_keep_the_invariants() {
    // A fixed linear congruential sequence: deterministic, so a failure can be repeated.
    let mut state = 0x2545_f491_4f6c_dd1du64;
    let mut next = move |max: f64| {
        state = state.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        1.0 + ((state >> 33) as f64 / (1u64 << 31) as f64) * (max - 1.0)
    };
    for round in 0..200 {
        let plates: Vec<_> = (1..=3).map(|n| plate(n, next(200.0), next(120.0))).collect();
        let count = 1 + (next(12.0) as u64);
        let parts: Vec<_> = (1..=count).map(|n| item(n, next(60.0), next(60.0), None)).collect();
        let gap = next(6.0) - 1.0;
        let d = distribute_over_plates(&plates, &parts, gap);
        assert_eq!(d.assigned.len() + d.unplaced.len(), parts.len(), "round {round}: everything once");
        for p in &plates {
            let area: f64 = d
                .assigned
                .iter()
                .filter(|(_, to)| *to == p.id)
                .map(|(i, _)| {
                    let it = parts.iter().find(|x| x.id == *i).unwrap();
                    it.width * it.height
                })
                .sum();
            assert!(area <= p.width * p.height + 1e-9, "round {round}: {area} on {}x{}", p.width, p.height);
        }
        assert_eq!(d, distribute_over_plates(&plates, &parts, gap), "round {round}: deterministic");
    }
}
