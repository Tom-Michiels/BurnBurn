//! A fake GRBL 1.1 controller behind the `SerialPort` trait, for the host tests of the GRBL window
//! (G5-grbl-ui). No device, no thread, no clock: it answers each complete line at once (or holds
//! the answers until the test releases them) and records what the host sent. Never a real laser.

#![allow(dead_code)]

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use burnburn_presentation::{GrblSerialError, GrblSerialPort};

#[derive(Default)]
pub struct Inner {
    line: Vec<u8>,
    outbox: Vec<u8>,
    /// Lines the controller consumed, in order.
    pub received: Vec<String>,
    /// Realtime bytes the controller saw (`!`, `~`, 0x18, 0x85).
    pub realtime: Vec<u8>,
    pub state: String,
    /// Lines containing this text are answered with `error:20`.
    pub reject: Option<String>,
    pub held: bool,
    pending: VecDeque<String>,
}

#[derive(Clone)]
pub struct FakeGrbl(pub Rc<RefCell<Inner>>);

impl Default for FakeGrbl {
    fn default() -> Self {
        FakeGrbl::new()
    }
}

impl FakeGrbl {
    pub fn new() -> FakeGrbl {
        FakeGrbl(Rc::new(RefCell::new(Inner { state: "Idle".into(), ..Inner::default() })))
    }

    pub fn received(&self) -> Vec<String> {
        self.0.borrow().received.clone()
    }

    pub fn realtime(&self) -> Vec<u8> {
        self.0.borrow().realtime.clone()
    }

    pub fn set_state(&self, state: &str) {
        self.0.borrow_mut().state = state.into();
    }

    pub fn hold_answers(&self, held: bool) {
        self.0.borrow_mut().held = held;
    }

    /// Releases the oldest held answer.
    pub fn answer_one(&self) {
        let mut c = self.0.borrow_mut();
        if let Some(a) = c.pending.pop_front() {
            c.outbox.extend_from_slice(a.as_bytes());
        }
    }
}

impl GrblSerialPort for FakeGrbl {
    fn write_all(&mut self, bytes: &[u8]) -> Result<(), GrblSerialError> {
        let mut c = self.0.borrow_mut();
        for &b in bytes {
            match b {
                b'?' => {
                    let report = format!("<{}|MPos:0.000,0.000,0.000|FS:0,0>\r\n", c.state);
                    c.outbox.extend_from_slice(report.as_bytes());
                }
                b'!' | b'~' | 0x18 | 0x85 => c.realtime.push(b),
                b'\n' => {
                    let line = String::from_utf8_lossy(&std::mem::take(&mut c.line)).into_owned();
                    let mut answer = String::new();
                    if line == "$$" {
                        answer.push_str("$30=1000.000\r\n$32=1\r\n");
                    } else {
                        c.received.push(line.clone());
                    }
                    if c.reject.as_ref().is_some_and(|r| line.contains(r.as_str())) {
                        answer.push_str("error:20\r\n");
                    } else {
                        answer.push_str("ok\r\n");
                    }
                    if c.held {
                        c.pending.push_back(answer);
                    } else {
                        c.outbox.extend_from_slice(answer.as_bytes());
                    }
                }
                other => c.line.push(other),
            }
        }
        Ok(())
    }

    fn read_available(&mut self, out: &mut Vec<u8>) -> Result<(), GrblSerialError> {
        out.append(&mut self.0.borrow_mut().outbox);
        Ok(())
    }
}
