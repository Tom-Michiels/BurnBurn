//! NL/EN texts of the Schikken flows (W3-UI-schikken): dock, move together, warp/deform, resize
//! slots, tangent circle, print and cut, center finder and nesting. Kept apart so `l10n.rs` only
//! gets a hook per table (the Windows host compiles the same files).

use burnburn_document::EditLabel;
use burnburn_presentation::{
    CommandKey, MessageKey, PlaceFailure, ReshapeProblem, SchikkenMessage, SlotField, SlotsFailure,
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
        _ => return None,
    })
}

/// The edit labels this module owns (for the completeness test).
pub(super) const EDIT_LABELS: [EditLabel; 5] =
    [EditLabel::Dock, EditLabel::MoveTogether, EditLabel::Warp, EditLabel::Deform, EditLabel::ResizeSlots];

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

/// Text of a Schikken status message.
pub(super) fn message(m: SchikkenMessage, locale: Locale) -> String {
    match m {
        SchikkenMessage::Dock(f) | SchikkenMessage::MoveTogether(f) => pick(failure_pair(f), locale),
        SchikkenMessage::Slots(f) => pick(slots_pair(f), locale),
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
    for label in EDIT_LABELS {
        let label = burnburn_document::UndoLabel::Edit(label);
        keys.push(MessageKey::UndoAction(label));
        keys.push(MessageKey::RedoAction(label));
    }
    keys
}
