//! Turning program text into stream-safe lines.
#![allow(clippy::unwrap_used)]

mod support;

use burnburn_gcode::grbl::{MAX_LINE_BYTES, PrepareError, prepare_job};

#[test]
fn comments_blanks_and_whitespace_are_removed() {
    let job = prepare_job("; header\nG21 ; mm\n\n  G90  \n(inline) G1 X1 (c) F100\n\t\nM2\n").unwrap();
    assert_eq!(job.lines(), ["G21", "G90", "G1 X1 F100", "M2"]);
    assert_eq!(job.len(), 4);
}

#[test]
fn crlf_programs_are_accepted() {
    assert_eq!(prepare_job("G21\r\nG90\r\n").unwrap().lines(), ["G21", "G90"]);
}

#[test]
fn an_empty_or_comment_only_program_is_refused() {
    assert_eq!(prepare_job(""), Err(PrepareError::Empty));
    assert_eq!(prepare_job("; nothing\n(also nothing)\n"), Err(PrepareError::Empty));
}

#[test]
fn realtime_characters_would_be_intercepted_by_the_controller_and_are_refused() {
    for ch in ['?', '!', '~'] {
        let e = prepare_job(&format!("G21\nG1 X1{ch}\n")).unwrap_err();
        assert_eq!(e, PrepareError::ForbiddenCharacter { line: 2, ch });
    }
}

#[test]
fn control_and_non_ascii_characters_are_refused() {
    assert_eq!(prepare_job("G21\u{18}\n").unwrap_err(), PrepareError::ForbiddenCharacter { line: 1, ch: '\u{18}' });
    assert_eq!(prepare_job("G21 \u{e9}\n").unwrap_err(), PrepareError::ForbiddenCharacter { line: 1, ch: '\u{e9}' });
}

#[test]
fn a_realtime_character_inside_a_comment_is_harmless_because_comments_are_stripped() {
    assert_eq!(prepare_job("G21 ; what?!~\n").unwrap().lines(), ["G21"]);
    assert_eq!(prepare_job("G21 (stop!)\n").unwrap().lines(), ["G21"]);
}

#[test]
fn system_commands_do_not_belong_in_a_job() {
    assert_eq!(prepare_job("G21\n$H\n").unwrap_err(), PrepareError::SystemCommand { line: 2 });
    assert_eq!(prepare_job("$32=0\n").unwrap_err(), PrepareError::SystemCommand { line: 1 });
}

#[test]
fn lines_longer_than_the_limit_are_refused_at_the_limit_boundary() {
    let ok = format!("G1 X{}", "1".repeat(MAX_LINE_BYTES - 4)); // exactly MAX_LINE_BYTES
    assert_eq!(ok.len(), MAX_LINE_BYTES);
    assert!(prepare_job(&ok).is_ok());
    let long = format!("{ok}1");
    assert_eq!(prepare_job(&long).unwrap_err(), PrepareError::LineTooLong { line: 1, bytes: MAX_LINE_BYTES + 1 });
}

#[test]
fn a_lowered_program_is_always_streamable() {
    // The backend output must pass its own stream gate (header comment included).
    let plan = support::plans::rectangle_cut(false);
    let text = burnburn_gcode::lower(&plan, &burnburn_gcode::GrblProfile::grbl_1_1_laser()).unwrap().to_text();
    let job = prepare_job(&text).unwrap();
    assert!(job.lines().iter().all(|l| l.len() <= MAX_LINE_BYTES));
    assert_eq!(job.lines().last().map(String::as_str), Some("M2"));
}
