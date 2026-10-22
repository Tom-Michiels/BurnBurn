//! The golf-7 editor panels, headless (G7-editor): the modifier stack (E06) as a section of the
//! structure panel, plates and variants (S07, P06). Same plain [`G5Line`]s and controls as the
//! golf-5 panels, so both hosts draw them without code of their own. Field text goes to
//! presentation unparsed (DR-05); a control whose value is what presentation already shows
//! sends nothing.

use burnburn_presentation::{
    DocumentViewModel, ModifierKind, ModifierRow, ModifierStatus, ModifierValueKind, StructureEdit, StructureViewModel,
    UiIntent,
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
        EditorControl::ModOffsetDistance => None,
        EditorControl::ModOffsetMake => structure(
            document,
            StructureEdit::CreateOffsetModifier {
                distance: typed(EditorControl::ModOffsetDistance).unwrap_or_default(),
            },
        ),
    }
}
