//! NL/EN texts of the Schikken flows (W3-UI-schikken): dock, move together, warp/deform, resize
//! slots, tangent circle, print and cut, center finder and nesting. Kept apart so `l10n.rs` only
//! gets a hook per table (the Windows host compiles the same files).

use burnburn_document::EditLabel;
use burnburn_presentation::{
    CenterProblem, CommandKey, MessageKey, NestFailure, NestField, NestSkipReason, NestUnplacedReason, PlaceFailure,
    RegistrationFailure, RegistrationField, ReshapeProblem, SchikkenMessage, SlotField, SlotsFailure, TangentProblem,
};

use super::{Locale, pick};

pub(super) fn command_pair(key: CommandKey) -> Option<(&'static str, &'static str)> {
    use CommandKey as C;
    Some(match key {
        C::DockLeft => ("Dok links", "Dock Left"),
        C::DockRight => ("Dok rechts", "Dock Right"),
        C::DockUp => ("Dok omhoog", "Dock Up"),
        C::DockDown => ("Dok omlaag", "Dock Down"),
        C::MoveTogetherHorizontal => ("Schuif samen horizontaal", "Move Together Horizontally"),
        C::MoveTogetherVertical => ("Schuif samen verticaal", "Move Together Vertically"),
        C::ToolWarp => ("Vervorm selectie (4 punten)", "Warp Selection (4 Points)"),
        C::ToolDeform => ("Vervorm selectie (16 punten)", "Deform Selection (16 Points)"),
        C::ReshapeApply => ("Pas vervorming toe", "Apply Reshape"),
        C::ReshapeReset => ("Zet grepen terug", "Reset Handles"),
        C::ResizeSlots => ("Sleuven aanpassen…", "Resize Slots…"),
        C::QuickNest => ("Snel nesten…", "Quick Nest…"),
        C::NestSelected => ("Nest selectie…", "Nest Selected…"),
        C::PrintAndCut => ("Print en snij…", "Print and Cut…"),
        C::CenterFinder => ("Middelpuntzoeker…", "Center Finder…"),
        _ => return None,
    })
}

pub(super) fn edit_label_pair(label: EditLabel) -> Option<(&'static str, &'static str)> {
    Some(match label {
        EditLabel::Dock => ("Docken", "Dock"),
        EditLabel::MoveTogether => ("Samenschuiven", "Move Together"),
        EditLabel::Warp => ("Vervormen (4 punten)", "Warp"),
        EditLabel::Deform => ("Vervormen (16 punten)", "Deform"),
        EditLabel::ResizeSlots => ("Sleuven aanpassen", "Resize Slots"),
        EditLabel::Nest => ("Nesten", "Nest"),
        _ => return None,
    })
}

/// The edit labels this module owns (for the completeness test).
pub(super) const EDIT_LABELS: [EditLabel; 6] = [
    EditLabel::Dock,
    EditLabel::MoveTogether,
    EditLabel::Warp,
    EditLabel::Deform,
    EditLabel::ResizeSlots,
    EditLabel::Nest,
];

fn failure_pair(f: PlaceFailure) -> (&'static str, &'static str) {
    match f {
        PlaceFailure::NoContact => {
            ("Niets om tegen te docken in die richting.", "There is nothing to dock against in that direction.")
        }
        PlaceFailure::Overlapping => (
            "De objecten overlappen de referentie al; docken kan dan niet.",
            "The objects already overlap the reference, so they cannot dock.",
        ),
        PlaceFailure::ReferenceNotSelected => {
            ("De gekozen referentie hoort niet bij de selectie.", "The chosen reference is not part of the selection.")
        }
        PlaceFailure::TooFewObjects => ("Kies minstens twee objecten.", "Select at least two objects."),
        PlaceFailure::EmptyGeometry => {
            ("Een van de objecten heeft geen geometrie.", "One of the objects has no geometry.")
        }
        PlaceFailure::NothingToMove => ("Alles ligt al op zijn plaats.", "Everything is already in place."),
        PlaceFailure::Refused => {
            ("De opdracht is geweigerd; er is niets gewijzigd.", "The command was refused; nothing changed.")
        }
    }
}

const SLOT_FAILURES: [SlotsFailure; 7] = [
    SlotsFailure::InvalidField(SlotField::From),
    SlotsFailure::InvalidField(SlotField::To),
    SlotsFailure::InvalidField(SlotField::Tolerance),
    SlotsFailure::NoSlotsFound,
    SlotsFailure::Collision,
    SlotsFailure::NothingChanged,
    SlotsFailure::Refused,
];

fn slots_pair(f: SlotsFailure) -> (&'static str, &'static str) {
    match f {
        SlotsFailure::InvalidField(SlotField::From) => (
            "De huidige breedte moet een getal groter dan 0 zijn.",
            "The current width must be a number greater than 0.",
        ),
        SlotsFailure::InvalidField(SlotField::To) => {
            ("De nieuwe breedte moet een getal groter dan 0 zijn.", "The new width must be a number greater than 0.")
        }
        SlotsFailure::InvalidField(SlotField::Tolerance) => {
            ("De tolerantie moet een getal van 0 of meer zijn.", "The tolerance must be a number of 0 or more.")
        }
        SlotsFailure::NoSlotsFound => ("Geen sleuven met die breedte gevonden.", "No slots of that width were found."),
        SlotsFailure::Collision => (
            "Een naburige rand zou dichtklappen; kies een andere breedte.",
            "A neighbouring edge would collapse; choose another width.",
        ),
        SlotsFailure::NothingChanged => ("Er verandert niets.", "Nothing would change."),
        SlotsFailure::Refused => {
            ("De opdracht is geweigerd; er is niets gewijzigd.", "The command was refused; nothing changed.")
        }
    }
}

const NEST_FAILURES: [NestFailure; 9] = [
    NestFailure::InvalidField(NestField::Padding),
    NestFailure::InvalidField(NestField::EdgeBuffer),
    NestFailure::InvalidField(NestField::Step),
    NestFailure::InvalidField(NestField::Sheets),
    NestFailure::NothingToNest,
    NestFailure::GridTooLarge,
    NestFailure::Geometry,
    NestFailure::Stale,
    NestFailure::Refused,
];

fn nest_pair(f: NestFailure) -> (&'static str, &'static str) {
    match f {
        NestFailure::InvalidField(NestField::Padding) => (
            "De tussenruimte tussen onderdelen moet een getal van 0 of meer zijn.",
            "The padding between parts must be a number of 0 or more.",
        ),
        NestFailure::InvalidField(NestField::EdgeBuffer) => {
            ("De randmarge moet een getal van 0 of meer zijn.", "The edge buffer must be a number of 0 or more.")
        }
        NestFailure::InvalidField(NestField::Step) => {
            ("De rasterstap moet een getal groter dan 0 zijn.", "The grid step must be a number greater than 0.")
        }
        NestFailure::InvalidField(NestField::Sheets) => (
            "Het aantal platen moet een geheel getal van 1 tot en met 20 zijn.",
            "The number of sheets must be a whole number from 1 to 20.",
        ),
        NestFailure::NothingToNest => (
            "Er is niets om te nesten: kies gesloten, niet-vergrendelde vormen.",
            "There is nothing to nest: choose closed, unlocked shapes.",
        ),
        NestFailure::GridTooLarge => (
            "Het raster wordt te groot voor deze stap; kies een grotere stap.",
            "The grid would be too large for this step; choose a larger step.",
        ),
        NestFailure::Geometry => (
            "Een onderdeel of het plaatmateriaal kon niet verwerkt worden.",
            "A part or the sheet could not be processed.",
        ),
        NestFailure::Stale => (
            "Het document is gewijzigd na het nesten; nest opnieuw.",
            "The document changed after nesting; nest again.",
        ),
        NestFailure::Refused => (
            "Het resultaat kon niet worden toegepast; er is niets gewijzigd.",
            "The result could not be applied; nothing changed.",
        ),
    }
}

const REGISTRATION_FAILURES: [RegistrationFailure; 12] = [
    RegistrationFailure::InvalidField(RegistrationField::X),
    RegistrationFailure::InvalidField(RegistrationField::Y),
    RegistrationFailure::InvalidField(RegistrationField::MaxDeviation),
    RegistrationFailure::TargetNotSet,
    RegistrationFailure::NotMeasured,
    RegistrationFailure::PositionUnknown,
    RegistrationFailure::NoMachine,
    RegistrationFailure::NoMachineProfile,
    RegistrationFailure::DesignTargetsTooClose,
    RegistrationFailure::MeasuredTargetsTooClose,
    RegistrationFailure::ScaleOutOfRange,
    RegistrationFailure::NeedsAbsoluteCoordinates,
];

fn registration_pair(f: RegistrationFailure) -> (&'static str, &'static str) {
    match f {
        RegistrationFailure::InvalidField(RegistrationField::X) => {
            ("De X-waarde is geen getal.", "The X value is not a number.")
        }
        RegistrationFailure::InvalidField(RegistrationField::Y) => {
            ("De Y-waarde is geen getal.", "The Y value is not a number.")
        }
        RegistrationFailure::InvalidField(RegistrationField::MaxDeviation) => (
            "De toegestane afwijking moet een getal van 0 of meer zijn (0,05 = 5 %).",
            "The allowed deviation must be a number of 0 or more (0.05 = 5 %).",
        ),
        RegistrationFailure::TargetNotSet => {
            ("Kies eerst beide doelpunten in het ontwerp.", "Choose both targets in the design first.")
        }
        RegistrationFailure::NotMeasured => {
            ("Meet eerst beide doelpunten met de laser.", "Measure both targets with the laser first.")
        }
        RegistrationFailure::PositionUnknown => (
            "De laserpositie is onbekend; lees eerst de positie uit.",
            "The laser position is unknown; read the position first.",
        ),
        RegistrationFailure::NoMachine => ("Er is geen machine aangesloten.", "No machine is connected."),
        RegistrationFailure::NoMachineProfile => {
            ("Er is geen machineprofiel gekozen.", "No machine profile is chosen.")
        }
        RegistrationFailure::DesignTargetsTooClose => (
            "De doelpunten in het ontwerp liggen te dicht bij elkaar.",
            "The targets in the design are too close together.",
        ),
        RegistrationFailure::MeasuredTargetsTooClose => {
            ("De gemeten doelpunten liggen te dicht bij elkaar.", "The measured targets are too close together.")
        }
        RegistrationFailure::ScaleOutOfRange => (
            "De schaal valt buiten de toegestane afwijking; er is niets toegepast.",
            "The scale is outside the allowed deviation; nothing was applied.",
        ),
        RegistrationFailure::NeedsAbsoluteCoordinates => (
            "Print en snij werkt met Absolute coördinaten; zet Start vanaf daarop.",
            "Print and Cut works with Absolute Coordinates; set Start From to that.",
        ),
        RegistrationFailure::Refused => {
            ("De registratie is geweigerd; er is niets gewijzigd.", "The registration was refused; nothing changed.")
        }
    }
}

/// Why Tangent Circle has no solution to show.
pub fn tangent_problem_text(p: TangentProblem, locale: Locale) -> String {
    pick(
        match p {
            TangentProblem::Coincident => (
                "Twee doelen zijn hetzelfde punt, dezelfde lijn of dezelfde cirkel.",
                "Two targets are the same point, line or circle.",
            ),
            TangentProblem::Degenerate => (
                "Oneindig veel cirkels passen; kies een andere diameter.",
                "Infinitely many circles fit; choose another diameter.",
            ),
            TangentProblem::NoSolution => ("Geen cirkel raakt al deze doelen.", "No circle touches all these targets."),
            TangentProblem::Other => ("Deze raakcirkel kan niet.", "This tangent circle is not possible."),
        },
        locale,
    )
}

/// What is wrong with the Center Finder's points.
pub fn center_problem_text(p: CenterProblem, locale: Locale) -> String {
    pick(
        match p {
            CenterProblem::Coincident => {
                ("Minstens drie verschillende punten zijn nodig.", "At least three distinct points are needed.")
            }
            CenterProblem::Collinear => {
                ("De punten liggen op één lijn: er is geen cirkel.", "The points lie on one line: there is no circle.")
            }
        },
        locale,
    )
}

/// Why a part was not placed (the report of a finished nest).
pub fn nest_unplaced_text(reason: NestUnplacedReason, locale: Locale) -> String {
    pick(
        match reason {
            NestUnplacedReason::NoFit => ("past niet op het plaatmateriaal", "does not fit on the sheet"),
            NestUnplacedReason::NoAllowedRotation => ("geen toegestane draaiing past", "no allowed rotation fits"),
            NestUnplacedReason::Budget => {
                ("zoekbudget op; probeer een grotere stap", "search budget used up; try a larger step")
            }
        },
        locale,
    )
}

/// Why an object was left out of the nest.
pub fn nest_skip_text(reason: NestSkipReason, locale: Locale) -> String {
    pick(
        match reason {
            NestSkipReason::OpenContour => ("open pad: niets om te plaatsen", "open path: nothing to place"),
            NestSkipReason::NoGeometry => ("geen geometrie", "no geometry"),
            NestSkipReason::Derived => ("afgeleide vorm: verplaats de bron", "derived shape: move its source"),
        },
        locale,
    )
}

/// Text of a Schikken status message.
pub(super) fn message(m: SchikkenMessage, locale: Locale) -> String {
    match m {
        SchikkenMessage::Dock(f) | SchikkenMessage::MoveTogether(f) => pick(failure_pair(f), locale),
        SchikkenMessage::Slots(f) => pick(slots_pair(f), locale),
        SchikkenMessage::Nest(f) => pick(nest_pair(f), locale),
        SchikkenMessage::Registration(f) => pick(registration_pair(f), locale),
        SchikkenMessage::InvalidGap => pick(
            ("De tussenruimte moet een getal van 0 of meer zijn.", "The gap must be a number of 0 or more."),
            locale,
        ),
        _ => pick(("Onbekende melding.", "Unknown message."), locale),
    }
}

/// Why Warp/Deform cannot be applied, for the panel next to the handles.
pub fn reshape_problem_text(p: ReshapeProblem, locale: Locale) -> String {
    pick(
        match p {
            ReshapeProblem::NotConvex => (
                "De vierhoek is niet bol: sleep een hoek terug.",
                "The quadrilateral is not convex: drag a corner back.",
            ),
            ReshapeProblem::Degenerate => {
                ("De hoeken omsluiten geen oppervlak.", "The corners do not enclose any area.")
            }
            ReshapeProblem::Folded => (
                "Het net vouwt het vlak dubbel: sleep een greep terug.",
                "The net folds the plane over itself: drag a handle back.",
            ),
            ReshapeProblem::Other => ("Deze vervorming kan niet.", "This reshape is not possible."),
        },
        locale,
    )
}

/// Every key this module adds to the catalogue.
pub(super) fn known_keys() -> Vec<MessageKey> {
    let mut keys: Vec<MessageKey> = Vec::new();
    for f in [
        PlaceFailure::NoContact,
        PlaceFailure::Overlapping,
        PlaceFailure::ReferenceNotSelected,
        PlaceFailure::TooFewObjects,
        PlaceFailure::EmptyGeometry,
        PlaceFailure::NothingToMove,
        PlaceFailure::Refused,
    ] {
        keys.push(MessageKey::Schikken(SchikkenMessage::Dock(f)));
        keys.push(MessageKey::Schikken(SchikkenMessage::MoveTogether(f)));
    }
    keys.push(MessageKey::Schikken(SchikkenMessage::InvalidGap));
    keys.extend(SLOT_FAILURES.map(|f| MessageKey::Schikken(SchikkenMessage::Slots(f))));
    keys.extend(NEST_FAILURES.map(|f| MessageKey::Schikken(SchikkenMessage::Nest(f))));
    keys.extend(REGISTRATION_FAILURES.map(|f| MessageKey::Schikken(SchikkenMessage::Registration(f))));
    keys.push(MessageKey::Schikken(SchikkenMessage::Registration(RegistrationFailure::Refused)));
    for label in EDIT_LABELS {
        let label = burnburn_document::UndoLabel::Edit(label);
        keys.push(MessageKey::UndoAction(label));
        keys.push(MessageKey::RedoAction(label));
    }
    keys
}
