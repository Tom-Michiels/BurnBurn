//! G8-cam-werk, W01/W02: the names of the parts of a released job are read back from its frozen
//! project, also after a restart, so an order finds the job by the name of a part.
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
use burnburn_presentation::{HistoryStore, JobInputs, PreparedJob, Role};
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
    job_named(length_mm, None)
}

fn job_named(length_mm: f64, part_name: Option<&str>) -> Arc<PreparedJob> {
    let plan = Arc::new(plan(length_mm));
    let mut session = DocumentSession::new(DocumentId::from_raw(1));
    for (i, name) in ["Beugel", "Plaatje"].iter().enumerate() {
        let rect = burnburn_geometry::Rect::new(
            burnburn_geometry::Point::new(10.0 * i as f64, 0.0),
            burnburn_types::Length::from_mm(5.0).unwrap(),
            burnburn_types::Length::from_mm(5.0).unwrap(),
        )
        .unwrap();
        let cmd = burnburn_document::DocumentCommand {
            document: session.id(),
            expected: session.revision(),
            request: burnburn_types::RequestId::from_raw(1),
            payload: burnburn_document::CommandPayload::CreateRectangle { rect, name: (*name).to_owned() },
        };
        session.apply(cmd).unwrap();
    }
    let snapshot = (*session.snapshot()).clone();
    let mut project = Project::new(ProjectUuid::from_bytes([1; 16]), snapshot);
    project.part_name = part_name.map(str::to_owned);
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
fn the_names_of_the_parts_of_a_released_job_are_read_from_its_frozen_project_also_after_a_restart() {
    let fs = Arc::new(MemoryFileSystem::new());
    let first = start(&fs);
    let j = job(10.0);
    assert!(first.job_parts(j.plan).is_empty(), "nothing is known of a job that is not in the history");
    first.record_approval(&j, Role::Designer).unwrap();
    let mut names = first.job_parts(j.plan);
    names.sort();
    assert_eq!(names, vec!["Beugel".to_owned(), "Plaatje".to_owned()]);
    drop(first);

    let second = start(&fs);
    let back = second.approved_jobs().unwrap().remove(0);
    assert!(back.inputs.is_none(), "the rebuilt job has no project in memory");
    let mut names = second.job_parts(back.plan);
    names.sort();
    assert_eq!(names, vec!["Beugel".to_owned(), "Plaatje".to_owned()], "the frozen project still says them");
}

#[test]
fn a_job_the_history_does_not_hold_has_no_parts() {
    let fs = Arc::new(MemoryFileSystem::new());
    let store = start(&fs);
    store.record_approval(&job(10.0), Role::Designer).unwrap();
    assert!(store.job_parts(job(20.0).plan).is_empty());
}

#[test]
fn an_explicit_part_name_replaces_the_object_names_in_the_frozen_project_also_after_a_restart() {
    let fs = Arc::new(MemoryFileSystem::new());
    let first = start(&fs);
    let j = job_named(10.0, Some("Houder links"));
    first.record_approval(&j, Role::Designer).unwrap();
    assert_eq!(first.job_parts(j.plan), vec!["Houder links".to_owned()]);
    drop(first);
    let second = start(&fs);
    let back = second.approved_jobs().unwrap().remove(0);
    assert_eq!(second.job_parts(back.plan), vec!["Houder links".to_owned()], "the name came back with the frozen project");
}
