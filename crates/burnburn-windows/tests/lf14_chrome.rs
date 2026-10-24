//! LF14: the window chrome of the Windows host, headless. The tool strip gets icon bitmaps from
//! `burnburn-icons` (size per DPI, premultiplied BGRA), the status bar has four parts, the window
//! state is kept in a file below `%LOCALAPPDATA%` (here a temporary folder) and restored only
//! where it can be reached, and the dark title bar is one `DwmSetWindowAttribute` call chosen by
//! appearance and Windows build. Creating the real controls is `win32`: a Windows runner.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use burnburn_presentation::{HeadlessEditorHarness, ToolCatalog};
use burnburn_windows::appearance::Appearance;
use burnburn_windows::dwm::{
    DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWA_USE_IMMERSIVE_DARK_MODE_BEFORE_20H1, DarkTitleBar, dark_title_bar,
};
use burnburn_windows::forms::spec_layout::{DluBase, TextMeter};
use burnburn_windows::forms::toolstrip::{StripAction, StripIcon, strip, strip_layout};
use burnburn_windows::icon_dib::{Dib, ICON_POINTS, caret_dib, icon_dib, side_px};
use burnburn_windows::l10n::Locale;
use burnburn_windows::panels::StatusBar;
use burnburn_windows::status_bar::{StatusPartKind, part_edges, status_parts};
use burnburn_windows::status_line::StatusLine;
use burnburn_windows::style::StatusTone;
use burnburn_windows::window_state::{
    SavedWindow, StoredWindow, default_window, load, placement, save, state_file,
};

// ---- tool strip icons -------------------------------------------------------------------------

#[test]
fn the_icon_side_follows_the_dpi_from_16_pixels_at_100_percent() {
    assert_eq!(ICON_POINTS, 16);
    for (dpi, side) in [(96, 16), (120, 20), (144, 24), (192, 32), (0, 16)] {
        assert_eq!(side_px(dpi), side, "{dpi} dpi");
    }
}

fn premultiplied_ok(dib: &Dib) -> bool {
    dib.bgra.chunks_exact(4).all(|p| p[0] <= p[3] && p[1] <= p[3] && p[2] <= p[3])
}

#[test]
fn every_tool_with_artwork_renders_a_dib_of_the_right_size_and_bit_depth_at_every_dpi() {
    let mut with_art = 0;
    for spec in ToolCatalog::standard().specs() {
        for dpi in [96, 144, 192] {
            let dib = icon_dib(spec.icon, Appearance::Light, dpi);
            assert_eq!(dib.is_some(), burnburn_icons::has_icon(spec.icon), "{}", spec.icon);
            let Some(dib) = dib else { continue };
            let side = side_px(dpi);
            assert_eq!((dib.width, dib.height), (side, side), "{} at {dpi}", spec.icon);
            assert_eq!(dib.bgra.len(), (side * side * 4) as usize, "32 bits per pixel: {}", spec.icon);
            assert!(premultiplied_ok(&dib), "{} at {dpi}", spec.icon);
            assert!(dib.bgra.chunks_exact(4).any(|p| p[3] > 0), "{} is not empty", spec.icon);
            if dpi == 96 {
                with_art += 1;
            }
        }
    }
    assert!(with_art > 20, "the register has artwork for many tools: {with_art}");
    assert_eq!(icon_dib("no-such-icon", Appearance::Light, 96), None);
}

#[test]
fn the_channels_are_blue_green_red_and_the_dark_theme_differs_from_the_light_one() {
    // An opaque pixel needs no premultiplication, so it must be the straight RGBA reversed.
    let mut checked = false;
    let mut differs = false;
    for id in burnburn_icons::icon_ids() {
        let rgba = burnburn_icons::render(id, 16, 1.0, burnburn_icons::Theme::Light).unwrap();
        let dib = icon_dib(id, Appearance::Light, 96).unwrap();
        for (s, d) in rgba.pixels.chunks_exact(4).zip(dib.bgra.chunks_exact(4)) {
            if s[3] == 255 {
                assert_eq!([d[0], d[1], d[2], d[3]], [s[2], s[1], s[0], 255], "{id}");
                checked = true;
            }
        }
        differs |= icon_dib(id, Appearance::Dark, 96).unwrap() != dib;
    }
    assert!(checked, "some icon has an opaque pixel");
    assert!(differs, "the dark appearance draws at least one icon in other colours");
}

#[test]
fn the_caret_is_a_drawn_symmetric_chevron_in_the_ink_of_the_appearance() {
    for dpi in [96, 144, 192] {
        let light = caret_dib(Appearance::Light, dpi);
        let side = side_px(dpi);
        assert_eq!((light.width, light.height), (side, side));
        assert!(premultiplied_ok(&light));
        let w = side as usize;
        for row in light.bgra.chunks_exact(w * 4) {
            for x in 0..w / 2 {
                assert_eq!(row[x * 4..x * 4 + 4], row[(w - 1 - x) * 4..(w - x) * 4], "{dpi}: mirrored");
            }
        }
        assert!(light.bgra.chunks_exact(4).any(|p| p[3] == 255));
        assert_ne!(light, caret_dib(Appearance::Dark, dpi));
    }
}

#[test]
fn strip_buttons_carry_their_catalogue_icon_and_the_layout_draws_icons_not_glyph_titles() {
    let mut h = HeadlessEditorHarness::new();
    h.new_document().unwrap();
    let panels = strip(&h.view(), Locale::En);
    let catalog = ToolCatalog::standard();
    for group in panels.iter().flat_map(|p| &p.groups) {
        for b in std::iter::once(&group.button).chain(&group.variants) {
            let spec = catalog.specs().iter().find(|s| s.id == b.id).unwrap();
            assert_eq!(b.icon, spec.icon, "{}", b.id);
        }
    }
    let layout = strip_layout(&panels, 1200, 1.0);
    let tools: Vec<_> = layout.controls.iter().filter(|c| matches!(c.action, StripAction::Tool(_))).collect();
    let iconed: Vec<_> = tools.iter().filter(|c| matches!(c.icon, StripIcon::Tool(_))).collect();
    assert!(iconed.len() > 20);
    // an icon button is as wide as its neighbours whatever its name; its name stays for the screen reader
    assert!(iconed.iter().all(|c| c.rect.w == iconed[0].rect.w && !c.label.is_empty()));
    for c in layout.controls.iter().filter(|c| matches!(c.action, StripAction::Dropdown { .. })) {
        assert_eq!(c.icon, StripIcon::Caret);
        assert!(c.label.chars().all(|ch| ch.is_ascii()), "no glyph character in a title: {:?}", c.label);
    }
}

// ---- status bar -------------------------------------------------------------------------------

struct Fixed(f64);

impl TextMeter for Fixed {
    fn width(&self, text: &str, _bold: bool) -> i32 {
        (text.chars().count() as f64 * self.0).round() as i32
    }
}

fn bar(coordinate: &str) -> StatusBar {
    StatusBar { coordinate: coordinate.into(), zoom: "100 %".into(), unit: "mm".into() }
}

#[test]
fn the_status_bar_has_four_parts_status_with_symbol_coordinate_zoom_and_unit() {
    let error = StatusLine::toned("Openen mislukt", StatusTone::Error);
    let parts = status_parts(&error, &bar("X 12,5  Y 30"), Locale::Nl);
    let kinds: Vec<StatusPartKind> = parts.parts.iter().map(|p| p.kind).collect();
    assert_eq!(kinds, vec![StatusPartKind::Status, StatusPartKind::Coordinate, StatusPartKind::Zoom, StatusPartKind::Unit]);
    let texts: Vec<&str> = parts.parts.iter().map(|p| p.text.as_str()).collect();
    assert_eq!(texts[1..], ["X 12,5  Y 30", "100 %", "mm"]);
    assert!(texts[0].starts_with('\u{2716}') && texts[0].ends_with("Openen mislukt"), "the symbol comes with the status");
    assert_eq!(parts.tone, StatusTone::Error);
    assert_eq!(parts.parts.iter().map(|p| p.name).collect::<Vec<_>>(), vec!["Status", "Aanwijzerpositie", "Zoom", "Eenheid"]);
    let en = status_parts(&StatusLine::toned("", StatusTone::Neutral), &bar(""), Locale::En);
    assert_eq!(en.parts[1].name, "Pointer position");
    assert_eq!(en.parts[0].text, "");
}

#[test]
fn the_part_edges_end_at_minus_one_keep_the_fixed_parts_steady_and_give_the_status_the_rest() {
    for dpi in [96, 144, 192] {
        let base = DluBase::segoe_ui_9(dpi);
        let meter = Fixed(6.0 * f64::from(dpi) / 96.0);
        let wide = part_edges(1280 * dpi as i32 / 96, base, &meter);
        assert_eq!(wide.len(), 4, "{dpi}");
        assert_eq!(wide[3], -1, "the last part runs to the end");
        assert!(wide[0] > 0 && wide.windows(2).take(2).all(|w| w[0] < w[1]), "{dpi}: {wide:?}");
        // the status gets the bulk of a wide bar
        assert!(wide[0] * 2 > 1280 * dpi as i32 / 96, "{dpi}: {wide:?}");
        // a narrow bar keeps the edges increasing and positive
        let narrow = part_edges(200 * dpi as i32 / 96, base, &meter);
        assert!(narrow[0] > 0 && narrow[0] < narrow[1] && narrow[1] < narrow[2], "{dpi}: {narrow:?}");
        // the fixed parts keep their widths when the bar grows
        let widths = |e: &[i32]| [e[1] - e[0], e[2] - e[1]];
        assert_eq!(widths(&wide), widths(&part_edges(1600 * dpi as i32 / 96, base, &meter)), "{dpi}");
    }
}

// ---- window state -----------------------------------------------------------------------------

struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> TempDir {
        let dir = std::env::temp_dir().join(format!("burnburn-lf14-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn window(x: f64) -> StoredWindow {
    StoredWindow {
        window: SavedWindow { x, y: 40.0, width: 1200.0, height: 800.0, sidebar: 220.0, inspector: 280.0 },
        maximized: false,
    }
}

#[test]
fn the_window_state_lives_in_the_burnburn_folder_and_survives_a_round_trip() {
    let dir = TempDir::new("roundtrip");
    let file = state_file(&dir.0);
    assert_eq!(file, dir.0.join("BurnBurn").join("window-state.txt"));
    assert_eq!(load(&file), None, "nothing stored yet");
    let mut stored = window(100.5);
    stored.maximized = true;
    save(&file, &stored).unwrap();
    assert_eq!(load(&file), Some(stored));
    // saving again replaces it and leaves no other file behind
    save(&file, &window(10.0)).unwrap();
    assert_eq!(load(&file), Some(window(10.0)));
    let names: Vec<_> = std::fs::read_dir(file.parent().unwrap()).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(names.len(), 1, "{names:?}");
}

#[test]
fn a_damaged_or_odd_file_is_ignored_and_a_path_that_cannot_be_written_is_an_error() {
    let dir = TempDir::new("damaged");
    let file = state_file(&dir.0);
    for text in ["", "garbage", "1 2 3", "1 2 0 4 5 6\nmaximized=0", "1 2 3 4 5 NaN\nmaximized=0", "1 2 3 4 5 6\nmaximized=maybe"] {
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, text).unwrap();
        assert_eq!(load(&file), None, "{text:?}");
    }
    // the BurnBurn folder cannot be made because a file stands there
    let blocked = TempDir::new("blocked");
    std::fs::write(blocked.0.join("BurnBurn"), "x").unwrap();
    assert!(save(&state_file(&blocked.0), &window(0.0)).is_err());
}

#[test]
fn the_stored_window_opens_only_where_it_can_be_reached() {
    let default = default_window((0.0, 0.0, 1920.0, 1040.0));
    let one = [(0.0, 0.0, 1920.0, 1040.0)];
    let two = [(0.0, 0.0, 1920.0, 1040.0), (1920.0, 0.0, 1920.0, 1040.0)];
    assert_eq!(placement(Some(window(300.0)), &one, default), window(300.0));
    // it was on a second monitor that is gone
    assert_eq!(placement(Some(window(2500.0)), &one, default), default);
    assert_eq!(placement(Some(window(2500.0)), &two, default), window(2500.0));
    assert_eq!(placement(None, &one, default), default);
    // a first start: four fifths of the work area, centred, inside it
    let w = default.window;
    assert!((w.width - 1536.0).abs() < 1.0 && (w.height - 832.0).abs() < 1.0, "{w:?}");
    assert!((w.x - 192.0).abs() < 1.0 && (w.y - 104.0).abs() < 1.0, "{w:?}");
    assert!(!default.maximized && w.sidebar > 0.0 && w.inspector > 0.0);
}

// ---- dark title bar ---------------------------------------------------------------------------

#[test]
fn the_dark_title_bar_is_one_attribute_call_chosen_by_appearance_and_build() {
    assert_eq!(DWMWA_USE_IMMERSIVE_DARK_MODE, 20);
    assert_eq!(DWMWA_USE_IMMERSIVE_DARK_MODE_BEFORE_20H1, 19);
    let call = |appearance, build| dark_title_bar(appearance, build);
    assert_eq!(call(Appearance::Dark, 22631), Some(DarkTitleBar { attribute: 20, value: 1 }));
    assert_eq!(call(Appearance::Light, 22631), Some(DarkTitleBar { attribute: 20, value: 0 }), "switching back resets it");
    assert_eq!(call(Appearance::Dark, 19041), Some(DarkTitleBar { attribute: 20, value: 1 }));
    assert_eq!(call(Appearance::Dark, 19040), Some(DarkTitleBar { attribute: 19, value: 1 }));
    assert_eq!(call(Appearance::Dark, 17763), Some(DarkTitleBar { attribute: 19, value: 1 }));
    assert_eq!(call(Appearance::Dark, 17762), None, "Windows 10 before 1809 has no dark title bar");
    assert_eq!(call(Appearance::Dark, 0), None, "an unknown build is left alone");
}
