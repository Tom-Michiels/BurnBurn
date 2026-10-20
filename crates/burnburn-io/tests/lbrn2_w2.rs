//! W2-L6: more of the LightBurn (.lbrn2) file — values of unmapped layer settings, project-level
//! information, embedded bitmaps with their placement, image cut settings. Expectations are read
//! from the research fixtures' text or computed by hand; the image-mode name table is an
//! *assumption* of the importer and is asserted as such (listed in the proposal's assumptions).

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers and let-else in tests

use std::path::PathBuf;

use burnburn_document::process::{DitherMode, ImageRender, OperationMode};
use burnburn_io::import::{NoteKind, RasterFormat, parse_lbrn2};

fn research_fixture(name: &str) -> Vec<u8> {
    std::fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../research/lightburn/fixtures").join(name))
        .unwrap()
}

fn close(a: f64, b: f64, eps: f64) -> bool {
    (a - b).abs() <= eps
}

/// Independent base64 encoder (standard alphabet, padded) for building bitmap payloads.
fn base64(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |acc, (i, b)| acc | (u32::from(*b) << (16 - 8 * i)));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn png_2x2() -> Vec<u8> {
    let mut bytes = Vec::new();
    image::RgbaImage::from_pixel(2, 2, image::Rgba([10, 20, 30, 255]))
        .write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png)
        .unwrap();
    bytes
}

#[test]
fn unmapped_layer_settings_keep_the_values_they_had() {
    let draft = parse_lbrn2(&research_fixture("rectangle-fill.lbrn2")).unwrap();
    let unmapped: Vec<(&str, Option<&str>)> =
        draft.cut_settings[0].unmapped.iter().map(|u| (u.name.as_str(), u.value.as_deref())).collect();
    // The fixture's <CutSetting type="Scan"> children that are not mapped, with their Value attribute.
    for expected in [
        ("maxPower2", Some("20")),
        ("dotTime", Some("1")),
        ("priority", Some("0")),
        ("tabCount", Some("1")),
        ("tabCountMax", Some("1")),
    ] {
        assert!(unmapped.contains(&expected), "{expected:?} missing from {unmapped:?}");
    }
    assert!(!unmapped.iter().any(|(n, _)| *n == "speed"), "mapped settings are not repeated");
}

#[test]
fn project_information_is_carried_raw_and_still_reported_as_not_applied() {
    let draft = parse_lbrn2(&research_fixture("rectangle-fill.lbrn2")).unwrap();
    let p = &draft.report.project;
    assert_eq!(p.device_name.as_deref(), Some("Ruida"));
    assert_eq!((p.mirror_x, p.mirror_y), (Some(true), Some(true)));
    assert_eq!(p.material_height_mm, Some(0.0));
    let notes = p.notes.as_ref().unwrap();
    assert_eq!((notes.text.as_str(), notes.show_on_load), ("", false));
    let vt: Vec<(&str, &str)> = p.variable_text.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    assert_eq!(vt, [("Start", "0"), ("End", "999"), ("Current", "0"), ("Increment", "1"), ("AutoAdvance", "0")]);
    let opt = |k: &str| p.optimize.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
    assert_eq!(
        (opt("Optimize_ByPriority"), opt("Optimize_OverlapDist"), opt("Optimize_ByGroup")),
        (Some("1"), Some("0.025"), Some("-1"))
    );
    assert_eq!(p.optimize.len(), 14);
    // Carried is not applied: the elements stay in the report.
    for name in ["UIPrefs", "VariableText", "Notes"] {
        assert!(draft.report.notes.iter().any(|n| n.kind == NoteKind::UnknownElement && n.name == name), "{name}");
    }
}

#[test]
fn embedded_bitmap_is_decoded_and_framed_by_its_transform() {
    let png = png_2x2();
    // 5 × 2 mm bitmap, centred on the origin, moved by (10, 20) inside a group translated by (100, 0).
    let xml = format!(
        r#"<LightBurnProject><Shape Type="Group"><XForm>1 0 0 1 100 0</XForm><Children>
        <Shape Type="Bitmap" CutIndex="0" W="5" H="2" Gamma="1.5" Contrast="10" File="/x/y.png" Data="{}"><XForm>1 0 0 1 10 20</XForm></Shape>
        </Children></Shape></LightBurnProject>"#,
        base64(&png)
    );
    let draft = parse_lbrn2(xml.as_bytes()).unwrap();
    let asset = &draft.report.assets[0];
    let data = asset.data.as_ref().unwrap();
    assert_eq!((data.format, data.bytes.as_slice()), (RasterFormat::Png, png.as_slice()));
    assert_eq!(asset.size_mm, Some((5.0, 2.0)));
    let b = asset.placement.as_ref().unwrap().bounds().unwrap();
    assert!(close(b.min().x, 107.5, 1e-9) && close(b.max().x, 112.5, 1e-9), "{b:?}");
    assert!(close(b.min().y, 19.0, 1e-9) && close(b.max().y, 21.0, 1e-9), "{b:?}");
    assert_eq!(asset.adjustments, [("Gamma".to_owned(), "1.5".to_owned()), ("Contrast".to_owned(), "10".to_owned())]);
}

/// The bitmap names the image cut setting it was drawn with, so the UI can pair frame and operation.
#[test]
fn a_bitmap_remembers_the_cut_setting_it_was_drawn_with() {
    let xml = format!(
        r#"<LightBurnProject><Shape Type="Bitmap" CutIndex="3" W="5" H="2" Data="{0}"><XForm>1 0 0 1 0 0</XForm></Shape>
        <Shape Type="Bitmap" W="5" H="2" Data="{0}"><XForm>1 0 0 1 0 0</XForm></Shape></LightBurnProject>"#,
        base64(&png_2x2())
    );
    let draft = parse_lbrn2(xml.as_bytes()).unwrap();
    let cuts: Vec<_> = draft.report.assets.iter().map(|a| a.cut_index).collect();
    assert_eq!(cuts, [Some(3), None], "a bitmap without CutIndex has none");
}

#[test]
fn undecodable_or_unknown_bitmap_data_is_reported_and_never_imported() {
    let text = base64(b"this is not an image");
    for data in [text.as_str(), "!!!!", "iVBORw0K"] {
        let xml = format!(
            r#"<LightBurnProject><Shape Type="Bitmap" W="1" H="1" Data="{data}"><XForm>1 0 0 1 0 0</XForm></Shape></LightBurnProject>"#
        );
        let draft = parse_lbrn2(xml.as_bytes()).unwrap();
        let asset = &draft.report.assets[0];
        assert!(asset.data.is_none(), "{data}");
        assert!(
            draft.report.notes.iter().any(|n| n.kind == NoteKind::InvalidValue && n.name == "Bitmap@Data"),
            "{data}"
        );
    }
    // Whitespace inside the base64 text (line-wrapped data) is accepted.
    let png = png_2x2();
    let wrapped =
        base64(&png).as_bytes().chunks(16).map(|c| std::str::from_utf8(c).unwrap()).collect::<Vec<_>>().join("\n");
    let xml = format!(
        r#"<LightBurnProject><Shape Type="Bitmap" W="1" H="1" Data="{wrapped}"><XForm>1 0 0 1 0 0</XForm></Shape></LightBurnProject>"#
    );
    assert!(parse_lbrn2(xml.as_bytes()).unwrap().report.assets[0].data.is_some());
}

#[test]
fn the_research_bitmap_decodes_to_the_png_next_to_it() {
    let draft = parse_lbrn2(&research_fixture("gray-ramp-grayscale.lbrn2")).unwrap();
    let asset = &draft.report.assets[0];
    let data = asset.data.as_ref().unwrap();
    assert_eq!(data.format, RasterFormat::Png);
    // LightBurn re-encodes the image, so the bytes differ from gray-ramp.png (independent oracle: the
    // `image` crate decodes both). OBSERVED in this one fixture (MirrorX/MirrorY = True, identity XForm):
    // the embedded pixels are the horizontal mirror image of the original. The importer hands the
    // bytes over unchanged; what the orientation means is unverified (see IM28 notes).
    let embedded = image::load_from_memory(&data.bytes).unwrap().to_luma8();
    let original = image::load_from_memory(&research_fixture("gray-ramp.png")).unwrap().to_luma8();
    assert_eq!((embedded.width(), embedded.height()), (16, 4));
    assert_eq!(embedded, image::imageops::flip_horizontal(&original));
    assert_eq!(asset.size_mm, Some((5.6437387, 1.4109347)));
}

#[test]
fn image_cut_setting_becomes_a_disabled_image_operation_once_the_bitmap_has_an_asset() {
    let draft = parse_lbrn2(&research_fixture("gray-ramp-grayscale.lbrn2")).unwrap();
    let p = &draft.cut_settings[0];
    let image = p.image.as_ref().unwrap();
    assert_eq!(image.dither_name.as_deref(), Some("grayscale"));
    assert_eq!(image.render, Some(ImageRender::Grayscale));
    assert_eq!((image.min_power, image.max_power, image.speed_mm_s), (1.0, 60.0, 100.0));
    let source = burnburn_io::asset_hash(b"gray-ramp.png");
    let op = draft.image_operation(0, source).unwrap();
    assert!(!op.output_enabled);
    let OperationMode::Image(settings) = op.mode else { panic!("expected an image mode") };
    assert_eq!(settings.render(), ImageRender::Grayscale);
    assert_eq!(settings.source(), source);
    assert!(draft.image_operation(7, source).is_none(), "no such layer");
    // The interval is not in the file: the default is assumed visibly.
    assert!(p.assumptions.iter().any(|a| a.field == "interval"), "{:?}", p.assumptions);
}

#[test]
fn image_mode_names_map_through_a_visible_assumption_table() {
    let setting = |mode: &str| {
        format!(
            r#"<LightBurnProject><CutSetting_Img type="Image"><index Value="0"/><maxPower Value="50"/><speed Value="100"/><interval Value="0.2"/><ditherMode Value="{mode}"/></CutSetting_Img></LightBurnProject>"#
        )
    };
    let cases: [(&str, Option<ImageRender>); 8] = [
        ("grayscale", Some(ImageRender::Grayscale)),
        ("threshold", Some(ImageRender::Dither(DitherMode::Threshold { level: 128 }))),
        ("dither", Some(ImageRender::Dither(DitherMode::FloydSteinberg))),
        ("atkinson", Some(ImageRender::Dither(DitherMode::Atkinson))),
        ("stucki", Some(ImageRender::Dither(DitherMode::Stucki))),
        ("jarvis", Some(ImageRender::Dither(DitherMode::Jarvis))),
        ("newsprint", Some(ImageRender::Dither(DitherMode::Newsprint))),
        ("3dslice", None),
    ];
    for (name, expected) in cases {
        let draft = parse_lbrn2(setting(name).as_bytes()).unwrap();
        let p = &draft.cut_settings[0];
        assert_eq!(p.image.as_ref().unwrap().render, expected, "{name}");
        assert!(close(p.image.as_ref().unwrap().interval_mm, 0.2, 1e-12));
        if expected.is_some() && name != "grayscale" {
            assert!(
                p.assumptions.iter().any(|a| a.field == "ditherMode" && a.value.contains(name)),
                "{name}: {:?}",
                p.assumptions
            );
        }
        if expected.is_none() {
            assert!(draft.image_operation(0, burnburn_io::asset_hash(b"x")).is_none(), "{name} has no operation");
            assert!(
                draft
                    .report
                    .notes
                    .iter()
                    .any(|n| n.kind == NoteKind::InvalidValue && n.name == format!("ditherMode={name}")),
                "{name}"
            );
        }
    }
}
