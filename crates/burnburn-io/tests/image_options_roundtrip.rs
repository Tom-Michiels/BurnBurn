//! W1-L5a: the image options added after IM18 and the new dither modes persist additively in
//! schema 2. A project that sets them must not lose them on save; one that does not keeps its
//! earlier file form.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use burnburn_document::process::{
    DitherMode, GrayMethod, ImageCrop, ImageRender, ImageSettings, ImageSharpen, Kerf, OperationDraft, OperationMode,
    Passes, PowerPercent, PowerRange, ProcessCommand, ProcessSettings, ScreenSettings, Speed,
};
use burnburn_document::{CommandPayload, DocumentCommand, DocumentSession};
use burnburn_geometry::{Point, Rect};
use burnburn_io::{LoadError, Project, ProjectUuid, decode_container, encode_container, project_hash};
use burnburn_types::{ContentHash, DocumentId, Length, RequestId, StableId};

fn mm(v: f64) -> Length {
    Length::from_mm(v).unwrap()
}

fn base(render: ImageRender) -> ImageSettings {
    ImageSettings::new(ContentHash::from_bytes([4; 32]), mm(0.1), mm(0.08), render).unwrap()
}

fn everything() -> ImageSettings {
    base(ImageRender::Dither(DitherMode::FloydSteinberg))
        .with_negative(true)
        .with_dot_width_correction(mm(0.02))
        .unwrap()
        .with_scan_angle(37.5)
        .unwrap()
        .with_gray_method(GrayMethod::Lightness)
        .with_crop(Some(ImageCrop { x: 3, y: 4, width: 50, height: 60 }))
        .unwrap()
        .with_sharpen(Some(ImageSharpen::new(1.5, mm(0.3)).unwrap()))
        .with_screen(ScreenSettings::new(mm(0.8), 15.0).unwrap())
        .with_mask(true)
}

fn project_with(modes: Vec<OperationMode>) -> Project {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let apply = |s: &mut DocumentSession, payload| {
        let cmd =
            DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload };
        s.apply(cmd).unwrap()
    };
    let rect = Rect::new(Point::new(0.0, 0.0), mm(40.0), mm(30.0)).unwrap();
    let g = apply(&mut s, CommandPayload::CreateRectangle { rect, name: "Plaat".into() }).changes.created[0];
    let p = PowerPercent::new(30.0).unwrap();
    for (i, mode) in modes.into_iter().enumerate() {
        let settings = ProcessSettings {
            speed: Speed::mm_per_s(20.0).unwrap(),
            power: PowerRange::new(p, p).unwrap(),
            passes: Passes::new(1).unwrap(),
            air_assist: true,
        };
        let draft = OperationDraft { name: format!("op {i}"), mode, settings, output_enabled: true };
        apply(&mut s, CommandPayload::Process(ProcessCommand::AddOperation { draft, targets: vec![g] }));
    }
    Project::new(ProjectUuid::from_bytes([3; 16]), (*s.snapshot()).clone())
}

fn document_json(p: &Project) -> String {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(encode_container(p).unwrap())).unwrap();
    let mut json = String::new();
    std::io::Read::read_to_string(&mut zip.by_name("document.json").unwrap(), &mut json).unwrap();
    json
}

#[test]
fn every_image_option_survives_save_and_reopen_with_the_same_hash() {
    let p = project_with(vec![OperationMode::Image(everything())]);
    let back = decode_container(&encode_container(&p).unwrap()).unwrap();
    assert_eq!(back.document.process(), p.document.process(), "options must not be dropped by a save");
    assert_eq!(project_hash(&back).unwrap(), project_hash(&p).unwrap());
    assert_eq!(encode_container(&back).unwrap(), encode_container(&p).unwrap(), "deterministic file form");
}

#[test]
fn every_new_dither_mode_survives_save_and_reopen() {
    let modes =
        [DitherMode::Newsprint, DitherMode::Halftone, DitherMode::Sketch { level: 77 }, DitherMode::PassThrough];
    let p = project_with(modes.iter().map(|m| OperationMode::Image(base(ImageRender::Dither(*m)))).collect());
    let back = decode_container(&encode_container(&p).unwrap()).unwrap();
    assert_eq!(back.document.process(), p.document.process());
}

#[test]
fn image_operations_without_new_options_keep_their_file_form() {
    let plain = project_with(vec![
        OperationMode::Line { kerf: Kerf::NONE },
        OperationMode::Image(base(ImageRender::Dither(DitherMode::FloydSteinberg))),
    ]);
    let json = document_json(&plain);
    for absent in ["negative", "dot_width", "scan_angle", "gray", "crop", "sharpen", "screen", "mask", "options"] {
        assert!(!json.contains(absent), "{absent} must be omitted when unused:\n{json}");
    }
}

#[test]
fn damaged_image_options_are_refused_with_their_path() {
    let bytes = encode_container(&project_with(vec![OperationMode::Image(everything())])).unwrap();
    let cases: [(&str, fn(&mut serde_json::Value)); 4] = [
        ("options.crop", |v| v["operations"][0]["process"]["mode"]["options"]["crop"]["width"] = 0.into()),
        ("options.sharpen", |v| v["operations"][0]["process"]["mode"]["options"]["sharpen"]["amount"] = 99.0.into()),
        ("options.screen", |v| {
            v["operations"][0]["process"]["mode"]["options"]["screen"]["cell_mm"] = serde_json::Value::Null
        }),
        ("options.scan_angle_deg", |v| {
            v["operations"][0]["process"]["mode"]["options"]["scan_angle_deg"] = serde_json::Value::Null
        }),
    ];
    for (needle, edit) in cases {
        let broken = common::rewrite_json(&bytes, "document.json", edit);
        match decode_container(&broken) {
            Err(LoadError::InvalidValue { path, .. }) => assert!(path.contains(needle), "{needle}: {path}"),
            other => panic!("{needle}: expected InvalidValue, got {other:?}"),
        }
    }
}
