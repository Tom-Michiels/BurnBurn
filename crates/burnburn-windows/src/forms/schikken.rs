//! The Schikken panel.

use super::{ControlId, ControlValue, Form, FormContext, FormOutcome, FormValues, PanelState};

pub fn form(_ctx: &FormContext<'_>, _state: &PanelState) -> Option<Form> {
    None
}

pub fn react(_id: &ControlId, _value: &ControlValue, _values: &FormValues, _ctx: &FormContext<'_>) -> FormOutcome {
    FormOutcome::Nothing
}
