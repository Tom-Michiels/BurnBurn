//! G5-windows: starting the host with a file argument, headless.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::ffi::OsString;
use std::path::PathBuf;

use burnburn_presentation::UiIntent;
use burnburn_windows::startup::startup_intents;

fn args(list: &[&str]) -> Vec<OsString> {
    list.iter().map(OsString::from).collect()
}

fn open(path: &str) -> UiIntent {
    UiIntent::Open { path: PathBuf::from(path) }
}

#[test]
fn no_argument_starts_with_a_new_document() {
    assert_eq!(startup_intents(args(&[])), vec![UiIntent::NewDocument]);
}

#[test]
fn a_project_path_is_opened_instead_of_a_blank_document() {
    assert_eq!(startup_intents(args(&["C:\\werk\\plaat.burnburn"])), vec![open("C:\\werk\\plaat.burnburn")]);
}

#[test]
fn several_projects_open_in_order_and_each_only_once() {
    let got = startup_intents(args(&["C:\\a.burnburn", "C:\\b.BURNBURN", "C:\\a.burnburn"]));
    assert_eq!(got, vec![open("C:\\a.burnburn"), open("C:\\b.BURNBURN")]);
}

#[test]
fn quotes_around_a_path_are_dropped() {
    assert_eq!(startup_intents(args(&["\"C:\\mijn map\\x.burnburn\""])), vec![open("C:\\mijn map\\x.burnburn")]);
}

#[test]
fn flags_empty_strings_and_other_files_are_ignored_and_leave_a_new_document() {
    let got = startup_intents(args(&["--verbose", "", "C:\\tekening.svg", "C:\\x.burnburntemplate"]));
    assert_eq!(got, vec![UiIntent::NewDocument]);
}

#[test]
fn a_valid_project_among_ignored_arguments_is_still_opened() {
    let got = startup_intents(args(&["--x", "C:\\tekening.svg", "C:\\x.burnburn"]));
    assert_eq!(got, vec![open("C:\\x.burnburn")]);
}
