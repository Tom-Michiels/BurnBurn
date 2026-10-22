//! The golf-7 editor panels, headless (G7-editor): the modifier stack (E06) as a section of the
//! structure panel, plates and variants (S07, P06). Same plain [`G5Line`]s and controls as the
//! golf-5 panels, so both hosts draw them without code of their own. Field text goes to
//! presentation unparsed (DR-05); a control whose value is what presentation already shows
//! sends nothing.

use burnburn_presentation::{
    Distort, DocumentViewModel, ModifierKind, ModifierRow, ModifierStatus, ModifierValueKind, PartShapeKind,
    PathBindingStatus, PlateField, ShapeField, StructureEdit, StructureViewModel, TextEdit, UiIntent,
};
use burnburn_types::DocumentId;

use crate::l10n::Locale;
use crate::l10n_g7_editor::{G7Text, g7_fmt, g7_text};

use super::g5::{G5Control, G5Line, flag_of, value_of};

/// A control of the golf-7 editor sections. Fields report their text, checks `1` / `0`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EditorControl {
    /// Step `step` of chain `modifier` on or off.
    ModStepOn {
        modifier: u64,
        step: usize,
    },
    /// The step's main number.
    ModStepValue {
        modifier: u64,
        step: usize,
    },
    ModStepUp {
        modifier: u64,
        step: usize,
    },
    ModStepDown {
        modifier: u64,
        step: usize,
    },
    ModStepRemove {
        modifier: u64,
        step: usize,
    },
    ModRemove {
        modifier: u64,
    },
    ModBake {
        modifier: u64,
    },
    /// The distance for a new offset chain on the selection, and the button that makes it.
    ModOffsetDistance,
    ModOffsetMake,
    // --- Plates (P06).
    /// One text field of plate `plate`.
    PlateText {
        plate: u64,
        field: PlateField,
    },
    /// The selection onto plate `plate`; `PlateUnassign` takes it off any plate.
    PlateAssign {
        plate: u64,
    },
    PlateUnassign,
    PlateRemove {
        plate: u64,
    },
    /// A field of the plate that is going to be made.
    NewPlateText(PlateField),
    NewPlateMake,
    /// The plate of an instance (a choice: none, then the plates in order).
    InstancePlate {
        instance: u64,
    },
    // --- Parts: variants and shapes (S07).
    /// The new name of variant number `variant` (position in the part's list).
    VariantRename {
        part: u64,
        variant: usize,
    },
    VariantRemove {
        part: u64,
        variant: usize,
    },
    ShapeName {
        part: u64,
        shape: u32,
    },
    ShapeNumber {
        part: u64,
        shape: u32,
        field: ShapeField,
    },
    ShapeRemove {
        part: u64,
        shape: u32,
    },
    ShapeAddName {
        part: u64,
    },
    ShapeAdd {
        part: u64,
        kind: PartShapeKind,
    },
    // --- Text that follows a path (E08).
    TextPathStart,
    TextPathOffset,
    TextPathReverse,
    /// A choice: rigid, then warp.
    TextPathDistort,
    TextPathUnbind,
}

fn t(text: G7Text, locale: Locale) -> String {
    g7_text(text, locale).to_owned()
}

fn kind_name(kind: ModifierKind, locale: Locale) -> String {
    t(
        match kind {
            ModifierKind::Offset => G7Text::StepOffset,
            ModifierKind::Boolean => G7Text::StepBoolean,
            ModifierKind::LinearArray => G7Text::StepLinearArray,
            ModifierKind::GridArray => G7Text::StepGridArray,
            ModifierKind::CircularArray => G7Text::StepCircularArray,
            ModifierKind::PathArray => G7Text::StepPathArray,
            _ => G7Text::StepBoolean,
        },
        locale,
    )
}

fn button(control: EditorControl, title: String, enabled: bool) -> G5Line {
    G5Line::Button { control: G5Control::Editor(control), title, enabled }
}

fn field(control: EditorControl, caption: String, text: String) -> G5Line {
    G5Line::Field { control: G5Control::Editor(control), caption, text }
}

/// Everything the golf-7 editor adds to the structure panel, in order: the modifier stack, the
/// plates to manage and the parts to edit.
pub(super) fn editor_lines(s: &StructureViewModel, locale: Locale) -> Vec<G5Line> {
    let mut lines = modifier_lines(s, locale);
    lines.extend(plate_lines(s, locale));
    lines.extend(part_edit_lines(s, locale));
    lines
}

fn plate_field_text(field: PlateField, locale: Locale) -> G7Text {
    let _ = locale;
    match field {
        PlateField::Name => G7Text::PlateFieldName,
        PlateField::Width => G7Text::PlateFieldWidth,
        PlateField::Height => G7Text::PlateFieldHeight,
        PlateField::Material => G7Text::PlateFieldMaterial,
        _ => G7Text::PlateFieldThickness,
    }
}

const PLATE_FIELDS: [PlateField; 5] =
    [PlateField::Name, PlateField::Width, PlateField::Height, PlateField::Material, PlateField::Thickness];

fn plate_lines(s: &StructureViewModel, locale: Locale) -> Vec<G5Line> {
    let mut lines = vec![G5Line::Heading(t(G7Text::PlatesManageHeading, locale))];
    lines.push(G5Line::Text(t(G7Text::PlateAssignHint, locale)));
    for plate in &s.plates {
        lines.push(G5Line::Heading(g7_fmt(G7Text::PlateEditHeading, locale, &[&plate.name])));
        for f in PLATE_FIELDS {
            let text = match f {
                PlateField::Name => plate.name.clone(),
                PlateField::Width => plate.width.clone(),
                PlateField::Height => plate.height.clone(),
                PlateField::Material => plate.material.clone(),
                _ => plate.thickness.clone().unwrap_or_default(),
            };
            let caption = g7_fmt(plate_field_text(f, locale), locale, &[&plate.name]);
            lines.push(field(EditorControl::PlateText { plate: plate.id, field: f }, caption, text));
        }
        lines.push(button(
            EditorControl::PlateAssign { plate: plate.id },
            g7_fmt(G7Text::PlateAssignSelection, locale, &[&plate.name]),
            s.selected > 0,
        ));
        lines.push(button(
            EditorControl::PlateRemove { plate: plate.id },
            g7_fmt(G7Text::PlateRemoveButton, locale, &[&plate.name]),
            true,
        ));
    }
    lines.push(button(EditorControl::PlateUnassign, t(G7Text::PlateUnassign, locale), s.selected > 0));

    lines.push(G5Line::Heading(t(G7Text::NewPlateHeading, locale)));
    for (f, caption) in [
        (PlateField::Name, G7Text::NewPlateName),
        (PlateField::Width, G7Text::NewPlateWidth),
        (PlateField::Height, G7Text::NewPlateHeight),
        (PlateField::Material, G7Text::NewPlateMaterial),
        (PlateField::Thickness, G7Text::NewPlateThickness),
    ] {
        lines.push(field(EditorControl::NewPlateText(f), t(caption, locale), String::new()));
    }
    lines.push(button(EditorControl::NewPlateMake, t(G7Text::NewPlateMake, locale), true));

    if s.parts.iter().any(|p| !p.instances.is_empty()) {
        lines.push(G5Line::Heading(t(G7Text::InstancePlatesHeading, locale)));
        for part in &s.parts {
            for instance in &part.instances {
                let mut options = vec![t(G7Text::InstancePlateNone, locale)];
                options.extend(s.plates.iter().map(|p| p.name.clone()));
                let selected =
                    instance.plate_id.and_then(|id| s.plates.iter().position(|p| p.id == id)).map_or(0, |i| i + 1);
                lines.push(G5Line::Choice {
                    control: G5Control::Editor(EditorControl::InstancePlate { instance: instance.id }),
                    caption: g7_fmt(G7Text::InstancePlateCaption, locale, &[&instance.id.to_string(), &part.name]),
                    options,
                    selected,
                });
            }
        }
    }
    lines
}

fn shape_field_text(field: ShapeField) -> G7Text {
    match field {
        ShapeField::X => G7Text::ShapeX,
        ShapeField::Y => G7Text::ShapeY,
        ShapeField::Width => G7Text::ShapeWidth,
        ShapeField::Height => G7Text::ShapeHeight,
        _ => G7Text::ShapeRadius,
    }
}

fn part_edit_lines(s: &StructureViewModel, locale: Locale) -> Vec<G5Line> {
    let mut lines = Vec::new();
    for part in &s.parts {
        lines.push(G5Line::Heading(g7_fmt(G7Text::PartEditHeading, locale, &[&part.name])));
        for (index, name) in part.variants.iter().enumerate() {
            lines.push(field(
                EditorControl::VariantRename { part: part.id, variant: index },
                g7_fmt(G7Text::VariantRenameCaption, locale, &[name]),
                name.clone(),
            ));
            lines.push(button(
                EditorControl::VariantRemove { part: part.id, variant: index },
                g7_fmt(G7Text::VariantRemoveButton, locale, &[name]),
                true,
            ));
        }
        lines.push(G5Line::Heading(g7_fmt(G7Text::ShapesHeading, locale, &[&part.name])));
        if part.shapes.is_empty() {
            lines.push(G5Line::Text(t(G7Text::ShapesNone, locale)));
        } else {
            lines.push(G5Line::Text(t(G7Text::ShapesHint, locale)));
        }
        for shape in &part.shapes {
            let kind = t(
                match shape.kind {
                    PartShapeKind::Rectangle => G7Text::ShapeKindRectangle,
                    PartShapeKind::Circle => G7Text::ShapeKindCircle,
                    _ => G7Text::ShapeKindFixed,
                },
                locale,
            );
            lines.push(G5Line::Text(g7_fmt(G7Text::ShapeLine, locale, &[&shape.name, &kind])));
            lines.push(field(
                EditorControl::ShapeName { part: part.id, shape: shape.key },
                g7_fmt(G7Text::ShapeNameCaption, locale, &[&shape.name]),
                shape.name.clone(),
            ));
            for (f, text) in &shape.fields {
                lines.push(field(
                    EditorControl::ShapeNumber { part: part.id, shape: shape.key, field: *f },
                    g7_fmt(shape_field_text(*f), locale, &[&shape.name]),
                    text.clone(),
                ));
            }
            lines.push(button(
                EditorControl::ShapeRemove { part: part.id, shape: shape.key },
                g7_fmt(G7Text::ShapeRemoveButton, locale, &[&shape.name]),
                true,
            ));
        }
        lines.push(field(
            EditorControl::ShapeAddName { part: part.id },
            t(G7Text::ShapeAddName, locale),
            String::new(),
        ));
        lines.push(button(
            EditorControl::ShapeAdd { part: part.id, kind: PartShapeKind::Rectangle },
            t(G7Text::ShapeAddRectangle, locale),
            true,
        ));
        lines.push(button(
            EditorControl::ShapeAdd { part: part.id, kind: PartShapeKind::Circle },
            t(G7Text::ShapeAddCircle, locale),
            true,
        ));
    }
    lines
}

/// The modifier-stack section: every chain with its steps, and the way to make an offset chain.
pub(super) fn modifier_lines(s: &StructureViewModel, locale: Locale) -> Vec<G5Line> {
    let mut lines = vec![G5Line::Heading(t(G7Text::ModifiersHeading, locale))];
    if s.modifiers.is_empty() {
        lines.push(G5Line::Text(t(G7Text::ModifiersNone, locale)));
    }
    for chain in &s.modifiers {
        lines.extend(chain_lines(chain, locale));
    }
    lines.push(G5Line::Heading(t(G7Text::MakeOffsetHeading, locale)));
    lines.push(G5Line::Field {
        control: G5Control::Editor(EditorControl::ModOffsetDistance),
        caption: t(G7Text::MakeOffsetDistance, locale),
        text: String::new(),
    });
    lines.push(button(EditorControl::ModOffsetMake, t(G7Text::MakeOffsetButton, locale), s.selected == 1));
    lines
}

fn chain_lines(chain: &ModifierRow, locale: Locale) -> Vec<G5Line> {
    let mut lines = vec![G5Line::Heading(chain.name.clone())];
    let status = match chain.status {
        ModifierStatus::Valid => t(G7Text::ModifierValid, locale),
        ModifierStatus::Pending => t(G7Text::ModifierPending, locale),
        _ => t(G7Text::ModifierFailed, locale),
    };
    let source = chain.source_name.clone().unwrap_or_else(|| t(G7Text::ModifierSourceMissing, locale));
    lines.push(G5Line::Text(g7_fmt(
        G7Text::ModifierLine,
        locale,
        &[&chain.name, &source, &chain.outputs.to_string(), &status],
    )));
    if let ModifierStatus::Failed { step: Some(step) } = chain.status {
        lines.push(G5Line::Problem(g7_fmt(G7Text::ModifierFailedStep, locale, &[&(step + 1).to_string()])));
    }
    let last = chain.steps.len().saturating_sub(1);
    for step in &chain.steps {
        let (modifier, index) = (chain.id, step.index);
        let n = (index + 1).to_string();
        lines.push(G5Line::Check {
            control: G5Control::Editor(EditorControl::ModStepOn { modifier, step: index }),
            caption: g7_fmt(G7Text::StepEnabled, locale, &[&n, &kind_name(step.kind, locale)]),
            on: step.enabled,
        });
        if let Some((kind, value)) = &step.value {
            let caption = match kind {
                ModifierValueKind::Distance => G7Text::StepDistance,
                _ => G7Text::StepCount,
            };
            lines.push(G5Line::Field {
                control: G5Control::Editor(EditorControl::ModStepValue { modifier, step: index }),
                caption: g7_fmt(caption, locale, &[&n]),
                text: value.clone(),
            });
        }
        lines.push(button(
            EditorControl::ModStepUp { modifier, step: index },
            g7_fmt(G7Text::StepUp, locale, &[&n]),
            index > 0,
        ));
        lines.push(button(
            EditorControl::ModStepDown { modifier, step: index },
            g7_fmt(G7Text::StepDown, locale, &[&n]),
            index < last,
        ));
        lines.push(button(
            EditorControl::ModStepRemove { modifier, step: index },
            g7_fmt(G7Text::StepRemove, locale, &[&n]),
            chain.steps.len() > 1,
        ));
    }
    lines.push(G5Line::Text(t(G7Text::ChainBakeHint, locale)));
    lines.push(button(
        EditorControl::ModBake { modifier: chain.id },
        g7_fmt(G7Text::ChainBake, locale, &[&chain.name]),
        true,
    ));
    lines.push(button(
        EditorControl::ModRemove { modifier: chain.id },
        g7_fmt(G7Text::ChainRemove, locale, &[&chain.name]),
        true,
    ));
    lines
}

/// The panel of a text that follows a guide path: where it starts, how far from the path, which
/// way, how it bends, and the way to release it. `None` when the selection is no such text.
pub(super) fn text_path_lines(doc: &DocumentViewModel, locale: Locale) -> Option<Vec<G5Line>> {
    let p = doc.text_toolbar.as_ref()?.path.as_ref()?;
    let mut lines = Vec::new();
    match p.status {
        PathBindingStatus::Following => {
            lines.push(G5Line::Text(g7_fmt(G7Text::TextPathFollows, locale, &[p.guide.as_deref().unwrap_or_default()])))
        }
        PathBindingStatus::GuideMissing => lines.push(G5Line::Problem(t(G7Text::TextPathGuideMissing, locale))),
        _ => lines.push(G5Line::Problem(t(G7Text::TextPathDoesNotFit, locale))),
    }
    lines.push(field(EditorControl::TextPathStart, t(G7Text::TextPathStart, locale), p.start.clone()));
    lines.push(field(EditorControl::TextPathOffset, t(G7Text::TextPathOffset, locale), p.offset.clone()));
    lines.push(G5Line::Check {
        control: G5Control::Editor(EditorControl::TextPathReverse),
        caption: t(G7Text::TextPathReverse, locale),
        on: p.reverse,
    });
    lines.push(G5Line::Choice {
        control: G5Control::Editor(EditorControl::TextPathDistort),
        caption: t(G7Text::TextPathDistort, locale),
        options: vec![t(G7Text::TextPathRigid, locale), t(G7Text::TextPathWarp, locale)],
        selected: usize::from(p.distort == Distort::Warp),
    });
    lines.push(button(EditorControl::TextPathUnbind, t(G7Text::TextPathUnbind, locale), true));
    if let Some(issue) = doc.text_toolbar.as_ref().and_then(|b| b.issue) {
        lines.push(G5Line::Problem(super::text::text_issue_text(issue, locale)));
    }
    Some(lines)
}

fn structure(document: DocumentId, edit: StructureEdit) -> Option<UiIntent> {
    Some(UiIntent::Structure { document, edit })
}

/// The intent of a control of the golf-7 editor sections.
pub(super) fn editor_intent(
    doc: &DocumentViewModel,
    document: DocumentId,
    control: EditorControl,
    values: &[(G5Control, String)],
) -> Option<UiIntent> {
    let typed = |c: EditorControl| value_of(values, G5Control::Editor(c)).map(str::to_owned);
    let chain = |id: u64| doc.structure.modifiers.iter().find(|m| m.id == id);
    match control {
        EditorControl::ModStepOn { modifier, step } => {
            let current = chain(modifier)?.steps.get(step)?.enabled;
            let enabled = flag_of(values, G5Control::Editor(control))?;
            (enabled != current).then(|| UiIntent::Structure {
                document,
                edit: StructureEdit::ToggleModifierStep { modifier, step, enabled },
            })
        }
        EditorControl::ModStepValue { modifier, step } => {
            let text = typed(control)?;
            let current = chain(modifier)?.steps.get(step)?.value.as_ref().map(|(_, v)| v.clone())?;
            (text.trim() != current).then(|| UiIntent::Structure {
                document,
                edit: StructureEdit::SetModifierStepValue { modifier, step, text },
            })
        }
        EditorControl::ModStepUp { modifier, step } => {
            structure(document, StructureEdit::MoveModifierStep { modifier, step, up: true })
        }
        EditorControl::ModStepDown { modifier, step } => {
            structure(document, StructureEdit::MoveModifierStep { modifier, step, up: false })
        }
        EditorControl::ModStepRemove { modifier, step } => {
            structure(document, StructureEdit::RemoveModifierStep { modifier, step })
        }
        EditorControl::ModRemove { modifier } => structure(document, StructureEdit::RemoveModifier { modifier }),
        EditorControl::ModBake { modifier } => structure(document, StructureEdit::BakeModifier { modifier }),
        EditorControl::PlateText { plate, field } => {
            let row = doc.structure.plates.iter().find(|p| p.id == plate)?;
            let current = match field {
                PlateField::Name => row.name.clone(),
                PlateField::Width => row.width.clone(),
                PlateField::Height => row.height.clone(),
                PlateField::Material => row.material.clone(),
                _ => row.thickness.clone().unwrap_or_default(),
            };
            let text = typed(control)?;
            (text.trim() != current)
                .then(|| UiIntent::Structure { document, edit: StructureEdit::SetPlateField { plate, field, text } })
        }
        EditorControl::PlateAssign { plate } => {
            structure(document, StructureEdit::AssignSelectionToPlate { plate: Some(plate) })
        }
        EditorControl::PlateUnassign => structure(document, StructureEdit::AssignSelectionToPlate { plate: None }),
        EditorControl::PlateRemove { plate } => structure(document, StructureEdit::RemovePlate { plate }),
        EditorControl::NewPlateText(_) => None,
        EditorControl::NewPlateMake => {
            let text = |f| typed(EditorControl::NewPlateText(f)).unwrap_or_default();
            structure(
                document,
                StructureEdit::CreatePlate {
                    name: text(PlateField::Name),
                    width: text(PlateField::Width),
                    height: text(PlateField::Height),
                    material: text(PlateField::Material),
                    thickness: text(PlateField::Thickness),
                },
            )
        }
        EditorControl::InstancePlate { instance } => {
            let s = &doc.structure;
            let current = s.parts.iter().flat_map(|p| p.instances.iter()).find(|i| i.id == instance)?.plate_id;
            let chosen = match value_of(values, G5Control::Editor(control))?.trim().parse::<usize>().ok()? {
                0 => None,
                i => Some(s.plates.get(i - 1)?.id),
            };
            (chosen != current).then(|| UiIntent::Structure {
                document,
                edit: StructureEdit::SetInstancePlate { instance, plate: chosen },
            })
        }
        EditorControl::VariantRename { part, variant } => {
            let from = doc.structure.parts.iter().find(|p| p.id == part)?.variants.get(variant)?.clone();
            let to = typed(control)?;
            (to.trim() != from).then(|| UiIntent::Structure {
                document,
                edit: StructureEdit::RenameVariant { definition: part, from, to },
            })
        }
        EditorControl::VariantRemove { part, variant } => {
            let name = doc.structure.parts.iter().find(|p| p.id == part)?.variants.get(variant)?.clone();
            structure(document, StructureEdit::RemoveVariant { definition: part, name })
        }
        EditorControl::ShapeName { part, shape } => {
            let row = doc.structure.parts.iter().find(|p| p.id == part)?.shapes.iter().find(|s| s.key == shape)?;
            let name = typed(control)?;
            (name.trim() != row.name).then(|| UiIntent::Structure {
                document,
                edit: StructureEdit::RenamePartShape { definition: part, shape, name },
            })
        }
        EditorControl::ShapeNumber { part, shape, field } => {
            let row = doc.structure.parts.iter().find(|p| p.id == part)?.shapes.iter().find(|s| s.key == shape)?;
            let current = row.fields.iter().find(|(f, _)| *f == field).map(|(_, v)| v.clone())?;
            let text = typed(control)?;
            (text.trim() != current).then(|| UiIntent::Structure {
                document,
                edit: StructureEdit::SetPartShapeField { definition: part, shape, field, text },
            })
        }
        EditorControl::ShapeRemove { part, shape } => {
            structure(document, StructureEdit::RemovePartShape { definition: part, shape })
        }
        EditorControl::ShapeAddName { .. } => None,
        EditorControl::ShapeAdd { part, kind } => structure(
            document,
            StructureEdit::AddPartShape {
                definition: part,
                kind,
                name: typed(EditorControl::ShapeAddName { part }).unwrap_or_default(),
            },
        ),
        EditorControl::TextPathStart | EditorControl::TextPathOffset => {
            let p = doc.text_toolbar.as_ref()?.path.as_ref()?;
            let text = typed(control)?;
            let (shown, edit): (&str, fn(String) -> TextEdit) = if control == EditorControl::TextPathStart {
                (p.start.as_str(), TextEdit::PathStart)
            } else {
                (p.offset.as_str(), TextEdit::PathOffset)
            };
            (text.trim() != shown).then(|| UiIntent::Text { document, edit: edit(text) })
        }
        EditorControl::TextPathReverse => {
            let on = flag_of(values, G5Control::Editor(control))?;
            (on != doc.text_toolbar.as_ref()?.path.as_ref()?.reverse)
                .then(|| UiIntent::Text { document, edit: TextEdit::PathReverse(on) })
        }
        EditorControl::TextPathDistort => {
            let chosen = if super::g5::index_of(values, G5Control::Editor(control))? == 1 {
                Distort::Warp
            } else {
                Distort::Rigid
            };
            (chosen != doc.text_toolbar.as_ref()?.path.as_ref()?.distort)
                .then(|| UiIntent::Text { document, edit: TextEdit::PathDistort(chosen) })
        }
        EditorControl::TextPathUnbind => {
            doc.text_toolbar.as_ref()?.path.as_ref()?;
            Some(UiIntent::Text { document, edit: TextEdit::UnbindPath })
        }
        EditorControl::ModOffsetDistance => None,
        EditorControl::ModOffsetMake => structure(
            document,
            StructureEdit::CreateOffsetModifier {
                distance: typed(EditorControl::ModOffsetDistance).unwrap_or_default(),
            },
        ),
    }
}
