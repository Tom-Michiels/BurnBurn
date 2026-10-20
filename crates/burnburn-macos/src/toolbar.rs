//! What the tool strip shows, without AppKit: labels, tooltips and panel titles from
//! `assets/l10n/tools.toml`, the reasons for disabled tools, and the stable mapping between a
//! catalog entry and the tag a native button carries.

use std::sync::OnceLock;

use burnburn_presentation::{CommandKey, DisabledReason, Enablement, MessageKey, PanelId, ToolCatalog, ToolSpec};

use crate::l10n::{Locale, message_or_key};

/// Labels, tooltips and panel titles (H-assets' file; the shortcuts live in the catalog).
const TOOLS_TOML: &str = include_str!("../../../assets/l10n/tools.toml");

fn texts() -> Option<&'static toml::Table> {
    static TEXTS: OnceLock<Option<toml::Table>> = OnceLock::new();
    TEXTS
        .get_or_init(|| match toml::from_str::<toml::Table>(TOOLS_TOML) {
            Ok(table) => Some(table),
            Err(e) => {
                eprintln!("burnburn: assets/l10n/tools.toml: {e}");
                None
            }
        })
        .as_ref()
}

fn lang(locale: Locale) -> &'static str {
    match locale {
        Locale::Nl => "nl",
        Locale::En => "en",
    }
}

fn tool_entry(id: &str) -> Option<&'static toml::Table> {
    texts()?.get("tools")?.as_table()?.get(id)?.as_table()
}

fn text_of(table: &toml::Table, key: &str) -> Option<String> {
    table.get(key)?.as_str().map(str::trim).filter(|t| !t.is_empty()).map(str::to_owned)
}

/// The catalog entry behind a native control's tag (its index in the register).
pub fn spec_for_tag(tag: isize) -> Option<&'static ToolSpec> {
    ToolCatalog::standard().specs().get(usize::try_from(tag).ok()?)
}

/// The tag a native control for `id` carries.
pub fn tag_for_id(id: &str) -> Option<isize> {
    ToolCatalog::standard().specs().iter().position(|s| s.id == id).map(|i| i as isize)
}

/// Short name of a tool. A tool the text file does not list yet (pen, polyline, bounding-box ellipse) shows
/// its command's name; never the raw id.
pub fn tool_label(id: &str, locale: Locale) -> String {
    if let Some(label) = tool_entry(id).and_then(|t| text_of(t, lang(locale))) {
        return label;
    }
    let command = match id {
        "sketch.pen" => Some(CommandKey::ToolPen),
        "sketch.polyline" => Some(CommandKey::ToolPolyline),
        "sketch.ellipse.box" => Some(CommandKey::ToolEllipse),
        _ => None,
    };
    match command {
        Some(c) => message_or_key(MessageKey::Command(c), locale),
        None => id.rsplit('.').next().unwrap_or(id).replace('_', " "),
    }
}

/// What the tool does and how to use it; `None` when the text file has none.
pub fn tool_tip(id: &str, locale: Locale) -> Option<String> {
    text_of(tool_entry(id)?, &format!("tip_{}", lang(locale)))
}

/// Title of a toolbar panel.
pub fn panel_title(panel: PanelId, locale: Locale) -> String {
    texts()
        .and_then(|t| t.get("panels")?.as_table()?.get(panel.key())?.as_table().and_then(|p| text_of(p, lang(locale))))
        .unwrap_or_else(|| panel.key().to_uppercase())
}

/// Why a tool is disabled, in words.
pub fn reason_text(reason: DisabledReason, locale: Locale) -> &'static str {
    let (nl, en) = match reason {
        DisabledReason::NotAvailable => ("Nog niet beschikbaar", "Not available yet"),
        DisabledReason::Follows => {
            ("Volgt: wacht op een atomair batchcommando", "Follows: waits for an atomic batch command")
        }
        DisabledReason::NeedsSketch => ("Alleen tijdens het bewerken van een schets", "Only while editing a sketch"),
        DisabledReason::NoDocument => ("Open eerst een document", "Open a document first"),
        DisabledReason::OperatorRole => {
            ("Niet beschikbaar in de operatorweergave", "Not available in the operator view")
        }
        DisabledReason::NeedsSelection => ("Selecteer eerst passende vormen", "Select suitable shapes first"),
        DisabledReason::NeedsNodes => {
            ("Kies het knooppuntgereedschap en selecteer knooppunten", "Use the node tool and select nodes")
        }
    };
    match locale {
        Locale::Nl => nl,
        Locale::En => en,
    }
}

/// Tooltip: "Label (shortcut) — explanation", or "Label — reason" for a disabled tool.
pub fn tooltip(id: &str, shortcut: Option<&str>, enablement: Enablement, locale: Locale) -> String {
    let label = tool_label(id, locale);
    match enablement {
        Enablement::Disabled(reason) => format!("{label} — {}", reason_text(reason, locale)),
        Enablement::Enabled => {
            let head = match shortcut {
                Some(key) => format!("{label} ({key})"),
                None => label,
            };
            match tool_tip(id, locale) {
                Some(tip) => format!("{head} — {tip}"),
                None => head,
            }
        }
    }
}

/// The readout of the measuring tool.
pub fn measurement_text(m: &burnburn_presentation::ToolMeasurement, locale: Locale) -> String {
    let word = match locale {
        Locale::Nl => "Afstand",
        Locale::En => "Distance",
    };
    format!("{word} {} mm (dx {} mm, dy {} mm)", m.distance_text, m.dx_text, m.dy_text)
}
