//! Golf-9 integration: in quiet mode the user's own input never reaches the test app. Found under
//! load: the native `cut_settings_options` test once received five `Pointer { phase: Move }` intents
//! from the real mouse moving over its (transparent) window.

use burnburn_macos::adapter::quiet_drops_queued_event;

// NSEventType raw values.
const USER_INPUT: [usize; 22] = [
    1, 2, 3, 4, 5, 6, 7, 8, 9, // mouse down/up/moved/dragged/entered/exited (left, right)
    10, 11, 12, // key down/up, flags changed
    22, // scroll wheel
    25, 26, 27, // other mouse down/up/dragged
    29, 30, 31, 32, // gesture, magnify, swipe, smart magnify
    18, 34, // rotate, pressure
];
const NOT_INPUT: [usize; 4] = [13, 14, 15, 16]; // app-kit defined, system defined, application defined, periodic

/// The window server stamps real events with the system uptime.
const REAL: f64 = 416_948.68;

#[test]
fn the_users_mouse_keys_and_gestures_are_dropped() {
    for t in USER_INPUT {
        assert!(quiet_drops_queued_event(t, REAL), "event type {t} is user input");
    }
}

/// A test's own event (built with timestamp 0) may go through the queue (a button's tracking loop
/// takes its mouse-up from there) and must never be dropped.
#[test]
fn a_tests_own_queued_event_still_arrives() {
    for t in USER_INPUT {
        assert!(!quiet_drops_queued_event(t, 0.0), "event type {t} from a test");
    }
}

#[test]
fn system_and_application_events_still_run() {
    for t in NOT_INPUT {
        assert!(!quiet_drops_queued_event(t, REAL), "event type {t} keeps the app running");
    }
}
