//! The toolpath preview as a picture (LF11), without AppKit or a GPU: an anti-aliased raster that
//! both native hosts show.
//!
//! * A cut is drawn in the colour of its layer; a stretch at a lowered power (a tab) in orange.
//! * Travel (laser off) is a dashed grey line, drawn only when it is shown.
//! * What has been played is the same colour at [`PLAYED_OPACITY`] (35 %): dimmed, never another
//!   hue, so the contrast is not red against green.
//! * The picked move is the accent colour, heavier, on a white halo.
//! * The head is a cross in a circle.
//! * The legend is a drawn swatch plus a word per role ([`preview_legend`], [`render_swatch`]):
//!   a colour is never the only carrier.
//!
//! Strokes are rasterised by distance to the segment: coverage falls off linearly over one pixel
//! on both edges, which is what gives a slanted line its intermediate tones. Strokes of one pass
//! are merged by maximum coverage, so a polyline of dimmed segments has no darker joints.

use burnburn_presentation::{MoveKind, PartialMove, PlanPreview, PreviewMove, PreviewViewModel};

use crate::app_text::{AppText, app_text};
use crate::app_view::PreviewFrame;
use crate::l10n::Locale;
use crate::panels_image::BitmapImage;

/// How the preview is coloured and how thick its strokes are.
#[derive(Debug, Clone, PartialEq)]
pub struct PreviewStyle {
    /// sRGB colour of layer palette slot `n` (`layers[n % len]`); empty draws every cut in
    /// [`CUT_FALLBACK`]. Hosts pass the canvas theme's palette, so the preview, the operations
    /// panel and the canvas agree.
    pub layers: Vec<[u8; 3]>,
    /// Device pixels per point: stroke widths and marker sizes are in points.
    pub pixel_ratio: f64,
}

impl Default for PreviewStyle {
    fn default() -> Self {
        PreviewStyle { layers: Vec::new(), pixel_ratio: 1.0 }
    }
}

impl PreviewStyle {
    /// The colour of a move of the operation in palette `slot`.
    fn layer(&self, slot: Option<u8>) -> [u8; 3] {
        match (slot, self.layers.is_empty()) {
            (Some(slot), false) => self.layers[usize::from(slot) % self.layers.len()],
            _ => CUT_FALLBACK,
        }
    }

    fn ratio(&self) -> f64 {
        if self.pixel_ratio.is_finite() && self.pixel_ratio > 0.0 { self.pixel_ratio } else { 1.0 }
    }
}

/// How much of its colour what has been played keeps: the plan's "35 % dimmed".
pub const PLAYED_OPACITY: f64 = 0.35;
/// A cut whose layer is unknown (no layer palette given).
pub const CUT_FALLBACK: [u8; 3] = [220, 40, 40];
const LOWERED: [u8; 3] = [235, 140, 20];
const TRAVEL: [u8; 3] = [140, 140, 145];
const PICKED: [u8; 3] = [0, 100, 255];
const HEAD: [u8; 3] = [24, 24, 28];
const BED_FILL: [u8; 3] = [245, 245, 245];
const BED_EDGE: [u8; 3] = [200, 200, 200];
const WHITE: [u8; 3] = [255, 255, 255];

// Sizes in points.
const CUT_WIDTH: f64 = 1.6;
const TRAVEL_WIDTH: f64 = 1.1;
const TRAVEL_DASH: (f64, f64) = (4.0, 3.0);
const PICKED_WIDTH: f64 = 3.0;
const PICKED_HALO: f64 = 5.5;
const HEAD_RADIUS: f64 = 6.0;
const HEAD_ARM: f64 = 9.0;
const HEAD_WIDTH: f64 = 1.4;

// ---- legend ----------------------------------------------------------------------------------

/// What a legend swatch shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Swatch {
    Cut,
    LoweredPower,
    Travel,
    Played,
    Picked,
    Head,
}

/// A drawn swatch and the word that names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LegendEntry {
    pub swatch: Swatch,
    pub word: &'static str,
}

/// The legend, in reading order.
pub fn preview_legend(locale: Locale) -> Vec<LegendEntry> {
    let t = |text| app_text(text, locale);
    vec![
        LegendEntry { swatch: Swatch::Cut, word: t(AppText::PreviewLegendCut) },
        LegendEntry { swatch: Swatch::LoweredPower, word: t(AppText::PreviewLegendLowered) },
        LegendEntry { swatch: Swatch::Travel, word: t(AppText::PreviewLegendTravel) },
        LegendEntry { swatch: Swatch::Played, word: t(AppText::PreviewLegendPlayed) },
        LegendEntry { swatch: Swatch::Picked, word: t(AppText::PreviewLegendPicked) },
        LegendEntry { swatch: Swatch::Head, word: t(AppText::PreviewLegendHead) },
    ]
}

/// The swatch drawn like the preview draws it, on a tile of the bed colour.
pub fn render_swatch(swatch: Swatch, size: [u32; 2], style: &PreviewStyle) -> BitmapImage {
    let mut canvas = Canvas::new(size[0], size[1], BED_FILL);
    let r = style.ratio();
    let (w, h) = (f64::from(size[0]), f64::from(size[1]));
    let y = (h / 2.0).floor() + 0.5;
    let (a, b) = ([h * 0.35, y], [w - h * 0.35, y]);
    let ink = style.layer(Some(0));
    match swatch {
        Swatch::Cut => canvas.pass(1.0, [(a, b, CUT_WIDTH * r, ink, None)]),
        Swatch::LoweredPower => canvas.pass(1.0, [(a, b, CUT_WIDTH * r, LOWERED, None)]),
        Swatch::Travel => {
            canvas.pass(1.0, [(a, b, TRAVEL_WIDTH * r, TRAVEL, Some((TRAVEL_DASH.0 * r, TRAVEL_DASH.1 * r)))])
        }
        Swatch::Played => canvas.pass(PLAYED_OPACITY, [(a, b, CUT_WIDTH * r, ink, None)]),
        Swatch::Picked => {
            canvas.pass(0.9, [(a, b, PICKED_HALO * r, WHITE, None)]);
            canvas.pass(1.0, [(a, b, PICKED_WIDTH * r, PICKED, None)]);
        }
        Swatch::Head => canvas.head([w / 2.0, y], r),
    }
    canvas.into_bitmap()
}

// ---- scrubbing -------------------------------------------------------------------------------

/// The plan event of the move under the estimate clock at `seconds`: the move being drawn, or the
/// next one when the clock sits exactly between two. Past the end it is the last move; before the
/// start (or not a number) the first. `None` for a plan without moves. This is the same walk the
/// controller does for the head, so the scrubbed picture and the event agree.
pub fn event_at_seconds(preview: &PlanPreview, seconds: f64) -> Option<usize> {
    let position = if seconds.is_finite() { seconds } else { 0.0 };
    let mut at = 0.0;
    for m in &preview.moves {
        if at + m.seconds <= position {
            at += m.seconds;
            continue;
        }
        return Some(m.event);
    }
    preview.moves.last().map(|m| m.event)
}

// ---- raster ----------------------------------------------------------------------------------

/// One stroke: from, to, width (device pixels), colour and an optional dash (on, off) in pixels.
type Stroke = ([f64; 2], [f64; 2], f64, [u8; 3], Option<(f64, f64)>);

struct Canvas {
    width: usize,
    height: usize,
    rgba: Vec<u8>,
}

/// The coverage and colour of the strokes of one pass; the strongest stroke wins a pixel.
struct Mask {
    cover: Vec<f32>,
    colour: Vec<[u8; 3]>,
}

impl Canvas {
    fn new(width: u32, height: u32, fill: [u8; 3]) -> Canvas {
        let (width, height) = (width as usize, height as usize);
        let mut rgba = vec![255; width * height * 4];
        for px in rgba.as_chunks_mut::<4>().0 {
            px[..3].copy_from_slice(&fill);
        }
        Canvas { width, height, rgba }
    }

    fn into_bitmap(self) -> BitmapImage {
        BitmapImage { width: self.width as u32, height: self.height as u32, rgba: self.rgba }
    }

    fn mask(&self) -> Mask {
        Mask { cover: vec![0.0; self.width * self.height], colour: vec![[0; 3]; self.width * self.height] }
    }

    /// Strokes the segments into one mask and lays it over the picture with `opacity`.
    fn pass<I: IntoIterator<Item = Stroke>>(&mut self, opacity: f64, strokes: I) {
        let mut mask = self.mask();
        for (a, b, width, colour, dash) in strokes {
            self.stroke(&mut mask, a, b, width, colour, dash);
        }
        self.composite(&mask, opacity);
    }

    fn stroke(&self, mask: &mut Mask, a: [f64; 2], b: [f64; 2], width: f64, colour: [u8; 3], dash: Option<(f64, f64)>) {
        let reach = width / 2.0 + 1.0;
        let (x0, x1) =
            ((a[0].min(b[0]) - reach).floor().max(0.0) as usize, (a[0].max(b[0]) + reach).ceil().max(0.0) as usize);
        let (y0, y1) =
            ((a[1].min(b[1]) - reach).floor().max(0.0) as usize, (a[1].max(b[1]) + reach).ceil().max(0.0) as usize);
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let len2 = dx * dx + dy * dy;
        let len = len2.sqrt();
        for y in y0..=y1.min(self.height.saturating_sub(1)) {
            for x in x0..=x1.min(self.width.saturating_sub(1)) {
                let (px, py) = (x as f64 + 0.5, y as f64 + 0.5);
                let t = if len2 > 0.0 { (((px - a[0]) * dx + (py - a[1]) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
                let d = (px - (a[0] + t * dx)).hypot(py - (a[1] + t * dy));
                let mut cover = (width / 2.0 + 0.5 - d).clamp(0.0, 1.0);
                if let Some((on, off)) = dash.filter(|(on, off)| on + off > 0.0 && *on > 0.0) {
                    let m = (t * len) % (on + off);
                    cover *= (m.min(on - m) + 0.5).clamp(0.0, 1.0);
                }
                mask.put(y * self.width + x, cover as f32, colour);
            }
        }
    }

    fn ring(&self, mask: &mut Mask, centre: [f64; 2], radius: f64, width: f64, colour: [u8; 3]) {
        let reach = radius + width / 2.0 + 1.0;
        let (x0, x1) = ((centre[0] - reach).floor().max(0.0) as usize, (centre[0] + reach).ceil().max(0.0) as usize);
        let (y0, y1) = ((centre[1] - reach).floor().max(0.0) as usize, (centre[1] + reach).ceil().max(0.0) as usize);
        for y in y0..=y1.min(self.height.saturating_sub(1)) {
            for x in x0..=x1.min(self.width.saturating_sub(1)) {
                let d = ((x as f64 + 0.5) - centre[0]).hypot((y as f64 + 0.5) - centre[1]);
                let cover = (width / 2.0 + 0.5 - (d - radius).abs()).clamp(0.0, 1.0);
                mask.put(y * self.width + x, cover as f32, colour);
            }
        }
    }

    fn composite(&mut self, mask: &Mask, opacity: f64) {
        for (i, px) in self.rgba.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let a = f64::from(mask.cover[i]) * opacity;
            if a <= 0.0 {
                continue;
            }
            for (value, ink) in px.iter_mut().zip(mask.colour[i]) {
                *value = (f64::from(*value) * (1.0 - a) + f64::from(ink) * a).round() as u8;
            }
        }
    }

    /// The head: a cross inside a circle, on a white halo so it reads over any stroke.
    fn head(&mut self, at: [f64; 2], ratio: f64) {
        let (r, arm) = (HEAD_RADIUS * ratio, HEAD_ARM * ratio);
        let cross = |width: f64, colour| {
            [
                ([at[0] - arm, at[1]], [at[0] + arm, at[1]], width, colour, None),
                ([at[0], at[1] - arm], [at[0], at[1] + arm], width, colour, None),
            ]
        };
        let mut halo = self.mask();
        self.ring(&mut halo, at, r, (HEAD_WIDTH + 2.4) * ratio, WHITE);
        for (a, b, w, c, d) in cross((HEAD_WIDTH + 2.4) * ratio, WHITE) {
            self.stroke(&mut halo, a, b, w, c, d);
        }
        self.composite(&halo, 0.85);
        let mut ink = self.mask();
        self.ring(&mut ink, at, r, HEAD_WIDTH * ratio, HEAD);
        for (a, b, w, c, d) in cross(HEAD_WIDTH * ratio, HEAD) {
            self.stroke(&mut ink, a, b, w, c, d);
        }
        self.composite(&ink, 1.0);
    }
}

impl Mask {
    fn put(&mut self, i: usize, cover: f32, colour: [u8; 3]) {
        if cover > self.cover[i] {
            self.cover[i] = cover;
            self.colour[i] = colour;
        }
    }
}

fn burning(kind: MoveKind) -> bool {
    matches!(kind, MoveKind::Cut | MoveKind::Scan | MoveKind::Dot)
}

/// The preview with the default style (cuts in [`CUT_FALLBACK`], one device pixel per point).
pub fn render_preview(view: &PreviewViewModel, frame: &PreviewFrame) -> BitmapImage {
    render_preview_styled(view, frame, &PreviewStyle::default())
}

/// The preview as a bitmap: the bed, laser-off moves (dashed, when shown), what has been played
/// (dimmed), what is still to do (full colour), the picked move on top and the head.
pub fn render_preview_styled(view: &PreviewViewModel, frame: &PreviewFrame, style: &PreviewStyle) -> BitmapImage {
    let [width, height] = frame.size_px();
    let mut canvas = Canvas::new(width, height, WHITE);
    let r = style.ratio();
    if let Some(bed) = view.bed_mm {
        let (a, b) = (frame.to_px([0.0, bed[1]]), frame.to_px([bed[0], 0.0]));
        let edge = r.round().max(1.0);
        fill_rect(&mut canvas, a, b, BED_EDGE);
        fill_rect(&mut canvas, [a[0] + edge, a[1] + edge], [b[0] - edge, b[1] - edge], BED_FILL);
    }
    let moves = &view.preview.moves;
    let px = |p: burnburn_presentation::MachinePoint| frame.to_px([p.x_mm(), p.y_mm()]);
    let colour_of = |m: &PreviewMove| {
        if m.power.is_some() {
            LOWERED
        } else {
            style.layer(m.operation.and_then(|op| view.layer_slots.get(&op).copied()))
        }
    };
    if view.show_travel {
        let travel = moves
            .iter()
            .filter(|m| !burning(m.kind))
            .map(|m| (px(m.from), px(m.to), TRAVEL_WIDTH * r, TRAVEL, Some((TRAVEL_DASH.0 * r, TRAVEL_DASH.1 * r))));
        canvas.pass(1.0, travel);
    }
    // Played (dimmed) below, still to do on top. The move the clock is in is split at its fraction.
    let mut played: Vec<Stroke> = Vec::new();
    let mut todo: Vec<Stroke> = Vec::new();
    for (i, m) in moves.iter().enumerate().filter(|(_, m)| burning(m.kind)) {
        let (a, b, colour, w) = (px(m.from), px(m.to), colour_of(m), CUT_WIDTH * r);
        match view.partial {
            Some(PartialMove { index, fraction }) if index == i => {
                let split = [a[0] + (b[0] - a[0]) * fraction, a[1] + (b[1] - a[1]) * fraction];
                played.push((a, split, w, colour, None));
                todo.push((split, b, w, colour, None));
            }
            _ if i < view.completed_moves => played.push((a, b, w, colour, None)),
            _ => todo.push((a, b, w, colour, None)),
        }
    }
    canvas.pass(PLAYED_OPACITY, played);
    canvas.pass(1.0, todo);
    if let Some(m) = view.selected.and_then(|p| moves.get(p.index)) {
        let (a, b) = (px(m.from), px(m.to));
        canvas.pass(0.9, [(a, b, PICKED_HALO * r, WHITE, None)]);
        canvas.pass(1.0, [(a, b, PICKED_WIDTH * r, PICKED, None)]);
    }
    canvas.head(frame.to_px(view.head), r);
    canvas.into_bitmap()
}

fn fill_rect(canvas: &mut Canvas, min: [f64; 2], max: [f64; 2], colour: [u8; 3]) {
    let (x0, x1) = (min[0].round().max(0.0) as usize, max[0].round().max(0.0) as usize);
    let (y0, y1) = (min[1].round().max(0.0) as usize, max[1].round().max(0.0) as usize);
    for y in y0..=y1.min(canvas.height.saturating_sub(1)) {
        for x in x0..=x1.min(canvas.width.saturating_sub(1)) {
            let i = (y * canvas.width + x) * 4;
            canvas.rgba[i..i + 3].copy_from_slice(&colour);
        }
    }
}
