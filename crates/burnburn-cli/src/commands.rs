//! The subcommands. Each reads/writes only through `CliEnv.fs` and contacts a machine only
//! through `CliEnv.connector`, and only for `upload`/`start` with `--allow-machine-contact`.

use std::fmt::Write as _;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use burnburn_cam::{CompileOptions, Finding, Placement, PreflightReport, Severity, StartPoint};
use burnburn_geometry::ProductionTolerance;
use burnburn_history::{
    CompileRecord, CompiledJobId, ExecutionStateDto, History, JournalLimits, PreflightSummary, encode_plan,
};
use burnburn_io::fs::FileSystem;
use burnburn_io::load_project;
use burnburn_machine::{ExecutionActor, ExecutionId, ExecutionState, Policy};
use burnburn_presentation::{TaskError, TaskInput, TaskOutput, run_task};
use burnburn_ruida::{Command, Magic, decode_rd_file};
use burnburn_runtime::MachineDriver;
use burnburn_types::{ContentHash, Length};

use crate::profile_file::{Backend, ProfileFile, parse_profile};
use crate::{Args, CliEnv, CliError, CliOutcome};

/// Same production tolerance as the GUI's compile (`burnburn-presentation`, 0.01 mm). Guarded
/// by `cli_and_gui_compile_same_revision_to_same_normalized_plan`.
pub const COMPILE_TOLERANCE_MM: f64 = 0.01;

const MAX_PROFILE_BYTES: u64 = 1 << 20;
const MAX_RD_BYTES: u64 = 64 << 20;

pub fn compile_options() -> Result<CompileOptions, CliError> {
    let mm = Length::from_mm(COMPILE_TOLERANCE_MM).map_err(|e| CliError::Compile(format!("{e:?}")))?;
    let tolerance = ProductionTolerance::new(mm).map_err(|e| CliError::Compile(format!("{e:?}")))?;
    Ok(CompileOptions { tolerance, placement: Placement::Absolute, start: StartPoint::MachineOrigin })
}

fn fs_dyn(env: &CliEnv) -> Arc<dyn FileSystem> {
    Arc::clone(&env.fs) as Arc<dyn FileSystem>
}

fn read_profile(env: &CliEnv, a: &Args) -> Result<ProfileFile, CliError> {
    parse_profile(&env.fs.read(Path::new(a.required("profile")?), MAX_PROFILE_BYTES)?)
}

fn finding_code(f: &Finding) -> String {
    let dbg = format!("{f:?}");
    dbg.split([' ', '{', '(']).next().unwrap_or("Finding").to_owned()
}

fn report_text(report: &PreflightReport) -> String {
    let mut out = String::new();
    for f in report.errors() {
        let _ = writeln!(out, "error   {f:?}");
    }
    for f in report.warnings() {
        let _ = writeln!(out, "warning {f:?}");
    }
    out
}

struct Compiled {
    plan: Arc<burnburn_job::JobPlan>,
    report: PreflightReport,
    preview_seconds: f64,
    project: burnburn_io::Project,
    profile: ProfileFile,
}

/// Load → compile (the GUI's `run_task`) → preflight. Writes nothing.
fn compile_project(env: &CliEnv, a: &Args) -> Result<Compiled, CliError> {
    let project_path = PathBuf::from(a.one_positional("project file")?);
    let profile = read_profile(env, a)?;
    env.cancel.check().map_err(|_| CliError::Cancelled)?;
    let project = load_project(env.fs.as_ref(), &project_path).map_err(|e| CliError::Load(format!("{e:?}")))?;
    let input = TaskInput::Compile {
        snapshot: Arc::new(project.document.clone()),
        profile: Arc::new(profile.profile.clone()),
        options: compile_options()?,
        assets: project.assets.clone(),
    };
    match run_task(input, &env.cancel) {
        Ok(TaskOutput::Compiled { plan, preview, report }) => {
            Ok(Compiled { plan, report, preview_seconds: preview.estimated_seconds, project, profile })
        }
        Ok(_) => Err(CliError::Compile("unexpected task output".into())),
        Err(TaskError::Cancelled) => Err(CliError::Cancelled),
        Err(e) => Err(CliError::Compile(e.to_string())),
    }
}

fn refuse_errors(report: &PreflightReport) -> Result<(), CliError> {
    let errors = report.errors().count();
    if errors > 0 {
        return Err(CliError::Preflight { errors, report: report_text(report) });
    }
    Ok(())
}

pub fn preflight(env: &CliEnv, a: &Args) -> Result<CliOutcome, CliError> {
    a.reject_unknown(&["profile"])?;
    let c = compile_project(env, a)?;
    refuse_errors(&c.report)?;
    let stdout = format!(
        "plan {}\npreflight: 0 errors, {} warnings\n{}",
        c.plan.id(),
        c.report.warnings().count(),
        report_text(&c.report)
    );
    Ok(CliOutcome { stdout, plan: Some(c.plan.id()), ..CliOutcome::default() })
}

pub fn compile(env: &CliEnv, a: &Args) -> Result<CliOutcome, CliError> {
    for flag in ["and-start", "start", "upload"] {
        if a.flag(flag).is_some() {
            return Err(CliError::Usage(format!("compile never contacts a machine (--{flag})")));
        }
    }
    a.reject_unknown(&["profile", "out", "plan-json", "history"])?;
    let c = compile_project(env, a)?;
    refuse_errors(&c.report)?;
    let out = PathBuf::from(a.required("out")?);
    if let Backend::Grbl(grbl) = &c.profile.backend {
        return compile_gcode(env, a, &c, grbl, &out);
    }
    env.cancel.check().map_err(|_| CliError::Cancelled)?;
    let export =
        TaskInput::ExportRd { plan: Arc::clone(&c.plan), ruida: c.profile.ruida, path: out.clone(), fs: fs_dyn(env) };
    let (rd_hash, bytes) = match run_task(export, &env.cancel) {
        Ok(TaskOutput::ExportedRd { plan, rd_hash, bytes, .. }) if plan == c.plan.id() => (rd_hash, bytes),
        Ok(_) => return Err(CliError::Rd("export named another plan".into())),
        Err(TaskError::Cancelled) => return Err(CliError::Cancelled),
        Err(e) => return Err(CliError::Rd(e.to_string())),
    };
    if let Some(plan_json) = a.flag("plan-json") {
        let blob = encode_plan(&c.plan)?;
        write_atomically(env.fs.as_ref(), Path::new(plan_json), &blob)?;
    }
    let mut stdout = format!(
        "plan {}\nrd {} ({} bytes) -> {}\npreflight: 0 errors, {} warnings\nestimate {:.1} s (constant-velocity estimate, not measured)\n",
        c.plan.id(),
        rd_hash,
        bytes,
        out.display(),
        c.report.warnings().count(),
        c.preview_seconds
    );
    let mut job = None;
    if let Some(dir) = a.flag("history") {
        // Freeze exactly the bytes that were written.
        let rd = env.fs.read(&out, MAX_RD_BYTES)?;
        let (mut h, _) = History::open(env.fs.as_ref(), Path::new(dir), &JournalLimits::default())?;
        let preflight = PreflightSummary {
            errors: 0,
            warnings: c.report.warnings().count() as u32,
            codes: c.report.warnings().map(finding_code).collect(),
        };
        let record = CompileRecord {
            plan: &c.plan,
            rd_file: &rd,
            project: &c.project,
            project_path: a.positional.first().map(PathBuf::from),
            preflight,
            profile: c.profile.profile.revision(),
        };
        let id = h.record_compiled(env.fs.as_ref(), record, env.wall.now())?;
        let _ = writeln!(stdout, "job {id}");
        job = Some(id);
    }
    Ok(CliOutcome { stdout, plan: Some(c.plan.id()), job, execution: None })
}

/// GRBL backend: lower the compiled plan to G-code and write it. Same plan, same preflight; the
/// RD history journal is not used (it records RD bytes), so `--history` is refused.
fn compile_gcode(
    env: &CliEnv,
    a: &Args,
    c: &Compiled,
    grbl: &burnburn_gcode::GrblProfile,
    out: &Path,
) -> Result<CliOutcome, CliError> {
    if a.flag("history").is_some() {
        return Err(CliError::Usage("--history records RD jobs only; it is not available for the GRBL backend".into()));
    }
    let program = burnburn_gcode::lower(&c.plan, grbl).map_err(|e| match e {
        burnburn_gcode::LowerError::Refused(all) => {
            let mut report = String::new();
            for r in &all {
                let _ = writeln!(report, "error   {r}");
            }
            CliError::Preflight { errors: all.len(), report }
        }
        other => CliError::Compile(other.to_string()),
    })?;
    env.cancel.check().map_err(|_| CliError::Cancelled)?;
    let text = program.to_text();
    write_atomically(env.fs.as_ref(), out, text.as_bytes())?;
    if let Some(plan_json) = a.flag("plan-json") {
        let blob = encode_plan(&c.plan)?;
        write_atomically(env.fs.as_ref(), Path::new(plan_json), &blob)?;
    }
    let stdout = format!(
        "plan {}\ngcode {} lines, {} bytes -> {}\npreflight: 0 errors, {} warnings\nestimate {:.1} s (constant-velocity estimate, not measured)\n",
        c.plan.id(),
        program.lines().len(),
        text.len(),
        out.display(),
        c.report.warnings().count(),
        c.preview_seconds
    );
    Ok(CliOutcome { stdout, plan: Some(c.plan.id()), ..CliOutcome::default() })
}

fn write_atomically(fs: &dyn FileSystem, path: &Path, bytes: &[u8]) -> Result<(), CliError> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let stem = path.file_name().and_then(|n| n.to_str()).unwrap_or("burnburn");
    let temp = fs.create_temp_in(dir, stem)?;
    fs.write_all(&temp, bytes)?;
    fs.sync_file(&temp)?;
    fs.replace(&temp, path)?;
    fs.sync_dir(dir)?;
    Ok(())
}

pub fn inspect(env: &CliEnv, a: &Args) -> Result<CliOutcome, CliError> {
    a.reject_unknown(&["magic"])?;
    let path = a.one_positional("RD file")?;
    let magic = match a.flag("magic") {
        None => Magic::FIXTURE_0X88,
        Some(m) => {
            let v = m.strip_prefix("0x").map_or_else(|| m.parse(), |h| u8::from_str_radix(h, 16));
            Magic(v.map_err(|_| CliError::Usage("--magic takes a byte (e.g. 0x88)".into()))?)
        }
    };
    let bytes = env.fs.read(Path::new(path), MAX_RD_BYTES)?;
    let file = decode_rd_file(&bytes, magic).map_err(|e| CliError::Rd(format!("{e:?}")))?;
    let mut s = Summary::default();
    for r in &file.records {
        s.add(&r.command);
    }
    let mut out = format!(
        "{path}: {} bytes, {} records, file sum ok ({})\nrd hash {}\n",
        bytes.len(),
        file.records.len(),
        file.file_sum,
        burnburn_machine::rd_hash(&bytes)
    );
    let _ = writeln!(
        out,
        "moves {} travel, {} cut; {} unknown, {} registered-only records",
        s.travels, s.cuts, s.unknown, s.registered
    );
    if let Some([x0, y0, x1, y1]) = s.cut_bounds {
        let mm = |um: i64| um as f64 / 1000.0;
        let _ = writeln!(out, "cut bounds: x {:.3}..{:.3} mm, y {:.3}..{:.3} mm", mm(x0), mm(x1), mm(y0), mm(y1));
    } else {
        let _ = writeln!(out, "cut bounds: none (no laser-on motion)");
    }
    let speeds: Vec<String> = s.speeds.iter().map(|v| format!("{:.1}", *v as f64 / 1000.0)).collect();
    let _ = writeln!(out, "speeds mm/s: [{}]; max power raw {} of 16383", speeds.join(", "), s.max_power);
    let _ = writeln!(out, "decoder labels are not firmware semantics; values are unverified on hardware");
    Ok(CliOutcome { stdout: out, ..CliOutcome::default() })
}

#[derive(Default)]
struct Summary {
    at: (i64, i64),
    travels: usize,
    cuts: usize,
    unknown: usize,
    registered: usize,
    cut_bounds: Option<[i64; 4]>,
    speeds: std::collections::BTreeSet<u64>,
    max_power: u16,
}

impl Summary {
    fn cut_to(&mut self, to: (i64, i64)) {
        let from = self.at;
        let b = self.cut_bounds.get_or_insert([from.0, from.1, from.0, from.1]);
        for p in [from, to] {
            b[0] = b[0].min(p.0);
            b[1] = b[1].min(p.1);
            b[2] = b[2].max(p.0);
            b[3] = b[3].max(p.1);
        }
        self.at = to;
        self.cuts += 1;
    }

    fn add(&mut self, c: &Command) {
        let (x, y) = self.at;
        match c {
            Command::MoveAbs { x: nx, y: ny } => {
                self.at = (nx.um() as i64, ny.um() as i64);
                self.travels += 1;
            }
            Command::MoveRel { dx, dy } => {
                self.at = (x + i64::from(dx.um()), y + i64::from(dy.um()));
                self.travels += 1;
            }
            Command::MoveRelX(d) => {
                self.at = (x + i64::from(d.um()), y);
                self.travels += 1;
            }
            Command::MoveRelY(d) => {
                self.at = (x, y + i64::from(d.um()));
                self.travels += 1;
            }
            Command::CutAbs { x: nx, y: ny } => self.cut_to((nx.um() as i64, ny.um() as i64)),
            Command::CutRel { dx, dy } => self.cut_to((x + i64::from(dx.um()), y + i64::from(dy.um()))),
            Command::CutRelX(d) => self.cut_to((x + i64::from(d.um()), y)),
            Command::CutRelY(d) => self.cut_to((x, y + i64::from(d.um()))),
            Command::Speed(v) => {
                self.speeds.insert(v.um_per_s());
            }
            Command::PartSpeed { speed, .. } => {
                self.speeds.insert(speed.um_per_s());
            }
            Command::LaserPower { value, .. }
            | Command::PartPower { value, .. }
            | Command::ImmediatePower { value, .. }
            | Command::ThroughPower { value, .. } => self.max_power = self.max_power.max(value.raw()),
            Command::Unknown { .. } => self.unknown += 1,
            Command::Registered { .. } => self.registered += 1,
            _ => {}
        }
    }
}

fn state_name(s: &ExecutionStateDto) -> String {
    let v = serde_json::to_value(s).unwrap_or_default();
    let base = v.get("state").and_then(|x| x.as_str()).unwrap_or("?").to_owned();
    match s {
        ExecutionStateDto::Unknown { why } => format!("{base} ({why:?})"),
        ExecutionStateDto::Failed { why } => format!("{base} ({why:?})"),
        _ => base,
    }
}

pub fn history(env: &CliEnv, a: &Args) -> Result<CliOutcome, CliError> {
    a.reject_unknown(&["history", "project", "machine"])?;
    let dir = a.required("history")?;
    let (h, report) = History::open(env.fs.as_ref(), Path::new(dir), &JournalLimits::default())?;
    let project = match a.flag("project") {
        Some(p) => {
            Some(ContentHash::from_hex(p).map_err(|_| CliError::Usage("--project takes a project hash".into()))?)
        }
        None => None,
    };
    let machine = a.flag("machine").map(burnburn_machine::MachineId::new);
    let mut out = String::new();
    if h.read_only() {
        let _ = writeln!(out, "history is READ-ONLY: {:?}", report.corrupt);
    }
    for t in &report.torn_tails {
        let _ = writeln!(out, "torn tail kept in {} at {} ({} bytes)", t.segment.display(), t.offset, t.bytes);
    }
    for j in h.jobs().filter(|j| project.is_none_or(|p| j.project_hash == p)) {
        let path = j.project_path.as_ref().map_or_else(String::new, |p| format!(" {}", p.display()));
        let _ = writeln!(
            out,
            "job {} plan {} rd {} profile {}#{}{} at {}",
            j.id, j.plan, j.rd_hash, j.profile.id, j.profile.revision, path, j.created
        );
    }
    for e in h.executions() {
        let job_matches = project.is_none_or(|p| h.job(e.job).is_some_and(|j| j.project_hash == p));
        if !job_matches || machine.as_ref().is_some_and(|m| *m != e.machine) {
            continue;
        }
        let restart =
            if e.interrupted_by_restart { " [interrupted by restart: needs status query or operator]" } else { "" };
        let intents: Vec<String> = e
            .intents
            .iter()
            .map(|i| {
                serde_json::to_value(i.intent).ok().and_then(|v| v.as_str().map(str::to_owned)).unwrap_or_default()
            })
            .collect();
        let _ = writeln!(
            out,
            "execution {} job {} machine {} state {}{} intents [{}]",
            e.id.raw(),
            e.job,
            e.machine,
            state_name(&e.state),
            restart,
            intents.join(", ")
        );
    }
    for q in h.queue() {
        if let Some(c) = h.queue_counts(q.id) {
            let _ = writeln!(
                out,
                "queue {} job {}: planned {}, completed {}, unknown {}, failed {}, in progress {}, remaining {}",
                q.id.raw(),
                q.job,
                c.planned,
                c.completed,
                c.unknown,
                c.failed,
                c.in_progress,
                c.remaining
            );
        }
    }
    Ok(CliOutcome { stdout: out, ..CliOutcome::default() })
}

/// A machine action names its target machine (`--machine`, must be the machine the profile
/// describes) and its intent (`--intent`, must be the command being run). Checked before
/// anything is read from the machine side or connected.
fn machine_intent(a: &Args, intent: &str) -> Result<String, CliError> {
    let machine = a.required("machine")?.to_owned();
    let given = a.required("intent")?;
    if given != intent {
        return Err(CliError::Usage(format!("--intent {given} does not match the command ({intent})")));
    }
    Ok(machine)
}

fn machine_session(
    env: &CliEnv,
    a: &Args,
    intent: &str,
) -> Result<(MachineDriver<Box<dyn burnburn_machine::ControllerPort>>, ProfileFile), CliError> {
    if !a.switch("allow-machine-contact") {
        return Err(CliError::MachineContactNotAllowed);
    }
    let machine = machine_intent(a, intent)?;
    let profile = read_profile(env, a)?;
    if matches!(profile.backend, Backend::Grbl(_)) {
        return Err(CliError::Machine(
            "GRBL upload/start is not available: no serial adapter and no hardware evidence; nothing was contacted"
                .into(),
        ));
    }
    let identity = profile
        .identity
        .clone()
        .ok_or_else(|| CliError::Profile("profile has no machine identity (needed before any upload)".into()))?;
    if identity.id.as_str() != machine {
        return Err(CliError::MachineTargetMismatch { expected: identity.id.as_str().to_owned(), given: machine });
    }
    let peer: SocketAddr = a.required("peer")?.parse().map_err(|_| CliError::Usage("--peer takes ip:port".into()))?;
    let (history, _) = History::open(env.fs.as_ref(), Path::new(a.required("history")?), &JournalLimits::default())?;
    let port = env.connector.connect(peer, &identity.id).map_err(|e| CliError::Machine(e.to_string()))?;
    let actor = ExecutionActor::new(identity, port, Arc::clone(&env.clock), Policy::default());
    let driver = MachineDriver::new(actor, history, Arc::clone(&env.fs), Arc::clone(&env.wall));
    Ok((driver, profile))
}

/// Polls until `settled` or the poll budget is used up; returns the actor state.
fn drive(
    env: &CliEnv,
    d: &mut MachineDriver<Box<dyn burnburn_machine::ControllerPort>>,
    settled: impl Fn(&ExecutionState) -> bool,
) -> Result<Option<ExecutionState>, CliError> {
    for _ in 0..env.max_polls {
        d.poll().map_err(|e| CliError::Machine(e.to_string()))?;
        let state = d.actor().execution().map(|e| e.state.clone());
        if state.as_ref().is_some_and(&settled) {
            return Ok(state);
        }
        (env.pause)();
    }
    Ok(d.actor().execution().map(|e| e.state.clone()))
}

pub fn upload(env: &CliEnv, a: &Args) -> Result<CliOutcome, CliError> {
    if !a.switch("allow-machine-contact") {
        return Err(CliError::MachineContactNotAllowed);
    }
    a.reject_unknown(&["history", "job", "profile", "peer", "machine", "intent"])?;
    let job =
        ContentHash::from_hex(a.required("job")?).map_err(|_| CliError::Usage("--job takes a job hash".into()))?;
    let (mut d, profile) = machine_session(env, a, "upload")?;
    let job = d
        .history()
        .jobs()
        .map(|j| j.id)
        .find(|id: &CompiledJobId| id.hash() == job)
        .ok_or_else(|| CliError::Usage("no such job in this history".into()))?;
    let machine_err = |e: burnburn_runtime::DriverError| CliError::Machine(e.to_string());
    let exec = d.upload(job, None, profile.profile.revision(), (env.new_action)()).map_err(machine_err)?;
    let state =
        drive(env, &mut d, |s| !matches!(s, ExecutionState::VerifyingIdentity | ExecutionState::Uploading { .. }))?;
    let mut out = format!("execution {} job {job}\n", exec.raw());
    if state == Some(ExecutionState::UploadAcked) {
        d.confirm_stored((env.new_action)()).map_err(machine_err)?;
        let state = drive(env, &mut d, |s| *s != ExecutionState::UploadAcked)?;
        let line = match state {
            Some(ExecutionState::Stored) => "stored (read-back confirmed); not started".to_owned(),
            Some(ExecutionState::UploadAcked) => "uploaded, not verified (no read-back answer)".to_owned(),
            other => format!("{other:?}"),
        };
        let _ = writeln!(out, "{line}");
    } else {
        let _ = writeln!(out, "upload ended in {state:?} (nothing is resent automatically)");
    }
    Ok(CliOutcome { stdout: out, execution: Some(exec), job: Some(job), ..CliOutcome::default() })
}

pub fn start(env: &CliEnv, a: &Args) -> Result<CliOutcome, CliError> {
    if !a.switch("allow-machine-contact") {
        return Err(CliError::MachineContactNotAllowed);
    }
    a.reject_unknown(&["history", "execution", "profile", "peer", "machine", "intent"])?;
    let raw: u64 =
        a.required("execution")?.parse().map_err(|_| CliError::Usage("--execution takes a number".into()))?;
    let exec = ExecutionId::from_raw(raw);
    let (mut d, _) = machine_session(env, a, "start")?;
    let machine_err = |e: burnburn_runtime::DriverError| CliError::Machine(e.to_string());
    d.reverify(exec, (env.new_action)()).map_err(machine_err)?;
    let state = drive(env, &mut d, |s| !matches!(s, ExecutionState::VerifyingIdentity | ExecutionState::Reverifying))?;
    if state != Some(ExecutionState::Stored) {
        return Err(CliError::Machine(format!("not started: read-back did not confirm the stored job ({state:?})")));
    }
    d.start(exec, (env.new_action)()).map_err(machine_err)?;
    let state = drive(env, &mut d, |s| *s != ExecutionState::StartRequested)?;
    let line = match state {
        Some(ExecutionState::StartRequested) | Some(ExecutionState::Running) => {
            "start requested once (an ACK is not proof of running; query status to follow)".to_owned()
        }
        other => format!("start outcome {other:?} (never retried automatically)"),
    };
    Ok(CliOutcome { stdout: format!("execution {raw}\n{line}\n"), execution: Some(exec), ..CliOutcome::default() })
}

#[allow(dead_code)]
fn severity_name(s: Severity) -> &'static str {
    match s {
        Severity::Error => "error",
        _ => "warning",
    }
}
