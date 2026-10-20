//! The CAM dialogs of the window (W3-UI-cam): Optimization Settings, Material / Interval / Focus
//! Test and Device Settings. What each shows comes from presentation's [`DialogViewModel`]; this
//! file only says it in words (Dutch and English) and turns a widget change into an intent. No
//! AppKit here: `appkit/panels/cam_dialog.rs` builds the widgets from these texts.

use burnburn_document::process::OrderKey;
use burnburn_presentation::{
    CamDialog, CapabilityRow, DialogAction, DialogActionKind, DialogChoice, DialogError, DialogField, DialogNote,
    DialogValue, DialogViewModel, Feature, RdRefusal, SupportView, TestResultView, UiIntent,
};
use burnburn_types::DocumentId;

use super::estimate_text;
use crate::l10n::{Locale, feature_name};

fn pick(pair: (&'static str, &'static str), locale: Locale) -> &'static str {
    match locale {
        Locale::Nl => pair.0,
        Locale::En => pair.1,
    }
}

/// Window title of a dialog.
pub fn cam_title(dialog: CamDialog, locale: Locale) -> &'static str {
    pick(
        match dialog {
            CamDialog::Optimization => ("Optimalisatie-instellingen", "Optimization Settings"),
            CamDialog::MaterialTest => ("Materiaaltest", "Material Test"),
            CamDialog::IntervalTest => ("Intervaltest", "Interval Test"),
            CamDialog::FocusTest => ("Focustest", "Focus Test"),
            CamDialog::Device => ("Apparaatinstellingen", "Device Settings"),
        },
        locale,
    )
}

/// Menu item that opens a dialog (an ellipsis: it asks for input first).
pub fn cam_menu_title(dialog: CamDialog, locale: Locale) -> String {
    format!("{}…", cam_title(dialog, locale))
}

/// Caption of an `Order by` level.
pub fn order_key_caption(key: OrderKey, locale: Locale) -> &'static str {
    pick(
        match key {
            OrderKey::Layer => ("Laag", "Layer"),
            OrderKey::Group => ("Groep", "Group"),
            OrderKey::Priority => ("Prioriteit", "Priority"),
        },
        locale,
    )
}

/// Caption of a field. The diameter follows the kind of rotary in the dialog.
pub fn cam_caption(field: DialogField, vm: &DialogViewModel, locale: Locale) -> String {
    use DialogField as F;
    if let F::OrderEnabled(key) = field {
        return format!("{}: {}", pick(("Sorteer op", "Order by"), locale), order_key_caption(key, locale));
    }
    if field == F::RotaryDiameter {
        let roller = vm.choice(F::RotaryKind) == Some(DialogChoice::RotaryRoller);
        return pick(
            if roller {
                ("Rollerdiameter (mm)", "Roller diameter (mm)")
            } else {
                ("Objectdiameter (mm)", "Object diameter (mm)")
            },
            locale,
        )
        .to_owned();
    }
    pick(
        match field {
            F::InnerFirst => ("Binnenste vormen eerst snijden", "Cut inner shapes first"),
            F::ReduceTravel => ("Verplaatsingen verkorten", "Reduce travel moves"),
            F::BestStart => ("Beste beginpunt kiezen", "Choose best starting point"),
            F::BestDirection => ("Beste richting kiezen", "Choose best direction"),
            F::HideBacklash => ("Speling (backlash) verbergen", "Hide backlash"),
            F::RemoveOverlapping => ("Overlappende lijnen verwijderen", "Remove overlapping lines"),
            F::OverlapTolerance => ("Tolerantie (mm, hoogstens 5)", "Tolerance (mm, at most 5)"),
            F::CellMode => ("Vakken", "Cells"),
            F::ColumnParameter => ("Kolommen variëren", "Columns vary"),
            F::ColumnMin => ("Kolommen, van", "Columns, from"),
            F::ColumnMax => ("Kolommen, tot", "Columns, to"),
            F::ColumnCount => ("Kolommen, aantal (1–25)", "Columns, count (1–25)"),
            F::RowsEnabled => ("Ook rijen variëren", "Also vary rows"),
            F::RowParameter => ("Rijen variëren", "Rows vary"),
            F::RowMin => ("Rijen, van", "Rows, from"),
            F::RowMax => ("Rijen, tot", "Rows, to"),
            F::RowCount => ("Rijen, aantal (1–25)", "Rows, count (1–25)"),
            F::BaseSpeed => ("Snelheid (mm/s)", "Speed (mm/s)"),
            F::BasePower => ("Vermogen (%)", "Power (%)"),
            F::BasePasses => ("Doorgangen", "Passes"),
            F::FillInterval => ("Lijnafstand vulling (mm)", "Fill line interval (mm)"),
            F::FillOverscan => ("Overscan (mm)", "Overscan (mm)"),
            F::CellWidth => ("Vakbreedte (mm)", "Cell width (mm)"),
            F::CellHeight => ("Vakhoogte (mm)", "Cell height (mm)"),
            F::CellGap => ("Tussenruimte (mm)", "Gap (mm)"),
            F::LabelsEnabled => ("Waarden als label erbij snijden", "Cut value labels"),
            F::LabelHeight => ("Labelhoogte (mm)", "Label height (mm)"),
            F::BorderEnabled => ("Rand eromheen", "Border around it"),
            F::BorderMargin => ("Randmarge (mm)", "Border margin (mm)"),
            F::IntervalMin => ("Kleinste lijnafstand (mm of lpi)", "Smallest interval (mm or lpi)"),
            F::IntervalMax => ("Grootste lijnafstand (mm of lpi)", "Largest interval (mm or lpi)"),
            F::IntervalCount => ("Aantal vakken (1–25)", "Number of cells (1–25)"),
            F::FocusStart => ("Eerste focusverschuiving (mm)", "First focus offset (mm)"),
            F::FocusStep => ("Stap tussen lijnen (mm)", "Step between lines (mm)"),
            F::FocusCount => ("Aantal lijnen (1–25)", "Number of lines (1–25)"),
            F::LineLength => ("Lijnlengte (mm)", "Line length (mm)"),
            F::LineSpacing => ("Afstand tussen lijnen (mm)", "Line spacing (mm)"),
            F::BedWidth => ("Bedbreedte (mm)", "Bed width (mm)"),
            F::BedHeight => ("Bedhoogte (mm)", "Bed height (mm)"),
            F::BedOrigin => ("Oorsprong van de machine", "Machine origin"),
            F::MaxSpeed => ("Maximale snelheid (mm/s)", "Maximum speed (mm/s)"),
            F::TravelSpeed => ("Verplaatsingssnelheid (mm/s)", "Travel speed (mm/s)"),
            F::MinPower => ("Minimaal vermogen (%)", "Minimum power (%)"),
            F::MaxPower => ("Maximaal vermogen (%)", "Maximum power (%)"),
            F::RotaryEnabled => ("Rotary gebruiken", "Use rotary"),
            F::RotaryKind => ("Type rotary", "Rotary type"),
            F::StepsPerRotation => ("Stappen per omwenteling", "Steps per rotation"),
            F::YStepsPerMm => ("Y-stappen per mm (uit de machine lezen)", "Y steps per mm (read from the machine)"),
            F::RotaryMirror => ("Spiegelen", "Mirror"),
            _ => ("", ""),
        },
        locale,
    )
    .to_owned()
}

/// Label of a pop-up entry.
pub fn cam_choice_label(choice: DialogChoice, locale: Locale) -> &'static str {
    use DialogChoice as C;
    pick(
        match choice {
            C::CellLine => ("Lijn (omtrek snijden)", "Line (cut the outline)"),
            C::CellFill => ("Vulling", "Fill"),
            C::ParamSpeed => ("Snelheid", "Speed"),
            C::ParamPower => ("Vermogen", "Power"),
            C::ParamPasses => ("Doorgangen", "Passes"),
            C::ParamInterval => ("Lijnafstand", "Interval"),
            C::ParamZOffset => ("Z-offset", "Z offset"),
            C::OriginBottomLeft => ("Linksonder", "Bottom left"),
            C::OriginTopLeft => ("Linksboven", "Top left"),
            C::OriginTopRight => ("Rechtsboven", "Top right"),
            C::OriginBottomRight => ("Rechtsonder", "Bottom right"),
            C::RotaryChuck => ("Klauwplaat (chuck)", "Chuck"),
            C::RotaryRoller => ("Rollers", "Roller"),
            _ => ("", ""),
        },
        locale,
    )
}

/// The refusal of RD output in words.
pub fn rd_refusal_text(reason: RdRefusal, locale: Locale) -> &'static str {
    pick(
        match reason {
            RdRefusal::CrossHatch => (
                "Kruisarcering heeft nog geen bewezen Ruida-vorm: plan en voorvertoning bestaan, RD-uitvoer wordt geweigerd.",
                "Cross-hatch has no proven Ruida form yet: the plan and preview exist, RD output is refused.",
            ),
            RdRefusal::DotMode => (
                "Puntmodus (dwell) heeft nog geen bewezen Ruida-vorm: RD-uitvoer wordt geweigerd.",
                "Dot (dwell) mode has no proven Ruida form yet: RD output is refused.",
            ),
            RdRefusal::RampLength => (
                "Vermogensverloop (ramp) heeft nog geen bewezen Ruida-vorm: RD-uitvoer wordt geweigerd.",
                "Power ramp has no proven Ruida form yet: RD output is refused.",
            ),
            RdRefusal::FocusOffset => (
                "Focus-/Z-verschuiving heeft nog geen bewezen Ruida-vorm (zie docs/hardware/evidence-cam.md): RD-uitvoer wordt geweigerd.",
                "Focus/Z offsets have no proven Ruida form yet (see docs/hardware/evidence-cam.md): RD output is refused.",
            ),
            RdRefusal::ScanAngle => (
                "Schuine scanlijnen hebben nog geen bewezen Ruida-vorm: RD-uitvoer wordt geweigerd.",
                "Angled scan lines have no proven Ruida form yet: RD output is refused.",
            ),
            RdRefusal::GrayscaleModulation => (
                "Grijswaarden (vermogen per run) hebben nog geen bewezen Ruida-vorm: RD-uitvoer wordt geweigerd.",
                "Grayscale (power per run) has no proven Ruida form yet: RD output is refused.",
            ),
            RdRefusal::TooManyParts => (
                "Meer instellingenblokken dan het RD-formaat toelaat (128): RD-uitvoer wordt geweigerd.",
                "More settings blocks than the RD format allows (128): RD output is refused.",
            ),
            RdRefusal::NegativeCoordinate => (
                "Een coördinaat onder nul is niet bevestigd voor RD: RD-uitvoer wordt geweigerd.",
                "A coordinate below zero is not confirmed for RD: RD output is refused.",
            ),
            _ => (
                "Dit onderdeel heeft nog geen bewezen Ruida-vorm: RD-uitvoer wordt geweigerd.",
                "This part has no proven Ruida form yet: RD output is refused.",
            ),
        },
        locale,
    )
}

/// What is wrong with a field, in words.
pub fn cam_error(error: DialogError, locale: Locale) -> String {
    match error {
        DialogError::Invalid => pick(("Geen geldige waarde.", "Not a valid value."), locale).to_owned(),
        DialogError::OutOfRange => {
            pick(("Buiten het toegestane bereik.", "Outside the allowed range."), locale).to_owned()
        }
        DialogError::NotWhole => pick(("Moet een geheel getal zijn.", "Must be a whole number."), locale).to_owned(),
        DialogError::Count => pick(("Aantal moet 1 tot 25 zijn.", "The count must be 1 to 25."), locale).to_owned(),
        DialogError::ParameterNotForMode => {
            pick(("Lijnafstand past alleen bij vulvakken.", "Interval only applies to fill cells."), locale).to_owned()
        }
        DialogError::SameParameter => {
            pick(("Kolommen en rijen variëren hetzelfde.", "Columns and rows vary the same setting."), locale)
                .to_owned()
        }
        DialogError::Size => pick(("Moet groter zijn dan nul.", "Must be greater than zero."), locale).to_owned(),
        DialogError::TooManyCells => {
            pick(("Meer dan 100 vakken in één test.", "More than 100 cells in one test."), locale).to_owned()
        }
        DialogError::Unsupported(feature) => {
            let name = feature_name(feature, locale);
            match locale {
                Locale::Nl => format!("Ontbrekende capability: {name} (zie Apparaatinstellingen)."),
                Locale::En => format!("Missing capability: {name} (see Device Settings)."),
            }
        }
        _ => pick(("Ongeldig.", "Invalid."), locale).to_owned(),
    }
}

/// A note of the dialog, in words.
pub fn cam_note(note: DialogNote, locale: Locale) -> String {
    let pair = match note {
        DialogNote::PriorityNeedsOrderLevel => (
            "Er zijn vormen met een prioriteit, maar Prioriteit staat niet in de lijst Sorteer op: de prioriteiten sorteren niets.",
            "Some shapes have a priority, but Priority is not in the Order by list: the priorities reorder nothing.",
        ),
        DialogNote::NoMachineProfile => (
            "Er is nog geen machineprofiel: maak er een in Apparaatinstellingen.",
            "There is no machine profile yet: create one in Device Settings.",
        ),
        DialogNote::FocusNeedsZ => (
            "Dit profiel heeft geen Z-as die BurnBurn kan aansturen: de focustest is niet beschikbaar.",
            "This profile has no Z axis BurnBurn can drive: the focus test is not available.",
        ),
        DialogNote::TestIsSeparate => (
            "De test is een eigen taak naast het project: het document verandert niet.",
            "The test is a job of its own beside the project: the document does not change.",
        ),
        DialogNote::NewProfile => (
            "Er is nog geen profiel: Toepassen maakt het eerste. De beginwaarden zijn instellingen, geen meting.",
            "There is no profile yet: Apply creates the first one. The start values are settings, not measurements.",
        ),
        DialogNote::DialectUnverified => (
            "Het Ruida-dialect van dit profiel is niet op een echte machine bewezen; uploaden en starten blijven geweigerd tot daar bewijs voor is.",
            "This profile's Ruida dialect is not proven on a real machine; upload and start stay refused until there is evidence.",
        ),
        DialogNote::RotaryUnconfirmed => (
            "De Y-schaal van de rotary is door geen machine bevestigd (docs/hardware/evidence-cam.md); controleer met een proefstuk.",
            "The rotary's Y scale has not been confirmed on a machine (docs/hardware/evidence-cam.md); check with a test piece.",
        ),
        DialogNote::RotaryNotInCliProfile => (
            "Het profielbestand van de commandoregel kent nog geen rotary: deze instelling geldt alleen in dit venster.",
            "The command-line profile file knows no rotary yet: this setting only applies in this window.",
        ),
        DialogNote::ProfileChangeMakesPlansStale => (
            "Toepassen maakt een nieuwe profielversie: eerder gecompileerde plannen worden verouderd getoond tot je opnieuw compileert.",
            "Applying makes a new profile revision: plans compiled earlier show as out of date until you compile again.",
        ),
        DialogNote::TestCompileFailed => ("De test kon niet worden gecompileerd.", "The test could not be compiled."),
        DialogNote::TestSaved => ("De test is als RD-bestand opgeslagen.", "The test was saved as an RD file."),
        DialogNote::ExportFailed => ("Het RD-bestand kon niet worden geschreven.", "The RD file could not be written."),
        DialogNote::RdRefused(reason) => return rd_refusal_text(reason, locale).to_owned(),
        _ => ("", ""),
    };
    pick(pair, locale).to_owned()
}

/// Title of a button.
pub fn cam_action_title(kind: DialogActionKind, dialog: CamDialog, locale: Locale) -> &'static str {
    pick(
        match (kind, dialog) {
            (DialogActionKind::Apply, _) => ("Toepassen", "Apply"),
            (DialogActionKind::Close, _) => ("Sluiten", "Close"),
            (DialogActionKind::Generate, _) => ("Maak test", "Generate test"),
            (DialogActionKind::ExportRd, _) => ("Opslaan als RD…", "Save as RD…"),
            (DialogActionKind::ReturnToFlatbed, _) => ("Terug naar vlakbed", "Back to flat bed"),
        },
        locale,
    )
}

/// One line of the capability list: the feature and how the profile supports it.
pub fn cam_support_line(row: &CapabilityRow, locale: Locale) -> String {
    let name = feature_name(row.feature, locale);
    let state = match &row.support {
        SupportView::Supported => pick(("ondersteund", "supported"), locale).to_owned(),
        SupportView::Unsupported => {
            format!(
                "{} — {}",
                pick(("niet ondersteund", "not supported"), locale),
                unsupported_reason(row.feature, locale)
            )
        }
        SupportView::Unverified(reason) => format!(
            "{} — {}",
            pick(("niet bewezen", "not proven"), locale),
            unverified_reason(row.feature, reason, locale)
        ),
    };
    format!("{name}: {state}")
}

fn unsupported_reason(feature: Feature, locale: Locale) -> String {
    match feature {
        Feature::ZStep => pick(
            ("dit profiel heeft geen Z-as die BurnBurn kan aansturen", "this profile has no Z axis BurnBurn can drive"),
            locale,
        )
        .to_owned(),
        _ => pick(("door dit profiel uitgesloten", "ruled out by this profile"), locale).to_owned(),
    }
}

fn unverified_reason(feature: Feature, domain_reason: &str, locale: Locale) -> String {
    match feature {
        Feature::ScanAngle => {
            pick(("schuine scanlijnen zijn niet als RD bevestigd", "diagonal scan runs are not RD-confirmed"), locale)
                .to_owned()
        }
        Feature::GrayscaleModulation => pick(
            ("de RD-records voor grijswaarden zijn niet ontcijferd", "the RD records for grayscale are not decoded"),
            locale,
        )
        .to_owned(),
        Feature::CommonLine => {
            pick(("gemeenschappelijk snijden is niet gevalideerd", "common-line cutting is not validated"), locale)
                .to_owned()
        }
        Feature::DotMode => {
            pick(("puntpulsen hebben nog geen RD-vorm", "dot pulses have no RD form yet"), locale).to_owned()
        }
        Feature::RampLength => {
            pick(("vermogensverloop heeft nog geen RD-vorm", "power ramps have no RD form yet"), locale).to_owned()
        }
        // A reason a profile gave itself stays as it was written.
        _ => domain_reason.to_owned(),
    }
}

/// What the generated test is, in words: cells, the plan's identity, the time estimate and the
/// findings; written, stale or not.
pub fn cam_result_text(result: &TestResultView, locale: Locale) -> String {
    let cells = match locale {
        Locale::Nl => format!("{} {}", result.cells, if result.cells == 1 { "vak" } else { "vakken" }),
        Locale::En => format!("{} {}", result.cells, if result.cells == 1 { "cell" } else { "cells" }),
    };
    let findings = match locale {
        Locale::Nl => format!(
            "{} {}, {} {}",
            result.errors,
            if result.errors == 1 { "fout" } else { "fouten" },
            result.warnings,
            if result.warnings == 1 { "waarschuwing" } else { "waarschuwingen" }
        ),
        Locale::En => format!(
            "{} {}, {} {}",
            result.errors,
            if result.errors == 1 { "error" } else { "errors" },
            result.warnings,
            if result.warnings == 1 { "warning" } else { "warnings" }
        ),
    };
    let mut text =
        format!("{cells}\n{}\n{}\n{findings}", result.plan, estimate_text(result.preview.estimated_seconds, locale));
    if result.stale {
        text.push('\n');
        text.push_str(pick(
            (
                "Verouderd: het machineprofiel is gewijzigd; maak de test opnieuw.",
                "Out of date: the machine profile changed; generate the test again.",
            ),
            locale,
        ));
    }
    if let Some(path) = &result.saved_to {
        text.push('\n');
        text.push_str(&match locale {
            Locale::Nl => format!("Opgeslagen als {}", path.display()),
            Locale::En => format!("Saved as {}", path.display()),
        });
    }
    text
}

/// The intent for a field the user changed; `None` when the new value equals what the dialog
/// shows (Return and the end of editing both send the field's action).
pub fn cam_field_intent(
    vm: Option<&DialogViewModel>,
    document: DocumentId,
    dialog: CamDialog,
    field: DialogField,
    value: DialogValue,
) -> Option<UiIntent> {
    let vm = vm?;
    let row = vm.row(field)?;
    if row.value == value && row.error.is_none() {
        return None;
    }
    Some(UiIntent::SetCamField { document, dialog, field, value })
}

/// The intent for a button. Save as RD needs a path from the save panel first: pass it in `path`;
/// without it the button has no intent yet (`None`). A disabled button has none either.
pub fn cam_action_intent(
    vm: &DialogViewModel,
    document: DocumentId,
    dialog: CamDialog,
    kind: DialogActionKind,
    path: Option<std::path::PathBuf>,
) -> Option<UiIntent> {
    if !vm.action(kind)?.enabled {
        return None;
    }
    let action = match kind {
        DialogActionKind::Apply => DialogAction::Apply,
        DialogActionKind::Close => DialogAction::Close,
        DialogActionKind::Generate => DialogAction::Generate,
        DialogActionKind::ReturnToFlatbed => DialogAction::ReturnToFlatbed,
        DialogActionKind::ExportRd => DialogAction::ExportRd(path?),
    };
    Some(UiIntent::CamDialogAction { document, dialog, action })
}

/// The intent for moving an `Order by` level up or down (a button beside its row).
pub fn cam_move_intent(vm: &DialogViewModel, document: DocumentId, key: OrderKey, up: bool) -> Option<UiIntent> {
    let (can_up, can_down) = vm.row(DialogField::OrderEnabled(key))?.reorder?;
    if (up && !can_up) || (!up && !can_down) {
        return None;
    }
    let action = if up { DialogAction::MoveUp(key) } else { DialogAction::MoveDown(key) };
    Some(UiIntent::CamDialogAction { document, dialog: CamDialog::Optimization, action })
}

/// What the inspector says under the priority field: lower numbers are cut first, and only when
/// Priority is a level of Optimization Settings ▸ Order by.
pub fn cam_priority_hint(locale: Locale) -> &'static str {
    pick(
        (
            "Lager = eerder gesneden. Werkt alleen als Prioriteit in Bewerken ▸ Optimalisatie-instellingen staat.",
            "Lower = cut first. Only applies when Priority is in Edit ▸ Optimization Settings ▸ Order by.",
        ),
        locale,
    )
}

/// The short line under the priority field (the tooltip has [`cam_priority_hint`]).
pub fn cam_priority_hint_short(locale: Locale) -> &'static str {
    pick(
        (
            "Lager = eerder · alleen met Prioriteit in Optimalisatie",
            "Lower = first · only with Priority in Optimization",
        ),
        locale,
    )
}
