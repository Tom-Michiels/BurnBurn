//! Process layer (IM13) ↔ DTO mapping. Owner: CAM/job role (in coordination with Kern).
//!
//! Schema 2 stores every operation with its settings. Schema 1 only had placeholder operations
//! (id, name, targets); they migrate to **disabled** Line operations with explicit, inert
//! settings (`MIGRATED_V1_*`) so that opening an old file can never produce laser output
//! before the user has reviewed the operation.

use std::collections::BTreeSet;

use burnburn_document::DocumentSnapshot;
use burnburn_document::process::{
    AdvancedOptions, DitherMode, FillSettings, ImageRender, ImageSettings, Kerf, Lead, LineOptions, MaterialRecipe,
    OffsetFillOrder, OffsetFillSettings, Operation, OperationDraft, OperationMode, OrderConstraint, Overcut, Passes,
    Perforation, PowerPercent, PowerRange, ProcessLayer, ProcessSettings, RecipeDraft, RecipeId, RecipeRef, ScanAxis,
    ScanDirection, SharedEdgePolicy, Speed, SubOperation, TabSpec, ToneSettings, ZStep,
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
        tabs: a.line.tabs.map(|t| v1::Tabs { count: t.count(), width_mm: v1::Num(t.width_mm()) }),
        lead_in_mm: a.line.lead_in.map(|l| v1::Num(l.length_mm())),
        lead_out_mm: a.line.lead_out.map(|l| v1::Num(l.length_mm())),
        overcut_mm: a.line.overcut.map(|o| v1::Num(o.length_mm())),
        perforation: a
            .line
            .perforation
            .map(|p| v1::PerforationDto { on_mm: v1::Num(p.on_mm()), off_mm: v1::Num(p.off_mm()) }),
        z_step_mm: a.z_step.map(|z| v1::Num(z.mm())),
        sub_operations,
        shared_edges: match a.shared_edges {
            SharedEdgePolicy::CutOnce => None,
            SharedEdgePolicy::CutPerRegion => Some(v1::SharedEdges::CutPerRegion),
        },
    }))
}

fn mode_to_dto(mode: OperationMode) -> Result<v1::Mode, SaveError> {
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
                // `DitherMode` is non-exhaustive: never store a future algorithm as another one.
                ImageRender::Dither(_) => return Err(SaveError::Encode("dither mode has no file form".into())),
            },
            gamma: v1::Num(i.tone().gamma()),
            brightness: v1::Num(i.tone().brightness()),
            contrast: v1::Num(i.tone().contrast()),
            direction: direction_to(i.direction()),
            serpentine: i.serpentine(),
            overscan_mm: v1::Num(i.overscan_mm()),
        },
        // `OperationMode` is non-exhaustive: a future mode without a schema is an error, never
        // saved as another mode.
        _ => return Err(SaveError::Encode("operation mode has no file schema".into())),
    })
}

fn settings_to_dto(s: &ProcessSettings) -> v1::Settings {
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

fn mode_from_dto(path: PathFn<'_>, m: &v1::Mode) -> Result<OperationMode, LoadError> {
    Ok(match *m {
        v1::Mode::Line { kerf_mm } => {
            let at = || format!("{}.kerf_mm", path());
            OperationMode::Line { kerf: Kerf::width(length(&at, kerf_mm)?).map_err(|e| invalid(&at, e))? }
        }
        v1::Mode::Fill { interval_mm, axis, direction, overscan_mm, angle_deg } => {
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
            OperationMode::Fill(fill)
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
            OperationMode::Image(settings)
        }
    })
}

fn advanced_from_dto(path: PathFn<'_>, a: &v1::Advanced) -> Result<AdvancedOptions, LoadError> {
    let at = |f: &'static str| move || format!("{}.{f}", path());
    let opt = |f: &'static str, n: Option<v1::Num>| n.map(|n| length(&at(f), n)).transpose();
    let tabs = match a.tabs {
        None => None,
        Some(t) => {
            let width = length(&at("tabs.width_mm"), t.width_mm)?;
            Some(TabSpec::new(t.count, width).map_err(|e| invalid(&at("tabs"), e))?)
        }
    };
    let lead = |f: &'static str, n| -> Result<Option<Lead>, LoadError> {
        opt(f, n)?.map(|l| Lead::new(l).map_err(|e| invalid(&at(f), e))).transpose()
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
            lead_in: lead("lead_in_mm", a.lead_in_mm)?,
            lead_out: lead("lead_out_mm", a.lead_out_mm)?,
            overcut: opt("overcut_mm", a.overcut_mm)?
                .map(|l| Overcut::new(l).map_err(|e| invalid(&at("overcut_mm"), e)))
                .transpose()?,
            perforation,
        },
        z_step: opt("z_step_mm", a.z_step_mm)?
            .map(|l| ZStep::new(l).map_err(|e| invalid(&at("z_step_mm"), e)))
            .transpose()?,
        sub_operations,
        shared_edges: match a.shared_edges {
            None | Some(v1::SharedEdges::CutOnce) => SharedEdgePolicy::CutOnce,
            Some(v1::SharedEdges::CutPerRegion) => SharedEdgePolicy::CutPerRegion,
        },
    })
}

fn settings_from_dto(path: PathFn<'_>, s: &v1::Settings) -> Result<ProcessSettings, LoadError> {
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
    doc.with_process(layer).map_err(|e| LoadError::InvalidValue { path: "operations".into(), reason: e.to_string() })
}
