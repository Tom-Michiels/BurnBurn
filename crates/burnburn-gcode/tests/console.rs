//! Console (M212) and macros (M216) domain logic.
#![allow(clippy::unwrap_used, clippy::panic)]

mod support;

use burnburn_gcode::GrblDriver;
use burnburn_gcode::console::{
    Console, ConsoleError, EntryKind, MAX_HISTORY, MAX_MACRO_LINES, MAX_TRANSCRIPT, MacroError, MacroSet, MacroState,
};
use burnburn_gcode::grbl::{Command, MAX_LINE_BYTES, SessionConfig, SessionEvent};
use support::fake_grbl::FakeGrbl;

fn line(c: Command) -> String {
    let Command::Line(l) = c else { panic!("{c:?}") };
    l
}

#[test]
fn a_typed_line_becomes_a_session_command_and_is_remembered() {
    let mut c = Console::new();
    assert_eq!(line(c.submit("  G0 X5 Y5  ", false).unwrap()), "G0 X5 Y5");
    assert_eq!(c.history(), ["G0 X5 Y5"]);
}

#[test]
fn system_commands_are_allowed_in_the_console() {
    let mut c = Console::new();
    assert_eq!(line(c.submit("$$", false).unwrap()), "$$");
    assert_eq!(line(c.submit("$H", false).unwrap()), "$H");
}

#[test]
fn the_console_is_closed_while_a_job_runs_and_the_refused_line_is_not_remembered() {
    let mut c = Console::new();
    assert_eq!(c.submit("G0 X5", true), Err(ConsoleError::JobActive));
    assert!(c.history().is_empty());
}

#[test]
fn unsafe_lines_are_refused() {
    let mut c = Console::new();
    assert_eq!(c.submit("   ", false), Err(ConsoleError::Empty));
    assert_eq!(c.submit("", false), Err(ConsoleError::Empty));
    for bad in ["?", "G0 X1 !", "~", "G0\nG1", "G0\r", "G0 \u{18}", "G0 \u{e9}"] {
        assert!(matches!(c.submit(bad, false), Err(ConsoleError::ForbiddenCharacter(_))), "{bad:?}");
    }
    assert_eq!(
        c.submit(&"G".repeat(MAX_LINE_BYTES + 1), false),
        Err(ConsoleError::TooLong { bytes: MAX_LINE_BYTES + 1 })
    );
    assert!(c.submit(&"G".repeat(MAX_LINE_BYTES), false).is_ok());
}

#[test]
fn history_collapses_repeats_and_is_bounded() {
    let mut c = Console::new();
    for l in ["A1", "A1", "B2", "A1"] {
        c.submit(l, false).unwrap();
    }
    assert_eq!(c.history(), ["A1", "B2", "A1"]);
    let mut c = Console::new();
    for i in 0..MAX_HISTORY + 25 {
        c.submit(&format!("G4 P{i}"), false).unwrap();
    }
    assert_eq!(c.history().len(), MAX_HISTORY);
    assert_eq!(c.history()[0], "G4 P25", "the oldest entries are dropped");
}

#[test]
fn recall_walks_the_history_like_a_shell() {
    let mut c = Console::new();
    for l in ["A", "B", "C"] {
        c.submit(l, false).unwrap();
    }
    assert_eq!(c.recall_previous(), Some("C"));
    assert_eq!(c.recall_previous(), Some("B"));
    assert_eq!(c.recall_previous(), Some("A"));
    assert_eq!(c.recall_previous(), Some("A"), "stays at the oldest");
    assert_eq!(c.recall_next(), Some("B"));
    assert_eq!(c.recall_next(), Some("C"));
    assert_eq!(c.recall_next(), None, "past the newest is the empty input line");
    assert_eq!(c.recall_previous(), Some("C"));
    // Submitting resets the walk to the newest.
    c.submit("D", false).unwrap();
    assert_eq!(c.recall_previous(), Some("D"));
    assert_eq!(Console::new().recall_previous(), None);
}

#[test]
fn the_transcript_is_bounded_and_clearable() {
    let mut c = Console::new();
    c.record(EntryKind::Sent, "G0 X1");
    c.record(EntryKind::Received, "ok");
    assert_eq!(c.transcript().len(), 2);
    assert_eq!(c.transcript()[1].kind, EntryKind::Received);
    for i in 0..MAX_TRANSCRIPT + 10 {
        c.record(EntryKind::Info, format!("n{i}"));
    }
    assert_eq!(c.transcript().len(), MAX_TRANSCRIPT);
    assert_eq!(c.transcript().last().unwrap().text, format!("n{}", MAX_TRANSCRIPT + 9));
    c.clear_transcript();
    assert!(c.transcript().is_empty());
}

#[test]
fn macros_hold_named_comment_free_lines() {
    let mut m = MacroSet::new();
    m.add("Home Z", "G91\nG0 Z5 ; lift\n(note)\nG90\n").unwrap();
    assert_eq!(m.names(), ["Home Z"]);
    assert_eq!(m.get("home z").unwrap().lines(), ["G91", "G0 Z5", "G90"]);
}

#[test]
fn macros_may_contain_system_commands() {
    let mut m = MacroSet::new();
    m.add("Home", "$H\n$X").unwrap();
    assert_eq!(m.get("Home").unwrap().lines(), ["$H", "$X"]);
}

#[test]
fn invalid_macros_are_refused_with_the_reason() {
    let mut m = MacroSet::new();
    assert_eq!(m.add("  ", "G0 X1"), Err(MacroError::EmptyName));
    assert_eq!(m.add(&"n".repeat(41), "G0 X1"), Err(MacroError::NameTooLong));
    assert_eq!(m.add("a\nb", "G0 X1"), Err(MacroError::InvalidName));
    assert_eq!(m.add("@x", "G0 X1"), Err(MacroError::InvalidName));
    assert_eq!(m.add("empty", "; only a comment\n\n"), Err(MacroError::NoLines));
    assert_eq!(m.add("many", &"G4 P1\n".repeat(MAX_MACRO_LINES + 1)), Err(MacroError::TooManyLines));
    assert_eq!(
        m.add("bad", "G0 X1\nG1 X2 !"),
        Err(MacroError::BadLine { line: 2, reason: ConsoleError::ForbiddenCharacter('!') })
    );
    assert!(m.names().is_empty(), "a refused macro leaves no trace");
    m.add("ok", "G0 X1").unwrap();
    assert_eq!(m.add("OK", "G0 X2"), Err(MacroError::DuplicateName));
}

#[test]
fn replace_rename_and_remove() {
    let mut m = MacroSet::new();
    m.add("A", "G0 X1").unwrap();
    m.add("B", "G0 X2").unwrap();
    m.replace("a", "G0 X9\nG0 X8").unwrap();
    assert_eq!(m.get("A").unwrap().lines(), ["G0 X9", "G0 X8"]);
    assert_eq!(m.replace("nope", "G0 X1"), Err(MacroError::UnknownMacro));
    assert_eq!(m.replace("A", ""), Err(MacroError::NoLines));
    assert_eq!(m.get("A").unwrap().lines().len(), 2, "a refused replace keeps the old macro");
    m.rename("A", "C").unwrap();
    assert_eq!(m.names(), ["C", "B"]);
    assert_eq!(m.rename("C", "b"), Err(MacroError::DuplicateName));
    assert_eq!(m.rename("zzz", "q"), Err(MacroError::UnknownMacro));
    m.remove("b").unwrap();
    assert_eq!(m.names(), ["C"]);
    assert_eq!(m.remove("b"), Err(MacroError::UnknownMacro));
}

#[test]
fn a_macro_never_starts_during_a_job() {
    let mut m = MacroSet::new();
    m.add("A", "G0 X1").unwrap();
    assert_eq!(m.run("A", true), Err(MacroError::JobActive));
    assert_eq!(m.run("nope", false), Err(MacroError::UnknownMacro));
    assert!(m.run("a", false).is_ok());
}

#[test]
fn a_run_sends_one_line_at_a_time_and_stops_at_the_first_error() {
    let mut m = MacroSet::new();
    m.add("M", "G91\nG0 X5\nG90").unwrap();
    let mut run = m.run("M", false).unwrap();
    assert_eq!(line(run.next_command().unwrap()), "G91");
    assert_eq!(run.next_command(), None, "the previous line is not answered yet");
    run.on_event(&SessionEvent::CommandAccepted);
    assert_eq!(line(run.next_command().unwrap()), "G0 X5");
    run.on_event(&SessionEvent::CommandError { code: 20 });
    assert_eq!(run.state(), &MacroState::Failed { line: 1, code: 20 });
    assert_eq!(run.next_command(), None, "the rest is never sent");

    let mut run = m.run("M", false).unwrap();
    for expected in ["G91", "G0 X5", "G90"] {
        assert_eq!(line(run.next_command().unwrap()), expected);
        assert_eq!(run.state(), &MacroState::Running);
        run.on_event(&SessionEvent::CommandAccepted);
    }
    assert_eq!(run.state(), &MacroState::Done);
    assert_eq!(run.next_command(), None);
}

#[test]
fn unrelated_events_do_not_advance_a_run() {
    let mut m = MacroSet::new();
    m.add("M", "G91\nG90").unwrap();
    let mut run = m.run("M", false).unwrap();
    run.next_command().unwrap();
    run.on_event(&SessionEvent::Feedback("hi".into()));
    assert_eq!(run.next_command(), None);
}

#[test]
fn a_macro_runs_through_the_driver_against_the_fake_controller() {
    let fake = FakeGrbl::new();
    let mut d = GrblDriver::new(fake.clone(), SessionConfig::laser(1000));
    d.poll_status().unwrap();
    fake.say(b"<Idle|MPos:0,0,0|FS:0,0>\r\n");
    d.pump().unwrap();
    let mut m = MacroSet::new();
    m.add("Nudge", "G91\nG0 X5\nG90").unwrap();
    let mut run = m.run("Nudge", d.session().job().is_some_and(|j| j.state.is_active())).unwrap();
    for _ in 0..20 {
        if let Some(cmd) = run.next_command() {
            d.command(cmd).unwrap();
        }
        fake.process(1);
        for e in d.pump().unwrap() {
            run.on_event(&e);
        }
        if run.state() != &MacroState::Running {
            break;
        }
    }
    assert_eq!(run.state(), &MacroState::Done);
    assert_eq!(fake.received_text(), "G91\nG0 X5\nG90\n");
}

#[test]
fn the_text_form_reads_a_fixed_fixture() {
    let text = "# BurnBurn macros v1\n@macro Home Z\nG91\nG0 Z5 ; lift\n@end\n@macro Frame\nG0 X0 Y0\n@end\n";
    let m = MacroSet::parse_text(text).unwrap();
    assert_eq!(m.names(), ["Home Z", "Frame"]);
    assert_eq!(m.get("Home Z").unwrap().lines(), ["G91", "G0 Z5"]);
    // Writing keeps the comment-free lines in the same layout.
    assert_eq!(m.to_text(), "# BurnBurn macros v1\n@macro Home Z\nG91\nG0 Z5\n@end\n@macro Frame\nG0 X0 Y0\n@end\n");
    assert_eq!(MacroSet::parse_text(&m.to_text()).unwrap(), m);
}

#[test]
fn the_text_form_rejects_malformed_input_with_its_line() {
    for (text, line) in [
        ("@macro A\nG0 X1\n@end\n", 1),                          // no header
        ("# BurnBurn macros v1\nG0 X1\n", 2),                    // line outside a macro
        ("# BurnBurn macros v1\n@macro A\nG0 X1\n", 4),          // missing @end
        ("# BurnBurn macros v1\n@macro A\n@macro B\n@end\n", 3), // nested
        ("# BurnBurn macros v1\n@end\n", 2),                     // stray end
        ("# BurnBurn macros v2\n", 1),                           // unknown version
    ] {
        assert_eq!(MacroSet::parse_text(text), Err(MacroError::Syntax { line }), "{text:?}");
    }
    assert_eq!(
        MacroSet::parse_text("# BurnBurn macros v1\n@macro A\nG0 X1\n@end\n@macro a\nG0 X2\n@end\n"),
        Err(MacroError::DuplicateName)
    );
    assert_eq!(MacroSet::parse_text("# BurnBurn macros v1\n@macro A\n@end\n"), Err(MacroError::NoLines));
}
