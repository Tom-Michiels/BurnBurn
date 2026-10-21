//! W4-L8b, headless: the panel description the Win32 layer renders (ids, layout, in-place updates).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_windows::host_panels::ids::{self, RANGE};
use burnburn_windows::host_panels::{
    ControlSpec, Family, Metrics, PanelSpec, PanelValues, Row, SpecDiff, diff_specs, layout,
};

const ALL: [Family; 5] = [Family::Laser, Family::Image, Family::Files, Family::Camera, Family::App];

#[test]
fn every_id_block_lies_in_the_lane_range_and_blocks_do_not_overlap() {
    let mut ends = Vec::new();
    for family in ALL {
        let menu = ids::menu_base(family);
        let control = ids::control_base(family);
        assert!(RANGE.contains(&menu) && RANGE.contains(&(menu + ids::MENU_BLOCK - 1)), "{family:?}");
        assert!(RANGE.contains(&control) && RANGE.contains(&(control + ids::CONTROL_BLOCK - 1)), "{family:?}");
        ends.push((menu, menu + ids::MENU_BLOCK));
        ends.push((control, control + ids::CONTROL_BLOCK));
    }
    ends.sort();
    for pair in ends.windows(2) {
        assert!(pair[0].1 <= pair[1].0, "blocks overlap: {pair:?}");
    }
}

#[test]
fn an_id_maps_back_to_its_family_and_a_foreign_id_to_nothing() {
    for family in ALL {
        assert_eq!(ids::control_owner(ids::control_base(family) + 7), Some((family, 7)));
        assert_eq!(ids::menu_owner(ids::menu_base(family) + 3), Some((family, 3)));
    }
    // The existing menu ids (100..) and ids past the control range are not ours to decode.
    assert_eq!(ids::menu_owner(101), None);
    assert_eq!(ids::control_owner(8000), None);
    assert_eq!(ids::control_owner(ids::CONTROL_BASE - 1), None);
}

fn two_row_spec() -> PanelSpec {
    PanelSpec {
        title: "t".into(),
        rows: vec![
            Row::of(vec![ControlSpec::label(1, "a").weighted(1), ControlSpec::edit(2, "b", true).weighted(3)]),
            Row::one(ControlSpec::report(3, 4, "lines")),
            Row::one(ControlSpec::bitmap(4, 90, None)),
        ],
    }
}

#[test]
fn controls_of_a_row_share_the_width_by_weight_and_rows_stack() {
    let m = Metrics { width: 400, margin: 10, gap: 6, line: 24, text_line: 16 };
    let (places, height) = layout(&two_row_spec(), &m);
    let get = |id| *places.iter().find(|p| p.id == id).expect("placed");
    // 400 - 2 * 10 margin - 6 gap = 374 pixels for weights 1:3.
    let (a, b) = (get(1), get(2));
    assert_eq!((a.x, a.y, a.h), (10, 10, 24));
    assert_eq!(a.w + b.w, 374);
    assert!(b.w >= 3 * a.w - 3 && b.w <= 3 * a.w + 3, "{a:?} {b:?}");
    assert_eq!(b.x, a.x + a.w + 6);
    // Row 2 starts a gap below row 1; a report is as tall as its lines (4 * 16 + padding).
    let report = get(3);
    assert_eq!((report.x, report.w), (10, 380));
    assert_eq!(report.y, 10 + 24 + 6);
    assert!(report.h >= 4 * 16 && report.h <= 4 * 16 + 12, "{report:?}");
    // A bitmap has its own fixed height.
    let bitmap = get(4);
    assert_eq!(bitmap.h, 90);
    assert_eq!(bitmap.y, report.y + report.h + 6);
    // The total includes the bottom margin.
    assert_eq!(height, bitmap.y + bitmap.h + 10);
}

#[test]
fn an_empty_spec_needs_only_its_margins() {
    let (places, height) = layout(&PanelSpec::default(), &Metrics::base(300));
    assert!(places.is_empty());
    assert_eq!(height, 20);
}

#[test]
fn an_unchanged_spec_is_the_same_and_a_changed_value_patches_only_that_control() {
    let old = two_row_spec();
    assert_eq!(diff_specs(&old, &old.clone()), SpecDiff::Same);

    let mut new = old.clone();
    new.rows[0].controls[1].text = "changed".into();
    new.rows[1].controls[0].enabled = false;
    new.rows[1].controls[0].tooltip = "waarom niet".into();
    match diff_specs(&old, &new) {
        SpecDiff::Patch(patches) => {
            assert_eq!(patches.len(), 2, "{patches:?}");
            let edit = patches.iter().find(|p| p.id == 2).unwrap();
            assert_eq!(edit.text.as_deref(), Some("changed"));
            assert_eq!(edit.enabled, None);
            let report = patches.iter().find(|p| p.id == 3).unwrap();
            assert_eq!(report.enabled, Some(false));
            assert_eq!(report.tooltip.as_deref(), Some("waarom niet"));
            assert_eq!(report.text, None);
        }
        other => panic!("expected a patch, got {other:?}"),
    }
}

#[test]
fn new_items_for_a_list_and_a_new_check_state_are_patches_but_a_new_control_is_a_rebuild() {
    let old = PanelSpec {
        title: String::new(),
        rows: vec![Row::of(vec![
            ControlSpec::list(1, 3, vec!["x".into()], Some(0), true),
            ControlSpec::check(2, "c", false, true),
        ])],
    };
    let mut items = old.clone();
    items.rows[0].controls[0] =
        ControlSpec::list(1, 3, vec!["x".into(), "y".into()], Some(1), true);
    items.rows[0].controls[1] = ControlSpec::check(2, "c", true, true);
    match diff_specs(&old, &items) {
        SpecDiff::Patch(p) => {
            assert_eq!(p.iter().find(|x| x.id == 1).unwrap().items, Some((vec!["x".into(), "y".into()], Some(1))));
            assert_eq!(p.iter().find(|x| x.id == 2).unwrap().checked, Some(true));
        }
        other => panic!("{other:?}"),
    }

    let mut other_kind = old.clone();
    other_kind.rows[0].controls[1] = ControlSpec::button(2, "c", true);
    assert_eq!(diff_specs(&old, &other_kind), SpecDiff::Rebuild);
    let mut extra = old.clone();
    extra.rows.push(Row::one(ControlSpec::label(9, "new")));
    assert_eq!(diff_specs(&old, &extra), SpecDiff::Rebuild);
    let mut taller = old.clone();
    taller.rows[0].controls[0] = ControlSpec::list(1, 6, vec!["x".into()], Some(0), true);
    assert_eq!(diff_specs(&old, &taller), SpecDiff::Rebuild, "a different height needs a new layout");
}

#[test]
fn panel_values_read_missing_controls_as_empty() {
    let mut v = PanelValues::default();
    v.texts.insert(5, "12,5".into());
    v.selection.insert(6, Some(2));
    v.selection.insert(7, None);
    assert_eq!(v.text(5), "12,5");
    assert_eq!(v.text(99), "");
    assert!(!v.checked(99));
    assert_eq!((v.selected(6), v.selected(7), v.selected(99)), (2, -1, -1));
}
