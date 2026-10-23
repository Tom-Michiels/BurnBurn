//! SF Symbols per UI action (look-and-feel plan, LF8).
//!
//! One table names the SF Symbol of every generic action (arrows, play, lock, align, ...), with
//! an accessible description in Dutch and English. A button is styled with [`apply`]: the SF
//! Symbol when the system has it, else the matching master of `burnburn-icons`, else the
//! description as text. A Unicode glyph (a triangle, an arrow, a dot) is never a button title.
//!
//! The table and the descriptions need no AppKit; only [`image`] and the `apply*` functions do.

use objc2::rc::Retained;
use objc2_app_kit::{NSAccessibility, NSAppearanceCustomization, NSButton, NSCellImagePosition, NSImage};
use objc2_foundation::NSString;

use crate::l10n::Locale;

/// A generic action with a system symbol.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Symbol {
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    ArrowDown,
    OrderUp,
    OrderDown,
    Previous,
    Next,
    Disclosure,
    AnchorOff,
    AnchorOn,
    Play,
    Pause,
    Stop,
    Add,
    Remove,
    Trash,
    Info,
    Warning,
    Folder,
    Camera,
    Laser,
    Lock,
    Unlock,
    Eye,
    EyeHidden,
    MirrorHorizontal,
    MirrorVertical,
    RotateLeft,
    RotateRight,
    AlignLeft,
    AlignCenterHorizontal,
    AlignRight,
    AlignTop,
    AlignCenterVertical,
    AlignBottom,
    DistributeHorizontal,
    DistributeVertical,
    StatusOk,
    StatusError,
}

/// (SF Symbol name, fallback master in `burnburn-icons`, Dutch description, English description).
type Row = (&'static str, Option<&'static str>, &'static str, &'static str);

impl Symbol {
    /// Every symbol, in table order.
    pub const ALL: &'static [Symbol] = &[
        Symbol::ArrowLeft,
        Symbol::ArrowRight,
        Symbol::ArrowUp,
        Symbol::ArrowDown,
        Symbol::OrderUp,
        Symbol::OrderDown,
        Symbol::Previous,
        Symbol::Next,
        Symbol::Disclosure,
        Symbol::AnchorOff,
        Symbol::AnchorOn,
        Symbol::Play,
        Symbol::Pause,
        Symbol::Stop,
        Symbol::Add,
        Symbol::Remove,
        Symbol::Trash,
        Symbol::Info,
        Symbol::Warning,
        Symbol::Folder,
        Symbol::Camera,
        Symbol::Laser,
        Symbol::Lock,
        Symbol::Unlock,
        Symbol::Eye,
        Symbol::EyeHidden,
        Symbol::MirrorHorizontal,
        Symbol::MirrorVertical,
        Symbol::RotateLeft,
        Symbol::RotateRight,
        Symbol::AlignLeft,
        Symbol::AlignCenterHorizontal,
        Symbol::AlignRight,
        Symbol::AlignTop,
        Symbol::AlignCenterVertical,
        Symbol::AlignBottom,
        Symbol::DistributeHorizontal,
        Symbol::DistributeVertical,
        Symbol::StatusOk,
        Symbol::StatusError,
    ];

    fn row(self) -> Row {
        match self {
            Symbol::ArrowLeft => ("arrow.left", None, "Naar links", "Left"),
            Symbol::ArrowRight => ("arrow.right", None, "Naar rechts", "Right"),
            Symbol::ArrowUp => ("arrow.up", None, "Omhoog", "Up"),
            Symbol::ArrowDown => ("arrow.down", None, "Omlaag", "Down"),
            Symbol::OrderUp => ("chevron.up", None, "Eerder in de volgorde", "Earlier in the order"),
            Symbol::OrderDown => ("chevron.down", None, "Later in de volgorde", "Later in the order"),
            Symbol::Previous => ("chevron.left", None, "Vorige", "Previous"),
            Symbol::Next => ("chevron.right", None, "Volgende", "Next"),
            Symbol::Disclosure => ("chevron.compact.down", None, "Meer opties", "More options"),
            Symbol::AnchorOff => ("circle", None, "Niet gekozen", "Not selected"),
            Symbol::AnchorOn => ("circle.inset.filled", None, "Gekozen", "Selected"),
            Symbol::Play => ("play.fill", None, "Afspelen", "Play"),
            Symbol::Pause => ("pause.fill", None, "Pauzeren", "Pause"),
            Symbol::Stop => ("stop.fill", None, "Stoppen", "Stop"),
            Symbol::Add => ("plus", Some("new"), "Toevoegen", "Add"),
            Symbol::Remove => ("minus", None, "Verwijderen uit de lijst", "Remove from the list"),
            Symbol::Trash => ("trash", Some("delete"), "Verwijderen", "Delete"),
            Symbol::Info => ("info.circle", None, "Informatie", "Information"),
            Symbol::Warning => ("exclamationmark.triangle", None, "Waarschuwing", "Warning"),
            Symbol::Folder => ("folder", Some("open"), "Map", "Folder"),
            Symbol::Camera => ("camera", None, "Camera", "Camera"),
            Symbol::Laser => ("rays", None, "Laser", "Laser"),
            Symbol::Lock => ("lock", None, "Vergrendeld", "Locked"),
            Symbol::Unlock => ("lock.open", None, "Ontgrendeld", "Unlocked"),
            Symbol::Eye => ("eye", None, "Zichtbaar", "Visible"),
            Symbol::EyeHidden => ("eye.slash", None, "Verborgen", "Hidden"),
            Symbol::MirrorHorizontal => (
                "arrow.left.and.right.righttriangle.left.righttriangle.right",
                None,
                "Horizontaal spiegelen",
                "Flip horizontally",
            ),
            Symbol::MirrorVertical => (
                "arrow.up.and.down.righttriangle.up.righttriangle.down",
                None,
                "Verticaal spiegelen",
                "Flip vertically",
            ),
            Symbol::RotateLeft => ("rotate.left", None, "Linksom draaien", "Rotate counterclockwise"),
            Symbol::RotateRight => ("rotate.right", None, "Rechtsom draaien", "Rotate clockwise"),
            Symbol::AlignLeft => ("align.horizontal.left", None, "Links uitlijnen", "Align left"),
            Symbol::AlignCenterHorizontal => ("align.horizontal.center", None, "Horizontaal centreren", "Center horizontally"),
            Symbol::AlignRight => ("align.horizontal.right", None, "Rechts uitlijnen", "Align right"),
            Symbol::AlignTop => ("align.vertical.top", None, "Boven uitlijnen", "Align top"),
            Symbol::AlignCenterVertical => ("align.vertical.center", None, "Verticaal centreren", "Center vertically"),
            Symbol::AlignBottom => ("align.vertical.bottom", None, "Onder uitlijnen", "Align bottom"),
            Symbol::DistributeHorizontal => {
                ("distribute.horizontal.center", None, "Horizontaal verdelen", "Distribute horizontally")
            }
            Symbol::DistributeVertical => {
                ("distribute.vertical.center", None, "Verticaal verdelen", "Distribute vertically")
            }
            Symbol::StatusOk => ("checkmark.circle.fill", None, "In orde", "OK"),
            Symbol::StatusError => ("xmark.octagon.fill", None, "Fout", "Error"),
        }
    }

    /// The SF Symbol name.
    pub fn name(self) -> &'static str {
        self.row().0
    }

    /// The `burnburn-icons` master drawn when the system has no such symbol, if one fits.
    pub fn fallback_icon(self) -> Option<&'static str> {
        self.row().1
    }

    /// The accessible name and tooltip of the action.
    pub fn description(self, locale: Locale) -> &'static str {
        match locale {
            Locale::Nl => self.row().2,
            Locale::En => self.row().3,
        }
    }

    /// The text shown when neither the symbol nor a master exists: the description, never a glyph.
    pub fn fallback_text(self, locale: Locale) -> &'static str {
        self.description(locale)
    }
}

/// `text` without the Unicode glyphs that used to stand in for icons, and without the space they
/// leave behind ("◀ X−" becomes "X−").
pub fn without_glyphs(text: &str) -> String {
    const GLYPHS: [char; 17] = ['▾', '■', '↔', '↕', '◀', '▶', '▲', '▼', '●', '○', '‹', '›', '↑', '↓', '▸', '◂', '▴'];
    text.chars().filter(|c| !GLYPHS.contains(c)).collect::<String>().trim().to_owned()
}

/// The SF Symbol as a template image with its accessible description; `None` when this system
/// has no symbol of that name.
pub fn image(symbol: Symbol, locale: Locale) -> Option<Retained<NSImage>> {
    let description = NSString::from_str(symbol.description(locale));
    let image =
        NSImage::imageWithSystemSymbolName_accessibilityDescription(&NSString::from_str(symbol.name()), Some(&description))?;
    image.setTemplate(true);
    Some(image)
}

/// The symbol's image, or its `burnburn-icons` master when the system lacks the symbol.
fn image_or_master(symbol: Symbol, locale: Locale, dark: bool) -> Option<Retained<NSImage>> {
    image(symbol, locale).or_else(|| {
        let id = symbol.fallback_icon()?;
        let theme = if dark { burnburn_icons::Theme::Dark } else { burnburn_icons::Theme::Light };
        super::toolstrip::icon_image(id, 16, 2, theme)
    })
}

fn is_dark(button: &NSButton) -> bool {
    button.effectiveAppearance().name().to_string().contains("Dark")
}

/// An icon-only button for `symbol`: image, accessible name and tooltip. Without symbol and master
/// the button shows the description as its title.
pub fn apply(button: &NSButton, symbol: Symbol, locale: Locale) {
    let description = NSString::from_str(symbol.description(locale));
    button.setAccessibilityLabel(Some(&description));
    button.setToolTip(Some(&description));
    match image_or_master(symbol, locale, is_dark(button)) {
        Some(image) => {
            button.setTitle(&NSString::from_str(""));
            button.setImage(Some(&image));
            button.setImagePosition(NSCellImagePosition::ImageOnly);
        }
        None => {
            button.setImage(None);
            button.setImagePosition(NSCellImagePosition::NoImage);
            button.setTitle(&NSString::from_str(symbol.fallback_text(locale)));
        }
    }
}

/// A button with a title and the symbol beside it (`position`: leading or trailing image). The
/// title keeps its words; a glyph in it is dropped.
pub fn apply_with_title(button: &NSButton, symbol: Symbol, locale: Locale, title: &str, trailing: bool) {
    button.setTitle(&NSString::from_str(&without_glyphs(title)));
    button.setToolTip(Some(&NSString::from_str(symbol.description(locale))));
    if let Some(image) = image_or_master(symbol, locale, is_dark(button)) {
        button.setImage(Some(&image));
        button.setImagePosition(if trailing {
            NSCellImagePosition::ImageTrailing
        } else {
            NSCellImagePosition::ImageLeading
        });
    }
}

/// Swap the image of an already styled button (for a two-state symbol such as the job origin dot).
pub fn set_symbol(button: &NSButton, symbol: Symbol, locale: Locale) {
    if let Some(image) = image_or_master(symbol, locale, is_dark(button)) {
        button.setImage(Some(&image));
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn every_symbol_has_unique_name_and_both_descriptions() {
        let mut names = HashSet::new();
        for s in Symbol::ALL {
            assert!(names.insert(s.name()), "{s:?}: name used twice");
            assert!(!s.description(Locale::Nl).is_empty() && !s.description(Locale::En).is_empty(), "{s:?}");
        }
        assert_eq!(names.len(), Symbol::ALL.len());
        let variants: HashSet<_> = Symbol::ALL.iter().collect();
        assert_eq!(variants.len(), Symbol::ALL.len(), "a symbol is listed twice in ALL");
    }

    #[test]
    fn descriptions_never_hold_a_glyph() {
        for s in Symbol::ALL {
            for l in [Locale::Nl, Locale::En] {
                let d = s.description(l);
                assert_eq!(without_glyphs(d), d, "{s:?}");
            }
        }
    }

    #[test]
    fn without_glyphs_keeps_the_words() {
        assert_eq!(without_glyphs("◀ X−"), "X−");
        assert_eq!(without_glyphs("X+ ▶"), "X+");
        assert_eq!(without_glyphs("▲ Y+"), "Y+");
        assert_eq!(without_glyphs("Kopie"), "Kopie");
        assert_eq!(without_glyphs("▾"), "");
    }

    #[test]
    fn fallback_masters_exist() {
        for s in Symbol::ALL {
            if let Some(id) = s.fallback_icon() {
                assert!(burnburn_icons::has_icon(id), "{s:?}: no icon `{id}`");
            }
        }
    }
}
