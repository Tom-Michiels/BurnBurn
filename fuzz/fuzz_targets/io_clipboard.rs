//! The clipboard payload from the OS pasteboard: any other app can put bytes under our type.
#![no_main]

use burnburn_io::clipboard::decode_clipboard;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = decode_clipboard(data);
});
