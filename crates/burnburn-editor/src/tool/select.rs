//! Selection tool: click, Shift/Cmd toggle, Alt-click cycling through overlapping objects (E03),
//! rubber band, and move/scale/rotate drags through handles. Each finished drag is one
//! `EditCommit::Transform` (one history entry); Esc or a pointer cancel commits nothing.

use burnburn_geometry::{Affine, Bounds, Point, Vector};
use burnburn_types::{Angle, GeometryId};

use super::{
    CLICK_TOLERANCE_PT, DRAG_THRESHOLD_PT, EditCommit, HANDLE_RADIUS_PT, ROTATE_HANDLE_OFFSET_PT, Tool, ToolContext,
    ToolPreview, ToolStep, TransformKind, is_escape, tool_pointer,
};
use crate::input::{CanvasInput, PointerEvent, PointerPhase};
use crate::selection::{SelectionChange, hit_test, next_in_cycle, objects_inside};
use crate::snap::SnapCandidate;

/// Selection handles around the selection bounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Handle {
    BottomLeft,
    BottomRight,
    TopRight,
    TopLeft,
    /// Above the top edge's midpoint.
    Rotate,
}

/// Handle positions (world mm) for selection bounds at the given zoom.
pub fn selection_handles(bounds: Bounds, mm_per_point: f64) -> Vec<(Handle, Point)> {
    let (min, max) = (bounds.min(), bounds.max());
    vec![
        (Handle::BottomLeft, min),
        (Handle::BottomRight, Point::new(max.x, min.y)),
        (Handle::TopRight, max),
        (Handle::TopLeft, Point::new(min.x, max.y)),
        (Handle::Rotate, Point::new((min.x + max.x) / 2.0, max.y + ROTATE_HANDLE_OFFSET_PT * mm_per_point)),
    ]
}

/// Rotation steps with Shift held.
const ROTATE_STEP_DEG: f64 = 15.0;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Mode {
    Rubber { extend: bool, clear_on_click: bool },
    Move,
    Scale { handle_at: Point, fixed: Point },
    Rotate { center: Point },
}

#[derive(Debug, Clone, PartialEq)]
struct Drag {
    press: Point,
    targets: Vec<GeometryId>,
    mode: Mode,
    /// Past the drag threshold: a real drag, not a click.
    live: bool,
    current: Point,
    transform: Affine,
    /// Shift/Cmd-press on a selected object: a click toggles it off, a drag moves the selection.
    toggle_on_click: Option<GeometryId>,
}

#[derive(Debug, Clone, Default)]
pub struct SelectTool {
    drag: Option<Drag>,
    snap: Option<SnapCandidate>,
}

impl SelectTool {
    fn start(&mut self, press: Point, targets: Vec<GeometryId>, mode: Mode) {
        self.drag = Some(Drag {
            press,
            targets,
            mode,
            live: false,
            current: press,
            transform: Affine::IDENTITY,
            toggle_on_click: None,
        });
    }

    fn press(&mut self, ev: &PointerEvent, ctx: &ToolContext<'_>) -> ToolStep {
        let at = ev.position;
        if !ev.modifiers.alt
            && let Some(bounds) = ctx.selection.bounds(ctx.document)
        {
            let radius = ctx.tolerance_mm(HANDLE_RADIUS_PT);
            let picked = selection_handles(bounds, ctx.mm_per_point)
                .into_iter()
                .map(|(h, p)| (p.distance(at), h, p))
                .filter(|(d, ..)| *d <= radius)
                .min_by(|a, b| a.0.total_cmp(&b.0));
            if let Some((_, handle, p)) = picked {
                let (min, max) = (bounds.min(), bounds.max());
                let opposite = match handle {
                    Handle::BottomLeft => max,
                    Handle::BottomRight => Point::new(min.x, max.y),
                    Handle::TopRight => min,
                    Handle::TopLeft => Point::new(max.x, min.y),
                    Handle::Rotate => bounds.center(),
                };
                let mode = match handle {
                    Handle::Rotate => Mode::Rotate { center: bounds.center() },
                    _ if ev.modifiers.alt => Mode::Scale { handle_at: p, fixed: bounds.center() },
                    _ => Mode::Scale { handle_at: p, fixed: opposite },
                };
                self.start(at, ctx.selection.ids().to_vec(), mode);
                return ToolStep::Nothing;
            }
        }
        let hits = hit_test(ctx.document, at, ctx.tolerance_mm(CLICK_TOLERANCE_PT));
        let extend = ev.modifiers.shift || ev.modifiers.command;
        if ev.modifiers.alt
            && let Some((next, _)) = next_in_cycle(&hits, ctx.selection)
        {
            return ToolStep::Select(SelectionChange::Replace(vec![next]));
        }
        match hits.first().copied() {
            Some(top) if extend && ctx.selection.contains(top) => {
                self.start(at, ctx.selection.ids().to_vec(), Mode::Move);
                if let Some(d) = self.drag.as_mut() {
                    d.toggle_on_click = Some(top);
                }
                ToolStep::Nothing
            }
            Some(top) if extend => {
                let mut targets = ctx.selection.ids().to_vec();
                targets.push(top);
                self.start(at, targets, Mode::Move);
                ToolStep::Select(SelectionChange::Toggle(top))
            }
            Some(top) if ctx.selection.contains(top) => {
                self.start(at, ctx.selection.ids().to_vec(), Mode::Move);
                ToolStep::Nothing
            }
            Some(top) => {
                self.start(at, vec![top], Mode::Move);
                ToolStep::Select(SelectionChange::Replace(vec![top]))
            }
            None => {
                let clear_on_click = !extend && !ctx.selection.is_empty();
                self.start(at, Vec::new(), Mode::Rubber { extend, clear_on_click });
                ToolStep::Nothing
            }
        }
    }

    fn drag_to(&mut self, ev: &PointerEvent, ctx: &ToolContext<'_>) -> ToolStep {
        let Some(drag) = self.drag.as_mut() else { return ToolStep::Nothing };
        if !drag.live && (ev.position - drag.press).length() < ctx.tolerance_mm(DRAG_THRESHOLD_PT) {
            return ToolStep::Nothing;
        }
        drag.live = true;
        drag.current = ev.position;
        let shift = ev.modifiers.shift;
        let transform = match drag.mode {
            Mode::Rubber { .. } => return ToolStep::PreviewChanged,
            Mode::Move => {
                let (at, snap) = ctx.snapped(ev, &drag.targets);
                self.snap = snap;
                let mut d = at - drag.press;
                if shift {
                    if d.x.abs() >= d.y.abs() { d.y = 0.0 } else { d.x = 0.0 }
                }
                Affine::translate(d).ok()
            }
            Mode::Scale { handle_at, fixed } => {
                let (at, snap) = ctx.snapped(ev, &drag.targets);
                self.snap = snap;
                let factor = |cur: f64, h: f64, f: f64| if h == f { 1.0 } else { (cur - f) / (h - f) };
                let (mut sx, mut sy) = (factor(at.x, handle_at.x, fixed.x), factor(at.y, handle_at.y, fixed.y));
                if shift {
                    let s = sx.abs().max(sy.abs());
                    sx = s.copysign(sx);
                    sy = s.copysign(sy);
                }
                // A collapsed object cannot be scaled back; keep the last valid preview.
                if sx.abs() < 1e-9 || sy.abs() < 1e-9 { None } else { Affine::scale_about(fixed, sx, sy).ok() }
            }
            Mode::Rotate { center } => {
                let angle = |p: Point| (p.y - center.y).atan2(p.x - center.x);
                let mut deg = (angle(ev.position) - angle(drag.press)).to_degrees();
                if shift {
                    deg = (deg / ROTATE_STEP_DEG).round() * ROTATE_STEP_DEG;
                }
                rotation_about(center, deg)
            }
        };
        if let Some(t) = transform {
            drag.transform = t;
        }
        ToolStep::PreviewChanged
    }

    fn release(&mut self, ev: &PointerEvent, ctx: &ToolContext<'_>) -> ToolStep {
        // The release position is the final drag position.
        self.drag_to(ev, ctx);
        self.snap = None;
        let Some(drag) = self.drag.take() else { return ToolStep::Nothing };
        match (drag.mode, drag.live) {
            (Mode::Rubber { clear_on_click: true, .. }, false) => ToolStep::Select(SelectionChange::Clear),
            (Mode::Move, false) if drag.toggle_on_click.is_some() => {
                drag.toggle_on_click.map_or(ToolStep::Nothing, |id| ToolStep::Select(SelectionChange::Toggle(id)))
            }
            (_, false) => ToolStep::Nothing,
            (Mode::Rubber { extend, .. }, true) => {
                let ids = objects_inside(ctx.document, Bounds::from_corners(drag.press, drag.current));
                if extend {
                    ToolStep::Select(SelectionChange::Extend(ids))
                } else {
                    ToolStep::Select(SelectionChange::Replace(ids))
                }
            }
            (mode, true) => {
                let kind = match mode {
                    Mode::Scale { .. } => TransformKind::Scale,
                    Mode::Rotate { .. } => TransformKind::Rotate,
                    _ => TransformKind::Move,
                };
                if drag.transform == Affine::IDENTITY {
                    return ToolStep::Nothing;
                }
                ToolStep::Commit(EditCommit::Transform { targets: drag.targets, transform: drag.transform, kind })
            }
        }
    }
}

fn rotation_about(center: Point, degrees: f64) -> Option<Affine> {
    let to_origin = Affine::translate(Vector::new(-center.x, -center.y)).ok()?;
    let back = Affine::translate(center.to_vector()).ok()?;
    Some(to_origin.then(&Affine::rotate(Angle::from_degrees(degrees).ok()?)).then(&back))
}

impl Tool for SelectTool {
    fn input(&mut self, input: &CanvasInput, ctx: &ToolContext<'_>) -> ToolStep {
        if is_escape(input) {
            return self.cancel();
        }
        let Some(ev) = tool_pointer(input) else { return ToolStep::Nothing };
        match ev.phase {
            PointerPhase::Down => self.press(ev, ctx),
            PointerPhase::Move => self.drag_to(ev, ctx),
            PointerPhase::Up => self.release(ev, ctx),
            PointerPhase::Cancel => self.cancel(),
        }
    }

    fn cancel(&mut self) -> ToolStep {
        self.snap = None;
        match self.drag.take() {
            Some(d) if d.live => ToolStep::Cancelled,
            _ => ToolStep::Nothing,
        }
    }

    fn preview(&self) -> Option<ToolPreview> {
        let d = self.drag.as_ref().filter(|d| d.live)?;
        let kind = match d.mode {
            Mode::Rubber { .. } => {
                return Some(ToolPreview::RubberBand { bounds: Bounds::from_corners(d.press, d.current) });
            }
            Mode::Move => TransformKind::Move,
            Mode::Scale { .. } => TransformKind::Scale,
            Mode::Rotate { .. } => TransformKind::Rotate,
        };
        Some(ToolPreview::Transform { targets: d.targets.clone(), transform: d.transform, kind })
    }

    fn snap(&self) -> Option<SnapCandidate> {
        self.snap
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::CanvasKey;
    use crate::tool::fixture::*;
    use crate::tool::{EditCommit, ROTATE_HANDLE_OFFSET_PT, TransformKind};
    use burnburn_types::StableId;

    fn id(raw: u64) -> GeometryId {
        GeometryId::from_raw(raw)
    }

    fn press(tool: &mut SelectTool, f: &mut Fixture, x: f64, y: f64, mods: crate::input::Modifiers) -> ToolStep {
        let step = tool.input(&at(PointerPhase::Down, x, y, mods), &f.ctx());
        f.follow(&step);
        step
    }

    fn drag(
        tool: &mut SelectTool,
        f: &mut Fixture,
        from: (f64, f64),
        to: (f64, f64),
        mods: crate::input::Modifiers,
    ) -> ToolStep {
        press(tool, f, from.0, from.1, mods);
        tool.input(&at(PointerPhase::Move, (from.0 + to.0) / 2.0, (from.1 + to.1) / 2.0, mods), &f.ctx());
        tool.input(&at(PointerPhase::Move, to.0, to.1, mods), &f.ctx());
        let step = tool.input(&at(PointerPhase::Up, to.0, to.1, mods), &f.ctx());
        f.follow(&step);
        step
    }

    fn transform(step: &ToolStep) -> Option<(Vec<GeometryId>, Affine, TransformKind)> {
        match step {
            ToolStep::Commit(EditCommit::Transform { targets, transform, kind }) => {
                Some((targets.clone(), *transform, *kind))
            }
            _ => None,
        }
    }

    fn maps(m: &Affine, from: (f64, f64), to: (f64, f64)) -> bool {
        close(m.apply(Point::new(from.0, from.1)), Point::new(to.0, to.1))
    }

    #[test]
    fn select_click_replaces_extends_and_clears() {
        let mut f = Fixture::new(vec![rect(1, 0.0, 0.0, 10.0, 10.0), rect(2, 50.0, 0.0, 10.0, 10.0)]);
        let mut tool = SelectTool::default();
        assert_eq!(press(&mut tool, &mut f, 5.0, 5.0, none()), ToolStep::Select(SelectionChange::Replace(vec![id(1)])));
        tool.input(&at(PointerPhase::Up, 5.0, 5.0, none()), &f.ctx());
        assert_eq!(press(&mut tool, &mut f, 55.0, 5.0, command()), ToolStep::Select(SelectionChange::Toggle(id(2))));
        tool.input(&at(PointerPhase::Up, 55.0, 5.0, none()), &f.ctx());
        let step = press(&mut tool, &mut f, 30.0, 30.0, none());
        assert_eq!(
            tool.input(&at(PointerPhase::Up, 30.0, 30.0, none()), &f.ctx()),
            ToolStep::Select(SelectionChange::Clear),
            "{step:?}"
        );
    }

    /// Shift-click on a selected object removes it; Shift-drag on it moves the selection.
    #[test]
    fn shift_click_on_a_selected_object_toggles_it_off() {
        let mut f = Fixture::new(vec![rect(1, 0.0, 0.0, 10.0, 10.0), rect(2, 50.0, 0.0, 10.0, 10.0)]);
        f.select(&[1, 2]);
        let mut tool = SelectTool::default();
        assert_eq!(press(&mut tool, &mut f, 55.0, 5.0, shift()), ToolStep::Nothing, "deferred until release");
        assert_eq!(
            tool.input(&at(PointerPhase::Up, 55.0, 5.0, shift()), &f.ctx()),
            ToolStep::Select(SelectionChange::Toggle(id(2)))
        );
    }

    #[test]
    fn click_tolerance_is_in_screen_points_not_millimetres() {
        // 2 mm right of the edge: 4 pt at 0.5 mm/pt (hit), 16 pt at 0.125 mm/pt (miss).
        let mut f = Fixture::new(vec![rect(1, 0.0, 0.0, 10.0, 10.0)]);
        let mut tool = SelectTool::default();
        assert_eq!(
            press(&mut tool, &mut f, 12.0, 5.0, none()),
            ToolStep::Select(SelectionChange::Replace(vec![id(1)]))
        );
        tool.input(&at(PointerPhase::Up, 12.0, 5.0, none()), &f.ctx());
        let mut f = Fixture::new(vec![rect(1, 0.0, 0.0, 10.0, 10.0)]);
        f.mm_per_point = 0.125;
        assert_eq!(press(&mut tool, &mut f, 12.0, 5.0, none()), ToolStep::Nothing);
    }

    /// E03 / B2: Alt-click at the same place walks through the overlapping candidates.
    #[test]
    fn alt_click_cycles_overlapping_objects() {
        let mut f = Fixture::new(vec![
            rect(1, 0.0, 0.0, 50.0, 50.0),
            rect(2, 10.0, 10.0, 50.0, 50.0),
            rect(3, 20.0, 20.0, 50.0, 50.0),
        ]);
        let mut tool = SelectTool::default();
        let mut picked = Vec::new();
        for _ in 0..4 {
            match press(&mut tool, &mut f, 30.0, 30.0, alt()) {
                ToolStep::Select(SelectionChange::Replace(ids)) => picked.push(ids[0]),
                other => panic!("{other:?}"),
            }
            assert_eq!(tool.input(&at(PointerPhase::Up, 30.0, 30.0, alt()), &f.ctx()), ToolStep::Nothing);
        }
        assert_eq!(picked, vec![id(3), id(2), id(1), id(3)]);
    }

    #[test]
    fn rubber_band_from_empty_canvas_selects_enclosed_objects() {
        let mut f = Fixture::new(vec![
            rect(1, 0.0, 0.0, 10.0, 10.0),
            rect(2, 20.0, 0.0, 10.0, 10.0),
            rect(3, 100.0, 0.0, 10.0, 10.0),
        ]);
        let mut tool = SelectTool::default();
        press(&mut tool, &mut f, -5.0, -5.0, none());
        tool.input(&at(PointerPhase::Move, 15.0, 15.0, none()), &f.ctx());
        assert!(matches!(tool.preview(), Some(ToolPreview::RubberBand { .. })));
        let step = tool.input(&at(PointerPhase::Up, 35.0, 15.0, none()), &f.ctx());
        assert_eq!(step, ToolStep::Select(SelectionChange::Replace(vec![id(1), id(2)])));
        f.follow(&step);
        assert_eq!(tool.preview(), None);
        let step = drag(&mut tool, &mut f, (95.0, -5.0), (115.0, 15.0), shift());
        assert_eq!(step, ToolStep::Select(SelectionChange::Extend(vec![id(3)])), "Shift adds");
        assert_eq!(f.selection.ids(), &[id(1), id(2), id(3)]);
    }

    #[test]
    fn dragging_a_selected_object_commits_one_move() {
        let mut f = Fixture::new(vec![rect(1, 0.0, 0.0, 10.0, 10.0), rect(2, 50.0, 0.0, 10.0, 10.0)]);
        f.select(&[1, 2]);
        let mut tool = SelectTool::default();
        press(&mut tool, &mut f, 5.0, 5.0, none());
        tool.input(&at(PointerPhase::Move, 10.0, 8.0, none()), &f.ctx());
        assert!(matches!(tool.preview(), Some(ToolPreview::Transform { kind: TransformKind::Move, .. })));
        let step = tool.input(&at(PointerPhase::Up, 15.0, 10.0, none()), &f.ctx());
        let (targets, m, kind) = transform(&step).unwrap();
        assert_eq!((targets, kind), (vec![id(1), id(2)], TransformKind::Move), "both selected objects, one commit");
        assert!(maps(&m, (0.0, 0.0), (10.0, 5.0)));
        assert_eq!(tool.preview(), None);
    }

    #[test]
    fn pressing_an_unselected_object_selects_it_and_drags_it() {
        let mut f = Fixture::new(vec![rect(1, 0.0, 0.0, 10.0, 10.0), rect(2, 50.0, 0.0, 10.0, 10.0)]);
        f.select(&[1]);
        let mut tool = SelectTool::default();
        let step = drag(&mut tool, &mut f, (55.0, 5.0), (55.0, 25.0), none());
        let (targets, m, _) = transform(&step).unwrap();
        assert_eq!(targets, vec![id(2)]);
        assert!(maps(&m, (50.0, 0.0), (50.0, 20.0)));
    }

    #[test]
    fn jitter_below_the_drag_threshold_is_a_click_and_shift_constrains_moves() {
        let mut f = Fixture::new(vec![rect(1, 0.0, 0.0, 10.0, 10.0)]);
        f.select(&[1]);
        let mut tool = SelectTool::default();
        // 1 mm = 2 pt < 3 pt threshold.
        assert_eq!(drag(&mut tool, &mut f, (5.0, 5.0), (6.0, 5.0), none()), ToolStep::Nothing);
        let (_, m, _) = transform(&drag(&mut tool, &mut f, (5.0, 5.0), (25.0, 9.0), shift())).unwrap();
        assert!(maps(&m, (0.0, 0.0), (20.0, 0.0)), "horizontal only");
    }

    #[test]
    fn escape_or_pointer_cancel_during_a_move_commits_nothing() {
        for cancel in [key(CanvasKey::Escape), at(PointerPhase::Cancel, 30.0, 30.0, none())] {
            let mut f = Fixture::new(vec![rect(1, 0.0, 0.0, 10.0, 10.0)]);
            f.select(&[1]);
            let mut tool = SelectTool::default();
            press(&mut tool, &mut f, 5.0, 5.0, none());
            tool.input(&at(PointerPhase::Move, 30.0, 30.0, none()), &f.ctx());
            assert_eq!(tool.input(&cancel, &f.ctx()), ToolStep::Cancelled);
            assert_eq!(tool.preview(), None);
            assert_eq!(tool.input(&at(PointerPhase::Up, 30.0, 30.0, none()), &f.ctx()), ToolStep::Nothing);
        }
    }

    #[test]
    fn handles_sit_on_the_corners_and_above_the_top_edge() {
        let b = Bounds::from_corners(Point::new(0.0, 0.0), Point::new(40.0, 20.0));
        let h = selection_handles(b, 0.5);
        assert!(h.contains(&(Handle::TopRight, Point::new(40.0, 20.0))));
        assert!(h.contains(&(Handle::BottomLeft, Point::new(0.0, 0.0))));
        assert!(h.contains(&(Handle::Rotate, Point::new(20.0, 20.0 + ROTATE_HANDLE_OFFSET_PT * 0.5))));
        assert_eq!(h.len(), 5);
    }

    #[test]
    fn corner_handle_scales_about_the_opposite_corner_and_shift_keeps_proportions() {
        let mut f = Fixture::new(vec![rect(1, 0.0, 0.0, 40.0, 20.0)]);
        f.select(&[1]);
        let mut tool = SelectTool::default();
        let (targets, m, kind) = transform(&drag(&mut tool, &mut f, (40.0, 20.0), (80.0, 30.0), none())).unwrap();
        assert_eq!((targets, kind), (vec![id(1)], TransformKind::Scale));
        assert!(maps(&m, (0.0, 0.0), (0.0, 0.0)) && maps(&m, (40.0, 20.0), (80.0, 30.0)));
        let (_, m, _) = transform(&drag(&mut tool, &mut f, (40.0, 20.0), (80.0, 30.0), shift())).unwrap();
        assert!(maps(&m, (40.0, 20.0), (80.0, 40.0)), "uniform factor 2");
    }

    #[test]
    fn rotate_handle_rotates_about_the_centre_and_shift_snaps_to_15_degrees() {
        let mut f = Fixture::new(vec![rect(1, 0.0, 0.0, 40.0, 20.0)]);
        f.select(&[1]);
        let mut tool = SelectTool::default();
        let handle = (20.0, 20.0 + ROTATE_HANDLE_OFFSET_PT * 0.5);
        // Centre (20, 10); from straight up to straight left = +90°.
        let (_, m, kind) = transform(&drag(&mut tool, &mut f, handle, (-2.0, 10.0), none())).unwrap();
        assert_eq!(kind, TransformKind::Rotate);
        assert!(maps(&m, (40.0, 10.0), (20.0, 30.0)), "{:?}", m.apply(Point::new(40.0, 10.0)));
        // About 100° with Shift snaps to 105°.
        let a = Angle::from_degrees(190.0).unwrap().radians();
        let to = (20.0 + 22.0 * a.cos(), 10.0 + 22.0 * a.sin());
        let (_, m, _) = transform(&drag(&mut tool, &mut f, handle, to, shift())).unwrap();
        let expected = Affine::translate(Vector::new(-20.0, -10.0))
            .unwrap()
            .then(&Affine::rotate(Angle::from_degrees(105.0).unwrap()))
            .then(&Affine::translate(Vector::new(20.0, 10.0)).unwrap());
        assert!(maps(&m, (40.0, 10.0), {
            let p = expected.apply(Point::new(40.0, 10.0));
            (p.x, p.y)
        }));
    }

    /// E04: the grab point snaps to a target of another object; the moved object offers none.
    #[test]
    fn move_snaps_the_grab_point_to_other_objects() {
        let mut f = Fixture::new(vec![rect(1, 0.0, 0.0, 40.0, 20.0), rect(2, 100.0, 0.0, 10.0, 10.0)]);
        f.snap.objects = true;
        f.select(&[1]);
        let mut tool = SelectTool::default();
        let (_, m, _) = transform(&drag(&mut tool, &mut f, (35.0, 15.0), (99.5, 0.4), none())).unwrap();
        assert!(maps(&m, (35.0, 15.0), (100.0, 0.0)), "{:?}", m.apply(Point::new(35.0, 15.0)));
    }
}
