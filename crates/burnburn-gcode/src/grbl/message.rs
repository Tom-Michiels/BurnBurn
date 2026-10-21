//! One response line from the controller.

use std::fmt;

/// Machine state from a status report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MachineState {
    Idle,
    Run,
    /// `Hold:0` (hold complete) or `Hold:1` (hold in progress).
    Hold(u8),
    Jog,
    Alarm,
    /// `Door:0..=3`.
    Door(u8),
    Check,
    Home,
    Sleep,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Position {
    pub x: f64,
    pub y: f64,
    pub z: Option<f64>,
}

/// A parsed `<...>` status report.
#[derive(Debug, Clone, PartialEq)]
pub struct StatusReport {
    pub state: MachineState,
    pub mpos: Option<Position>,
    pub wpos: Option<Position>,
    pub wco: Option<Position>,
    /// `Bf:planner,rx` free buffer slots / bytes.
    pub buffer: Option<(u32, u32)>,
    pub line: Option<u32>,
    pub feed: Option<f64>,
    pub spindle: Option<f64>,
    pub pins: Option<String>,
    /// `Ov:feed,rapid,spindle` percent.
    pub overrides: Option<(u16, u16, u16)>,
    pub accessories: Option<String>,
    /// Fields this parser does not know, verbatim (forward compatible, never dropped).
    pub unknown_fields: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Message {
    Ok,
    Error(u8),
    Alarm(u8),
    Status(Box<StatusReport>),
    /// `Grbl 1.1h ['$' for help]`: the controller (re)started.
    Welcome {
        version: String,
    },
    /// `$N=value`.
    Setting {
        id: u16,
        value: String,
    },
    /// `[...]` feedback/message block, without the brackets.
    Feedback(String),
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// A line that has a known shape but is malformed.
    Malformed { reason: &'static str },
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::Malformed { reason } => write!(f, "malformed controller line: {reason}"),
        }
    }
}

impl std::error::Error for ParseError {}

fn bad<T>(reason: &'static str) -> Result<T, ParseError> {
    Err(ParseError::Malformed { reason })
}

/// Parses one line (terminator and surrounding whitespace are ignored). Total: never panics.
///
/// Known shapes that are malformed are errors; a line of an unknown shape is
/// [`Message::Other`] (banners, echo of `$N` startup blocks, ...).
pub fn parse_line(line: &str) -> Result<Message, ParseError> {
    let t = line.trim();
    if t == "ok" {
        return Ok(Message::Ok);
    }
    if let Some(rest) = t.strip_prefix("error:") {
        return rest.trim().parse::<u8>().map(Message::Error).or_else(|_| bad("error code"));
    }
    if let Some(rest) = t.strip_prefix("ALARM:") {
        return rest.trim().parse::<u8>().map(Message::Alarm).or_else(|_| bad("alarm code"));
    }
    if t.starts_with('<') {
        return parse_status(t).map(|r| Message::Status(Box::new(r)));
    }
    if let Some(rest) = t.strip_prefix("Grbl ") {
        let version = rest.split_whitespace().next().unwrap_or_default();
        return Ok(Message::Welcome { version: version.to_owned() });
    }
    if let Some(rest) = t.strip_prefix('$')
        && rest.starts_with(|c: char| c.is_ascii_digit())
    {
        let Some((id, value)) = rest.split_once('=') else { return bad("setting without value") };
        let Ok(id) = id.parse::<u16>() else { return bad("setting number") };
        return Ok(Message::Setting { id, value: value.trim().to_owned() });
    }
    if let Some(inner) = t.strip_prefix('[').and_then(|r| r.strip_suffix(']')) {
        return Ok(Message::Feedback(inner.to_owned()));
    }
    Ok(Message::Other(t.to_owned()))
}

fn number(text: &str) -> Result<f64, ParseError> {
    match text.trim().parse::<f64>() {
        Ok(v) if v.is_finite() => Ok(v),
        _ => bad("number"),
    }
}

fn position(text: &str) -> Result<Position, ParseError> {
    let parts: Vec<&str> = text.split(',').collect();
    if !(2..=6).contains(&parts.len()) {
        return bad("position needs 2 to 6 axes");
    }
    let z = if parts.len() >= 3 { Some(number(parts[2])?) } else { None };
    // Axes beyond Z are validated but not kept.
    for extra in parts.iter().skip(3) {
        number(extra)?;
    }
    Ok(Position { x: number(parts[0])?, y: number(parts[1])?, z })
}

fn state(text: &str) -> Result<MachineState, ParseError> {
    let (name, sub) = match text.split_once(':') {
        Some((n, s)) => (n, Some(s)),
        None => (text, None),
    };
    let sub = match sub {
        None => None,
        Some(s) => Some(s.parse::<u8>().or_else(|_| bad("state substate"))?),
    };
    Ok(match (name, sub) {
        ("Idle", None) => MachineState::Idle,
        ("Run", None) => MachineState::Run,
        ("Jog", None) => MachineState::Jog,
        ("Alarm", None) => MachineState::Alarm,
        ("Check", None) => MachineState::Check,
        ("Home", None) => MachineState::Home,
        ("Sleep", None) => MachineState::Sleep,
        ("Hold", Some(n)) => MachineState::Hold(n),
        ("Door", Some(n)) => MachineState::Door(n),
        _ => return bad("machine state"),
    })
}

fn parse_status(t: &str) -> Result<StatusReport, ParseError> {
    let Some(inner) = t.strip_prefix('<').and_then(|r| r.strip_suffix('>')) else {
        return bad("status report is not closed");
    };
    let mut fields = inner.split('|');
    let first = fields.next().unwrap_or_default();
    if first.is_empty() {
        return bad("status report without state");
    }
    let mut r = StatusReport {
        state: state(first)?,
        mpos: None,
        wpos: None,
        wco: None,
        buffer: None,
        line: None,
        feed: None,
        spindle: None,
        pins: None,
        overrides: None,
        accessories: None,
        unknown_fields: Vec::new(),
    };
    for field in fields {
        let Some((key, value)) = field.split_once(':') else {
            r.unknown_fields.push(field.to_owned());
            continue;
        };
        match key {
            "MPos" => r.mpos = Some(position(value)?),
            "WPos" => r.wpos = Some(position(value)?),
            "WCO" => r.wco = Some(position(value)?),
            "Bf" => {
                let Some((a, b)) = value.split_once(',') else { return bad("Bf needs two values") };
                let (Ok(a), Ok(b)) = (a.parse::<u32>(), b.parse::<u32>()) else { return bad("Bf values") };
                r.buffer = Some((a, b));
            }
            "Ln" => r.line = Some(value.parse::<u32>().or_else(|_| bad("Ln value"))?),
            "FS" => {
                let Some((f, s)) = value.split_once(',') else { return bad("FS needs two values") };
                r.feed = Some(number(f)?);
                r.spindle = Some(number(s)?);
            }
            "F" => r.feed = Some(number(value)?),
            "Ov" => {
                let p: Vec<&str> = value.split(',').collect();
                let [a, b, c] = p.as_slice() else { return bad("Ov needs three values") };
                let n = |v: &str| v.parse::<u16>().or_else(|_| bad("Ov value"));
                r.overrides = Some((n(a)?, n(b)?, n(c)?));
            }
            "Pn" => r.pins = Some(value.to_owned()),
            "A" => r.accessories = Some(value.to_owned()),
            _ => r.unknown_fields.push(field.to_owned()),
        }
    }
    Ok(r)
}
