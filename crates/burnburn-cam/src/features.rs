//! Which capability features an operation needs (IM25 feature matrix).

use burnburn_document::process::{FillGrouping, ImageRender, Operation, OperationMode, ScanAxis};
use burnburn_job::{Feature, MachineProfile};

use crate::CompileError;

/// Effective scan angle in degrees `[0, 180)` measured from bed X (axis Y adds 90°).
pub(crate) fn effective_angle(f: &burnburn_document::process::FillSettings) -> f64 {
    let base = match f.axis() {
        ScanAxis::X => 0.0,
        ScanAxis::Y => 90.0,
    };
    (base + f.angle_deg()).rem_euclid(180.0)
}

fn mode_features(mode: OperationMode, out: &mut Vec<Feature>) {
    match mode {
        OperationMode::Line { kerf } if !kerf.is_none() => out.push(Feature::KerfCompensation),
        OperationMode::Fill(f) => {
            let a = effective_angle(&f);
            if a != 0.0 && a != 90.0 {
                out.push(Feature::ScanAngle);
            }
            if f.cross_hatch() {
                out.push(Feature::CrossHatch);
            }
            if f.flood_fill() {
                out.push(Feature::FloodFill);
            }
            if f.grouping() != FillGrouping::AllShapes {
                out.push(Feature::FillGrouping);
            }
            if f.ramp_mm() > 0.0 {
                out.push(Feature::RampLength);
            }
        }
        OperationMode::OffsetFill(_) => out.push(Feature::OffsetFill),
        OperationMode::Image(i) => {
            out.push(match i.render() {
                ImageRender::Dither(_) => Feature::ImageDither,
                ImageRender::Grayscale => Feature::GrayscaleModulation,
            });
            let a = i.scan_angle_deg();
            if a != 0.0 && a != 90.0 {
                out.push(Feature::ScanAngle);
            }
        }
        _ => {}
    }
}

/// Every feature `op` uses, in a fixed order (mode, line options, z step, sub-operations).
pub fn required_features(op: &Operation) -> Vec<Feature> {
    let mut out = Vec::new();
    mode_features(op.mode(), &mut out);
    let a = op.advanced();
    let l = &a.line;
    if l.tabs.is_some() {
        out.push(Feature::Tabs);
    }
    if l.lead_in.is_some() || l.lead_out.is_some() {
        out.push(Feature::Leads);
    }
    if l.overcut.is_some() {
        out.push(Feature::Overcut);
    }
    if l.perforation.is_some() {
        out.push(Feature::Perforation);
    }
    if l.dot.is_some() {
        out.push(Feature::DotMode);
    }
    if a.z_step.is_some() || a.z_offset.is_some() {
        out.push(Feature::ZStep);
    }
    if !a.sub_operations.is_empty() {
        out.push(Feature::SubOperations);
        for s in &a.sub_operations {
            mode_features(s.mode, &mut out);
        }
    }
    let mut unique = Vec::with_capacity(out.len());
    for f in out {
        if !unique.contains(&f) {
            unique.push(f);
        }
    }
    unique
}

/// The first feature of `op` the profile does not support, as a typed error. The compiler
/// never replaces such a feature by another one.
pub(crate) fn check(op: &Operation, profile: &MachineProfile) -> Result<(), CompileError> {
    let has_line_options = op.advanced().line != Default::default();
    if has_line_options && !matches!(op.mode(), OperationMode::Line { .. }) {
        return Err(CompileError::LineOptionsOnNonLineMode { operation: op.id() });
    }
    for feature in required_features(op) {
        let support = profile.capabilities().support(feature);
        if support != burnburn_job::Support::Supported {
            return Err(CompileError::UnsupportedFeature { operation: op.id(), feature, support });
        }
    }
    Ok(())
}
