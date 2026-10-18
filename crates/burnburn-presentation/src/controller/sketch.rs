//! Sketches: the constraint list, dimensions and parameters (contract §11).

use super::{AppController, AppUpdate, apply_edit};
use burnburn_document::CommandPayload;
use burnburn_document::sketch::SketchCommand;
use burnburn_document::{DerivedOwner, EvalDiagnosis, EvalStatus};
use burnburn_types::{DocumentId, RequestId};

use crate::view_model::{SketchStatus, SketchViewModel};

pub(super) fn sketch_rows(snapshot: &burnburn_document::DocumentSnapshot) -> Vec<SketchViewModel> {
    snapshot
        .sketch_layer()
        .sketches()
        .map(|(id, sketch)| {
            let record = snapshot.derived_record(DerivedOwner::Sketch(id)).map(|r| &r.status);
            let status = match record {
                Some(EvalStatus::Valid) => SketchStatus::Valid,
                Some(EvalStatus::Failed { .. }) => SketchStatus::Failed,
                _ => SketchStatus::Pending,
            };
            let problem_constraints = match record {
                Some(EvalStatus::Failed {
                    diagnosis:
                        EvalDiagnosis::Conflict { constraints }
                        | EvalDiagnosis::InvalidDimensions { constraints }
                        | EvalDiagnosis::NotConverged { unmet: constraints },
                }) => constraints.clone(),
                _ => Vec::new(),
            };
            let parameters = sketch.parameters().iter().map(|(n, t)| (n.to_owned(), t.to_owned())).collect();
            SketchViewModel {
                id,
                name: sketch.name().to_owned(),
                status,
                parameters,
                constraints: sketch.constraints().len(),
                dof: sketch.solution().filter(|_| status == SketchStatus::Valid).map(|sol| sol.dof),
                problem_constraints,
                last_constraint: sketch.constraints().map(|(c, _)| c).max(),
            }
        })
        .collect()
}

impl AppController {
    /// One sketch edit as one command (one undo entry, derived geometry in the same transaction).
    pub(super) fn sketch_edit(&mut self, document: DocumentId, edit: SketchCommand) -> AppUpdate {
        let request = self.next_id::<RequestId>();
        self.with_session(document, |s| apply_edit(s, document, CommandPayload::Sketch(edit), request))
    }
}
