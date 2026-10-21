//! The CAM dialogs.

use burnburn_presentation::{CamDialog, DialogViewModel};

use super::{ControlId, ControlValue, Form, FormContext, FormOutcome};

pub fn form(_ctx: &FormContext<'_>, _dialog: CamDialog, _model: &DialogViewModel) -> Option<Form> {
    None
}

pub fn react(
    _dialog: CamDialog,
    _model: Option<&DialogViewModel>,
    _id: &ControlId,
    _value: &ControlValue,
    _ctx: &FormContext<'_>,
) -> FormOutcome {
    FormOutcome::Nothing
}
