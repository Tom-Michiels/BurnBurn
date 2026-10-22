//! Shared rig for the production scenarios (gate C, IM31): a document with one cut operation, a
//! machine service over the real `ExecutionActor` and a scripted fake controller with a fake
//! clock. No socket, no real laser, no wall clock.

#![allow(dead_code, clippy::expect_used, clippy::unwrap_used, clippy::panic)] // shared test helpers

pub mod laser;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use burnburn_document::process::{
    Kerf, OperationDraft, OperationMode, Passes, PowerPercent, PowerRange, ProcessSettings, Speed,
};
use burnburn_job::{MachineFrame, MachineProfile, OriginCorner, PowerRatio};
use burnburn_machine::{
    ControllerIdentity, ControllerPort, Envelope, ExecutionActor, Incoming, MachineId, MachineIdentity, Policy,
    PortError, Reply, Request, RequestTag,
};
use burnburn_presentation::{
    ActionSubject, CommandKey, HeadlessEditorHarness, HistoryStore, HistoryStoreError, IntentRecord, JobCardViewModel,
    MachineAction, MachineService, OrderId, OutcomeRecord, Recorded, RecoveredExecution, RecoveredMachine, SelectMode,
    StoredOrder, StoredOrders, UiIntent, UploadRecord,
};
use burnburn_ruida::{Magic, OverscanPolicy, RuidaProfile};
use burnburn_test_support::{FakeClock, PeerError, ScriptedPeer, Step};
use burnburn_types::DocumentId;

pub type Peer = ScriptedPeer<Request, Reply>;

pub const MACHINE: &str = "werkplaats-ruida-1";
/// Model configured in the machine profile and reported by the fake controller.
pub const MODEL: &str = "RDC6445G";
pub const TIMEOUT: Duration = Duration::from_millis(500);
/// Small on purpose, so an upload has several chunks.
pub const CHUNK: usize = 64;
pub const RUIDA: RuidaProfile =
    RuidaProfile { magic: Magic::FIXTURE_0X88, overscan: OverscanPolicy::ControllerManaged };

/// Test adapter in the role of the UDP adapter: one request in flight, each reply is attributed to
/// the last request sent.
pub struct FakePort {
    peer: Peer,
    machine: MachineId,
    last_tag: Option<RequestTag>,
}

impl ControllerPort for FakePort {
    fn send(&mut self, envelope: Envelope) -> Result<(), PortError> {
        assert_eq!(envelope.to, self.machine, "service addressed another machine");
        self.last_tag = Some(envelope.tag);
        self.peer.send(envelope.request).map_err(port_error)
    }

    fn try_recv(&mut self) -> Result<Option<Incoming>, PortError> {
        let reply = self.peer.recv().map_err(port_error)?;
        Ok(reply.map(|reply| Incoming { from: self.machine.clone(), tag: self.last_tag.unwrap(), reply }))
    }

    fn reconnect(&mut self) -> Result<(), PortError> {
        self.peer.reconnect();
        Ok(())
    }
}

fn port_error(e: PeerError) -> PortError {
    match e {
        PeerError::Disconnected => PortError::Disconnected,
        other => PortError::Other(other.to_string()),
    }
}

/// History fake (in memory, survives a "restart" of the service): records each intent with how
/// many requests the controller had seen at that moment (so a test can prove "recorded before
/// sent"), the executions it allocated, observed state and orders. Can be switched to failing.
pub struct RecordingHistory {
    peer: Peer,
    pub records: Mutex<Vec<(IntentRecord, usize)>>,
    pub fail: AtomicBool,
    pub executions: Mutex<Vec<RecoveredExecution>>,
    pub outcomes: Mutex<Vec<OutcomeRecord>>,
    pub orders: Mutex<StoredOrders>,
    /// Jobs the designer released (W01): kept like the journal keeps them, rebuilt without inputs.
    pub approvals: Mutex<Vec<Arc<burnburn_presentation::PreparedJob>>>,
    /// The names of the parts of the project of each released job, kept like the journal keeps the
    /// project blob (W01/W02, G8).
    pub parts: Mutex<Vec<(burnburn_job::JobPlanId, Vec<String>)>>,
}

impl RecordingHistory {
    fn check(&self) -> Result<(), HistoryStoreError> {
        if self.fail.load(Ordering::SeqCst) { Err(HistoryStoreError("disk full".into())) } else { Ok(()) }
    }

    fn seen(&self, action: &burnburn_presentation::ActionId) -> bool {
        self.records.lock().unwrap().iter().any(|(r, _)| r.action == *action)
            || self.outcomes.lock().unwrap().iter().any(|o| o.action == *action)
    }
}

impl HistoryStore for RecordingHistory {
    fn record_upload(
        &self,
        record: &UploadRecord,
    ) -> Result<(burnburn_machine::ExecutionId, Recorded), HistoryStoreError> {
        self.check()?;
        if let Some((r, _)) = self.records.lock().unwrap().iter().find(|(r, _)| r.action == record.action) {
            return Ok((r.execution, Recorded::AlreadyRecorded));
        }
        let mut executions = self.executions.lock().unwrap();
        let id = burnburn_machine::ExecutionId::from_raw(executions.len() as u64 + 1);
        executions.push(RecoveredExecution {
            execution: burnburn_machine::Execution {
                id,
                machine: record.machine.clone(),
                plan: record.job.plan,
                rd_hash: record.job.rd_hash,
                state: burnburn_machine::ExecutionState::VerifyingIdentity,
                evidence: Vec::new(),
                start_sent: false,
            },
            job: Arc::clone(&record.job),
            order: record.order,
        });
        if let Some(order) = record.order {
            self.orders.lock().unwrap().executions.push((id, order));
        }
        let intent = IntentRecord {
            action: record.action,
            machine: record.machine.clone(),
            execution: id,
            plan: record.job.plan,
            rd_hash: record.job.rd_hash,
        };
        self.records.lock().unwrap().push((intent, self.peer.sent().len()));
        Ok((id, Recorded::Fresh))
    }

    fn record_intent(&self, record: &IntentRecord) -> Result<Recorded, HistoryStoreError> {
        self.check()?;
        if self.seen(&record.action) {
            return Ok(Recorded::AlreadyRecorded);
        }
        self.records.lock().unwrap().push((record.clone(), self.peer.sent().len()));
        Ok(Recorded::Fresh)
    }

    fn record_outcome(&self, record: &OutcomeRecord) -> Result<Recorded, HistoryStoreError> {
        self.check()?;
        if self.seen(&record.action) {
            return Ok(Recorded::AlreadyRecorded);
        }
        self.outcomes.lock().unwrap().push(*record);
        if let Some(e) = self.executions.lock().unwrap().iter_mut().find(|e| e.execution.id == record.execution) {
            e.execution.state = record.outcome.state();
        }
        Ok(Recorded::Fresh)
    }

    fn record_observed(&self, execution: &burnburn_machine::Execution) -> Result<(), HistoryStoreError> {
        if let Some(e) = self.executions.lock().unwrap().iter_mut().find(|e| e.execution.id == execution.id) {
            e.execution.state = execution.state.clone();
            e.execution.start_sent |= execution.start_sent;
        }
        Ok(())
    }

    fn recover(&self, machine: &MachineId) -> Result<RecoveredMachine, HistoryStoreError> {
        let executions: Vec<RecoveredExecution> =
            self.executions.lock().unwrap().iter().filter(|e| e.execution.machine == *machine).cloned().collect();
        let mut handled: Vec<((MachineAction, ActionSubject), u32)> = Vec::new();
        for (r, _) in self.records.lock().unwrap().iter() {
            let key = (r.action.kind, r.action.subject);
            match handled.iter_mut().find(|(k, _)| *k == key) {
                Some((_, n)) => *n += 1,
                None => handled.push((key, 1)),
            }
        }
        Ok(RecoveredMachine { executions, handled })
    }

    fn record_approval(
        &self,
        job: &burnburn_presentation::PreparedJob,
        role: burnburn_presentation::Role,
    ) -> Result<(), HistoryStoreError> {
        self.check()?;
        if role != burnburn_presentation::Role::Designer {
            return Err(HistoryStoreError("only a designer approves".into()));
        }
        let mut approvals = self.approvals.lock().unwrap();
        if !approvals.iter().any(|a| a.plan == job.plan) {
            approvals.push(Arc::new(burnburn_presentation::PreparedJob { inputs: None, ..job.clone() }));
            if let Some(inputs) = job.inputs.as_ref() {
                self.parts.lock().unwrap().push((job.plan, inputs.project.order_part_names()));
            }
        }
        Ok(())
    }

    fn approved_jobs(&self) -> Result<Vec<Arc<burnburn_presentation::PreparedJob>>, HistoryStoreError> {
        Ok(self.approvals.lock().unwrap().clone())
    }

    fn job_parts(&self, plan: burnburn_job::JobPlanId) -> Vec<String> {
        self.parts.lock().unwrap().iter().find(|(p, _)| *p == plan).map(|(_, n)| n.clone()).unwrap_or_default()
    }

    fn record_order(&self, order: &StoredOrder) -> Result<(), HistoryStoreError> {
        self.check()?;
        let mut orders = self.orders.lock().unwrap();
        orders.orders.retain(|o| o.id != order.id);
        orders.orders.push(order.clone());
        orders.orders.sort_by_key(|o| o.id);
        Ok(())
    }

    fn orders(&self) -> Result<StoredOrders, HistoryStoreError> {
        Ok(self.orders.lock().unwrap().clone())
    }
}

/// Order ids are compared by value in the fake.
pub fn order_id(n: u64) -> OrderId {
    OrderId(n)
}

pub struct Rig {
    pub h: HeadlessEditorHarness,
    pub doc: DocumentId,
    pub clock: FakeClock,
    pub peer: Peer,
    pub service: Arc<MachineService<FakePort>>,
    pub history: Arc<RecordingHistory>,
}

pub fn profile(revision: u32) -> MachineProfile {
    MachineProfile::new(
        "research-1500x900",
        revision,
        MachineFrame::new(1500.0, 900.0, OriginCorner::BottomLeft).unwrap(),
        1000.0,
        400.0,
        PowerRatio::new(0.0).unwrap(),
        PowerRatio::new(1.0).unwrap(),
    )
    .unwrap()
}

pub fn cut() -> OperationDraft {
    let p = PowerPercent::new(20.0).unwrap();
    OperationDraft {
        name: "Snijden".into(),
        mode: OperationMode::Line { kerf: Kerf::NONE },
        settings: ProcessSettings {
            speed: Speed::mm_per_s(100.0).unwrap(),
            power: PowerRange::new(p, p).unwrap(),
            passes: Passes::new(1).unwrap(),
            air_assist: true,
        },
        output_enabled: true,
    }
}

/// A new machine service for `peer`, as the runtime would create it once per machine.
pub fn service(clock: &FakeClock, peer: &Peer, history: Arc<RecordingHistory>) -> Arc<MachineService<FakePort>> {
    let machine = MachineId::new(MACHINE);
    let port = FakePort { peer: peer.clone(), machine: machine.clone(), last_tag: None };
    let policy = Policy { reply_timeout: TIMEOUT, max_query_attempts: 2 };
    let identity = MachineIdentity { id: machine, model: Some(MODEL.into()), firmware: None };
    let actor = ExecutionActor::new(identity, port, Arc::new(clock.clone()), policy);
    Arc::new(MachineService::new(actor, history, CHUNK).expect("history readable"))
}

pub fn history(peer: &Peer) -> Arc<RecordingHistory> {
    Arc::new(RecordingHistory {
        peer: peer.clone(),
        records: Mutex::new(Vec::new()),
        fail: AtomicBool::new(false),
        executions: Mutex::new(Vec::new()),
        outcomes: Mutex::new(Vec::new()),
        orders: Mutex::new(StoredOrders::default()),
        approvals: Mutex::new(Vec::new()),
        parts: Mutex::new(Vec::new()),
    })
}

/// One window with a 120 × 80 rectangle and a cut operation; the machine service attached.
pub fn rig() -> Rig {
    let clock = FakeClock::new();
    let peer = Peer::new(clock.clone());
    let history = history(&peer);
    let service = service(&clock, &peer, Arc::clone(&history));
    let mut h = HeadlessEditorHarness::new();
    h.controller().attach_machine(service.clone());
    let doc = h.new_document().expect("window");
    design(&mut h, doc);
    Rig { h, doc, clock, peer, service, history }
}

/// Draws the rectangle and adds the operation in `doc`.
pub fn design(h: &mut HeadlessEditorHarness, doc: DocumentId) {
    h.send(UiIntent::SetMachineProfile { profile: Box::new(profile(1)), ruida: RUIDA });
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (10.0, 10.0), (130.0, 90.0));
    let rect = h.document(doc).expect("view").objects[0].id;
    h.send(UiIntent::Select { document: doc, ids: vec![rect], mode: SelectMode::Replace });
    h.send(UiIntent::AddOperation { document: doc, draft: cut() });
    h.invoke(doc, CommandKey::ToolSelect);
}

/// Compile → preview → "Job klaarzetten" (C1). Returns the job card.
pub fn prepare(h: &mut HeadlessEditorHarness, doc: DocumentId) -> JobCardViewModel {
    h.invoke(doc, CommandKey::Compile);
    h.run_tasks();
    h.invoke(doc, CommandKey::PrepareJob);
    h.run_tasks();
    h.document(doc).expect("view").job.expect("prepared job")
}

pub fn chunk_count(job: &JobCardViewModel) -> usize {
    job.bytes.div_ceil(CHUNK)
}

/// The controller reports `model` when asked who it is (before any job byte).
pub fn script_identity(peer: &Peer, model: &str) {
    let identity = ControllerIdentity { model: Some(model.into()), firmware: None };
    peer.script(Step::expect(Request::QueryIdentity).reply(Reply::Identity(identity)));
}

/// The controller confirms its identity and ACKs every chunk of `job`.
pub fn script_upload_acks(peer: &Peer, job: &JobCardViewModel) {
    script_identity(peer, MODEL);
    for i in 0..chunk_count(job) {
        peer.script(
            Step::expect_matching(
                format!("chunk {i}"),
                move |r| matches!(r, Request::UploadChunk { index, .. } if *index == i),
            )
            .reply(Reply::Ack),
        );
    }
}

/// Upload accepted by the controller and confirmed as stored by read-back.
pub fn upload_and_store(r: &mut Rig, job: &JobCardViewModel) {
    script_upload_acks(&r.peer, job);
    let upload = job.upload.clone().expect("upload control");
    r.h.press(Some(r.doc), &upload);
    pump(r);
    r.peer.script(Step::expect(Request::QueryStored).reply(Reply::Stored(true)));
    let confirm =
        r.h.machine_view()
            .expect("machine")
            .button(burnburn_presentation::MachineAction::ConfirmStored)
            .cloned()
            .expect("confirm control");
    r.h.press(None, &confirm);
    pump(r);
}

/// Lets the actor process every reply that is due now (bounded).
pub fn pump(r: &Rig) {
    for _ in 0..64 {
        r.service.poll().unwrap();
        if r.peer.next_reply_due().is_none_or(|due| due > burnburn_types::Clock::now(&r.clock)) {
            r.service.poll().unwrap();
            return;
        }
    }
}

/// Bytes of every upload chunk the controller received, in order.
pub fn uploaded_bytes(peer: &Peer) -> Vec<u8> {
    peer.sent()
        .into_iter()
        .filter_map(|r| match r {
            Request::UploadChunk { bytes, .. } => Some(bytes),
            _ => None,
        })
        .flatten()
        .collect()
}

pub fn starts_sent(peer: &Peer) -> usize {
    peer.sent().iter().filter(|r| **r == Request::Start).count()
}

// --- laser window (W2-L1-laser) -----------------------------------------------------------------

impl FakePort {
    pub fn new(peer: &Peer, machine: MachineId) -> FakePort {
        FakePort { peer: peer.clone(), machine, last_tag: None }
    }
}

/// A machine service over any history, as the runtime would create it once per machine.
pub fn service_with(clock: &FakeClock, peer: &Peer, history: Arc<dyn HistoryStore>) -> Arc<MachineService<FakePort>> {
    let machine = MachineId::new(MACHINE);
    let port = FakePort::new(peer, machine.clone());
    let policy = Policy { reply_timeout: TIMEOUT, max_query_attempts: 2 };
    let identity = MachineIdentity { id: machine, model: Some(MODEL.into()), firmware: None };
    let actor = ExecutionActor::new(identity, port, Arc::new(clock.clone()), policy);
    Arc::new(MachineService::new(actor, history, CHUNK).expect("history readable"))
}
