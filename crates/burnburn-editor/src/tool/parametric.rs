//! Drawing tools for the parametric basic shapes (W1-L2b): triangle, regular polygon (also the
//! pentagon and octagon presets), star, double star and rounded rectangle. A drag is the shape's
//! bounding box; Shift keeps the shape equal-sided (natural aspect), Alt/Option draws from the
//! centre, Cmd/Ctrl suppresses snapping — the same rules as the rectangle and ellipse tools. A
//! finished gesture is one `EditCommit::CreateParametric`; the object stays editable in the
//! document until it is converted to a path.

use std::sync::Arc;

use burnburn_geometry::{Bounds, ParametricShape, Point, ShapeParamError, ShapeParams};
use burnburn_types::Length;

use super::{EditCommit, ShapeKind, Tool, ToolContext, ToolKind, ToolPreview, ToolStep, is_escape, tool_pointer};
use crate::input::{CanvasInput, PointerPhase};
use crate::snap::SnapCandidate;

/// Sides, tips, ratio and radius the shape tools draw with (the options bar of each tool).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShapeDefaults {
    /// Regular polygon (`ToolKind::Polygon`).
    pub polygon_sides: u32,
    pub star_points: u32,
    /// Inner radius of the star as a fraction of the outer radius.
    pub star_ratio: f64,
    pub double_star_points: u32,
    pub double_star_ratio: f64,
    /// Corner radius of the rounded rectangle (mm); clamped to the drawn size.
    pub corner_radius: f64,
}

impl Default for ShapeDefaults {
    fn default() -> Self {
        ShapeDefaults {
            polygon_sides: 6,
            star_points: 5,
            star_ratio: 0.382,
            double_star_points: 6,
            double_star_ratio: 0.577,
            corner_radius: 3.0,
        }
    }
}

impl ShapeDefaults {
    pub fn validate(&self) -> Result<(), ShapeParamError> {
        ShapeParams::Polygon { sides: self.polygon_sides }.validate()?;
        ShapeParams::Star { points: self.star_points, ratio: self.star_ratio }.validate()?;
        ShapeParams::DoubleStar { points: self.double_star_points, ratio: self.double_star_ratio }.validate()?;
        if self.corner_radius.is_finite() && self.corner_radius >= 0.0 {
            Ok(())
        } else {
            Err(ShapeParamError::RadiusOutOfRange)
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Drag {
    anchor: Point,
    current: Point,
    /// Shift: keep the shape's natural aspect (equal sides) instead of stretching it.
    equilateral: bool,
    /// Alt/Option: the press is the centre of the bounding box.
    from_center: bool,
}

impl Drag {
    /// The dragged bounding box. Equal-sided drawing scales the natural size until it covers the
    /// larger extent of the drag, growing from the press point in the drag direction.
    fn bounds(&self, natural: (f64, f64)) -> Bounds {
        let mut dx = self.current.x - self.anchor.x;
        let mut dy = self.current.y - self.anchor.y;
        if self.equilateral && natural.0 > 0.0 && natural.1 > 0.0 {
            let scale = (dx.abs() / natural.0).max(dy.abs() / natural.1);
            dx = (scale * natural.0).copysign(dx);
            dy = (scale * natural.1).copysign(dy);
        }
        let far = Point::new(self.anchor.x + dx, self.anchor.y + dy);
        if self.from_center {
            Bounds::from_corners(Point::new(self.anchor.x - dx, self.anchor.y - dy), far)
        } else {
            Bounds::from_corners(self.anchor, far)
        }
    }

    /// The same gesture with an exact box size (typed while dragging, E01): the drag direction is
    /// kept, a drag that has not moved yet grows towards +x/+y.
    fn with_size(&self, width: f64, height: f64) -> Drag {
        let sx = if self.current.x < self.anchor.x { -1.0 } else { 1.0 };
        let sy = if self.current.y < self.anchor.y { -1.0 } else { 1.0 };
        let (dx, dy) = if self.from_center { (width / 2.0, height / 2.0) } else { (width, height) };
        Drag { current: Point::new(self.anchor.x + sx * dx, self.anchor.y + sy * dy), equilateral: false, ..*self }
    }
}

/// One tool for all basic shapes; the constructors are the per-shape tools.
#[derive(Debug, Clone)]
pub struct BasicShapeTool {
    template: ShapeParams,
    drag: Option<Drag>,
    snap: Option<SnapCandidate>,
}

impl BasicShapeTool {
    pub fn triangle() -> Self {
        Self::with(ShapeParams::Polygon { sides: 3 })
    }

    pub fn polygon(sides: u32) -> Self {
        Self::with(ShapeParams::Polygon { sides })
    }

    pub fn pentagon() -> Self {
        Self::polygon(5)
    }

    pub fn octagon() -> Self {
        Self::polygon(8)
    }

    pub fn star(points: u32, ratio: f64) -> Self {
        Self::with(ShapeParams::Star { points, ratio })
    }

    pub fn double_star(points: u32, ratio: f64) -> Self {
        Self::with(ShapeParams::DoubleStar { points, ratio })
    }

    /// The width and height are placeholders: the drag sets them; only the radius counts.
    pub fn rounded_rectangle(radius: f64) -> Self {
        Self::with(ShapeParams::RoundedRectangle { width: 1.0, height: 1.0, radius })
    }

    /// The tool of a basic-shape [`ToolKind`] with the current defaults.
    pub(crate) fn for_kind(kind: ToolKind, d: &ShapeDefaults) -> Self {
        match kind {
            ToolKind::Triangle => Self::triangle(),
            ToolKind::Pentagon => Self::pentagon(),
            ToolKind::Octagon => Self::octagon(),
            ToolKind::Star => Self::star(d.star_points, d.star_ratio),
            ToolKind::DoubleStar => Self::double_star(d.double_star_points, d.double_star_ratio),
            ToolKind::RoundedRectangle => Self::rounded_rectangle(d.corner_radius),
            _ => Self::polygon(d.polygon_sides),
        }
    }

    /// New defaults for the next gesture; a drag in progress keeps the shape it started with.
    pub(crate) fn retarget(&mut self, kind: ToolKind, d: &ShapeDefaults) {
        if self.drag.is_none() {
            self.template = Self::for_kind(kind, d).template;
        }
    }

    fn with(template: ShapeParams) -> Self {
        BasicShapeTool { template, drag: None, snap: None }
    }

    /// What the tool draws, for the options bar.
    pub fn params(&self) -> &ShapeParams {
        &self.template
    }

    fn kind(&self) -> ShapeKind {
        match self.template {
            ShapeParams::Star { .. } => ShapeKind::Star,
            ShapeParams::DoubleStar { .. } => ShapeKind::DoubleStar,
            ShapeParams::RoundedRectangle { .. } => ShapeKind::RoundedRectangle,
            _ => ShapeKind::Polygon,
        }
    }

    /// The shape of a drag; `None` without area (no half object, E01) or for invalid parameters.
    fn shape(&self, drag: &Drag) -> Option<ParametricShape> {
        let bounds = drag.bounds(self.template.natural_size());
        ParametricShape::fit(self.template, bounds).ok()
    }

    fn commit(&mut self, drag: Drag) -> ToolStep {
        self.drag = None;
        match self.shape(&drag) {
            Some(shape) => ToolStep::Commit(EditCommit::CreateParametric { shape }),
            None => ToolStep::Cancelled,
        }
    }
}

impl Tool for BasicShapeTool {
    fn input(&mut self, input: &CanvasInput, ctx: &ToolContext<'_>) -> ToolStep {
        if is_escape(input) {
            return self.cancel();
        }
        let Some(ev) = tool_pointer(input) else { return ToolStep::Nothing };
        if ev.phase == PointerPhase::Cancel {
            return self.cancel();
        }
        let (world, snap) = ctx.snapped(ev, &[]);
        let snap_changed = snap != self.snap;
        self.snap = snap;
        let (equilateral, from_center) = (ev.modifiers.shift, ev.modifiers.alt);
        match (ev.phase, self.drag) {
            (PointerPhase::Down, _) => {
                self.drag = Some(Drag { anchor: world, current: world, equilateral, from_center });
                ToolStep::PreviewChanged
            }
            (PointerPhase::Move, Some(d)) => {
                self.drag = Some(Drag { current: world, equilateral, from_center, ..d });
                ToolStep::PreviewChanged
            }
            (PointerPhase::Up, Some(d)) => self.commit(Drag { current: world, equilateral, from_center, ..d }),
            (PointerPhase::Move, None) if snap_changed => ToolStep::PreviewChanged,
            _ => ToolStep::Nothing,
        }
    }

    fn cancel(&mut self) -> ToolStep {
        self.snap = None;
        if self.drag.take().is_some() { ToolStep::Cancelled } else { ToolStep::Nothing }
    }

    fn preview(&self) -> Option<ToolPreview> {
        let path = self.shape(&self.drag?)?.to_path().ok()?;
        Some(ToolPreview::Shape { path: Arc::new(path), shape: self.kind() })
    }

    fn snap(&self) -> Option<SnapCandidate> {
        self.snap
    }

    fn exact_size(&mut self, width: Length, height: Length) -> ToolStep {
        match self.drag {
            Some(d) => self.commit(d.with_size(width.mm(), height.mm())),
            None => ToolStep::Nothing,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{CanvasKey, PointerPhase};
    use crate::snap::SnapKind;
    use crate::tool::fixture::*;
    use crate::tool::{Editor, ToolKind};
    use burnburn_document::GeometryObject;

    fn empty() -> Fixture {
        Fixture::new(Vec::<GeometryObject>::new())
    }

    fn shape_of(step: &ToolStep) -> Option<&ParametricShape> {
        match step {
            ToolStep::Commit(EditCommit::CreateParametric { shape }) => Some(shape),
            _ => None,
        }
    }

    fn bounds_of(shape: &ParametricShape) -> (f64, f64, f64, f64) {
        let b = shape.to_path().unwrap().bounds().unwrap();
        (b.min().x, b.min().y, b.width(), b.height())
    }

    fn approx(a: (f64, f64, f64, f64), b: (f64, f64, f64, f64)) -> bool {
        (a.0 - b.0).abs() < 1e-7 && (a.1 - b.1).abs() < 1e-7 && (a.2 - b.2).abs() < 1e-7 && (a.3 - b.3).abs() < 1e-7
    }

    fn drag(
        tool: &mut BasicShapeTool,
        f: &Fixture,
        from: (f64, f64),
        to: (f64, f64),
        mods: crate::Modifiers,
    ) -> ToolStep {
        tool.input(&at(PointerPhase::Down, from.0, from.1, mods), &f.ctx());
        tool.input(&at(PointerPhase::Move, to.0, to.1, mods), &f.ctx());
        tool.input(&at(PointerPhase::Up, to.0, to.1, mods), &f.ctx())
    }

    #[test]
    fn drag_is_the_bounding_box_of_every_shape_and_commits_once() {
        let f = empty();
        let tools = [
            BasicShapeTool::triangle(),
            BasicShapeTool::polygon(7),
            BasicShapeTool::pentagon(),
            BasicShapeTool::octagon(),
            BasicShapeTool::star(5, 0.4),
            BasicShapeTool::double_star(6, 0.5),
            BasicShapeTool::rounded_rectangle(4.0),
        ];
        for mut tool in tools {
            assert_eq!(tool.input(&at(PointerPhase::Down, 120.0, 80.0, none()), &f.ctx()), ToolStep::PreviewChanged);
            assert_eq!(tool.input(&at(PointerPhase::Move, 60.0, 40.0, none()), &f.ctx()), ToolStep::PreviewChanged);
            assert!(tool.preview().is_some());
            let step = tool.input(&at(PointerPhase::Up, 60.0, 40.0, none()), &f.ctx());
            let shape = shape_of(&step).unwrap_or_else(|| panic!("{:?}: {step:?}", tool.params()));
            // Dragged up-left: normalised bounds (60,40)-(120,80), whatever the shape.
            assert!(approx(bounds_of(shape), (60.0, 40.0, 60.0, 40.0)), "{:?}: {:?}", tool.params(), bounds_of(shape));
            assert_eq!(std::mem::discriminant(shape.params()), std::mem::discriminant(tool.params()));
            assert_eq!(tool.preview(), None, "back to idle after the commit");
        }
    }

    #[test]
    fn presets_draw_the_documented_vertex_counts() {
        let f = empty();
        for (mut tool, vertices) in [
            (BasicShapeTool::triangle(), 3),
            (BasicShapeTool::pentagon(), 5),
            (BasicShapeTool::octagon(), 8),
            (BasicShapeTool::polygon(11), 11),
            (BasicShapeTool::star(5, 0.4), 10),
        ] {
            let step = drag(&mut tool, &f, (0.0, 0.0), (40.0, 40.0), none());
            let path = shape_of(&step).unwrap().to_path().unwrap();
            assert_eq!(path.subpaths()[0].segments().len() + 1, vertices);
        }
    }

    #[test]
    fn shift_keeps_the_shape_equal_sided_anchored_at_the_press() {
        let f = empty();
        // Triangle: natural size √3 × 1.5 (unit circumradius). A 30 × 30 drag is limited by the
        // height: scale 20 → 34.64 × 30, growing from the press point in the drag direction.
        let mut tool = BasicShapeTool::triangle();
        let step = drag(&mut tool, &f, (0.0, 0.0), (30.0, 30.0), shift());
        let (w, h) = (20.0 * 3f64.sqrt(), 30.0);
        assert!(
            approx(bounds_of(shape_of(&step).unwrap()), (0.0, 0.0, w, h)),
            "{:?}",
            bounds_of(shape_of(&step).unwrap())
        );
        // The outline is equilateral: three equal sides.
        let path = shape_of(&step).unwrap().to_path().unwrap();
        let sp = &path.subpaths()[0];
        let v = [sp.start(), sp.segments()[0].end(), sp.segments()[1].end()];
        let side = v[0].distance(v[1]);
        assert!((v[1].distance(v[2]) - side).abs() < 1e-7 && (v[2].distance(v[0]) - side).abs() < 1e-7);
        // Dragging towards -x/-y puts the far corner at the press point.
        let mut tool = BasicShapeTool::triangle();
        let step = drag(&mut tool, &f, (100.0, 100.0), (70.0, 70.0), shift());
        assert!(approx(bounds_of(shape_of(&step).unwrap()), (100.0 - w, 70.0, w, h)));
        // A rounded rectangle becomes a square.
        let mut tool = BasicShapeTool::rounded_rectangle(2.0);
        let step = drag(&mut tool, &f, (0.0, 0.0), (30.0, 10.0), shift());
        assert!(approx(bounds_of(shape_of(&step).unwrap()), (0.0, 0.0, 30.0, 30.0)));
        // Without Shift the same drag stretches.
        let mut tool = BasicShapeTool::pentagon();
        let step = drag(&mut tool, &f, (0.0, 0.0), (30.0, 10.0), none());
        assert!(approx(bounds_of(shape_of(&step).unwrap()), (0.0, 0.0, 30.0, 10.0)));
    }

    #[test]
    fn alt_draws_from_the_centre_and_combines_with_shift() {
        let f = empty();
        let mut tool = BasicShapeTool::octagon();
        let step = drag(&mut tool, &f, (50.0, 50.0), (60.0, 70.0), alt());
        assert!(approx(bounds_of(shape_of(&step).unwrap()), (40.0, 30.0, 20.0, 40.0)));
        let mods = crate::Modifiers { shift: true, alt: true, ..none() };
        let mut tool = BasicShapeTool::rounded_rectangle(1.0);
        let step = drag(&mut tool, &f, (50.0, 50.0), (60.0, 70.0), mods);
        // Square of side 2 · 20 centred on the press point.
        assert!(approx(bounds_of(shape_of(&step).unwrap()), (30.0, 30.0, 40.0, 40.0)));
    }

    #[test]
    fn the_preview_is_what_would_be_committed() {
        let f = empty();
        let mut tool = BasicShapeTool::double_star(5, 0.5);
        tool.input(&at(PointerPhase::Down, 0.0, 0.0, none()), &f.ctx());
        tool.input(&at(PointerPhase::Move, 40.0, 30.0, none()), &f.ctx());
        let Some(ToolPreview::Shape { path, shape }) = tool.preview() else { panic!("no preview") };
        assert_eq!(shape, ShapeKind::DoubleStar);
        let step = tool.input(&at(PointerPhase::Up, 40.0, 30.0, none()), &f.ctx());
        assert_eq!(*path, shape_of(&step).unwrap().to_path().unwrap());
        let kinds = [
            (BasicShapeTool::triangle(), ShapeKind::Polygon),
            (BasicShapeTool::star(5, 0.4), ShapeKind::Star),
            (BasicShapeTool::rounded_rectangle(1.0), ShapeKind::RoundedRectangle),
        ];
        for (mut tool, kind) in kinds {
            tool.input(&at(PointerPhase::Down, 0.0, 0.0, none()), &f.ctx());
            tool.input(&at(PointerPhase::Move, 10.0, 10.0, none()), &f.ctx());
            assert!(matches!(tool.preview(), Some(ToolPreview::Shape { shape, .. }) if shape == kind));
        }
    }

    #[test]
    fn a_gesture_without_area_creates_nothing() {
        for (x, y) in [(0.0, 0.0), (30.0, 0.0), (0.0, 30.0)] {
            let f = empty();
            let mut tool = BasicShapeTool::pentagon();
            tool.input(&at(PointerPhase::Down, 0.0, 0.0, none()), &f.ctx());
            assert_eq!(tool.input(&at(PointerPhase::Up, x, y, none()), &f.ctx()), ToolStep::Cancelled, "({x},{y})");
            assert_eq!(tool.preview(), None);
        }
    }

    #[test]
    fn cancel_mid_drag_leaves_no_half_object() {
        for how in ["esc", "pointer-cancel", "tool-switch"] {
            let f = empty();
            let mut ed = Editor::new();
            ed.select_tool(ToolKind::Star);
            ed.input(&at(PointerPhase::Down, 0.0, 0.0, none()), &f.ctx());
            ed.input(&at(PointerPhase::Move, 50.0, 50.0, none()), &f.ctx());
            assert!(ed.is_gesture_active(), "{how}");
            let step = match how {
                "esc" => ed.input(&key(CanvasKey::Escape), &f.ctx()),
                "pointer-cancel" => ed.input(&at(PointerPhase::Cancel, 50.0, 50.0, none()), &f.ctx()),
                _ => ed.select_tool(ToolKind::Select),
            };
            assert_eq!(step, ToolStep::Cancelled, "{how}");
            assert_eq!(ed.preview(), None, "{how}");
            ed.select_tool(ToolKind::Star);
            assert_eq!(ed.input(&at(PointerPhase::Up, 50.0, 50.0, none()), &f.ctx()), ToolStep::Nothing, "{how}");
        }
    }

    #[test]
    fn corners_snap_to_endpoints_unless_suppressed() {
        let mut f = Fixture::new(vec![rect(1, 0.0, 0.0, 40.0, 20.0)]);
        f.snap.objects = true;
        let mut tool = BasicShapeTool::pentagon();
        tool.input(&at(PointerPhase::Down, 40.6, 20.4, none()), &f.ctx());
        assert_eq!(tool.snap().map(|s| s.kind), Some(SnapKind::Endpoint));
        let r = bounds_of(shape_of(&tool.input(&at(PointerPhase::Up, 80.0, 50.0, none()), &f.ctx())).unwrap());
        assert!(approx(r, (40.0, 20.0, 40.0, 30.0)), "{r:?}");
        let mut tool = BasicShapeTool::pentagon();
        tool.input(&at(PointerPhase::Down, 40.6, 20.4, command()), &f.ctx());
        assert_eq!(tool.snap(), None);
        let r = bounds_of(shape_of(&tool.input(&at(PointerPhase::Up, 80.0, 50.0, command()), &f.ctx())).unwrap());
        assert!(approx(r, (40.6, 20.4, 39.4, 29.6)), "{r:?}");
    }

    /// E01: typing the size while dragging commits exactly that bounding box from the anchor.
    #[test]
    fn typed_size_while_dragging_commits_the_exact_box() {
        let f = empty();
        let mut tool = BasicShapeTool::star(5, 0.4);
        assert_eq!(tool.exact_size(Length::from_mm(1.0).unwrap(), Length::from_mm(1.0).unwrap()), ToolStep::Nothing);
        tool.input(&at(PointerPhase::Down, 10.0, 10.0, none()), &f.ctx());
        tool.input(&at(PointerPhase::Move, 3.0, 50.0, none()), &f.ctx());
        let step = tool.exact_size(Length::from_mm(120.0).unwrap(), Length::from_mm(80.0).unwrap());
        assert!(approx(bounds_of(shape_of(&step).unwrap()), (-110.0, 10.0, 120.0, 80.0)), "drag went left/up");
        assert_eq!(tool.preview(), None);
        // From the centre the typed size is centred on the press point.
        let mut tool = BasicShapeTool::octagon();
        tool.input(&at(PointerPhase::Down, 10.0, 10.0, alt()), &f.ctx());
        tool.input(&at(PointerPhase::Move, 14.0, 16.0, alt()), &f.ctx());
        let step = tool.exact_size(Length::from_mm(20.0).unwrap(), Length::from_mm(8.0).unwrap());
        assert!(approx(bounds_of(shape_of(&step).unwrap()), (0.0, 6.0, 20.0, 8.0)));
    }

    #[test]
    fn corner_radius_is_clamped_to_the_drawn_size() {
        let f = empty();
        let mut tool = BasicShapeTool::rounded_rectangle(5.0);
        let step = drag(&mut tool, &f, (0.0, 0.0), (4.0, 20.0), none());
        assert_eq!(
            *shape_of(&step).unwrap().params(),
            ShapeParams::RoundedRectangle { width: 4.0, height: 20.0, radius: 2.0 }
        );
    }

    #[test]
    fn the_editor_draws_with_its_shape_defaults_and_refuses_invalid_ones() {
        let f = empty();
        let mut ed = Editor::new();
        ed.select_tool(ToolKind::Polygon);
        let step_sides = |ed: &mut Editor| {
            ed.input(&at(PointerPhase::Down, 0.0, 0.0, none()), &f.ctx());
            let s = ed.input(&at(PointerPhase::Up, 40.0, 40.0, none()), &f.ctx());
            shape_of(&s).map(|s| *s.params())
        };
        assert_eq!(step_sides(&mut ed), Some(ShapeParams::Polygon { sides: 6 }));
        let defaults = ShapeDefaults { polygon_sides: 9, star_points: 7, star_ratio: 0.3, ..ShapeDefaults::default() };
        ed.set_shape_defaults(defaults).unwrap();
        assert_eq!(step_sides(&mut ed), Some(ShapeParams::Polygon { sides: 9 }));
        ed.select_tool(ToolKind::Star);
        assert_eq!(step_sides(&mut ed), Some(ShapeParams::Star { points: 7, ratio: 0.3 }));
        // Presets ignore the polygon default.
        ed.select_tool(ToolKind::Pentagon);
        assert_eq!(step_sides(&mut ed), Some(ShapeParams::Polygon { sides: 5 }));
        ed.select_tool(ToolKind::Triangle);
        assert_eq!(step_sides(&mut ed), Some(ShapeParams::Polygon { sides: 3 }));
        ed.select_tool(ToolKind::Octagon);
        assert_eq!(step_sides(&mut ed), Some(ShapeParams::Polygon { sides: 8 }));
        // Invalid defaults change nothing.
        let bad = ShapeDefaults { star_ratio: 1.2, ..defaults };
        assert_eq!(ed.set_shape_defaults(bad), Err(ShapeParamError::RatioOutOfRange));
        assert_eq!(*ed.shape_defaults(), defaults);
        let bad = ShapeDefaults { polygon_sides: 2, ..defaults };
        assert_eq!(ed.set_shape_defaults(bad), Err(ShapeParamError::TooFewSides));
        // Changing the defaults mid-gesture does not alter the shape being drawn.
        ed.select_tool(ToolKind::Polygon);
        ed.input(&at(PointerPhase::Down, 0.0, 0.0, none()), &f.ctx());
        ed.set_shape_defaults(ShapeDefaults { polygon_sides: 4, ..defaults }).unwrap();
        let s = ed.input(&at(PointerPhase::Up, 40.0, 40.0, none()), &f.ctx());
        assert_eq!(shape_of(&s).map(|s| *s.params()), Some(ShapeParams::Polygon { sides: 9 }));
    }

    #[test]
    fn hover_without_press_shows_no_preview() {
        let f = empty();
        let mut tool = BasicShapeTool::triangle();
        assert_eq!(tool.input(&hover(5.0, 5.0), &f.ctx()), ToolStep::Nothing);
        assert_eq!(tool.preview(), None);
    }
}
