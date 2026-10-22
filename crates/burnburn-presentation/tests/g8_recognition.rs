//! G8-cam-werk, W06: recognition of a turned and/or scaled product, learned templates that outlive the
//! session, and pointing at the box of a template in the camera picture. Fake camera, rendered frames
//! with analytic ground truth (the frame is made by evaluating the product at the position turned back
//! by the angle and divided by the scale), the real controller and task port. The operator still
//! decides about every candidate, and the position in millimetres still comes from the calibration only.

#![allow(clippy::unwrap_used, clippy::panic, clippy::expect_used)] // test helpers

mod common;
#[path = "common/camera.rs"]
mod rig;

use std::path::{Path as FsPath, PathBuf};
use std::sync::Arc;

use burnburn_editor::{Modifiers, Overlay};
use burnburn_geometry::ProductionTolerance;
use burnburn_image::locate::{LocateError, LocateMatch, LocateParams};
use burnburn_image::{GrayImage, SourceImage};
use burnburn_io::fs::FileSystem;
use burnburn_io::memory::MemoryFileSystem;
use burnburn_presentation::{
    CandidateDecision as D, CommandKey, HeadlessEditorHarness, RecognitionIntent as R, RecognitionProblem as P,
    RecognitionViewModel, SelectMode, TemplateLocator, UiIntent,
};
use burnburn_types::{CancellationToken, DocumentId, GeometryId, Length};
use rig::*;

const A: &str = "cam-A";
const W: u32 = 160;
const H: u32 = 120;
const MAP: Mapping = Mapping { mm_per_px: 0.5, origin_mm: [100.0, 300.0] };
const FILE: &str = "/lib/recognition-templates.json";

fn speckle(x: i64, y: i64) -> f64 {
    let mut h = (x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (y as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    h ^= h >> 29;
    h = h.wrapping_mul(0x94D0_49BB_1331_11EB);
    h ^= h >> 32;
    (h % 1000) as f64 / 1000.0 * 6.0 - 3.0
}

/// The product at `(u, v)` from its centre in its own axes: a bright blob, a dark tail, a satellite.
fn product(u: f64, v: f64) -> f64 {
    90.0 * (-(u * u + v * v) / 50.0).exp() - 40.0 * (-((u + 7.0).powi(2) + (v - 4.0).powi(2)) / 18.0).exp()
        + 55.0 * (-((u - 8.0).powi(2) + (v + 6.0).powi(2)) / 8.0).exp()
}

/// A product centred on pixel index `(x, y)`, turned by `angle` degrees (y down) and `scale` times as large.
#[derive(Clone, Copy)]
struct Posed(f64, f64, f64, f64);

fn upright(x: f64, y: f64) -> Posed {
    Posed(x, y, 0.0, 1.0)
}

fn photo(at: &[Posed]) -> SourceImage {
    let mut rgba = Vec::with_capacity((W * H * 4) as usize);
    for y in 0..H {
        for x in 0..W {
            let mut value = 128.0 + speckle(i64::from(x), i64::from(y));
            for Posed(cx, cy, angle, scale) in at {
                let (dx, dy) = (f64::from(x) - cx, f64::from(y) - cy);
                let a = angle.to_radians();
                let (u, v) = ((a.cos() * dx + a.sin() * dy) / scale, (-a.sin() * dx + a.cos() * dy) / scale);
                if u.abs() <= 24.0 && v.abs() <= 24.0 {
                    value += product(u, v);
                }
            }
            let g = value.clamp(0.0, 255.0).round() as u8;
            rgba.extend_from_slice(&[g, g, g, 255]);
        }
    }
    SourceImage::from_rgba8(W, H, rgba).unwrap()
}

struct Rig {
    h: HeadlessEditorHarness,
    doc: DocumentId,
    cam: Arc<FakeCam>,
}

fn rig_on(fs: Option<&Arc<MemoryFileSystem>>) -> Rig {
    let cam = FakeCam::new(&[(A, &[mode(W, H), mode(80, 60)])]);
    cam.authorize();
    let mut h = match fs {
        Some(fs) => {
            let mut h = HeadlessEditorHarness::with_fs(Arc::clone(fs));
            h.fs().create_dir_all_for_test(FsPath::new("/lib"));
            h.controller().set_library_dir(Some(PathBuf::from("/lib")));
            h
        }
        None => HeadlessEditorHarness::new(),
    };
    h.controller().attach_camera(cam.clone());
    let doc = h.new_document().unwrap();
    h.run_tasks();
    Rig { h, doc, cam }
}

fn rig() -> Rig {
    rig_on(None)
}

fn view(r: &Rig) -> RecognitionViewModel {
    r.h.document(r.doc).unwrap().recognition
}

fn send(r: &mut Rig, intent: R) {
    r.h.send(UiIntent::Recognition(intent));
    r.h.run_tasks();
}

fn show(r: &mut Rig, at: &[Posed]) {
    r.cam.show(A, mode(W, H), photo(at));
    r.h.send(UiIntent::CameraWorkspace(burnburn_presentation::CameraWorkspaceIntent::UseCamera {
        device: dev(A),
        mode: mode(W, H),
        lens: "standard".into(),
    }));
}

fn learn_named(r: &mut Rig, name: &str) {
    send(
        r,
        R::Learn {
            device: dev(A),
            x: "46".into(),
            y: "46".into(),
            width: "29".into(),
            height: "29".into(),
            name: name.into(),
        },
    );
}

/// An upright product at (60, 60); the operator marks the 29 x 29 window around it.
fn learned(name: &str) -> Rig {
    let mut r = rig();
    show(&mut r, &[upright(60.0, 60.0)]);
    learn_named(&mut r, name);
    assert_eq!(view(&r).problem, None);
    r
}

fn find(r: &mut Rig, min_percent: &str) {
    send(r, R::Find { min_percent: min_percent.into() });
}

fn calibrate(r: &mut Rig) {
    let mut store = burnburn_image::calib::CalibrationStore::default();
    store.insert_alignment(aligned(A, mode(W, H), 0.0, MAP));
    r.h.controller().attach_calibrations(store);
}

fn angle_gap(a: f64, b: f64) -> f64 {
    let d = (a - b).rem_euclid(360.0);
    d.min(360.0 - d)
}

// --- rotation and scale ---------------------------------------------------------------------------------------

#[test]
fn a_turned_product_is_found_with_its_angle_only_when_rotation_is_switched_on() {
    let mut r = learned("Kaartje");
    show(&mut r, &[Posed(100.0, 70.0, 40.0, 1.0)]);
    assert_eq!(view(&r).search, Default::default(), "nothing but translation unless the operator asks");
    find(&mut r, "97");
    assert_eq!(view(&r).problem, Some(P::NothingFound), "upright search: a turned product is not alike enough");

    send(&mut r, R::SetSearch { rotation: true, scale: false });
    assert!(view(&r).search.rotation && !view(&r).search.scale);
    find(&mut r, "90");
    let v = view(&r);
    assert_eq!(v.problem, None);
    assert_eq!(v.candidates.len(), 1, "{:?}", v.candidates);
    let c = &v.candidates[0];
    assert!((c.x_px - 100.5).abs() < 0.75 && (c.y_px - 70.5).abs() < 0.75, "{c:?}");
    assert!(angle_gap(c.angle_deg, 40.0) < 3.0, "{c:?}");
    assert!((c.scale - 1.0).abs() < 1e-9, "{c:?}");
    assert!(c.similarity_percent > 95.0, "{c:?}");
    assert_eq!(c.decision, D::Pending, "a turned match waits for the operator like any other");
}

#[test]
fn a_larger_product_is_found_with_its_scale_when_scale_is_switched_on() {
    let mut r = learned("Kaartje");
    show(&mut r, &[Posed(100.0, 70.0, 0.0, 1.25)]);
    send(&mut r, R::SetSearch { rotation: false, scale: true });
    find(&mut r, "90");
    let v = view(&r);
    assert_eq!(v.candidates.len(), 1, "{:?} {:?}", v.candidates, v.problem);
    let c = &v.candidates[0];
    assert!((c.scale - 1.25).abs() < 0.05 && c.angle_deg == 0.0, "{c:?}");
    assert!((c.x_px - 100.5).abs() < 1.0 && (c.y_px - 70.5).abs() < 1.0, "{c:?}");
}

#[test]
fn a_search_without_rotation_or_scale_reports_angle_zero_and_scale_one() {
    let mut r = learned("Kaartje");
    show(&mut r, &[upright(100.0, 70.0)]);
    find(&mut r, "80");
    let c = &view(&r).candidates[0];
    assert_eq!((c.angle_deg, c.scale), (0.0, 1.0));
}

#[derive(Debug)]
struct Canned(Vec<LocateMatch>);

impl TemplateLocator for Canned {
    fn locate(
        &self,
        _frame: &GrayImage,
        _template: &GrayImage,
        _params: &LocateParams,
        _cancel: &CancellationToken,
    ) -> Result<Vec<LocateMatch>, LocateError> {
        Ok(self.0.clone())
    }
}

#[test]
fn an_algorithm_that_knows_only_translation_still_works_behind_the_port_with_rotation_on() {
    let mut r = learned("Kaartje");
    r.h.controller().attach_locator(Arc::new(Canned(vec![LocateMatch { x_px: 40.0, y_px: 30.0, score: 0.93 }])));
    send(&mut r, R::SetSearch { rotation: true, scale: true });
    show(&mut r, &[]);
    find(&mut r, "");
    let v = view(&r);
    assert_eq!(v.candidates.len(), 1);
    assert_eq!((v.candidates[0].angle_deg, v.candidates[0].scale), (0.0, 1.0));
}

// --- placing turned and scaled copies ------------------------------------------------------------------------

/// A 10 x 4 rectangle (the cut geometry of one product), selected: corners (10,10) (20,10) (20,14) (10,14).
fn select_product_geometry(r: &mut Rig) -> GeometryId {
    r.h.invoke(r.doc, CommandKey::ToolRectangle);
    r.h.drag(r.doc, (10.0, 10.0), (20.0, 14.0));
    r.h.invoke(r.doc, CommandKey::ToolSelect);
    let id = r.h.document(r.doc).unwrap().objects.last().unwrap().id;
    r.h.send(UiIntent::Select { document: r.doc, ids: vec![id], mode: SelectMode::Replace });
    id
}

/// The corners of every object except the original, as lists of points.
fn corners_of_copies(r: &Rig, original: GeometryId) -> Vec<Vec<[f64; 2]>> {
    let tolerance = ProductionTolerance::new(Length::from_mm(0.01).unwrap()).unwrap();
    r.h.scene(r.doc)
        .unwrap()
        .items
        .iter()
        .filter(|i| i.source != original)
        .map(|i| {
            let mut points: Vec<[f64; 2]> = i.path.flatten(&tolerance)[0].points.iter().map(|p| [p.x, p.y]).collect();
            if points.len() > 1 && points.first() == points.last() {
                points.pop();
            }
            points
        })
        .collect()
}

/// Where the corners of the original go: turned by `theta_deg` (y up, counter-clockwise), `scale` times
/// as large, about the centre (15, 12), the centre put on `target`.
fn expected_corners(target: [f64; 2], theta_deg: f64, scale: f64) -> Vec<[f64; 2]> {
    let t = theta_deg.to_radians();
    [[10.0, 10.0], [20.0, 10.0], [20.0, 14.0], [10.0, 14.0]]
        .iter()
        .map(|c| {
            let (x, y) = ((c[0] - 15.0) * scale, (c[1] - 12.0) * scale);
            [target[0] + x * t.cos() - y * t.sin(), target[1] + x * t.sin() + y * t.cos()]
        })
        .collect()
}

fn same_corners(found: &[[f64; 2]], want: &[[f64; 2]], tolerance: f64) -> bool {
    found.len() == want.len()
        && want.iter().all(|w| found.iter().any(|f| (f[0] - w[0]).hypot(f[1] - w[1]) <= tolerance))
}

#[test]
fn a_copy_is_turned_and_scaled_like_the_product_it_is_put_on() {
    // The bed is the picture mirrored in y: a product turned clockwise in the picture is turned
    // counter-clockwise on the bed by the same angle.
    for (angle, scale) in [(90.0, 1.0), (40.0, 1.25)] {
        let mut r = learned("Kaartje");
        calibrate(&mut r);
        let original = select_product_geometry(&mut r);
        send(&mut r, R::SetSearch { rotation: true, scale: scale != 1.0 });
        show(&mut r, &[Posed(100.0, 70.0, angle, scale)]);
        find(&mut r, "85");
        assert_eq!(view(&r).candidates.len(), 1, "{angle}: {:?}", view(&r));
        send(&mut r, R::Decide { index: 0, decision: D::Accepted });
        let doc = r.doc;
    send(&mut r, R::Place { document: doc });
        assert_eq!(view(&r).problem, None);
        let target = MAP.bed([100.5, 70.5]);
        let copies = corners_of_copies(&r, original);
        assert_eq!(copies.len(), 1, "one copy for the one accepted place");
        let want = expected_corners(target, -angle, scale);
        assert!(same_corners(&copies[0], &want, 1.0), "{angle}/{scale}: {:?} vs {want:?}", copies[0]);
        // The other way round is a different place (a rectangle looks the same turned by 90 degrees
        // either way, so only the 40 degree case can tell the sign).
        if angle == 40.0 {
            let wrong = expected_corners(target, angle, scale);
            assert!(!same_corners(&copies[0], &wrong, 1.0), "the sign of the angle");
        }
    }
}

#[test]
fn an_upright_candidate_is_still_placed_by_translation_only() {
    let mut r = learned("Kaartje");
    calibrate(&mut r);
    let original = select_product_geometry(&mut r);
    show(&mut r, &[upright(100.0, 70.0)]);
    find(&mut r, "80");
    send(&mut r, R::Decide { index: 0, decision: D::Accepted });
    let doc = r.doc;
    send(&mut r, R::Place { document: doc });
    let copies = corners_of_copies(&r, original);
    let want = expected_corners(MAP.bed([100.5, 70.5]), 0.0, 1.0);
    assert!(same_corners(&copies[0], &want, 0.6), "{:?} vs {want:?}", copies[0]);
}

// --- templates that outlive the session -----------------------------------------------------------------------

fn disk() -> Arc<MemoryFileSystem> {
    Arc::new(MemoryFileSystem::new())
}

#[test]
fn a_named_template_is_kept_and_is_back_after_a_restart_where_it_finds_the_product() {
    let fs = disk();
    let mut first = rig_on(Some(&fs));
    show(&mut first, &[upright(60.0, 60.0)]);
    learn_named(&mut first, "Kaartje");
    assert!(view(&first).template.as_ref().unwrap().saved);
    assert_eq!(view(&first).saved.len(), 1);
    assert!(fs.exists(FsPath::new(FILE)).unwrap(), "next to the other libraries");
    drop(first);

    let mut second = rig_on(Some(&fs));
    let v = view(&second);
    assert!(v.template.is_none(), "nothing is in use until the operator picks it");
    assert_eq!(v.saved.len(), 1);
    let saved = &v.saved[0];
    assert_eq!((saved.name.as_str(), saved.width_px, saved.height_px), ("Kaartje", 29, 29));
    assert!(!saved.in_use);
    let id = saved.id;
    show(&mut second, &[Posed(100.0, 70.0, 0.0, 1.0)]);
    send(&mut second, R::Use { id });
    assert_eq!(view(&second).problem, None);
    assert!(view(&second).saved[0].in_use);
    assert_eq!(view(&second).template.as_ref().unwrap().name, "Kaartje");
    find(&mut second, "80");
    let c = &view(&second).candidates;
    assert_eq!(c.len(), 1, "{:?}", view(&second).problem);
    assert!((c[0].x_px - 100.5).abs() < 0.75 && (c[0].y_px - 70.5).abs() < 0.75, "{:?}", c[0]);
}

#[test]
fn a_restored_template_finds_the_same_as_the_template_just_learned() {
    let fs = disk();
    let mut first = rig_on(Some(&fs));
    show(&mut first, &[upright(60.0, 60.0)]);
    learn_named(&mut first, "Kaartje");
    show(&mut first, &[Posed(100.0, 70.0, 0.0, 1.0), upright(40.0, 90.0)]);
    find(&mut first, "80");
    let fresh: Vec<(f64, f64, f64)> = view(&first).candidates.iter().map(|c| (c.x_px, c.y_px, c.similarity_percent)).collect();
    drop(first);
    let mut second = rig_on(Some(&fs));
    show(&mut second, &[Posed(100.0, 70.0, 0.0, 1.0), upright(40.0, 90.0)]);
    let id = view(&second).saved[0].id;
    send(&mut second, R::Use { id });
    find(&mut second, "80");
    let back: Vec<(f64, f64, f64)> = view(&second).candidates.iter().map(|c| (c.x_px, c.y_px, c.similarity_percent)).collect();
    assert_eq!(fresh.len(), 2);
    assert_eq!(fresh, back);
}

#[test]
fn a_template_without_a_name_is_used_but_not_kept() {
    let fs = disk();
    let mut r = rig_on(Some(&fs));
    show(&mut r, &[upright(60.0, 60.0)]);
    learn_named(&mut r, "  ");
    let v = view(&r);
    assert!(v.template.is_some() && !v.template.as_ref().unwrap().saved);
    assert!(v.saved.is_empty());
    assert!(!fs.exists(FsPath::new(FILE)).unwrap_or(false), "nothing to write");
}

#[test]
fn teaching_a_name_again_replaces_that_template_and_delete_removes_it_for_good() {
    let fs = disk();
    let mut r = rig_on(Some(&fs));
    show(&mut r, &[upright(60.0, 60.0)]);
    learn_named(&mut r, "Kaartje");
    let id = view(&r).saved[0].id;
    learn_named(&mut r, "kaartje");
    assert_eq!(view(&r).saved.len(), 1, "same name, same template");
    assert_eq!(view(&r).saved[0].id, id);
    learn_named(&mut r, "Ander");
    assert_eq!(view(&r).saved.len(), 2);

    send(&mut r, R::Delete { id });
    assert_eq!(view(&r).saved.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), vec!["Ander"]);
    send(&mut r, R::Use { id });
    assert_eq!(view(&r).problem, Some(P::UnknownTemplate));
    drop(r);
    let second = rig_on(Some(&fs));
    assert_eq!(view(&second).saved.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), vec!["Ander"]);
}

#[test]
fn a_saved_template_is_only_searched_in_pictures_of_the_camera_configuration_it_was_learned_in() {
    let fs = disk();
    let mut first = rig_on(Some(&fs));
    show(&mut first, &[upright(60.0, 60.0)]);
    learn_named(&mut first, "Kaartje");
    drop(first);
    let mut second = rig_on(Some(&fs));
    let id = view(&second).saved[0].id;
    // The same camera in another resolution.
    second.cam.show(A, mode(80, 60), grey(80, 60, 128));
    second.h.send(UiIntent::CameraWorkspace(burnburn_presentation::CameraWorkspaceIntent::UseCamera {
        device: dev(A),
        mode: mode(80, 60),
        lens: "standard".into(),
    }));
    send(&mut second, R::Use { id });
    find(&mut second, "80");
    assert_eq!(view(&second).problem, Some(P::FrameConfigChanged));
}

#[test]
fn a_library_file_that_cannot_be_read_is_left_alone_and_the_template_is_still_used() {
    for damaged in [&b"not json"[..], br#"{"format":"burnburn-recognition-templates","schema":2,"templates":[]}"#] {
        let fs = disk();
        fs.create_dir_all_for_test(FsPath::new("/lib"));
        fs.insert(FsPath::new(FILE), damaged.to_vec());
        let mut r = rig_on(Some(&fs));
        assert!(view(&r).library_unreadable);
        show(&mut r, &[upright(60.0, 60.0)]);
        learn_named(&mut r, "Kaartje");
        let v = view(&r);
        assert!(v.template.is_some() && !v.template.as_ref().unwrap().saved, "usable now, not kept");
        assert!(v.saved.is_empty());
        assert_eq!(fs.read(FsPath::new(FILE), 1 << 20).unwrap(), damaged, "the file is never replaced");
    }
}

// --- pointing at the box -----------------------------------------------------------------------------------------

/// Pixel (x, y) of the camera picture on the sheet: 0.1 mm per pixel, y up from the bottom left.
fn on_sheet(x: f64, y: f64) -> (f64, f64) {
    (x * 0.1, (f64::from(H) - y) * 0.1)
}

#[test]
fn pointing_at_two_corners_in_the_picture_on_the_sheet_learns_the_template() {
    let mut r = rig();
    show(&mut r, &[upright(60.0, 60.0)]);
    send(&mut r, R::PointBox { device: dev(A), name: "Gewezen".into() });
    let p = view(&r).pointing.expect("pointing");
    assert_eq!((p.first, p.frame_px), (None, (W, H)));
    let overlays = r.h.scene(r.doc).unwrap().overlays.clone();
    let Some(Overlay::BedImage { picture, quads, .. }) = overlays.iter().find(|o| matches!(o, Overlay::BedImage { .. }))
    else {
        panic!("the camera picture to point at lies on the sheet");
    };
    assert_eq!((picture.width, picture.height), (W, H));
    let (min, max) = (quads[0].bed_mm[3], quads[0].bed_mm[1]);
    assert!((min.x - 0.0).abs() < 1e-9 && (min.y - 0.0).abs() < 1e-9, "{min:?}");
    assert!((max.x - 16.0).abs() < 1e-9 && (max.y - 12.0).abs() < 1e-9, "16 x 12 mm: {max:?}");

    // The same box as typing 46, 46, 29 x 29 — corners clicked in any order.
    r.h.click(r.doc, on_sheet(75.0, 75.0), Modifiers::default());
    assert_eq!(view(&r).pointing.unwrap().first, Some([75.0, 75.0]));
    assert!(view(&r).template.is_none(), "one corner is not a box");
    r.h.click(r.doc, on_sheet(46.0, 46.0), Modifiers::default());
    r.h.run_tasks();
    let v = view(&r);
    assert!(v.pointing.is_none(), "done");
    let t = v.template.expect("learned");
    assert_eq!((t.name.as_str(), t.width_px, t.height_px), ("Gewezen", 29, 29));
    assert!(!r.h.scene(r.doc).unwrap().overlays.iter().any(|o| matches!(o, Overlay::BedImage { .. })));

    // It finds the product like a typed template.
    show(&mut r, &[upright(100.0, 70.0)]);
    find(&mut r, "80");
    let c = &view(&r).candidates[0];
    assert!((c.x_px - 100.5).abs() < 0.75 && (c.y_px - 70.5).abs() < 0.75, "{c:?}");
}

#[test]
fn a_drag_across_the_picture_marks_the_box_too() {
    let mut r = rig();
    show(&mut r, &[upright(60.0, 60.0)]);
    send(&mut r, R::PointBox { device: dev(A), name: "Gesleept".into() });
    r.h.drag(r.doc, on_sheet(46.0, 46.0), on_sheet(75.0, 75.0));
    let t = view(&r).template.expect("learned by a drag");
    assert_eq!((t.width_px, t.height_px), (29, 29));
}

#[test]
fn pointing_is_stopped_by_escape_or_end_and_a_bad_box_says_why() {
    let mut r = rig();
    show(&mut r, &[upright(60.0, 60.0)]);
    send(&mut r, R::PointBox { device: dev(A), name: "x".into() });
    r.h.click(r.doc, on_sheet(46.0, 46.0), Modifiers::default());
    r.h.escape(r.doc);
    assert!(view(&r).pointing.is_none() && view(&r).template.is_none());
    assert!(!r.h.scene(r.doc).unwrap().overlays.iter().any(|o| matches!(o, Overlay::BedImage { .. })));

    send(&mut r, R::PointBox { device: dev(A), name: "x".into() });
    send(&mut r, R::EndPoint);
    assert!(view(&r).pointing.is_none());

    // A box of 2 x 2 pixels is no pattern.
    send(&mut r, R::PointBox { device: dev(A), name: "x".into() });
    r.h.click(r.doc, on_sheet(50.0, 50.0), Modifiers::default());
    r.h.click(r.doc, on_sheet(52.0, 52.0), Modifiers::default());
    let v = view(&r);
    assert_eq!(v.problem, Some(P::TemplateTooSmall));
    assert!(v.pointing.is_none() && v.template.is_none());

    // A flat part of the picture has nothing to look for.
    send(&mut r, R::PointBox { device: dev(A), name: "x".into() });
    r.h.click(r.doc, on_sheet(5.0, 5.0), Modifiers::default());
    r.h.click(r.doc, on_sheet(30.0, 30.0), Modifiers::default());
    assert_eq!(view(&r).problem, Some(P::FlatTemplate));
}

#[test]
fn pointing_needs_a_running_camera_with_a_picture_and_eats_the_clicks_meanwhile() {
    let mut r = rig();
    send(&mut r, R::PointBox { device: dev(A), name: "x".into() });
    assert_eq!(view(&r).problem, Some(P::NoFrame));
    assert!(view(&r).pointing.is_none());

    show(&mut r, &[upright(60.0, 60.0)]);
    send(&mut r, R::PointBox { device: dev(A), name: "x".into() });
    let before = r.h.document(r.doc).unwrap().objects.len();
    r.h.invoke(r.doc, CommandKey::ToolRectangle);
    r.h.drag(r.doc, on_sheet(10.0, 10.0), on_sheet(20.0, 20.0));
    assert_eq!(r.h.document(r.doc).unwrap().objects.len(), before, "the pointer is the pointer in the picture");
}
