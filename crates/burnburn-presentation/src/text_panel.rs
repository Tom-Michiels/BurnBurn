//! The text toolbar and the text-on-path dialog (W3-UI-tekst): what a host shows for the selected
//! text object and the semantic edits it sends back. Raw field text goes to the controller
//! unparsed (DR-05); every accepted edit is one document command and one undo step.

use std::sync::Arc;

pub use burnburn_generators::Distort;
pub use burnburn_text::{Align, VAlign};

/// A numeric field of the text toolbar, named in [`TextIssue::InvalidNumber`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextField {
    /// Extra space between letters (mm, may be negative).
    HSpace,
    /// Extra space between lines (mm, may be negative).
    VSpace,
    /// The em size of the text (mm, greater than zero).
    Height,
    /// Where the text starts along the guide (mm, not negative).
    PathStart,
    /// Distance of the baseline from the guide (mm, may be negative).
    PathOffset,
}

/// Why a toolbar control is off or why the last input was refused. Hosts show it in words.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum TextIssue {
    /// The field does not hold a valid number; nothing changed.
    InvalidNumber(TextField),
    /// The font family has no face in this style; the toggle is off (shaping would report the
    /// font missing, it never picks another face).
    StyleMissing,
    /// The chosen family is not in the font source.
    FamilyUnavailable,
    /// Welded needs outline text; single-line text has nothing to weld.
    WeldedNeedsOutline,
    /// The document refused the edit.
    Refused,
    /// Convert to Path found no outlines (empty text).
    NothingToConvert,
    /// Text on a path needs exactly one text object and one other object as the guide.
    PathNeedsTextAndGuide,
    /// Text on a path is one line.
    PathMultiline,
    /// The text is longer than the guide after the start.
    PathTextTooLong,
    /// The guide has no length.
    PathEmptyGuide,
    /// The generator refused the text or guide for another reason.
    PathFailed,
    /// No font source is attached.
    NoFonts,
    /// A path setting was changed on a text that does not follow a path (G7-editor, E08).
    NotOnPath,
}

/// A toggle of the toolbar: its state, whether it can be used, and why not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextToggle {
    pub on: bool,
    pub enabled: bool,
    pub why_off: Option<TextIssue>,
}

/// For each toolbar value: the selected texts do not all have it (G7-editor, E07). The toolbar then
/// shows the value of the first text and the host marks the control as mixed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TextMixed {
    pub family: bool,
    pub bold: bool,
    pub italic: bool,
    pub uppercase: bool,
    pub align: bool,
    pub valign: bool,
    pub h_space: bool,
    pub v_space: bool,
    pub height: bool,
    pub welded: bool,
    pub single_line: bool,
}

/// The toolbar for the selected text object(s): one text, or several at once. Values of the text source, formatted for the
/// fields (millimetres); `None` in the document view model when no single text is selected.
#[derive(Debug, Clone, PartialEq)]
pub struct TextToolbarViewModel {
    /// How many text objects the toolbar edits (1 for a single text).
    pub count: usize,
    /// Which values differ between those texts.
    pub mixed: TextMixed,
    /// Font families the source lists, sorted; at least the current one.
    pub families: Arc<[String]>,
    pub family: String,
    pub bold: TextToggle,
    pub italic: TextToggle,
    pub uppercase: bool,
    pub align: Align,
    pub valign: VAlign,
    /// HSpace: extra letter spacing (mm).
    pub h_space: String,
    /// VSpace: extra line distance (mm).
    pub v_space: String,
    /// Height: the em size (mm).
    pub height: String,
    pub welded: TextToggle,
    /// Single-line stroke text (open paths) instead of outline text.
    pub single_line: bool,
    /// Characters no font in the chain can draw (they get no geometry, never tofu).
    pub missing_glyphs: usize,
    /// The font of the text is not available: the outlines are the last valid ones.
    pub font_missing: bool,
    /// The font is missing: which one, and the installed families to choose from instead (E07).
    /// Choosing is `TextEdit::Family`; until then the stored outlines stay.
    pub missing_font: Option<MissingFontChoice>,
    /// Convert to Path is possible (the text has outlines).
    pub can_convert: bool,
    /// The text is variable text (see the variable-text panel).
    pub variable: bool,
    /// Why the last input was refused.
    pub issue: Option<TextIssue>,
    /// The text follows a guide path (live binding), and how (G7-editor, E08).
    pub path: Option<TextPathView>,
}

/// Whether a text bound to a path can follow it now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PathBindingStatus {
    Following,
    /// The guide object is gone: the last outlines stay and production is blocked.
    GuideMissing,
    /// The guide cannot carry the text (too short, empty, several lines): same consequences.
    DoesNotFit,
}

/// A text that follows a guide path (E08): what the toolbar shows and lets the user change.
#[derive(Debug, Clone, PartialEq)]
pub struct TextPathView {
    /// Name of the guide object, when it still exists.
    pub guide: Option<String>,
    pub start: String,
    pub offset: String,
    pub reverse: bool,
    pub distort: Distort,
    pub status: PathBindingStatus,
}

/// The replacement choice shown while the font of a text is not installed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingFontChoice {
    /// The family the project asks for.
    pub requested: String,
    /// Installed families, sorted.
    pub replacements: Vec<String>,
}

/// One edit of the toolbar. Each accepted edit is one `TextCommand::Set` (one undo step).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum TextEdit {
    /// Another family; the style stays when the family has it, else the regular face.
    Family(String),
    Bold(bool),
    Italic(bool),
    Uppercase(bool),
    Align(Align),
    VAlign(VAlign),
    /// Raw text of the HSpace field.
    HSpace(String),
    /// Raw text of the VSpace field.
    VSpace(String),
    /// Raw text of the Height field.
    Height(String),
    Welded(bool),
    /// Single-line (engraving stroke) text instead of outline text.
    SingleLine(bool),
    /// Where the text starts along its guide (raw text, mm). Only for a text that follows a path.
    PathStart(String),
    /// Distance of the baseline from the guide (raw text, mm, may be negative).
    PathOffset(String),
    PathReverse(bool),
    PathDistort(Distort),
    /// The text stops following its guide and returns to its own place.
    UnbindPath,
}

/// The text-on-path dialog, open after `CommandKey::TextOnPath`.
#[derive(Debug, Clone, PartialEq)]
pub struct TextOnPathViewModel {
    pub distort: Distort,
    pub start: String,
    pub offset: String,
    pub reverse: bool,
    /// Keep the text bound to the guide: it follows when the guide changes (the default). Off,
    /// the dialog makes ordinary paths of the bent text.
    pub live: bool,
    /// The selection is one text and one guide.
    pub ready: bool,
    pub issue: Option<TextIssue>,
}

/// An edit of the text-on-path dialog. `Apply` creates the bent text as ordinary paths (the text
/// and the guide stay); there is no live binding yet.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum TextPathEdit {
    Distort(Distort),
    Start(String),
    Offset(String),
    Reverse(bool),
    /// Follow the guide (`true`) or make ordinary paths (`false`).
    Live(bool),
    Apply,
    Cancel,
}
