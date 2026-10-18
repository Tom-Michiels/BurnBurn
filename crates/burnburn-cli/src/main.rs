//! `burnburn` binary: the production environment around [`burnburn_cli::run`].

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use burnburn_cli::{CliEnv, MachineConnector, run};
use burnburn_history::{ActionId, JournalFs};
use burnburn_machine::{ControllerPort, MachineId, PortError};
use burnburn_runtime::{StdJournalFs, SystemClock, SystemWallClock};
use burnburn_types::CancellationToken;

/// Ruida UDP needs a verified wire dialect for identity, read-back and start. None exists yet
/// for any model, so the production CLI refuses before opening a socket instead of guessing.
struct RuidaUdpConnector;

impl MachineConnector for RuidaUdpConnector {
    fn connect(&self, peer: SocketAddr, machine: &MachineId) -> Result<Box<dyn ControllerPort>, PortError> {
        Err(PortError::NotSupported(format!(
            "no verified Ruida dialect (identity/read-back/start) for {machine} at {peer}; nothing was sent"
        )))
    }
}

fn action_id() -> ActionId {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let hi = (u128::from(std::process::id()) << 32) | u128::from(n as u32);
    ActionId::from_raw((hi << 64) | u128::from(nanos))
}

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let env = CliEnv {
        fs: Arc::new(StdJournalFs::new()) as Arc<dyn JournalFs>,
        clock: Arc::new(SystemClock::new()),
        wall: Arc::new(SystemWallClock),
        cancel: CancellationToken::never(),
        connector: Arc::new(RuidaUdpConnector),
        pause: Arc::new(|| std::thread::sleep(Duration::from_millis(20))),
        new_action: Arc::new(action_id),
        max_polls: 500,
    };
    match run(&argv, &env) {
        Ok(out) => print!("{}", out.stdout),
        Err(e) => {
            eprintln!("burnburn: {e}");
            std::process::exit(e.exit_code());
        }
    }
}
