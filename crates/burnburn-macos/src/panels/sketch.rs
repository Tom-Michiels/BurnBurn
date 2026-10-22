//! The sketch panel (W1-L1b, B8): the constraint list of the open sketch with its value column,
//! the degrees of freedom or the conflict in words, and the parameters. Like every panel it only
//! shows what presentation's view model says and forwards raw text (DR-05).

use burnburn_document::sketch::{ConstraintId, DimensionRole, EntityId, SketchId};
use burnburn_presentation::{
    AppViewModel, ConstraintHealth, ConstraintRow, MessageKey, SelectMode, SketchModeViewModel, SketchStatus, UiIntent,
};
use burnburn_types::DocumentId;

use crate::l10n::{HostText, Locale, host_text, message_or_key};

/// Columns of the constraint table, left to right.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SketchColumn {
    Kind,
    Entities,
    /// A dimension's text, editable.
    Value,
    Status,
}

pub const SKETCH_COLUMNS: [SketchColumn; 4] =
    [SketchColumn::Kind, SketchColumn::Entities, SketchColumn::Value, SketchColumn::Status];

impl SketchColumn {
    /// `NSTableColumn` identifier.
    pub fn id(self) -> &'static str {
        match self {
            SketchColumn::Kind => "sketch-kind",
            SketchColumn::Entities => "sketch-entities",
            SketchColumn::Value => "sketch-value",
            SketchColumn::Status => "sketch-status",
        }
    }

    pub fn from_id(id: &str) -> Option<SketchColumn> {
        SKETCH_COLUMNS.iter().copied().find(|c| c.id() == id)
    }

    pub fn is_editable(self) -> bool {
        self == SketchColumn::Value
    }

    pub fn caption(self, locale: Locale) -> &'static str {
        let text = match self {
            SketchColumn::Kind => HostText::SketchColumnKind,
            SketchColumn::Entities => HostText::SketchColumnEntities,
            SketchColumn::Value => HostText::SketchColumnValue,
            SketchColumn::Status => HostText::SketchColumnStatus,
        };
        host_text(text, locale)
    }
}

/// One constraint as the table shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SketchLine {
    pub id: ConstraintId,
    pub kind: String,
    pub entities: String,
    /// A dimension: its text as typed (editable); a relation: empty.
    pub value: String,
    pub editable: bool,
    pub status: String,
    pub selected: bool,
    pub driven: bool,
}

impl SketchLine {
    pub fn text(&self, column: SketchColumn) -> String {
        match column {
            SketchColumn::Kind => self.kind.clone(),
            SketchColumn::Entities => self.entities.clone(),
            SketchColumn::Value => self.value.clone(),
            SketchColumn::Status => self.status.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SketchPanel {
    pub sketch: SketchId,
    pub title: String,
    /// Degrees of freedom, "fully constrained" or "not solved yet".
    pub freedom: String,
    /// Why the sketch failed, or empty; clicking it selects the constraints involved.
    pub problem: String,
    pub problem_hint: String,
    /// Why the last sketch input did nothing, or empty.
    pub refused: String,
    pub rows: Vec<SketchLine>,
    pub empty_text: String,
    pub parameters: Vec<(String, String)>,
    pub hint: String,
    /// The Remove button applies (a constraint is selected).
    pub remove_enabled: bool,
    /// Measure/Drive applies (a dimension is selected).
    pub role_enabled: bool,
    /// "Herstel laatste geldige" applies (the sketch failed and there is history to go back in).
    pub restore_enabled: bool,
}

fn mode_of(vm: &AppViewModel, document: DocumentId) -> Option<&SketchModeViewModel> {
    vm.document(document)?.sketch_mode.as_ref()
}

fn health_text(row: &ConstraintRow, locale: Locale) -> &'static str {
    let text = match row.health {
        ConstraintHealth::Satisfied => HostText::SketchHealthSatisfied,
        ConstraintHealth::Redundant => HostText::SketchHealthRedundant,
        ConstraintHealth::Conflicting => HostText::SketchHealthConflicting,
        ConstraintHealth::Unmet => HostText::SketchHealthUnmet,
        ConstraintHealth::Invalid => HostText::SketchHealthInvalid,
        ConstraintHealth::Measured => HostText::SketchHealthMeasured,
        _ => HostText::SketchHealthUnknown,
    };
    host_text(text, locale)
}

fn entities_text(entities: &[EntityId]) -> String {
    entities.iter().map(|e| format!("#{}", e.raw())).collect::<Vec<_>>().join(", ")
}

/// The sketch panel of `document`, or `None` outside sketch mode.
pub fn sketch_panel(vm: &AppViewModel, document: DocumentId, locale: Locale) -> Option<SketchPanel> {
    let m = mode_of(vm, document)?;
    let rows = m
        .rows
        .iter()
        .map(|r| SketchLine {
            id: r.id,
            kind: message_or_key(MessageKey::SketchConstraint(r.kind), locale),
            entities: entities_text(&r.entities),
            value: r.text.clone().unwrap_or_default(),
            editable: r.text.is_some(),
            status: health_text(r, locale).to_owned(),
            selected: r.selected,
            driven: r.role == Some(DimensionRole::Driven),
        })
        .collect();
    let freedom = match (m.status, m.dof) {
        (SketchStatus::Valid, Some(0)) => host_text(HostText::SketchFullyConstrained, locale).to_owned(),
        (SketchStatus::Valid, Some(n)) => format!("{}: {n}", host_text(HostText::SketchFreedom, locale)),
        (SketchStatus::Pending, _) => host_text(HostText::SketchPending, locale).to_owned(),
        _ => String::new(),
    };
    let problem = m.problem.map(|k| message_or_key(k, locale)).unwrap_or_default();
    Some(SketchPanel {
        sketch: m.sketch,
        title: format!("{} — {}", host_text(HostText::SketchPanelTitle, locale), m.name),
        freedom,
        problem_hint: if problem.is_empty() {
            String::new()
        } else {
            host_text(HostText::SketchProblemHint, locale).to_owned()
        },
        problem,
        refused: m.refused.map(|k| message_or_key(k, locale)).unwrap_or_default(),
        rows,
        empty_text: host_text(HostText::SketchNoConstraints, locale).to_owned(),
        parameters: m.parameters.clone(),
        hint: host_text(HostText::SketchHint, locale).to_owned(),
        remove_enabled: !m.selected_constraints.is_empty(),
        role_enabled: m.rows.iter().any(|r| r.selected && r.role.is_some()),
        restore_enabled: m.can_restore,
    })
}

/// The table rows selected by the user become the selected constraints (entities stay selected).
pub fn sketch_rows_intent(vm: &AppViewModel, document: DocumentId, rows: &[usize]) -> Option<UiIntent> {
    let m = mode_of(vm, document)?;
    let mut ids: Vec<ConstraintId> = rows.iter().filter_map(|r| m.rows.get(*r).map(|row| row.id)).collect();
    ids.sort();
    if ids == m.selected_constraints {
        return None;
    }
    Some(UiIntent::SketchSelect {
        document,
        entities: m.selected_entities.clone(),
        constraints: ids,
        mode: SelectMode::Replace,
    })
}

/// A value cell edited: the raw text goes to presentation unless it is what is shown.
pub fn sketch_value_intent(vm: &AppViewModel, document: DocumentId, row: usize, text: &str) -> Option<UiIntent> {
    let m = mode_of(vm, document)?;
    let r = m.rows.get(row)?;
    let shown = r.text.as_deref()?;
    if shown == text {
        return None;
    }
    Some(UiIntent::SetSketchDimension { document, sketch: m.sketch, constraint: r.id, text: text.to_owned() })
}

/// Meten/Sturen on the selected dimension.
pub fn sketch_role_intent(vm: &AppViewModel, document: DocumentId) -> Option<UiIntent> {
    let m = mode_of(vm, document)?;
    let (row, role) = m.rows.iter().find_map(|r| (r.selected).then_some(r.role).flatten().map(|role| (r, role)))?;
    let role = match role {
        DimensionRole::Driving => DimensionRole::Driven,
        _ => DimensionRole::Driving,
    };
    Some(UiIntent::SetSketchDimensionRole { document, sketch: m.sketch, constraint: row.id, role })
}

/// The conflict message was clicked.
pub fn sketch_problem_intent(vm: &AppViewModel, document: DocumentId) -> Option<UiIntent> {
    mode_of(vm, document)?.problem?;
    Some(UiIntent::SelectSketchProblem { document })
}

/// "Herstel laatste geldige": only for a failed sketch with history to go back in.
pub fn sketch_restore_intent(vm: &AppViewModel, document: DocumentId) -> Option<UiIntent> {
    let m = mode_of(vm, document)?;
    m.can_restore.then_some(UiIntent::RestoreLastValidSketch { document, sketch: m.sketch })
}

/// "Verwijder parameter" with the selected row of the parameter table.
pub fn sketch_parameter_remove_intent(vm: &AppViewModel, document: DocumentId, row: usize) -> Option<UiIntent> {
    let m = mode_of(vm, document)?;
    let (name, _) = m.parameters.get(row)?;
    Some(UiIntent::RemoveSketchParameter { document, sketch: m.sketch, name: name.clone() })
}

/// A parameter cell edited or added: name and expression as typed.
pub fn sketch_parameter_intent(vm: &AppViewModel, document: DocumentId, name: &str, text: &str) -> Option<UiIntent> {
    let m = mode_of(vm, document)?;
    let name = name.trim();
    if name.is_empty() || m.parameters.iter().any(|(n, t)| n == name && t == text) {
        return None;
    }
    Some(UiIntent::SetSketchParameter { document, sketch: m.sketch, name: name.to_owned(), text: text.to_owned() })
}

/// The table row of `constraint` (to start editing it after "Maat").
pub fn sketch_row_of(vm: &AppViewModel, document: DocumentId, constraint: ConstraintId) -> Option<usize> {
    mode_of(vm, document)?.rows.iter().position(|r| r.id == constraint)
}
