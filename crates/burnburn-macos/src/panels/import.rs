//! The import preview section (P03).

use burnburn_io::import::{SvgUnitBasis, UnitDecision};

use burnburn_presentation::{AppViewModel, CommandKey, ImportUnit, format_mm};
use burnburn_types::DocumentId;

use crate::l10n::Locale;

/// Choices of the import preview's unit pop-up, top to bottom.
pub const IMPORT_UNITS: [ImportUnit; 3] = [ImportUnit::Millimeter, ImportUnit::Centimeter, ImportUnit::Inch];

/// The unit for pop-up item `index` (−1 = nothing chosen).
pub fn import_unit(index: isize) -> Option<ImportUnit> {
    IMPORT_UNITS.get(usize::try_from(index).ok()?).copied()
}

pub fn import_unit_name(unit: ImportUnit, locale: Locale) -> &'static str {
    match (unit, locale) {
        (ImportUnit::Millimeter, _) => "mm",
        (ImportUnit::Centimeter, _) => "cm",
        (ImportUnit::Inch, Locale::Nl) => "inch",
        (ImportUnit::Inch, Locale::En) => "inch",
    }
}

/// The import preview section (P03): file, unit decision, size, report, Importeer enablement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportPanel {
    pub file: String,
    pub unit_prompt: String,
    /// Pop-up item of the chosen unit (`None`: nothing chosen, the file's own unit applies).
    pub selected_unit: Option<usize>,
    pub size: String,
    pub report: String,
    pub commit_enabled: bool,
    /// The unit pop-up is for SVG and DXF only; the other formats know their unit.
    pub show_units: bool,
    /// PDF/AI: one item per page, then "all pages"; empty for every other format.
    pub pages: crate::panels_files::Popup,
    /// PDF/AI and plotter files that propose layers: the check "create an operation for each" and
    /// whether it is on; `None` for every other format (G7-trace, P04).
    pub layers: Option<bool>,
}

fn unit_label(mm_per_unit: f64) -> Option<&'static str> {
    [(1.0, "mm"), (10.0, "cm"), (25.4, "inch"), (1000.0, "m")]
        .iter()
        .find(|(f, _)| (mm_per_unit - f).abs() < 1e-9)
        .map(|(_, l)| *l)
}

pub fn import_panel(vm: &AppViewModel, document: DocumentId, locale: Locale) -> Option<ImportPanel> {
    let import = vm.document(document)?.import.as_ref()?;
    let nl = locale == Locale::Nl;
    let unit_prompt = if let Some(line) = crate::panels_files::import_unit_line(import, locale) {
        line
    } else if import.needs_unit {
        if nl {
            "Het bestand noemt geen eenheid: kies er een.".to_owned()
        } else {
            "The file names no unit: choose one.".to_owned()
        }
    } else {
        match import.units {
            Some(UnitDecision::DxfDeclared { mm_per_unit, .. }) => {
                let unit =
                    unit_label(mm_per_unit).map_or_else(|| format!("{} mm", format_mm(mm_per_unit)), str::to_owned);
                if nl { format!("Eenheid uit het bestand: {unit}") } else { format!("Unit from the file: {unit}") }
            }
            Some(UnitDecision::DxfExplicit { mm_per_unit }) => {
                let unit =
                    unit_label(mm_per_unit).map_or_else(|| format!("{} mm", format_mm(mm_per_unit)), str::to_owned);
                if nl { format!("Gekozen eenheid: {unit}") } else { format!("Chosen unit: {unit}") }
            }
            Some(UnitDecision::Svg { basis, .. }) => match (basis, nl) {
                (SvgUnitBasis::ViewBoxToPhysicalSize, true) => {
                    "Maat uit het SVG-bestand (viewBox en breedte/hoogte)".to_owned()
                }
                (SvgUnitBasis::ViewBoxToPhysicalSize, false) => {
                    "Size from the SVG file (viewBox and width/height)".to_owned()
                }
                (SvgUnitBasis::CssPixelsBySpec, true) => "SVG-pixels volgens de norm (96 per inch)".to_owned(),
                (SvgUnitBasis::CssPixelsBySpec, false) => "SVG pixels by the specification (96 per inch)".to_owned(),
                (_, true) => "Geen maat in het SVG-bestand: 96 eenheden per inch aangenomen".to_owned(),
                (_, false) => "No size in the SVG file: 96 units per inch assumed".to_owned(),
            },
            _ => String::new(),
        }
    };
    let size = match import.size_mm {
        Some((w, h)) => {
            if nl {
                format!("Maat: {} × {} mm", format_mm(w), format_mm(h))
            } else {
                format!("Size: {} × {} mm", format_mm(w), format_mm(h))
            }
        }
        None => {
            if nl {
                "Maat: onbekend tot je een eenheid kiest".to_owned()
            } else {
                "Size: unknown until you choose a unit".to_owned()
            }
        }
    };
    let skipped = import.skipped.len();
    let mut kinds: Vec<&str> = import.skipped.iter().map(|s| s.kind.as_str()).collect();
    kinds.sort_unstable();
    kinds.dedup();
    let open = import.open_contours;
    let report = if nl {
        let mut r = format!("{} objecten; {skipped} niet geïmporteerd", import.items);
        if !kinds.is_empty() {
            r.push_str(&format!(" ({})", kinds.join(", ")));
        }
        r.push_str(&format!("; {open} open {}", if open == 1 { "contour" } else { "contouren" }));
        r
    } else {
        let mut r = format!("{} objects; {skipped} not imported", import.items);
        if !kinds.is_empty() {
            r.push_str(&format!(" ({})", kinds.join(", ")));
        }
        r.push_str(&format!("; {open} open contour{}", if open == 1 { "" } else { "s" }));
        r
    };
    let details = crate::panels_files::import_details(import, locale);
    let report = if details.is_empty() { report } else { format!("{report}\n{details}") };
    let commit_enabled = vm.commands.get(&CommandKey::CommitImport).is_some_and(|c| c.enabled) && !import.needs_unit;
    Some(ImportPanel {
        file: import.file.clone(),
        unit_prompt,
        selected_unit: import.unit.and_then(|u| IMPORT_UNITS.iter().position(|x| *x == u)),
        size,
        report,
        commit_enabled,
        show_units: matches!(
            import.kind,
            burnburn_presentation::ImportKind::Svg | burnburn_presentation::ImportKind::Dxf
        ),
        pages: crate::panels_files::import_pages_popup(import, locale),
        layers: (!import.layers.is_empty()).then_some(import.layers_accepted),
    })
}
