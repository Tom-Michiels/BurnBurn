//! Validated process parameters. Every value has an explicit physical unit; constructors reject
//! non-finite and out-of-range input, so an `Operation` can never hold an unusable setting.

use burnburn_types::{ContentHash, Length};

use super::ProcessError;

/// Head speed while the laser works, in mm/s. Finite and strictly positive.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Speed(f64);

impl Speed {
    pub fn mm_per_s(v: f64) -> Result<Self, ProcessError> {
        if v.is_finite() && v > 0.0 { Ok(Speed(v)) } else { Err(ProcessError::NotPositive { field: "speed" }) }
    }

    pub fn get_mm_per_s(self) -> f64 {
        self.0
    }
}

/// Laser power as a percentage of the source's rated power, `0..=100`.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct PowerPercent(f64);

impl PowerPercent {
    pub fn new(pct: f64) -> Result<Self, ProcessError> {
        if pct.is_finite() && (0.0..=100.0).contains(&pct) {
            Ok(PowerPercent(pct))
        } else {
            Err(ProcessError::OutOfRange { field: "power" })
        }
    }

    pub fn get(self) -> f64 {
        self.0
    }
}

/// Minimum (corners/slow segments) and maximum power. `min <= max`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PowerRange {
    min: PowerPercent,
    max: PowerPercent,
}

impl PowerRange {
    pub fn new(min: PowerPercent, max: PowerPercent) -> Result<Self, ProcessError> {
        if min.get() <= max.get() { Ok(PowerRange { min, max }) } else { Err(ProcessError::MinPowerAboveMax) }
    }

    pub fn min(&self) -> PowerPercent {
        self.min
    }

    pub fn max(&self) -> PowerPercent {
        self.max
    }
}

/// Number of times the operation is executed. `1..=MAX`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Passes(u32);

impl Passes {
    /// Upper bound so one setting cannot make a compile unbounded.
    pub const MAX: u32 = 1000;

    pub fn new(n: u32) -> Result<Self, ProcessError> {
        if (1..=Self::MAX).contains(&n) { Ok(Passes(n)) } else { Err(ProcessError::OutOfRange { field: "passes" }) }
    }

    pub fn get(self) -> u32 {
        self.0
    }
}

/// Settings every mode has.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProcessSettings {
    pub speed: Speed,
    pub power: PowerRange,
    pub passes: Passes,
    pub air_assist: bool,
}

/// Total kerf width. The compiler offsets by `width / 2` per edge, outward on outer contours and
/// inward on holes, so the cut part keeps its nominal size. Zero means no compensation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Kerf(f64);

impl Kerf {
    pub const NONE: Kerf = Kerf(0.0);

    pub fn width(w: Length) -> Result<Self, ProcessError> {
        if w.mm() >= 0.0 { Ok(Kerf(w.mm())) } else { Err(ProcessError::OutOfRange { field: "kerf" }) }
    }

    /// Total kerf width in mm.
    pub fn width_mm(self) -> f64 {
        self.0
    }

    pub fn is_none(self) -> bool {
        self.0 == 0.0
    }
}

/// Machine axis the scan lines run along.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScanAxis {
    /// Horizontal lines (scan along X, step in Y).
    X,
    /// Vertical lines (scan along Y, step in X).
    Y,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScanDirection {
    /// Alternate direction on every row.
    Bidirectional,
    /// Every row in the same direction (travel back between rows).
    Unidirectional,
}

/// Which shapes of one operation are scanned together (LightBurn "Fill Grouping").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum FillGrouping {
    /// One scan over all targets: overlaps and enclosed areas follow the even-odd rule across
    /// shapes ("Fill All Shapes at Once"). The default.
    #[default]
    AllShapes,
    /// Shapes of the same outermost group are scanned together, one group after the other;
    /// an ungrouped shape is its own group ("Fill Groups Together").
    Groups,
    /// Every target geometry is scanned on its own ("Fill Shapes Individually").
    Individual,
}

/// Fill (scan) parameters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FillSettings {
    interval: Length,
    axis: ScanAxis,
    direction: ScanDirection,
    overscan: Length,
    angle_deg: f64,
    cross_hatch: bool,
    grouping: FillGrouping,
    flood_fill: bool,
    ramp_mm: f64,
}

impl FillSettings {
    /// Smallest accepted line interval (1 µm); smaller values are below any controller resolution.
    pub const MIN_INTERVAL_MM: f64 = 0.001;

    pub fn new(
        interval: Length,
        axis: ScanAxis,
        direction: ScanDirection,
        overscan: Length,
    ) -> Result<Self, ProcessError> {
        if !(interval.mm() >= Self::MIN_INTERVAL_MM) {
            return Err(ProcessError::OutOfRange { field: "interval" });
        }
        if !(overscan.mm() >= 0.0) {
            return Err(ProcessError::OutOfRange { field: "overscan" });
        }
        Ok(FillSettings {
            interval,
            axis,
            direction,
            overscan,
            angle_deg: 0.0,
            cross_hatch: false,
            grouping: FillGrouping::AllShapes,
            flood_fill: false,
            ramp_mm: 0.0,
        })
    }

    /// The same fill with scan lines rotated by `degrees` (counter-clockwise) from `axis`,
    /// normalised to `[0, 180)`. A non-zero angle needs the `ScanAngle` capability.
    pub fn with_angle(mut self, degrees: f64) -> Result<Self, ProcessError> {
        if !degrees.is_finite() {
            return Err(ProcessError::OutOfRange { field: "angle" });
        }
        let a = degrees.rem_euclid(180.0);
        self.angle_deg = if a >= 180.0 { 0.0 } else { a };
        Ok(self)
    }

    /// A second scan pass perpendicular to the first (needs the `CrossHatch` capability).
    pub fn with_cross_hatch(mut self, on: bool) -> Self {
        self.cross_hatch = on;
        self
    }

    /// Which shapes are scanned together (anything but `AllShapes` needs `FillGrouping`).
    pub fn with_grouping(mut self, grouping: FillGrouping) -> Self {
        self.grouping = grouping;
        self
    }

    /// Scans each connected region completely before the next (needs `FloodFill`).
    pub fn with_flood_fill(mut self, on: bool) -> Self {
        self.flood_fill = on;
        self
    }

    /// Power ramps from minimum to maximum over `ramp` at both ends of each run; zero = off
    /// (needs `RampLength` when positive).
    pub fn with_ramp(mut self, ramp: Length) -> Result<Self, ProcessError> {
        if !(ramp.mm().is_finite() && ramp.mm() >= 0.0) {
            return Err(ProcessError::OutOfRange { field: "ramp length" });
        }
        self.ramp_mm = ramp.mm();
        Ok(self)
    }

    pub fn cross_hatch(&self) -> bool {
        self.cross_hatch
    }
    pub fn grouping(&self) -> FillGrouping {
        self.grouping
    }
    pub fn flood_fill(&self) -> bool {
        self.flood_fill
    }
    pub fn ramp_mm(&self) -> f64 {
        self.ramp_mm
    }

    /// Whether any option beyond interval, axis, direction, overscan and angle is set.
    pub fn has_extras(&self) -> bool {
        self.cross_hatch || self.flood_fill || self.ramp_mm != 0.0 || self.grouping != FillGrouping::AllShapes
    }

    /// Scan-line rotation from `axis` in degrees, `[0, 180)`.
    pub fn angle_deg(&self) -> f64 {
        self.angle_deg
    }

    pub fn interval(&self) -> Length {
        self.interval
    }

    pub fn axis(&self) -> ScanAxis {
        self.axis
    }

    pub fn direction(&self) -> ScanDirection {
        self.direction
    }

    /// Extra travel with the laser off before and after each scan run (acceleration room).
    pub fn overscan(&self) -> Length {
        self.overscan
    }
}

/// Order of the rings of an offset fill.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OffsetFillOrder {
    /// Outermost ring first.
    OutsideIn,
    /// Innermost ring first.
    InsideOut,
}

/// Offset Fill: concentric rings at `step` spacing inside the closed contours, cut as lines.
/// The first ring lies `step / 2` inside the contour (like row centres of a scan fill).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OffsetFillSettings {
    step: Length,
    order: OffsetFillOrder,
}

impl OffsetFillSettings {
    /// Smallest accepted ring step (10 µm): keeps the ring count bounded.
    pub const MIN_STEP_MM: f64 = 0.01;

    pub fn new(step: Length, order: OffsetFillOrder) -> Result<Self, ProcessError> {
        if !(step.mm() >= Self::MIN_STEP_MM) {
            return Err(ProcessError::OutOfRange { field: "offset step" });
        }
        Ok(OffsetFillSettings { step, order })
    }

    pub fn step(&self) -> Length {
        self.step
    }

    pub fn order(&self) -> OffsetFillOrder {
        self.order
    }
}

/// Dither choice stored with an image operation; the pixels are dithered at compile time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DitherMode {
    /// Burn where the toned gray value is below `level`.
    Threshold {
        level: u8,
    },
    FloydSteinberg,
    Jarvis,
    Stucki,
    Atkinson,
    /// Bayer matrix of size 2, 4 or 8.
    Ordered {
        size: u8,
    },
    /// 45° round-dot screen; the cell size comes from [`ImageSettings::screen`].
    Newsprint,
    /// Round-dot screen with cell size and angle from [`ImageSettings::screen`].
    Halftone,
    /// Sobel edge lines: burn where the edge strength (`0..=255`) is at least `level`.
    Sketch {
        level: u8,
    },
    /// The source pixels as they are: one source pixel per dot, thresholded at 128, no
    /// resampling and no adjustments.
    PassThrough,
}

/// How colour becomes gray (before transparency is composited over paper).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum GrayMethod {
    /// Rec. 709 luma `0.2126 R + 0.7152 G + 0.0722 B`.
    #[default]
    Rec709,
    /// Rec. 601 luma `0.299 R + 0.587 G + 0.114 B`.
    Rec601,
    Average,
    /// `(max + min) / 2` of the channels.
    Lightness,
    Red,
    Green,
    Blue,
}

/// A crop of the source image in source pixels: `[x, x+width) × [y, y+height)`, y down.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ImageCrop {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// Unsharp mask applied on the physical output grid. Stored in fixed units (amount in
/// hundredths, radius in hundredths of a millimetre) so `ImageSettings` stays small and `Copy`
/// (it travels inside UI intents).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageSharpen {
    amount_centi: u16,
    radius_centi_mm: u16,
}

impl ImageSharpen {
    /// `amount ∈ [0.01, 10]` (resolution 0.01), `radius ∈ [0.01 mm, 100 mm]` (resolution 0.01 mm).
    pub fn new(amount: f64, radius: Length) -> Result<Self, ProcessError> {
        let centi = (amount * 100.0).round();
        if !(amount.is_finite() && (1.0..=1000.0).contains(&centi)) {
            return Err(ProcessError::OutOfRange { field: "sharpen amount" });
        }
        let radius_centi = (radius.mm() * 100.0).round();
        if !(1.0..=10_000.0).contains(&radius_centi) {
            return Err(ProcessError::OutOfRange { field: "sharpen radius" });
        }
        Ok(ImageSharpen { amount_centi: centi as u16, radius_centi_mm: radius_centi as u16 })
    }

    pub fn amount(&self) -> f64 {
        f64::from(self.amount_centi) / 100.0
    }
    pub fn radius_mm(&self) -> f64 {
        f64::from(self.radius_centi_mm) / 100.0
    }
}

/// Cell size and angle of the Halftone screen (Newsprint always uses 45°). Stored as hundredths
/// of a millimetre and of a degree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenSettings {
    cell_centi_mm: u16,
    angle_centi: u16,
}

impl ScreenSettings {
    /// `cell ∈ [0.01 mm, 500 mm]` (resolution 0.01 mm); the angle is normalised to `[0°, 180°)`
    /// (resolution 0.01°).
    pub fn new(cell: Length, angle_deg: f64) -> Result<Self, ProcessError> {
        let cell_centi = (cell.mm() * 100.0).round();
        if !(1.0..=50_000.0).contains(&cell_centi) {
            return Err(ProcessError::OutOfRange { field: "screen cell" });
        }
        let angle_centi = angle_to_centi(angle_deg).ok_or(ProcessError::OutOfRange { field: "screen angle" })?;
        Ok(ScreenSettings { cell_centi_mm: cell_centi as u16, angle_centi })
    }

    pub fn cell_mm(&self) -> f64 {
        f64::from(self.cell_centi_mm) / 100.0
    }
    pub fn angle_deg(&self) -> f64 {
        f64::from(self.angle_centi) / 100.0
    }
}

impl Default for ScreenSettings {
    /// 0.5 mm cells at 45°.
    fn default() -> Self {
        ScreenSettings { cell_centi_mm: 50, angle_centi: 4500 }
    }
}

/// An angle in hundredths of a degree within `[0, 18000)`; `None` when not finite.
fn angle_to_centi(angle_deg: f64) -> Option<u16> {
    if !angle_deg.is_finite() {
        return None;
    }
    let centi = (angle_deg.rem_euclid(180.0) * 100.0).round() as u32;
    Some((centi % 18_000) as u16)
}

/// How an image becomes laser output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImageRender {
    /// Binary on/off dots (IM18).
    Dither(DitherMode),
    /// Power modulated by gray value. Needs the `GrayscaleModulation` capability, which no
    /// profile has proven (IM25 semantics experiment); never replaced by dithering.
    Grayscale,
}

/// Tone correction: `out = clamp(contrast · (v − ½) + ½ + brightness, 0, 1) ^ gamma`, with
/// `gamma ∈ [0.1, 10]`, `brightness ∈ [−1, 1]`, `contrast ∈ [0, 10]`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ToneSettings {
    gamma: f64,
    brightness: f64,
    contrast: f64,
}

impl ToneSettings {
    pub const IDENTITY: ToneSettings = ToneSettings { gamma: 1.0, brightness: 0.0, contrast: 1.0 };

    pub fn new(gamma: f64, brightness: f64, contrast: f64) -> Result<Self, ProcessError> {
        let within = |v: f64, lo: f64, hi: f64| v.is_finite() && v >= lo && v <= hi;
        if !within(gamma, 0.1, 10.0) {
            return Err(ProcessError::OutOfRange { field: "gamma" });
        }
        if !within(brightness, -1.0, 1.0) {
            return Err(ProcessError::OutOfRange { field: "brightness" });
        }
        if !within(contrast, 0.0, 10.0) {
            return Err(ProcessError::OutOfRange { field: "contrast" });
        }
        Ok(ToneSettings { gamma, brightness, contrast })
    }

    pub fn gamma(&self) -> f64 {
        self.gamma
    }
    pub fn brightness(&self) -> f64 {
        self.brightness
    }
    pub fn contrast(&self) -> f64 {
        self.contrast
    }
}

/// Image engraving: the source pixels (`source` = content hash of the original pixels) are
/// stretched over the bounds of the target geometry, resampled to `dot_pitch × line_interval`
/// and rendered. Scan lines run along X. The processed raster is never document data.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImageSettings {
    source: ContentHash,
    dot_pitch: Length,
    line_interval: Length,
    render: ImageRender,
    tone: ToneSettings,
    direction: ScanDirection,
    serpentine: bool,
    overscan_mm: f64,
    negative: bool,
    dot_width_correction_um: u16,
    scan_angle_centi: u16,
    gray: GrayMethod,
    crop: Option<ImageCrop>,
    sharpen: Option<ImageSharpen>,
    screen: ScreenSettings,
    mask: bool,
}

impl ImageSettings {
    /// Smallest accepted pitch/interval (1 µm), as for fills.
    pub const MIN_PITCH_MM: f64 = 0.001;

    /// Bidirectional, identity tone, no serpentine, no overscan.
    pub fn new(
        source: ContentHash,
        dot_pitch: Length,
        line_interval: Length,
        render: ImageRender,
    ) -> Result<Self, ProcessError> {
        if !(dot_pitch.mm() >= Self::MIN_PITCH_MM) {
            return Err(ProcessError::OutOfRange { field: "dot pitch" });
        }
        if !(line_interval.mm() >= Self::MIN_PITCH_MM) {
            return Err(ProcessError::OutOfRange { field: "interval" });
        }
        Ok(ImageSettings {
            source,
            dot_pitch,
            line_interval,
            render,
            tone: ToneSettings::IDENTITY,
            direction: ScanDirection::Bidirectional,
            serpentine: false,
            overscan_mm: 0.0,
            negative: false,
            dot_width_correction_um: 0,
            scan_angle_centi: 0,
            gray: GrayMethod::Rec709,
            crop: None,
            sharpen: None,
            screen: ScreenSettings::default(),
            mask: false,
        })
    }

    /// The same settings for other original pixels (Replace/Refresh Image): every option stays;
    /// the caller drops a crop that does not fit the new pixels.
    pub fn with_source(mut self, source: ContentHash) -> Self {
        self.source = source;
        self
    }

    /// Burn light instead of dark: the toned gray value is inverted before dithering or
    /// power mapping.
    pub fn with_negative(mut self, negative: bool) -> Self {
        self.negative = negative;
        self
    }

    /// Each laser-on run is shortened by `correction` in total (half at each end) to offset the
    /// beam being wider than the dot pitch; runs that vanish are dropped. Range 0–65 mm,
    /// resolution 1 µm.
    pub fn with_dot_width_correction(mut self, correction: Length) -> Result<Self, ProcessError> {
        let um = (correction.mm() * 1000.0).round();
        if !(0.0..=65_535.0).contains(&um) {
            return Err(ProcessError::OutOfRange { field: "dot width correction" });
        }
        self.dot_width_correction_um = um as u16;
        Ok(self)
    }

    /// Scan line angle in degrees from bed X, counter-clockwise, normalised to `[0, 180)`
    /// (resolution 0.01°).
    pub fn with_scan_angle(mut self, angle_deg: f64) -> Result<Self, ProcessError> {
        self.scan_angle_centi = angle_to_centi(angle_deg).ok_or(ProcessError::OutOfRange { field: "scan angle" })?;
        Ok(self)
    }

    pub fn with_gray_method(mut self, gray: GrayMethod) -> Self {
        self.gray = gray;
        self
    }

    /// Engrave only this window of the source pixels; it is stretched over the target bounds.
    pub fn with_crop(mut self, crop: Option<ImageCrop>) -> Result<Self, ProcessError> {
        if crop.is_some_and(|c| c.width == 0 || c.height == 0) {
            return Err(ProcessError::OutOfRange { field: "crop" });
        }
        self.crop = crop;
        Ok(self)
    }

    pub fn with_sharpen(mut self, sharpen: Option<ImageSharpen>) -> Self {
        self.sharpen = sharpen;
        self
    }

    pub fn with_screen(mut self, screen: ScreenSettings) -> Self {
        self.screen = screen;
        self
    }

    /// Burn only where the cell centre lies inside the target geometry (even-odd), instead of
    /// over its whole bounding box.
    pub fn with_mask(mut self, mask: bool) -> Self {
        self.mask = mask;
        self
    }

    /// Whether any option added after IM18 differs from its default. Such settings have no
    /// representation in file schema 1 and must not be saved as if they had.
    pub fn has_extended_options(&self) -> bool {
        self.negative
            || self.dot_width_correction_um != 0
            || self.scan_angle_centi != 0
            || self.gray != GrayMethod::Rec709
            || self.crop.is_some()
            || self.sharpen.is_some()
            || self.screen != ScreenSettings::default()
            || self.mask
    }

    pub fn with_tone(mut self, tone: ToneSettings) -> Self {
        self.tone = tone;
        self
    }

    pub fn with_direction(mut self, direction: ScanDirection, serpentine: bool) -> Self {
        self.direction = direction;
        self.serpentine = serpentine;
        self
    }

    pub fn with_overscan(mut self, overscan: Length) -> Result<Self, ProcessError> {
        if !(overscan.mm() >= 0.0) {
            return Err(ProcessError::OutOfRange { field: "overscan" });
        }
        self.overscan_mm = overscan.mm();
        Ok(self)
    }

    pub fn source(&self) -> ContentHash {
        self.source
    }
    pub fn dot_pitch(&self) -> Length {
        self.dot_pitch
    }
    pub fn line_interval(&self) -> Length {
        self.line_interval
    }
    pub fn render(&self) -> ImageRender {
        self.render
    }
    pub fn tone(&self) -> ToneSettings {
        self.tone
    }
    pub fn direction(&self) -> ScanDirection {
        self.direction
    }
    /// Error diffusion runs odd pixel rows right to left.
    pub fn serpentine(&self) -> bool {
        self.serpentine
    }
    /// Laser-off run-up/run-out per scan run, mm.
    pub fn overscan_mm(&self) -> f64 {
        self.overscan_mm
    }
    pub fn negative(&self) -> bool {
        self.negative
    }
    /// Total shortening of every laser-on run, mm.
    pub fn dot_width_correction_mm(&self) -> f64 {
        f64::from(self.dot_width_correction_um) / 1000.0
    }
    /// Scan line angle in degrees `[0, 180)` from bed X.
    pub fn scan_angle_deg(&self) -> f64 {
        f64::from(self.scan_angle_centi) / 100.0
    }
    pub fn gray_method(&self) -> GrayMethod {
        self.gray
    }
    pub fn crop(&self) -> Option<ImageCrop> {
        self.crop
    }
    pub fn sharpen(&self) -> Option<ImageSharpen> {
        self.sharpen
    }
    pub fn screen(&self) -> ScreenSettings {
        self.screen
    }
    pub fn mask(&self) -> bool {
        self.mask
    }
}

/// What the operation does with its geometry.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum OperationMode {
    /// Follow the contours (cut or score).
    Line { kerf: Kerf },
    /// Scan the enclosed area line by line.
    Fill(FillSettings),
    /// Concentric offset rings, cut as lines (IM25).
    OffsetFill(OffsetFillSettings),
    /// Engrave a source image over the bounds of the target geometry (IM18).
    Image(ImageSettings),
}
