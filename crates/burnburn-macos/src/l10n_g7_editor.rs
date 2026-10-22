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
        "Een getal is in mm; de naam van een parameter laat de vorm die parameter volgen.",
        "A number is in mm; the name of a parameter makes the shape follow that parameter."
    ),
    ShapeLine => ("{0}: {1}", "{0}: {1}"),
    ShapeKindRectangle => ("rechthoek", "rectangle"),
    ShapeKindCircle => ("cirkel", "circle"),
    ShapeKindFixed => ("vaste vorm (geen getallen)", "fixed shape (no numbers)"),
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
