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
    AdvancedOptions, CutPriority, DitherMode, FillGrouping, GrayMethod, ImageRender, Lead, LeadStyle, LineOptions,
    MaterialRecipe, OffsetFillOrder, Operation, OperationMode, OrderConstraint, OrderKey, ProcessLayer,
    ProcessSettings, ScanAxis, ScanDirection, SharedEdgePolicy, TabPlacement,
};

impl CanonicalEncode for ProcessLayer {
    fn encode(&self, out: &mut CanonicalWriter) {
        out.seq(self.operations.iter().map(|o| o.as_ref()));
        out.seq(self.constraints.iter());
        out.seq(self.recipes.values().map(|r| r.as_ref()));
        self.encode_optimization(out);
        self.encode_path_locks(out);
        // Notes only when present, so projects without notes keep their hash.
        if !self.notes.is_empty() {
            out.tag("notes");
            out.str(&self.notes);
        }
    }
}

/// `tag("optimization")` after the recipes, only when the settings or any priority differ from
/// the defaults (so existing projects keep their hash): `seq` of order keys (tag each), five
/// `bool`s (inner-first, reduce-travel, best-start, best-direction, hide-backlash), optional
/// `f64` overlap tolerance (`u64` 0 | 1 + `f64`), `u64` priority count, each `u64` geometry +
/// `i64`-as-`u64` offset priority (`priority + CutPriority::MAX`), in geometry id order.
impl ProcessLayer {
    /// Writes the optimization block (nothing when settings and priorities are the defaults).
    pub fn encode_optimization(&self, out: &mut CanonicalWriter) {
        encode_optimization(self, out);
    }

    /// Writes the start/direction locks (C06): `tag("path-locks")`, `u64` count, each `u64`
    /// geometry, `bool` start, `bool` direction, in geometry id order. Nothing when no shape is
    /// locked, so projects without locks keep their hash.
    pub fn encode_path_locks(&self, out: &mut CanonicalWriter) {
        if self.path_locks.is_empty() {
            return;
        }
        out.tag("path-locks");
        out.u64(self.path_locks.len() as u64);
        for (g, lock) in &self.path_locks {
            out.u64(g.raw());
            out.bool(lock.start);
            out.bool(lock.direction);
        }
    }
}

fn encode_optimization(layer: &ProcessLayer, out: &mut CanonicalWriter) {
    let o = layer.optimization();
    if o.is_default() && layer.priorities.is_empty() {
        return;
    }
    out.tag("optimization");
    out.u64(o.order_by().len() as u64);
    for k in o.order_by() {
        out.tag(match k {
            OrderKey::Layer => "layer",
            OrderKey::Group => "group",
            OrderKey::Priority => "priority",
        });
    }
    for b in [o.inner_first(), o.reduce_travel(), o.best_start(), o.best_direction(), o.hide_backlash()] {
        out.bool(b);
    }
    opt_f64(out, o.remove_overlapping_mm());
    out.u64(layer.priorities.len() as u64);
    for (g, p) in &layer.priorities {
        out.u64(g.raw());
        out.u64((i64::from(p.get()) + i64::from(CutPriority::MAX)) as u64);
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
    if let Some(z) = a.z_offset {
        out.tag("advanced-z");
        out.f64(z.mm());
    }
    if let Some(tool) = a.tool {
        out.tag("advanced-tool");
        out.tag(tool.name());
    }
}

/// `tag("advanced-ext")` + tab spacing (optional `f64` spacing, then width) + lead in and lead
/// out (`u64` 0 | 1 + style tag + `f64` angle) + dot mode (`u64` 0 | 1 + `f64` spacing + `f64`
/// dwell) + optional `tag("tab-cut-power")` + `f64` percent; written only when a later option is set. Spacing-based tabs are written *only* here
/// (the earlier tab field then reads "no tabs"), so they never collide with counted tabs.
fn encode_line_extensions(line: &LineOptions, out: &mut CanonicalWriter) {
    let lead_is_plain = |l: Option<Lead>| l.is_none_or(|l| l.style() == LeadStyle::Line && l.angle_deg() == 90.0);
    let spaced = line.tabs.filter(|t| matches!(t.placement(), TabPlacement::Spacing(_)));
    let tab_power = line.tabs.and_then(|t| t.cut_power_percent());
    if spaced.is_none()
        && lead_is_plain(line.lead_in)
        && lead_is_plain(line.lead_out)
        && line.dot.is_none()
        && tab_power.is_none()
    {
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
    // Tab cut power (C04): written last and only when set, so every earlier project keeps its hash.
    if let Some(percent) = tab_power {
        out.tag("tab-cut-power");
        out.f64(percent);
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
                    DitherMode::Newsprint => out.tag("newsprint"),
                    DitherMode::Halftone => out.tag("halftone"),
                    DitherMode::Sketch { level } => {
                        out.tag("sketch");
                        out.u64(u64::from(level));
                    }
                    DitherMode::PassThrough => out.tag("pass-through"),
                },
            }
            let t = i.tone();
            out.f64(t.gamma());
            out.f64(t.brightness());
            out.f64(t.contrast());
            out.tag(direction_tag(i.direction()));
            out.bool(i.serpentine());
            out.f64(i.overscan_mm());
            // Options added after IM18 are appended only when set, so earlier hashes stay valid.
            if i.has_extended_options() {
                out.tag("image-options");
                out.bool(i.negative());
                out.f64(i.dot_width_correction_mm());
                out.f64(i.scan_angle_deg());
                out.tag(match i.gray_method() {
                    GrayMethod::Rec709 => "gray-709",
                    GrayMethod::Rec601 => "gray-601",
                    GrayMethod::Average => "gray-average",
                    GrayMethod::Lightness => "gray-lightness",
                    GrayMethod::Red => "gray-red",
                    GrayMethod::Green => "gray-green",
                    GrayMethod::Blue => "gray-blue",
                });
                match i.crop() {
                    None => out.bool(false),
                    Some(c) => {
                        out.bool(true);
                        for v in [c.x, c.y, c.width, c.height] {
                            out.u64(u64::from(v));
                        }
                    }
                }
                match i.sharpen() {
                    None => out.bool(false),
                    Some(sh) => {
                        out.bool(true);
                        out.f64(sh.amount());
                        out.f64(sh.radius_mm());
                    }
                }
                out.f64(i.screen().cell_mm());
                out.f64(i.screen().angle_deg());
                out.bool(i.mask());
            }
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
