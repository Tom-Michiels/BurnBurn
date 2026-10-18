//! Canonical projection of the process layer (ADR 0004), used by project and plan hashes.
//!
//! Layout: `seq` operations in list order (`u64` id, `str` name, mode, settings, `seq` of `u64`
//! targets, `bool` output, recipe ref as `u64` 0 | `u64` 1 + `u64` id + `u64` revision; with
//! non-default advanced options the recipe marker is 2 | 3 instead and the advanced options
//! follow, so operations without them keep their earlier hash; options added later (tab spacing,
//! lead style/angle, dot mode) follow the older ones in an `advanced-ext` block that exists only
//! when one of them is set, and extended fills use the tag `fill-ext`);
//! `seq` constraints (`u64` before, `u64` after) in stored order; `seq` recipes in id order.

use burnburn_types::{CanonicalEncode, CanonicalWriter, StableId};

use super::{
    AdvancedOptions, DitherMode, FillGrouping, ImageRender, Lead, LeadStyle, LineOptions, MaterialRecipe,
    OffsetFillOrder, Operation, OperationMode, OrderConstraint, ProcessLayer, ProcessSettings, ScanAxis, ScanDirection,
    SharedEdgePolicy, TabPlacement,
};

impl CanonicalEncode for ProcessLayer {
    fn encode(&self, out: &mut CanonicalWriter) {
        out.seq(self.operations.iter().map(|o| o.as_ref()));
        out.seq(self.constraints.iter());
        out.seq(self.recipes.values().map(|r| r.as_ref()));
    }
}

impl CanonicalEncode for Operation {
    fn encode(&self, out: &mut CanonicalWriter) {
        out.u64(self.id().raw());
        out.str(self.name());
        encode_mode(self.mode(), out);
        encode_settings(&self.settings(), out);
        out.u64(self.targets().len() as u64);
        for t in self.targets() {
            out.u64(t.raw());
        }
        out.bool(self.output_enabled());
        let extra = if self.advanced().is_default() { 0 } else { 2 };
        match self.recipe() {
            None => out.u64(extra),
            Some(r) => {
                out.u64(extra + 1);
                out.u64(r.recipe.raw());
                out.u64(u64::from(r.revision));
            }
        }
        if extra != 0 {
            encode_advanced(self.advanced(), out);
        }
    }
}

fn opt_f64(out: &mut CanonicalWriter, v: Option<f64>) {
    match v {
        None => out.u64(0),
        Some(v) => {
            out.u64(1);
            out.f64(v);
        }
    }
}

/// `tag("advanced")`; tabs (`u64` 0 | 1 + `u64` count + `f64` width); lead in, lead out,
/// overcut (each optional `f64`); perforation (`u64` 0 | 1 + `f64` on + `f64` off); z step
/// (optional `f64`); `u64` sub-operation count, each mode + settings; shared-edge tag.
fn encode_advanced(a: &AdvancedOptions, out: &mut CanonicalWriter) {
    out.tag("advanced");
    match a.line.tabs {
        Some(t) if matches!(t.placement(), TabPlacement::Count(_)) => {
            out.u64(1);
            out.u64(u64::from(t.count()));
            out.f64(t.width_mm());
        }
        _ => out.u64(0),
    }
    opt_f64(out, a.line.lead_in.map(|l| l.length_mm()));
    opt_f64(out, a.line.lead_out.map(|l| l.length_mm()));
    opt_f64(out, a.line.overcut.map(|o| o.length_mm()));
    match a.line.perforation {
        None => out.u64(0),
        Some(p) => {
            out.u64(1);
            out.f64(p.on_mm());
            out.f64(p.off_mm());
        }
    }
    opt_f64(out, a.z_step.map(|z| z.mm()));
    out.u64(a.sub_operations.len() as u64);
    for sub in &a.sub_operations {
        encode_mode(sub.mode, out);
        encode_settings(&sub.settings, out);
    }
    out.tag(match a.shared_edges {
        SharedEdgePolicy::CutOnce => "shared-cut-once",
        SharedEdgePolicy::CutPerRegion => "shared-cut-per-region",
    });
    encode_line_extensions(&a.line, out);
}

/// `tag("advanced-ext")` + tab spacing (optional `f64` spacing, then width) + lead in and lead
/// out (`u64` 0 | 1 + style tag + `f64` angle) + dot mode (`u64` 0 | 1 + `f64` spacing + `f64`
/// dwell); written only when a later option is set. Spacing-based tabs are written *only* here
/// (the earlier tab field then reads "no tabs"), so they never collide with counted tabs.
fn encode_line_extensions(line: &LineOptions, out: &mut CanonicalWriter) {
    let lead_is_plain = |l: Option<Lead>| l.is_none_or(|l| l.style() == LeadStyle::Line && l.angle_deg() == 90.0);
    let spaced = line.tabs.filter(|t| matches!(t.placement(), TabPlacement::Spacing(_)));
    if spaced.is_none() && lead_is_plain(line.lead_in) && lead_is_plain(line.lead_out) && line.dot.is_none() {
        return;
    }
    out.tag("advanced-ext");
    match spaced.map(|t| (t.placement(), t.width_mm())) {
        Some((TabPlacement::Spacing(spacing), width)) => {
            out.u64(1);
            out.f64(spacing);
            out.f64(width);
        }
        _ => out.u64(0),
    }
    for lead in [line.lead_in, line.lead_out] {
        match lead {
            None => out.u64(0),
            Some(l) => {
                out.u64(1);
                out.tag(match l.style() {
                    LeadStyle::Line => "line",
                    LeadStyle::Arc => "arc",
                });
                out.f64(l.angle_deg());
            }
        }
    }
    match line.dot {
        None => out.u64(0),
        Some(d) => {
            out.u64(1);
            out.f64(d.spacing_mm());
            out.f64(d.dwell_ms());
        }
    }
}

fn direction_tag(d: ScanDirection) -> &'static str {
    match d {
        ScanDirection::Bidirectional => "bidirectional",
        ScanDirection::Unidirectional => "unidirectional",
    }
}

impl CanonicalEncode for OrderConstraint {
    fn encode(&self, out: &mut CanonicalWriter) {
        out.u64(self.before.raw());
        out.u64(self.after.raw());
    }
}

impl CanonicalEncode for MaterialRecipe {
    fn encode(&self, out: &mut CanonicalWriter) {
        out.u64(self.id().raw());
        out.str(self.name());
        out.str(self.material());
        out.f64(self.thickness().mm());
        out.str(self.machine());
        out.u64(u64::from(self.revision()));
        out.str(self.notes());
        encode_mode(self.mode(), out);
        encode_settings(&self.settings(), out);
    }
}

pub(crate) fn encode_mode(mode: OperationMode, out: &mut CanonicalWriter) {
    match mode {
        OperationMode::Line { kerf } => {
            out.tag("line");
            out.f64(kerf.width_mm());
        }
        OperationMode::Fill(f) => {
            // A rotated fill has its own tag (angle last) so unrotated fills keep their hash;
            // a fill with extras has another (angle always, then the extras).
            out.tag(if f.has_extras() {
                "fill-ext"
            } else if f.angle_deg() == 0.0 {
                "fill"
            } else {
                "fill-angled"
            });
            out.f64(f.interval().mm());
            out.tag(match f.axis() {
                ScanAxis::X => "x",
                ScanAxis::Y => "y",
            });
            out.tag(direction_tag(f.direction()));
            out.f64(f.overscan().mm());
            if f.has_extras() {
                out.f64(f.angle_deg());
                out.bool(f.cross_hatch());
                out.tag(match f.grouping() {
                    FillGrouping::AllShapes => "all-shapes",
                    FillGrouping::Groups => "groups",
                    FillGrouping::Individual => "individual",
                });
                out.bool(f.flood_fill());
                out.f64(f.ramp_mm());
            } else if f.angle_deg() != 0.0 {
                out.f64(f.angle_deg());
            }
        }
        OperationMode::OffsetFill(o) => {
            out.tag("offset-fill");
            out.f64(o.step().mm());
            out.tag(match o.order() {
                OffsetFillOrder::OutsideIn => "outside-in",
                OffsetFillOrder::InsideOut => "inside-out",
            });
        }
        OperationMode::Image(i) => {
            out.tag("image");
            out.bytes(i.source().as_bytes());
            out.f64(i.dot_pitch().mm());
            out.f64(i.line_interval().mm());
            match i.render() {
                ImageRender::Grayscale => out.tag("grayscale"),
                ImageRender::Dither(d) => match d {
                    DitherMode::Threshold { level } => {
                        out.tag("threshold");
                        out.u64(u64::from(level));
                    }
                    DitherMode::FloydSteinberg => out.tag("floyd-steinberg"),
                    DitherMode::Jarvis => out.tag("jarvis"),
                    DitherMode::Stucki => out.tag("stucki"),
                    DitherMode::Atkinson => out.tag("atkinson"),
                    DitherMode::Ordered { size } => {
                        out.tag("ordered");
                        out.u64(u64::from(size));
                    }
                },
            }
            let t = i.tone();
            out.f64(t.gamma());
            out.f64(t.brightness());
            out.f64(t.contrast());
            out.tag(direction_tag(i.direction()));
            out.bool(i.serpentine());
            out.f64(i.overscan_mm());
        }
    }
}

pub(crate) fn encode_settings(s: &ProcessSettings, out: &mut CanonicalWriter) {
    out.f64(s.speed.get_mm_per_s());
    out.f64(s.power.min().get());
    out.f64(s.power.max().get());
    out.u64(u64::from(s.passes.get()));
    out.bool(s.air_assist);
}
