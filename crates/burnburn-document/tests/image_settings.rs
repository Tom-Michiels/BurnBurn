//! W1-L5a: the image-operation options added after IM18 (negative, dot width correction, scan
//! angle, colour-to-gray, crop, sharpen, screen, mask) and the new dither modes.

#![allow(clippy::unwrap_used)]

use burnburn_document::process::{
    DitherMode, GrayMethod, ImageCrop, ImageRender, ImageSettings, ImageSharpen, OperationDraft, OperationMode,
    ProcessCommand, ProcessError, ScreenSettings,
};
use burnburn_document::{CommandPayload, DocumentCommand, DocumentSession};
use burnburn_types::{
    CanonicalEncode, CanonicalWriter, ContentHash, DocumentId, HashDomain, Length, RequestId, StableId,
};

fn mm(v: f64) -> Length {
    Length::from_mm(v).unwrap()
}

fn base() -> ImageSettings {
    let hash = ContentHash::of(HashDomain::SourcePixels, b"pixels");
    ImageSettings::new(hash, mm(0.1), mm(0.2), ImageRender::Dither(DitherMode::FloydSteinberg)).unwrap()
}

#[test]
fn new_options_default_to_the_behaviour_of_im18() {
    let s = base();
    assert!(!s.negative() && !s.mask());
    assert_eq!((s.dot_width_correction_mm(), s.scan_angle_deg()), (0.0, 0.0));
    assert_eq!((s.gray_method(), s.crop(), s.sharpen()), (GrayMethod::Rec709, None, None));
    assert_eq!((s.screen().cell_mm(), s.screen().angle_deg()), (0.5, 45.0));
    assert!(!s.has_extended_options());
}

#[test]
fn every_new_option_counts_as_an_extended_option() {
    let crop = ImageCrop { x: 1, y: 2, width: 3, height: 4 };
    let variants = [
        base().with_negative(true),
        base().with_dot_width_correction(mm(0.02)).unwrap(),
        base().with_scan_angle(30.0).unwrap(),
        base().with_gray_method(GrayMethod::Red),
        base().with_crop(Some(crop)).unwrap(),
        base().with_sharpen(Some(ImageSharpen::new(1.0, mm(0.3)).unwrap())),
        base().with_screen(ScreenSettings::new(mm(1.0), 30.0).unwrap()),
        base().with_mask(true),
    ];
    for (i, v) in variants.iter().enumerate() {
        assert!(v.has_extended_options(), "variant {i}");
    }
    // Setting an option back to its default is not an extension.
    assert!(!base().with_negative(true).with_negative(false).has_extended_options());
    assert!(!base().with_scan_angle(180.0).unwrap().has_extended_options(), "180° is 0°");
}

#[test]
fn option_values_are_validated_at_the_boundary() {
    let out_of_range = |field| Err::<ImageSettings, _>(ProcessError::OutOfRange { field });
    let crop = |width, height| Some(ImageCrop { x: 0, y: 0, width, height });
    assert_eq!(base().with_dot_width_correction(mm(-0.01)), out_of_range("dot width correction"));
    assert_eq!(base().with_scan_angle(f64::NAN), out_of_range("scan angle"));
    assert_eq!(base().with_scan_angle(f64::INFINITY), out_of_range("scan angle"));
    assert_eq!(base().with_crop(crop(0, 5)), out_of_range("crop"));
    assert_eq!(base().with_crop(crop(5, 0)), out_of_range("crop"));
    let sharpen = |amount, radius| ImageSharpen::new(amount, mm(radius)).unwrap_err();
    assert_eq!(sharpen(0.0, 0.2), ProcessError::OutOfRange { field: "sharpen amount" });
    assert_eq!(sharpen(10.5, 0.2), ProcessError::OutOfRange { field: "sharpen amount" });
    assert_eq!(sharpen(f64::NAN, 0.2), ProcessError::OutOfRange { field: "sharpen amount" });
    assert_eq!(sharpen(1.0, 0.0), ProcessError::OutOfRange { field: "sharpen radius" });
    assert_eq!(ScreenSettings::new(mm(0.0), 45.0).unwrap_err(), ProcessError::OutOfRange { field: "screen cell" });
    assert_eq!(ScreenSettings::new(mm(1.0), f64::NAN).unwrap_err(), ProcessError::OutOfRange { field: "screen angle" });
}

#[test]
fn angles_are_normalised_to_a_half_turn() {
    assert_eq!(base().with_scan_angle(190.0).unwrap().scan_angle_deg(), 10.0);
    assert_eq!(base().with_scan_angle(-10.0).unwrap().scan_angle_deg(), 170.0);
    assert_eq!(base().with_scan_angle(180.0).unwrap().scan_angle_deg(), 0.0);
    assert_eq!(ScreenSettings::new(mm(1.0), -30.0).unwrap().angle_deg(), 150.0);
}

fn session_with(settings: ImageSettings) -> (DocumentSession, burnburn_types::OperationId) {
    use burnburn_document::process::{Passes, PowerPercent, PowerRange, ProcessSettings, Speed};
    use burnburn_geometry::{Point, Rect};
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let mut run = |payload| {
        let cmd =
            DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload };
        s.apply(cmd).unwrap()
    };
    let rect = Rect::new(Point::new(0.0, 0.0), mm(10.0), mm(10.0)).unwrap();
    let g = run(CommandPayload::CreateRectangle { rect, name: String::new() }).changes.created[0];
    let p = PowerPercent::new(20.0).unwrap();
    let draft = OperationDraft {
        name: "Beeld".into(),
        mode: OperationMode::Image(settings),
        settings: ProcessSettings {
            speed: Speed::mm_per_s(100.0).unwrap(),
            power: PowerRange::new(p, p).unwrap(),
            passes: Passes::new(1).unwrap(),
            air_assist: true,
        },
        output_enabled: true,
    };
    let op =
        run(CommandPayload::Process(ProcessCommand::AddOperation { draft, targets: vec![g] })).changes.operations[0];
    (s, op)
}

fn encoded(settings: ImageSettings) -> Vec<u8> {
    let (s, op) = session_with(settings);
    let mut w = CanonicalWriter::new();
    s.snapshot().process().operation(op).unwrap().encode(&mut w);
    w.finish().unwrap()
}

#[test]
fn image_operations_without_new_options_keep_their_im18_canonical_layout() {
    let s = base();
    let (session, op) = session_with(s);
    let mut w = CanonicalWriter::new();
    session.snapshot().process().operation(op).unwrap().encode(&mut w);
    // IM18 layout, written out by hand up to the targets; the image mode body is:
    let mut mode = CanonicalWriter::new();
    mode.tag("image");
    mode.bytes(s.source().as_bytes());
    mode.f64(0.1);
    mode.f64(0.2);
    mode.tag("floyd-steinberg");
    for v in [1.0, 0.0, 1.0] {
        mode.f64(v);
    }
    mode.tag("bidirectional");
    mode.bool(false);
    mode.f64(0.0);
    let body = mode.finish().unwrap();
    let all = w.finish().unwrap();
    assert!(
        all.windows(body.len()).any(|win| win == &body[..]),
        "the image block of an unextended operation is unchanged"
    );
}

#[test]
fn each_new_option_and_mode_changes_the_canonical_form() {
    let crop = ImageCrop { x: 1, y: 2, width: 3, height: 4 };
    let plain = encoded(base());
    let variants: Vec<(&str, ImageSettings)> = vec![
        ("negative", base().with_negative(true)),
        ("dwc", base().with_dot_width_correction(mm(0.02)).unwrap()),
        ("dwc2", base().with_dot_width_correction(mm(0.03)).unwrap()),
        ("angle", base().with_scan_angle(30.0).unwrap()),
        ("gray", base().with_gray_method(GrayMethod::Red)),
        ("gray2", base().with_gray_method(GrayMethod::Blue)),
        ("crop", base().with_crop(Some(crop)).unwrap()),
        ("crop2", base().with_crop(Some(ImageCrop { x: 1, y: 2, width: 3, height: 5 })).unwrap()),
        ("sharpen", base().with_sharpen(Some(ImageSharpen::new(1.0, mm(0.3)).unwrap()))),
        ("sharpen2", base().with_sharpen(Some(ImageSharpen::new(2.0, mm(0.3)).unwrap()))),
        ("screen", base().with_screen(ScreenSettings::new(mm(1.0), 30.0).unwrap())),
        ("screen2", base().with_screen(ScreenSettings::new(mm(1.0), 31.0).unwrap())),
        ("mask", base().with_mask(true)),
    ];
    let mut seen = vec![plain.clone()];
    for (name, v) in variants {
        let e = encoded(v);
        assert!(!seen.contains(&e), "{name} must hash differently from every other variant");
        seen.push(e);
    }
    let modes = [
        DitherMode::Newsprint,
        DitherMode::Halftone,
        DitherMode::Sketch { level: 100 },
        DitherMode::Sketch { level: 101 },
        DitherMode::PassThrough,
    ];
    for m in modes {
        let mut s = base();
        s = ImageSettings::new(s.source(), mm(0.1), mm(0.2), ImageRender::Dither(m)).unwrap();
        let e = encoded(s);
        assert!(!seen.contains(&e), "{m:?}");
        seen.push(e);
    }
}
