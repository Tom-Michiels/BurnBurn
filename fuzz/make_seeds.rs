//! Writes seed files for the container-shaped fuzz targets that have no checked-in fixture:
//! clipboard payload, material library, art library, bundle. Usage: `cargo run --bin make_seeds -- <corpus-dir>`
//! (creates `<corpus-dir>/io_clipboard`, `io_material_library`, `io_art_library`, `io_bundle`).
//! Seeds only; the real inputs for each target are in the corpus after a run.

use std::path::PathBuf;

use burnburn_document::process::{
    Kerf, OperationMode, Passes, PowerPercent, PowerRange, ProcessSettings, RecipeDraft, Speed,
};
use burnburn_document::{ObjectKind, PasteItem};
use burnburn_geometry::{Path, Point, Rect};
use burnburn_io::ProjectUuid;
use burnburn_io::art_library::{ArtFormat, ArtLibrary, NewArt, encode_art_library};
use burnburn_io::bundle::{BundleContents, encode_bundle};
use burnburn_io::clipboard::encode_clipboard;
use burnburn_io::material_library::{MaterialLibrary, encode_material_library};
use burnburn_types::Length;

const SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10" width="10mm" height="10mm"><rect x="1" y="1" width="5" height="5"/></svg>"#;

fn write(dir: &std::path::Path, target: &str, name: &str, bytes: &[u8]) -> std::io::Result<()> {
    let d = dir.join(target);
    std::fs::create_dir_all(&d)?;
    std::fs::write(d.join(name), bytes)
}

fn main() -> Result<(), String> {
    let dir = PathBuf::from(std::env::args().nth(1).ok_or("usage: make_seeds <corpus-dir>")?);
    let io = |e: std::io::Error| e.to_string();
    let len = |mm: f64| Length::from_mm(mm).map_err(|e| format!("{e:?}"));

    let rect = Rect::new(Point::new(0.0, 0.0), len(20.0)?, len(10.0)?).map_err(|e| format!("{e:?}"))?;
    let item = PasteItem {
        name: "Plaat".into(),
        kind: ObjectKind::Rectangle,
        path: Path::rectangle(rect),
        parametric: None,
        group: vec![0],
        operations: vec![],
    };
    let clip = encode_clipboard(ProjectUuid::from_bytes([9; 16]), &[item]).map_err(|e| format!("{e:?}"))?;
    write(&dir, "io_clipboard", "rect.json", &clip).map_err(io)?;

    let p = PowerPercent::new(50.0).map_err(|e| format!("{e:?}"))?;
    let mut materials = MaterialLibrary::new("seed");
    materials
        .add(RecipeDraft {
            name: "Snijden".into(),
            material: "Acryl".into(),
            thickness: len(3.0)?,
            machine: "m".into(),
            notes: String::new(),
            mode: OperationMode::Line { kerf: Kerf::NONE },
            settings: ProcessSettings {
                speed: Speed::mm_per_s(10.0).map_err(|e| format!("{e:?}"))?,
                power: PowerRange::new(p, p).map_err(|e| format!("{e:?}"))?,
                passes: Passes::new(1).map_err(|e| format!("{e:?}"))?,
                air_assist: false,
            },
        })
        .map_err(|e| format!("{e:?}"))?;
    let mat = encode_material_library(&materials).map_err(|e| format!("{e:?}"))?;
    write(&dir, "io_material_library", "one.bbmat", &mat).map_err(io)?;

    let mut art = ArtLibrary::new("seed");
    art.add_group("G").map_err(|e| format!("{e:?}"))?;
    art.add_item(NewArt {
        name: "A",
        group: "G",
        tags: vec!["t".into()],
        format: ArtFormat::Svg,
        bytes: SVG,
        dxf_scale: None,
    })
    .map_err(|e| format!("{e:?}"))?;
    let art_bytes = encode_art_library(&art).map_err(|e| format!("{e:?}"))?;
    write(&dir, "io_art_library", "one.bbart", &art_bytes).map_err(io)?;

    let bundle =
        encode_bundle(&BundleContents { materials: Some(&materials), art: Some(&art), ..BundleContents::default() })
            .map_err(|e| format!("{e:?}"))?;
    write(&dir, "io_bundle", "mat-art.bbbundle", &bundle).map_err(io)?;
    // Container-shaped targets also accept each other's bytes; a mismatched seed costs nothing.
    write(&dir, "io_bundle", "art-only.zip", &art_bytes).map_err(io)?;
    Ok(())
}
