//! W1-L3 process options: tab spacing, lead style/angle, dot mode and the fill extras are
//! validated at construction, change the canonical project hash, and leave every operation
//! without them with its earlier hash.

#![allow(clippy::unwrap_used)]

use burnburn_document::process::{
    AdvancedOptions, DotMode, FillGrouping, FillSettings, Kerf, Lead, LeadStyle, LineOptions, Operation,
    OperationDraft, OperationMode, Overcut, Passes, PowerPercent, PowerRange, ProcessCommand, ProcessError,
    ProcessSettings, ScanAxis, ScanDirection, Speed, TabPlacement, TabSpec,
};
use burnburn_document::{CommandError, CommandPayload, DocumentCommand, DocumentSession};
use burnburn_geometry::{Point, Rect};
use burnburn_types::{
    CanonicalEncode, CanonicalWriter, DocumentId, GeometryId, Length, OperationId, RequestId, StableId,
};

fn mm(v: f64) -> Length {
    Length::from_mm(v).unwrap()
}

fn settings() -> ProcessSettings {
    let p = PowerPercent::new(20.0).unwrap();
    ProcessSettings {
        speed: Speed::mm_per_s(100.0).unwrap(),
        power: PowerRange::new(p, p).unwrap(),
        passes: Passes::new(1).unwrap(),
        air_assist: true,
    }
}

fn operation(mode: OperationMode, advanced: AdvancedOptions) -> Operation {
    let draft = OperationDraft { name: "Op".into(), mode, settings: settings(), output_enabled: true };
    Operation::new(OperationId::from_raw(5), draft, vec![GeometryId::from_raw(1)], None)
        .unwrap()
        .with_advanced(advanced)
        .unwrap()
}

fn bytes(op: &Operation) -> Vec<u8> {
    let mut w = CanonicalWriter::new();
    op.encode(&mut w);
    w.finish().unwrap()
}

fn line(options: LineOptions) -> Operation {
    operation(OperationMode::Line { kerf: Kerf::NONE }, AdvancedOptions { line: options, ..Default::default() })
}

fn fill(f: FillSettings) -> Operation {
    operation(OperationMode::Fill(f), AdvancedOptions::default())
}

fn base_fill() -> FillSettings {
    FillSettings::new(mm(0.1), ScanAxis::X, ScanDirection::Bidirectional, mm(2.0)).unwrap()
}

#[test]
fn tab_spacing_is_validated_and_is_a_distinct_placement() {
    assert!(TabSpec::spaced(mm(0.0), mm(1.0)).is_err(), "zero spacing");
    assert!(TabSpec::spaced(mm(10.0), mm(0.0)).is_err(), "zero width");
    assert_eq!(
        TabSpec::spaced(mm(1.0), mm(1.0)),
        Err(ProcessError::OutOfRange { field: "tab spacing" }),
        "tabs as wide as their spacing leave nothing to cut"
    );
    let spaced = TabSpec::spaced(mm(25.0), mm(2.0)).unwrap();
    assert_eq!(spaced.placement(), TabPlacement::Spacing(25.0));
    assert_eq!(TabSpec::new(3, mm(2.0)).unwrap().placement(), TabPlacement::Count(3));
    let by_count = line(LineOptions { tabs: Some(TabSpec::new(3, mm(2.0)).unwrap()), ..Default::default() });
    let by_spacing = line(LineOptions { tabs: Some(spaced), ..Default::default() });
    let other_spacing =
        line(LineOptions { tabs: Some(TabSpec::spaced(mm(26.0), mm(2.0)).unwrap()), ..Default::default() });
    assert_ne!(bytes(&by_count), bytes(&by_spacing));
    assert_ne!(bytes(&by_spacing), bytes(&other_spacing), "the spacing is content");
    assert_ne!(bytes(&by_spacing), bytes(&line(LineOptions::default())));
}

#[test]
fn lead_style_and_angle_are_validated_and_content() {
    let lead = Lead::new(mm(2.0)).unwrap();
    assert_eq!((lead.style(), lead.angle_deg()), (LeadStyle::Line, 90.0), "perpendicular line by default");
    for bad in [0.0, -10.0, 90.5, 180.0, f64::NAN, f64::INFINITY] {
        assert_eq!(lead.with_angle(bad), Err(ProcessError::OutOfRange { field: "lead angle" }), "{bad}");
    }
    let tangential = lead.with_angle(30.0).unwrap();
    assert_eq!(tangential.angle_deg(), 30.0);
    let of = |l: Lead| line(LineOptions { lead_in: Some(l), ..Default::default() });
    let plain = bytes(&of(lead));
    assert_ne!(plain, bytes(&of(tangential)));
    assert_ne!(plain, bytes(&of(lead.with_style(LeadStyle::Arc))));
    // A default lead keeps the hash it had when a lead was only a length.
    let mut w = CanonicalWriter::new();
    of(lead).encode(&mut w);
    let mut legacy = CanonicalWriter::new();
    line(LineOptions { lead_in: Some(Lead::new(mm(2.0)).unwrap()), ..Default::default() }).encode(&mut legacy);
    assert_eq!(w.finish().unwrap(), legacy.finish().unwrap());
}

#[test]
fn dot_mode_is_validated_and_excludes_the_continuous_line_options() {
    assert!(DotMode::new(mm(0.0), 1.0).is_err());
    for bad in [0.0, -1.0, f64::NAN, 10_000.5] {
        assert_eq!(DotMode::new(mm(1.0), bad), Err(ProcessError::OutOfRange { field: "dot dwell" }), "{bad}");
    }
    let dot = DotMode::new(mm(1.5), 2.0).unwrap();
    assert_eq!((dot.spacing_mm(), dot.dwell_ms()), (1.5, 2.0));

    let draft = OperationDraft {
        name: "Punten".into(),
        mode: OperationMode::Line { kerf: Kerf::NONE },
        settings: settings(),
        output_enabled: true,
    };
    let op = Operation::new(OperationId::from_raw(5), draft, vec![GeometryId::from_raw(1)], None).unwrap();
    let with = |line: LineOptions| op.with_advanced(AdvancedOptions { line, ..Default::default() });
    assert!(with(LineOptions { dot: Some(dot), ..Default::default() }).is_ok());
    let tabs = Some(TabSpec::new(2, mm(1.0)).unwrap());
    let conflicts = [
        ("dot mode", "tabs", LineOptions { dot: Some(dot), tabs, ..Default::default() }),
        (
            "dot mode",
            "lead-in",
            LineOptions { dot: Some(dot), lead_in: Some(Lead::new(mm(1.0)).unwrap()), ..Default::default() },
        ),
        (
            "dot mode",
            "overcut",
            LineOptions { dot: Some(dot), overcut: Some(Overcut::new(mm(1.0)).unwrap()), ..Default::default() },
        ),
    ];
    for (first, second, options) in conflicts {
        assert_eq!(with(options).unwrap_err(), ProcessError::ConflictingOptions { first, second });
    }
    assert_ne!(
        bytes(&line(LineOptions { dot: Some(dot), ..Default::default() })),
        bytes(&line(LineOptions { dot: Some(DotMode::new(mm(1.5), 3.0).unwrap()), ..Default::default() }))
    );
    assert_ne!(
        bytes(&line(LineOptions { dot: Some(dot), ..Default::default() })),
        bytes(&line(LineOptions::default()))
    );
}

#[test]
fn fill_extras_default_off_are_validated_and_change_the_hash() {
    let f = base_fill();
    assert!(!f.cross_hatch() && !f.flood_fill() && !f.has_extras());
    assert_eq!((f.grouping(), f.ramp_mm()), (FillGrouping::AllShapes, 0.0));
    assert_eq!(f.with_ramp(mm(-0.5)), Err(ProcessError::OutOfRange { field: "ramp length" }));
    assert_eq!(f.with_ramp(mm(0.0)).unwrap(), f, "a zero ramp is no ramp");

    let variants = [
        f.with_cross_hatch(true),
        f.with_flood_fill(true),
        f.with_grouping(FillGrouping::Groups),
        f.with_grouping(FillGrouping::Individual),
        f.with_ramp(mm(1.0)).unwrap(),
        f.with_ramp(mm(2.0)).unwrap(),
    ];
    let plain = bytes(&fill(f));
    let mut seen = vec![plain.clone()];
    for v in variants {
        assert!(v.has_extras());
        let b = bytes(&fill(v));
        assert!(!seen.contains(&b), "{v:?} must have its own canonical form");
        seen.push(b);
    }
    // Extras do not make a rotated fill collide with an unrotated one either.
    assert_ne!(bytes(&fill(f.with_angle(30.0).unwrap().with_flood_fill(true))), bytes(&fill(f.with_flood_fill(true))));
}

#[test]
fn fills_without_extras_keep_their_earlier_canonical_layout() {
    let op = fill(base_fill());
    // Layout of IM13 written out by hand: id, name, mode (tag, interval, axis, direction,
    // overscan), settings, targets, output, recipe marker.
    let mut w = CanonicalWriter::new();
    w.u64(5);
    w.str("Op");
    w.tag("fill");
    w.f64(0.1);
    w.tag("x");
    w.tag("bidirectional");
    w.f64(2.0);
    for v in [100.0, 20.0, 20.0] {
        w.f64(v);
    }
    w.u64(1);
    w.bool(true);
    w.u64(1);
    w.u64(1);
    w.bool(true);
    w.u64(0);
    assert_eq!(bytes(&op), w.finish().unwrap());
}

#[test]
fn advanced_options_with_the_new_fields_are_one_validated_transaction() {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let rect = Rect::new(Point::new(0.0, 0.0), mm(10.0), mm(10.0)).unwrap();
    let apply = |s: &mut DocumentSession, payload| {
        let cmd =
            DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload };
        s.apply(cmd)
    };
    let g = apply(&mut s, CommandPayload::CreateRectangle { rect, name: String::new() }).unwrap().changes.created[0];
    let draft = OperationDraft {
        name: "Snijden".into(),
        mode: OperationMode::Line { kerf: Kerf::NONE },
        settings: settings(),
        output_enabled: true,
    };
    let op = apply(&mut s, CommandPayload::Process(ProcessCommand::AddOperation { draft, targets: vec![g] }))
        .unwrap()
        .changes
        .operations[0];
    let dot = DotMode::new(mm(1.0), 1.0).unwrap();
    let conflicting = AdvancedOptions {
        line: LineOptions { dot: Some(dot), tabs: Some(TabSpec::new(2, mm(1.0)).unwrap()), ..Default::default() },
        ..Default::default()
    };
    let before = s.revision();
    let err =
        apply(&mut s, CommandPayload::Process(ProcessCommand::SetAdvanced { operation: op, advanced: conflicting }))
            .unwrap_err();
    assert_eq!(err, CommandError::Process(ProcessError::ConflictingOptions { first: "dot mode", second: "tabs" }));
    assert_eq!(s.revision(), before, "a rejected combination changes nothing");
}
