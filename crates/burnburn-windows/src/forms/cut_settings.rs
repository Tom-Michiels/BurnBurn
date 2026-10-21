//! The Cut Settings sheet.

use super::{ControlId, ControlValue, Form, FormContext, FormOutcome, FormValues};

pub fn form(_ctx: &FormContext<'_>, _live: &FormValues) -> Option<Form> {
    None
}

pub fn react(_id: &ControlId, _value: &ControlValue, _values: &FormValues, _ctx: &FormContext<'_>) -> FormOutcome {
    FormOutcome::Nothing
}
