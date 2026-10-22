//! G5-grbl-ui: preflight names what the GRBL backend refuses (with reason and the object it
//! belongs to) and Export G-code writes exactly the plan the preview shows. A plan the backend
//! refuses writes no file; the document is never touched; a Ruida machine has no G-code export.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::path::PathBuf;

use burnburn_gcode::{GrblProfile, RefusalKind, grbl_capabilities};
use burnburn_io::fs::FileSystem;
use burnburn_presentation::{CommandKey, HeadlessEditorHarness, HostEffect, MessageKey, SelectMode, UiIntent};
use burnburn_types::{DocumentId, GeometryId};

use common::{RUIDA, profile};

fn grbl_machine(h: &mut HeadlessEditorHarness, grbl: &GrblProfile) {
    h.send(UiIntent::SetGrblMachineProfile {
        profile: Box::new(profile(1).with_capabilities(grbl_capabilities(grbl)).unwrap()),
        grbl: Box::new(grbl.clone()),
    });
}

/// A 40 x 20 mm rectangle on layer 1 (its default operation asks for air assist).
fn job(h: &mut HeadlessEditorHarness) -> (DocumentId, GeometryId) {
    let doc = h.new_document().expect("window");
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (10.0, 10.0), (50.0, 30.0));
    h.invoke(doc, CommandKey::ToolSelect);
    let id = h.document(doc).unwrap().objects[0].id;
    h.send(UiIntent::Select { document: doc, ids: vec![id], mode: SelectMode::Replace });
    h.send(UiIntent::AssignLayer { document: doc, slot: 1, add: false });
    (doc, id)
}

fn compile(h: &mut HeadlessEditorHarness, doc: DocumentId) {
    h.invoke(doc, CommandKey::Compile);
    h.run_tasks();
}

fn with_air() -> GrblProfile {
    GrblProfile { air_assist: true, ..GrblProfile::grbl_1_1_laser() }
}

#[test]
fn a_plan_the_grbl_backend_accepts_has_no_gcode_findings() {
    let mut h = HeadlessEditorHarness::new();
    let (doc, _) = job(&mut h);
    grbl_machine(&mut h, &with_air());
    compile(&mut h, doc);
    let plan = h.document(doc).unwrap().plan.expect("plan");
    assert!(plan.grbl.is_empty(), "{:?}", plan.grbl);
    assert_eq!(plan.errors, 0);
}

#[test]
fn air_assist_without_the_output_is_a_finding_with_its_reason_and_the_object() {
    let mut h = HeadlessEditorHarness::new();
    let (doc, object) = job(&mut h);
    grbl_machine(&mut h, &GrblProfile::grbl_1_1_laser());
    compile(&mut h, doc);
    let plan = h.document(doc).unwrap().plan.expect("plan");
    let finding = plan
        .grbl
        .iter()
        .find(|f| f.kind == RefusalKind::AirAssistUnavailable)
        .unwrap_or_else(|| panic!("air assist is named: {:?}", plan.grbl));
    assert_eq!(finding.geometry, Some(object), "a click goes to the object of the operation");
    assert!(finding.operation.is_some());
    assert_eq!(plan.errors, plan.grbl.len(), "a refusal is an error of the plan, like a Ruida refusal");
    assert!(plan.errors > 0);
}

#[test]
fn a_profile_without_laser_mode_is_refused_for_the_plan_as_a_whole() {
    let mut h = HeadlessEditorHarness::new();
    let (doc, _) = job(&mut h);
    grbl_machine(&mut h, &GrblProfile { laser_mode: false, ..with_air() });
    compile(&mut h, doc);
    let plan = h.document(doc).unwrap().plan.expect("plan");
    let finding = plan.grbl.iter().find(|f| f.kind == RefusalKind::LaserModeRequired).expect("laser mode is named");
    assert_eq!((finding.geometry, finding.operation), (None, None));
}

#[test]
fn a_ruida_machine_has_no_gcode_findings() {
    let mut h = HeadlessEditorHarness::new();
    let (doc, _) = job(&mut h);
    h.send(UiIntent::SetMachineProfile { profile: Box::new(profile(1)), ruida: RUIDA });
    compile(&mut h, doc);
    assert!(h.document(doc).unwrap().plan.expect("plan").grbl.is_empty());
}

#[test]
fn changing_the_grbl_profile_makes_the_findings_stale_with_the_plan() {
    let mut h = HeadlessEditorHarness::new();
    let (doc, _) = job(&mut h);
    grbl_machine(&mut h, &with_air());
    compile(&mut h, doc);
    grbl_machine(&mut h, &GrblProfile::grbl_1_1_laser());
    assert!(h.document(doc).unwrap().plan.expect("plan").stale);
}

fn export(h: &mut HeadlessEditorHarness, doc: DocumentId, path: &str) -> Vec<HostEffect> {
    let mut effects = h.send(UiIntent::ExportGcode { document: doc, path: PathBuf::from(path) }).effects;
    effects.extend(h.run_tasks().into_iter().flat_map(|u| u.effects));
    effects
}

/// Burned length of a G-code text: independent of the lowering (modal absolute X/Y, G1 with S > 0).
fn burned_mm(text: &str) -> f64 {
    let (mut x, mut y, mut s, mut g) = (0.0_f64, 0.0_f64, 0.0_f64, 0);
    let mut burned = 0.0;
    for line in text.lines().map(|l| l.split(';').next().unwrap_or("")) {
        let (mut nx, mut ny) = (x, y);
        for word in line.split_whitespace() {
            let (letter, rest) = word.split_at(1);
            match (letter, rest.parse::<f64>()) {
                ("G", Ok(n)) if n == 0.0 || n == 1.0 => g = n as i32,
                ("X", Ok(v)) => nx = v,
                ("Y", Ok(v)) => ny = v,
                ("S", Ok(v)) => s = v,
                _ => {}
            }
        }
        if (nx, ny) != (x, y) && g == 1 && s > 0.0 {
            burned += ((nx - x).powi(2) + (ny - y).powi(2)).sqrt();
        }
        (x, y) = (nx, ny);
    }
    burned
}

#[test]
fn export_gcode_writes_the_plan_the_preview_shows() {
    let mut h = HeadlessEditorHarness::new();
    let (doc, _) = job(&mut h);
    grbl_machine(&mut h, &with_air());
    compile(&mut h, doc);
    let plan = h.document(doc).unwrap().plan.expect("plan").id;
    let revision = h.document(doc).unwrap().revision;

    let effects = export(&mut h, doc, "/out/job.gcode");
    assert!(effects.iter().all(|e| !matches!(e, HostEffect::ShowError { .. })), "{effects:?}");
    let bytes = h.fs().read(&PathBuf::from("/out/job.gcode"), 1 << 20).expect("the file exists");
    let text = String::from_utf8(bytes.clone()).unwrap();
    assert!(text.starts_with(&format!("; BurnBurn {plan}")), "the header names the previewed plan: {}", &text[..40]);
    assert!((burned_mm(&text) - 120.0).abs() < 1e-6, "the perimeter of 40 x 20 is burned: {}", burned_mm(&text));
    assert!(text.trim_end().ends_with("M2"));
    let view = h.document(doc).unwrap();
    assert_eq!(view.revision, revision, "the document is untouched");
    let exported = view.gcode_export.expect("the export is reported");
    assert_eq!((exported.plan, exported.bytes, exported.path), (plan, bytes.len(), PathBuf::from("/out/job.gcode")));
}

#[test]
fn a_refused_plan_writes_no_file_and_says_so() {
    let mut h = HeadlessEditorHarness::new();
    let (doc, _) = job(&mut h);
    grbl_machine(&mut h, &GrblProfile::grbl_1_1_laser());
    compile(&mut h, doc);
    let effects = export(&mut h, doc, "/out/job.gcode");
    assert!(
        effects.iter().any(|e| matches!(e, HostEffect::ShowError { message: MessageKey::GcodeRefused, .. })),
        "{effects:?}"
    );
    assert!(h.fs().read(&PathBuf::from("/out/job.gcode"), 1 << 20).is_err(), "no file was written");
    assert!(h.document(doc).unwrap().gcode_export.is_none());
}

#[test]
fn the_command_asks_for_a_location_only_for_a_compiled_grbl_plan() {
    let mut h = HeadlessEditorHarness::new();
    let (doc, _) = job(&mut h);
    // No machine yet, then a Ruida machine, then GRBL without a plan: the command is off.
    assert!(!h.view().commands.get(&CommandKey::ExportGcode).is_some_and(|c| c.enabled));
    h.send(UiIntent::SetMachineProfile { profile: Box::new(profile(1)), ruida: RUIDA });
    compile(&mut h, doc);
    let enabled = |h: &HeadlessEditorHarness, key| h.view().commands.get(&key).is_some_and(|c| c.enabled);
    assert!(enabled(&h, CommandKey::ExportRd));
    assert!(!enabled(&h, CommandKey::ExportGcode), "Ruida writes RD, not G-code");

    grbl_machine(&mut h, &with_air());
    assert!(!enabled(&h, CommandKey::ExportGcode), "the old plan is for another machine revision");
    compile(&mut h, doc);
    assert!(enabled(&h, CommandKey::ExportGcode));
    assert!(!enabled(&h, CommandKey::ExportRd), "a GRBL machine has no RD file");
    let effects = h.invoke(doc, CommandKey::ExportGcode).effects;
    assert!(
        effects.iter().any(|e| matches!(e,
            HostEffect::ShowGcodeExportPanel { document, suggested_name } if *document == doc && suggested_name.ends_with(".gcode"))),
        "{effects:?}"
    );
}

#[test]
fn export_gcode_on_a_ruida_machine_and_export_rd_on_a_grbl_machine_are_refused() {
    let mut h = HeadlessEditorHarness::new();
    let (doc, _) = job(&mut h);
    h.send(UiIntent::SetMachineProfile { profile: Box::new(profile(1)), ruida: RUIDA });
    compile(&mut h, doc);
    let effects = export(&mut h, doc, "/out/a.gcode");
    assert!(effects.iter().any(|e| matches!(e, HostEffect::ShowError { .. })), "{effects:?}");
    assert!(h.fs().read(&PathBuf::from("/out/a.gcode"), 1 << 20).is_err());

    grbl_machine(&mut h, &with_air());
    compile(&mut h, doc);
    let mut effects = h.send(UiIntent::ExportRd { document: doc, path: PathBuf::from("/out/a.rd") }).effects;
    effects.extend(h.run_tasks().into_iter().flat_map(|u| u.effects));
    assert!(h.fs().read(&PathBuf::from("/out/a.rd"), 1 << 20).is_err(), "no RD file for a GRBL machine: {effects:?}");
    assert!(
        effects.iter().any(|e| matches!(e, HostEffect::ShowError { message: MessageKey::RdNotForGrbl, .. })),
        "{effects:?}"
    );
}
