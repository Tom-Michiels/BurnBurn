//! Several cameras, one bed coordinate system (W2-L5e).
//!
//! Every camera has its own [`WorkspaceAlignment`] to the same bed millimetres. For a bed point,
//! [`cameras_seeing`] lists the cameras whose image contains it, best first. The rule for overlapping
//! regions is explicit and deterministic:
//!
//! 1. a camera sees the point when it projects at least `edge_margin_px` inside the image (the edges
//!    of a lens image are the least reliable part);
//! 2. among those, the camera whose *view centre* (principal point) is closest to the projected point
//!    wins, the distance normalised on the half image diagonal so that resolutions compare;
//! 3. distances closer than a nano-unit are a tie, which goes to the lower device id, so the result does
//!    not depend on the order of the input.
//!
//! A camera whose current configuration differs from the one its alignment was made for is never used:
//! it is listed in `skipped` with the reason. Pass one view per camera (the alignment for the current
//! material height).

use super::{CameraConfigKey, CameraDeviceId, Invalidity, WorkspaceAlignment};

/// One camera as it is configured right now, with the alignment to use for it.
#[derive(Debug, Clone)]
pub struct CameraView<'a> {
    pub current: &'a CameraConfigKey,
    pub alignment: &'a WorkspaceAlignment,
}

/// A camera that sees a bed point.
#[derive(Debug, Clone, PartialEq)]
pub struct Sighting {
    pub camera: CameraDeviceId,
    /// Where the point appears in that camera's image.
    pub pixel: [f64; 2],
    /// Distance of `pixel` to the view centre, normalised on the half diagonal (0 = centre).
    pub centre_distance: f64,
    /// Distance in pixels from `pixel` to the nearest image edge.
    pub edge_margin_px: f64,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct BedPointCoverage {
    /// Cameras that see the point, best first (see the module rule).
    pub sightings: Vec<Sighting>,
    /// Cameras left out because their configuration no longer matches their alignment.
    pub skipped: Vec<(CameraDeviceId, Invalidity)>,
}

impl BedPointCoverage {
    /// The camera to take the pixels from for this point, if any camera sees it.
    pub fn chosen(&self) -> Option<&Sighting> {
        self.sightings.first()
    }
}

/// Which cameras see `bed` (mm), and which one to use. `edge_margin_px` is clamped at 0.
pub fn cameras_seeing(bed: [f64; 2], views: &[CameraView<'_>], edge_margin_px: f64) -> BedPointCoverage {
    let margin = if edge_margin_px.is_finite() { edge_margin_px.max(0.0) } else { f64::INFINITY };
    let mut out = BedPointCoverage::default();
    for view in views {
        let cal = &view.alignment.calibration;
        let pixel = match cal.bed_to_pixel(view.current, bed) {
            Err(why) => {
                out.skipped.push((view.current.device.clone(), why));
                continue;
            }
            Ok(None) => continue,
            Ok(Some(p)) if p.iter().all(|v| v.is_finite()) => p,
            Ok(Some(_)) => continue,
        };
        let (w, h) = (f64::from(view.current.mode.width), f64::from(view.current.mode.height));
        let edge = pixel[0].min(w - pixel[0]).min(pixel[1]).min(h - pixel[1]);
        if edge < margin || edge < 0.0 {
            continue;
        }
        let c = cal.intrinsics.center_px;
        let centre_distance = (pixel[0] - c[0]).hypot(pixel[1] - c[1]) / cal.intrinsics.radius_px;
        out.sightings.push(Sighting {
            camera: view.current.device.clone(),
            pixel,
            centre_distance,
            edge_margin_px: edge,
        });
    }
    let tie_key = |s: &Sighting| (s.centre_distance * 1e9).round() as i64;
    out.sightings.sort_by(|a, b| tie_key(a).cmp(&tie_key(b)).then_with(|| a.camera.cmp(&b.camera)));
    out.skipped.sort_by(|a, b| a.0.cmp(&b.0));
    out
}
