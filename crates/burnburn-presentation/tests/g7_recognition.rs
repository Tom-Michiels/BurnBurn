//! G7-werk, W06: recognising products in the camera picture. The operator learns a template from a
//! photo, looks for it in a later frame, decides about every candidate and has copies of the cut
//! geometry (the selection) put at the accepted places. Fake camera, rendered frames with analytic
//! ground truth, the real controller and task port. Nothing is accepted automatically, a similarity
//! is not a distance, and the position in millimetres comes from the camera calibration only.

#![allow(clippy::unwrap_used, clippy::panic, clippy::expect_used)] // test helpers

mod common;
#[path = "common/camera.rs"]
mod rig;

use std::sync::Arc;

use burnburn_image::locate::{LocateError, LocateMatch, LocateParams};
use burnburn_image::{GrayImage, SourceImage};
use burnburn_presentation::{
    CandidateDecision as D, CommandKey, HeadlessEditorHarness, RecognitionIntent as R, RecognitionPhase,
    RecognitionProblem as P, RecognitionViewModel, SelectMode, TemplateLocator, UiIntent,
};
use burnburn_types::{CancellationToken, DocumentId, GeometryId};
use rig::*;

const A: &str = "cam-A";
const W: u32 = 160;
const H: u32 = 120;
const MAP: Mapping = Mapping { mm_per_px: 0.5, origin_mm: [100.0, 300.0] };

fn speckle(x: i64, y: i64) -> f64 {
    let mut h = (x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (y as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    h ^= h >> 29;
    h = h.wrapping_mul(0x94D0_49BB_1331_11EB);
    h ^= h >> 32;
    (h % 1000) as f64 / 1000.0 * 6.0 - 3.0
}

/// The product around its centre: a bright blob, a dark tail, a small satellite (one orientation).
fn product(dx: f64, dy: f64) -> f64 {
    90.0 * (-(dx * dx + dy * dy) / 50.0).exp()
        - 40.0 * (-((dx + 7.0).powi(2) + (dy - 4.0).powi(2)) / 18.0).exp()
        + 55.0 * (-((dx - 8.0).powi(2) + (dy + 6.0).powi(2)) / 8.0).exp()
}

/// A camera frame with products centred on the pixel indices `at`.
fn photo(at: &[(f64, f64)]) -> SourceImage {
    let mut rgba = Vec::with_capacity((W * H * 4) as usize);
    for y in 0..H {
        for x in 0..W {
            let mut v = 128.0 + speckle(i64::from(x), i64::from(y));
            for (cx, cy) in at {
                let (dx, dy) = (f64::from(x) - cx, f64::from(y) - cy);
                if dx.abs() <= 15.0 && dy.abs() <= 15.0 {
                    v += product(dx, dy);
                }
            }
            let g = v.clamp(0.0, 255.0).round() as u8;
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

fn rig() -> Rig {
    let cam = FakeCam::new(&[(A, &[mode(W, H)])]);
    cam.authorize();
    let mut h = HeadlessEditorHarness::new();
    h.controller().attach_camera(cam.clone());
    let doc = h.new_document().unwrap();
    Rig { h, doc, cam }
}

fn view(r: &Rig) -> RecognitionViewModel {
    r.h.document(r.doc).unwrap().recognition
}

fn show(r: &mut Rig, at: &[(f64, f64)]) {
    r.cam.show(A, mode(W, H), photo(at));
    r.h.send(UiIntent::CameraWorkspace(burnburn_presentation::CameraWorkspaceIntent::UseCamera {
        device: dev(A),
        mode: mode(W, H),
        lens: "standard".into(),
    }));
}

fn learn(r: &mut Rig, rect: (u32, u32, u32, u32)) {
    r.h.send(UiIntent::Recognition(R::Learn {
        device: dev(A),
        x: rect.0.to_string(),
        y: rect.1.to_string(),
        width: rect.2.to_string(),
        height: rect.3.to_string(),
        name: "Kaartje".into(),
    }));
}

/// A photo of one product at (60, 60), the operator marks the 29 x 29 window around it.
fn learned() -> Rig {
    let mut r = rig();
    show(&mut r, &[(60.0, 60.0)]);
    learn(&mut r, (46, 46, 29, 29));
    assert_eq!(view(&r).problem, None);
    r
}

fn find(r: &mut Rig, min_percent: &str) {
    r.h.send(UiIntent::Recognition(R::Find { min_percent: min_percent.into() }));
    r.h.run_tasks();
}

fn calibrate(r: &mut Rig) {
    let mut store = burnburn_image::calib::CalibrationStore::default();
    store.insert_alignment(aligned(A, mode(W, H), 0.0, MAP));
    r.h.controller().attach_calibrations(store);
}

fn decide(r: &mut Rig, index: usize, decision: D) {
    r.h.send(UiIntent::Recognition(R::Decide { index, decision }));
}

#[test]
fn the_view_lists_the_cameras_that_run_so_a_template_can_be_learned_from_one() {
    let mut r = rig();
    assert!(view(&r).cameras.is_empty());
    show(&mut r, &[]);
    let cameras = view(&r).cameras;
    assert_eq!(cameras.iter().map(|c| (c.device.clone(), c.name.as_str())).collect::<Vec<_>>(), vec![(dev(A), "Camera cam-A")]);
}

// --- learning ----------------------------------------------------------------------------------------------

#[test]
fn learning_takes_the_marked_rectangle_of_the_latest_frame() {
    let r = learned();
    let t = view(&r).template.expect("a template");
    assert_eq!((t.name.as_str(), t.width_px, t.height_px, t.device.clone()), ("Kaartje", 29, 29, dev(A)));
    assert_eq!(view(&r).phase, RecognitionPhase::Idle);
    assert!(view(&r).candidates.is_empty());
}

#[test]
fn what_cannot_be_learned_is_said_and_keeps_the_old_template() {
    let mut r = learned();
    let before = view(&r).template;
    for (rect, problem) in [
        ((0, 0, 0, 10), P::OutsideFrame),
        ((150, 100, 29, 29), P::OutsideFrame),
        ((2, 2, 2, 2), P::TemplateTooSmall),
        ((120, 80, 20, 20), P::FlatTemplate),
    ] {
        learn(&mut r, rect);
        assert_eq!(view(&r).problem, Some(problem), "{rect:?}");
        assert_eq!(view(&r).template, before, "a refused learn changes nothing");
    }
    r.h.send(UiIntent::Recognition(R::Learn {
        device: dev(A),
        x: "veel".into(),
        y: "0".into(),
        width: "10".into(),
        height: "10".into(),
        name: String::new(),
    }));
    assert_eq!(view(&r).problem, Some(P::InvalidNumber));
    // No camera running: no frame.
    let mut idle = rig();
    learn(&mut idle, (46, 46, 29, 29));
    assert_eq!(view(&idle).problem, Some(P::NoFrame));
}

// --- finding and reviewing ---------------------------------------------------------------------------------

#[test]
fn a_search_lists_every_alike_copy_with_its_similarity_and_its_place_on_the_bed() {
    let mut r = learned();
    calibrate(&mut r);
    show(&mut r, &[(100.0, 70.0), (30.0, 95.0)]);
    r.h.send(UiIntent::Recognition(R::Find { min_percent: "80".into() }));
    assert_eq!(view(&r).phase, RecognitionPhase::Searching, "the search runs off the UI thread");
    r.h.run_tasks();

    let v = view(&r);
    assert_eq!(v.phase, RecognitionPhase::Found);
    assert_eq!(v.candidates.len(), 2, "{v:?}");
    assert!(v.needs_review, "two candidates: each needs a decision");
    for c in &v.candidates {
        assert!(c.similarity_percent > 95.0 && c.similarity_percent <= 100.0, "{c:?}");
        assert_eq!(c.decision, D::Pending, "nothing is accepted by the program");
        let bed = c.bed_mm.expect("calibrated");
        let want = MAP.bed([c.x_px, c.y_px]);
        assert!((bed[0] - want[0]).abs() < 1e-6 && (bed[1] - want[1]).abs() < 1e-6, "{bed:?} vs {want:?}");
    }
    let found: Vec<(f64, f64)> = v.candidates.iter().map(|c| (c.x_px, c.y_px)).collect();
    assert!(found.iter().any(|(x, y)| (x - 100.5).abs() < 0.1 && (y - 70.5).abs() < 0.1), "{found:?}");
    assert!(found.iter().any(|(x, y)| (x - 30.5).abs() < 0.1 && (y - 95.5).abs() < 0.1), "{found:?}");
    assert!(!v.can_place);
}

#[test]
fn even_a_single_clear_match_waits_for_the_operator() {
    let mut r = learned();
    calibrate(&mut r);
    show(&mut r, &[(100.0, 70.0)]);
    find(&mut r, "");
    let v = view(&r);
    assert_eq!(v.candidates.len(), 1);
    assert!(!v.needs_review);
    assert_eq!(v.candidates[0].decision, D::Pending);
    assert!(!v.can_place, "pending is not accepted");
    decide(&mut r, 0, D::Accepted);
    assert!(view(&r).can_place);
    decide(&mut r, 0, D::Pending);
    assert!(!view(&r).can_place, "a decision can be taken back");
}

#[test]
fn nothing_found_is_said_and_a_missing_template_or_a_changed_camera_stops_the_search() {
    let mut r = learned();
    show(&mut r, &[]);
    find(&mut r, "90");
    assert_eq!(view(&r).problem, Some(P::NothingFound));
    assert!(view(&r).candidates.is_empty());

    // The same camera started in another resolution is another configuration.
    let cam2 = FakeCam::new(&[(A, &[mode(W, H), mode(320, 240)])]);
    cam2.authorize();
    r.h.controller().attach_camera(cam2.clone());
    cam2.show(A, mode(320, 240), grey(320, 240, 128));
    r.h.send(UiIntent::CameraWorkspace(burnburn_presentation::CameraWorkspaceIntent::UseCamera {
        device: dev(A),
        mode: mode(320, 240),
        lens: "standard".into(),
    }));
    find(&mut r, "");
    assert_eq!(view(&r).problem, Some(P::FrameConfigChanged));

    let mut fresh = rig();
    show(&mut fresh, &[(100.0, 70.0)]);
    find(&mut fresh, "");
    assert_eq!(view(&fresh).problem, Some(P::NoTemplate));
    find(&mut r, "boven de 100");
    assert_eq!(view(&r).problem, Some(P::InvalidNumber));
}

// --- stale and cancelled results -------------------------------------------------------------------------

#[test]
fn a_result_for_a_search_that_was_replaced_cancelled_or_forgotten_is_dropped() {
    let mut r = learned();
    show(&mut r, &[(100.0, 70.0)]);

    // A newer search replaces the one still waiting.
    r.h.send(UiIntent::Recognition(R::Find { min_percent: "80".into() }));
    r.h.send(UiIntent::Recognition(R::Find { min_percent: "80".into() }));
    r.h.run_tasks();
    assert_eq!(view(&r).candidates.len(), 1, "one list, not two");

    // Cancelled: nothing arrives.
    r.h.send(UiIntent::Recognition(R::Forget));
    learn(&mut r, (46, 46, 29, 29));
    r.h.send(UiIntent::Recognition(R::Find { min_percent: "80".into() }));
    r.h.send(UiIntent::Recognition(R::CancelFind));
    assert_eq!(view(&r).phase, RecognitionPhase::Idle);
    r.h.run_tasks();
    assert!(view(&r).candidates.is_empty(), "a cancelled search leaves no candidates");

    // Forgotten while running: the template is gone, so is the result.
    r.h.send(UiIntent::Recognition(R::Find { min_percent: "80".into() }));
    r.h.send(UiIntent::Recognition(R::Forget));
    r.h.run_tasks();
    let v = view(&r);
    assert!(v.template.is_none() && v.candidates.is_empty() && v.phase == RecognitionPhase::Idle);
}

// --- the algorithm sits behind a port ----------------------------------------------------------------

#[derive(Debug)]
struct Canned(Vec<LocateMatch>);

impl TemplateLocator for Canned {
    fn locate(
        &self,
        _frame: &GrayImage,
        _template: &GrayImage,
        params: &LocateParams,
        _cancel: &CancellationToken,
    ) -> Result<Vec<LocateMatch>, LocateError> {
        assert!(params.min_score > 0.0);
        Ok(self.0.clone())
    }
}

#[test]
fn another_algorithm_can_stand_behind_the_port_and_its_matches_are_reviewed_the_same_way() {
    let mut r = learned();
    calibrate(&mut r);
    r.h.controller().attach_locator(Arc::new(Canned(vec![
        LocateMatch { x_px: 40.0, y_px: 30.0, score: 0.93 },
        LocateMatch { x_px: 120.0, y_px: 90.0, score: 0.71 },
    ])));
    show(&mut r, &[]);
    find(&mut r, "");
    let v = view(&r);
    assert_eq!(v.candidates.iter().map(|c| (c.x_px, c.y_px)).collect::<Vec<_>>(), vec![(40.0, 30.0), (120.0, 90.0)]);
    assert_eq!(v.candidates.iter().map(|c| c.similarity_percent).collect::<Vec<_>>(), vec![93.0, 71.0]);
    assert!(v.needs_review && v.candidates.iter().all(|c| c.decision == D::Pending));
    assert_eq!(v.note, burnburn_presentation::RecognitionNote::SimilarityIsNotAccuracy);
}

// --- placing the cut geometry --------------------------------------------------------------------------------

/// A 10 x 10 square (the cut geometry of one product), selected.
fn select_product_geometry(r: &mut Rig) -> GeometryId {
    r.h.invoke(r.doc, CommandKey::ToolRectangle);
    r.h.drag(r.doc, (10.0, 10.0), (20.0, 20.0));
    r.h.invoke(r.doc, CommandKey::ToolSelect);
    let id = r.h.document(r.doc).unwrap().objects.last().unwrap().id;
    r.h.send(UiIntent::Select { document: r.doc, ids: vec![id], mode: SelectMode::Replace });
    id
}

fn centres(r: &Rig) -> Vec<[f64; 2]> {
    r.h.scene(r.doc)
        .unwrap()
        .items
        .iter()
        .filter_map(|i| i.path.bounds())
        .map(|b| [(b.min().x + b.max().x) / 2.0, (b.min().y + b.max().y) / 2.0])
        .collect()
}

#[test]
fn only_accepted_candidates_get_a_copy_and_only_when_every_candidate_is_decided() {
    let mut r = learned();
    calibrate(&mut r);
    select_product_geometry(&mut r);
    show(&mut r, &[(100.0, 70.0), (30.0, 95.0)]);
    find(&mut r, "80");
    let candidates = view(&r).candidates;
    assert_eq!(candidates.len(), 2);

    // Accepting one while the other waits: nothing is placed.
    decide(&mut r, 0, D::Accepted);
    r.h.send(UiIntent::Recognition(R::Place { document: r.doc }));
    assert_eq!(view(&r).problem, Some(P::Undecided));
    assert_eq!(r.h.document(r.doc).unwrap().objects.len(), 1);

    // Rejecting the other makes the list complete.
    decide(&mut r, 1, D::Rejected);
    assert!(view(&r).can_place);
    r.h.send(UiIntent::Recognition(R::Place { document: r.doc }));
    assert_eq!(r.h.document(r.doc).unwrap().objects.len(), 2, "one copy for the one accepted place");
    let want = candidates[0].bed_mm.unwrap();
    let found = centres(&r);
    assert!(
        found.iter().any(|c| (c[0] - want[0]).abs() < 1e-6 && (c[1] - want[1]).abs() < 1e-6),
        "the copy is centred on the accepted place {want:?}: {found:?}"
    );
    // The original did not move.
    assert!(found.iter().any(|c| (c[0] - 15.0).abs() < 1e-6 && (c[1] - 15.0).abs() < 1e-6), "{found:?}");
    // The list is spent: placing twice would double the product.
    assert!(view(&r).candidates.is_empty() && view(&r).phase == RecognitionPhase::Idle);
}

#[test]
fn placing_needs_a_selection_an_acceptance_and_a_calibration() {
    let mut r = learned();
    show(&mut r, &[(100.0, 70.0)]);
    find(&mut r, "80");
    decide(&mut r, 0, D::Accepted);
    assert_eq!(view(&r).candidates[0].bed_mm, None, "no calibration: no millimetres");

    r.h.send(UiIntent::Recognition(R::Place { document: r.doc }));
    assert_eq!(view(&r).problem, Some(P::NothingSelected));
    select_product_geometry(&mut r);
    r.h.send(UiIntent::Recognition(R::Place { document: r.doc }));
    assert_eq!(view(&r).problem, Some(P::NotCalibrated), "pixels do not become millimetres by themselves");
    assert_eq!(r.h.document(r.doc).unwrap().objects.len(), 1);

    decide(&mut r, 0, D::Rejected);
    r.h.send(UiIntent::Recognition(R::Place { document: r.doc }));
    assert_eq!(view(&r).problem, Some(P::NothingAccepted));
    r.h.send(UiIntent::Recognition(R::Decide { index: 9, decision: D::Accepted }));
    assert_eq!(view(&r).problem, Some(P::UnknownCandidate));
}
