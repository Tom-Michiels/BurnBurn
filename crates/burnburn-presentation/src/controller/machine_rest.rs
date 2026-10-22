//! Machine-rest (G5): what the machine is busy with, for hosts that guard quitting (update gate),
//! and the undo-history memory notice.

use burnburn_document::{CommandWarning, HistoryBudget};
use burnburn_machine::ExecutionState;
use burnburn_types::DocumentId;

use super::AppController;

/// What the machine side is doing, as evidence says: every execution state the machine service
/// knows (earlier ones included; terminal ones are harmless) and whether a live action is held.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MachineActivity {
    pub executions: Vec<ExecutionState>,
    /// A jog key is held (or its release is unconfirmed), or a frame run is in progress.
    pub live_control: bool,
}

impl AppController {
    /// The machine's current activity; empty without an attached machine.
    pub fn machine_activity(&self) -> MachineActivity {
        let Some(port) = self.machine_port.as_ref() else { return MachineActivity::default() };
        let status = port.status();
        let executions: Vec<ExecutionState> = status.executions().map(|r| r.execution.state.clone()).collect();
        let frame_running =
            status.current_is_frame && status.current.as_ref().is_some_and(|r| !r.execution.state.is_terminal());
        MachineActivity { executions, live_control: status.live.jog_held.is_some() || frame_running }
    }

    /// Sets the memory budget of one document's undo history (tests and low-memory hosts).
    pub fn set_history_budget(&mut self, document: DocumentId, budget: HistoryBudget) {
        if let Some(s) = self.sessions.get_mut(&document) {
            s.doc.set_history_budget(budget);
        }
    }
}

/// The number of oldest undo steps a command made unreachable, if any.
pub(super) fn trimmed_steps(warnings: &[CommandWarning]) -> Option<usize> {
    let total: usize = warnings
        .iter()
        .map(|w| match w {
            CommandWarning::HistoryTrimmed { dropped_steps } => *dropped_steps,
            _ => 0,
        })
        .sum();
    (total > 0).then_some(total)
}
