//! The text panels (W3-UI-tekst) as plain data: what the text toolbar, the text-on-path sheet, the
//! variable-text panel and the barcode dialog show from presentation's view models, and which
//! `UiIntent` each control becomes. No AppKit here, so all of it is tested headlessly; the widgets
//! in `appkit/panels/{text,variable,barcode}.rs` only copy these models and report a control as a
//! [`Slot`] with a [`SlotValue`].
//!
//! Panels show and forward: numbers go to presentation as typed (DR-05), a control that cannot be
//! used is off with its reason as tooltip, a type without a verified encoder is greyed out with the
//! reason, and nothing is rounded or substituted here.

use burnburn_presentation::{
    AdvancePolicy, Align, AppViewModel, BarcodeDialogViewModel, BarcodeEdit, BarcodeField, BarcodeIssue,
    BarcodeProblem, CommandKey, Distort, SerialField, SymbolInputMode, TextEdit, TextField, TextIssue,
    TextOnPathViewModel, TextPathEdit, TextToggle, TextToolbarViewModel, UiIntent, VAlign, VariableEdit, VariableIssue,
    VariablePanelViewModel, VariableProblem,
};
use burnburn_types::DocumentId;

use crate::l10n::Locale;

/// Every control of the four panels. The widgets carry the slot as their tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Slot {
    Family,
    Bold,
    Italic,
    Uppercase,
    Welded,
    SingleLine,
    Align,
    VAlign,
    HSpace,
    VSpace,
    Height,
    Convert,
    PathDistort,
    PathStart,
    PathOffset,
    PathReverse,
    PathApply,
    PathCancel,
    VarKind,
    VarOffset,
    SerialCurrent,
    SerialStart,
    SerialEnd,
    SerialBy,
    SerialDigits,
    SerialWrap,
    VarPolicy,
    VarPrevious,
    VarNext,
    VarReset,
    VarRow,
    VarCsv,
    VarClearCsv,
    VarTest,
    VarBake,
    BcType,
    BcPayload,
    BcModule,
    BcMinModule,
    BcQuiet,
    BcHeight,
    BcEcc,
    BcInputMode,
    BcColumns,
    BcWidth,
    BcCreate,
    BcCancel,
}

impl Slot {
    pub const ALL: [Slot; 47] = [
        Slot::Family,
        Slot::Bold,
        Slot::Italic,
        Slot::Uppercase,
        Slot::Welded,
        Slot::SingleLine,
        Slot::Align,
        Slot::VAlign,
        Slot::HSpace,
        Slot::VSpace,
        Slot::Height,
        Slot::Convert,
        Slot::PathDistort,
        Slot::PathStart,
        Slot::PathOffset,
        Slot::PathReverse,
        Slot::PathApply,
        Slot::PathCancel,
        Slot::VarKind,
        Slot::VarOffset,
        Slot::SerialCurrent,
        Slot::SerialStart,
        Slot::SerialEnd,
        Slot::SerialBy,
        Slot::SerialDigits,
        Slot::SerialWrap,
        Slot::VarPolicy,
        Slot::VarPrevious,
        Slot::VarNext,
        Slot::VarReset,
        Slot::VarRow,
        Slot::VarCsv,
        Slot::VarClearCsv,
        Slot::VarTest,
        Slot::VarBake,
        Slot::BcType,
        Slot::BcPayload,
        Slot::BcModule,
        Slot::BcMinModule,
        Slot::BcQuiet,
        Slot::BcHeight,
        Slot::BcEcc,
        Slot::BcInputMode,
        Slot::BcColumns,
        Slot::BcWidth,
        Slot::BcCreate,
        Slot::BcCancel,
    ];

    /// How the control reports itself.
    pub fn kind(self) -> SlotKind {
        use Slot::*;
        match self {
            Height | HSpace | VSpace | PathStart | PathOffset | VarOffset | SerialCurrent | SerialStart | SerialEnd
            | SerialBy | SerialDigits | VarRow | BcPayload | BcModule | BcMinModule | BcQuiet | BcHeight
            | BcColumns | BcWidth => SlotKind::Text,
            Bold | Italic | Uppercase | Welded | SingleLine | PathReverse | SerialWrap => SlotKind::Toggle,
            Family | Align | VAlign | PathDistort | VarKind | VarPolicy | BcType | BcEcc | BcInputMode => {
                SlotKind::Choice
            }
            VarCsv => SlotKind::Csv,
            Convert | PathApply | PathCancel | VarPrevious | VarNext | VarReset | VarClearCsv | VarTest | VarBake
            | BcCreate | BcCancel => SlotKind::Press,
        }
    }

    /// The widget tag of this slot (never 0: AppKit's default tag).
    pub fn tag(self) -> isize {
        Slot::ALL.iter().position(|s| *s == self).map_or(0, |i| i as isize + 1)
    }

    pub fn from_tag(tag: isize) -> Option<Slot> {
        usize::try_from(tag.checked_sub(1)?).ok().and_then(|i| Slot::ALL.get(i).copied())
    }
}

/// What a slot's control reports to the host's one action method.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotKind {
    /// A text field: its string.
    Text,
    /// A checkbox: its state.
    Toggle,
    /// A pop-up or segmented control: the chosen index.
    Choice,
    /// A push button.
    Press,
    /// The CSV button: the host opens a file panel and reports the file.
    Csv,
}

/// What a control reports: the text of a field, a checkbox, a pop-up/segment index, a button press,
/// or the file the CSV panel returned (name and bytes; `None` when cancelled).
#[derive(Debug, Clone, PartialEq)]
pub enum SlotValue {
    Text(String),
    Toggle(bool),
    Choice(usize),
    Press,
    Csv(Option<(String, Vec<u8>)>),
}

/// A checkbox: its state, whether it can be clicked, and why not.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Check {
    pub on: bool,
    pub enabled: bool,
    pub tip: String,
}

/// The text toolbar for the selected text.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ToolbarPanel {
    pub families: Vec<String>,
    pub family: usize,
    pub bold: Check,
    pub italic: Check,
    pub uppercase: bool,
    pub welded: Check,
    pub single_line: bool,
    pub align: usize,
    pub valign: usize,
    pub h_space: String,
    pub v_space: String,
    pub height: String,
    /// The reason of the last refusal, missing characters or a missing font; empty when fine.
    pub note: String,
    pub convert_enabled: bool,
}

/// The text-on-path sheet.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PathPanel {
    pub distort: usize,
    pub start: String,
    pub offset: String,
    pub reverse: bool,
    pub apply_enabled: bool,
    /// "Select one text and one path" while the selection does not fit, else the last refusal.
    pub note: String,
}

/// The variable-text panel.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct VariablePanel {
    pub variable: bool,
    pub offset: String,
    pub current: String,
    pub start: String,
    pub end: String,
    pub advance_by: String,
    pub digits: String,
    pub wrap: bool,
    pub policy: usize,
    pub csv_line: String,
    pub csv_loaded: bool,
    pub row: String,
    /// What the selected text prints now, or the red reason it cannot.
    pub preview: String,
    pub preview_is_error: bool,
    /// The lines of the latest Test, one per variable text.
    pub test: String,
    pub problem: String,
    pub bound: String,
}

/// One entry of the barcode type pop-up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeItem {
    pub title: String,
    pub enabled: bool,
    pub tip: String,
}

/// The barcode sheet.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BarcodePanel {
    pub types: Vec<TypeItem>,
    pub symbology: usize,
    pub payload: String,
    pub payload_hint: String,
    /// "Maak", or "Pas toe" while a generated barcode is changed.
    pub create_title: String,
    pub module: String,
    pub min_module: String,
    pub quiet: String,
    pub quiet_hint: String,
    pub height: Option<String>,
    pub ecc: Option<(Vec<String>, usize)>,
    /// Input modes by title and the chosen one; only when the type has more than one.
    pub input_modes: Option<(Vec<String>, usize)>,
    pub columns: Option<String>,
    pub width: String,
    pub size_line: String,
    pub canonical_line: String,
    pub problems: String,
    pub readability: String,
    pub create_enabled: bool,
}

/// The four panels of one window.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TextPanels {
    pub toolbar: Option<ToolbarPanel>,
    pub path: Option<PathPanel>,
    pub variable: Option<VariablePanel>,
    pub barcode: Option<BarcodePanel>,
}

/// Alignment choices, in the order of the control's segments.
pub const ALIGNS: [Align; 3] = [Align::Left, Align::Center, Align::Right];
/// Vertical alignment choices, in the order of the pop-up.
pub const VALIGNS: [VAlign; 4] = [VAlign::Baseline, VAlign::Top, VAlign::Middle, VAlign::Bottom];
/// Auto-advance choices, in the order of the pop-up.
pub const POLICIES: [AdvancePolicy; 3] =
    [AdvancePolicy::Manual, AdvancePolicy::OnUploadConfirmed, AdvancePolicy::OnJobStarted];

/// Static captions of the four panels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextText {
    ToolbarTitle,
    Font,
    Bold,
    Italic,
    Uppercase,
    Welded,
    SingleLine,
    Alignment,
    AlignLeft,
    AlignCenter,
    AlignRight,
    Vertical,
    VBaseline,
    VTop,
    VMiddle,
    VBottom,
    HSpace,
    VSpace,
    Height,
    Convert,
    MenuText,
    PathTitle,
    Distort,
    DistortRigid,
    DistortWarp,
    PathStart,
    PathOffset,
    PathReverse,
    Apply,
    Cancel,
    VariableTitle,
    Kind,
    KindNormal,
    KindVariable,
    VarOffset,
    SerialCurrent,
    SerialStart,
    SerialEnd,
    SerialBy,
    SerialDigits,
    SerialWrap,
    AutoAdvance,
    PolicyManual,
    PolicyUpload,
    PolicyStart,
    Previous,
    Next,
    Reset,
    Test,
    Bake,
    ChooseCsv,
    ClearCsv,
    Row,
    Preview,
    BarcodeTitle,
    BarcodeType,
    Payload,
    Module,
    MinModule,
    Quiet,
    BarHeight,
    Ecc,
    InputMode,
    Columns,
    Width,
    Create,
}

fn pair(text: TextText) -> (&'static str, &'static str) {
    use TextText as T;
    match text {
        T::ToolbarTitle => ("Tekst", "Text"),
        T::Font => ("Lettertype", "Font"),
        T::Bold => ("Vet", "Bold"),
        T::Italic => ("Cursief", "Italic"),
        T::Uppercase => ("Hoofdletters", "Uppercase"),
        T::Welded => ("Gelast", "Welded"),
        T::SingleLine => ("Enkele lijn", "Single line"),
        T::Alignment => ("Uitlijning", "Alignment"),
        T::AlignLeft => ("Links", "Left"),
        T::AlignCenter => ("Midden", "Centre"),
        T::AlignRight => ("Rechts", "Right"),
        T::Vertical => ("Verticaal", "Vertical"),
        T::VBaseline => ("Basislijn", "Baseline"),
        T::VTop => ("Boven", "Top"),
        T::VMiddle => ("Midden", "Middle"),
        T::VBottom => ("Onder", "Bottom"),
        T::HSpace => ("Letterafstand (mm)", "Letter spacing (mm)"),
        T::VSpace => ("Regelafstand (mm)", "Line spacing (mm)"),
        T::Height => ("Hoogte (mm)", "Height (mm)"),
        T::Convert => ("Omzetten naar pad", "Convert to Path"),
        T::MenuText => ("Tekst", "Text"),
        T::PathTitle => ("Tekst op pad", "Text on a Path"),
        T::Distort => ("Vervormen", "Distort"),
        T::DistortRigid => ("Star: letters houden hun vorm", "Rigid: letters keep their shape"),
        T::DistortWarp => ("Buigen: contouren buigen mee", "Warp: outlines bend along"),
        T::PathStart => ("Start op pad (mm)", "Start on the path (mm)"),
        T::PathOffset => ("Afstand tot pad (mm)", "Distance from the path (mm)"),
        T::PathReverse => ("Pad omkeren", "Reverse the path"),
        T::Apply => ("Toepassen", "Apply"),
        T::Cancel => ("Annuleer", "Cancel"),
        T::VariableTitle => ("Variabele tekst", "Variable text"),
        T::Kind => ("Type", "Type"),
        T::KindNormal => ("Normaal", "Normal"),
        T::KindVariable => ("Variabel", "Variable"),
        T::VarOffset => ("Offset in de reeks", "Offset in the series"),
        T::SerialCurrent => ("Huidig", "Current"),
        T::SerialStart => ("Start", "Start"),
        T::SerialEnd => ("Einde", "End"),
        T::SerialBy => ("Stap", "Advance by"),
        T::SerialDigits => ("Cijfers", "Digits"),
        T::SerialWrap => ("Na het einde opnieuw beginnen", "Start over after the end"),
        T::AutoAdvance => ("Automatisch ophogen", "Auto-advance"),
        T::PolicyManual => ("Handmatig", "Manual"),
        T::PolicyUpload => ("Na bevestigde upload", "After a confirmed upload"),
        T::PolicyStart => ("Na start van de job", "After the job starts"),
        T::Previous => ("Vorige", "Previous"),
        T::Next => ("Volgende", "Next"),
        T::Reset => ("Herstel", "Reset"),
        T::Test => ("Test", "Test"),
        T::Bake => ("Vastleggen", "Bake"),
        T::ChooseCsv => ("Kies CSV…", "Choose CSV…"),
        T::ClearCsv => ("Wis CSV", "Clear CSV"),
        T::Row => ("Rij", "Row"),
        T::Preview => ("Voorbeeld", "Preview"),
        T::BarcodeTitle => ("Streepjescode", "Bar Code"),
        T::BarcodeType => ("Type", "Type"),
        T::Payload => ("Inhoud", "Content"),
        T::Module => ("Modulegrootte (mm)", "Module size (mm)"),
        T::MinModule => ("Kleinste leesbare module (mm)", "Smallest readable module (mm)"),
        T::Quiet => ("Stille zone (modules)", "Quiet zone (modules)"),
        T::BarHeight => ("Hoogte (mm)", "Height (mm)"),
        T::Ecc => ("Foutcorrectie", "Error correction"),
        T::InputMode => ("Invoermodus", "Input mode"),
        T::Columns => ("Kolommen", "Columns"),
        T::Width => ("Breedte (mm)", "Width (mm)"),
        T::Create => ("Maak", "Create"),
    }
}

/// A caption in the window language.
pub fn text_text(text: TextText, locale: Locale) -> &'static str {
    let (nl, en) = pair(text);
    match locale {
        Locale::Nl => nl,
        Locale::En => en,
    }
}

fn tr(nl: &str, en: &str, locale: Locale) -> String {
    match locale {
        Locale::Nl => nl.to_owned(),
        Locale::En => en.to_owned(),
    }
}

fn text_field_name(field: TextField, locale: Locale) -> String {
    match field {
        TextField::HSpace => tr("Letterafstand", "Letter spacing", locale),
        TextField::VSpace => tr("Regelafstand", "Line spacing", locale),
        TextField::Height => tr("Hoogte", "Height", locale),
        TextField::PathStart => tr("Start op pad", "Start on the path", locale),
        _ => tr("Afstand tot pad", "Distance from the path", locale),
    }
}

/// The sentence for a toolbar or text-on-path refusal.
pub fn text_issue_text(issue: TextIssue, locale: Locale) -> String {
    match issue {
        TextIssue::InvalidNumber(f) => {
            let name = text_field_name(f, locale);
            match locale {
                Locale::Nl => format!("{name} is geen geldig getal"),
                Locale::En => format!("{name} is not a valid number"),
            }
        }
        TextIssue::StyleMissing => tr("Dit lettertype mist die stijl", "The font lacks that style", locale),
        TextIssue::FamilyUnavailable => tr("Lettertype niet beschikbaar", "Font not available", locale),
        TextIssue::WeldedNeedsOutline => tr(
            "Gelast werkt alleen met omtrektekst, niet met enkele lijn",
            "Welded needs outline text, not single-line text",
            locale,
        ),
        TextIssue::Refused => tr("Het document weigerde de wijziging", "The document refused the change", locale),
        TextIssue::NothingToConvert => tr("Geen omtrekken om om te zetten", "No outlines to convert", locale),
        TextIssue::PathNeedsTextAndGuide => {
            tr("Selecteer één tekst en één pad", "Select one text and one path", locale)
        }
        TextIssue::PathMultiline => {
            tr("Tekst op pad werkt met één regel", "Text on a path needs a single line", locale)
        }
        TextIssue::PathTextTooLong => tr("De tekst is langer dan het pad", "The text is longer than the path", locale),
        TextIssue::PathEmptyGuide => tr("Het pad heeft geen lengte", "The path has no length", locale),
        TextIssue::PathFailed => tr("Tekst op pad is mislukt", "Text on a path failed", locale),
        TextIssue::NoFonts => tr("Geen lettertype beschikbaar", "No font available", locale),
        _ => tr("Niet mogelijk", "Not possible", locale),
    }
}

fn check(t: &TextToggle, missing: TextIssue, tip_for: Option<&str>, locale: Locale) -> Check {
    let tip = match (t.enabled, t.why_off) {
        (true, _) => String::new(),
        (false, Some(TextIssue::StyleMissing)) => {
            tip_for.map_or_else(|| text_issue_text(missing, locale), str::to_owned)
        }
        (false, Some(why)) => text_issue_text(why, locale),
        (false, None) => String::new(),
    };
    Check { on: t.on, enabled: t.enabled, tip }
}

fn toolbar_panel(t: &TextToolbarViewModel, locale: Locale) -> ToolbarPanel {
    let note = if let Some(issue) = t.issue {
        text_issue_text(issue, locale)
    } else if let Some(choice) = t.missing_font.as_ref() {
        match locale {
            Locale::Nl => format!(
                "Lettertype \u{201c}{}\u{201d} is niet geïnstalleerd; de opgeslagen omtrekken blijven. Kies een vervanger in de lijst ({} beschikbaar).",
                choice.requested,
                choice.replacements.len()
            ),
            Locale::En => format!(
                "Font \u{201c}{}\u{201d} is not installed; the stored outlines stay. Choose a replacement from the list ({} available).",
                choice.requested,
                choice.replacements.len()
            ),
        }
    } else if t.font_missing {
        tr(
            "Lettertype niet beschikbaar: de laatste omtrekken blijven",
            "Font not available: the last outlines stay",
            locale,
        )
    } else if t.missing_glyphs > 0 {
        match locale {
            Locale::Nl => format!("{} teken(s) ontbreken in het lettertype", t.missing_glyphs),
            Locale::En => format!("{} character(s) missing from the font", t.missing_glyphs),
        }
    } else {
        String::new()
    };
    let bold_tip = tr("Dit lettertype heeft geen vette variant", "This font has no bold face", locale);
    let italic_tip = tr("Dit lettertype heeft geen cursieve variant", "This font has no italic face", locale);
    ToolbarPanel {
        families: t.families.to_vec(),
        family: t.families.iter().position(|f| *f == t.family).unwrap_or(0),
        bold: check(&t.bold, TextIssue::StyleMissing, Some(&bold_tip), locale),
        italic: check(&t.italic, TextIssue::StyleMissing, Some(&italic_tip), locale),
        uppercase: t.uppercase,
        welded: check(&t.welded, TextIssue::WeldedNeedsOutline, None, locale),
        single_line: t.single_line,
        align: ALIGNS.iter().position(|a| *a == t.align).unwrap_or(0),
        valign: VALIGNS.iter().position(|a| *a == t.valign).unwrap_or(0),
        h_space: t.h_space.clone(),
        v_space: t.v_space.clone(),
        height: t.height.clone(),
        note,
        convert_enabled: t.can_convert,
    }
}

fn path_panel(p: &TextOnPathViewModel, locale: Locale) -> PathPanel {
    let note = match (p.issue, p.ready) {
        (Some(issue), _) => text_issue_text(issue, locale),
        (None, false) => text_issue_text(TextIssue::PathNeedsTextAndGuide, locale),
        (None, true) => String::new(),
    };
    PathPanel {
        distort: usize::from(p.distort == Distort::Warp),
        start: p.start.clone(),
        offset: p.offset.clone(),
        reverse: p.reverse,
        apply_enabled: p.ready,
        note,
    }
}

fn serial_field_name(field: SerialField, locale: Locale) -> &'static str {
    let t = match field {
        SerialField::Current => TextText::SerialCurrent,
        SerialField::Start => TextText::SerialStart,
        SerialField::End => TextText::SerialEnd,
        SerialField::AdvanceBy => TextText::SerialBy,
        SerialField::Digits => TextText::SerialDigits,
    };
    text_text(t, locale)
}

/// The sentence for a variable-text problem, with the place it was found.
pub fn variable_problem_text(problem: &VariableProblem, locale: Locale) -> String {
    let d = problem.detail.as_str();
    match problem.issue {
        VariableIssue::InvalidNumber(f) => {
            let name = serial_field_name(f, locale);
            match locale {
                Locale::Nl => format!("{name} is geen geheel getal"),
                Locale::En => format!("{name} is not a whole number"),
            }
        }
        VariableIssue::InvalidSerial => {
            tr("Start, einde en stap vormen geen reeks", "Start, end and step do not form a series", locale)
        }
        VariableIssue::NotInSeries => {
            tr("De huidige waarde hoort niet bij de reeks", "The current value is not in the series", locale)
        }
        VariableIssue::SerialExhausted => tr("De reeks is afgelopen", "The series has reached its end", locale),
        VariableIssue::InvalidOffset => tr("Offset is geen geheel getal", "Offset is not a whole number", locale),
        VariableIssue::RowOutOfRange => match locale {
            Locale::Nl => format!("Kies een rij van 1 tot het aantal rijen ({d})"),
            Locale::En => format!("Choose a row from 1 to the number of rows ({d})"),
        },
        VariableIssue::CsvUnreadable => tr(
            "Het CSV-bestand is niet te lezen (UTF-8 met gelijke kolommen nodig)",
            "The CSV file cannot be read (UTF-8 with equal columns needed)",
            locale,
        ),
        VariableIssue::MissingColumn => match locale {
            Locale::Nl => format!("Kolom '{d}' staat niet in de gegevens"),
            Locale::En => format!("The data has no column '{d}'"),
        },
        VariableIssue::MissingValue => match locale {
            Locale::Nl => format!("Waarde ontbreekt in kolom (rij): {d}"),
            Locale::En => format!("Value missing in column (row): {d}"),
        },
        VariableIssue::NoData => tr("Geen CSV-gegevens gekozen: kies een CSV", "No data chosen: choose a CSV", locale),
        VariableIssue::NoSerial => tr("Geen serienummer ingesteld", "No serial number set up", locale),
        VariableIssue::BadTemplate => match locale {
            Locale::Nl => format!("De tekst bevat een onbekend of onvolledig veld ({d})"),
            Locale::En => format!("The text has an unknown or unfinished field ({d})"),
        },
        VariableIssue::NoCutValue => match locale {
            Locale::Nl => format!("Geen waarde bekend voor {d}"),
            Locale::En => format!("No value known for {d}"),
        },
        VariableIssue::NoClock => tr("Geen klok beschikbaar voor {date}", "No clock available for {date}", locale),
        VariableIssue::NoFonts => tr("Geen lettertype beschikbaar", "No font available", locale),
        VariableIssue::FontMissing => match locale {
            Locale::Nl => format!("Lettertype ontbreekt: {d}"),
            Locale::En => format!("Font missing: {d}"),
        },
        VariableIssue::Refused => tr("Het document weigerde de wijziging", "The document refused the change", locale),
        VariableIssue::StaleJob => tr(
            "De tellers zijn verzet na het klaarzetten van de job",
            "The counters moved after the job was prepared",
            locale,
        ),
        _ => tr("Niet mogelijk", "Not possible", locale),
    }
}

fn variable_panel(v: &VariablePanelViewModel, locale: Locale) -> VariablePanel {
    let (preview, preview_is_error) = match &v.preview {
        Some(Ok(text)) => (text.clone(), false),
        Some(Err(p)) => (variable_problem_text(p, locale), true),
        None => (String::new(), false),
    };
    let test = v.test.as_ref().map_or_else(String::new, |lines| {
        lines
            .iter()
            .map(|l| match &l.result {
                Ok(text) => format!("{} → {text}", l.template),
                Err(p) => format!("{} → {}", l.template, variable_problem_text(p, locale)),
            })
            .collect::<Vec<_>>()
            .join("\n")
    });
    VariablePanel {
        variable: v.variable,
        offset: v.offset.clone(),
        current: v.serial.current.clone(),
        start: v.serial.start.clone(),
        end: v.serial.end.clone(),
        advance_by: v.serial.advance_by.clone(),
        digits: v.serial.digits.clone(),
        wrap: v.serial.wrap,
        policy: POLICIES.iter().position(|p| *p == v.policy).unwrap_or(0),
        csv_line: v.csv.as_ref().map_or_else(String::new, |c| match locale {
            Locale::Nl => format!("{} — {} rijen", c.name, c.rows),
            Locale::En => format!("{} — {} rows", c.name, c.rows),
        }),
        csv_loaded: v.csv.is_some(),
        row: v.csv.as_ref().map_or_else(String::new, |c| c.row.clone()),
        preview,
        preview_is_error,
        test,
        problem: v.problem.as_ref().map_or_else(String::new, |p| variable_problem_text(p, locale)),
        bound: v.bound_record.as_ref().map_or_else(String::new, |id| {
            let short: String = id.chars().take(8).collect();
            match locale {
                Locale::Nl => format!("Gebonden aan jobrecord {short}"),
                Locale::En => format!("Bound to job record {short}"),
            }
        }),
    }
}

fn barcode_field_name(field: BarcodeField, locale: Locale) -> &'static str {
    let t = match field {
        BarcodeField::Payload => TextText::Payload,
        BarcodeField::Module => TextText::Module,
        BarcodeField::MinModule => TextText::MinModule,
        BarcodeField::Quiet => TextText::Quiet,
        BarcodeField::Height => TextText::BarHeight,
        BarcodeField::Ecc => TextText::Ecc,
        BarcodeField::InputMode => TextText::InputMode,
        BarcodeField::Columns => TextText::Columns,
        BarcodeField::Width => TextText::Width,
    };
    text_text(t, locale)
}

/// The sentence for a barcode refusal; positions of characters count from 1.
pub fn barcode_problem_text(problem: &BarcodeProblem, locale: Locale) -> String {
    let field = barcode_field_name(problem.field, locale);
    match problem.issue {
        BarcodeIssue::InvalidNumber => match locale {
            Locale::Nl => format!("{field}: geen geldig getal"),
            Locale::En => format!("{field}: not a valid number"),
        },
        BarcodeIssue::Unsupported => tr(
            "Dit type heeft geen geverifieerde encoder: er wordt geen patroon gemaakt",
            "This type has no verified encoder: no pattern is made",
            locale,
        ),
        BarcodeIssue::EmptyPayload => tr("Inhoud ontbreekt", "Content is missing", locale),
        BarcodeIssue::PayloadLength { min, max, len } => match locale {
            Locale::Nl => format!("De inhoud heeft {len} tekens; {min} tot {max} zijn toegestaan"),
            Locale::En => format!("The content has {len} characters; {min} to {max} are allowed"),
        },
        BarcodeIssue::InvalidChar { ch, index } => match locale {
            Locale::Nl => format!("Teken '{ch}' op positie {} kan niet worden gecodeerd", index + 1),
            Locale::En => format!("Character '{ch}' at position {} cannot be encoded", index + 1),
        },
        BarcodeIssue::CheckDigit { expected, found } => match locale {
            Locale::Nl => format!("Controlecijfer klopt niet: {found} moet {expected} zijn"),
            Locale::En => format!("Check digit is wrong: {found} should be {expected}"),
        },
        BarcodeIssue::InvalidPayload { rule } => rule.to_owned(),
        BarcodeIssue::UnknownAi => {
            tr("Onbekende GS1-toepassingsidentificatie", "Unknown GS1 application identifier", locale)
        }
        BarcodeIssue::DataTooLong => {
            tr("De inhoud past niet in het grootste symbool", "The content does not fit the largest symbol", locale)
        }
        BarcodeIssue::QuietZoneTooSmall { min_modules, got_modules } => match locale {
            Locale::Nl => format!("Stille zone van {got_modules} modules is te klein; minimaal {min_modules}"),
            Locale::En => format!("A quiet zone of {got_modules} modules is too small; at least {min_modules}"),
        },
        BarcodeIssue::ModuleTooSmall { min_mm, value_mm } => match locale {
            Locale::Nl => format!("Module {value_mm} mm is kleiner dan het minimum {min_mm} mm"),
            Locale::En => format!("Module {value_mm} mm is smaller than the minimum {min_mm} mm"),
        },
        BarcodeIssue::EccStepUnavailable => {
            tr("Die foutcorrectiestap bestaat niet", "That error-correction step does not exist", locale)
        }
        BarcodeIssue::InputModeUnavailable => {
            tr("Dit type kent die invoermodus niet", "This type has no such input mode", locale)
        }
        BarcodeIssue::ParamNotApplicable => {
            tr("Deze instelling hoort niet bij dit type", "This setting does not apply to this type", locale)
        }
        BarcodeIssue::MarkerId { id, count } => match locale {
            Locale::Nl => format!("Marker {id} bestaat niet; kies 0 tot {}", count.saturating_sub(1)),
            Locale::En => format!("Marker {id} does not exist; choose 0 to {}", count.saturating_sub(1)),
        },
        BarcodeIssue::Engine => tr("De encoder weigerde deze invoer", "The encoder refused this input", locale),
        _ => tr("Niet mogelijk", "Not possible", locale),
    }
}

fn input_mode_name(mode: SymbolInputMode) -> &'static str {
    match mode {
        SymbolInputMode::Data => "Data",
        SymbolInputMode::Unicode => "Unicode",
        _ => "GS1",
    }
}

fn barcode_panel(b: &BarcodeDialogViewModel, locale: Locale) -> BarcodePanel {
    let types = b
        .types
        .iter()
        .map(|t| {
            let suffix =
                if t.supported { "" } else { tr(" — niet ondersteund", " — not supported", locale).leak() };
            let tip = match (t.supported, t.reason, t.source) {
                (false, Some(reason), _) => reason.to_owned(),
                (true, _, Some(source)) => match locale {
                    Locale::Nl => format!("Bron: {source}"),
                    Locale::En => format!("Source: {source}"),
                },
                _ => String::new(),
            };
            TypeItem { title: format!("{}{suffix}", t.name), enabled: t.supported, tip }
        })
        .collect();
    let readability = b.readability.map_or_else(String::new, |r| match locale {
        Locale::Nl => format!(
            "Module {} mm is kleiner dan het minimum van {} mm — niet betrouwbaar leesbaar",
            r.module_mm, r.min_module_mm
        ),
        Locale::En => format!(
            "Module {} mm is below the minimum of {} mm — may not be reliably readable",
            r.module_mm, r.min_module_mm
        ),
    });
    BarcodePanel {
        types,
        symbology: b.types.iter().position(|t| t.symbology == b.symbology).unwrap_or(0),
        payload: b.payload.clone(),
        payload_hint: b.payload_hint.to_owned(),
        create_title: text_text(if b.editing { TextText::Apply } else { TextText::Create }, locale).to_owned(),
        module: b.module.clone(),
        min_module: b.min_module.clone(),
        quiet: b.quiet.clone(),
        quiet_hint: match locale {
            Locale::Nl => format!("leeg = standaard, minimaal {}", b.quiet_minimum),
            Locale::En => format!("empty = standard, at least {}", b.quiet_minimum),
        },
        height: b.height.clone(),
        ecc: b.ecc.as_ref().map(|e| {
            let labels = e
                .steps
                .iter()
                .map(|s| match s.recovery_percent {
                    Some(p) => format!("{} ({p} %)", s.label),
                    None => s.label.to_owned(),
                })
                .collect();
            (labels, usize::from(e.selected))
        }),
        input_modes: (b.input_modes.len() > 1).then(|| {
            let index = b.input_modes.iter().position(|m| *m == b.input_mode).unwrap_or(0);
            (b.input_modes.iter().map(|m| input_mode_name(*m).to_owned()).collect(), index)
        }),
        columns: b.columns.clone(),
        width: b.width.clone(),
        size_line: b.size.as_ref().map_or_else(String::new, |(w, h)| match locale {
            Locale::Nl => format!("Afmeting: {w} × {h} mm"),
            Locale::En => format!("Size: {w} × {h} mm"),
        }),
        canonical_line: b.canonical.as_ref().map_or_else(String::new, |c| match locale {
            Locale::Nl => format!("Gecodeerd als: {c}"),
            Locale::En => format!("Encoded as: {c}"),
        }),
        problems: b.problems.iter().map(|p| barcode_problem_text(p, locale)).collect::<Vec<_>>().join("\n"),
        readability,
        create_enabled: b.can_create,
    }
}

pub fn text_panels(vm: &AppViewModel, document: DocumentId, locale: Locale) -> TextPanels {
    let Some(doc) = vm.document(document) else { return TextPanels::default() };
    TextPanels {
        toolbar: doc.text_toolbar.as_ref().map(|t| toolbar_panel(t, locale)),
        path: doc.text_on_path.as_ref().map(|p| path_panel(p, locale)),
        variable: doc.variable_panel.as_ref().map(|v| variable_panel(v, locale)),
        barcode: doc.barcode_dialog.as_ref().map(|b| barcode_panel(b, locale)),
    }
}

/// Text goes on unless it is what the field already shows and nothing is wrong with it (Return
/// and the following end of editing are one commit; a field in error still takes a retry).
fn changed(shown: &str, typed: &str, has_error: bool) -> bool {
    has_error || shown != typed
}

/// The intent a control becomes; `None` when it changes nothing (the same text again, a greyed
/// choice, a window that is gone).
pub fn text_panel_intent(vm: &AppViewModel, document: DocumentId, slot: Slot, value: SlotValue) -> Option<UiIntent> {
    let doc = vm.document(document)?;
    let text = |edit: TextEdit| UiIntent::Text { document, edit };
    let path = |edit: TextPathEdit| UiIntent::TextPath { document, edit };
    let var = |edit: VariableEdit| UiIntent::Variable { document, edit };
    let bc = |edit: BarcodeEdit| UiIntent::Barcode { document, edit };
    match (slot, value) {
        // --- the text toolbar
        (Slot::Family, SlotValue::Choice(i)) => {
            let bar = doc.text_toolbar.as_ref()?;
            let name = bar.families.get(i)?;
            (*name != bar.family).then(|| text(TextEdit::Family(name.clone())))
        }
        (Slot::Bold, SlotValue::Toggle(on)) => {
            (on != doc.text_toolbar.as_ref()?.bold.on).then(|| text(TextEdit::Bold(on)))
        }
        (Slot::Italic, SlotValue::Toggle(on)) => {
            (on != doc.text_toolbar.as_ref()?.italic.on).then(|| text(TextEdit::Italic(on)))
        }
        (Slot::Uppercase, SlotValue::Toggle(on)) => {
            (on != doc.text_toolbar.as_ref()?.uppercase).then(|| text(TextEdit::Uppercase(on)))
        }
        (Slot::Welded, SlotValue::Toggle(on)) => {
            (on != doc.text_toolbar.as_ref()?.welded.on).then(|| text(TextEdit::Welded(on)))
        }
        (Slot::SingleLine, SlotValue::Toggle(on)) => {
            (on != doc.text_toolbar.as_ref()?.single_line).then(|| text(TextEdit::SingleLine(on)))
        }
        (Slot::Align, SlotValue::Choice(i)) => {
            let align = *ALIGNS.get(i)?;
            (align != doc.text_toolbar.as_ref()?.align).then(|| text(TextEdit::Align(align)))
        }
        (Slot::VAlign, SlotValue::Choice(i)) => {
            let valign = *VALIGNS.get(i)?;
            (valign != doc.text_toolbar.as_ref()?.valign).then(|| text(TextEdit::VAlign(valign)))
        }
        (Slot::HSpace, SlotValue::Text(t)) => {
            let bar = doc.text_toolbar.as_ref()?;
            changed(&bar.h_space, &t, bar.issue.is_some()).then(|| text(TextEdit::HSpace(t)))
        }
        (Slot::VSpace, SlotValue::Text(t)) => {
            let bar = doc.text_toolbar.as_ref()?;
            changed(&bar.v_space, &t, bar.issue.is_some()).then(|| text(TextEdit::VSpace(t)))
        }
        (Slot::Height, SlotValue::Text(t)) => {
            let bar = doc.text_toolbar.as_ref()?;
            changed(&bar.height, &t, bar.issue.is_some()).then(|| text(TextEdit::Height(t)))
        }
        (Slot::Convert, SlotValue::Press) => {
            doc.text_toolbar.as_ref()?;
            Some(UiIntent::Invoke { document: Some(document), command: CommandKey::ShapesConvertToPath })
        }
        // --- the text-on-path sheet
        (Slot::PathDistort, SlotValue::Choice(i)) => {
            let distort = if i == 1 { Distort::Warp } else { Distort::Rigid };
            (distort != doc.text_on_path.as_ref()?.distort).then(|| path(TextPathEdit::Distort(distort)))
        }
        (Slot::PathStart, SlotValue::Text(t)) => {
            let p = doc.text_on_path.as_ref()?;
            changed(&p.start, &t, p.issue.is_some()).then(|| path(TextPathEdit::Start(t)))
        }
        (Slot::PathOffset, SlotValue::Text(t)) => {
            let p = doc.text_on_path.as_ref()?;
            changed(&p.offset, &t, p.issue.is_some()).then(|| path(TextPathEdit::Offset(t)))
        }
        (Slot::PathReverse, SlotValue::Toggle(on)) => {
            (on != doc.text_on_path.as_ref()?.reverse).then(|| path(TextPathEdit::Reverse(on)))
        }
        (Slot::PathApply, SlotValue::Press) => doc.text_on_path.as_ref().map(|_| path(TextPathEdit::Apply)),
        (Slot::PathCancel, SlotValue::Press) => doc.text_on_path.as_ref().map(|_| path(TextPathEdit::Cancel)),
        // --- the variable-text panel
        (Slot::VarKind, SlotValue::Choice(i)) => {
            let variable = i == 1;
            (variable != doc.variable_panel.as_ref()?.variable).then(|| var(VariableEdit::SetVariable(variable)))
        }
        (Slot::VarOffset, SlotValue::Text(t)) => {
            let p = doc.variable_panel.as_ref()?;
            changed(&p.offset, &t, p.problem.is_some()).then(|| var(VariableEdit::Offset(t)))
        }
        (
            Slot::SerialCurrent | Slot::SerialStart | Slot::SerialEnd | Slot::SerialBy | Slot::SerialDigits,
            SlotValue::Text(t),
        ) => {
            let p = doc.variable_panel.as_ref()?;
            let (field, shown) = match slot {
                Slot::SerialCurrent => (SerialField::Current, &p.serial.current),
                Slot::SerialStart => (SerialField::Start, &p.serial.start),
                Slot::SerialEnd => (SerialField::End, &p.serial.end),
                Slot::SerialBy => (SerialField::AdvanceBy, &p.serial.advance_by),
                _ => (SerialField::Digits, &p.serial.digits),
            };
            changed(shown, &t, p.problem.is_some()).then(|| var(VariableEdit::Serial(field, t)))
        }
        (Slot::SerialWrap, SlotValue::Toggle(on)) => {
            (on != doc.variable_panel.as_ref()?.serial.wrap).then(|| var(VariableEdit::Wrap(on)))
        }
        (Slot::VarPolicy, SlotValue::Choice(i)) => {
            let policy = *POLICIES.get(i)?;
            (policy != doc.variable_panel.as_ref()?.policy).then(|| var(VariableEdit::Policy(policy)))
        }
        (Slot::VarPrevious, SlotValue::Press) => doc.variable_panel.as_ref().map(|_| var(VariableEdit::Previous)),
        (Slot::VarNext, SlotValue::Press) => doc.variable_panel.as_ref().map(|_| var(VariableEdit::Next)),
        (Slot::VarReset, SlotValue::Press) => doc.variable_panel.as_ref().map(|_| var(VariableEdit::Reset)),
        (Slot::VarTest, SlotValue::Press) => doc.variable_panel.as_ref().map(|_| var(VariableEdit::Test)),
        (Slot::VarBake, SlotValue::Press) => doc.variable_panel.as_ref().map(|_| var(VariableEdit::Bake)),
        (Slot::VarClearCsv, SlotValue::Press) => doc.variable_panel.as_ref().map(|_| var(VariableEdit::ClearCsv)),
        (Slot::VarRow, SlotValue::Text(t)) => {
            let p = doc.variable_panel.as_ref()?;
            let shown = p.csv.as_ref().map_or("", |c| c.row.as_str());
            changed(shown, &t, p.problem.is_some()).then(|| var(VariableEdit::Row(t)))
        }
        (Slot::VarCsv, SlotValue::Csv(file)) => {
            let (name, bytes) = file?;
            doc.variable_panel.as_ref().map(|_| var(VariableEdit::LoadCsv { name, bytes }))
        }
        // --- the barcode sheet
        (Slot::BcType, SlotValue::Choice(i)) => {
            let b = doc.barcode_dialog.as_ref()?;
            let row = b.types.get(i)?;
            (row.supported && row.symbology != b.symbology).then(|| bc(BarcodeEdit::Symbology(row.symbology)))
        }
        (Slot::BcEcc, SlotValue::Choice(i)) => {
            let ecc = doc.barcode_dialog.as_ref()?.ecc.as_ref()?;
            let step = u8::try_from(i).ok().filter(|s| usize::from(*s) < ecc.steps.len())?;
            (step != ecc.selected).then(|| bc(BarcodeEdit::Ecc(step)))
        }
        (Slot::BcInputMode, SlotValue::Choice(i)) => {
            let b = doc.barcode_dialog.as_ref()?;
            let mode = *b.input_modes.get(i)?;
            (mode != b.input_mode).then(|| bc(BarcodeEdit::InputMode(mode)))
        }
        (
            Slot::BcPayload
            | Slot::BcModule
            | Slot::BcMinModule
            | Slot::BcQuiet
            | Slot::BcHeight
            | Slot::BcColumns
            | Slot::BcWidth,
            SlotValue::Text(t),
        ) => {
            let b = doc.barcode_dialog.as_ref()?;
            let (field, shown) = match slot {
                Slot::BcPayload => (BarcodeField::Payload, &b.payload),
                Slot::BcModule => (BarcodeField::Module, &b.module),
                Slot::BcMinModule => (BarcodeField::MinModule, &b.min_module),
                Slot::BcQuiet => (BarcodeField::Quiet, &b.quiet),
                Slot::BcHeight => (BarcodeField::Height, b.height.as_ref()?),
                Slot::BcColumns => (BarcodeField::Columns, b.columns.as_ref()?),
                _ => (BarcodeField::Width, &b.width),
            };
            if !changed(shown, &t, b.problems.iter().any(|p| p.field == field)) {
                return None;
            }
            Some(bc(match slot {
                Slot::BcPayload => BarcodeEdit::Payload(t),
                Slot::BcModule => BarcodeEdit::Module(t),
                Slot::BcMinModule => BarcodeEdit::MinModule(t),
                Slot::BcQuiet => BarcodeEdit::Quiet(t),
                Slot::BcHeight => BarcodeEdit::Height(t),
                Slot::BcColumns => BarcodeEdit::Columns(t),
                _ => BarcodeEdit::Width(t),
            }))
        }
        (Slot::BcCreate, SlotValue::Press) => doc.barcode_dialog.as_ref().map(|_| bc(BarcodeEdit::Create)),
        (Slot::BcCancel, SlotValue::Press) => doc.barcode_dialog.as_ref().map(|_| bc(BarcodeEdit::Cancel)),
        _ => None,
    }
}
