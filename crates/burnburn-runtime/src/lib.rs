//! Concrete runtime adapters (ADR 0005): the monotonic OS clock, the Ruida UDP transport on its
//! own thread with a bounded queue, the (not yet supported) USB port, and the glue that makes
//! machine intents durable in the history before a driver sees them.
//!
//! No Tokio and no discovery: a UDP port only ever talks to the one configured peer address.

mod clock;
mod driver;
pub mod fonts;
mod history_store;
mod journal_fs;
pub mod udp;
pub mod update_gate;
pub mod usb;

pub use clock::SystemClock;
pub use driver::{DriverError, MachineDriver, SystemWallClock, WallClock};
pub use history_store::JournalHistoryStore;
pub use journal_fs::StdJournalFs;
pub use udp::{NoVerifiedDialect, RegistryLiveDialect, RuidaUdpPort, UdpConfig, UdpStats, WireDialect};
pub use usb::{USB_NOT_SUPPORTED, UsbConfig, UsbPort};
