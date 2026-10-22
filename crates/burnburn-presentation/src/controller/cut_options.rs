//! Cut Settings option fields (W2-L1-cut): what the sheet shows for an operation, what the
//! machine profile blocks, and how raw field text becomes domain options.
//!
//! A field is applied only when its text differs from what the sheet displayed, so an unchanged
//! value is never a command (and never rounded through the three-decimal display text), and a
//! stored option the profile lacks stays untouched.

use burnburn_document::process::{
    AdvancedOptions, CommonLine, DitherMode, DotMode, FillGrouping, FillSettings, ImageRender, ImageSettings, Lead,
    LeadStyle, LineOptions, Operation, Overcut, Perforation, ScanDirection, ScreenSettings, TabPlacement, TabSpec,
    ZOffset, ZStep,
};
use burnburn_job::{Feature, MachineCapabilities, Support};
use burnburn_types::Length;

use crate::cut_options::{CutChoice, CutExtras, CutField, CutGates};
use crate::field::{format_mm, parse_angle, parse_length, parse_number, parse_position};
use crate::intent::OperationKind;
use crate::view_model::MessageKey;

const TABS: [CutField; 5] =
    [CutField::TabsEnabled, CutField::TabsMode, CutField::TabsValue, CutField::TabsWidth, CutField::TabsPower];
const LEAD_IN: [CutField; 4] =
    [CutField::LeadInEnabled, CutField::LeadInLength, CutField::LeadInShape, CutField::LeadInAngle];
const LEAD_OUT: [CutField; 4] =
    [CutField::LeadOutEnabled, CutField::LeadOutLength, CutField::LeadOutShape, CutField::LeadOutAngle];
const PERFORATION: [CutField; 3] = [CutField::PerforationEnabled, CutField::PerforationOn, CutField::PerforationOff];
const DOT: [CutField; 3] = [CutField::DotEnabled, CutField::DotSpacing, CutField::DotDwell];
const COMMON_LINE: [CutField; 3] =
    [CutField::CommonLineEnabled, CutField::CommonLineTolerance, CutField::CommonLineMinOverlap];
const SCREEN: [CutField; 2] = [CutField::ScreenCell, CutField::ScreenAngle];

// Defaults shown for an option that is off (LightBurn-like starting values).
const DEFAULT_TAB_COUNT: &str = "4";
const DEFAULT_TAB_WIDTH_MM: &str = "0.5";
/// No power: the tabs are skipped.
const DEFAULT_TAB_POWER: &str = "0";
const DEFAULT_LEAD_MM: &str = "2";
const DEFAULT_LEAD_ANGLE_DEG: &str = "90";
const DEFAULT_PERFORATION_MM: &str = "1";
const DEFAULT_DOT_SPACING_MM: &str = "1";
const DEFAULT_DOT_DWELL_MS: &str = "10";
const DEFAULT_COMMON_TOLERANCE_MM: &str = "0.05";
const DEFAULT_COMMON_OVERLAP_MM: &str = "1";
const DEFAULT_LEVEL: &str = "128";
const DEFAULT_MATRIX: &str = "4";

fn lead_fields(e: &mut CutExtras, lead: Option<Lead>, f: [CutField; 4]) {
    let [enabled, length, shape, angle] = f;
    e.set_flag(enabled, lead.is_some());
    e.set_text(length, lead.map_or(DEFAULT_LEAD_MM.to_owned(), |l| format_mm(l.length_mm())));
    let style = match lead.map(|l| l.style()) {
        Some(LeadStyle::Arc) => CutChoice::LeadArc,
        _ => CutChoice::LeadLine,
    };
    e.set_choice(shape, style);
    e.set_text(angle, lead.map_or(DEFAULT_LEAD_ANGLE_DEG.to_owned(), |l| format_mm(l.angle_deg())));
}

fn common_line_extras(c: Option<CommonLine>, e: &mut CutExtras) {
    e.set_flag(CutField::CommonLineEnabled, c.is_some());
    e.set_text(
        CutField::CommonLineTolerance,
        c.map_or(DEFAULT_COMMON_TOLERANCE_MM.to_owned(), |c| format_mm(c.tolerance_mm())),
    );
    e.set_text(
        CutField::CommonLineMinOverlap,
        c.map_or(DEFAULT_COMMON_OVERLAP_MM.to_owned(), |c| format_mm(c.min_overlap_mm())),
    );
}

fn line_extras(l: &LineOptions, e: &mut CutExtras) {
    e.set_flag(CutField::TabsEnabled, l.tabs.is_some());
    e.set_text(
        CutField::TabsPower,
        l.tabs.and_then(|t| t.cut_power_percent()).map_or(DEFAULT_TAB_POWER.to_owned(), format_mm),
    );
    match l.tabs.map(|t| (t.placement(), t.width_mm())) {
        Some((TabPlacement::Count(n), w)) => {
            e.set_choice(CutField::TabsMode, CutChoice::TabsByCount);
            e.set_text(CutField::TabsValue, n.to_string());
            e.set_text(CutField::TabsWidth, format_mm(w));
        }
        Some((TabPlacement::Spacing(s), w)) => {
            e.set_choice(CutField::TabsMode, CutChoice::TabsBySpacing);
            e.set_text(CutField::TabsValue, format_mm(s));
            e.set_text(CutField::TabsWidth, format_mm(w));
        }
        None => {
            e.set_choice(CutField::TabsMode, CutChoice::TabsByCount);
            e.set_text(CutField::TabsValue, DEFAULT_TAB_COUNT);
            e.set_text(CutField::TabsWidth, DEFAULT_TAB_WIDTH_MM);
        }
    }
    lead_fields(e, l.lead_in, LEAD_IN);
    lead_fields(e, l.lead_out, LEAD_OUT);
    e.set_text(CutField::Overcut, format_mm(l.overcut.map_or(0.0, |o| o.length_mm())));
    e.set_flag(CutField::PerforationEnabled, l.perforation.is_some());
    e.set_text(
        CutField::PerforationOn,
        l.perforation.map_or(DEFAULT_PERFORATION_MM.to_owned(), |p| format_mm(p.on_mm())),
    );
    e.set_text(
        CutField::PerforationOff,
        l.perforation.map_or(DEFAULT_PERFORATION_MM.to_owned(), |p| format_mm(p.off_mm())),
    );
    e.set_flag(CutField::DotEnabled, l.dot.is_some());
    e.set_text(CutField::DotSpacing, l.dot.map_or(DEFAULT_DOT_SPACING_MM.to_owned(), |d| format_mm(d.spacing_mm())));
    e.set_text(CutField::DotDwell, l.dot.map_or(DEFAULT_DOT_DWELL_MS.to_owned(), |d| format_mm(d.dwell_ms())));
}

fn grouping_choice(g: FillGrouping) -> CutChoice {
    match g {
        FillGrouping::AllShapes => CutChoice::GroupAllShapes,
        FillGrouping::Groups => CutChoice::GroupGroups,
        FillGrouping::Individual => CutChoice::GroupIndividual,
    }
}

fn fill_extras(f: &FillSettings, e: &mut CutExtras) {
    e.set_text(CutField::FillInterval, format_mm(f.interval().mm()));
    e.set_text(CutField::FillAngle, format_mm(f.angle_deg()));
    e.set_flag(CutField::FillBidirectional, f.direction() == ScanDirection::Bidirectional);
    e.set_text(CutField::FillOverscan, format_mm(f.overscan().mm()));
    e.set_flag(CutField::FillCrossHatch, f.cross_hatch());
    e.set_flag(CutField::FillFloodFill, f.flood_fill());
    e.set_choice(CutField::FillGrouping, grouping_choice(f.grouping()));
    e.set_text(CutField::FillRamp, format_mm(f.ramp_mm()));
}

fn render_choice(r: ImageRender) -> CutChoice {
    match r {
        ImageRender::Grayscale => CutChoice::Grayscale,
        ImageRender::Dither(d) => match d {
            DitherMode::Threshold { .. } => CutChoice::Threshold,
            DitherMode::Ordered { .. } => CutChoice::Ordered,
            DitherMode::Atkinson => CutChoice::Atkinson,
            DitherMode::FloydSteinberg => CutChoice::Dither,
            DitherMode::Stucki => CutChoice::Stucki,
            DitherMode::Jarvis => CutChoice::Jarvis,
            DitherMode::Newsprint => CutChoice::Newsprint,
            DitherMode::Halftone => CutChoice::Halftone,
            DitherMode::Sketch { .. } => CutChoice::Sketch,
            _ => CutChoice::PassThrough,
        },
    }
}

fn image_extras(i: &ImageSettings, e: &mut CutExtras) {
    e.set_choice(CutField::ImageMode, render_choice(i.render()));
    let (level, matrix) = match i.render() {
        ImageRender::Dither(DitherMode::Threshold { level } | DitherMode::Sketch { level }) => {
            (level.to_string(), DEFAULT_MATRIX.to_owned())
        }
        ImageRender::Dither(DitherMode::Ordered { size }) => (DEFAULT_LEVEL.to_owned(), size.to_string()),
        _ => (DEFAULT_LEVEL.to_owned(), DEFAULT_MATRIX.to_owned()),
    };
    e.set_text(CutField::ImageLevel, level);
    e.set_text(CutField::ImageMatrix, matrix);
    e.set_flag(CutField::ImageNegative, i.negative());
    e.set_text(CutField::ImageDotWidth, format_mm(i.dot_width_correction_mm()));
    e.set_text(CutField::ImageAngle, format_mm(i.scan_angle_deg()));
    e.set_text(CutField::ImageInterval, format_mm(i.line_interval().mm()));
    e.set_text(CutField::ImageDotPitch, format_mm(i.dot_pitch().mm()));
    e.set_flag(CutField::ImageBidirectional, i.direction() == ScanDirection::Bidirectional);
    e.set_text(CutField::ImageOverscan, format_mm(i.overscan_mm()));
    e.set_text(CutField::ScreenCell, format_mm(i.screen().cell_mm()));
    e.set_text(CutField::ScreenAngle, format_mm(i.screen().angle_deg()));
}

/// The fields of the sheet with their stored values as it displays them. An image has its own
/// fields; every other operation lists the Line and the Fill fields (the ones of the modes it
/// does not have show LightBurn-like defaults), so choosing another mode in the sheet has values
/// to show.
pub(super) fn extras_of(op: &Operation) -> CutExtras {
    use burnburn_document::process::OperationMode as M;
    let mut e = CutExtras::new();
    e.set_text(CutField::ZOffset, format_mm(op.advanced().z_offset.map_or(0.0, |z| z.mm())));
    e.set_text(CutField::ZStep, format_mm(op.advanced().z_step.map_or(0.0, |z| z.mm())));
    if let M::Image(i) = op.mode() {
        image_extras(&i, &mut e);
        return e;
    }
    let line = if matches!(op.mode(), M::Line { .. }) { op.advanced().line } else { LineOptions::default() };
    line_extras(&line, &mut e);
    common_line_extras(if matches!(op.mode(), M::Line { .. }) { op.advanced().common_line } else { None }, &mut e);
    match (op.mode(), super::layers::default_fill()) {
        (M::Fill(f), _) | (_, Some(f)) => fill_extras(&f, &mut e),
        _ => {}
    }
    e
}

fn missing(feature: Feature, caps: &MachineCapabilities) -> Option<MessageKey> {
    (caps.support(feature) != Support::Supported).then_some(MessageKey::MissingCapability(feature))
}

/// What `caps` does not allow, for the fields of an image operation or of any other one.
pub(super) fn gates_of(kind: OperationKind, caps: &MachineCapabilities) -> CutGates {
    let mut g = CutGates::default();
    let mut block = |fields: &[CutField], feature: Feature| {
        if let Some(reason) = missing(feature, caps) {
            for f in fields {
                g.blocked.insert(*f, reason);
            }
        }
    };
    block(&[CutField::ZOffset, CutField::ZStep], Feature::ZStep);
    match kind {
        OperationKind::Image => {
            for choice in CutField::ImageMode.choices() {
                let feature = match choice {
                    CutChoice::Grayscale => Feature::GrayscaleModulation,
                    _ => Feature::ImageDither,
                };
                if let Some(reason) = missing(feature, caps) {
                    g.blocked_choices.insert((CutField::ImageMode, *choice), reason);
                }
            }
            if let Some(reason) = missing(Feature::ScanAngle, caps) {
                g.advisory.insert(CutField::ImageAngle, reason);
            }
        }
        _ => {
            // The power first: when the tabs themselves are missing too, their reason wins.
            block(&[CutField::TabsPower], Feature::TabCutPower);
            block(&TABS, Feature::Tabs);
            block(&[LEAD_IN, LEAD_OUT].concat(), Feature::Leads);
            block(&[CutField::Overcut], Feature::Overcut);
            block(&PERFORATION, Feature::Perforation);
            block(&DOT, Feature::DotMode);
            block(&COMMON_LINE, Feature::CommonLine);
            block(&[CutField::FillCrossHatch], Feature::CrossHatch);
            block(&[CutField::FillFloodFill], Feature::FloodFill);
            block(&[CutField::FillGrouping], Feature::FillGrouping);
            block(&[CutField::FillRamp], Feature::RampLength);
            if let Some(reason) = missing(Feature::ScanAngle, caps) {
                g.advisory.insert(CutField::FillAngle, reason);
            }
        }
    }
    g
}

fn invalid(field: CutField) -> MessageKey {
    MessageKey::InvalidCutField(field)
}

fn length(text: &str, field: CutField) -> Result<Length, MessageKey> {
    parse_length(text).map_err(|_| invalid(field))
}

/// A length of zero or more (zero allowed).
fn length_or_zero(text: &str, field: CutField) -> Result<Length, MessageKey> {
    parse_position(text).ok().filter(|l| l.mm() >= 0.0).ok_or(invalid(field))
}

fn whole(text: &str, field: CutField) -> Result<u32, MessageKey> {
    let v = parse_number(text, &[]).map_err(|_| invalid(field))?;
    if v.fract() != 0.0 || !(0.0..=f64::from(u32::MAX)).contains(&v) {
        return Err(invalid(field));
    }
    Ok(v as u32)
}

/// `0,1 mm` or `254 dpi` (an interval or pitch).
fn resolution(text: &str, field: CutField) -> Result<Length, MessageKey> {
    let lower = text.trim().to_ascii_lowercase();
    match lower.strip_suffix("dpi") {
        Some(n) => {
            let dpi = parse_number(n, &[]).map_err(|_| invalid(field))?;
            if dpi > 0.0 { Length::from_mm(25.4 / dpi).map_err(|_| invalid(field)) } else { Err(invalid(field)) }
        }
        None => length(text, field),
    }
}

fn tabs(m: &CutExtras) -> Result<TabSpec, MessageKey> {
    let width = length(m.text(CutField::TabsWidth), CutField::TabsWidth)?;
    let value = m.text(CutField::TabsValue);
    let spec = match m.choice(CutField::TabsMode) {
        Some(CutChoice::TabsBySpacing) => TabSpec::spaced(length(value, CutField::TabsValue)?, width),
        _ => TabSpec::new(whole(value, CutField::TabsValue)?, width),
    };
    let spec = spec.map_err(|_| invalid(CutField::TabsValue))?;
    // `0` or an empty field: the tabs are skipped (C04); otherwise a percentage of the power.
    let text = m.text(CutField::TabsPower);
    if text.trim().is_empty() {
        return Ok(spec);
    }
    let percent = parse_number(text, &["%"]).map_err(|_| invalid(CutField::TabsPower))?;
    if percent == 0.0 {
        return Ok(spec);
    }
    spec.with_cut_power(percent).map_err(|_| invalid(CutField::TabsPower))
}

fn lead(m: &CutExtras, f: [CutField; 4]) -> Result<Option<Lead>, MessageKey> {
    let [enabled, len, shape, angle] = f;
    if !m.flag(enabled) {
        return Ok(None);
    }
    let lead = Lead::new(length(m.text(len), len)?).map_err(|_| invalid(len))?;
    if m.choice(shape) == Some(CutChoice::LeadArc) {
        return Ok(Some(lead.with_style(LeadStyle::Arc)));
    }
    let degrees = parse_angle(m.text(angle)).map_err(|_| invalid(angle))?;
    Ok(Some(lead.with_angle(degrees).map_err(|_| invalid(angle))?))
}

fn line_options(
    stored: LineOptions,
    m: &CutExtras,
    changed: &dyn Fn(&[CutField]) -> bool,
) -> Result<LineOptions, MessageKey> {
    let mut out = stored;
    if changed(&TABS) {
        out.tabs = if m.flag(CutField::TabsEnabled) { Some(tabs(m)?) } else { None };
    }
    if changed(&LEAD_IN) {
        out.lead_in = lead(m, LEAD_IN)?;
    }
    if changed(&LEAD_OUT) {
        out.lead_out = lead(m, LEAD_OUT)?;
    }
    if changed(&[CutField::Overcut]) {
        let text = m.text(CutField::Overcut);
        let mm = if text.trim().is_empty() { 0.0 } else { length_or_zero(text, CutField::Overcut)?.mm() };
        out.overcut = if mm == 0.0 {
            None
        } else {
            let l = Length::from_mm(mm).map_err(|_| invalid(CutField::Overcut))?;
            Some(Overcut::new(l).map_err(|_| invalid(CutField::Overcut))?)
        };
    }
    if changed(&PERFORATION) {
        out.perforation = if m.flag(CutField::PerforationEnabled) {
            let on = length(m.text(CutField::PerforationOn), CutField::PerforationOn)?;
            let off = length(m.text(CutField::PerforationOff), CutField::PerforationOff)?;
            Some(Perforation::new(on, off).map_err(|_| invalid(CutField::PerforationOn))?)
        } else {
            None
        };
    }
    if changed(&DOT) {
        out.dot = if m.flag(CutField::DotEnabled) {
            let spacing = length(m.text(CutField::DotSpacing), CutField::DotSpacing)?;
            let dwell = parse_number(m.text(CutField::DotDwell), &["ms"]).map_err(|_| invalid(CutField::DotDwell))?;
            Some(DotMode::new(spacing, dwell).map_err(|_| invalid(CutField::DotDwell))?)
        } else {
            None
        };
    }
    let others = out.tabs.is_some()
        || out.lead_in.is_some()
        || out.lead_out.is_some()
        || out.overcut.is_some()
        || out.perforation.is_some();
    if out.dot.is_some() && others {
        return Err(MessageKey::CutOptionsConflict);
    }
    Ok(out)
}

/// Common-line cutting from its three fields: off, or the tolerance and the minimal overlap.
fn common_line(m: &CutExtras) -> Result<Option<CommonLine>, MessageKey> {
    if !m.flag(CutField::CommonLineEnabled) {
        return Ok(None);
    }
    let tolerance = length(m.text(CutField::CommonLineTolerance), CutField::CommonLineTolerance)?;
    let overlap_text = m.text(CutField::CommonLineMinOverlap);
    let overlap = if overlap_text.trim().is_empty() {
        length_or_zero("0", CutField::CommonLineMinOverlap)?
    } else {
        parse_position(overlap_text).map_err(|_| invalid(CutField::CommonLineMinOverlap))?
    };
    CommonLine::new(tolerance, overlap).map(Some).map_err(|e| match e {
        burnburn_document::process::ProcessError::OutOfRange { field: "common-line overlap" } => {
            invalid(CutField::CommonLineMinOverlap)
        }
        _ => invalid(CutField::CommonLineTolerance),
    })
}

fn grouping(c: Option<CutChoice>, stored: FillGrouping) -> FillGrouping {
    match c {
        Some(CutChoice::GroupGroups) => FillGrouping::Groups,
        Some(CutChoice::GroupIndividual) => FillGrouping::Individual,
        Some(CutChoice::GroupAllShapes) => FillGrouping::AllShapes,
        _ => stored,
    }
}

fn fill(
    stored: FillSettings,
    m: &CutExtras,
    changed: &dyn Fn(&[CutField]) -> bool,
) -> Result<FillSettings, MessageKey> {
    let pick = |f: CutField| changed(&[f]);
    let interval = if pick(CutField::FillInterval) {
        length(m.text(CutField::FillInterval), CutField::FillInterval)?
    } else {
        stored.interval()
    };
    let angle = if pick(CutField::FillAngle) {
        parse_angle(m.text(CutField::FillAngle)).map_err(|_| invalid(CutField::FillAngle))?
    } else {
        stored.angle_deg()
    };
    let direction = if !pick(CutField::FillBidirectional) {
        stored.direction()
    } else if m.flag(CutField::FillBidirectional) {
        ScanDirection::Bidirectional
    } else {
        ScanDirection::Unidirectional
    };
    let overscan = if pick(CutField::FillOverscan) {
        length_or_zero(m.text(CutField::FillOverscan), CutField::FillOverscan)?
    } else {
        stored.overscan()
    };
    let ramp = if pick(CutField::FillRamp) {
        length_or_zero(m.text(CutField::FillRamp), CutField::FillRamp)?
    } else {
        Length::from_mm(stored.ramp_mm()).map_err(|_| invalid(CutField::FillRamp))?
    };
    let cross = if pick(CutField::FillCrossHatch) { m.flag(CutField::FillCrossHatch) } else { stored.cross_hatch() };
    let flood = if pick(CutField::FillFloodFill) { m.flag(CutField::FillFloodFill) } else { stored.flood_fill() };
    let group = if pick(CutField::FillGrouping) {
        grouping(m.choice(CutField::FillGrouping), stored.grouping())
    } else {
        stored.grouping()
    };
    FillSettings::new(interval, stored.axis(), direction, overscan)
        .and_then(|f| f.with_angle(angle))
        .and_then(|f| f.with_ramp(ramp))
        .map(|f| f.with_cross_hatch(cross).with_flood_fill(flood).with_grouping(group))
        .map_err(|_| invalid(CutField::FillInterval))
}

fn render(m: &CutExtras, stored: ImageRender) -> Result<ImageRender, MessageKey> {
    let level = |field| {
        let v = whole(m.text(field), field)?;
        u8::try_from(v).map_err(|_| invalid(field))
    };
    let Some(choice) = m.choice(CutField::ImageMode) else { return Ok(stored) };
    Ok(match choice {
        CutChoice::Threshold => ImageRender::Dither(DitherMode::Threshold { level: level(CutField::ImageLevel)? }),
        CutChoice::Sketch => ImageRender::Dither(DitherMode::Sketch { level: level(CutField::ImageLevel)? }),
        CutChoice::Ordered => {
            let size = level(CutField::ImageMatrix)?;
            if ![2, 4, 8].contains(&size) {
                return Err(invalid(CutField::ImageMatrix));
            }
            ImageRender::Dither(DitherMode::Ordered { size })
        }
        CutChoice::Atkinson => ImageRender::Dither(DitherMode::Atkinson),
        CutChoice::Dither => ImageRender::Dither(DitherMode::FloydSteinberg),
        CutChoice::Stucki => ImageRender::Dither(DitherMode::Stucki),
        CutChoice::Jarvis => ImageRender::Dither(DitherMode::Jarvis),
        CutChoice::Newsprint => ImageRender::Dither(DitherMode::Newsprint),
        CutChoice::Halftone => ImageRender::Dither(DitherMode::Halftone),
        CutChoice::PassThrough => ImageRender::Dither(DitherMode::PassThrough),
        CutChoice::Grayscale => ImageRender::Grayscale,
        _ => stored,
    })
}

fn image(
    stored: ImageSettings,
    m: &CutExtras,
    changed: &dyn Fn(&[CutField]) -> bool,
) -> Result<ImageSettings, MessageKey> {
    let pick = |f: CutField| changed(&[f]);
    let r = if changed(&[CutField::ImageMode, CutField::ImageLevel, CutField::ImageMatrix]) {
        render(m, stored.render())?
    } else {
        stored.render()
    };
    let dot_width = if pick(CutField::ImageDotWidth) {
        length_or_zero(m.text(CutField::ImageDotWidth), CutField::ImageDotWidth)?
    } else {
        Length::from_mm(stored.dot_width_correction_mm()).map_err(|_| invalid(CutField::ImageDotWidth))?
    };
    let angle = if pick(CutField::ImageAngle) {
        parse_angle(m.text(CutField::ImageAngle)).map_err(|_| invalid(CutField::ImageAngle))?
    } else {
        stored.scan_angle_deg()
    };
    let interval = if pick(CutField::ImageInterval) {
        resolution(m.text(CutField::ImageInterval), CutField::ImageInterval)?
    } else {
        stored.line_interval()
    };
    let pitch = if pick(CutField::ImageDotPitch) {
        resolution(m.text(CutField::ImageDotPitch), CutField::ImageDotPitch)?
    } else {
        stored.dot_pitch()
    };
    let overscan = if pick(CutField::ImageOverscan) {
        length_or_zero(m.text(CutField::ImageOverscan), CutField::ImageOverscan)?
    } else {
        Length::from_mm(stored.overscan_mm()).map_err(|_| invalid(CutField::ImageOverscan))?
    };
    let direction = if !pick(CutField::ImageBidirectional) {
        stored.direction()
    } else if m.flag(CutField::ImageBidirectional) {
        ScanDirection::Bidirectional
    } else {
        ScanDirection::Unidirectional
    };
    let negative = if pick(CutField::ImageNegative) { m.flag(CutField::ImageNegative) } else { stored.negative() };
    let screen = if changed(&SCREEN) {
        let cell = if pick(CutField::ScreenCell) {
            length(m.text(CutField::ScreenCell), CutField::ScreenCell)?
        } else {
            Length::from_mm(stored.screen().cell_mm()).map_err(|_| invalid(CutField::ScreenCell))?
        };
        let degrees = if pick(CutField::ScreenAngle) {
            parse_angle(m.text(CutField::ScreenAngle)).map_err(|_| invalid(CutField::ScreenAngle))?
        } else {
            stored.screen().angle_deg()
        };
        ScreenSettings::new(cell, degrees).map_err(|_| invalid(CutField::ScreenCell))?
    } else {
        stored.screen()
    };
    // Pass-through and grayscale scan along the axes only; an angle is refused, not ignored.
    let axis_only = matches!(r, ImageRender::Grayscale | ImageRender::Dither(DitherMode::PassThrough));
    if axis_only && angle.rem_euclid(180.0) != 0.0 {
        return Err(MessageKey::CutOptionNotForMode(CutField::ImageAngle));
    }
    let out = ImageSettings::new(stored.source(), pitch, interval, r)
        .map_err(|_| invalid(CutField::ImageInterval))?
        .with_tone(stored.tone())
        .with_direction(direction, stored.serpentine())
        .with_overscan(overscan)
        .map_err(|_| invalid(CutField::ImageOverscan))?
        .with_negative(negative)
        .with_dot_width_correction(dot_width)
        .map_err(|_| invalid(CutField::ImageDotWidth))?
        .with_scan_angle(angle)
        .map_err(|_| invalid(CutField::ImageAngle))?
        .with_gray_method(stored.gray_method())
        .with_crop(stored.crop())
        .map_err(|_| invalid(CutField::ImageInterval))?
        .with_sharpen(stored.sharpen())
        .with_screen(screen)
        .with_mask(stored.mask());
    Ok(out)
}

/// The mode settings and advanced options that result from the sheet's option fields. `mode` is
/// the mode the sheet chose with its own settings kept where they fit; fields whose value equals
/// what the sheet displayed leave the stored setting untouched.
pub(super) fn apply(
    op: &Operation,
    mode: burnburn_document::process::OperationMode,
    input: &CutExtras,
) -> Result<(burnburn_document::process::OperationMode, AdvancedOptions), MessageKey> {
    use burnburn_document::process::OperationMode as M;
    let current = extras_of(op);
    let merged = current.clone().merged(input);
    let is_changed = |f: CutField| input.get(f).is_some_and(|v| current.get(f) != Some(v));
    let changed = |fields: &[CutField]| fields.iter().any(|f| is_changed(*f));
    let mut advanced = op.advanced().clone();
    let mode = match mode {
        M::Line { kerf } => {
            advanced.line = line_options(op.advanced().line, &merged, &changed)?;
            if changed(&COMMON_LINE) {
                advanced.common_line = common_line(&merged)?;
            }
            M::Line { kerf }
        }
        M::Fill(f) => {
            advanced.line = LineOptions::default();
            advanced.common_line = None;
            M::Fill(fill(f, &merged, &changed)?)
        }
        M::Image(i) => {
            advanced.line = LineOptions::default();
            advanced.common_line = None;
            M::Image(image(i, &merged, &changed)?)
        }
        other => {
            advanced.line = LineOptions::default();
            advanced.common_line = None;
            other
        }
    };
    if changed(&[CutField::ZOffset]) {
        advanced.z_offset = z_offset(merged.text(CutField::ZOffset))?;
    }
    if changed(&[CutField::ZStep]) {
        advanced.z_step = z_step(merged.text(CutField::ZStep))?;
    }
    Ok((mode, advanced))
}

/// `0` or an empty field is no step; otherwise a signed length of at most 50 mm.
fn z_step(text: &str) -> Result<Option<ZStep>, MessageKey> {
    if text.trim().is_empty() {
        return Ok(None);
    }
    let length = parse_position(text).map_err(|_| invalid(CutField::ZStep))?;
    if length.mm() == 0.0 {
        return Ok(None);
    }
    ZStep::new(length).map(Some).map_err(|_| invalid(CutField::ZStep))
}

/// `0` or an empty field is no offset; otherwise a signed length of at most 50 mm.
fn z_offset(text: &str) -> Result<Option<ZOffset>, MessageKey> {
    if text.trim().is_empty() {
        return Ok(None);
    }
    let length = parse_position(text).map_err(|_| invalid(CutField::ZOffset))?;
    if length.mm() == 0.0 {
        return Ok(None);
    }
    ZOffset::new(length).map(Some).map_err(|_| invalid(CutField::ZOffset))
}
