//! LFB-rest: the laser window is bounded by the real height of the screen, not by a fixed 700 pt.

use burnburn_host_models::panels::{
    LASER_FALLBACK_WINDOW_HEIGHT_PT, LASER_MIN_WINDOW_HEIGHT_PT, LASER_WINDOW_CHROME_PT, laser_window_height,
};

const CONTENT: f64 = 928.0;

const SCREEN: (f64, f64, f64, f64) = (0.0, 25.0, 1440.0, 875.0); // x, y (Dock), width, height; top edge y = 900

#[test]
fn the_window_is_moved_back_onto_the_visible_screen() {
    use burnburn_host_models::panels::laser_window_top_left;
    let size = (590.0, 840.0);
    // Already inside: unchanged.
    assert_eq!(laser_window_top_left((100.0, 880.0), size, SCREEN), (100.0, 880.0));
    // Top edge above the menu bar: pulled down to the top of the visible area.
    assert_eq!(laser_window_top_left((100.0, 1000.0), size, SCREEN), (100.0, 900.0));
    // Bottom below the Dock: pushed up so the bottom edge is the Dock top.
    assert_eq!(laser_window_top_left((100.0, 500.0), size, SCREEN), (100.0, 25.0 + 840.0));
    // Off the right edge: pulled left; off the left edge: pushed right.
    assert_eq!(laser_window_top_left((1200.0, 880.0), size, SCREEN), (1440.0 - 590.0, 880.0));
    assert_eq!(laser_window_top_left((-300.0, 880.0), size, SCREEN), (0.0, 880.0));
    // A window taller than the screen is pinned to the top-left of the visible area.
    assert_eq!(laser_window_top_left((100.0, 500.0), (590.0, 2000.0), SCREEN), (100.0, 900.0));
}

#[test]
fn a_tall_screen_shows_all_of_the_content_without_scrolling() {
    assert_eq!(laser_window_height(1600.0, CONTENT), CONTENT);
    assert_eq!(laser_window_height(CONTENT + LASER_WINDOW_CHROME_PT, CONTENT), CONTENT);
}

#[test]
fn a_laptop_screen_gets_what_it_can_show_not_a_fixed_700() {
    // A 13-inch laptop leaves about 875 pt below the menu bar and above the Dock.
    let h = laser_window_height(875.0, CONTENT);
    assert_eq!(h, 875.0 - LASER_WINDOW_CHROME_PT);
    assert!(h > LASER_FALLBACK_WINDOW_HEIGHT_PT, "more than the old fixed height: {h}");
    // A short screen gets less than the old fixed height, so the window never runs off it.
    let short = laser_window_height(600.0, CONTENT);
    assert_eq!(short, 600.0 - LASER_WINDOW_CHROME_PT);
    assert!(short < LASER_FALLBACK_WINDOW_HEIGHT_PT);
}

#[test]
fn the_window_never_gets_smaller_than_its_minimum() {
    assert_eq!(laser_window_height(200.0, CONTENT), LASER_MIN_WINDOW_HEIGHT_PT);
}

#[test]
fn an_unknown_or_nonsensical_screen_height_falls_back_instead_of_panicking() {
    for bad in [f64::NAN, f64::INFINITY, -50.0, 0.0] {
        assert_eq!(laser_window_height(bad, CONTENT), LASER_FALLBACK_WINDOW_HEIGHT_PT, "{bad}");
    }
    // A content shorter than the fallback is never exceeded by it.
    assert_eq!(laser_window_height(f64::NAN, 500.0), 500.0);
}

#[test]
fn the_height_grows_with_the_screen_and_always_fits_it() {
    let mut last = 0.0;
    for screen in (300..2000).step_by(25) {
        let screen = f64::from(screen);
        let h = laser_window_height(screen, CONTENT);
        assert!(h >= last, "monotone at {screen}");
        assert!((LASER_MIN_WINDOW_HEIGHT_PT..=CONTENT).contains(&h));
        assert!(h + LASER_WINDOW_CHROME_PT <= screen.max(LASER_MIN_WINDOW_HEIGHT_PT + LASER_WINDOW_CHROME_PT));
        last = h;
    }
}
