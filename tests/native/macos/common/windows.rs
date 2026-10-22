//! The windows, panels and sheets that the GUI review (`gui_snapshots`) and the layout invariants
//! (`layout_invariants`) walk through: one list, opened through the menu like a user would
//! (`docs/plan/look-and-feel.md`, LF0/LF1), and the fixture data they share. Nothing here talks
//! to a machine: the profile is the fixture dialect.

use burnburn_macos::adapter::MenuItem;
use burnburn_macos::app_menu::AppAction;
use burnburn_presentation::{CamDialog, CommandKey};

/// Step name and the menu item that opens the window(s) of that step.
pub fn steps() -> Vec<(&'static str, MenuItem)> {
    vec![
        ("preferences", MenuItem::App(AppAction::Preferences)),
        ("shortcuts", MenuItem::App(AppAction::Shortcuts)),
        ("history", MenuItem::App(AppAction::History)),
        ("machine-hub", MenuItem::App(AppAction::MachineHub)),
        ("laser", MenuItem::LaserWindow),
        ("material-library", MenuItem::Command(CommandKey::ShowMaterialLibrary)),
        ("art-library", MenuItem::Command(CommandKey::ShowArtLibrary)),
        ("notes", MenuItem::Command(CommandKey::ShowNotes)),
        ("schikken", MenuItem::SchikkenPanel),
        ("offset", MenuItem::OffsetPanel),
        ("structure", MenuItem::StructurePanel),
        ("trace-prep", MenuItem::TracePrepPanel),
        ("operator", MenuItem::OperatorPanel),
        ("remnants", MenuItem::RemnantsPanel),
        ("recognition", MenuItem::RecognitionPanel),
        ("search", MenuItem::Command(CommandKey::SearchProjects)),
        ("generator", MenuItem::Command(CommandKey::GeneratorNew)),
        ("cam-optimization", MenuItem::CamDialog(CamDialog::Optimization)),
        ("cam-order", MenuItem::CamDialog(CamDialog::Order)),
        ("cam-sublayers", MenuItem::CamDialog(CamDialog::Sublayers)),
        ("cam-device", MenuItem::CamDialog(CamDialog::Device)),
        ("cam-array", MenuItem::CamDialog(CamDialog::Array)),
        ("cam-material-test", MenuItem::CamDialog(CamDialog::MaterialTest)),
        ("cam-interval-test", MenuItem::CamDialog(CamDialog::IntervalTest)),
        ("cam-focus-test", MenuItem::CamDialog(CamDialog::FocusTest)),
        ("cam-feeder", MenuItem::CamDialog(CamDialog::Feeder)),
        ("round-corners", MenuItem::Command(CommandKey::ShapesRoundCorners)),
        ("barcode", MenuItem::Command(CommandKey::BarcodeNew)),
        ("export-svg", MenuItem::Command(CommandKey::ExportSvg)),
        ("export-bundle", MenuItem::Command(CommandKey::ExportBundle)),
        ("cam-preflight", MenuItem::CamDialog(CamDialog::Preflight)),
        ("preview", MenuItem::App(AppAction::Preview)),
        ("inspect-rd", MenuItem::App(AppAction::InspectRd)),
        ("sketch", MenuItem::Command(CommandKey::SketchNew)),
    ]
}

/// A 600 x 400 mm fixture machine (never a real laser) so plan and preview can open.
pub fn machine_profile() -> Result<burnburn_job::MachineProfile, String> {
    use burnburn_job::{MachineFrame, MachineProfile, OriginCorner, PowerRatio};
    let e = |e: &dyn std::fmt::Debug| format!("{e:?}");
    MachineProfile::new(
        "review-600x400",
        1,
        MachineFrame::new(600.0, 400.0, OriginCorner::BottomLeft).map_err(|x| e(&x))?,
        400.0,
        400.0,
        PowerRatio::new(0.0).map_err(|x| e(&x))?,
        PowerRatio::new(1.0).map_err(|x| e(&x))?,
    )
    .map_err(|x| e(&x))
}

/// One line operation at 100 mm/s, 20 % power.
pub fn cut_draft() -> Result<burnburn_document::process::OperationDraft, String> {
    use burnburn_document::process::{
        Kerf, OperationDraft, OperationMode, Passes, PowerPercent, PowerRange, ProcessSettings, Speed,
    };
    let e = |e: &dyn std::fmt::Debug| format!("{e:?}");
    let p = PowerPercent::new(20.0).map_err(|x| e(&x))?;
    Ok(OperationDraft {
        name: "Snijden".into(),
        mode: OperationMode::Line { kerf: Kerf::NONE },
        settings: ProcessSettings {
            speed: Speed::mm_per_s(100.0).map_err(|x| e(&x))?,
            power: PowerRange::new(p, p).map_err(|x| e(&x))?,
            passes: Passes::new(1).map_err(|x| e(&x))?,
            air_assist: true,
        },
        output_enabled: true,
    })
}
