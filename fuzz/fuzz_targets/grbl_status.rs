//! GRBL controller lines: the parser is total, an accepted status holds only finite numbers, and
//! the streaming session never panics or exceeds its RX window on arbitrary controller output.
#![no_main]

use burnburn_gcode::grbl::{GrblSession, Message, SessionConfig, parse_line, prepare_job};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);
    if let Ok(Message::Status(s)) = parse_line(&text) {
        for p in [s.mpos, s.wpos, s.wco].into_iter().flatten() {
            assert!(p.x.is_finite() && p.y.is_finite() && p.z.is_none_or(f64::is_finite));
        }
    }
    let mut session = GrblSession::new(SessionConfig::laser(1000));
    session.on_line("$30=1000");
    session.on_line("$32=1");
    session.on_line("<Idle|MPos:0,0,0|FS:0,0>");
    if let Ok(job) = prepare_job("G21\nG90\nG1 X10 F100\nM2\n") {
        let _ = session.start_job(job);
    }
    for line in text.lines() {
        let _ = session.on_line(line);
        assert!(session.bytes_in_flight() <= 127);
    }
});
