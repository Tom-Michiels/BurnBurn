use burnburn_types::{CancellationToken, CanonicalEncode, CanonicalWriter, ContentHash};

use crate::adjust::{Orientation, PixelRect, QuarterTurn};
use crate::error::ImageError;
use crate::gray::{AlphaPolicy, GrayImage, median_filter, to_gray};
use crate::mask::{BinaryMask, Morphology, RegionMask, apply_morphology, apply_regions};
use crate::perspective::{Perspective, PerspectiveReport, warp};
use crate::scale::{PixelScale, ScaleError};
use crate::source::SourceImage;
use crate::threshold::{MAX_ADAPTIVE_RADIUS, Threshold, otsu_threshold};

/// Every parameter that turns a source image into a binary mask. Part of the trace recipe;
/// applying it never changes the source image.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ImageRecipe {
    /// Declared physical scale of *source* pixels. `None` → [`ScaleError::NeedsScale`].
    pub scale: Option<PixelScale>,
    /// Crop in source pixel coordinates; `None` = whole image.
    pub crop: Option<PixelRect>,
    pub rotation: QuarterTurn,
    pub alpha: AlphaPolicy,
    /// Median radius in pixels (0 = off), applied to the grayscale working image.
    pub median_radius: u32,
    pub threshold: Threshold,
    /// Light-on-dark: foreground is brighter than the threshold.
    pub invert: bool,
    /// In adjusted (cropped, rotated) pixel coordinates.
    pub regions: Vec<RegionMask>,
    /// Applied in order after the region masks.
    pub morphology: Vec<Morphology>,
    /// Optional four(+)-point plane rectification after crop/rotation; regions, morphology and the
    /// traced result are then in rectified pixels.
    pub perspective: Option<Perspective>,
}

/// The binarised working image handed to the tracer.
///
/// Invariant: `field[i] > 0` exactly when `mask` is foreground at `i`. Where the mask follows the
/// threshold, `field` is the signed gray distance to the threshold (sub-pixel edge information);
/// where a region mask or morphology changed the decision it is `±0.5`.
#[derive(Debug, Clone, PartialEq)]
pub struct PreparedImage {
    pub source_hash: ContentHash,
    /// Scale of adjusted pixels (swapped after a quarter turn).
    pub scale: PixelScale,
    pub threshold: u8,
    pub mask: BinaryMask,
    pub field: Vec<f32>,
    /// The applied plane transform and its residual, when the recipe has a perspective.
    pub perspective: Option<PerspectiveReport>,
}

impl PreparedImage {
    pub fn width(&self) -> u32 {
        self.mask.width()
    }

    pub fn height(&self) -> u32 {
        self.mask.height()
    }

    pub fn field_at(&self, x: u32, y: u32) -> f32 {
        self.field.get(y as usize * self.width() as usize + x as usize).copied().unwrap_or(-0.5)
    }
}

/// Runs crop → rotate → grayscale → median → threshold → regions → morphology.
pub fn prepare(
    source: &SourceImage,
    recipe: &ImageRecipe,
    cancel: &CancellationToken,
) -> Result<PreparedImage, ImageError> {
    let scale = recipe.scale.ok_or(ScaleError::NeedsScale)?;
    let crop = recipe.crop.unwrap_or(PixelRect { x: 0, y: 0, width: source.width(), height: source.height() });
    crop.validate_within(source.width(), source.height())?;
    for r in &recipe.regions {
        r.shape.validate()?;
    }
    let orientation = Orientation { crop, turn: recipe.rotation };
    let (w, h) = orientation.size();
    let scale = match recipe.rotation {
        QuarterTurn::Clockwise90 | QuarterTurn::CounterClockwise90 => scale.swapped(),
        QuarterTurn::None | QuarterTurn::Half => scale,
    };

    let mut values = Vec::with_capacity(w as usize * h as usize);
    for y in 0..h {
        if y % 64 == 0 {
            cancel.check()?;
        }
        for x in 0..w {
            let (sx, sy) = orientation.source_of(x, y);
            values.push(to_gray(source.rgba_at(sx, sy), recipe.alpha));
        }
    }
    let mut gray = GrayImage { width: w, height: h, values };
    let mut report = None;
    if let Some(p) = &recipe.perspective {
        let (warped, r) = warp(&gray, p, cancel)?;
        gray = warped;
        report = Some(r);
    }
    let (w, h) = (gray.width, gray.height);
    let gray = median_filter(&gray, recipe.median_radius, cancel)?;
    cancel.check()?;

    // Signed distance to the decision level: foreground ⇔ > 0. `t + 0.5` keeps integer gray values
    // off the level, so the field is never exactly zero for 8-bit sources.
    let signed = |level: f32, g: f32| if recipe.invert { g - level } else { level - g };
    let (threshold, mut field): (u8, Vec<f32>) = match recipe.threshold {
        Threshold::Fixed(t) => (t, gray.values.iter().map(|&g| signed(f32::from(t) + 0.5, g)).collect()),
        Threshold::Otsu => {
            let t = otsu_threshold(&gray);
            (t, gray.values.iter().map(|&g| signed(f32::from(t) + 0.5, g)).collect())
        }
        Threshold::Adaptive { radius, offset } => adaptive_field(&gray, radius, offset, recipe.invert, cancel)?,
    };
    let thresholded = BinaryMask::from_fn(w, h, |x, y| field.get((y * w + x) as usize).is_some_and(|&v| v > 0.0));

    let mut mask = thresholded.clone();
    apply_regions(&mut mask, &recipe.regions)?;
    for &op in &recipe.morphology {
        cancel.check()?;
        mask = apply_morphology(&mask, op, cancel)?;
    }
    // Where masks or morphology overrode the threshold, the gray value no longer describes the
    // edge: use a neutral ±0.5 so the sub-pixel boundary falls midway between pixel centres.
    for ((v, &now), &before) in field.iter_mut().zip(mask.bits()).zip(thresholded.bits()) {
        if now != before || (now && *v <= 0.0) || (!now && *v > 0.0) {
            *v = if now { 0.5 } else { -0.5 };
        }
    }
    cancel.check()?;
    Ok(PreparedImage { source_hash: source.content_hash(), scale, threshold, mask, field, perspective: report })
}

impl CanonicalEncode for ImageRecipe {
    fn encode(&self, out: &mut CanonicalWriter) {
        out.tag("image-recipe-v1");
        match &self.scale {
            Some(s) => s.encode(out),
            None => out.tag("no-scale"),
        }
        match &self.crop {
            Some(c) => c.encode(out),
            None => out.tag("no-crop"),
        }
        self.rotation.encode(out);
        self.alpha.encode(out);
        out.u64(u64::from(self.median_radius));
        self.threshold.encode(out);
        out.bool(self.invert);
        out.seq(self.regions.iter());
        out.seq(self.morphology.iter());
        // Only written when present, so keys of recipes without perspective stay unchanged.
        if let Some(p) = &self.perspective {
            p.encode(out);
        }
    }
}

/// Local threshold field: level = window mean ∓ offset (window clipped at the border, summed with
/// an integral image). Returns the mean level (rounded) as the reported threshold.
fn adaptive_field(
    gray: &GrayImage,
    radius: u32,
    offset: u8,
    invert: bool,
    cancel: &CancellationToken,
) -> Result<(u8, Vec<f32>), ImageError> {
    if radius == 0 || radius > MAX_ADAPTIVE_RADIUS {
        return Err(ImageError::RadiusTooLarge { radius, max: MAX_ADAPTIVE_RADIUS });
    }
    let (w, h) = (gray.width as usize, gray.height as usize);
    let mut integral = vec![0.0f64; (w + 1) * (h + 1)];
    for y in 0..h {
        if y % 64 == 0 {
            cancel.check()?;
        }
        let mut row = 0.0f64;
        for x in 0..w {
            row += f64::from(gray.values[y * w + x]);
            integral[(y + 1) * (w + 1) + x + 1] = integral[y * (w + 1) + x + 1] + row;
        }
    }
    let r = radius as usize;
    let mut field = Vec::with_capacity(w * h);
    let mut level_sum = 0.0f64;
    for y in 0..h {
        if y % 64 == 0 {
            cancel.check()?;
        }
        let (y0, y1) = (y.saturating_sub(r), (y + r + 1).min(h));
        for x in 0..w {
            let (x0, x1) = (x.saturating_sub(r), (x + r + 1).min(w));
            let sum = integral[y1 * (w + 1) + x1] - integral[y0 * (w + 1) + x1] - integral[y1 * (w + 1) + x0]
                + integral[y0 * (w + 1) + x0];
            let mean = sum / ((x1 - x0) * (y1 - y0)) as f64;
            let g = f64::from(gray.values[y * w + x]);
            let off = f64::from(offset) + 0.5;
            let v = if invert { g - (mean + off) } else { (mean - off) - g };
            level_sum += mean;
            field.push(v as f32);
        }
    }
    let mean_level = (level_sum / (w * h).max(1) as f64).round().clamp(0.0, 255.0) as u8;
    Ok((mean_level, field))
}
