//! A fake GRBL 1.1 controller behind `SerialPort`: a 128-byte RX buffer that really overflows,
//! realtime commands handled out of band, `ok`/`error:N` per consumed line and status reports.
//! The test steps the controller explicitly (`process`, `finish`), so there is no clock and no
//! thread. Never a real device.
#![allow(dead_code, clippy::unwrap_used)]

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use burnburn_gcode::{SerialError, SerialPort};

pub const CAPACITY: usize = 128;

#[derive(Default)]
pub struct Inner {
    rx: VecDeque<u8>,
    outbox: Vec<u8>,
    /// Lines consumed from the RX buffer, in order (what the machine would execute).
    pub received: Vec<String>,
    /// Count of lines consumed and acknowledged but not yet "finished" (physically executing).
    pub busy: usize,
    /// Set when a byte arrived while the RX buffer was full: a real GRBL would corrupt a line.
    pub overflowed: bool,
    pub max_rx_fill: usize,
    pub held: bool,
    pub resets: usize,
    pub jog_cancels: usize,
    pub write_attempts: usize,
    /// Consumed-line indices (0-based, counting only non-`$` lines) answered with `error:20`.
    pub reject_lines: Vec<usize>,
    pub alarm: bool,
    pub fail_writes: bool,
    pub fail_reads: bool,
    /// Bytes handed out per `read_available` call (0 = everything); lets tests split lines.
    pub read_chunk: usize,
    non_system_seen: usize,
}

#[derive(Clone, Default)]
pub struct FakeGrbl(pub Rc<RefCell<Inner>>);

impl FakeGrbl {
    pub fn new() -> FakeGrbl {
        FakeGrbl::default()
    }

    pub fn inner(&self) -> std::cell::RefMut<'_, Inner> {
        self.0.borrow_mut()
    }

    /// Controller consumes up to `n` complete lines from its RX buffer and answers each.
    pub fn process(&self, n: usize) {
        let mut c = self.0.borrow_mut();
        for _ in 0..n {
            let Some(pos) = c.rx.iter().position(|b| *b == b'\n') else { return };
            let line: Vec<u8> = c.rx.drain(..=pos).collect();
            let line = String::from_utf8_lossy(&line[..line.len() - 1]).to_string();
            if line.starts_with('$') {
                if line == "$$" {
                    c.outbox.extend_from_slice(b"$30=1000.000\r\n$32=1\r\n");
                }
                c.outbox.extend_from_slice(b"ok\r\n");
                continue;
            }
            let index = c.non_system_seen;
            c.non_system_seen += 1;
            c.received.push(line);
            if c.reject_lines.contains(&index) {
                c.outbox.extend_from_slice(b"error:20\r\n");
            } else {
                c.busy += 1;
                c.outbox.extend_from_slice(b"ok\r\n");
            }
        }
    }

    /// Everything the machine had queued has now physically executed.
    pub fn finish(&self) {
        self.0.borrow_mut().busy = 0;
    }

    /// Pushes raw bytes to the host as if the controller had sent them.
    pub fn say(&self, bytes: &[u8]) {
        self.0.borrow_mut().outbox.extend_from_slice(bytes);
    }

    pub fn received_text(&self) -> String {
        self.0.borrow().received.iter().map(|l| format!("{l}\n")).collect()
    }
}

impl SerialPort for FakeGrbl {
    fn write_all(&mut self, bytes: &[u8]) -> Result<(), SerialError> {
        let mut c = self.0.borrow_mut();
        c.write_attempts += 1;
        if c.fail_writes {
            return Err(SerialError::Disconnected);
        }
        for &b in bytes {
            match b {
                b'?' => {
                    let state = if c.alarm {
                        "Alarm".to_owned()
                    } else if c.held {
                        "Hold:0".to_owned()
                    } else if c.busy > 0 {
                        "Run".to_owned()
                    } else {
                        "Idle".to_owned()
                    };
                    let report = format!("<{state}|MPos:0.000,0.000,0.000|FS:0,0>\r\n");
                    c.outbox.extend_from_slice(report.as_bytes());
                }
                b'!' => c.held = true,
                b'~' => c.held = false,
                0x18 => {
                    c.rx.clear();
                    c.busy = 0;
                    c.held = false;
                    c.resets += 1;
                    c.outbox.extend_from_slice(b"\r\nGrbl 1.1h ['$' for help]\r\n");
                }
                0x85 => c.jog_cancels += 1,
                other => {
                    if c.rx.len() >= CAPACITY {
                        c.overflowed = true;
                    } else {
                        c.rx.push_back(other);
                        c.max_rx_fill = c.max_rx_fill.max(c.rx.len());
                    }
                }
            }
        }
        Ok(())
    }

    fn read_available(&mut self, out: &mut Vec<u8>) -> Result<(), SerialError> {
        let mut c = self.0.borrow_mut();
        if c.fail_reads {
            return Err(SerialError::Disconnected);
        }
        let n = if c.read_chunk == 0 { c.outbox.len() } else { c.read_chunk.min(c.outbox.len()) };
        out.extend(c.outbox.drain(..n));
        Ok(())
    }
}
