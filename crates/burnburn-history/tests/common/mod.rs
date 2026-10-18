//! Shared test rig: io's `MemoryFileSystem` (C-11: the FileSystem fake) plus `append` with a
//! torn-append fault, until io gets `FileSystem::append` (C-7).
#![allow(clippy::unwrap_used, clippy::expect_used, dead_code)]

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use burnburn_cam::{CompileOptions, Placement, StartPoint, compile, preflight};
use burnburn_document::process::{
    Kerf, OperationDraft, OperationMode, Passes, PowerPercent, PowerRange, ProcessCommand, ProcessSettings, Speed,
};
use burnburn_document::{CommandPayload, DocumentCommand, DocumentSession};
use burnburn_geometry::{Point, ProductionTolerance, Rect};
use burnburn_history::{CompileRecord, History, JournalLimits, PreflightSummary, WallTime};
use burnburn_io::fs::{FileSystem, FsError, FsErrorKind, FsOp};
use burnburn_io::memory::MemoryFileSystem;
use burnburn_io::{Project, ProjectUuid};
use burnburn_job::{JobPlan, MachineFrame, MachineProfile, OriginCorner, PowerRatio};
use burnburn_ruida::{Magic, OverscanPolicy, RuidaProfile, lower};
use burnburn_types::{CancellationToken, DocumentId, GeometryId, Length, RequestId, StableId};

#[derive(Default)]
pub struct Fs {
    pub mem: MemoryFileSystem,
    /// Next append fails after writing this many bytes of the record.
    torn_append: Mutex<Option<usize>>,
}

impl Fs {
    pub fn new() -> Self {
        Fs::default()
    }

    pub fn tear_next_append(&self, keep: usize) {
        *self.torn_append.lock().unwrap() = Some(keep);
    }
}

impl FileSystem for Fs {
    fn read(&self, path: &Path, limit: u64) -> Result<Vec<u8>, FsError> {
        self.mem.read(path, limit)
    }
    fn exists(&self, path: &Path) -> Result<bool, FsError> {
        self.mem.exists(path)
    }
    fn create_dir_all(&self, dir: &Path) -> Result<(), FsError> {
        self.mem.create_dir_all(dir)
    }
    fn create_temp_in(&self, dir: &Path, stem: &str) -> Result<PathBuf, FsError> {
        self.mem.create_temp_in(dir, stem)
    }
    fn write_all(&self, path: &Path, bytes: &[u8]) -> Result<(), FsError> {
        self.mem.write_all(path, bytes)
    }
    fn sync_file(&self, path: &Path) -> Result<(), FsError> {
        self.mem.sync_file(path)
    }
    fn replace(&self, from: &Path, to: &Path) -> Result<(), FsError> {
        self.mem.replace(from, to)
    }
    fn sync_dir(&self, dir: &Path) -> Result<(), FsError> {
        self.mem.sync_dir(dir)
    }
    fn remove_file(&self, path: &Path) -> Result<(), FsError> {
        self.mem.remove_file(path)
    }
    /// Modelled on top of the memory fs: current bytes grow, durability still needs `sync_file`.
    fn append(&self, path: &Path, bytes: &[u8]) -> Result<u64, FsError> {
        let mut old = match self.mem.read(path, u64::MAX) {
            Ok(b) => b,
            Err(e) if e.kind == FsErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(e),
        };
        if let Some(keep) = self.torn_append.lock().unwrap().take() {
            old.extend_from_slice(&bytes[..keep.min(bytes.len())]);
            self.mem.write_all(path, &old)?;
            return Err(FsError::new(FsOp::Write, path, FsErrorKind::Other));
        }
        old.extend_from_slice(bytes);
        self.mem.write_all(path, &old)?;
        Ok(old.len() as u64)
    }
    fn list_dir(&self, dir: &Path) -> Result<Vec<PathBuf>, FsError> {
        self.mem.list_dir(dir)
    }
}

pub fn dir() -> PathBuf {
    PathBuf::from("/app-data/history")
}

pub fn open(fs: &Fs) -> (History, burnburn_history::RecoveryReport) {
    History::open(fs, &dir(), &JournalLimits::default()).unwrap()
}

pub fn t(ms: i64) -> WallTime {
    WallTime::from_unix_millis(1_792_000_000_000 + ms)
}

pub fn mm(v: f64) -> Length {
    Length::from_mm(v).unwrap()
}

pub fn run(s: &mut DocumentSession, payload: CommandPayload) -> burnburn_document::CommandOutcome {
    let cmd = DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload };
    s.apply(cmd).unwrap()
}

pub fn rectangle(s: &mut DocumentSession, x: f64, y: f64, w: f64, h: f64) -> GeometryId {
    let rect = Rect::new(Point::new(x, y), mm(w), mm(h)).unwrap();
    run(s, CommandPayload::CreateRectangle { rect, name: String::new() }).changes.created[0]
}

pub fn cut_op() -> OperationDraft {
    let p = PowerPercent::new(60.0).unwrap();
    OperationDraft {
        name: "Snijden".into(),
        mode: OperationMode::Line { kerf: Kerf::NONE },
        settings: ProcessSettings {
            speed: Speed::mm_per_s(20.0).unwrap(),
            power: PowerRange::new(p, p).unwrap(),
            passes: Passes::new(1).unwrap(),
            air_assist: true,
        },
        output_enabled: true,
    }
}

/// A document with one 120 × 80 mm rectangle and a cut operation.
pub fn plate() -> DocumentSession {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let g = rectangle(&mut s, 10.0, 10.0, 120.0, 80.0);
    run(&mut s, CommandPayload::Process(ProcessCommand::AddOperation { draft: cut_op(), targets: vec![g] }));
    s
}

pub fn profile(revision: u32) -> MachineProfile {
    MachineProfile::new(
        "bench-1500x900",
        revision,
        MachineFrame::new(1500.0, 900.0, OriginCorner::BottomLeft).unwrap(),
        1000.0,
        400.0,
        PowerRatio::new(0.0).unwrap(),
        PowerRatio::new(1.0).unwrap(),
    )
    .unwrap()
}

pub const RUIDA: RuidaProfile = RuidaProfile { magic: Magic::FIXTURE_0X88, overscan: OverscanPolicy::Reject };

pub fn options() -> CompileOptions {
    CompileOptions {
        tolerance: ProductionTolerance::new(mm(0.01)).unwrap(),
        placement: Placement::Absolute,
        start: StartPoint::MachineOrigin,
    }
}

pub struct Compiled {
    pub plan: JobPlan,
    pub rd: Vec<u8>,
    pub project: Project,
    pub preflight: PreflightSummary,
}

/// The production route: compile → preflight → lower → RD bytes.
pub fn compile_session(s: &DocumentSession, profile: &MachineProfile) -> Compiled {
    let snapshot = s.snapshot();
    let plan = compile(&snapshot, profile, &options(), &CancellationToken::never()).unwrap();
    let report = preflight(&snapshot, &plan, profile);
    let rd = lower(&plan, &RUIDA).unwrap().to_rd_file(RUIDA.magic).unwrap();
    let preflight = PreflightSummary {
        errors: report.errors().count() as u32,
        warnings: report.warnings().count() as u32,
        codes: Vec::new(),
    };
    let project = Project::new(ProjectUuid::from_bytes([7; 16]), (*snapshot).clone());
    Compiled { plan, rd, project, preflight }
}

pub fn record(
    h: &mut History,
    fs: &Fs,
    c: &Compiled,
    profile: &MachineProfile,
    at: WallTime,
) -> burnburn_history::CompiledJobId {
    h.record_compiled(
        fs,
        CompileRecord {
            plan: &c.plan,
            rd_file: &c.rd,
            project: &c.project,
            project_path: Some(PathBuf::from("/work/plaat.burnburn")),
            preflight: c.preflight.clone(),
            profile: profile.revision(),
        },
        at,
    )
    .unwrap()
}
