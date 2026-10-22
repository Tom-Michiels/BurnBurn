//! Laser-window fixtures (W2-L1-laser): a history that also journals live actions, a lab with a
//! scripted fake controller and the capability sets.

#![allow(dead_code, clippy::expect_used, clippy::unwrap_used, clippy::panic)] // shared test helpers

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::FakeClock;
use burnburn_job::MachineFrame;
use burnburn_machine::{Capabilities, Capability, ExecutionId, MachineId};
use burnburn_presentation::{
    HistoryStore, HistoryStoreError, IntentRecord, LiveIntentRecord, LiveOutcomeRecord, MachineService, OutcomeRecord,
    Recorded, RecoveredMachine, StoredOrder, StoredOrders, UploadRecord,
};

use super::{FakePort, Peer, RecordingHistory, profile, service_with};

/// The common history fake plus the live records, each with the number of requests the
/// controller had seen when the record was made ("recorded before sent").
pub struct LiveHistory {
    pub inner: Arc<RecordingHistory>,
    pub peer: Peer,
    pub intents: Mutex<Vec<(LiveIntentRecord, usize)>>,
    pub outcomes: Mutex<Vec<LiveOutcomeRecord>>,
    pub fail: AtomicBool,
    /// The options of every diagnostic report asked for.
    pub reports: Mutex<Vec<burnburn_presentation::DiagnosticOptions>>,
}

impl HistoryStore for LiveHistory {
    fn record_upload(&self, r: &UploadRecord) -> Result<(ExecutionId, Recorded), HistoryStoreError> {
        self.inner.record_upload(r)
    }
    fn record_intent(&self, r: &IntentRecord) -> Result<Recorded, HistoryStoreError> {
        self.inner.record_intent(r)
    }
    fn record_outcome(&self, r: &OutcomeRecord) -> Result<Recorded, HistoryStoreError> {
        self.inner.record_outcome(r)
    }
    fn record_observed(&self, e: &burnburn_machine::Execution) -> Result<(), HistoryStoreError> {
        self.inner.record_observed(e)
    }
    fn recover(&self, m: &MachineId) -> Result<RecoveredMachine, HistoryStoreError> {
        self.inner.recover(m)
    }
    fn record_order(&self, o: &StoredOrder) -> Result<(), HistoryStoreError> {
        self.inner.record_order(o)
    }
    fn orders(&self) -> Result<StoredOrders, HistoryStoreError> {
        self.inner.orders()
    }
    fn record_live_intent(&self, r: &LiveIntentRecord) -> Result<Recorded, HistoryStoreError> {
        if self.fail.load(Ordering::SeqCst) {
            return Err(HistoryStoreError("disk full".into()));
        }
        if self.intents.lock().unwrap().iter().any(|(i, _)| i.action == r.action) {
            return Ok(Recorded::AlreadyRecorded);
        }
        self.intents.lock().unwrap().push((r.clone(), self.peer.sent().len()));
        Ok(Recorded::Fresh)
    }
    fn diagnostic_report(&self, o: &burnburn_presentation::DiagnosticOptions) -> Result<String, HistoryStoreError> {
        self.reports.lock().unwrap().push(*o);
        Ok(format!("diagnostic replay: paths={} ids={}", o.include_project_paths, o.include_machine_ids))
    }
    fn record_live_outcome(&self, r: &LiveOutcomeRecord) -> Result<(), HistoryStoreError> {
        self.outcomes.lock().unwrap().push(r.clone());
        Ok(())
    }
}

pub struct Lab {
    pub clock: FakeClock,
    pub peer: Peer,
    pub history: Arc<LiveHistory>,
    pub service: Arc<MachineService<FakePort>>,
}

pub fn lab() -> Lab {
    let clock = FakeClock::new();
    let peer = Peer::new(clock.clone());
    let history = Arc::new(LiveHistory {
        inner: super::history(&peer),
        peer: peer.clone(),
        intents: Mutex::new(Vec::new()),
        outcomes: Mutex::new(Vec::new()),
        fail: AtomicBool::new(false),
        reports: Mutex::new(Vec::new()),
    });
    let service = service_with(&clock, &peer, history.clone());
    Lab { clock, peer, history, service }
}

pub fn all_supported() -> Capabilities {
    let mut c = Capabilities::default();
    c.live.jog = Capability::Supported;
    c.live.home = Capability::Supported;
    c.live.move_absolute = Capability::Supported;
    c.live.pause_resume = Capability::Supported;
    c.live.read_position = Capability::Supported;
    c.files.list = Capability::Supported;
    c.files.metadata = Capability::Supported;
    c.files.run = Capability::Supported;
    c.files.delete = Capability::Supported;
    c.files.upload_named = Capability::Supported;
    c
}

pub fn frame() -> MachineFrame {
    *profile(1).frame()
}
