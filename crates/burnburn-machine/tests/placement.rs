//! Start-from modes, 3×3 job origin, "cut selected graphics" and "use selection origin"
//! (laser window, W1-L4a). Pure planning: no controller involved.
#![allow(clippy::unwrap_used, clippy::panic)]

use burnburn_job::{
    JobPlan, JobPlanBuilder, MachineFrame, MachinePoint, OperationSettings, OriginCorner, PlanEvent, PlanMode,
    PlanSource, PowerRatio, ProfileRevision,
};
use burnburn_machine::{
    CompileScope, JobAnchor, PlacementError, PlacementInput, PlacementSpec, Rect, ScopeError, ScopeObject, StartFrom,
    place_plan, resolve_scope,
};
use burnburn_types::{ContentHash, HashDomain, OperationId, RevisionId, StableId};

fn p(x: f64, y: f64) -> MachinePoint {
    MachinePoint::new(x, y).unwrap()
}

fn frame(origin: OriginCorner) -> MachineFrame {
    MachineFrame::new(300.0, 200.0, origin).unwrap()
}

/// A cut rectangle with machine-frame corners (10,20)–(50,40): 40 × 20 mm.
fn plan_on(frame: MachineFrame) -> JobPlan {
    let source = PlanSource {
        document_revision: RevisionId::INITIAL,
        source_hash: ContentHash::of(HashDomain::Plan, b"placement-test"),
        machine_profile: ProfileRevision {
            id: "bench".into(),
            revision: 1,
            content: ContentHash::of(HashDomain::Profile, b"bench"),
        },
        compiler_version: "test".into(),
    };
    let mut b = JobPlanBuilder::new(source, frame, p(0.0, 0.0));
    let s = b
        .add_settings(OperationSettings {
            mode: PlanMode::Line,
            speed_mm_s: 20.0,
            min_power: PowerRatio::new(0.5).unwrap(),
            max_power: PowerRatio::new(0.6).unwrap(),
            air_assist: false,
        })
        .unwrap();
    b.push(PlanEvent::OperationBoundary { operation: OperationId::from_raw(1), settings: s, pass: 0 }).unwrap();
    b.push(PlanEvent::Travel { to: p(10.0, 20.0) }).unwrap();
    for to in [p(50.0, 20.0), p(50.0, 40.0), p(10.0, 40.0), p(10.0, 20.0)] {
        b.push(PlanEvent::Cut { to, origin: None }).unwrap();
    }
    b.finish().unwrap()
}

fn bounds_of(plan: &JobPlan) -> ((f64, f64), (f64, f64)) {
    let b = plan.bounds().unwrap();
    ((b.min().x, b.min().y), (b.max().x, b.max().y))
}

/// The job's own bounds in bed view (y-up): where the 3×3 anchors are taken from.
fn job_rect() -> Rect {
    Rect::new(10.0, 20.0, 50.0, 40.0).unwrap()
}

const ANCHORS: [(JobAnchor, f64, f64); 9] = [
    // (anchor, fraction of width from the left, fraction of height from the bottom)
    (JobAnchor::TopLeft, 0.0, 1.0),
    (JobAnchor::TopCenter, 0.5, 1.0),
    (JobAnchor::TopRight, 1.0, 1.0),
    (JobAnchor::MiddleLeft, 0.0, 0.5),
    (JobAnchor::Center, 0.5, 0.5),
    (JobAnchor::MiddleRight, 1.0, 0.5),
    (JobAnchor::BottomLeft, 0.0, 0.0),
    (JobAnchor::BottomCenter, 0.5, 0.0),
    (JobAnchor::BottomRight, 1.0, 0.0),
];

fn input(frame: &MachineFrame, current: Option<MachinePoint>, user: Option<MachinePoint>) -> PlacementInput<'_> {
    PlacementInput { frame, current_position: current, user_origin: user }
}

#[test]
fn absolute_coordinates_keep_the_designed_position_for_every_anchor() {
    let f = frame(OriginCorner::BottomLeft);
    let plan = plan_on(f);
    for (anchor, ..) in ANCHORS {
        let spec = PlacementSpec { start_from: StartFrom::AbsoluteCoordinates, anchor };
        let placed = place_plan(&plan, &spec, job_rect(), &input(&f, None, None)).unwrap();
        assert_eq!(placed.plan, plan, "{anchor:?}: absolute coordinates must not move the job");
        assert_eq!(placed.delta_mm, (0.0, 0.0));
    }
}

#[test]
fn current_position_puts_each_3x3_anchor_of_the_job_on_the_laser() {
    let f = frame(OriginCorner::BottomLeft);
    let plan = plan_on(f);
    let laser = p(100.0, 100.0);
    for (anchor, fx, fy) in ANCHORS {
        let spec = PlacementSpec { start_from: StartFrom::CurrentPosition, anchor };
        let placed = place_plan(&plan, &spec, job_rect(), &input(&f, Some(laser), None)).unwrap();
        let ((x0, y0), (x1, y1)) = bounds_of(&placed.plan);
        // Independent oracle: the job is 40 × 20, so its min corner sits at laser - fraction × size.
        assert_eq!((x0, y0), (100.0 - fx * 40.0, 100.0 - fy * 20.0), "{anchor:?}");
        assert_eq!((x1 - x0, y1 - y0), (40.0, 20.0), "{anchor:?}: size must not change");
    }
}

#[test]
fn user_origin_is_its_own_reference_not_the_current_position() {
    let f = frame(OriginCorner::BottomLeft);
    let plan = plan_on(f);
    let spec = PlacementSpec { start_from: StartFrom::UserOrigin, anchor: JobAnchor::BottomLeft };
    let placed = place_plan(&plan, &spec, job_rect(), &input(&f, Some(p(200.0, 150.0)), Some(p(70.0, 60.0)))).unwrap();
    assert_eq!(bounds_of(&placed.plan).0, (70.0, 60.0));
    // The laser position is irrelevant (even unknown) when starting from the user origin.
    let placed = place_plan(&plan, &spec, job_rect(), &input(&f, None, Some(p(70.0, 60.0)))).unwrap();
    assert_eq!(bounds_of(&placed.plan).0, (70.0, 60.0));
}

#[test]
fn the_anchor_is_a_point_on_the_job_bounds_in_the_view_not_in_machine_axes() {
    // Machine origin top-right: view-left is machine max-x, view-top is machine min-y.
    let f = frame(OriginCorner::TopRight);
    let plan = plan_on(f);
    // Machine bounds (10,20)–(50,40) are bed-view x 250..290, y 160..180.
    let view = Rect::new(250.0, 160.0, 290.0, 180.0).unwrap();
    let spec = PlacementSpec { start_from: StartFrom::UserOrigin, anchor: JobAnchor::TopLeft };
    let placed = place_plan(&plan, &spec, view, &input(&f, None, Some(p(150.0, 100.0)))).unwrap();
    let ((_, y0), (x1, _)) = bounds_of(&placed.plan);
    assert_eq!((x1, y0), (150.0, 100.0), "visual top-left = machine (max x, min y)");
    // And the job moved by whole-job translation only.
    assert_eq!(placed.delta_mm, (100.0, 80.0));
}

#[test]
fn unknown_laser_position_is_refused_it_is_never_taken_as_zero() {
    let f = frame(OriginCorner::BottomLeft);
    let plan = plan_on(f);
    let spec = PlacementSpec { start_from: StartFrom::CurrentPosition, anchor: JobAnchor::Center };
    assert_eq!(place_plan(&plan, &spec, job_rect(), &input(&f, None, None)), Err(PlacementError::PositionUnknown));
    // A known position of exactly zero is a different thing and is accepted.
    let placed = place_plan(&plan, &spec, job_rect(), &input(&f, Some(p(60.0, 50.0)), None)).unwrap();
    assert_eq!(bounds_of(&placed.plan), ((40.0, 40.0), (80.0, 60.0)));
}

#[test]
fn start_from_user_origin_without_a_user_origin_is_refused() {
    let f = frame(OriginCorner::BottomLeft);
    let plan = plan_on(f);
    let spec = PlacementSpec { start_from: StartFrom::UserOrigin, anchor: JobAnchor::TopLeft };
    assert_eq!(
        place_plan(&plan, &spec, job_rect(), &input(&f, Some(p(1.0, 1.0)), None)),
        Err(PlacementError::UserOriginNotSet)
    );
}

#[test]
fn a_placement_that_leaves_the_bed_is_refused_with_the_resulting_bounds() {
    let f = frame(OriginCorner::BottomLeft);
    let plan = plan_on(f);
    let spec = PlacementSpec { start_from: StartFrom::CurrentPosition, anchor: JobAnchor::BottomLeft };
    // 40 mm wide job from x = 280 ends at 320 > 300.
    let err = place_plan(&plan, &spec, job_rect(), &input(&f, Some(p(280.0, 10.0)), None)).unwrap_err();
    match err {
        PlacementError::OutsideBed { min, max } => assert_eq!((min, max), ((280.0, 10.0), (320.0, 30.0))),
        other => panic!("{other:?}"),
    }
}

#[test]
fn placing_translates_every_event_and_leaves_the_source_plan_untouched() {
    let f = frame(OriginCorner::BottomLeft);
    let plan = plan_on(f);
    let before = plan.clone();
    let spec = PlacementSpec { start_from: StartFrom::CurrentPosition, anchor: JobAnchor::BottomLeft };
    let placed = place_plan(&plan, &spec, job_rect(), &input(&f, Some(p(110.0, 120.0)), None)).unwrap();
    assert_eq!(plan, before);
    assert_ne!(placed.plan.id(), plan.id(), "a moved job is a different plan revision");
    assert_eq!(placed.plan.settings(), plan.settings());
    assert_eq!(placed.delta_mm, (100.0, 100.0));
    for (a, b) in plan.events().iter().zip(placed.plan.events()) {
        match (a, b) {
            (PlanEvent::Travel { to: x }, PlanEvent::Travel { to: y })
            | (PlanEvent::Cut { to: x, .. }, PlanEvent::Cut { to: y, .. }) => {
                assert_eq!((y.x_mm() - x.x_mm(), y.y_mm() - x.y_mm()), (100.0, 100.0));
            }
            (PlanEvent::OperationBoundary { .. }, PlanEvent::OperationBoundary { .. }) => assert_eq!(a, b),
            other => panic!("event kinds diverged: {other:?}"),
        }
    }
    assert_eq!(plan.events().len(), placed.plan.events().len());
}

// ---- Cut Selected Graphics / Use Selection Origin: filters and origin are composed first ----

fn objects() -> Vec<ScopeObject> {
    vec![
        ScopeObject { id: 1, bounds: Rect::new(0.0, 0.0, 10.0, 10.0).unwrap() },
        ScopeObject { id: 2, bounds: Rect::new(40.0, 40.0, 60.0, 50.0).unwrap() },
        ScopeObject { id: 3, bounds: Rect::new(100.0, 0.0, 120.0, 30.0).unwrap() },
    ]
}

#[test]
fn cut_selected_includes_only_selected_objects_and_takes_their_bounds_as_origin() {
    let scope = CompileScope { cut_selected: true, use_selection_origin: false };
    let r = resolve_scope(&scope, &objects(), &[2, 3]).unwrap();
    assert_eq!(r.included, vec![2, 3]);
    assert_eq!(r.origin_bounds, Rect::new(40.0, 0.0, 120.0, 50.0).unwrap());
}

#[test]
fn without_cut_selected_everything_is_cut_and_the_origin_follows_all_objects() {
    let scope = CompileScope { cut_selected: false, use_selection_origin: false };
    let r = resolve_scope(&scope, &objects(), &[2]).unwrap();
    assert_eq!(r.included, vec![1, 2, 3]);
    assert_eq!(r.origin_bounds, Rect::new(0.0, 0.0, 120.0, 50.0).unwrap());
}

#[test]
fn use_selection_origin_places_by_the_selection_while_everything_is_still_cut() {
    let scope = CompileScope { cut_selected: false, use_selection_origin: true };
    let r = resolve_scope(&scope, &objects(), &[2]).unwrap();
    assert_eq!(r.included, vec![1, 2, 3], "selection origin does not filter output");
    assert_eq!(r.origin_bounds, Rect::new(40.0, 40.0, 60.0, 50.0).unwrap());
}

#[test]
fn selection_origin_with_cut_selected_is_the_filtered_set_and_selection_order_is_irrelevant() {
    let scope = CompileScope { cut_selected: true, use_selection_origin: true };
    let a = resolve_scope(&scope, &objects(), &[3, 1]).unwrap();
    let b = resolve_scope(&scope, &objects(), &[1, 3, 1]).unwrap();
    assert_eq!(a, b);
    assert_eq!(a.included, vec![1, 3], "document order, no duplicates");
}

#[test]
fn an_empty_selection_cannot_be_used_for_selection_filters() {
    for scope in [
        CompileScope { cut_selected: true, use_selection_origin: false },
        CompileScope { cut_selected: false, use_selection_origin: true },
    ] {
        assert_eq!(resolve_scope(&scope, &objects(), &[]), Err(ScopeError::NothingSelected), "{scope:?}");
    }
    // Without selection-based options an empty selection is fine.
    let plain = CompileScope { cut_selected: false, use_selection_origin: false };
    assert!(resolve_scope(&plain, &objects(), &[]).is_ok());
}

#[test]
fn selected_ids_that_are_not_in_the_document_are_an_error_not_silently_dropped() {
    let scope = CompileScope { cut_selected: true, use_selection_origin: false };
    assert_eq!(resolve_scope(&scope, &objects(), &[2, 99]), Err(ScopeError::UnknownObject(99)));
}

#[test]
fn a_document_without_objects_has_nothing_to_place() {
    let plain = CompileScope { cut_selected: false, use_selection_origin: false };
    assert_eq!(resolve_scope(&plain, &[], &[]), Err(ScopeError::NoObjects));
}
