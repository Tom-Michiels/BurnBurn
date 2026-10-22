//! Texts of the golf-7 editor panels (G7-editor): the modifier stack (E06), plates and variants
//! (S07, P06), several texts at once and text on a path (E07, E08). NL/EN, kept apart so the older
//! text files stay untouched; the Windows host compiles this file too.

use crate::l10n::Locale;

macro_rules! texts {
    ($($name:ident => ($nl:expr, $en:expr)),* $(,)?) => {
        /// A text only the golf-7 editor panels show.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum G7Text {
            $($name),*
        }

        impl G7Text {
            /// Every text, for the completeness test.
            pub const ALL: &'static [G7Text] = &[$(G7Text::$name),*];
        }

        fn pair(text: G7Text) -> (&'static str, &'static str) {
            match text {
                $(G7Text::$name => ($nl, $en)),*
            }
        }
    };
}

texts! {
    // --- Modifier stack (E06).
    ModifiersHeading => ("Modifierstapel", "Modifier stack"),
    ModifiersNone => (
        "Geen modifierketens. Maak een patroon (Schikken ▸ Reeks…) of een offsetketen hieronder.",
        "No modifier chains. Make a pattern (Arrange ▸ Array…) or an offset chain below."
    ),
    ModifierLine => (
        "{0}: bron {1}, {2} resultaat-objecten, {3}",
        "{0}: source {1}, {2} result objects, {3}"
    ),
    ModifierSourceMissing => ("(bron ontbreekt)", "(source missing)"),
    ModifierValid => ("in orde", "valid"),
    ModifierPending => ("wordt berekend", "being computed"),
    ModifierFailed => (
        "mislukt; het laatste geldige resultaat blijft en productie is geblokkeerd",
        "failed; the last valid result stays and production is blocked"
    ),
    ModifierFailedStep => (
        "Stap {0} kan niet berekend worden; het laatste geldige resultaat blijft en productie is geblokkeerd.",
        "Step {0} cannot be computed; the last valid result stays and production is blocked."
    ),
    StepOffset => ("Offset", "Offset"),
    StepBoolean => ("Boolean", "Boolean"),
    StepLinearArray => ("Lineaire reeks", "Linear array"),
    StepGridArray => ("Rasterreeks", "Grid array"),
    StepCircularArray => ("Cirkelreeks", "Circular array"),
    StepPathArray => ("Reeks langs pad", "Array along path"),
    StepEnabled => ("Stap {0}: {1} actief", "Step {0}: {1} on"),
    StepCount => ("Stap {0}: aantal", "Step {0}: count"),
    StepDistance => ("Stap {0}: afstand (mm)", "Step {0}: distance (mm)"),
    StepUp => ("Stap {0} omhoog", "Move step {0} up"),
    StepDown => ("Stap {0} omlaag", "Move step {0} down"),
    StepRemove => ("Stap {0} verwijderen", "Remove step {0}"),
    ChainRemove => ("Keten {0} verwijderen", "Remove chain {0}"),
    ChainBake => ("Keten {0} toepassen", "Apply chain {0}"),
    ChainBakeHint => (
        "Toepassen maakt het resultaat tot gewone objecten; de bron blijft staan.",
        "Applying turns the result into plain objects; the source stays."
    ),
    MakeOffsetHeading => ("Offsetketen op de selectie", "Offset chain on the selection"),
    MakeOffsetDistance => ("Afstand (mm, positief vergroot)", "Distance (mm, positive grows)"),
    MakeOffsetButton => ("Offsetketen maken", "Make offset chain"),
    // --- Adding a step, and the stack of the selection (G8, E06).
    StackTitle => ("Stapel van de selectie", "Stack of the selection"),
    StackHint => (
        "De ketens waar de selectie bron of resultaat van is.",
        "The chains the selection is the source or a result of."
    ),
    AddStepHeading => ("Stap toevoegen", "Add a step"),
    AddStepHint => (
        "Vul de getallen van de soort in; een booleanstap combineert met de andere geselecteerde objecten, een reeks langs een pad volgt het andere geselecteerde object.",
        "Fill in the numbers of the kind; a boolean step combines with the other selected objects, a series along a path follows the other selected object."
    ),
    NewKindCaption => ("Soort stap", "Kind of step"),
    NewKindOffset => ("Offset", "Offset"),
    NewKindUnion => ("Boolean: vereniging", "Boolean: union"),
    NewKindDifference => ("Boolean: verschil", "Boolean: difference"),
    NewKindIntersection => ("Boolean: doorsnede", "Boolean: intersection"),
    NewKindXor => ("Boolean: exclusieve of", "Boolean: xor"),
    NewKindGrid => ("Raster", "Grid"),
    NewKindCircle => ("Cirkel", "Circle"),
    NewKindLinear => ("Lineaire reeks", "Linear series"),
    NewKindPath => ("Reeks langs een pad", "Series along a path"),
    NewLinearCount => ("Lineaire reeks: aantal", "Linear series: count"),
    NewStepX => ("Lineaire reeks: stap x (mm)", "Linear series: step x (mm)"),
    NewStepY => ("Lineaire reeks: stap y (mm)", "Linear series: step y (mm)"),
    NewPathCount => ("Reeks langs pad: aantal", "Series along a path: count"),
    NewPathOrient => ("Reeks langs pad: kopieen draaien mee met het pad", "Series along a path: copies turn with the path"),
    NewDistance => ("Offset: afstand (mm)", "Offset: distance (mm)"),
    NewColumns => ("Raster: kolommen", "Grid: columns"),
    NewRows => ("Raster: rijen", "Grid: rows"),
    NewPitchX => ("Raster: kolomafstand (mm)", "Grid: column pitch (mm)"),
    NewPitchY => ("Raster: rijafstand (mm)", "Grid: row pitch (mm)"),
    NewCount => ("Cirkel: aantal", "Circle: count"),
    NewCenterX => ("Cirkel: middelpunt x (mm)", "Circle: centre x (mm)"),
    NewCenterY => ("Cirkel: middelpunt y (mm)", "Circle: centre y (mm)"),
    NewSweep => ("Cirkel: hoek (graden)", "Circle: sweep (degrees)"),
    AddStepButton => ("Stap toevoegen aan {0}", "Add the step to {0}"),
    NewChainButton => ("Keten maken van de selectie met deze stap", "Make a chain of the selection with this step"),
    InstanceRotationCaption => (
        "Exemplaar {0} van {1}: draaihoek (graden)",
        "Instance {0} of {1}: angle (degrees)"
    ),
    InstanceTurnButton => ("Exemplaar {0} 90 graden draaien", "Turn instance {0} by 90 degrees"),
    DistributeHeading => ("Automatisch verdelen over platen", "Distribute over plates automatically"),
    DistributeHint => (
        "Zet elk exemplaar zonder plaat op een plaat waar het past (in rijen, het hoogste eerst, ook een kwartslag gedraaid). Waar het op de plaat komt, bepaalt het nesten.",
        "Puts every instance without a plate on a plate where it fits (in rows, tallest first, also turned a quarter). Where it lies on the plate is for the nest."
    ),
    DistributeGap => ("Tussenruimte (mm)", "Space between parts (mm)"),
    DistributeButton => ("Verdeel exemplaren zonder plaat", "Distribute instances without a plate"),
    TextContentTitle => ("Tekst bewerken", "Edit text"),
    TextContentTexts => ("Geselecteerde teksten ({0}):", "Selected texts ({0}):"),
    TextContentRow => ("{0}: «{1}» ({2} tekens)", "{0}: \"{1}\" ({2} characters)"),
    TextContentRowSample => (
        "{0}: «{1}» wordt «{3}» ({2} tekens in de sjabloon)",
        "{0}: \"{1}\" becomes \"{3}\" ({2} characters in the template)"
    ),
    TextContentMore => ("… en {0} meer", "... and {0} more"),
    TextContentHint => (
        "Een positie telt tekens vanaf het begin van elke tekst; een negatieve telt vanaf het eind, leeg is het eind, en voorbij het eind van een kortere tekst is dat zijn eind. Alle teksten veranderen in één stap.",
        "A position counts characters from the start of each text; a negative one counts from its end, empty is the end, and beyond the end of a shorter text is its end. All texts change in one step."
    ),
    TextContentPosition => ("Positie (tekens)", "Position (characters)"),
    TextContentInsert => ("Invoegen: tekst", "Insert: text"),
    TextContentInsertGo => ("Voeg in op de positie", "Insert at the position"),
    TextContentCount => ("Wissen: aantal tekens", "Delete: number of characters"),
    TextContentDeleteGo => ("Wis vanaf de positie", "Delete from the position"),
    DistributeNestButton => ("Verdeel en nest", "Distribute and nest"),
    DistributeNestHint => (
        "Verdeel en nest kiest de plaat net zo en laat het nesten daarna bepalen waar elk exemplaar op de plaat ligt (ook gedraaid). De platen liggen naast elkaar vanaf de oorsprong van het document, 10 mm uit elkaar. Het nesten loopt op de achtergrond.",
        "Distribute and nest picks the plate the same way and then lets the nest decide where each instance lies on its plate (also turned). The plates lie side by side from the origin of the document, 10 mm apart. The nest runs in the background."
    ),
    ProblemBadFormula => (
        "Dat is geen getal, parameternaam of formule over de parameters van het onderdeel (+ - * /, haakjes en min, max, abs, round, floor, ceil, sqrt, pow, clamp, pi; scheid argumenten met ; of ,).",
        "That is no number, parameter name or formula over the parameters of the part (+ - * /, brackets and min, max, abs, round, floor, ceil, sqrt, pow, clamp, pi; separate arguments with ; or ,)."
    ),
    ProblemNoPlates => ("Er zijn nog geen platen om over te verdelen.", "There are no plates to distribute over yet."),
    ProblemNothingToDistribute => (
        "Elk exemplaar staat al op een plaat.",
        "Every instance is on a plate already."
    ),
    ProblemNoRoomOnPlates => (
        "Minstens één exemplaar past op geen enkele plaat; er is niets verplaatst.",
        "At least one instance fits on no plate; nothing was moved."
    ),
    ProblemNeedsOperands => (
        "Een booleanstap heeft andere geselecteerde objecten nodig om mee te combineren.",
        "A boolean step needs other selected objects to combine with."
    ),
    ProblemInvalidSweep => (
        "Een hoek is een getal tussen 0 en 360 graden, niet nul.",
        "A sweep is an angle between 0 and 360 degrees, not zero."
    ),
    // --- Text that follows a path (E08).
    TextPathTitle => ("Tekst op pad", "Text on a path"),
    TextPathFollows => ("De tekst volgt het pad {0}.", "The text follows the path {0}."),
    TextPathGuideMissing => (
        "Het pad is verwijderd: de laatste omtrekken blijven staan en productie is geblokkeerd. Herstel het pad (ongedaan maken) of maak de tekst los.",
        "The path is gone: the last outlines stay and production is blocked. Bring the path back (undo) or release the text."
    ),
    TextPathDoesNotFit => (
        "De tekst past niet meer op het pad: de laatste omtrekken blijven staan en productie is geblokkeerd.",
        "The text no longer fits on the path: the last outlines stay and production is blocked."
    ),
    TextPathStart => ("Start op pad (mm)", "Start on the path (mm)"),
    TextPathOffset => ("Afstand tot pad (mm)", "Distance from the path (mm)"),
    TextPathReverse => ("Pad omkeren", "Reverse the path"),
    TextPathDistort => ("Vervormen", "Distort"),
    TextPathRigid => ("Star: letters houden hun vorm", "Rigid: letters keep their shape"),
    TextPathWarp => ("Buigen: contouren buigen mee", "Warp: outlines bend along"),
    TextPathUnbind => ("Tekst losmaken van het pad", "Release the text from the path"),
    // --- Plates (P06).
    PlatesManageHeading => ("Platen beheren", "Manage plates"),
    PlateEditHeading => ("Plaat {0}", "Plate {0}"),
    PlateFieldName => ("{0}: naam", "{0}: name"),
    PlateFieldWidth => ("{0}: breedte (mm)", "{0}: width (mm)"),
    PlateFieldHeight => ("{0}: hoogte (mm)", "{0}: height (mm)"),
    PlateFieldMaterial => ("{0}: materiaal", "{0}: material"),
    PlateFieldThickness => ("{0}: dikte (mm, leeg = geen)", "{0}: thickness (mm, empty = none)"),
    PlateAssignSelection => ("Selectie op plaat {0} zetten", "Put the selection on plate {0}"),
    PlateRemoveButton => ("Plaat {0} verwijderen", "Remove plate {0}"),
    PlateUnassign => ("Selectie van elke plaat halen", "Take the selection off any plate"),
    PlateAssignHint => (
        "Het onderdeel zelf gaat via zijn exemplaar naar een plaat (hieronder).",
        "A part goes to a plate through its instance (below)."
    ),
    NewPlateHeading => ("Nieuwe plaat", "New plate"),
    NewPlateName => ("Naam", "Name"),
    NewPlateWidth => ("Breedte (mm)", "Width (mm)"),
    NewPlateHeight => ("Hoogte (mm)", "Height (mm)"),
    NewPlateMaterial => ("Materiaal", "Material"),
    NewPlateThickness => ("Dikte (mm, leeg = geen)", "Thickness (mm, empty = none)"),
    NewPlateMake => ("Plaat maken", "Make plate"),
    InstancePlatesHeading => ("Exemplaren op platen", "Instances on plates"),
    InstancePlateCaption => ("Exemplaar {0} van {1}: plaat", "Instance {0} of {1}: plate"),
    InstancePlateNone => ("Geen plaat", "No plate"),
    // --- Parts: variants and shapes (S07).
    PartEditHeading => ("Onderdeel {0} bewerken", "Edit part {0}"),
    VariantRenameCaption => ("Variant {0}: nieuwe naam", "Variant {0}: new name"),
    VariantRemoveButton => ("Variant {0} verwijderen", "Remove variant {0}"),
    ShapesHeading => ("Vormen van {0}", "Shapes of {0}"),
    ShapesNone => ("Dit onderdeel heeft geen vormen.", "This part has no shapes."),
    ShapesHint => (
        "Een getal is in mm; de naam van een parameter of een formule over parameters (breedte / 2 + 3) laat de vorm die volgen. Een vaste vorm kan alleen verplaatst en geschaald worden.",
        "A number is in mm; the name of a parameter or a formula over parameters (breedte / 2 + 3) makes the shape follow it. A fixed shape can only be moved and resized."
    ),
    ShapeLine => ("{0}: {1}", "{0}: {1}"),
    ShapeKindRectangle => ("rechthoek", "rectangle"),
    ShapeKindCircle => ("cirkel", "circle"),
    ShapeKindFixed => ("vaste vorm (verplaatsen en schalen)", "fixed shape (move and resize)"),
    ShapeNameCaption => ("Vorm {0}: naam", "Shape {0}: name"),
    ShapeX => ("Vorm {0}: x", "Shape {0}: x"),
    ShapeY => ("Vorm {0}: y", "Shape {0}: y"),
    ShapeWidth => ("Vorm {0}: breedte", "Shape {0}: width"),
    ShapeHeight => ("Vorm {0}: hoogte", "Shape {0}: height"),
    ShapeRadius => ("Vorm {0}: straal", "Shape {0}: radius"),
    ShapeRemoveButton => ("Vorm {0} verwijderen", "Remove shape {0}"),
    ShapeAddName => ("Naam van de nieuwe vorm", "Name of the new shape"),
    ShapeAddRectangle => ("Rechthoek toevoegen", "Add a rectangle"),
    ShapeAddCircle => ("Cirkel toevoegen", "Add a circle"),
    ProblemUnknownPlate => ("Die plaat bestaat niet (meer).", "That plate no longer exists."),
    ProblemUnknownShape => ("Die vorm bestaat niet (meer).", "That shape no longer exists."),
    ProblemNoSuchField => ("Die vorm heeft dat getal niet.", "That shape has no such number."),
    ProblemVariantInUse => (
        "Een exemplaar gebruikt deze variant nog; kies daar eerst een andere.",
        "An instance still uses this variant; choose another one there first."
    ),
    ProblemPartOutputSelected => (
        "De selectie bevat de omtrek van een onderdeel; zet het exemplaar op de plaat.",
        "The selection holds a part's outline; put its instance on the plate instead."
    ),
    ProblemCannotAddFixed => (
        "Alleen een rechthoek of een cirkel kan worden toegevoegd.",
        "Only a rectangle or a circle can be added."
    ),
    ProblemUnknownModifier => ("Die modifierketen bestaat niet meer.", "That modifier chain no longer exists."),
    ProblemUnknownStep => ("Die stap bestaat niet (meer).", "That step no longer exists."),
    ProblemCannotMoveStep => ("De stap staat al aan dat einde.", "The step is already at that end."),
    ProblemLastStep => (
        "De laatste stap blijft; verwijder de hele keten als je klaar bent.",
        "The last step stays; remove the whole chain instead."
    ),
    ProblemStepHasNoValue => (
        "Deze stap heeft geen enkel getal om te bewerken.",
        "This step has no single number to edit."
    ),
    ProblemInvalidCount => (
        "Een aantal is een heel getal van minstens 1.",
        "A count is a whole number of at least 1."
    ),
}

pub fn g7_text(text: G7Text, locale: Locale) -> &'static str {
    let (nl, en) = pair(text);
    match locale {
        Locale::Nl => nl,
        Locale::En => en,
    }
}

/// `text` with `{0}`, `{1}`, ... replaced by `values`.
pub fn g7_fmt(text: G7Text, locale: Locale, values: &[&str]) -> String {
    let mut s = g7_text(text, locale).to_owned();
    for (i, v) in values.iter().enumerate() {
        s = s.replace(&format!("{{{i}}}"), v);
    }
    s
}

/// The structure edits the golf-7 editor lane made reachable from a panel, for the undo menu.
pub const STRUCTURE_LABELS: [burnburn_document::structure::StructureLabel; 14] = {
    use burnburn_document::structure::StructureLabel as L;
    [
        L::ToggleModifierStep,
        L::EditModifier,
        L::RemoveModifier,
        L::BakeModifier,
        L::CreatePlate,
        L::EditPlate,
        L::RemovePlate,
        L::AssignToPlate,
        L::EditInstance,
        L::EditPart,
        L::RenameVariant,
        L::DefinePart,
        L::PlaceInstance,
        L::RemoveInstance,
    ]
};

/// Undo/redo words of a structure edit, or `None` when the label has none (yet).
pub fn structure_undo_pair(
    label: burnburn_document::structure::StructureLabel,
) -> Option<(&'static str, &'static str)> {
    use burnburn_document::structure::StructureLabel as L;
    Some(match label {
        L::CreateModifier => ("Patroon maken", "Create Pattern"),
        L::ToggleModifierStep => ("Modifierstap schakelen", "Toggle Modifier Step"),
        L::EditModifier => ("Modifierketen wijzigen", "Edit Modifier Chain"),
        L::RemoveModifier => ("Modifierketen verwijderen", "Remove Modifier Chain"),
        L::BakeModifier => ("Modifierketen toepassen", "Apply Modifier Chain"),
        L::CreatePlate => ("Plaat maken", "Make Plate"),
        L::EditPlate => ("Plaat wijzigen", "Edit Plate"),
        L::RemovePlate => ("Plaat verwijderen", "Remove Plate"),
        L::AssignToPlate => ("Aan plaat toewijzen", "Assign to Plate"),
        L::EditInstance => ("Exemplaar wijzigen", "Edit Instance"),
        L::EditPart => ("Onderdeel wijzigen", "Edit Part"),
        L::RenameVariant => ("Variant hernoemen", "Rename Variant"),
        L::DefinePart => ("Onderdeel maken", "Define Part"),
        L::PlaceInstance => ("Exemplaar plaatsen", "Place Instance"),
        L::RemoveInstance => ("Exemplaar verwijderen", "Remove Instance"),
        _ => return None,
    })
}

/// The words for a refused structure edit of the golf-7 editor lane, or `None` for the older ones.
pub fn structure_problem_text(problem: burnburn_presentation::StructureProblem, locale: Locale) -> Option<String> {
    use burnburn_presentation::StructureProblem as P;
    let text = match problem {
        P::UnknownModifier => G7Text::ProblemUnknownModifier,
        P::UnknownStep => G7Text::ProblemUnknownStep,
        P::CannotMoveStep => G7Text::ProblemCannotMoveStep,
        P::LastStep => G7Text::ProblemLastStep,
        P::StepHasNoValue => G7Text::ProblemStepHasNoValue,
        P::InvalidCount => G7Text::ProblemInvalidCount,
        P::NeedsOperands => G7Text::ProblemNeedsOperands,
        P::BadFormula => G7Text::ProblemBadFormula,
        P::NoPlates => G7Text::ProblemNoPlates,
        P::NothingToDistribute => G7Text::ProblemNothingToDistribute,
        P::NoRoomOnPlates => G7Text::ProblemNoRoomOnPlates,
        P::InvalidSweep => G7Text::ProblemInvalidSweep,
        P::UnknownPlate => G7Text::ProblemUnknownPlate,
        P::UnknownShape => G7Text::ProblemUnknownShape,
        P::NoSuchField => G7Text::ProblemNoSuchField,
        P::VariantInUse => G7Text::ProblemVariantInUse,
        P::PartOutputSelected => G7Text::ProblemPartOutputSelected,
        P::CannotAddFixed => G7Text::ProblemCannotAddFixed,
        _ => return None,
    };
    Some(g7_text(text, locale).to_owned())
}
