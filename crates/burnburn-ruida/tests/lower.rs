//! IM14 coupling (owner CAM/job): a compiled JobPlan lowered to RD and compared with the real
//! LightBurn Core 2.1.04 exports.
//!
//! The reference conditions (`test-data/ruida/manifest.json`, `04-exportanalyse.md`): one
//! 30 × 15 mm rectangle with corners (771.687; 448.024)–(801.687; 463.024) mm in a
//! 1500 × 900 mm research profile, 100 mm/s, min/max 20 %, air on, one pass; Line, and Fill
//! with 0.1 mm interval, 0°, bidirectional. The semantic comparison (bounds, power, speed,
//! moves) is decisive. Byte equality is only claimed for records whose conditions we
//! reproduce exactly (speed, laser-1 power, each absolute cut). Differences are explained in
//! `docs/dev-notes/IM15.md`; expected fixture bytes are never adjusted.

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers outside #[test] fns

mod common;

use burnburn_cam::{CompileOptions, Placement, StartPoint, compile};
use burnburn_document::process::{
    FillSettings, Kerf, OperationDraft, OperationMode, Passes, PowerPercent, PowerRange, ProcessCommand,
    ProcessSettings, ScanAxis, ScanDirection, Speed as DocSpeed,
};
use burnburn_document::{CommandPayload, DocumentCommand, DocumentSession};
use burnburn_geometry::{Point, ProductionTolerance, Rect};
use burnburn_job::{JobPlan, MachineFrame, MachineProfile, OriginCorner, PowerRatio, preview};
use burnburn_ruida::{
    AbsCoord, BoundsKind, Command, Laser, LowerError, Magic, ModeFlag, OverscanPolicy, PowerBound, RdFile,
    RuidaProfile, decode_rd_file, encode, lower,
};
use burnburn_types::{CancellationToken, DocumentId, Length, RequestId, StableId};
use common::fixture;

fn mm(v: f64) -> Length {
    Length::from_mm(v).unwrap()
}

const RUIDA: RuidaProfile = RuidaProfile { magic: Magic::FIXTURE_0X88, overscan: OverscanPolicy::ControllerManaged };

fn research_profile() -> MachineProfile {
    MachineProfile::new(
        "research-1500x900",
        1,
        // Bottom-left origin: document mm = RD machine mm, so the fixture corners are reproduced.
        MachineFrame::new(1500.0, 900.0, OriginCorner::BottomLeft).unwrap(),
        1000.0,
        400.0,
        PowerRatio::new(0.0).unwrap(),
        PowerRatio::new(1.0).unwrap(),
    )
    .unwrap()
}

fn fixture_rectangle_plan(mode: OperationMode) -> JobPlan {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let apply = |s: &mut DocumentSession, payload| {
        let cmd =
            DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload };
        s.apply(cmd).unwrap()
    };
    let rect = Rect::new(Point::new(771.687, 448.024), mm(30.0), mm(15.0)).unwrap();
    let g = apply(&mut s, CommandPayload::CreateRectangle { rect, name: "rectangle.svg".into() }).changes.created[0];
    let p20 = PowerPercent::new(20.0).unwrap();
    let draft = OperationDraft {
        name: "C00".into(),
        mode,
        settings: ProcessSettings {
            speed: DocSpeed::mm_per_s(100.0).unwrap(),
            power: PowerRange::new(p20, p20).unwrap(),
            passes: Passes::new(1).unwrap(),
            air_assist: true,
        },
        output_enabled: true,
    };
    apply(&mut s, CommandPayload::Process(ProcessCommand::AddOperation { draft, targets: vec![g] }));
    let options = CompileOptions {
        tolerance: ProductionTolerance::new(mm(0.001)).unwrap(),
        placement: Placement::Absolute,
        start: StartPoint::MachineOrigin,
    };
    compile(&s.snapshot(), &research_profile(), &options, &CancellationToken::never()).unwrap()
}

fn fill_mode() -> OperationMode {
    OperationMode::Fill(FillSettings::new(mm(0.1), ScanAxis::X, ScanDirection::Bidirectional, mm(0.0)).unwrap())
}

fn decoded(rd: &[u8]) -> RdFile {
    decode_rd_file(rd, Magic::FIXTURE_0X88).unwrap()
}

/// `(x, y)` µm of absolute travel (`88`) and cut (`A8`) targets, with relative moves resolved.
#[derive(Debug, Default)]
struct Motion {
    travels: Vec<(i64, i64)>,
    cuts: Vec<((i64, i64), (i64, i64))>,
    relative_travel_dy: Vec<i32>,
}

fn motion(file: &RdFile) -> Motion {
    let mut m = Motion::default();
    let mut at = (0i64, 0i64);
    for c in file.commands() {
        match *c {
            Command::MoveAbs { x, y } => {
                at = (x.um() as i64, y.um() as i64);
                m.travels.push(at);
            }
            Command::MoveRelY(d) => {
                at.1 += i64::from(d.um());
                m.relative_travel_dy.push(d.um());
                m.travels.push(at);
            }
            Command::CutAbs { x, y } => {
                let to = (x.um() as i64, y.um() as i64);
                m.cuts.push((at, to));
                at = to;
            }
            Command::CutRelX(d) => {
                let to = (at.0 + i64::from(d.um()), at.1);
                m.cuts.push((at, to));
                at = to;
            }
            Command::MoveRel { .. } | Command::MoveRelX(_) | Command::CutRel { .. } | Command::CutRelY(_) => {
                panic!("unexpected motion opcode {c:?}")
            }
            _ => {}
        }
    }
    m
}

fn bounds(file: &RdFile, min: BoundsKind, max: BoundsKind) -> ((u64, u64), (u64, u64)) {
    let find = |k| {
        file.commands()
            .find_map(|c| match *c {
                Command::Bounds { kind, x, y } if kind == k => Some((x.um(), y.um())),
                _ => None,
            })
            .unwrap()
    };
    (find(min), find(max))
}

fn has(file: &RdFile, c: &Command) -> bool {
    file.commands().any(|x| x == c)
}

/// Undirected segment as a sorted pair, so a different start vertex compares equal.
fn undirected(s: ((i64, i64), (i64, i64))) -> ((i64, i64), (i64, i64)) {
    if s.0 <= s.1 { s } else { (s.1, s.0) }
}

#[test]
fn lowered_rectangle_matches_reference_semantics() {
    let plan = fixture_rectangle_plan(OperationMode::Line { kerf: Kerf::NONE });
    let program = lower(&plan, &RUIDA).unwrap();
    assert_eq!(program.plan(), plan.id(), "the RD program names the plan it came from");
    let ours = decoded(&program.to_rd_file(RUIDA.magic).unwrap());
    let reference = decoded(&fixture("rectangle-default.rd"));

    // Nothing unknown is generated (production output uses a validated whitelist).
    assert!(ours.commands().all(|c| !matches!(c, Command::Unknown { .. } | Command::Registered { .. })));

    // Speed 100 mm/s and laser-1 min/max 20 % (raw14 3277): same records as LightBurn.
    let speed = Command::Speed(burnburn_ruida::Speed::from_um_per_s(100_000).unwrap());
    let p =
        |bound| Command::LaserPower { laser: Laser::L1, bound, value: burnburn_ruida::Power::from_raw(3277).unwrap() };
    for c in [&speed, &p(PowerBound::Min), &p(PowerBound::Max), &Command::Mode(ModeFlag::LayerEnd)] {
        assert!(has(&reference, c), "fixture has {c:?}");
        assert!(has(&ours, c), "ours has {c:?}");
    }
    assert!(
        has(&ours, &Command::Mode(ModeFlag::AirAssistOn)) && has(&reference, &Command::Mode(ModeFlag::AirAssistOn))
    );
    // Byte equality where conditions are identical: the encoded speed and power records.
    let raw = |f: &RdFile, at: usize, n: usize| f.raw[at..at + n].to_vec();
    assert_eq!(encode(&[speed]).unwrap(), raw(&reference, 0x18d, 7));
    assert_eq!(encode(&[p(PowerBound::Min), p(PowerBound::Max)]).unwrap(), raw(&reference, 0x1aa, 8));

    // Moves: one absolute travel, four absolute cuts around the same 30 × 15 mm frame, in the
    // same (counter-clockwise) direction. Only the entry corner differs (explained in IM15.md).
    let (m_ours, m_ref) = (motion(&ours), motion(&reference));
    assert_eq!((m_ours.travels.len(), m_ref.travels.len()), (1, 1));
    assert_eq!((m_ours.cuts.len(), m_ref.cuts.len()), (4, 4));
    let mut a: Vec<_> = m_ours.cuts.iter().copied().map(undirected).collect();
    let mut b: Vec<_> = m_ref.cuts.iter().copied().map(undirected).collect();
    a.sort();
    b.sort();
    assert_eq!(a, b, "same four edges, identical µm corners");
    let rotation = (0..4).find(|r| (0..4).all(|k| m_ours.cuts[(k + r) % 4] == m_ref.cuts[k]));
    assert!(rotation.is_some(), "same directed cycle up to the start vertex: {:?} vs {:?}", m_ours.cuts, m_ref.cuts);
    let ref_cuts: Vec<Command> =
        reference.commands().filter(|c| matches!(c, Command::CutAbs { .. })).cloned().collect();
    for c in ours.commands().filter(|c| matches!(c, Command::CutAbs { .. })) {
        assert!(ref_cuts.contains(c), "each A8 record is byte-identical to one in the fixture: {c:?}");
    }
    let cut_mm: f64 = m_ours.cuts.iter().map(|(f, t)| (((t.0 - f.0).pow(2) + (t.1 - f.1).pow(2)) as f64).sqrt()).sum();
    assert_eq!(cut_mm / 1000.0, 90.0, "30 + 15 + 30 + 15 mm, as the fixture's DA 01 06 20 value");

    // Bounds: ours are the exact motion extrema; LightBurn's header uses a 10 µm grid
    // (771690 vs motion 771687). Agreement within half a grid step, never inward of the motion.
    let ((ox0, oy0), (ox1, oy1)) = bounds(&ours, BoundsKind::DocumentMin, BoundsKind::DocumentMax);
    let ((rx0, ry0), (rx1, ry1)) = bounds(&reference, BoundsKind::DocumentMin, BoundsKind::DocumentMax);
    assert_eq!(((ox0, oy0), (ox1, oy1)), ((771_687, 448_024), (801_687, 463_024)));
    for (o, r) in [(ox0, rx0), (oy0, ry0), (ox1, rx1), (oy1, ry1)] {
        assert!(o.abs_diff(r) <= 5, "{o} vs {r}");
    }
}

#[test]
fn lowered_fill_rectangle_matches_reference_semantics() {
    let plan = fixture_rectangle_plan(fill_mode());
    let program = lower(&plan, &RUIDA).unwrap();
    let ours = decoded(&program.to_rd_file(RUIDA.magic).unwrap());
    let reference = decoded(&fixture("rectangle-fill.rd"));

    // Native scan work mode in body and part metadata, as in the fixture.
    let work = Command::Mode(ModeFlag::WorkMode1);
    assert!(has(&ours, &work) && has(&reference, &work));
    let part_mode = |f: &RdFile| {
        f.commands().find_map(|c| match *c {
            Command::WorkModePart { mode, .. } => Some(mode.get()),
            _ => None,
        })
    };
    assert_eq!(part_mode(&ours), part_mode(&reference));
    assert_eq!(part_mode(&ours), Some(2));

    let (m_ours, m_ref) = (motion(&ours), motion(&reference));
    // 150 runs of 30 mm, 149 relative 0.1 mm row steps, one absolute approach.
    assert_eq!((m_ours.cuts.len(), m_ref.cuts.len()), (150, 150));
    assert_eq!((m_ours.relative_travel_dy.len(), m_ref.relative_travel_dy.len()), (149, 149));
    assert!(m_ours.relative_travel_dy.iter().chain(&m_ref.relative_travel_dy).all(|d| d.abs() == 100));
    let rows = |m: &Motion| {
        let mut r: Vec<(i64, i64, i64)> = m
            .cuts
            .iter()
            .map(|((x0, y0), (x1, y1))| {
                assert_eq!(y0, y1);
                (*y0, *x0.min(x1), *x0.max(x1))
            })
            .collect();
        r.sort();
        r
    };
    assert_eq!(rows(&m_ours), rows(&m_ref), "identical row centres (µm) and x extents");
    // Bidirectional: consecutive runs alternate direction in both files.
    for m in [&m_ours, &m_ref] {
        for w in m.cuts.windows(2) {
            assert_ne!(w[0].1.0 > w[0].0.0, w[1].1.0 > w[1].0.0, "zig-zag");
        }
    }
    let cut_mm: i64 = m_ours.cuts.iter().map(|((x0, _), (x1, _))| (x1 - x0).abs()).sum();
    assert_eq!(cut_mm, 4_500_000, "4500 mm, as the app preview and the DA 01 06 20 value");
    // Preview of the same plan agrees and names the same plan.
    let pv = preview(&plan, &research_profile());
    assert_eq!(pv.plan, program.plan(), "preview and export carry the same plan hash");
    assert!((pv.scan_length_mm - 4500.0).abs() < 1e-6, "{}", pv.scan_length_mm);

    let ((ox0, oy0), (ox1, oy1)) = bounds(&ours, BoundsKind::DocumentMin, BoundsKind::DocumentMax);
    let ((rx0, ry0), (rx1, ry1)) = bounds(&reference, BoundsKind::DocumentMin, BoundsKind::DocumentMax);
    for (o, r) in [(ox0, rx0), (oy0, ry0), (ox1, rx1), (oy1, ry1)] {
        assert!(o.abs_diff(r) <= 5, "{o} vs {r}");
    }
}

#[test]
fn negative_coordinates_and_unsupported_scan_axis_are_rejected_not_clamped() {
    // Origin top-right with a document placed past the bed edge gives negative machine x.
    let plan = fixture_rectangle_plan(OperationMode::Line { kerf: Kerf::NONE });
    let mirrored = {
        let p = MachineProfile::new(
            "small",
            1,
            MachineFrame::new(500.0, 500.0, OriginCorner::TopRight).unwrap(),
            1000.0,
            400.0,
            PowerRatio::new(0.0).unwrap(),
            PowerRatio::new(1.0).unwrap(),
        )
        .unwrap();
        let _ = &plan;
        let mut s = DocumentSession::new(DocumentId::from_raw(1));
        let rect = Rect::new(Point::new(771.687, 448.024), mm(30.0), mm(15.0)).unwrap();
        let cmd = |s: &DocumentSession, payload| DocumentCommand {
            document: s.id(),
            expected: s.revision(),
            request: RequestId::from_raw(1),
            payload,
        };
        let g =
            s.apply(cmd(&s, CommandPayload::CreateRectangle { rect, name: String::new() })).unwrap().changes.created[0];
        let draft = OperationDraft {
            name: "C00".into(),
            mode: OperationMode::Line { kerf: Kerf::NONE },
            settings: ProcessSettings {
                speed: DocSpeed::mm_per_s(100.0).unwrap(),
                power: PowerRange::new(PowerPercent::new(20.0).unwrap(), PowerPercent::new(20.0).unwrap()).unwrap(),
                passes: Passes::new(1).unwrap(),
                air_assist: true,
            },
            output_enabled: true,
        };
        s.apply(cmd(&s, CommandPayload::Process(ProcessCommand::AddOperation { draft, targets: vec![g] }))).unwrap();
        let options = CompileOptions {
            tolerance: ProductionTolerance::new(mm(0.01)).unwrap(),
            placement: Placement::Absolute,
            start: StartPoint::MachineOrigin,
        };
        compile(&s.snapshot(), &p, &options, &CancellationToken::never()).unwrap()
    };
    assert!(matches!(lower(&mirrored, &RUIDA), Err(LowerError::NegativeCoordinate { .. })));

    let vertical =
        OperationMode::Fill(FillSettings::new(mm(0.1), ScanAxis::Y, ScanDirection::Bidirectional, mm(0.0)).unwrap());
    let plan = fixture_rectangle_plan(vertical);
    assert!(matches!(lower(&plan, &RUIDA), Err(LowerError::Unsupported { what: "scan axis Y", .. })));

    let overscan =
        OperationMode::Fill(FillSettings::new(mm(0.1), ScanAxis::X, ScanDirection::Bidirectional, mm(2.0)).unwrap());
    let plan = fixture_rectangle_plan(overscan);
    let strict = RuidaProfile { overscan: OverscanPolicy::Reject, ..RUIDA };
    assert!(matches!(lower(&plan, &strict), Err(LowerError::Unsupported { what: "overscan", .. })));
    assert!(lower(&plan, &RUIDA).is_ok(), "controller-managed overscan is left to the DSP");
    let _ = AbsCoord::MAX_UM;
}

/// Burning segments the preview draws, in µm (round half away from zero, as the controller
/// field), for comparison with the decoded RD motion.
fn preview_cuts_um(pv: &burnburn_job::PlanPreview) -> Vec<((i64, i64), (i64, i64))> {
    let um = |v: f64| (v * 1000.0).round() as i64;
    pv.moves
        .iter()
        .filter(|m| matches!(m.kind, burnburn_job::MoveKind::Cut | burnburn_job::MoveKind::Scan))
        .map(|m| ((um(m.from.x_mm()), um(m.from.y_mm())), (um(m.to.x_mm()), um(m.to.y_mm()))))
        .collect()
}

#[test]
fn preview_and_export_carry_the_same_plan_hash() {
    // The preview and the RD file must not only name the same plan id: the burning motion the
    // RD bytes encode (decoded independently) must be exactly what the preview of that plan
    // draws, for Line and Fill.
    for mode in [OperationMode::Line { kerf: Kerf::NONE }, fill_mode()] {
        let plan = std::sync::Arc::new(fixture_rectangle_plan(mode));
        let pv = preview(&plan, &research_profile());
        let program = lower(&plan, &RUIDA).unwrap();
        assert_eq!(pv.plan, plan.id());
        assert_eq!(program.plan(), plan.id(), "preview, RD export (and later upload) name one frozen plan");
        let file = decoded(&program.to_rd_file(Magic::FIXTURE_0X88).unwrap());
        assert_eq!(motion(&file).cuts, preview_cuts_um(&pv), "RD motion = previewed motion");
    }
    // A different plan's preview does not match this export.
    let line = fixture_rectangle_plan(OperationMode::Line { kerf: Kerf::NONE });
    let other = fixture_rectangle_plan(fill_mode());
    assert_ne!(other.id(), line.id());
    let file = decoded(&lower(&line, &RUIDA).unwrap().to_rd_file(Magic::FIXTURE_0X88).unwrap());
    assert_ne!(motion(&file).cuts, preview_cuts_um(&preview(&other, &research_profile())));
}

/// The fixture rectangle with `mode` and `advanced`, compiled for `profile` with `sources`.
fn compiled_with(
    mode: OperationMode,
    advanced: burnburn_document::process::AdvancedOptions,
    profile: &MachineProfile,
    sources: &Vec<burnburn_image::SourceImage>,
) -> JobPlan {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let apply = |s: &mut DocumentSession, payload| {
        let cmd =
            DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload };
        s.apply(cmd).unwrap()
    };
    let rect = Rect::new(Point::new(771.687, 448.024), mm(30.0), mm(15.0)).unwrap();
    let g = apply(&mut s, CommandPayload::CreateRectangle { rect, name: String::new() }).changes.created[0];
    let p20 = PowerPercent::new(20.0).unwrap();
    let draft = OperationDraft {
        name: "C00".into(),
        mode,
        settings: ProcessSettings {
            speed: DocSpeed::mm_per_s(100.0).unwrap(),
            power: PowerRange::new(p20, PowerPercent::new(60.0).unwrap()).unwrap(),
            passes: Passes::new(2).unwrap(),
            air_assist: true,
        },
        output_enabled: true,
    };
    let op = apply(&mut s, CommandPayload::Process(ProcessCommand::AddOperation { draft, targets: vec![g] }))
        .changes
        .operations[0];
    if !advanced.is_default() {
        apply(&mut s, CommandPayload::Process(ProcessCommand::SetAdvanced { operation: op, advanced }));
    }
    let options = CompileOptions {
        tolerance: ProductionTolerance::new(mm(0.001)).unwrap(),
        placement: Placement::Absolute,
        start: StartPoint::MachineOrigin,
    };
    burnburn_cam::compile_with(&s.snapshot(), profile, &options, sources, &CancellationToken::never()).unwrap()
}

fn claiming(features: &[burnburn_job::Feature]) -> MachineProfile {
    use burnburn_job::{MachineCapabilities, Support};
    let caps = features.iter().fold(MachineCapabilities::conservative(), |c, f| c.with(*f, Support::Supported));
    research_profile().with_capabilities(caps).unwrap()
}

#[test]
fn unproven_plan_features_are_rejected_by_rd_lowering_not_replaced() {
    use burnburn_document::process::{AdvancedOptions, ImageRender, ImageSettings, ZStep};
    use burnburn_job::Feature;
    // Even a profile that claims grayscale, scan angles and Z gets no RD for them: the RD
    // records for modulation (C7/C2), diagonal scans and Z are not established.
    let gray = burnburn_image::SourceImage::from_gray8(2, 1, vec![0, 128]).unwrap();
    let image = ImageSettings::new(gray.content_hash(), mm(0.1), mm(0.1), ImageRender::Grayscale).unwrap();
    let p = claiming(&[Feature::GrayscaleModulation, Feature::ScanAngle, Feature::ZStep]);
    let plan = compiled_with(OperationMode::Image(image), AdvancedOptions::default(), &p, &vec![gray]);
    assert!(matches!(lower(&plan, &RUIDA), Err(LowerError::Unsupported { what: "grayscale modulation", .. })));

    let angled = OperationMode::Fill(
        FillSettings::new(mm(0.1), ScanAxis::X, ScanDirection::Bidirectional, mm(0.0))
            .unwrap()
            .with_angle(30.0)
            .unwrap(),
    );
    let plan = compiled_with(angled, AdvancedOptions::default(), &p, &vec![]);
    assert!(matches!(lower(&plan, &RUIDA), Err(LowerError::Unsupported { what: "scan angle", .. })));

    let z = AdvancedOptions { z_step: Some(ZStep::new(mm(1.0)).unwrap()), ..Default::default() };
    let plan = compiled_with(OperationMode::Line { kerf: Kerf::NONE }, z, &p, &vec![]);
    assert!(matches!(lower(&plan, &RUIDA), Err(LowerError::Unsupported { what: "focus offset", .. })));
}

#[test]
fn dithered_image_lowers_to_scan_records_without_modulation() {
    use burnburn_document::process::{AdvancedOptions, DitherMode, ImageRender, ImageSettings};
    // A 4 × 1 black-white pattern, threshold dither: binary runs at the operation power only.
    let src = burnburn_image::SourceImage::from_gray8(4, 1, vec![0, 255, 0, 255]).unwrap();
    let render = ImageRender::Dither(DitherMode::Threshold { level: 128 });
    let image = ImageSettings::new(src.content_hash(), mm(0.1), mm(0.1), render).unwrap();
    let plan = compiled_with(OperationMode::Image(image), AdvancedOptions::default(), &research_profile(), &vec![src]);
    let program = lower(&plan, &RUIDA).unwrap();
    let body = program.body();
    assert!(body.iter().any(|c| matches!(c, Command::CutRelX(_))), "scan runs as relative X cuts");
    assert!(!body.iter().any(|c| matches!(c, Command::ImmediatePower { .. })), "no grayscale power records");
    assert!(body.contains(&Command::Mode(ModeFlag::WorkMode1)), "scan block like the Fill fixture");
}
