//! CLI test rig: in-memory storage, fake clock, scripted fake controller. No socket, no laser.
#![allow(clippy::unwrap_used, clippy::expect_used, dead_code)]

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use burnburn_cli::{CliEnv, CliError, CliOutcome, MachineConnector, run};
use burnburn_document::process::{
    Kerf, OperationDraft, OperationMode, Passes, PowerPercent, PowerRange, ProcessCommand, ProcessSettings, Speed,
};
use burnburn_document::{CommandPayload, DocumentCommand, DocumentSession};
use burnburn_geometry::{Point, Rect};
use burnburn_history::{ActionId, JournalFs, WallTime};
use burnburn_io::fs::{FileSystem, FsError, FsErrorKind};
use burnburn_io::memory::MemoryFileSystem;
use burnburn_io::{Project, ProjectUuid, encode_container};
use burnburn_machine::{ControllerPort, Envelope, Incoming, MachineId, PortError, Reply, Request, RequestTag};
use burnburn_runtime::WallClock;
use burnburn_test_support::{FakeClock, ScriptedPeer};
use burnburn_types::{CancellationToken, DocumentId, Length, RequestId, StableId};

#[derive(Default)]
pub struct Fs {
    pub mem: MemoryFileSystem,
}

impl FileSystem for Fs {
    fn read(&self, p: &Path, l: u64) -> Result<Vec<u8>, FsError> {
        self.mem.read(p, l)
    }
    fn exists(&self, p: &Path) -> Result<bool, FsError> {
        self.mem.exists(p)
    }
    fn create_dir_all(&self, d: &Path) -> Result<(), FsError> {
        self.mem.create_dir_all(d)
    }
    fn create_temp_in(&self, d: &Path, s: &str) -> Result<PathBuf, FsError> {
        self.mem.create_temp_in(d, s)
    }
    fn write_all(&self, p: &Path, b: &[u8]) -> Result<(), FsError> {
        self.mem.write_all(p, b)
    }
    fn sync_file(&self, p: &Path) -> Result<(), FsError> {
        self.mem.sync_file(p)
    }
    fn replace(&self, f: &Path, t: &Path) -> Result<(), FsError> {
        self.mem.replace(f, t)
    }
    fn sync_dir(&self, d: &Path) -> Result<(), FsError> {
        self.mem.sync_dir(d)
    }
    fn remove_file(&self, p: &Path) -> Result<(), FsError> {
        self.mem.remove_file(p)
    }
    fn append(&self, path: &Path, bytes: &[u8]) -> Result<u64, FsError> {
        let mut old = match self.mem.read(path, u64::MAX) {
            Ok(b) => b,
            Err(e) if e.kind == FsErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(e),
        };
        old.extend_from_slice(bytes);
        self.mem.write_all(path, &old)?;
        Ok(old.len() as u64)
    }
    fn list_dir(&self, dir: &Path) -> Result<Vec<PathBuf>, FsError> {
        self.mem.list_dir(dir)
    }
}

pub type Peer = ScriptedPeer<Request, Reply>;

struct FakePort {
    peer: Peer,
    machine: MachineId,
    last: Option<RequestTag>,
}

impl ControllerPort for FakePort {
    fn send(&mut self, e: Envelope) -> Result<(), PortError> {
        self.last = Some(e.tag);
        self.peer.send(e.request).map_err(|e| PortError::Other(e.to_string()))
    }
    fn try_recv(&mut self) -> Result<Option<Incoming>, PortError> {
        let r = self.peer.recv().map_err(|e| PortError::Other(e.to_string()))?;
        Ok(r.and_then(|reply| self.last.map(|tag| Incoming { from: self.machine.clone(), tag, reply })))
    }
    fn reconnect(&mut self) -> Result<(), PortError> {
        self.peer.reconnect();
        Ok(())
    }
}

/// Hands out ports to one scripted fake controller and counts connection attempts.
pub struct FakeConnector {
    pub peer: Peer,
    pub connects: AtomicUsize,
    pub peers: Mutex<Vec<SocketAddr>>,
}

impl MachineConnector for FakeConnector {
    fn connect(&self, peer: SocketAddr, machine: &MachineId) -> Result<Box<dyn ControllerPort>, PortError> {
        self.connects.fetch_add(1, Ordering::SeqCst);
        self.peers.lock().unwrap().push(peer);
        Ok(Box::new(FakePort { peer: self.peer.clone(), machine: machine.clone(), last: None }))
    }
}

struct FixedWall;
impl WallClock for FixedWall {
    fn now(&self) -> WallTime {
        WallTime::from_unix_millis(1_792_000_000_000)
    }
}

pub struct Rig {
    pub fs: Arc<Fs>,
    pub clock: FakeClock,
    pub connector: Arc<FakeConnector>,
    pub env: CliEnv,
}

pub fn rig() -> Rig {
    let fs = Arc::new(Fs::default());
    let clock = FakeClock::new();
    let connector = Arc::new(FakeConnector {
        peer: Peer::new(clock.clone()),
        connects: AtomicUsize::new(0),
        peers: Mutex::default(),
    });
    let pause_clock = clock.clone();
    let counter = Arc::new(AtomicU64::new(1));
    let env = CliEnv {
        fs: Arc::clone(&fs) as Arc<dyn JournalFs>,
        clock: Arc::new(clock.clone()),
        wall: Arc::new(FixedWall),
        cancel: CancellationToken::never(),
        connector: Arc::clone(&connector) as Arc<dyn MachineConnector>,
        pause: Arc::new(move || {
            pause_clock.advance(Duration::from_millis(100));
        }),
        new_action: Arc::new(move || ActionId::from_raw(u128::from(counter.fetch_add(1, Ordering::SeqCst)))),
        max_polls: 100,
    };
    Rig { fs, clock, connector, env }
}

impl Rig {
    pub fn run(&self, line: &str) -> Result<CliOutcome, CliError> {
        let argv: Vec<String> = line.split_whitespace().map(str::to_owned).collect();
        run(&argv, &self.env)
    }

    pub fn put(&self, path: &str, bytes: Vec<u8>) {
        self.fs.mem.insert(Path::new(path), bytes);
    }

    pub fn file(&self, path: &str) -> Option<Vec<u8>> {
        self.fs.mem.read(Path::new(path), u64::MAX).ok()
    }
}

pub fn mm(v: f64) -> Length {
    Length::from_mm(v).unwrap()
}

fn apply(s: &mut DocumentSession, payload: CommandPayload) -> burnburn_document::CommandOutcome {
    let cmd = DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload };
    s.apply(cmd).unwrap()
}

pub fn cut_op(speed: f64) -> OperationDraft {
    let p = PowerPercent::new(60.0).unwrap();
    OperationDraft {
        name: "Snijden".into(),
        mode: OperationMode::Line { kerf: Kerf::NONE },
        settings: ProcessSettings {
            speed: Speed::mm_per_s(speed).unwrap(),
            power: PowerRange::new(p, p).unwrap(),
            passes: Passes::new(1).unwrap(),
            air_assist: true,
        },
        output_enabled: true,
    }
}

/// A project file with rectangles (x, y, w, h) cut by one operation.
pub fn project_bytes(rects: &[(f64, f64, f64, f64)]) -> Vec<u8> {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let mut ids = Vec::new();
    for &(x, y, w, h) in rects {
        let rect = Rect::new(Point::new(x, y), mm(w), mm(h)).unwrap();
        ids.push(apply(&mut s, CommandPayload::CreateRectangle { rect, name: String::new() }).changes.created[0]);
    }
    apply(&mut s, CommandPayload::Process(ProcessCommand::AddOperation { draft: cut_op(20.0), targets: ids }));
    let project = Project::new(ProjectUuid::from_bytes([3; 16]), (*s.snapshot()).clone());
    encode_container(&project).unwrap()
}

/// A rectangle and a circle (arcs make the flattening tolerance visible in the plan id).
pub fn project_with_circle_bytes() -> Vec<u8> {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let rect = Rect::new(Point::new(10.0, 10.0), mm(120.0), mm(80.0)).unwrap();
    let r = apply(&mut s, CommandPayload::CreateRectangle { rect, name: String::new() }).changes.created[0];
    let shape = burnburn_geometry::Shape::Circle { center: Point::new(200.0, 60.0), radius: mm(25.0) };
    let edit = burnburn_document::GeometryCommand::CreateShape { shape, name: String::new() };
    let c = apply(&mut s, CommandPayload::Edit(edit)).changes.created[0];
    apply(&mut s, CommandPayload::Process(ProcessCommand::AddOperation { draft: cut_op(20.0), targets: vec![r, c] }));
    let project = Project::new(ProjectUuid::from_bytes([4; 16]), (*s.snapshot()).clone());
    encode_container(&project).unwrap()
}

pub const PROFILE_JSON: &str = r#"{
  "id": "bench-1500x900", "revision": 1, "bed_mm": [1500, 900], "origin": "bottom-left",
  "max_speed_mm_s": 1000, "travel_speed_mm_s": 400, "power_range": [0, 1],
  "ruida": { "magic": 136, "overscan": "reject" },
  "identity": { "machine_id": "ruida-bench-1", "model": "RDC6445G", "firmware": "fake-1" }
}"#;

pub fn with_project(r: &Rig) {
    r.put("/work/plaat.burnburn", project_bytes(&[(10.0, 10.0, 120.0, 80.0)]));
    r.put("/work/bench.json", PROFILE_JSON.as_bytes().to_vec());
}
