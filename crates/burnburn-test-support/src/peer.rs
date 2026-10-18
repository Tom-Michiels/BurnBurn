//! A generic scripted fake peer (fake controller) for transport and actor tests.

use std::collections::VecDeque;
use std::fmt::{self, Debug};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use burnburn_types::{Clock, MonotonicTime};

use crate::FakeClock;

type Matcher<Req> = Box<dyn Fn(&Req) -> bool + Send>;

enum Planned<Resp> {
    Deliver { delay: Duration, response: Resp },
    Lose(Resp),
}

/// One scripted expectation: the next request the peer must receive, and what it does then.
///
/// A step without replies answers nothing (a silently ignored request).
pub struct Step<Req, Resp> {
    description: String,
    matcher: Matcher<Req>,
    planned: Vec<Planned<Resp>>,
    disconnect: bool,
}

impl<Req, Resp> Step<Req, Resp> {
    /// Expect exactly `request` (by `PartialEq`).
    pub fn expect(request: Req) -> Self
    where
        Req: PartialEq + Debug + Send + 'static,
    {
        let description = format!("{request:?}");
        Self::expect_matching(description, move |got| *got == request)
    }

    /// Expect a request accepted by `matcher`; `description` appears in failure messages.
    pub fn expect_matching(description: impl Into<String>, matcher: impl Fn(&Req) -> bool + Send + 'static) -> Self {
        Step { description: description.into(), matcher: Box::new(matcher), planned: Vec::new(), disconnect: false }
    }

    /// Answer immediately (deliverable at the fake time the request was sent).
    pub fn reply(self, response: Resp) -> Self {
        self.reply_after(Duration::ZERO, response)
    }

    /// Answer once the fake clock reaches send-time + `delay`.
    pub fn reply_after(mut self, delay: Duration, response: Resp) -> Self {
        self.planned.push(Planned::Deliver { delay, response });
        self
    }

    /// Answer `copies` times with the same response (duplicated datagrams).
    pub fn reply_duplicated(mut self, response: Resp, copies: usize) -> Self
    where
        Resp: Clone,
    {
        for _ in 0..copies {
            self.planned.push(Planned::Deliver { delay: Duration::ZERO, response: response.clone() });
        }
        self
    }

    /// The peer produces `response` but it never arrives (lost reply). It is kept in
    /// [`ScriptedPeer::lost`] so a test can assert what the caller never saw.
    pub fn lose_reply(mut self, response: Resp) -> Self {
        self.planned.push(Planned::Lose(response));
        self
    }

    /// After receiving this request the peer drops the connection before any reply of this
    /// step arrives; scripted replies of this step count as lost.
    pub fn then_disconnect(mut self) -> Self {
        self.disconnect = true;
        self
    }
}

/// What happened to one [`ScriptedPeer::send`] call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendOutcome {
    /// Matched the next scripted step.
    Accepted,
    /// Did not match the next step, or the script was exhausted.
    Rejected,
    /// The connection was down; the request never reached the peer.
    Disconnected,
}

/// Every `send` attempt, in order, with the fake time it happened.
#[derive(Debug, Clone, PartialEq)]
pub struct SentRecord<Req> {
    pub at: MonotonicTime,
    pub request: Req,
    pub outcome: SendOutcome,
}

/// Why a scripted interaction failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PeerError {
    Disconnected,
    /// The request does not match scripted step `step` (0-based).
    Unexpected {
        step: usize,
        expected: String,
        got: String,
    },
    /// A request arrived after the last scripted step.
    Unscripted {
        got: String,
    },
    /// [`ScriptedPeer::verify_complete`]: steps left or rejected requests seen.
    Incomplete {
        remaining: Vec<String>,
        rejected: usize,
    },
}

impl fmt::Display for PeerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PeerError::Disconnected => write!(f, "peer disconnected"),
            PeerError::Unexpected { step, expected, got } => {
                write!(f, "step {step}: expected {expected}, got {got}")
            }
            PeerError::Unscripted { got } => write!(f, "unscripted request after last step: {got}"),
            PeerError::Incomplete { remaining, rejected } => {
                write!(f, "script incomplete: {rejected} rejected request(s), remaining steps {remaining:?}")
            }
        }
    }
}

impl std::error::Error for PeerError {}

struct Pending<Resp> {
    due: MonotonicTime,
    seq: u64,
    response: Resp,
}

struct State<Req, Resp> {
    steps: VecDeque<Step<Req, Resp>>,
    consumed: usize,
    connected: bool,
    pending: Vec<Pending<Resp>>,
    next_seq: u64,
    sent: Vec<SentRecord<Req>>,
    lost: Vec<Resp>,
}

impl<Req, Resp> State<Req, Resp> {
    fn drop_connection(&mut self) {
        self.connected = false;
        let mut pending = std::mem::take(&mut self.pending);
        pending.sort_by_key(|p| (p.due, p.seq));
        self.lost.extend(pending.into_iter().map(|p| p.response));
    }
}

/// A scripted fake peer: requests must arrive in the scripted order; replies can be delayed to
/// a fake time, duplicated, lost, or replaced by a disconnect. Everything sent is recorded.
///
/// Clones share one script, so the code under test and the test can each hold a handle.
pub struct ScriptedPeer<Req, Resp> {
    clock: FakeClock,
    state: Arc<Mutex<State<Req, Resp>>>,
}

impl<Req, Resp> Clone for ScriptedPeer<Req, Resp> {
    fn clone(&self) -> Self {
        ScriptedPeer { clock: self.clock.clone(), state: Arc::clone(&self.state) }
    }
}

impl<Req: Debug, Resp> ScriptedPeer<Req, Resp> {
    /// A connected peer with an empty script, timed by `clock`.
    pub fn new(clock: FakeClock) -> Self {
        let state = State {
            steps: VecDeque::new(),
            consumed: 0,
            connected: true,
            pending: Vec::new(),
            next_seq: 0,
            sent: Vec::new(),
            lost: Vec::new(),
        };
        ScriptedPeer { clock, state: Arc::new(Mutex::new(state)) }
    }

    fn lock(&self) -> MutexGuard<'_, State<Req, Resp>> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Appends a step to the script.
    pub fn script(&self, step: Step<Req, Resp>) -> &Self {
        self.lock().steps.push_back(step);
        self
    }

    /// Delivers `request` to the peer. A request that does not match the next step is
    /// rejected and leaves that step in place.
    pub fn send(&self, request: Req) -> Result<(), PeerError> {
        let at = self.clock.now();
        let mut state = self.lock();
        if !state.connected {
            state.sent.push(SentRecord { at, request, outcome: SendOutcome::Disconnected });
            return Err(PeerError::Disconnected);
        }
        let step_index = state.consumed;
        let verdict = match state.steps.front() {
            None => Err(PeerError::Unscripted { got: format!("{request:?}") }),
            Some(step) if (step.matcher)(&request) => Ok(()),
            Some(step) => Err(PeerError::Unexpected {
                step: step_index,
                expected: step.description.clone(),
                got: format!("{request:?}"),
            }),
        };
        let outcome = if verdict.is_ok() { SendOutcome::Accepted } else { SendOutcome::Rejected };
        state.sent.push(SentRecord { at, request, outcome });
        verdict?;

        let Some(step) = state.steps.pop_front() else { return Ok(()) };
        state.consumed += 1;
        for planned in step.planned {
            match planned {
                Planned::Lose(response) => state.lost.push(response),
                Planned::Deliver { response, .. } if step.disconnect => state.lost.push(response),
                Planned::Deliver { delay, response } => {
                    let due = at.checked_add(delay).unwrap_or(MonotonicTime::from_offset(Duration::MAX));
                    let seq = state.next_seq;
                    state.next_seq += 1;
                    state.pending.push(Pending { due, seq, response });
                }
            }
        }
        if step.disconnect {
            state.drop_connection();
        }
        Ok(())
    }

    /// The next reply that is due at the current fake time, if any: earliest due time first,
    /// then script order.
    pub fn recv(&self) -> Result<Option<Resp>, PeerError> {
        let now = self.clock.now();
        let mut state = self.lock();
        if !state.connected {
            return Err(PeerError::Disconnected);
        }
        let next = state
            .pending
            .iter()
            .enumerate()
            .filter(|(_, p)| p.due <= now)
            .min_by_key(|(_, p)| (p.due, p.seq))
            .map(|(i, _)| i);
        Ok(next.map(|i| state.pending.remove(i).response))
    }

    /// The fake time at which the next pending reply becomes deliverable.
    pub fn next_reply_due(&self) -> Option<MonotonicTime> {
        self.lock().pending.iter().map(|p| p.due).min()
    }

    /// Drops the connection now; undelivered replies are lost.
    pub fn disconnect(&self) {
        self.lock().drop_connection();
    }

    /// Restores the connection (the script continues where it was).
    pub fn reconnect(&self) {
        self.lock().connected = true;
    }

    pub fn is_connected(&self) -> bool {
        self.lock().connected
    }

    /// Every `send` attempt with its outcome.
    pub fn sent_log(&self) -> Vec<SentRecord<Req>>
    where
        Req: Clone,
    {
        self.lock().sent.clone()
    }

    /// The requests of every `send` attempt, in order.
    pub fn sent(&self) -> Vec<Req>
    where
        Req: Clone,
    {
        self.lock().sent.iter().map(|r| r.request.clone()).collect()
    }

    /// Replies the peer produced that never reached the caller.
    pub fn lost(&self) -> Vec<Resp>
    where
        Resp: Clone,
    {
        self.lock().lost.clone()
    }

    /// Ok when every step was consumed and no request was rejected.
    pub fn verify_complete(&self) -> Result<(), PeerError> {
        let state = self.lock();
        let rejected = state.sent.iter().filter(|r| r.outcome == SendOutcome::Rejected).count();
        if state.steps.is_empty() && rejected == 0 {
            return Ok(());
        }
        let remaining = state.steps.iter().map(|s| s.description.clone()).collect();
        Err(PeerError::Incomplete { remaining, rejected })
    }
}
