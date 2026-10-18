//! UDP datagram framing: a parsed payload is always the datagram minus its two checksum bytes.
#![no_main]

use burnburn_ruida::udp::parse_frame;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(payload) = parse_frame(data) {
        assert_eq!(payload.len() + 2, data.len());
    }
});
