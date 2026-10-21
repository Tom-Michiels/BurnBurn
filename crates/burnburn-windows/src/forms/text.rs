//! The text panels.

use super::{ControlId, ControlValue, Form, FormContext, FormOutcome};

pub fn forms(_ctx: &FormContext<'_>) -> Vec<Form> {
    Vec::new()
}

pub fn react(_id: &ControlId, _value: &ControlValue, _ctx: &FormContext<'_>) -> FormOutcome {
    FormOutcome::Nothing
}
