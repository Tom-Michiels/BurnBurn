//! Direct computer-USB transport: port definition only (IM29).
//!
//! VID/PID, endpoints, framing, pacing and connect/disconnect behaviour are not established
//! for any controller (`research/lightburn/ruida/03-transport.md` § USB). Until a USB bench
//! and captures exist, opening a USB port fails with `PortError::NotSupported`; nothing is
//! guessed. A future driver follows the driver-thread model of ADR 0005, like UDP.

use burnburn_machine::{ControllerPort, Envelope, Incoming, PortError};

/// What will identify a USB controller once it is supported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsbConfig {
    pub vendor_id: u16,
    pub product_id: u16,
    /// OS device path or serial, when the OS exposes one.
    pub device: Option<String>,
}

pub const USB_NOT_SUPPORTED: &str = "USB transport is not supported yet (no verified driver for any model)";

/// Placeholder `ControllerPort` for direct USB.
#[derive(Debug)]
pub struct UsbPort {
    _cfg: UsbConfig,
}

impl UsbPort {
    pub fn open(cfg: UsbConfig) -> Result<UsbPort, PortError> {
        let _ = UsbPort { _cfg: cfg };
        Err(PortError::NotSupported(USB_NOT_SUPPORTED.into()))
    }
}

impl ControllerPort for UsbPort {
    fn send(&mut self, _envelope: Envelope) -> Result<(), PortError> {
        Err(PortError::NotSupported(USB_NOT_SUPPORTED.into()))
    }

    fn try_recv(&mut self) -> Result<Option<Incoming>, PortError> {
        Err(PortError::NotSupported(USB_NOT_SUPPORTED.into()))
    }

    fn reconnect(&mut self) -> Result<(), PortError> {
        Err(PortError::NotSupported(USB_NOT_SUPPORTED.into()))
    }
}
