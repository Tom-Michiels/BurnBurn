//! Console (M212) and macros (M216): domain logic only, no UI and no IO.
//!
//! - The [`Console`] validates one typed line, keeps a bounded command history with shell-style
//!   recall and a transcript. It refuses while a job is running (a console line would be
//!   interleaved with the job's own stream).
//! - A [`Macro`] is a named list of G-code lines. A [`MacroSet`] keeps unique names, refuses
//!   running during a job and has a small text form for persistence. A [`MacroRun`] feeds the
//!   lines to the session one at a time and stops at the first `error:N`.

use std::fmt;

use crate::grbl::{Command, MAX_LINE_BYTES, SessionEvent, strip_comments};

/// Commands remembered for recall.
pub const MAX_HISTORY: usize = 200;
/// Transcript entries kept.
pub const MAX_TRANSCRIPT: usize = 1000;
/// Longest macro name.
pub const MAX_MACRO_NAME: usize = 40;
/// Most lines in one macro.
pub const MAX_MACRO_LINES: usize = 100;

const TEXT_HEADER: &str = "# BurnBurn macros v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConsoleError {
    Empty,
    /// A job is running; console lines and macros wait until it ends.
    JobActive,
    TooLong {
        bytes: usize,
    },
    /// Realtime, control or non-ASCII characters, or a line break.
    ForbiddenCharacter(char),
}

impl fmt::Display for ConsoleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for ConsoleError {}

/// One line must be printable ASCII without the characters GRBL takes as realtime commands.
fn check_line(line: &str) -> Result<(), ConsoleError> {
    if let Some(c) = line.chars().find(|c| !(' '..='~').contains(c) || matches!(c, '?' | '!' | '~')) {
        return Err(ConsoleError::ForbiddenCharacter(c));
    }
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Err(ConsoleError::Empty);
    }
    if trimmed.len() > MAX_LINE_BYTES {
        return Err(ConsoleError::TooLong { bytes: trimmed.len() });
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    Sent,
    Received,
    Info,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptEntry {
    pub kind: EntryKind,
    pub text: String,
}

#[derive(Debug, Default)]
pub struct Console {
    history: Vec<String>,
    cursor: Option<usize>,
    transcript: Vec<TranscriptEntry>,
}

impl Console {
    pub fn new() -> Console {
        Console::default()
    }

    /// Validates a typed line; on success it is remembered and returned as the session command.
    /// `job_active`: a job is streaming; nothing is accepted then (and nothing remembered).
    pub fn submit(&mut self, line: &str, job_active: bool) -> Result<Command, ConsoleError> {
        if job_active {
            return Err(ConsoleError::JobActive);
        }
        check_line(line)?;
        let line = line.trim().to_owned();
        if self.history.last() != Some(&line) {
            self.history.push(line.clone());
            if self.history.len() > MAX_HISTORY {
                self.history.remove(0);
            }
        }
        self.cursor = None;
        Ok(Command::Line(line))
    }

    pub fn history(&self) -> &[String] {
        &self.history
    }

    /// Up arrow: the previous (older) command; stays at the oldest.
    pub fn recall_previous(&mut self) -> Option<&str> {
        if self.history.is_empty() {
            return None;
        }
        let i = match self.cursor {
            None => self.history.len() - 1,
            Some(i) => i.saturating_sub(1),
        };
        self.cursor = Some(i);
        self.history.get(i).map(String::as_str)
    }

    /// Down arrow: the next (newer) command; `None` past the newest (the empty input line).
    pub fn recall_next(&mut self) -> Option<&str> {
        let i = self.cursor?;
        if i + 1 < self.history.len() {
            self.cursor = Some(i + 1);
            self.history.get(i + 1).map(String::as_str)
        } else {
            self.cursor = None;
            None
        }
    }

    pub fn record(&mut self, kind: EntryKind, text: impl Into<String>) {
        self.transcript.push(TranscriptEntry { kind, text: text.into() });
        if self.transcript.len() > MAX_TRANSCRIPT {
            let excess = self.transcript.len() - MAX_TRANSCRIPT;
            self.transcript.drain(..excess);
        }
    }

    pub fn transcript(&self) -> &[TranscriptEntry] {
        &self.transcript
    }

    pub fn clear_transcript(&mut self) {
        self.transcript.clear();
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MacroError {
    EmptyName,
    NameTooLong,
    /// Control characters, line breaks or a leading `@` (reserved by the text form).
    InvalidName,
    DuplicateName,
    UnknownMacro,
    NoLines,
    TooManyLines,
    JobActive,
    /// A line of the macro is not a clean single G-code line (1-based source line).
    BadLine {
        line: usize,
        reason: ConsoleError,
    },
    /// The text form could not be read (1-based line).
    Syntax {
        line: usize,
    },
}

impl fmt::Display for MacroError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for MacroError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Macro {
    name: String,
    lines: Vec<String>,
}

impl Macro {
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Comment-free lines, ready to send.
    pub fn lines(&self) -> &[String] {
        &self.lines
    }
}

fn check_name(name: &str) -> Result<String, MacroError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(MacroError::EmptyName);
    }
    if name.chars().any(char::is_control) || name.starts_with('@') {
        return Err(MacroError::InvalidName);
    }
    if name.chars().count() > MAX_MACRO_NAME {
        return Err(MacroError::NameTooLong);
    }
    Ok(name.to_owned())
}

/// Program text -> clean lines (comments dropped), each validated like a console line.
fn parse_program(program: &str) -> Result<Vec<String>, MacroError> {
    let mut lines = Vec::new();
    for (n, raw) in program.lines().enumerate() {
        let text = strip_comments(raw);
        if text.is_empty() {
            continue;
        }
        check_line(&text).map_err(|reason| MacroError::BadLine { line: n + 1, reason })?;
        lines.push(text);
    }
    if lines.is_empty() {
        return Err(MacroError::NoLines);
    }
    if lines.len() > MAX_MACRO_LINES {
        return Err(MacroError::TooManyLines);
    }
    Ok(lines)
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct MacroSet {
    macros: Vec<Macro>,
}

impl MacroSet {
    pub fn new() -> MacroSet {
        MacroSet::default()
    }

    pub fn names(&self) -> Vec<&str> {
        self.macros.iter().map(Macro::name).collect()
    }

    pub fn get(&self, name: &str) -> Option<&Macro> {
        self.index(name).and_then(|i| self.macros.get(i))
    }

    fn index(&self, name: &str) -> Option<usize> {
        self.macros.iter().position(|m| m.name.eq_ignore_ascii_case(name.trim()))
    }

    /// Adds a macro from program text (comments allowed, they are dropped).
    pub fn add(&mut self, name: &str, program: &str) -> Result<(), MacroError> {
        let name = check_name(name)?;
        if self.index(&name).is_some() {
            return Err(MacroError::DuplicateName);
        }
        let lines = parse_program(program)?;
        self.macros.push(Macro { name, lines });
        Ok(())
    }

    /// Replaces the lines of an existing macro; a refused replace keeps the old lines.
    pub fn replace(&mut self, name: &str, program: &str) -> Result<(), MacroError> {
        let i = self.index(name).ok_or(MacroError::UnknownMacro)?;
        let lines = parse_program(program)?;
        self.macros[i].lines = lines;
        Ok(())
    }

    pub fn rename(&mut self, from: &str, to: &str) -> Result<(), MacroError> {
        let i = self.index(from).ok_or(MacroError::UnknownMacro)?;
        let to = check_name(to)?;
        if self.index(&to).is_some_and(|other| other != i) {
            return Err(MacroError::DuplicateName);
        }
        self.macros[i].name = to;
        Ok(())
    }

    pub fn remove(&mut self, name: &str) -> Result<(), MacroError> {
        let i = self.index(name).ok_or(MacroError::UnknownMacro)?;
        self.macros.remove(i);
        Ok(())
    }

    /// Starts a run of a macro; refused while a job is running.
    pub fn run(&self, name: &str, job_active: bool) -> Result<MacroRun, MacroError> {
        if job_active {
            return Err(MacroError::JobActive);
        }
        let m = self.get(name).ok_or(MacroError::UnknownMacro)?;
        Ok(MacroRun {
            name: m.name.clone(),
            lines: m.lines.clone(),
            next: 0,
            outstanding: false,
            state: MacroState::Running,
        })
    }

    /// Text form for persistence: a header line, then `@macro NAME`, the lines and `@end`.
    pub fn to_text(&self) -> String {
        let mut t = format!("{TEXT_HEADER}\n");
        for m in &self.macros {
            t.push_str(&format!("@macro {}\n", m.name));
            for l in &m.lines {
                t.push_str(l);
                t.push('\n');
            }
            t.push_str("@end\n");
        }
        t
    }

    pub fn parse_text(text: &str) -> Result<MacroSet, MacroError> {
        let mut lines = text.lines();
        if lines.next() != Some(TEXT_HEADER) {
            return Err(MacroError::Syntax { line: 1 });
        }
        let mut set = MacroSet::new();
        let mut current: Option<(String, Vec<&str>)> = None;
        let mut total = 1;
        for (n, raw) in lines.enumerate() {
            let line = n + 2;
            total = line;
            let t = raw.trim();
            if let Some(name) = t.strip_prefix("@macro ") {
                if current.is_some() {
                    return Err(MacroError::Syntax { line });
                }
                current = Some((name.to_owned(), Vec::new()));
            } else if t == "@end" {
                let Some((name, body)) = current.take() else { return Err(MacroError::Syntax { line }) };
                set.add(&name, &body.join("\n"))?;
            } else if let Some((_, body)) = current.as_mut() {
                body.push(raw);
            } else if !(t.is_empty() || t.starts_with('#')) {
                return Err(MacroError::Syntax { line });
            }
        }
        if current.is_some() {
            return Err(MacroError::Syntax { line: total + 1 });
        }
        Ok(set)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MacroState {
    Running,
    Done,
    /// The controller rejected line `line` (0-based) with `error:code`; the rest was not sent.
    Failed {
        line: usize,
        code: u8,
    },
}

/// One run of a macro: feeds its lines to the session one at a time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacroRun {
    name: String,
    lines: Vec<String>,
    next: usize,
    outstanding: bool,
    state: MacroState,
}

impl MacroRun {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn state(&self) -> &MacroState {
        &self.state
    }

    /// The next line to send, once the previous one was answered.
    pub fn next_command(&mut self) -> Option<Command> {
        if self.state != MacroState::Running || self.outstanding {
            return None;
        }
        let line = self.lines.get(self.next)?.clone();
        self.outstanding = true;
        Some(Command::Line(line))
    }

    /// Feed every session event here; only the answer to the outstanding line matters.
    pub fn on_event(&mut self, event: &SessionEvent) {
        if !self.outstanding {
            return;
        }
        match event {
            SessionEvent::CommandAccepted => {
                self.outstanding = false;
                self.next += 1;
                if self.next >= self.lines.len() {
                    self.state = MacroState::Done;
                }
            }
            SessionEvent::CommandError { code } => {
                self.outstanding = false;
                self.state = MacroState::Failed { line: self.next, code: *code };
            }
            _ => {}
        }
    }
}
