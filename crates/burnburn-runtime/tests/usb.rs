//! USB is defined as a port but not supported yet: it must say so, not pretend.

use burnburn_machine::PortError;
use burnburn_runtime::{USB_NOT_SUPPORTED, UsbConfig, UsbPort};

#[test]
fn usb_transport_reports_not_supported_yet() {
    let r = UsbPort::open(UsbConfig { vendor_id: 0x1234, product_id: 0x5678, device: None });
    assert_eq!(r.map(|_| ()), Err(PortError::NotSupported(USB_NOT_SUPPORTED.into())));
}
