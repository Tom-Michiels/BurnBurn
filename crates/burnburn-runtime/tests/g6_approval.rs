//! G6-machine, W01: the designer's approval goes through `JournalHistoryStore` into the journal,
//! comes back after a restart as the same frozen job, and a job rebuilt from the history can be
//! uploaded again without its original project (it is frozen already).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::sync::Arc;

use burnburn_document::DocumentSession;
use burnburn_history::{History, JournalFs, JournalLimits, WallTime};
use burnburn_io::memory::MemoryFileSystem;
use burnburn_io::{Project, ProjectUuid};
use burnburn_job::{
    JobPlan, JobPlanBuilder, MachineFrame, MachinePoint, OperationSettings, OriginCorner, PlanEvent, PlanMode,
    PlanSource, PowerRatio, ProfileRevision,
};
use burnburn_machine::MachineId;
use burnburn_presentation::{
    ActionId, ActionSubject, HistoryStore, JobInputs, MachineAction, PreparedJob, Role, UploadRecord,
};
use burnburn_runtime::{JournalHistoryStore, WallClock};
use burnburn_types::{ContentHash, DocumentId, HashDomain, OperationId, RevisionId, StableId};

const RD: [u8; 5] = [0xd8, 0x10, 0x89, 0x41, 0x42];

struct FixedWall;
impl WallClock for FixedWall {
    fn now(&self) -> WallTime {
        WallTime::from_unix_millis(1_792_000_000_000)
    }
}

fn dir() -> PathBuf {
    PathBuf::from("/app-data/history")
}

fn setup() -> ProfileRevision {
    ProfileRevision { id: "bench".into(), revision: 1, content: ContentHash::of(HashDomain::Profile, b"bench") }
}

fn plan(length_mm: f64) -> JobPlan {
    let source = PlanSource {
        document_revision: RevisionId::INITIAL,
        source_hash: ContentHash::of(HashDomain::Plan, b"approval-test"),
        machine_profile: setup(),
        compiler_version: "test".into(),
    };
    let mut b = JobPlanBuilder::new(
        source,
        MachineFrame::new(100.0, 100.0, OriginCorner::BottomLeft).unwrap(),
        MachinePoint::new(0.0, 0.0).unwrap(),
    );
    let s = b
        .add_settings(OperationSettings {
            mode: PlanMode::Line,
            speed_mm_s: 10.0,
            min_power: PowerRatio::new(0.5).unwrap(),
            max_power: PowerRatio::new(0.5).unwrap(),
            air_assist: false,
        })
        .unwrap();
    b.push(PlanEvent::OperationBoundary { operation: OperationId::from_raw(1), settings: s, pass: 0 }).unwrap();
    b.push(PlanEvent::Cut { to: MachinePoint::new(length_mm, 0.0).unwrap(), origin: None }).unwrap();
    b.finish().unwrap()
}

fn job(length_mm: f64) -> Arc<PreparedJob> {
    let plan = Arc::new(plan(length_mm));
    let snapshot = (*DocumentSession::new(DocumentId::from_raw(1)).snapshot()).clone();
    let project = Project::new(ProjectUuid::from_bytes([1; 16]), snapshot);
    Arc::new(PreparedJob {
        plan: plan.id(),
        plan_revision: plan.source().document_revision,
        profile: setup(),
        rd_hash: burnburn_machine::rd_hash(&RD),
        rd: Arc::from(&RD[..]),
        name: "plaat".into(),
        inputs: Some(Arc::new(JobInputs {
            plan,
            project: Arc::new(project),
            project_path: Some(PathBuf::from("/work/plaat.burnburn")),
        })),
    })
}

fn start(fs: &Arc<MemoryFileSystem>) -> Arc<JournalHistoryStore> {
    let (history, _) = History::open(fs.as_ref(), &dir(), &JournalLimits::default()).unwrap();
    let journal: Arc<dyn JournalFs> = Arc::clone(fs) as Arc<dyn JournalFs>;
    Arc::new(JournalHistoryStore::new(history, journal, Arc::new(FixedWall)))
}

#[test]
fn an_approved_job_comes_back_after_a_restart_as_the_same_frozen_job() {
    let fs = Arc::new(MemoryFileSystem::new());
    let first = start(&fs);
    let j = job(10.0);
    assert!(first.approved_jobs().unwrap().is_empty());
    first.record_approval(&j, Role::Designer).unwrap();
    assert_eq!(first.approved_jobs().unwrap().len(), 1);
    drop(first);

    let second = start(&fs);
    let back = second.approved_jobs().unwrap();
    assert_eq!(back.len(), 1, "the approval survived the restart");
    assert_eq!((back[0].plan, back[0].rd_hash, &back[0].profile), (j.plan, j.rd_hash, &j.profile));
    assert_eq!(&*back[0].rd, &RD[..], "the exact RD bytes of the reviewed job");
    assert_eq!(back[0].name, "plaat");
}

#[test]
fn approving_is_not_an_upload_and_is_idempotent() {
    let fs = Arc::new(MemoryFileSystem::new());
    let store = start(&fs);
    let j = job(10.0);
    store.record_approval(&j, Role::Designer).unwrap();
    store.record_approval(&j, Role::Designer).unwrap();
    assert_eq!(store.approved_jobs().unwrap().len(), 1);
    store.with_history(|h| {
        assert_eq!(h.executions().count(), 0, "approval creates no execution");
        assert_eq!(h.jobs().count(), 1);
    });
}

#[test]
fn only_a_designer_approves_and_a_job_without_frozen_inputs_cannot_be_approved() {
    let fs = Arc::new(MemoryFileSystem::new());
    let store = start(&fs);
    let j = job(10.0);
    assert!(store.record_approval(&j, Role::Operator).is_err());
    assert!(store.approved_jobs().unwrap().is_empty());

    let rebuilt = PreparedJob { inputs: None, ..(*job(20.0)).clone() };
    assert!(store.record_approval(&rebuilt, Role::Designer).is_err(), "never frozen, so nothing to release");
    assert!(store.approved_jobs().unwrap().is_empty());
}

#[test]
fn a_changed_plan_is_not_among_the_approved_jobs() {
    let fs = Arc::new(MemoryFileSystem::new());
    let store = start(&fs);
    store.record_approval(&job(10.0), Role::Designer).unwrap();
    let longer = job(20.0);
    let approved = store.approved_jobs().unwrap();
    assert!(approved.iter().all(|a| a.plan != longer.plan));
}

#[test]
fn a_job_rebuilt_from_the_history_uploads_again_without_its_project() {
    let fs = Arc::new(MemoryFileSystem::new());
    let first = start(&fs);
    let j = job(10.0);
    first.record_approval(&j, Role::Designer).unwrap();
    drop(first);

    let store = start(&fs);
    let rebuilt = store.approved_jobs().unwrap().remove(0);
    assert!(rebuilt.inputs.is_none());
    let action = ActionId { kind: MachineAction::Upload, subject: ActionSubject::Job(rebuilt.plan), generation: 0 };
    let record = UploadRecord { action, machine: MachineId::new("bench-ruida-1"), job: rebuilt, order: None };
    let (execution, _) = store.record_upload(&record).expect("a frozen job needs no project to be uploaded again");
    store.with_history(|h| {
        assert_eq!(h.jobs().count(), 1, "it is the same frozen job, not a copy");
        assert_eq!(h.execution(execution).map(|e| e.job), h.jobs().next().map(|j| j.id));
    });
}

#[test]
fn an_unknown_rebuilt_job_is_still_refused() {
    let fs = Arc::new(MemoryFileSystem::new());
    let store = start(&fs);
    let stranger = PreparedJob { inputs: None, ..(*job(30.0)).clone() };
    let action = ActionId { kind: MachineAction::Upload, subject: ActionSubject::Job(stranger.plan), generation: 0 };
    let record = UploadRecord { action, machine: MachineId::new("bench-ruida-1"), job: Arc::new(stranger), order: None };
    assert!(store.record_upload(&record).is_err());
}
