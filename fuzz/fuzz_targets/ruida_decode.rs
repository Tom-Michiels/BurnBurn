//! Unscrambled RD record stream: `decode` must return `Ok` or a typed `DecodeError`, never panic,
//! and stay bounded (`MAX_DECODE_INPUT`).
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = burnburn_ruida::decode(data);
});
