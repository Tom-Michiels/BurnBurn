//! Scene translation (contract §5): `burnburn_editor::SceneSnapshot` in world mm (`f64`) to a
//! [`RenderPacket`] in viewport-relative device pixels (`f32`).
//!
//! Paths are flattened with a [`DisplayTolerance`] of a quarter device pixel (zoom-dependent, never
//! used for CAM). The camera origin is subtracted in `f64` before narrowing to `f32` (ADR 0003).
//! Colours and widths are a theme decision; the scene only carries semantic roles.

use burnburn_editor::{CompareSide, Overlay, SceneSnapshot, StyleRole, Viewport};
use burnburn_geometry::{Affine, DisplayTolerance, Path, Point};
use burnburn_types::Length;

mod sketch;

use crate::input::Rgba;
use crate::packet::{ImageCell, Primitive, RenderPacket};

/// Maximum chord deviation on screen, in device pixels.
pub const DISPLAY_TOLERANCE_PX: f64 = 0.25;
/// Grids denser than this many lines per axis are not drawn (they would be a grey wash).
pub const MAX_GRID_LINES: usize = 1000;

/// Colours in the layer palette.
pub const LAYER_COLOURS: usize = 32;

/// The light theme's layer palette (sRGB 0–255): dark first, then saturated hues, then softer
/// tints, so neighbouring slots are easy to tell apart.
const LAYER_PALETTE: [[u8; 3]; LAYER_COLOURS] = [
    [0, 0, 0],
    [0, 0, 255],
    [255, 0, 0],
    [0, 160, 0],
    [150, 120, 0],
    [255, 128, 0],
    [0, 160, 200],
    [190, 0, 190],
    [128, 128, 128],
    [0, 0, 140],
    [140, 0, 0],
    [0, 90, 0],
    [90, 60, 20],
    [220, 180, 0],
    [0, 110, 120],
    [110, 0, 140],
    [70, 140, 255],
    [255, 90, 120],
    [110, 200, 60],
    [200, 140, 80],
    [255, 170, 60],
    [60, 210, 210],
    [230, 110, 230],
    [64, 64, 64],
    [150, 150, 230],
    [230, 150, 150],
    [150, 210, 150],
    [180, 160, 110],
    [240, 200, 120],
    [120, 190, 200],
    // The tool layers T1 and T2: muted, so they read as guides and not as cuts.
    [110, 110, 110],
    [90, 140, 170],
];

/// Look of one style role: optional fill, stroke colour and stroke width in logical points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RoleStyle {
    pub fill: Option<Rgba>,
    pub stroke: Option<Rgba>,
    pub stroke_width_pt: f64,
}

/// Colours and widths for every role and overlay (light appearance).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Theme {
    pub background: Rgba,
    pub design: RoleStyle,
    pub selected: RoleStyle,
    pub hovered: RoleStyle,
    pub disabled: RoleStyle,
    /// Stroke colour per layer-palette slot (presentation's `LAYER_SLOTS`); hosts draw the
    /// palette buttons in the same colours. Higher slots cycle.
    pub process_layers: [Rgba; LAYER_COLOURS],
    /// `None` hides the grid.
    pub grid: Option<Rgba>,
    pub selection: Rgba,
    pub selection_width_pt: f64,
    pub preview: Rgba,
    /// The outlines of the two variants of the tracer's A/B comparison: A the recipe in force, B the
    /// one it is compared with (also told apart by line weight, not by colour alone).
    pub compare_a: Rgba,
    pub compare_b: Rgba,
    pub rubber_band_fill: Rgba,
    pub handle_size_pt: f64,
    pub snap: Rgba,
    pub issue: Rgba,
    /// Sketch mode: relation glyphs, freedom markers and suggestions.
    pub sketch: Rgba,
    /// Sketch mode: dimension lines and their numbers.
    pub sketch_dimension: Rgba,
    /// How far (device pixels) the drawn polyline of a curve may lie from the curve: display
    /// quality, never the tolerance of a compiled job.
    pub tolerance_px: f64,
}

impl Theme {
    pub fn light() -> Self {
        let ink = Rgba::new(0.2, 0.2, 0.2, 1.0);
        let accent = Rgba::new(0.0, 0.48, 1.0, 1.0);
        let paper = Some(Rgba::new(0.93, 0.93, 0.95, 1.0));
        Theme {
            background: Rgba::new(1.0, 1.0, 1.0, 1.0),
            design: RoleStyle { fill: paper, stroke: Some(ink), stroke_width_pt: 1.0 },
            selected: RoleStyle { fill: paper, stroke: Some(accent), stroke_width_pt: 1.0 },
            hovered: RoleStyle { fill: paper, stroke: Some(Rgba::new(0.35, 0.6, 1.0, 1.0)), stroke_width_pt: 1.0 },
            disabled: RoleStyle { fill: None, stroke: Some(Rgba::new(0.7, 0.7, 0.7, 1.0)), stroke_width_pt: 1.0 },
            process_layers: LAYER_PALETTE
                .map(|[r, g, b]| Rgba::new(f32::from(r) / 255.0, f32::from(g) / 255.0, f32::from(b) / 255.0, 1.0)),
            grid: Some(Rgba::new(0.9, 0.9, 0.92, 1.0)),
            selection: accent,
            selection_width_pt: 1.0,
            preview: Rgba::new(0.0, 0.48, 1.0, 0.8),
            compare_a: Rgba::new(0.1, 0.1, 0.1, 1.0),
            compare_b: Rgba::new(0.9, 0.1, 0.1, 1.0),
            rubber_band_fill: Rgba::new(0.0, 0.48, 1.0, 0.12),
            handle_size_pt: 6.0,
            snap: Rgba::new(0.85, 0.0, 0.6, 1.0),
            issue: Rgba::new(0.9, 0.1, 0.1, 1.0),
            sketch: Rgba::new(0.0, 0.4, 0.45, 1.0),
            sketch_dimension: Rgba::new(0.1, 0.45, 0.15, 1.0),
            tolerance_px: DISPLAY_TOLERANCE_PX,
        }
    }

    /// The same look with curves flattened to within `px` device pixels (coarser is faster).
    pub fn with_tolerance_px(self, px: f64) -> Self {
        Theme { tolerance_px: px, ..self }
    }

    /// The same look without any fill: outlines only (Beeld ▸ Draadmodel).
    pub fn wireframe(self) -> Self {
        let bare = |style: RoleStyle| RoleStyle { fill: None, ..style };
        Theme {
            design: bare(self.design),
            selected: bare(self.selected),
            hovered: bare(self.hovered),
            disabled: bare(self.disabled),
            ..self
        }
    }

    pub fn role(&self, role: StyleRole) -> RoleStyle {
        match role {
            StyleRole::Selected => self.selected,
            StyleRole::Hovered => self.hovered,
            StyleRole::Disabled => self.disabled,
            StyleRole::ProcessLayer(n) => {
                let stroke = self.process_layers.get(usize::from(n) % self.process_layers.len()).copied();
                RoleStyle { stroke, ..self.design }
            }
            _ => self.design,
        }
    }
}

/// The drawable size in whole device pixels (rounded, at least one).
pub fn size_px(viewport: &Viewport) -> [u32; 2] {
    let px = |pt: f64| (pt * viewport.backing_scale()).round().clamp(1.0, f64::from(u32::MAX)) as u32;
    [px(viewport.width_pt()), px(viewport.height_pt())]
}

/// World mm → viewport-relative device px, in `f64` relative to the camera.
struct Projection {
    camera: burnburn_editor::Camera,
    viewport: Viewport,
    scale: f64,
    tolerance_px: f64,
}

impl Projection {
    fn px(&self, p: Point) -> [f64; 2] {
        let [x, y] = self.camera.world_to_points(&self.viewport, p);
        [x * self.scale, y * self.scale]
    }

    fn px32(&self, p: Point) -> [f32; 2] {
        let [x, y] = self.px(p);
        [x as f32, y as f32]
    }

    fn mm_per_px(&self) -> f64 {
        1.0 / (self.camera.points_per_mm() * self.scale)
    }
}

/// Flattened subpaths in device pixels: (points, closed).
fn flatten(path: &Path, transform: &Affine, proj: &Projection) -> Vec<(Vec<[f32; 2]>, bool)> {
    let Ok(tolerance) = Length::from_mm(proj.tolerance_px * proj.mm_per_px())
        .map_err(|_| ())
        .and_then(|l| DisplayTolerance::new(l).map_err(|_| ()))
    else {
        return Vec::new();
    };
    let placed;
    let path = if *transform == Affine::IDENTITY {
        path
    } else {
        placed = path.transform(transform);
        &placed
    };
    path.flatten(&tolerance)
        .into_iter()
        .map(|pl| (pl.points.into_iter().map(|p| proj.px32(p)).collect(), pl.closed))
        .collect()
}

fn draw_path(out: &mut Vec<Primitive>, polylines: Vec<(Vec<[f32; 2]>, bool)>, style: RoleStyle, scale: f64) {
    if let Some(color) = style.fill {
        let contours: Vec<Vec<[f32; 2]>> =
            polylines.iter().filter(|(_, closed)| *closed).map(|(p, _)| p.clone()).collect();
        if !contours.is_empty() {
            out.push(Primitive::Fill { contours_px: contours, color });
        }
    }
    if let Some(color) = style.stroke {
        let width_px = (style.stroke_width_pt * scale) as f32;
        for (points_px, closed) in polylines {
            out.push(Primitive::Stroke { points_px, closed, width_px, color });
        }
    }
}

fn bounds_box(b: &burnburn_geometry::Bounds, proj: &Projection) -> Vec<[f32; 2]> {
    let (min, max) = (b.min(), b.max());
    [Point::new(min.x, max.y), Point::new(max.x, max.y), Point::new(max.x, min.y), Point::new(min.x, min.y)]
        .map(|p| proj.px32(p))
        .to_vec()
}

/// A small square marker of `size_pt` centred on `at`.
fn marker(out: &mut Vec<Primitive>, at: Point, size_pt: f64, fill: Rgba, stroke: Rgba, proj: &Projection) {
    let [x, y] = proj.px(at);
    let h = size_pt * proj.scale / 2.0;
    let square = vec![[x - h, y - h], [x + h, y - h], [x + h, y + h], [x - h, y + h]]
        .into_iter()
        .map(|[a, b]| [a as f32, b as f32])
        .collect::<Vec<_>>();
    out.push(Primitive::Fill { contours_px: vec![square.clone()], color: fill });
    out.push(Primitive::Stroke { points_px: square, closed: true, width_px: proj.scale as f32, color: stroke });
}

/// Grid hairlines every `spacing_mm`, centred on device pixels, across the visible area.
fn grid(out: &mut Vec<Primitive>, spacing_mm: f64, color: Rgba, proj: &Projection, size: [u32; 2]) {
    if !(spacing_mm.is_finite() && spacing_mm > 0.0) {
        return;
    }
    let (w, h) = (f64::from(size[0]), f64::from(size[1]));
    let s = proj.scale;
    let top_left = proj.camera.points_to_world(&proj.viewport, [0.0, 0.0]);
    let bottom_right = proj.camera.points_to_world(&proj.viewport, [w / s, h / s]);
    let range = |a: f64, b: f64| {
        let (lo, hi) = (a.min(b), a.max(b));
        let (first, last) = ((lo / spacing_mm).ceil(), (hi / spacing_mm).floor());
        let n = last - first + 1.0;
        (n.is_finite() && n >= 0.0 && n <= MAX_GRID_LINES as f64).then_some((first as i64, last as i64))
    };
    let (Some((x0, x1)), Some((y0, y1))) = (range(top_left.x, bottom_right.x), range(top_left.y, bottom_right.y))
    else {
        return;
    };
    let crisp = |v: f64| (v.floor() + 0.5) as f32;
    let (wf, hf) = (w as f32, h as f32);
    for k in x0..=x1 {
        let x = crisp(proj.px(Point::new(k as f64 * spacing_mm, 0.0))[0]);
        if (0.0..wf).contains(&x) {
            out.push(Primitive::Stroke { points_px: vec![[x, 0.0], [x, hf]], closed: false, width_px: 1.0, color });
        }
    }
    for k in y0..=y1 {
        let y = crisp(proj.px(Point::new(0.0, k as f64 * spacing_mm))[1]);
        if (0.0..hf).contains(&y) {
            out.push(Primitive::Stroke { points_px: vec![[0.0, y], [wf, y]], closed: false, width_px: 1.0, color });
        }
    }
}

/// The cells of a picture that project to finite pixels, or `None` when there is nothing to draw:
/// no usable cell, or an opacity of 0 (or not a number).
fn picture_cells(quads: &[burnburn_editor::BedQuad], opacity: f32, proj: &Projection) -> Option<Vec<ImageCell>> {
    if !(opacity.is_finite() && opacity > 0.0) {
        return None;
    }
    let cells: Vec<ImageCell> = quads
        .iter()
        .filter(|q| q.bed_mm.iter().all(|p| p.x.is_finite() && p.y.is_finite()))
        .map(|q| ImageCell { corners_px: q.bed_mm.map(|p| proj.px32(p)), uv: q.uv })
        .filter(|c| c.corners_px.iter().flatten().all(|v| v.is_finite()))
        .collect();
    (!cells.is_empty()).then_some(cells)
}

/// A camera picture as one image primitive: cells with a corner that does not project to a finite
/// pixel are dropped, and a picture without cells or with an opacity of 0 (or not a number) is
/// not drawn at all.
fn bed_image(
    out: &mut Vec<Primitive>,
    picture: &std::sync::Arc<burnburn_editor::BedPicture>,
    quads: &[burnburn_editor::BedQuad],
    opacity: f32,
    proj: &Projection,
) {
    if let Some(cells) = picture_cells(quads, opacity, proj) {
        out.push(Primitive::Image { picture: std::sync::Arc::clone(picture), cells, opacity: opacity.min(1.0) });
    }
}

/// A bitmap on top of the design (the Adjust Image preview): same rules as [`bed_image`], but a
/// [`Primitive::Bitmap`] that is painted in list order.
fn bitmap(
    out: &mut Vec<Primitive>,
    picture: &std::sync::Arc<burnburn_editor::BedPicture>,
    quads: &[burnburn_editor::BedQuad],
    opacity: f32,
    proj: &Projection,
) {
    if let Some(cells) = picture_cells(quads, opacity, proj) {
        out.push(Primitive::Bitmap { picture: std::sync::Arc::clone(picture), cells, opacity: opacity.min(1.0) });
    }
}

/// Translate one scene into device pixels. Pure, no GPU.
///
/// Painting order: grid, items back to front (fill, then outline), then the other overlays.
pub fn build_packet(scene: &SceneSnapshot, theme: &Theme) -> RenderPacket {
    let size = size_px(&scene.viewport);
    let proj = Projection {
        camera: scene.camera,
        viewport: scene.viewport,
        scale: scene.viewport.backing_scale(),
        tolerance_px: theme.tolerance_px,
    };
    let mut out = Vec::new();
    // Camera pictures lie beneath everything, the grid included.
    for overlay in &scene.overlays {
        if let Overlay::BedImage { picture, quads, opacity } = overlay {
            bed_image(&mut out, picture, quads, *opacity, &proj);
        }
    }
    for overlay in &scene.overlays {
        if let (Overlay::Grid { spacing_mm }, Some(color)) = (overlay, theme.grid) {
            grid(&mut out, *spacing_mm, color, &proj, size);
        }
    }
    for item in &scene.items {
        draw_path(&mut out, flatten(&item.path, &item.transform, &proj), theme.role(item.role), proj.scale);
    }
    // A bitmap preview lies on its image object: above the design, below the selection marks.
    for overlay in &scene.overlays {
        if let Overlay::ImagePreview { picture, quads, opacity } = overlay {
            bitmap(&mut out, picture, quads, *opacity, &proj);
        }
    }
    let selection_width = (theme.selection_width_pt * proj.scale) as f32;
    for overlay in &scene.overlays {
        match overlay {
            Overlay::SelectionBounds { bounds } => {
                out.push(Primitive::Stroke {
                    points_px: bounds_box(bounds, &proj),
                    closed: true,
                    width_px: selection_width,
                    color: theme.selection,
                });
            }
            Overlay::RubberBand { bounds } => {
                let b = bounds_box(bounds, &proj);
                out.push(Primitive::Fill { contours_px: vec![b.clone()], color: theme.rubber_band_fill });
                out.push(Primitive::Stroke {
                    points_px: b,
                    closed: true,
                    width_px: selection_width,
                    color: theme.selection,
                });
            }
            Overlay::DragPreview { path, transform } => {
                let style = RoleStyle { fill: None, stroke: Some(theme.preview), stroke_width_pt: 1.0 };
                draw_path(&mut out, flatten(path, transform, &proj), style, proj.scale);
            }
            // A result not applied yet (centre lines, boolean preview): an outline in the preview colour.
            Overlay::ResultPreview { path } => {
                let style = RoleStyle { fill: None, stroke: Some(theme.preview), stroke_width_pt: 1.5 };
                draw_path(&mut out, flatten(path, &Affine::IDENTITY, &proj), style, proj.scale);
            }
            // A protected outline (T05): heavier, in the selection colour.
            Overlay::ProtectedOutline { path } => {
                let style = RoleStyle { fill: None, stroke: Some(theme.selection), stroke_width_pt: 2.5 };
                draw_path(&mut out, flatten(path, &Affine::IDENTITY, &proj), style, proj.scale);
            }
            // The two variants of an A/B comparison: A broad and dark underneath, B thin and red on top,
            // so where they coincide both can still be seen.
            Overlay::CompareOutline { path, side } => {
                let (color, width_pt) = match side {
                    CompareSide::A => (theme.compare_a, 2.5),
                    CompareSide::B => (theme.compare_b, 1.0),
                };
                let style = RoleStyle { fill: None, stroke: Some(color), stroke_width_pt: width_pt };
                draw_path(&mut out, flatten(path, &Affine::IDENTITY, &proj), style, proj.scale);
            }
            Overlay::Handle { at, .. } => {
                marker(&mut out, *at, theme.handle_size_pt, theme.background, theme.selection, &proj);
            }
            // A selected node is a larger hollow marker, so it stands out from the filled ones.
            Overlay::Node { at, selected: true, .. } => {
                marker(&mut out, *at, theme.handle_size_pt * 1.2, theme.background, theme.selection, &proj);
            }
            Overlay::Node { at, .. } => {
                marker(&mut out, *at, theme.handle_size_pt * 0.8, theme.selection, theme.selection, &proj);
            }
            // Warp/Deform handles: hollow squares, the dragged one larger; an unusable set (a folded
            // quadrilateral or net) is drawn in the problem colour, filled.
            Overlay::ReshapeHandle { at, active, invalid, .. } => {
                let size = theme.handle_size_pt * if *active { 1.4 } else { 1.0 };
                let (fill, line) =
                    if *invalid { (theme.issue, theme.issue) } else { (theme.background, theme.selection) };
                marker(&mut out, *at, size, fill, line, &proj);
            }
            Overlay::TangentCandidate { path, chosen } => {
                let style = if *chosen {
                    RoleStyle { fill: None, stroke: Some(theme.selection), stroke_width_pt: 2.0 }
                } else {
                    RoleStyle { fill: None, stroke: Some(theme.preview), stroke_width_pt: 1.0 }
                };
                draw_path(&mut out, flatten(path, &Affine::IDENTITY, &proj), style, proj.scale);
            }
            Overlay::TangentContact { at } => {
                marker(&mut out, *at, theme.handle_size_pt * 0.6, theme.snap, theme.snap, &proj);
            }
            Overlay::SnapCandidate { at, .. } => {
                marker(&mut out, *at, theme.handle_size_pt * 0.6, theme.snap, theme.snap, &proj);
            }
            Overlay::Issue { at, .. } => {
                marker(&mut out, *at, theme.handle_size_pt, theme.issue, theme.issue, &proj);
            }
            other => sketch::draw(&mut out, other, theme, &proj),
        }
    }
    RenderPacket { size_px: size, clear: theme.background, primitives: out }
}
