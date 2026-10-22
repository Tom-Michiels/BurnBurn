//! Design tokens both hosts follow (LF3): colour roles with a name and a symbol next to the
//! colour (colour is never the only carrier), the spacing grid, the typography roles, and which
//! tone a status message has. The macOS host maps the colour roles to system colours; the values
//! here are the Windows palette and the reference for the contrast tests.

use burnburn_presentation::MessageKey;

/// Light or dark appearance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Appearance {
    Light,
    Dark,
}

/// An sRGB colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

/// The colour roles of the interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ColorRole {
    Background,
    TextPrimary,
    TextSecondary,
    Error,
    Warning,
    Success,
    Accent,
}

impl ColorRole {
    pub const ALL: [ColorRole; 7] = [
        ColorRole::Background,
        ColorRole::TextPrimary,
        ColorRole::TextSecondary,
        ColorRole::Error,
        ColorRole::Warning,
        ColorRole::Success,
        ColorRole::Accent,
    ];

    /// Roles that carry text and must reach 4.5:1 against the background.
    pub const TEXT: [ColorRole; 5] =
        [ColorRole::TextPrimary, ColorRole::TextSecondary, ColorRole::Error, ColorRole::Warning, ColorRole::Success];

    /// Roles that draw interface parts (focus ring, selection, controls) and must reach 3:1.
    pub const GRAPHIC: [ColorRole; 1] = [ColorRole::Accent];
}

/// The colour of `role` in `appearance`.
pub fn color(role: ColorRole, appearance: Appearance) -> Rgb {
    use Appearance::{Dark, Light};
    use ColorRole as R;
    match (role, appearance) {
        (R::Background, Light) => Rgb(0xFF, 0xFF, 0xFF),
        (R::TextPrimary, Light) => Rgb(0x1D, 0x1D, 0x1F),
        (R::TextSecondary, Light) => Rgb(0x5A, 0x5A, 0x5E),
        (R::Error, Light) => Rgb(0xC0, 0x17, 0x1D),
        (R::Warning, Light) => Rgb(0x8A, 0x53, 0x00),
        (R::Success, Light) => Rgb(0x1E, 0x7B, 0x34),
        (R::Accent, Light) => Rgb(0x0A, 0x60, 0xD8),
        (R::Background, Dark) => Rgb(0x1E, 0x1E, 0x1E),
        (R::TextPrimary, Dark) => Rgb(0xF2, 0xF2, 0xF2),
        (R::TextSecondary, Dark) => Rgb(0xA8, 0xA8, 0xAD),
        (R::Error, Dark) => Rgb(0xFF, 0x6B, 0x6B),
        (R::Warning, Dark) => Rgb(0xFF, 0xB3, 0x40),
        (R::Success, Dark) => Rgb(0x5E, 0xD0, 0x7A),
        (R::Accent, Dark) => Rgb(0x4C, 0x9A, 0xFF),
    }
}

fn linear(channel: u8) -> f64 {
    let c = f64::from(channel) / 255.0;
    if c <= 0.039_28 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

fn luminance(c: Rgb) -> f64 {
    0.2126 * linear(c.0) + 0.7152 * linear(c.1) + 0.0722 * linear(c.2)
}

/// WCAG 2 contrast ratio of two colours (1 to 21).
pub fn contrast_ratio(a: Rgb, b: Rgb) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// How a status message looks: the colour is always accompanied by a symbol and a word.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StatusTone {
    Neutral,
    Warning,
    Error,
}

impl StatusTone {
    /// The colour role of the text.
    pub fn role(self) -> ColorRole {
        match self {
            StatusTone::Neutral => ColorRole::TextSecondary,
            StatusTone::Warning => ColorRole::Warning,
            StatusTone::Error => ColorRole::Error,
        }
    }

    /// The SF Symbol name (macOS); Windows draws `glyph` instead.
    pub fn symbol(self) -> &'static str {
        match self {
            StatusTone::Neutral => "info.circle",
            StatusTone::Warning => "exclamationmark.triangle",
            StatusTone::Error => "xmark.octagon",
        }
    }

    /// A text glyph that stands in for the symbol where no symbol font is available.
    pub fn glyph(self) -> &'static str {
        match self {
            StatusTone::Neutral => "ⓘ",
            StatusTone::Warning => "⚠",
            StatusTone::Error => "✖",
        }
    }

    /// The word screen readers get for the tone.
    pub fn name(self, nl: bool) -> &'static str {
        match (self, nl) {
            (StatusTone::Neutral, true) => "Melding",
            (StatusTone::Neutral, false) => "Message",
            (StatusTone::Warning, true) => "Waarschuwing",
            (StatusTone::Warning, false) => "Warning",
            (StatusTone::Error, true) => "Fout",
            (StatusTone::Error, false) => "Error",
        }
    }
}

/// The tone of a view-model message in the status line: only failures and refusals are an error;
/// outcomes that are uncertain (a machine whose state is unknown) are a warning; the rest is
/// information. A key not listed is an error, so a new failure is never shown as calm.
pub fn message_tone(key: MessageKey) -> StatusTone {
    use MessageKey as K;
    match key {
        K::Command(_)
        | K::UndoAction(_)
        | K::RedoAction(_)
        | K::Snap(_)
        | K::ExtendedStraight(_)
        | K::BoundTextFollowsGuide
        | K::NothingFound
        | K::NoDuplicatesFound
        | K::NothingToChange
        | K::NothingToRepair
        | K::StartRequestedAwaitingStatus => StatusTone::Neutral,
        K::StatusUnknownCheckMachine
        | K::StopSentOutcomeUnknown
        | K::UploadUnconfirmedCheckController
        | K::ControllerFileMayRemain
        | K::UploadedNotStoredConfirmed
        | K::SaveNotConfirmedDurable
        | K::PreviousVersionKept
        | K::HistoryTrimmed(_) => StatusTone::Warning,
        _ => StatusTone::Error,
    }
}

/// Spacing steps on the 4 pt grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Space {
    Hairline,
    Tight,
    Control,
    Group,
    Section,
    Page,
}

impl Space {
    pub const ALL: [Space; 6] = [Space::Hairline, Space::Tight, Space::Control, Space::Group, Space::Section, Space::Page];

    /// The distance in points.
    pub fn points(self) -> f64 {
        match self {
            Space::Hairline => 4.0,
            Space::Tight => 8.0,
            Space::Control => 12.0,
            Space::Group => 16.0,
            Space::Section => 24.0,
            Space::Page => 32.0,
        }
    }
}

/// Typography roles; a host maps them to its system font.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextRole {
    Title,
    Heading,
    Body,
    Label,
    Caption,
    Data,
}

impl TextRole {
    pub const ALL: [TextRole; 6] =
        [TextRole::Title, TextRole::Heading, TextRole::Body, TextRole::Label, TextRole::Caption, TextRole::Data];

    /// Size in points.
    pub fn size(self) -> f64 {
        match self {
            TextRole::Title => 17.0,
            TextRole::Heading => 14.0,
            TextRole::Body | TextRole::Label | TextRole::Data => 13.0,
            TextRole::Caption => 11.0,
        }
    }

    /// Whether the role is bold.
    pub fn bold(self) -> bool {
        matches!(self, TextRole::Title | TextRole::Heading)
    }

    /// Whether digits must line up (tabular figures / monospaced).
    pub fn tabular(self) -> bool {
        matches!(self, TextRole::Data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use burnburn_document::UndoLabel;

    #[test]
    fn contrast_follows_the_wcag_definition() {
        assert!((contrast_ratio(Rgb(0, 0, 0), Rgb(255, 255, 255)) - 21.0).abs() < 1e-9);
        assert!((contrast_ratio(Rgb(255, 255, 255), Rgb(255, 255, 255)) - 1.0).abs() < 1e-9);
        // Independent reference: #767676 on white is the well-known 4.54:1 boundary grey.
        assert!((contrast_ratio(Rgb(0x76, 0x76, 0x76), Rgb(255, 255, 255)) - 4.54).abs() < 0.01);
    }

    #[test]
    fn text_tokens_reach_4_5_to_1_on_the_background_in_both_appearances() {
        for appearance in [Appearance::Light, Appearance::Dark] {
            let bg = color(ColorRole::Background, appearance);
            for role in ColorRole::TEXT {
                let ratio = contrast_ratio(color(role, appearance), bg);
                assert!(ratio >= 4.5, "{role:?} {appearance:?}: {ratio:.2}");
            }
        }
    }

    #[test]
    fn interface_tokens_reach_3_to_1_on_the_background_in_both_appearances() {
        for appearance in [Appearance::Light, Appearance::Dark] {
            let bg = color(ColorRole::Background, appearance);
            for role in ColorRole::GRAPHIC {
                let ratio = contrast_ratio(color(role, appearance), bg);
                assert!(ratio >= 3.0, "{role:?} {appearance:?}: {ratio:.2}");
            }
        }
    }

    #[test]
    fn error_warning_and_success_are_not_told_apart_by_colour_alone() {
        let tones = [StatusTone::Neutral, StatusTone::Warning, StatusTone::Error];
        for tone in tones {
            assert!(!tone.symbol().is_empty() && !tone.glyph().is_empty() && !tone.name(true).is_empty(), "{tone:?}");
        }
        for (i, a) in tones.iter().enumerate() {
            for b in &tones[i + 1..] {
                assert_ne!(a.symbol(), b.symbol());
                assert_ne!(a.glyph(), b.glyph());
                assert_ne!(a.name(false), b.name(false));
            }
        }
        assert_eq!(StatusTone::Error.name(true), "Fout");
        assert_eq!(StatusTone::Error.name(false), "Error");
    }

    #[test]
    fn only_errors_are_red_and_ordinary_messages_are_secondary() {
        assert_eq!(StatusTone::Error.role(), ColorRole::Error);
        assert_eq!(StatusTone::Neutral.role(), ColorRole::TextSecondary);
        assert_eq!(StatusTone::Warning.role(), ColorRole::Warning);
    }

    #[test]
    fn messages_get_the_tone_of_what_happened() {
        assert_eq!(message_tone(MessageKey::SaveFailed), StatusTone::Error);
        assert_eq!(message_tone(MessageKey::CompileFailed), StatusTone::Error);
        assert_eq!(message_tone(MessageKey::CommandRejected), StatusTone::Error);
        assert_eq!(message_tone(MessageKey::UndoAction(UndoLabel::CreateRectangle)), StatusTone::Neutral);
        assert_eq!(message_tone(MessageKey::RedoAction(UndoLabel::CreateRectangle)), StatusTone::Neutral);
        assert_eq!(message_tone(MessageKey::NothingFound), StatusTone::Neutral);
        assert_eq!(message_tone(MessageKey::StatusUnknownCheckMachine), StatusTone::Warning);
        assert_eq!(message_tone(MessageKey::StopSentOutcomeUnknown), StatusTone::Warning);
    }

    #[test]
    fn spacing_sits_on_the_four_point_grid_and_grows() {
        let mut last = -1.0;
        for step in Space::ALL {
            let p = step.points();
            assert!(p > last, "{step:?} must be larger than the step before");
            assert_eq!(p % 4.0, 0.0, "{step:?} = {p} is off the 4 pt grid");
            last = p;
        }
    }

    #[test]
    fn typography_roles_form_a_readable_scale() {
        assert_eq!(TextRole::Body.size(), 13.0, "the macOS body size");
        assert!(TextRole::Caption.size() >= 11.0, "nothing smaller than 11 pt");
        assert!(TextRole::Title.size() > TextRole::Heading.size());
        assert!(TextRole::Heading.size() > TextRole::Body.size());
        assert!(TextRole::Body.size() > TextRole::Caption.size());
        assert!(TextRole::Heading.bold() && !TextRole::Body.bold());
        assert!(TextRole::Data.tabular() && !TextRole::Body.tabular());
    }
}
