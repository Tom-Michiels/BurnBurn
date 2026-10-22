//! Process layer (IM13) ↔ DTO mapping. Owner: CAM/job role (in coordination with Kern).
//!
//! Schema 2 stores every operation with its settings. Schema 1 only had placeholder operations
//! (id, name, targets); they migrate to **disabled** Line operations with explicit, inert
//! settings (`MIGRATED_V1_*`) so that opening an old file can never produce laser output
//! before the user has reviewed the operation.

use std::collections::BTreeSet;

use burnburn_document::DocumentSnapshot;
use burnburn_document::process::{
    AdvancedOptions, CutPriority, DitherMode, DotMode, FillGrouping, FillSettings, GrayMethod, ImageCrop, ImageRender,
    ImageSettings, ImageSharpen, Kerf, Lead, LeadStyle, LineOptions, MaterialRecipe, OffsetFillOrder,
    OffsetFillSettings, Operation, OperationDraft, OperationMode, OptimizationSettings, OrderConstraint, OrderKey,
    Overcut, Passes, PathLock, Perforation, PowerPercent, PowerRange, ProcessLayer, ProcessSettings, RecipeDraft,
    RecipeId, RecipeRef, ScanAxis, ScanDirection, ScreenSettings, SharedEdgePolicy, Speed, SubOperation, TabPlacement,
    TabSpec, ToneSettings, ToolLayer, ZOffset, ZStep,
};
use burnburn_types::{ContentHash, GeometryId, Length, OperationId, StableId};

use crate::dto::v1;
use crate::error::{LoadError, SaveError};
use crate::map::num;

/// Speed given to migrated schema-1 placeholder operations (mm/s).
pub const MIGRATED_V1_SPEED_MM_S: f64 = 10.0;
/// Power given to migrated schema-1 placeholder operations (percent, min = max).
pub const MIGRATED_V1_POWER_PCT: f64 = 0.0;

type ProcessDto = (Vec<v1::Operation>, Vec<v1::Constraint>, Vec<v1::Recipe>);

/// The process layer as schema-2 DTOs (IM25 options additively). Fails only for a mode that
/// has no file form yet.
pub(crate) fn process_to_dto(doc: &DocumentSnapshot) -> Result<ProcessDto, SaveError> {
    let layer = doc.process();
    let mut operations = Vec::with_capacity(layer.operations().len());
    for op in layer.operations() {
        operations.push(v1::Operation {
            id: op.id().raw(),
            name: op.name().to_owned(),
            targets: op.targets().iter().map(|g| v1::Target::Ref(v1::TargetRef { geometry: g.raw() })).collect(),
            process: Some(v1::OperationProcess {
                mode: mode_to_dto(op.mode())?,
                settings: settings_to_dto(&op.settings()),
                output_enabled: op.output_enabled(),
                recipe: op.recipe().map(|r| v1::RecipeRef { recipe: r.recipe.raw(), revision: r.revision }),
                advanced: advanced_to_dto(op.advanced())?,
            }),
        });
    }
    let constraints =
        layer.constraints().iter().map(|c| v1::Constraint { before: c.before.raw(), after: c.after.raw() }).collect();
    let mut recipes = Vec::with_capacity(layer.recipes().len());
    for r in layer.recipes() {
        recipes.push(v1::Recipe {
            id: r.id().raw(),
            name: r.name().to_owned(),
            material: r.material().to_owned(),
            thickness_mm: v1::Num(r.thickness().mm()),
            machine: r.machine().to_owned(),
            revision: r.revision(),
            notes: r.notes().to_owned(),
            mode: mode_to_dto(r.mode())?,
            settings: settings_to_dto(&r.settings()),
        });
    }
    Ok((operations, constraints, recipes))
}

/// The optimization settings and priorities as DTOs; both are omitted when they are the defaults.
pub(crate) fn optimization_to_dto(doc: &DocumentSnapshot) -> (Option<v1::Optimization>, Vec<v1::Priority>) {
    let layer = doc.process();
    let o = layer.optimization();
    let optimization = (!o.is_default()).then(|| v1::Optimization {
        order_by: o
            .order_by()
            .iter()
            .map(|k| match k {
                OrderKey::Layer => v1::OrderBy::Layer,
                OrderKey::Group => v1::OrderBy::Group,
                OrderKey::Priority => v1::OrderBy::Priority,
            })
            .collect(),
        inner_first: o.inner_first(),
        reduce_travel: o.reduce_travel(),
        best_start: o.best_start(),
        best_direction: o.best_direction(),
        hide_backlash: o.hide_backlash(),
        remove_overlapping_mm: o.remove_overlapping_mm().map(v1::Num),
    });
    let priorities = layer.priorities().map(|(g, p)| v1::Priority { geometry: g.raw(), priority: p.get() }).collect();
    (optimization, priorities)
}

/// The start/direction locks as DTOs (only shapes with something locked).
pub(crate) fn path_locks_to_dto(doc: &DocumentSnapshot) -> Vec<v1::PathLock> {
    doc.process()
        .path_locks()
        .map(|(g, l)| v1::PathLock { geometry: g.raw(), start: l.start, direction: l.direction })
        .collect()
}

fn optimization_from_dto(o: &v1::Optimization) -> Result<OptimizationSettings, LoadError> {
    let at = |field: &'static str| move || format!("optimization.{field}");
    let order = o
        .order_by
        .iter()
        .map(|k| match k {
            v1::OrderBy::Layer => OrderKey::Layer,
            v1::OrderBy::Group => OrderKey::Group,
            v1::OrderBy::Priority => OrderKey::Priority,
        })
        .collect();
    let mut settings = OptimizationSettings::default()
        .with_order(order)
        .map_err(|e| invalid(&at("order_by"), e))?
        .with_inner_first(o.inner_first)
        .with_reduce_travel(o.reduce_travel)
        .with_best_start(o.best_start)
        .with_best_direction(o.best_direction)
        .with_hide_backlash(o.hide_backlash);
    if let Some(t) = o.remove_overlapping_mm {
        let tolerance = length(&at("remove_overlapping_mm"), t)?;
        settings =
            settings.with_remove_overlapping(Some(tolerance)).map_err(|e| invalid(&at("remove_overlapping_mm"), e))?;
    }
    Ok(settings)
}

fn direction_to(d: ScanDirection) -> v1::ScanDirection {
    match d {
        ScanDirection::Bidirectional => v1::ScanDirection::Bidirectional,
        ScanDirection::Unidirectional => v1::ScanDirection::Unidirectional,
    }
}

fn direction_from(d: v1::ScanDirection) -> ScanDirection {
    match d {
        v1::ScanDirection::Bidirectional => ScanDirection::Bidirectional,
        v1::ScanDirection::Unidirectional => ScanDirection::Unidirectional,
    }
}

/// `None` for default options, so operations without them keep their file form.
fn advanced_to_dto(a: &AdvancedOptions) -> Result<Option<v1::Advanced>, SaveError> {
    if a.is_default() {
        return Ok(None);
    }
    let mut sub_operations = Vec::with_capacity(a.sub_operations.len());
    for s in &a.sub_operations {
        sub_operations.push(v1::SubOperationDto { mode: mode_to_dto(s.mode)?, settings: settings_to_dto(&s.settings) });
    }
    Ok(Some(v1::Advanced {
        tabs: a.line.tabs.map(|t| match t.placement() {
            TabPlacement::Count(n) => v1::Tabs { count: Some(n), spacing_mm: None, width_mm: v1::Num(t.width_mm()) },
            TabPlacement::Spacing(s) => {
                v1::Tabs { count: None, spacing_mm: Some(v1::Num(s)), width_mm: v1::Num(t.width_mm()) }
            }
        }),
        lead_in_mm: a.line.lead_in.map(|l| v1::Num(l.length_mm())),
        lead_out_mm: a.line.lead_out.map(|l| v1::Num(l.length_mm())),
        lead_in_style: a.line.lead_in.and_then(lead_style_to_dto),
        lead_in_angle_deg: a.line.lead_in.and_then(lead_angle_to_dto),
        lead_out_style: a.line.lead_out.and_then(lead_style_to_dto),
        lead_out_angle_deg: a.line.lead_out.and_then(lead_angle_to_dto),
        dot: a.line.dot.map(|d| v1::DotDto { spacing_mm: v1::Num(d.spacing_mm()), dwell_ms: v1::Num(d.dwell_ms()) }),
        overcut_mm: a.line.overcut.map(|o| v1::Num(o.length_mm())),
        perforation: a
            .line
            .perforation
            .map(|p| v1::PerforationDto { on_mm: v1::Num(p.on_mm()), off_mm: v1::Num(p.off_mm()) }),
        z_step_mm: a.z_step.map(|z| v1::Num(z.mm())),
        z_offset_mm: a.z_offset.map(|z| v1::Num(z.mm())),
        sub_operations,
        shared_edges: match a.shared_edges {
            SharedEdgePolicy::CutOnce => None,
            SharedEdgePolicy::CutPerRegion => Some(v1::SharedEdges::CutPerRegion),
        },
        tool: a.tool.map(|t| match t {
            ToolLayer::T1 => v1::ToolDto::T1,
            ToolLayer::T2 => v1::ToolDto::T2,
        }),
    }))
}

/// `None` for the default line lead.
fn lead_style_to_dto(l: Lead) -> Option<v1::LeadStyle> {
    match l.style() {
        LeadStyle::Line => None,
        LeadStyle::Arc => Some(v1::LeadStyle::Arc),
    }
}

/// `None` for the default angle (perpendicular).
fn lead_angle_to_dto(l: Lead) -> Option<v1::Num> {
    (l.angle_deg() != 90.0).then(|| v1::Num(l.angle_deg()))
}

pub(crate) fn mode_to_dto(mode: OperationMode) -> Result<v1::Mode, SaveError> {
    Ok(match mode {
        OperationMode::Fill(f) => v1::Mode::Fill {
            interval_mm: v1::Num(f.interval().mm()),
            axis: match f.axis() {
                ScanAxis::X => v1::ScanAxis::X,
                ScanAxis::Y => v1::ScanAxis::Y,
            },
            direction: match f.direction() {
                ScanDirection::Bidirectional => v1::ScanDirection::Bidirectional,
                ScanDirection::Unidirectional => v1::ScanDirection::Unidirectional,
            },
            overscan_mm: v1::Num(f.overscan().mm()),
            angle_deg: (f.angle_deg() != 0.0).then(|| v1::Num(f.angle_deg())),
            cross_hatch: f.cross_hatch(),
            grouping: match f.grouping() {
                FillGrouping::AllShapes => None,
                FillGrouping::Groups => Some(v1::FillGrouping::Groups),
                FillGrouping::Individual => Some(v1::FillGrouping::Individual),
            },
            flood_fill: f.flood_fill(),
            ramp_mm: (f.ramp_mm() != 0.0).then(|| v1::Num(f.ramp_mm())),
        },
        OperationMode::Line { kerf } => v1::Mode::Line { kerf_mm: v1::Num(kerf.width_mm()) },
        OperationMode::OffsetFill(o) => v1::Mode::OffsetFill {
            step_mm: v1::Num(o.step().mm()),
            order: match o.order() {
                OffsetFillOrder::OutsideIn => v1::RingOrder::OutsideIn,
                OffsetFillOrder::InsideOut => v1::RingOrder::InsideOut,
            },
        },
        OperationMode::Image(i) => v1::Mode::Image {
            source: i.source().to_hex(),
            dot_pitch_mm: v1::Num(i.dot_pitch().mm()),
            interval_mm: v1::Num(i.line_interval().mm()),
            render: match i.render() {
                ImageRender::Grayscale => v1::Render::Grayscale,
                ImageRender::Dither(DitherMode::Threshold { level }) => v1::Render::Threshold { level },
                ImageRender::Dither(DitherMode::FloydSteinberg) => v1::Render::FloydSteinberg,
                ImageRender::Dither(DitherMode::Jarvis) => v1::Render::Jarvis,
                ImageRender::Dither(DitherMode::Stucki) => v1::Render::Stucki,
                ImageRender::Dither(DitherMode::Atkinson) => v1::Render::Atkinson,
                ImageRender::Dither(DitherMode::Ordered { size }) => v1::Render::Ordered { size },
                ImageRender::Dither(DitherMode::Newsprint) => v1::Render::Newsprint,
                ImageRender::Dither(DitherMode::Halftone) => v1::Render::Halftone,
                ImageRender::Dither(DitherMode::Sketch { level }) => v1::Render::Sketch { level },
                ImageRender::Dither(DitherMode::PassThrough) => v1::Render::PassThrough,
                // `DitherMode` is non-exhaustive: never store a future algorithm as another one.
                ImageRender::Dither(_) => return Err(SaveError::Encode("dither mode has no file form".into())),
            },
            gamma: v1::Num(i.tone().gamma()),
            brightness: v1::Num(i.tone().brightness()),
            contrast: v1::Num(i.tone().contrast()),
            direction: direction_to(i.direction()),
            serpentine: i.serpentine(),
            overscan_mm: v1::Num(i.overscan_mm()),
            options: image_options_to_dto(&i),
        },
        // `OperationMode` is non-exhaustive: a future mode without a schema is an error, never
        // saved as another mode.
        _ => return Err(SaveError::Encode("operation mode has no file schema".into())),
    })
}

/// `None` when every option is at its default, so earlier image operations keep their file form.
fn image_options_to_dto(i: &ImageSettings) -> Option<v1::ImageOptions> {
    if !i.has_extended_options() {
        return None;
    }
    let num = |v: f64| v1::Num(v);
    let screen = i.screen();
    Some(v1::ImageOptions {
        negative: i.negative(),
        dot_width_correction_mm: (i.dot_width_correction_mm() != 0.0).then(|| num(i.dot_width_correction_mm())),
        scan_angle_deg: (i.scan_angle_deg() != 0.0).then(|| num(i.scan_angle_deg())),
        gray: (i.gray_method() != GrayMethod::Rec709).then(|| match i.gray_method() {
            GrayMethod::Rec601 => v1::GrayMethod::Rec601,
            GrayMethod::Average => v1::GrayMethod::Average,
            GrayMethod::Lightness => v1::GrayMethod::Lightness,
            GrayMethod::Red => v1::GrayMethod::Red,
            GrayMethod::Green => v1::GrayMethod::Green,
            GrayMethod::Blue => v1::GrayMethod::Blue,
            _ => v1::GrayMethod::Rec709,
        }),
        crop: i.crop().map(|c| v1::Crop { x: c.x, y: c.y, width: c.width, height: c.height }),
        sharpen: i.sharpen().map(|s| v1::Sharpen { amount: num(s.amount()), radius_mm: num(s.radius_mm()) }),
        screen: (screen != ScreenSettings::default())
            .then(|| v1::Screen { cell_mm: num(screen.cell_mm()), angle_deg: num(screen.angle_deg()) }),
        mask: i.mask(),
    })
}

pub(crate) fn settings_to_dto(s: &ProcessSettings) -> v1::Settings {
    v1::Settings {
        speed_mm_s: v1::Num(s.speed.get_mm_per_s()),
        power_min_pct: v1::Num(s.power.min().get()),
        power_max_pct: v1::Num(s.power.max().get()),
        passes: s.passes.get(),
        air_assist: s.air_assist,
    }
}

type PathFn<'a> = &'a dyn Fn() -> String;

fn invalid(path: PathFn<'_>, e: impl std::fmt::Display) -> LoadError {
    LoadError::InvalidValue { path: path(), reason: e.to_string() }
}

fn length(path: PathFn<'_>, n: v1::Num) -> Result<Length, LoadError> {
    Length::from_mm(num(path, n)?).map_err(|e| invalid(path, e))
}

pub(crate) fn mode_from_dto(path: PathFn<'_>, m: &v1::Mode) -> Result<OperationMode, LoadError> {
    Ok(match *m {
        v1::Mode::Line { kerf_mm } => {
            let at = || format!("{}.kerf_mm", path());
            OperationMode::Line { kerf: Kerf::width(length(&at, kerf_mm)?).map_err(|e| invalid(&at, e))? }
        }
        v1::Mode::Fill {
            interval_mm,
            axis,
            direction,
            overscan_mm,
            angle_deg,
            cross_hatch,
            grouping,
            flood_fill,
            ramp_mm,
        } => {
            let interval = length(&|| format!("{}.interval_mm", path()), interval_mm)?;
            let overscan = length(&|| format!("{}.overscan_mm", path()), overscan_mm)?;
            let axis = match axis {
                v1::ScanAxis::X => ScanAxis::X,
                v1::ScanAxis::Y => ScanAxis::Y,
            };
            let fill =
                FillSettings::new(interval, axis, direction_from(direction), overscan).map_err(|e| invalid(path, e))?;
            let fill = match angle_deg {
                None => fill,
                Some(a) => {
                    let at = || format!("{}.angle_deg", path());
                    fill.with_angle(num(&at, a)?).map_err(|e| invalid(&at, e))?
                }
            };
            let ramp = match ramp_mm {
                None => fill,
                Some(r) => {
                    let at = || format!("{}.ramp_mm", path());
                    fill.with_ramp(length(&at, r)?).map_err(|e| invalid(&at, e))?
                }
            };
            let grouping = match grouping {
                None => FillGrouping::AllShapes,
                Some(v1::FillGrouping::Groups) => FillGrouping::Groups,
                Some(v1::FillGrouping::Individual) => FillGrouping::Individual,
            };
            OperationMode::Fill(ramp.with_cross_hatch(cross_hatch).with_grouping(grouping).with_flood_fill(flood_fill))
        }
        v1::Mode::OffsetFill { step_mm, order } => {
            let at = || format!("{}.step_mm", path());
            let order = match order {
                v1::RingOrder::OutsideIn => OffsetFillOrder::OutsideIn,
                v1::RingOrder::InsideOut => OffsetFillOrder::InsideOut,
            };
            OperationMode::OffsetFill(
                OffsetFillSettings::new(length(&at, step_mm)?, order).map_err(|e| invalid(&at, e))?,
            )
        }
        v1::Mode::Image {
            ref source,
            dot_pitch_mm,
            interval_mm,
            render,
            gamma,
            brightness,
            contrast,
            direction,
            serpentine,
            overscan_mm,
            ref options,
        } => {
            let at = |f: &'static str| move || format!("{}.{f}", path());
            let source = ContentHash::from_hex(source).map_err(|e| invalid(&at("source"), e))?;
            let render = match render {
                v1::Render::Grayscale => ImageRender::Grayscale,
                v1::Render::Threshold { level } => ImageRender::Dither(DitherMode::Threshold { level }),
                v1::Render::FloydSteinberg => ImageRender::Dither(DitherMode::FloydSteinberg),
                v1::Render::Jarvis => ImageRender::Dither(DitherMode::Jarvis),
                v1::Render::Stucki => ImageRender::Dither(DitherMode::Stucki),
                v1::Render::Atkinson => ImageRender::Dither(DitherMode::Atkinson),
                v1::Render::Ordered { size } => ImageRender::Dither(DitherMode::Ordered { size }),
                v1::Render::Newsprint => ImageRender::Dither(DitherMode::Newsprint),
                v1::Render::Halftone => ImageRender::Dither(DitherMode::Halftone),
                v1::Render::Sketch { level } => ImageRender::Dither(DitherMode::Sketch { level }),
                v1::Render::PassThrough => ImageRender::Dither(DitherMode::PassThrough),
            };
            let tone = ToneSettings::new(
                num(&at("gamma"), gamma)?,
                num(&at("brightness"), brightness)?,
                num(&at("contrast"), contrast)?,
            )
            .map_err(|e| invalid(path, e))?;
            let settings = ImageSettings::new(
                source,
                length(&at("dot_pitch_mm"), dot_pitch_mm)?,
                length(&at("interval_mm"), interval_mm)?,
                render,
            )
            .map_err(|e| invalid(path, e))?
            .with_tone(tone)
            .with_direction(direction_from(direction), serpentine)
            .with_overscan(length(&at("overscan_mm"), overscan_mm)?)
            .map_err(|e| invalid(&at("overscan_mm"), e))?;
            let settings = match options {
                Some(o) => image_options_from_dto(&|| format!("{}.options", path()), o, settings)?,
                None => settings,
            };
            OperationMode::Image(settings)
        }
    })
}

fn image_options_from_dto(
    path: PathFn<'_>,
    o: &v1::ImageOptions,
    settings: ImageSettings,
) -> Result<ImageSettings, LoadError> {
    let at = |f: &'static str| move || format!("{}.{f}", path());
    let mut s = settings.with_negative(o.negative).with_mask(o.mask);
    if let Some(n) = o.dot_width_correction_mm {
        s = s
            .with_dot_width_correction(length(&at("dot_width_correction_mm"), n)?)
            .map_err(|e| invalid(&at("dot_width_correction_mm"), e))?;
    }
    if let Some(n) = o.scan_angle_deg {
        s = s.with_scan_angle(num(&at("scan_angle_deg"), n)?).map_err(|e| invalid(&at("scan_angle_deg"), e))?;
    }
    if let Some(g) = o.gray {
        s = s.with_gray_method(match g {
            v1::GrayMethod::Rec709 => GrayMethod::Rec709,
            v1::GrayMethod::Rec601 => GrayMethod::Rec601,
            v1::GrayMethod::Average => GrayMethod::Average,
            v1::GrayMethod::Lightness => GrayMethod::Lightness,
            v1::GrayMethod::Red => GrayMethod::Red,
            v1::GrayMethod::Green => GrayMethod::Green,
            v1::GrayMethod::Blue => GrayMethod::Blue,
        });
    }
    if let Some(c) = o.crop {
        let crop = ImageCrop { x: c.x, y: c.y, width: c.width, height: c.height };
        s = s.with_crop(Some(crop)).map_err(|e| invalid(&at("crop"), e))?;
    }
    if let Some(sh) = o.sharpen {
        let radius = length(&at("sharpen.radius_mm"), v1::Num(sh.radius_mm.0))?;
        let sharpen = ImageSharpen::new(num(&at("sharpen.amount"), sh.amount)?, radius)
            .map_err(|e| invalid(&at("sharpen"), e))?;
        s = s.with_sharpen(Some(sharpen));
    }
    if let Some(sc) = o.screen {
        let cell = length(&at("screen.cell_mm"), sc.cell_mm)?;
        let screen = ScreenSettings::new(cell, num(&at("screen.angle_deg"), sc.angle_deg)?)
            .map_err(|e| invalid(&at("screen"), e))?;
        s = s.with_screen(screen);
    }
    Ok(s)
}

fn advanced_from_dto(path: PathFn<'_>, a: &v1::Advanced) -> Result<AdvancedOptions, LoadError> {
    let at = |f: &'static str| move || format!("{}.{f}", path());
    let opt = |f: &'static str, n: Option<v1::Num>| n.map(|n| length(&at(f), n)).transpose();
    let tabs = match a.tabs {
        None => None,
        Some(t) => {
            let width = length(&at("tabs.width_mm"), t.width_mm)?;
            let spec = match (t.count, t.spacing_mm) {
                (Some(count), None) => TabSpec::new(count, width),
                (None, Some(spacing)) => TabSpec::spaced(length(&at("tabs.spacing_mm"), spacing)?, width),
                _ => {
                    return Err(invalid(&at("tabs"), "exactly one of count and spacing_mm is required"));
                }
            };
            Some(spec.map_err(|e| invalid(&at("tabs"), e))?)
        }
    };
    let lead = |f: &'static str,
                n,
                style: Option<v1::LeadStyle>,
                angle_field: &'static str,
                angle: Option<v1::Num>|
     -> Result<Option<Lead>, LoadError> {
        let Some(l) = opt(f, n)? else {
            if style.is_some() || angle.is_some() {
                return Err(invalid(&at(angle_field), "a lead style or angle needs the lead length"));
            }
            return Ok(None);
        };
        let mut lead = Lead::new(l).map_err(|e| invalid(&at(f), e))?;
        if let Some(v1::LeadStyle::Arc) = style {
            lead = lead.with_style(LeadStyle::Arc);
        }
        if let Some(a) = angle {
            lead = lead.with_angle(num(&at(angle_field), a)?).map_err(|e| invalid(&at(angle_field), e))?;
        }
        Ok(Some(lead))
    };
    let dot = match a.dot {
        None => None,
        Some(d) => {
            let spacing = length(&at("dot.spacing_mm"), d.spacing_mm)?;
            Some(DotMode::new(spacing, num(&at("dot.dwell_ms"), d.dwell_ms)?).map_err(|e| invalid(&at("dot"), e))?)
        }
    };
    let perforation = match a.perforation {
        None => None,
        Some(p) => {
            let on = length(&at("perforation.on_mm"), p.on_mm)?;
            let off = length(&at("perforation.off_mm"), p.off_mm)?;
            Some(Perforation::new(on, off).map_err(|e| invalid(&at("perforation"), e))?)
        }
    };
    let mut sub_operations = Vec::with_capacity(a.sub_operations.len());
    for (i, s) in a.sub_operations.iter().enumerate() {
        let sp = || format!("{}.sub_operations[{i}]", path());
        sub_operations.push(SubOperation {
            mode: mode_from_dto(&|| format!("{}.mode", sp()), &s.mode)?,
            settings: settings_from_dto(&|| format!("{}.settings", sp()), &s.settings)?,
        });
    }
    Ok(AdvancedOptions {
        line: LineOptions {
            tabs,
            lead_in: lead("lead_in_mm", a.lead_in_mm, a.lead_in_style, "lead_in_angle_deg", a.lead_in_angle_deg)?,
            lead_out: lead("lead_out_mm", a.lead_out_mm, a.lead_out_style, "lead_out_angle_deg", a.lead_out_angle_deg)?,
            overcut: opt("overcut_mm", a.overcut_mm)?
                .map(|l| Overcut::new(l).map_err(|e| invalid(&at("overcut_mm"), e)))
                .transpose()?,
            perforation,
            dot,
        },
        z_step: opt("z_step_mm", a.z_step_mm)?
            .map(|l| ZStep::new(l).map_err(|e| invalid(&at("z_step_mm"), e)))
            .transpose()?,
        z_offset: opt("z_offset_mm", a.z_offset_mm)?
            .map(|l| ZOffset::new(l).map_err(|e| invalid(&at("z_offset_mm"), e)))
            .transpose()?,
        sub_operations,
        shared_edges: match a.shared_edges {
            None | Some(v1::SharedEdges::CutOnce) => SharedEdgePolicy::CutOnce,
            Some(v1::SharedEdges::CutPerRegion) => SharedEdgePolicy::CutPerRegion,
        },
        tool: a.tool.map(|t| match t {
            v1::ToolDto::T1 => ToolLayer::T1,
            v1::ToolDto::T2 => ToolLayer::T2,
        }),
    })
}

pub(crate) fn settings_from_dto(path: PathFn<'_>, s: &v1::Settings) -> Result<ProcessSettings, LoadError> {
    let at = |f: &'static str| move || format!("{}.{f}", path());
    let pct = |f: &'static str, n| -> Result<PowerPercent, LoadError> {
        PowerPercent::new(num(&at(f), n)?).map_err(|e| invalid(&at(f), e))
    };
    Ok(ProcessSettings {
        speed: Speed::mm_per_s(num(&at("speed_mm_s"), s.speed_mm_s)?).map_err(|e| invalid(&at("speed_mm_s"), e))?,
        power: PowerRange::new(pct("power_min_pct", s.power_min_pct)?, pct("power_max_pct", s.power_max_pct)?)
            .map_err(|e| invalid(path, e))?,
        passes: Passes::new(s.passes).map_err(|e| invalid(&at("passes"), e))?,
        air_assist: s.air_assist,
    })
}

fn migrated_v1_settings() -> Result<ProcessSettings, LoadError> {
    let path = || "operations".to_owned();
    let p = PowerPercent::new(MIGRATED_V1_POWER_PCT).map_err(|e| invalid(&path, e))?;
    Ok(ProcessSettings {
        speed: Speed::mm_per_s(MIGRATED_V1_SPEED_MM_S).map_err(|e| invalid(&path, e))?,
        power: PowerRange::new(p, p).map_err(|e| invalid(&path, e))?,
        passes: Passes::new(1).map_err(|e| invalid(&path, e))?,
        air_assist: false,
    })
}

/// Rebuilds the process layer and attaches it to `doc`. `schema` decides which operation form
/// is valid: schema 1 = placeholder (migrated), schema 2 = full process data.
pub(crate) fn process_from_dto(
    dto: &v1::Document,
    schema: u32,
    doc: DocumentSnapshot,
) -> Result<DocumentSnapshot, LoadError> {
    let mut seen = BTreeSet::new();
    let mut check_id = |path: PathFn<'_>, raw: u64| -> Result<(), LoadError> {
        let clashes = doc.object(GeometryId::from_raw(raw)).is_some();
        if raw >= doc.next_id() || clashes || !seen.insert(raw) {
            return Err(LoadError::InvalidValue {
                path: path(),
                reason: format!("id {raw} is reused or not below next_id {}", doc.next_id()),
            });
        }
        Ok(())
    };
    let mut operations = Vec::with_capacity(dto.operations.len());
    for (i, op) in dto.operations.iter().enumerate() {
        let opath = move || format!("operations[{i}]");
        check_id(&|| format!("{}.id", opath()), op.id)?;
        let id = OperationId::from_raw(op.id);
        let mut targets = Vec::with_capacity(op.targets.len());
        for (t, target) in op.targets.iter().enumerate() {
            let tpath = || format!("{}.targets[{t}]", opath());
            let raw = match (schema, target) {
                (1, v1::Target::Legacy(raw)) => *raw,
                (2.., v1::Target::Ref(r)) => r.geometry,
                _ => {
                    return Err(LoadError::InvalidValue {
                        path: tpath(),
                        reason: format!("not a schema {schema} target"),
                    });
                }
            };
            let g = GeometryId::from_raw(raw);
            if doc.object(g).is_none() {
                return Err(LoadError::InvalidValue { path: tpath(), reason: format!("no geometry {g}") });
            }
            targets.push(g);
        }
        let mut advanced = None;
        let (draft, recipe) = match (schema, &op.process) {
            (1, None) => (
                OperationDraft {
                    name: op.name.clone(),
                    mode: OperationMode::Line { kerf: Kerf::NONE },
                    settings: migrated_v1_settings()?,
                    output_enabled: false,
                },
                None,
            ),
            (2.., Some(p)) => {
                let mode = mode_from_dto(&|| format!("{}.process.mode", opath()), &p.mode)?;
                let settings = settings_from_dto(&|| format!("{}.process.settings", opath()), &p.settings)?;
                let recipe = p.recipe.map(|r| RecipeRef { recipe: RecipeId::from_raw(r.recipe), revision: r.revision });
                if let Some(a) = &p.advanced {
                    advanced = Some(advanced_from_dto(&|| format!("{}.process.advanced", opath()), a)?);
                }
                (OperationDraft { name: op.name.clone(), mode, settings, output_enabled: p.output_enabled }, recipe)
            }
            _ => {
                return Err(LoadError::InvalidValue {
                    path: format!("{}.process", opath()),
                    reason: format!("not valid for schema {schema}"),
                });
            }
        };
        let mut operation = Operation::new(id, draft, targets, recipe).map_err(|e| invalid(&opath, e))?;
        if let Some(a) = advanced {
            operation =
                operation.with_advanced(a).map_err(|e| invalid(&|| format!("{}.process.advanced", opath()), e))?;
        }
        operations.push(operation);
    }
    if schema < 2 && !(dto.constraints.is_empty() && dto.recipes.is_empty()) {
        return Err(LoadError::InvalidValue { path: "constraints".into(), reason: "not valid for schema 1".into() });
    }
    let mut recipes = Vec::with_capacity(dto.recipes.len());
    for (i, r) in dto.recipes.iter().enumerate() {
        let rpath = move || format!("recipes[{i}]");
        check_id(&|| format!("{}.id", rpath()), r.id)?;
        let draft = RecipeDraft {
            name: r.name.clone(),
            material: r.material.clone(),
            thickness: length(&|| format!("{}.thickness_mm", rpath()), r.thickness_mm)?,
            machine: r.machine.clone(),
            notes: r.notes.clone(),
            mode: mode_from_dto(&|| format!("{}.mode", rpath()), &r.mode)?,
            settings: settings_from_dto(&|| format!("{}.settings", rpath()), &r.settings)?,
        };
        recipes.push(MaterialRecipe::new(RecipeId::from_raw(r.id), r.revision, draft).map_err(|e| invalid(&rpath, e))?);
    }
    let constraints = dto
        .constraints
        .iter()
        .map(|c| OrderConstraint { before: OperationId::from_raw(c.before), after: OperationId::from_raw(c.after) })
        .collect();
    let layer = ProcessLayer::from_parts(operations, constraints, recipes)
        .map_err(|e| LoadError::InvalidValue { path: "operations".into(), reason: e.to_string() })?;
    let mut priorities = Vec::with_capacity(dto.priorities.len());
    for (i, p) in dto.priorities.iter().enumerate() {
        let at = move || format!("priorities[{i}]");
        let geometry = GeometryId::from_raw(p.geometry);
        if doc.object(geometry).is_none() {
            return Err(invalid(&|| format!("{}.geometry", at()), "no such shape"));
        }
        if priorities.iter().any(|(g, _)| *g == geometry) {
            return Err(invalid(&|| format!("{}.geometry", at()), "priority set twice"));
        }
        let priority = CutPriority::new(p.priority).map_err(|e| invalid(&|| format!("{}.priority", at()), e))?;
        priorities.push((geometry, priority));
    }
    let layer = layer.with_priorities(priorities);
    let mut locks = Vec::with_capacity(dto.path_locks.len());
    for (i, l) in dto.path_locks.iter().enumerate() {
        let at = move || format!("path_locks[{i}]");
        let geometry = GeometryId::from_raw(l.geometry);
        if doc.object(geometry).is_none() {
            return Err(invalid(&|| format!("{}.geometry", at()), "no such shape"));
        }
        if locks.iter().any(|(g, _)| *g == geometry) {
            return Err(invalid(&|| format!("{}.geometry", at()), "shape locked twice"));
        }
        let lock = PathLock { start: l.start, direction: l.direction };
        if lock.is_none() {
            return Err(invalid(&|| at(), "an entry that locks nothing"));
        }
        locks.push((geometry, lock));
    }
    let layer = layer.with_path_locks(locks);
    if dto.notes.chars().count() > burnburn_document::process::ProcessCommand::MAX_NOTES_CHARS {
        return Err(invalid(&|| "notes".to_owned(), "too long"));
    }
    let layer = layer.with_notes(dto.notes.clone());
    let layer = match &dto.optimization {
        Some(o) => layer.with_optimization(optimization_from_dto(o)?),
        None => layer,
    };
    doc.with_process(layer).map_err(|e| LoadError::InvalidValue { path: "operations".into(), reason: e.to_string() })
}
