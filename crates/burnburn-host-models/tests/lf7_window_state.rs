//! LF7: the saved document window (frame and pane widths) as a defaults string. A stored value the
//! host cannot trust (garbage, not finite, negative or absurd) is dropped, never applied.

use burnburn_host_models::window_state::SavedWindow;

fn sample() -> SavedWindow {
    SavedWindow { x: 70.0, y: 90.0, width: 1210.5, height: 760.0, sidebar: 236.0, inspector: 260.25 }
}

#[test]
fn a_saved_window_comes_back_as_it_was_saved() {
    let text = sample().to_text();
    assert_eq!(SavedWindow::parse(&text), Some(sample()));
}

#[test]
fn the_text_is_six_plain_numbers_so_a_person_can_read_and_fix_it() {
    assert_eq!(sample().to_text(), "70 90 1210.5 760 236 260.25");
}

#[test]
fn text_that_is_not_six_finite_numbers_is_dropped() {
    for bad in ["", "1 2 3", "1 2 3 4 5 6 7", "a b c d e f", "70 90 NaN 760 236 260", "70 90 inf 760 236 260"] {
        assert_eq!(SavedWindow::parse(bad), None, "{bad:?}");
    }
}

#[test]
fn a_window_or_pane_of_no_size_or_absurd_size_is_dropped() {
    for bad in [
        "70 90 0 760 236 260",
        "70 90 1210 -5 236 260",
        "70 90 1210 760 -1 260",
        "70 90 1210 760 236 0",
        "70 90 1e9 760 236 260",
        "1e9 90 1210 760 236 260",
    ] {
        assert_eq!(SavedWindow::parse(bad), None, "{bad:?}");
    }
}

#[test]
fn a_frame_far_off_every_screen_is_not_visible_but_a_partly_visible_one_is() {
    let screens = [(0.0, 0.0, 1440.0, 875.0), (1440.0, 100.0, 1920.0, 1080.0)];
    assert!(sample().is_visible_on(&screens));
    let gone = SavedWindow { x: 5000.0, ..sample() };
    assert!(!gone.is_visible_on(&screens));
    let partly = SavedWindow { x: 1440.0 - 150.0, y: 300.0, ..sample() };
    assert!(partly.is_visible_on(&screens));
    let sliver = SavedWindow { x: 1440.0 - 20.0, y: 300.0, width: 20.0 + 1.0, ..sample() };
    let alone = [(0.0, 0.0, 1440.0, 875.0)];
    assert!(!sliver.is_visible_on(&alone), "a sliver of a few points cannot be grabbed");
}
