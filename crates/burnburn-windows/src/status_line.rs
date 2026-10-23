//! The status line of a document window (LFB-rest, LF3 `StatusTone`): the text with a symbol in
//! front of warnings and errors, and the colour of its tone. Colour is never the only carrier: the
//! glyph says the same. Plain data; the
//! `STATIC` control and `WM_CTLCOLORSTATIC` are in `win32`.

use burnburn_presentation::{DocumentViewModel, MessageKey};

use crate::adapter::message_text;
use crate::l10n::Locale;
use crate::style::{Appearance, Rgb, StatusTone, color, message_tone};

/// What the status line shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusLine {
    /// The text, with the tone's glyph in front for a warning or an error.
    pub text: String,
    pub tone: StatusTone,
}

impl StatusLine {
    /// A message of the view model.
    pub fn message(key: MessageKey, locale: Locale) -> StatusLine {
        let tone = message_tone(key);
        StatusLine::toned(&message_text(key, locale), tone)
    }

    /// Text in a tone (empty text stays empty and neutral: nothing to flag).
    pub fn toned(text: &str, tone: StatusTone) -> StatusLine {
        if text.is_empty() {
            return StatusLine { text: String::new(), tone: StatusTone::Neutral };
        }
        let text = match tone {
            StatusTone::Neutral => text.to_owned(),
            StatusTone::Warning | StatusTone::Error => format!("{} {text}", tone.glyph()),
        };
        StatusLine { text, tone }
    }

    /// The text colour. The window chrome is light, so this is the light palette; it reaches
    /// 4.5:1 on the dialog background (`COLOR_BTNFACE`).
    pub fn color(&self) -> Rgb {
        color(self.tone.role(), Appearance::Light)
    }

    /// The colour as a Win32 `COLORREF` (0x00BBGGRR).
    pub fn colorref(&self) -> u32 {
        let Rgb(r, g, b) = self.color();
        u32::from(r) | u32::from(g) << 8 | u32::from(b) << 16
    }
}

/// The status line of a document: a field error first, then the newest message, then the measurement
/// of the tool in use (neutral).
pub fn status_line(doc: &DocumentViewModel, locale: Locale) -> StatusLine {
    if let Some(error) = doc.inspector.as_ref().and_then(|i| i.fields.iter().find_map(|f| f.error)) {
        return StatusLine::toned(&message_text(error, locale), StatusTone::Error);
    }
    if let Some(last) = doc.messages.last() {
        return StatusLine::message(last.key, locale);
    }
    match &doc.tool_measurement {
        Some(m) => StatusLine::toned(&crate::toolbar::measurement_text(m, locale), StatusTone::Neutral),
        None => StatusLine::toned("", StatusTone::Neutral),
    }
}
