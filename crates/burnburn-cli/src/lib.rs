//! The `burnburn` command line (IM30, X05).
//!
//! - `compile <project> --profile <p.json> --out <x.rd> [--plan-json <p>] [--history <dir>]`
//! - `inspect <x.rd> [--magic <n>]` — decode overview of an RD file
//! - `preflight <project> --profile <p.json>`
//! - `history --history <dir> [--project <hash>] [--machine <id>]`
//! - `upload --history <dir> --job <id> --profile <p.json> --peer <ip:port> --allow-machine-contact`
//! - `start --history <dir> --execution <n> --profile <p.json> --peer <ip:port> --allow-machine-contact`
//!
//! Compile runs the same `burnburn_presentation::run_task` as the GUI (same options), so the
//! same project revision and profile give the same `JobPlanId`. Compile/export never touch a
//! machine. Upload and start are separate intents, each needing an explicit machine-contact
//! switch; there is no `upload --and-start`. Start only follows fresh read-back proof in the
//! same invocation and is never sent twice for one execution (history first, then driver).

mod args;
mod commands;
pub mod plugin_manifest;
pub mod profile_file;

use std::fmt;
use std::net::SocketAddr;
use std::sync::Arc;

use burnburn_history::{CompiledJobId, HistoryError, JournalFs};
use burnburn_io::fs::FsError;
use burnburn_job::JobPlanId;
use burnburn_machine::{ControllerPort, ExecutionId, MachineId, PortError};
use burnburn_runtime::WallClock;
use burnburn_types::{CancellationToken, Clock};

pub use args::Args;
pub use commands::{COMPILE_TOLERANCE_MM, compile_options};

/// Creates the transport to one machine (production: Ruida UDP; tests: a scripted fake).
pub trait MachineConnector: Send + Sync {
    fn connect(&self, peer: SocketAddr, machine: &MachineId) -> Result<Box<dyn ControllerPort>, PortError>;
}

/// Everything the CLI touches, injected (tests: in-memory fs, fake clock, fake controller).
#[derive(Clone)]
pub struct CliEnv {
    pub fs: Arc<dyn JournalFs>,
    pub clock: Arc<dyn Clock>,
    pub wall: Arc<dyn WallClock>,
    pub cancel: CancellationToken,
    pub connector: Arc<dyn MachineConnector>,
    /// Called between actor polls while waiting for replies (production: a short sleep).
    pub pause: Arc<dyn Fn() + Send + Sync>,
    /// A fresh action id per user action (idempotency key; never reused).
    pub new_action: Arc<dyn Fn() -> burnburn_history::ActionId + Send + Sync>,
    /// Upper bound of polls per waiting step.
    pub max_polls: u32,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CliOutcome {
    pub stdout: String,
    pub plan: Option<JobPlanId>,
    pub job: Option<CompiledJobId>,
    pub execution: Option<ExecutionId>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CliError {
    Usage(String),
    Fs(FsError),
    Load(String),
    Profile(String),
    Compile(String),
    /// Preflight found errors; nothing was written.
    Preflight {
        errors: usize,
        report: String,
    },
    Cancelled,
    Rd(String),
    History(HistoryError),
    /// Upload/start without `--allow-machine-contact`: nothing was contacted.
    MachineContactNotAllowed,
    Machine(String),
    NotImplemented(&'static str),
}

impl CliError {
    /// Process exit code: 2 for refusals the user can act on (preflight, safety), 1 otherwise.
    pub fn exit_code(&self) -> i32 {
        match self {
            CliError::Preflight { .. } | CliError::MachineContactNotAllowed | CliError::Usage(_) => 2,
            _ => 1,
        }
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CliError::Usage(m) => write!(f, "usage: {m}"),
            CliError::Preflight { errors, report } => {
                write!(f, "preflight: {errors} error(s); nothing written\n{report}")
            }
            CliError::MachineContactNotAllowed => {
                f.write_str("refusing to contact a machine without --allow-machine-contact")
            }
            other => write!(f, "{other:?}"),
        }
    }
}

impl std::error::Error for CliError {}

impl From<FsError> for CliError {
    fn from(e: FsError) -> Self {
        CliError::Fs(e)
    }
}

impl From<HistoryError> for CliError {
    fn from(e: HistoryError) -> Self {
        CliError::History(e)
    }
}

/// Runs one command line (without the program name).
pub fn run(argv: &[String], env: &CliEnv) -> Result<CliOutcome, CliError> {
    let a = Args::parse(argv)?;
    match a.command.as_str() {
        "compile" => commands::compile(env, &a),
        "inspect" => commands::inspect(env, &a),
        "preflight" => commands::preflight(env, &a),
        "history" => commands::history(env, &a),
        "upload" => commands::upload(env, &a),
        "start" => commands::start(env, &a),
        other => Err(CliError::Usage(format!("unknown command {other:?}"))),
    }
}
