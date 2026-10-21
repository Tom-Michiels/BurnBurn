//! Turning a program into lines that are safe to stream.

use std::fmt;

/// Longest line (without terminator) that is streamed. GRBL's line buffer holds 80 bytes and
/// drops longer lines with `error:11`; staying clearly below keeps a margin for the `\n`.
pub const MAX_LINE_BYTES: usize = 70;

/// A validated job: comment-free, trimmed, non-empty, printable-ASCII lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedJob {
    lines: Vec<String>,
}

impl PreparedJob {
    pub fn lines(&self) -> &[String] {
        &self.lines
    }
    pub fn len(&self) -> usize {
        self.lines.len()
    }
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrepareError {
    Empty,
    /// 1-based source line.
    LineTooLong {
        line: usize,
        bytes: usize,
    },
    /// A character GRBL would interpret as a realtime command, or a non-printable/non-ASCII one.
    ForbiddenCharacter {
        line: usize,
        ch: char,
    },
    /// `$...` system commands (settings, homing, ...) do not belong in a job program.
    SystemCommand {
        line: usize,
    },
}

impl fmt::Display for PrepareError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for PrepareError {}

/// Removes `;` and `( ... )` comments, collapses whitespace runs to one space and trims.
fn strip_comments(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut in_paren = false;
    for c in raw.chars() {
        match c {
            ';' if !in_paren => break,
            '(' if !in_paren => in_paren = true,
            ')' if in_paren => in_paren = false,
            _ if in_paren => {}
            c => out.push(c),
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Validates a program for streaming. Comments and blank lines are dropped (a comment costs RX
/// buffer bytes for nothing, and a `?`/`!`/`~` in it would be taken as a realtime command).
pub fn prepare_job(program: &str) -> Result<PreparedJob, PrepareError> {
    let mut lines = Vec::new();
    for (n, raw) in program.lines().enumerate() {
        let line = n + 1;
        let text = strip_comments(raw);
        if text.is_empty() {
            continue;
        }
        if let Some(ch) = text.chars().find(|c| !(' '..='~').contains(c) || matches!(c, '?' | '!' | '~')) {
            return Err(PrepareError::ForbiddenCharacter { line, ch });
        }
        if text.starts_with('$') {
            return Err(PrepareError::SystemCommand { line });
        }
        if text.len() > MAX_LINE_BYTES {
            return Err(PrepareError::LineTooLong { line, bytes: text.len() });
        }
        lines.push(text);
    }
    if lines.is_empty() {
        return Err(PrepareError::Empty);
    }
    Ok(PreparedJob { lines })
}
