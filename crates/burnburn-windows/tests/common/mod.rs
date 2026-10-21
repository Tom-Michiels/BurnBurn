//! Shared rigs of the W4-L8b tests: a machine service over a fake port (no laser, no network), a
//! fake camera and small helpers to build [`PanelEvent`]s the way the Win32 layer reports them.

#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::{Arc, Mutex};

use burnburn_job::{MachineFrame, MachineProfile, OriginCorner, PowerRatio};
use burnburn_machine::{
    Capabilities, Capability, ControllerPort, Envelope, ExecutionActor, ExecutionId, Incoming, MachineId,
    MachineIdentity, Policy, PortError, Request,
};
use burnburn_presentation::{
    HeadlessEditorHarness, HistoryStore, HistoryStoreError, IntentRecord, LaserViewModel, LiveIntentRecord,
    MachineService, OutcomeRecord, Recorded, UiIntent, UploadRecord,
};
use burnburn_ruida::{Magic, OverscanPolicy, RuidaProfile};
use burnburn_types::{Clock, DocumentId, MonotonicTime};
use burnburn_windows::host_panels::{PanelEvent, PanelValues, notify};

pub struct Port(pub Arc<Mutex<Vec<Request>>>);

impl ControllerPort for Port {
    fn send(&mut self, e: Envelope) -> Result<(), PortError> {
        self.0.lock().unwrap().push(e.request);
        Ok(())
    }
    fn try_recv(&mut self) -> Result<Option<Incoming>, PortError> {
        Ok(None)
    }
    fn reconnect(&mut self) -> Result<(), PortError> {
        Ok(())
    }
}

pub struct ZeroClock;
impl Clock for ZeroClock {
    fn now(&self) -> MonotonicTime {
        MonotonicTime::ZERO
    }
}

pub struct Journal;
impl HistoryStore for Journal {
    fn record_upload(&self, _: &UploadRecord) -> Result<(ExecutionId, Recorded), HistoryStoreError> {
        Err(HistoryStoreError("not used".into()))
    }
    fn record_intent(&self, _: &IntentRecord) -> Result<Recorded, HistoryStoreError> {
        Ok(Recorded::Fresh)
    }
    fn record_outcome(&self, _: &OutcomeRecord) -> Result<Recorded, HistoryStoreError> {
        Ok(Recorded::Fresh)
    }
    fn record_live_intent(&self, _: &LiveIntentRecord) -> Result<Recorded, HistoryStoreError> {
        Ok(Recorded::Fresh)
    }
}

pub struct Rig {
    pub h: HeadlessEditorHarness,
    pub doc: DocumentId,
    pub service: Arc<MachineService<Port>>,
    pub sent: Arc<Mutex<Vec<Request>>>,
}

/// A harness with a machine service over a fake port. `supported` marks jog, home and absolute
/// move as proven; without it every live control is off with its refusal in words.
pub fn rig(supported: bool) -> Rig {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let machine = MachineId::new("bench");
    let identity = MachineIdentity { id: machine, model: Some("RDC6445G".into()), firmware: None };
    let actor = ExecutionActor::new(identity, Port(sent.clone()), Arc::new(ZeroClock), Policy::default());
    let service = Arc::new(MachineService::new(actor, Arc::new(Journal), 64).unwrap());
    if supported {
        let mut c = Capabilities::default();
        c.live.jog = Capability::Supported;
        c.live.home = Capability::Supported;
        c.live.move_absolute = Capability::Supported;
        service.set_capabilities(c);
    }
    let mut h = HeadlessEditorHarness::new();
    h.controller().attach_machine(service.clone());
    let profile = MachineProfile::new(
        "bench-1500x900",
        1,
        MachineFrame::new(1500.0, 900.0, OriginCorner::BottomLeft).unwrap(),
        1000.0,
        400.0,
        PowerRatio::new(0.0).unwrap(),
        PowerRatio::new(1.0).unwrap(),
    )
    .unwrap();
    let ruida = RuidaProfile { magic: Magic::FIXTURE_0X88, overscan: OverscanPolicy::ControllerManaged };
    h.send(UiIntent::SetMachineProfile { profile: Box::new(profile), ruida });
    let doc = h.new_document().unwrap();
    Rig { h, doc, service, sent }
}

pub fn laser_view(r: &mut Rig) -> LaserViewModel {
    r.h.controller().laser_view().expect("laser view")
}

/// A click on control `id`, with the values the panel holds.
pub fn click(id: u16, values: PanelValues) -> PanelEvent {
    PanelEvent { id, code: notify::CLICKED, values, click_px: None, view_px: None }
}

/// A notification other than a click.
pub fn event(id: u16, code: u16, values: PanelValues) -> PanelEvent {
    PanelEvent { id, code, values, click_px: None, view_px: None }
}

pub fn texts(pairs: &[(u16, &str)]) -> PanelValues {
    let mut v = PanelValues::default();
    for (id, text) in pairs {
        v.texts.insert(*id, (*text).to_owned());
    }
    v
}
