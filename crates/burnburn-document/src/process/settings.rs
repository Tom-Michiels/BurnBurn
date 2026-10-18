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

/// Fill (scan) parameters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FillSettings {
    interval: Length,
    axis: ScanAxis,
    direction: ScanDirection,
    overscan: Length,
    angle_deg: f64,
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
        Ok(FillSettings { interval, axis, direction, overscan, angle_deg: 0.0 })
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
        })
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
