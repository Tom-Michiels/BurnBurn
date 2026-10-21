//! "Apply update" gate (X07): a pure decision over the machine-execution states.
//!
//! An update restarts the application. That must never happen while a transfer is in flight or
//! while the machine is being operated: the restart would turn a running or half-sent job into
//! `Unknown(InterruptedByRestart)` (ADR 0008 §5). There is no network update check in v1 (B-9);
//! this gate only decides when a *manually obtained* update may be applied.

use burnburn_machine::ExecutionState;

/// Why an update has to wait. Ordered from least to most urgent; the gate reports the most urgent one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DeferReason {
    /// Jog, frame, focus or another live machine action is in progress.
    LiveControl,
    /// A job is stored on the controller and waits for the operator's start.
    JobStored,
    /// Identity check or upload is in flight.
    TransferInProgress,
    /// An outcome is uncertain; the operator has to resolve it first.
    OutcomeUnresolved,
    /// A start was requested and its effect is not yet known.
    StartPending,
    /// The controller reports a running job.
    JobRunning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateDecision {
    Apply,
    Defer(DeferReason),
}

/// Decides whether an update may be applied now.
pub fn decide_update(executions: &[ExecutionState], live_control_active: bool) -> UpdateDecision {
    let live = live_control_active.then_some(DeferReason::LiveControl);
    let most_urgent = executions.iter().filter_map(defer_reason).chain(live).max();
    most_urgent.map_or(UpdateDecision::Apply, UpdateDecision::Defer)
}

/// No wildcard arm: a new execution state must make an explicit decision here.
fn defer_reason(state: &ExecutionState) -> Option<DeferReason> {
    match state {
        ExecutionState::VerifyingIdentity
        | ExecutionState::Uploading { .. }
        | ExecutionState::UploadAcked
        | ExecutionState::Reverifying => Some(DeferReason::TransferInProgress),
        ExecutionState::Stored => Some(DeferReason::JobStored),
        ExecutionState::StartRequested => Some(DeferReason::StartPending),
        ExecutionState::Running => Some(DeferReason::JobRunning),
        ExecutionState::Unknown(_) => Some(DeferReason::OutcomeUnresolved),
        ExecutionState::Completed | ExecutionState::Failed(_) | ExecutionState::Cancelled => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use burnburn_machine::{FailReason, UnknownReason};

    fn defer(executions: &[ExecutionState], live: bool) -> Option<DeferReason> {
        match decide_update(executions, live) {
            UpdateDecision::Apply => None,
            UpdateDecision::Defer(r) => Some(r),
        }
    }

    #[test]
    fn an_idle_machine_allows_the_update() {
        assert_eq!(decide_update(&[], false), UpdateDecision::Apply);
    }

    #[test]
    fn finished_executions_do_not_block_the_update() {
        let done = [
            ExecutionState::Completed,
            ExecutionState::Cancelled,
            ExecutionState::Failed(FailReason::IdentityMismatch),
        ];
        assert_eq!(decide_update(&done, false), UpdateDecision::Apply);
    }

    #[test]
    fn every_unfinished_state_defers_with_its_own_reason() {
        let cases = [
            (ExecutionState::VerifyingIdentity, DeferReason::TransferInProgress),
            (ExecutionState::Uploading { acked: 1, total: 4 }, DeferReason::TransferInProgress),
            (ExecutionState::UploadAcked, DeferReason::TransferInProgress),
            (ExecutionState::Reverifying, DeferReason::TransferInProgress),
            (ExecutionState::Stored, DeferReason::JobStored),
            (ExecutionState::StartRequested, DeferReason::StartPending),
            (ExecutionState::Running, DeferReason::JobRunning),
            (ExecutionState::Unknown(UnknownReason::StartReplyLost), DeferReason::OutcomeUnresolved),
            (ExecutionState::Unknown(UnknownReason::InterruptedByRestart), DeferReason::OutcomeUnresolved),
        ];
        for (state, reason) in cases {
            assert_eq!(defer(std::slice::from_ref(&state), false), Some(reason), "{state:?}");
        }
    }

    #[test]
    fn live_machine_control_defers_even_without_an_execution() {
        assert_eq!(defer(&[], true), Some(DeferReason::LiveControl));
    }

    #[test]
    fn the_most_urgent_reason_wins_regardless_of_order() {
        let mixed =
            [ExecutionState::Completed, ExecutionState::Stored, ExecutionState::Running, ExecutionState::UploadAcked];
        assert_eq!(defer(&mixed, true), Some(DeferReason::JobRunning));
        let reversed: Vec<_> = mixed.iter().rev().cloned().collect();
        assert_eq!(defer(&reversed, false), Some(DeferReason::JobRunning));
    }
}
