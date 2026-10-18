//! Executions, queue counts, Unknown reconciliation and queries (IM17, W02/W03).
#![allow(clippy::unwrap_used)]

mod common;

use burnburn_history::{
    ActionId, CompiledJobId, ExecutionStateDto, FailReasonDto, History, HistoryError, IntentKind, QueueItemId,
    ReconcileStatement, Role, UnknownReasonDto,
};
use burnburn_machine::{
    ControllerStatus, Evidence, EvidenceEntry, Execution, ExecutionId, ExecutionState, MachineId, Request,
    UnknownReason,
};
use burnburn_types::{ContentHash, MonotonicTime};
use common::*;

fn machine() -> MachineId {
    MachineId::new("ruida-bench-1")
}

struct Rig {
    fs: Fs,
    h: History,
    job: CompiledJobId,
    item: QueueItemId,
    rd_hash: ContentHash,
}

fn rig(quantity: u32) -> Rig {
    let fs = Fs::new();
    let (mut h, _) = open(&fs);
    let c = compile_session(&plate(), &profile(1));
    let job = record(&mut h, &fs, &c, &profile(1), t(0));
    let item = h.enqueue(&fs, job, quantity, t(1)).unwrap();
    let rd_hash = h.job(job).unwrap().rd_hash;
    Rig { fs, h, job, item, rd_hash }
}

/// A machine execution as the actor would report it.
fn actor_view(r: &Rig, id: ExecutionId, state: ExecutionState, evidence: Vec<Evidence>) -> Execution {
    let plan = r.h.load_plan(&r.fs, r.job).unwrap().id();
    Execution {
        id,
        machine: machine(),
        plan,
        rd_hash: r.rd_hash,
        state,
        evidence: evidence.into_iter().map(|evidence| EvidenceEntry { at: MonotonicTime::ZERO, evidence }).collect(),
        start_sent: false,
    }
}

fn new_execution(r: &mut Rig) -> ExecutionId {
    r.h.create_execution(&r.fs, r.job, machine(), Some(r.item), profile(1).revision(), t(2)).unwrap()
}

#[test]
fn restart_marks_running_execution_unknown_without_resending_start() {
    let mut r = rig(1);
    let id = new_execution(&mut r);
    r.h.record_intent(&r.fs, id, ActionId::from_raw(1), IntentKind::Start, t(3)).unwrap();
    let ev = vec![
        Evidence::Sent { request: Request::Start },
        Evidence::StartAcked,
        Evidence::Status(ControllerStatus::Running),
    ];
    r.h.observe(&r.fs, &actor_view(&r, id, ExecutionState::Running, ev), t(4)).unwrap();
    assert_eq!(r.h.execution(id).unwrap().state, ExecutionStateDto::Running);

    let (h, report) = open(&r.fs);
    assert_eq!(report.interrupted, vec![id]);
    let v = h.execution(id).unwrap();
    assert_eq!(v.state, ExecutionStateDto::Unknown { why: UnknownReasonDto::InterruptedByRestart });
    assert!(v.interrupted_by_restart);
    assert_eq!(
        v.intents.iter().filter(|i| i.intent == IntentKind::Start).count(),
        1,
        "the start intent is history, not a todo"
    );
    assert_eq!(h.next_execution_id(), ExecutionId::from_raw(id.raw() + 1), "ids are never reused after a restart");
    assert_eq!(h.queue_counts(r.item).unwrap().completed, 0, "running before the crash is not a produced piece");
}

#[test]
fn unknown_execution_is_not_counted_until_operator_confirms() {
    let mut r = rig(3);
    // 1: proven completion. 2: start reply lost → Unknown. 3: upload only.
    let a = new_execution(&mut r);
    let done = vec![
        Evidence::StorageConfirmed { rd_hash: r.rd_hash },
        Evidence::StartAcked,
        Evidence::Status(ControllerStatus::Finished),
    ];
    r.h.observe(&r.fs, &actor_view(&r, a, ExecutionState::Completed, done), t(3)).unwrap();
    let b = new_execution(&mut r);
    r.h.observe(&r.fs, &actor_view(&r, b, ExecutionState::Unknown(UnknownReason::StartReplyLost), vec![]), t(4))
        .unwrap();
    let c = new_execution(&mut r);
    r.h.observe(&r.fs, &actor_view(&r, c, ExecutionState::UploadAcked, vec![]), t(5)).unwrap();

    let counts = r.h.queue_counts(r.item).unwrap();
    assert_eq!(
        (counts.planned, counts.completed, counts.unknown, counts.in_progress, counts.remaining),
        (3, 1, 1, 1, 2)
    );

    // Only an explicit operator statement settles Unknown; it is a logged record.
    let partial = ActionId::from_raw(10);
    assert_eq!(
        r.h.reconcile_by_operator(&r.fs, a, Role::Operator, ReconcileStatement::RanToCompletion, partial, t(6)),
        Err(HistoryError::NotUnknown(ExecutionStateDto::Completed))
    );
    assert!(matches!(
        r.h.reconcile_by_operator(&r.fs, c, Role::Operator, ReconcileStatement::RanToCompletion, partial, t(6)),
        Err(HistoryError::NotUnknown(ExecutionStateDto::UploadAcked))
    ));
    r.h.reconcile_by_operator(
        &r.fs,
        b,
        Role::Operator,
        ReconcileStatement::RanToCompletion,
        ActionId::from_raw(11),
        t(7),
    )
    .unwrap();
    let counts = r.h.queue_counts(r.item).unwrap();
    assert_eq!((counts.completed, counts.unknown, counts.remaining), (2, 0, 1));

    // Survives a restart, including who settled it.
    let (h, _) = open(&r.fs);
    let v = h.execution(b).unwrap();
    assert_eq!(v.state, ExecutionStateDto::Completed);
    assert!(matches!(
        v.reconciled,
        Some(burnburn_history::ReconcileBasis::OperatorConfirmed {
            role: Role::Operator,
            statement: ReconcileStatement::RanToCompletion
        })
    ));
    assert_eq!(h.queue_counts(r.item).unwrap().completed, 2);
}

#[test]
fn partial_or_not_run_never_counts_as_produced() {
    let mut r = rig(2);
    for (raw, statement) in [(20u128, ReconcileStatement::Partial), (21, ReconcileStatement::DidNotRun)] {
        let e = new_execution(&mut r);
        r.h.observe(&r.fs, &actor_view(&r, e, ExecutionState::Unknown(UnknownReason::StartSendFailed), vec![]), t(3))
            .unwrap();
        r.h.reconcile_by_operator(&r.fs, e, Role::Operator, statement, ActionId::from_raw(raw), t(4)).unwrap();
    }
    let counts = r.h.queue_counts(r.item).unwrap();
    assert_eq!((counts.completed, counts.failed, counts.remaining), (0, 2, 2));
    let states: Vec<_> = r.h.executions().map(|e| e.state.clone()).collect();
    assert_eq!(
        states,
        vec![
            ExecutionStateDto::Failed { why: FailReasonDto::OperatorPartial },
            ExecutionStateDto::Failed { why: FailReasonDto::OperatorDidNotRun },
        ]
    );
}

#[test]
fn observed_evidence_is_journaled_once_in_order() {
    let mut r = rig(1);
    let e = new_execution(&mut r);
    let mut ev = vec![
        Evidence::Sent { request: Request::UploadChunk { index: 0, bytes: vec![1, 2, 3] } },
        Evidence::ChunkAcked { index: 0 },
    ];
    r.h.observe(&r.fs, &actor_view(&r, e, ExecutionState::UploadAcked, ev.clone()), t(3)).unwrap();
    r.h.observe(&r.fs, &actor_view(&r, e, ExecutionState::UploadAcked, ev.clone()), t(4)).unwrap();
    ev.push(Evidence::StorageConfirmed { rd_hash: r.rd_hash });
    r.h.observe(&r.fs, &actor_view(&r, e, ExecutionState::Stored, ev), t(5)).unwrap();
    let (h, _) = open(&r.fs);
    let v = h.execution(e).unwrap();
    let kinds: Vec<_> = v.evidence.iter().map(|(_, d)| d.clone()).collect();
    assert_eq!(
        kinds,
        vec![
            burnburn_history::EvidenceDto::Sent {
                request: burnburn_history::RequestDto::UploadChunk { index: 0, len: 3 }
            },
            burnburn_history::EvidenceDto::ChunkAcked { index: 0 },
            burnburn_history::EvidenceDto::StorageConfirmed { rd_hash: r.rd_hash },
        ]
    );
    // Stored is not terminal: after the restart it is Unknown, not "stored and fine".
    assert!(v.state.is_unknown());
}

#[test]
fn repeated_action_id_is_not_a_fresh_intent() {
    let mut r = rig(1);
    let e = new_execution(&mut r);
    let first = r.h.record_intent(&r.fs, e, ActionId::from_raw(77), IntentKind::Upload, t(3)).unwrap();
    let again = r.h.record_intent(&r.fs, e, ActionId::from_raw(77), IntentKind::Upload, t(4)).unwrap();
    assert!(first.fresh && !again.fresh);
    assert_eq!(first.seq, again.seq);
    let (h, _) = open(&r.fs);
    assert_eq!(h.execution(e).unwrap().intents.len(), 1);
    assert_eq!(h.action_outcome(ActionId::from_raw(77)).unwrap().seq, first.seq);
}

#[test]
fn history_answers_per_project_and_per_machine() {
    let fs = Fs::new();
    let (mut h, _) = open(&fs);
    let a = compile_session(&plate(), &profile(1));
    let mut other_doc = plate();
    rectangle(&mut other_doc, 300.0, 300.0, 10.0, 10.0);
    let b = compile_session(&other_doc, &profile(1));
    let ja = record(&mut h, &fs, &a, &profile(1), t(0));
    let jb = record(&mut h, &fs, &b, &profile(1), t(1));
    let other_machine = MachineId::new("ruida-bench-2");
    let e1 = h.create_execution(&fs, ja, machine(), None, profile(1).revision(), t(2)).unwrap();
    let e2 = h.create_execution(&fs, jb, other_machine.clone(), None, profile(1).revision(), t(3)).unwrap();
    let e3 = h.create_execution(&fs, ja, other_machine.clone(), None, profile(1).revision(), t(4)).unwrap();
    let (h, _) = open(&fs);
    let pa = burnburn_io::project_hash(&a.project).unwrap();
    assert_eq!(h.jobs_for_project(pa).map(|j| j.id).collect::<Vec<_>>(), vec![ja]);
    assert_eq!(h.executions_for_project(pa).map(|e| e.id).collect::<Vec<_>>(), vec![e1, e3]);
    assert_eq!(h.executions_for_machine(&other_machine).map(|e| e.id).collect::<Vec<_>>(), vec![e2, e3]);
}

#[test]
fn re_running_a_job_after_a_profile_change_is_refused() {
    let mut r = rig(1);
    let changed = profile(2); // same content, other display counter: still the same setup
    assert!(r.h.create_execution(&r.fs, r.job, machine(), None, changed.revision(), t(2)).is_ok());
    let other = burnburn_job::MachineProfile::new(
        "bench-1500x900",
        3,
        burnburn_job::MachineFrame::new(1500.0, 900.0, burnburn_job::OriginCorner::BottomLeft).unwrap(),
        900.0,
        400.0,
        burnburn_job::PowerRatio::new(0.0).unwrap(),
        burnburn_job::PowerRatio::new(1.0).unwrap(),
    )
    .unwrap();
    assert!(matches!(
        r.h.create_execution(&r.fs, r.job, machine(), None, other.revision(), t(3)),
        Err(HistoryError::ProfileChanged { .. })
    ));
}

/// IM31: orders and which execution was made for which order survive a restart; a link needs
/// a known order and execution. Only linked, completed executions count for an order.
#[test]
fn orders_and_their_executions_survive_a_restart() {
    let mut r = rig(1);
    let order = burnburn_history::OrderRecord {
        order: 1,
        name: "Klant A".into(),
        part: "plaat".into(),
        quantity: 3,
        remnant: Some("rest-12".into()),
        chosen_plan: None,
    };
    r.h.record_order(&r.fs, order.clone(), t(2)).unwrap();
    let id = new_execution(&mut r);
    assert_eq!(r.h.link_order(&r.fs, 9, id, t(3)), Err(HistoryError::UnknownOrder(9)));
    assert_eq!(
        r.h.link_order(&r.fs, 1, ExecutionId::from_raw(99), t(3)),
        Err(HistoryError::UnknownExecution(ExecutionId::from_raw(99)))
    );
    r.h.link_order(&r.fs, 1, id, t(3)).unwrap();
    let chosen = burnburn_history::OrderRecord { quantity: 4, chosen_plan: Some(r.rd_hash), ..order };
    r.h.record_order(&r.fs, chosen.clone(), t(4)).unwrap();

    let (h, _) = open(&r.fs);
    assert_eq!(h.orders().cloned().collect::<Vec<_>>(), vec![chosen], "latest version of each order");
    assert_eq!(h.order_of(id), Some(1));
}
