//! The laser window (W2-L1-laser): setup, controls with their enablement, and the actions that go
//! to the machine service. Never touches a document: the setup is window state (like the machine
//! panel), and an action is a machine intent that is journaled before it is sent.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use burnburn_io::Project;
use burnburn_machine::{
    ControllerFileName, ExecutionState, FrameOptions, FrameShape, JobAnchor, JogKey, LaserAction, LiveFeature,
    PlacementSpec, Rect, ScopeObject, StartFrom, UserOrigin, frame_plan_shaped, resolve_scope,
};
use burnburn_types::{ContentHash, DocumentId, HashDomain, StableId};

use crate::field::parse_number;
use crate::intent::ViewPointerEvent;
use crate::laser::{
    ControllerFileRow, LaserCommand, LaserCompile, LaserControl, LaserControlKind, LaserPosition, LaserSetup,
    LaserSetupView, LaserViewModel, LaserWhy, PlacementView, StepDirection,
};
use crate::production::{
    ActionId, ActionSubject, Dispatched, JobInputs, LaserCall, LaserIntent, LiveKind, MachineAction, MachineError,
    MachineIntent, MachineStatus, PreparedJob,
};
use burnburn_editor::PointerPhase;

use super::process::preparable;
use crate::task::TaskInput;
use crate::view_model::MessageKey;

use super::{AppController, AppUpdate, Session, error};

/// Default step of the jog buttons and speed of a frame run.
const DEFAULT_STEP_MM: f64 = 1.0;
const DEFAULT_FRAME_SPEED_MM_S: f64 = 50.0;
/// Largest typed step / frame speed; anything above is a typing mistake, not a wish.
const MAX_STEP_MM: f64 = 1000.0;

/// The laser window's setup: window state, never document data or undo.
pub(super) struct LaserState {
    pub(super) start_from: StartFrom,
    pub(super) anchor: JobAnchor,
    pub(super) cut_selected: bool,
    pub(super) use_selection_origin: bool,
    pub(super) user_origin: Option<UserOrigin>,
    pub(super) step_mm: f64,
    pub(super) frame_speed_mm_s: f64,
    pub(super) position_laser: bool,
    pub(super) last_refusal: Option<LaserWhy>,
}

impl Default for LaserState {
    fn default() -> Self {
        LaserState {
            start_from: StartFrom::AbsoluteCoordinates,
            anchor: JobAnchor::BottomLeft,
            cut_selected: false,
            use_selection_origin: false,
            user_origin: None,
            step_mm: DEFAULT_STEP_MM,
            frame_speed_mm_s: DEFAULT_FRAME_SPEED_MM_S,
            position_laser: false,
            last_refusal: None,
        }
    }
}

impl LaserState {
    /// Absolute coordinates and everything is cut: the plain compile.
    fn is_plain(&self) -> bool {
        self.start_from == StartFrom::AbsoluteCoordinates && !self.cut_selected
    }
}

fn changed() -> AppUpdate {
    AppUpdate { view_changed: true, ..AppUpdate::default() }
}

fn um(mm: f64) -> Option<i64> {
    let um = (mm * 1000.0).round();
    (um.is_finite() && um.abs() < 1.0e12).then_some(um as i64)
}

fn mm_of(um: i64) -> f64 {
    um as f64 / 1000.0
}

fn why_of(e: &MachineError) -> LaserWhy {
    match e {
        MachineError::Live(e) => LaserWhy::from(e),
        MachineError::Laser(r) => LaserWhy::from(r),
        MachineError::NotRecorded(_) => LaserWhy::NotRecorded,
        MachineError::NotAllowed => LaserWhy::NotAllowed,
        MachineError::StateChanged { .. } | MachineError::Actor(_) => LaserWhy::JobMayBeActive,
        _ => LaserWhy::Refused,
    }
}

impl AppController {
    /// The laser window of the active document; `None` without a machine service.
    pub fn laser_view(&self) -> Option<LaserViewModel> {
        self.laser_view_for(self.active)
    }

    pub fn laser_view_for(&self, document: Option<DocumentId>) -> Option<LaserViewModel> {
        let port = self.machine_port.as_ref()?;
        let status = port.status();
        let live = &status.live;
        let session = document.and_then(|d| self.sessions.get(&d));
        let mut controls = Vec::new();
        let action = |kind: LiveKind| Some(status.action_id(MachineAction::Live(kind), ActionSubject::Machine));
        let gate = |feature: LiveFeature| live.gate(feature).map(LaserWhy::from);
        let position_known = live.position.machine_point().is_some();
        let mut add = |kind: LaserControlKind, action: Option<ActionId>, why_not: Option<LaserWhy>| {
            controls.push(LaserControl { kind, action, enabled: why_not.is_none(), why_not });
        };
        for key in [JogKey::XLeft, JogKey::XRight, JogKey::YTop, JogKey::YBottom] {
            add(LaserControlKind::Jog(key), action(LiveKind::JogPress), gate(LiveFeature::Jog));
        }
        let step_why = gate(LiveFeature::MoveAbsolute).or((!position_known).then_some(LaserWhy::PositionUnknown));
        for d in [StepDirection::Left, StepDirection::Right, StepDirection::Up, StepDirection::Down] {
            add(LaserControlKind::Step(d), action(LiveKind::JogStep), step_why);
        }
        add(LaserControlKind::MoveTo, action(LiveKind::MoveTo), gate(LiveFeature::MoveAbsolute));
        add(LaserControlKind::PositionLaser, None, gate(LiveFeature::MoveAbsolute));
        add(LaserControlKind::Home, action(LiveKind::Home), gate(LiveFeature::Home));
        add(LaserControlKind::GoToMachineOrigin, action(LiveKind::GoToOrigin), gate(LiveFeature::MoveAbsolute));
        add(
            LaserControlKind::GoToUserOrigin,
            action(LiveKind::GoToOrigin),
            gate(LiveFeature::MoveAbsolute).or(self.laser.user_origin.is_none().then_some(LaserWhy::UserOriginNotSet)),
        );
        add(LaserControlKind::SetOrigin, None, (!position_known).then_some(LaserWhy::PositionUnknown));
        add(LaserControlKind::Pause, action(LiveKind::Pause), gate(LiveFeature::PauseResume));
        add(LaserControlKind::Resume, action(LiveKind::Resume), gate(LiveFeature::PauseResume));
        add(LaserControlKind::ShowPosition, action(LiveKind::ShowPosition), gate(LiveFeature::ReadPosition));
        let busy = status.current.as_ref().is_some_and(|c| !c.execution.state.is_terminal());
        let frame_why = if self.machine.is_none() {
            Some(LaserWhy::NoMachineProfile)
        } else if busy {
            Some(LaserWhy::JobMayBeActive)
        } else {
            session
                .and_then(|s| self.reviewed_plan_why(document, s))
                .or_else(|| session.is_none().then_some(LaserWhy::NoReviewedPlan))
        };
        let frame_action = Some(status.action_id(MachineAction::Frame, ActionSubject::Machine));
        for shape in [FrameShape::BoundingRectangle, FrameShape::RubberBand] {
            add(LaserControlKind::Frame(shape), frame_action, frame_why);
        }
        let has_job = session.is_some_and(|s| s.prepared.is_some());
        add(LaserControlKind::SaveRd, None, (!has_job).then_some(LaserWhy::NoReviewedPlan));
        add(LaserControlKind::ListFiles, action(LiveKind::ListFiles), gate(LiveFeature::FileList));
        let files: Option<Vec<ControllerFileRow>> = live.files.as_ref().map(|files| {
            files.iter().map(|f| ControllerFileRow { name: f.name.clone(), size_bytes: f.size_bytes }).collect()
        });
        for row in files.iter().flatten() {
            add(
                LaserControlKind::FileMetadata(row.name.clone()),
                action(LiveKind::FileMetadata),
                gate(LiveFeature::FileMetadata),
            );
            add(LaserControlKind::RunFile(row.name.clone()), action(LiveKind::RunFile), gate(LiveFeature::FileRun));
            add(
                LaserControlKind::DeleteFile(row.name.clone()),
                action(LiveKind::DeleteFile),
                gate(LiveFeature::FileDelete),
            );
        }
        let l = &self.laser;
        Some(LaserViewModel {
            position: LaserPosition { x_mm: live.position.x.mm(), y_mm: live.position.y.mm() },
            jog_held: live.jog_held,
            jog_release: live.jog_held.zip(action(LiveKind::JogRelease)),
            setup: LaserSetupView {
                start_from: l.start_from,
                anchor: l.anchor,
                cut_selected: l.cut_selected,
                use_selection_origin: l.use_selection_origin,
                user_origin_mm: l.user_origin.map(|o| (mm_of(o.x_um()), mm_of(o.y_um()))),
                step_mm: l.step_mm,
                frame_speed_mm_s: l.frame_speed_mm_s,
                position_laser_mode: l.position_laser,
            },
            controls,
            files,
            placement: session.and_then(|s| self.placement_view(document, s, &status)),
            last_refusal: l.last_refusal,
            seen: status.current.as_ref().map(|c| c.execution.state.clone()),
        })
    }

    fn placement_view(
        &self,
        document: Option<DocumentId>,
        s: &Session,
        status: &MachineStatus,
    ) -> Option<PlacementView> {
        let c = s.compiled.as_ref()?;
        let p = c.placement.as_ref()?;
        let _ = status;
        Some(PlacementView {
            start_from: p.spec.start_from,
            anchor: p.spec.anchor,
            delta_mm: p.delta_mm,
            from_position_mm: p.from_position_um.map(|(x, y)| (mm_of(x), mm_of(y))),
            setup_changed: document.is_some_and(|d| self.placement_stale(d)),
        })
    }

    /// Why the reviewed plan of the window cannot be framed or sent now (`None`: it can).
    fn reviewed_plan_why(&self, document: Option<DocumentId>, s: &Session) -> Option<LaserWhy> {
        if !preparable(s, self.machine.as_ref()) {
            return Some(LaserWhy::NoReviewedPlan);
        }
        document.filter(|d| self.placement_stale(*d)).map(|_| LaserWhy::PlacementStale)
    }

    /// Identifies what the next compile would use; 0 for the plain compile. A plan compiled with
    /// another key is stale for the window.
    pub(super) fn laser_setup_key(&self, document: DocumentId) -> u64 {
        let l = &self.laser;
        if l.is_plain() {
            return 0;
        }
        let mut h = DefaultHasher::new();
        (l.start_from, l.anchor, l.cut_selected, l.use_selection_origin).hash(&mut h);
        l.user_origin.map(|o| (o.x_um(), o.y_um())).hash(&mut h);
        if (l.cut_selected || l.use_selection_origin)
            && let Some(s) = self.sessions.get(&document)
        {
            s.selection.ids().hash(&mut h);
        }
        h.finish().max(1)
    }

    /// The window's plan was compiled with another setup, or on a head position that has moved.
    pub(super) fn placement_stale(&self, document: DocumentId) -> bool {
        let Some(c) = self.sessions.get(&document).and_then(|s| s.compiled.as_ref()) else { return false };
        if c.setup_key != self.laser_setup_key(document) {
            return true;
        }
        let Some((x, y)) = c.placement.as_ref().and_then(|p| p.from_position_um) else { return false };
        let now = self.machine_port.as_ref().map(|p| p.status().live.position);
        now.is_none_or(|p| p.x.um() != Some(x) || p.y.um() != Some(y))
    }

    /// Compile input of the setup, `Ok(None)` for the plain compile.
    pub(super) fn laser_compile_input(&self, document: DocumentId) -> Result<Option<(LaserCompile, u64)>, LaserWhy> {
        let l = &self.laser;
        if l.is_plain() {
            return Ok(None);
        }
        let machine = self.machine.as_ref().ok_or(LaserWhy::NoMachineProfile)?;
        let s = self.sessions.get(&document).ok_or(LaserWhy::Refused)?;
        let snapshot = s.doc.snapshot();
        let selected: Vec<u64> = s.selection.ids().iter().map(|g| g.raw()).collect();
        // Everything that burns, plus the selection when it decides the scope or the origin.
        let mut ids: Vec<burnburn_types::GeometryId> = snapshot
            .process()
            .operations()
            .filter(|o| o.output_enabled())
            .flat_map(|o| o.targets().iter().copied())
            .collect();
        if l.cut_selected || l.use_selection_origin {
            ids.extend(s.selection.ids().iter().copied());
        }
        ids.sort();
        ids.dedup();
        let objects: Vec<ScopeObject> = ids
            .iter()
            .filter_map(|id| {
                let b = snapshot.object(*id)?.bounds()?;
                let rect = Rect::new(b.min().x, b.min().y, b.max().x, b.max().y)?;
                Some(ScopeObject { id: id.raw(), bounds: rect })
            })
            .collect();
        let scope = burnburn_machine::CompileScope {
            cut_selected: l.cut_selected,
            use_selection_origin: l.use_selection_origin,
        };
        let resolution = resolve_scope(&scope, &objects, &selected).map_err(|e| LaserWhy::from(&e))?;
        let included = l
            .cut_selected
            .then(|| ids.iter().copied().filter(|g| resolution.included.contains(&g.raw())).collect::<Vec<_>>());
        let position = self.machine_port.as_ref().map(|p| p.status().live.position);
        let current_position_um = position.and_then(|p| p.x.um().zip(p.y.um()));
        // Refused here, before any task: an unknown position is never taken as (0, 0).
        match l.start_from {
            StartFrom::CurrentPosition if current_position_um.is_none() => return Err(LaserWhy::PositionUnknown),
            StartFrom::UserOrigin if l.user_origin.is_none() => return Err(LaserWhy::UserOriginNotSet),
            _ => {}
        }
        let compile = LaserCompile {
            included,
            origin_bounds: resolution.origin_bounds,
            spec: PlacementSpec { start_from: l.start_from, anchor: l.anchor },
            frame: *machine.profile.frame(),
            current_position_um,
            user_origin_um: l.user_origin.map(|o| (o.x_um(), o.y_um())),
        };
        Ok(Some((compile, self.laser_setup_key(document))))
    }

    /// Help ▸ Diagnose exporteren: the journal replayed as text (no machine contact), written by a
    /// task so the UI thread never touches the disk.
    pub(super) fn export_diagnostics(
        &mut self,
        options: crate::production::DiagnosticOptions,
        path: std::path::PathBuf,
    ) -> AppUpdate {
        let Some(history) = self.history.clone() else { return error(None, MessageKey::DiagnosticsFailed) };
        match history.diagnostic_report(&options) {
            Ok(text) => {
                self.app_task(TaskInput::WriteText { text: Arc::from(text), path, fs: Arc::clone(&self.ports.fs) })
            }
            Err(_) => error(None, MessageKey::DiagnosticsFailed),
        }
    }

    /// A laser-window setup change. Compile-relevant changes make the compiled plan stale (by key).
    pub(super) fn laser_setup(&mut self, setup: LaserSetup) -> AppUpdate {
        let mut refused = None;
        match setup {
            LaserSetup::StartFrom(v) => self.laser.start_from = v,
            LaserSetup::Anchor(v) => self.laser.anchor = v,
            LaserSetup::CutSelected(v) => self.laser.cut_selected = v,
            LaserSetup::UseSelectionOrigin(v) => self.laser.use_selection_origin = v,
            LaserSetup::StepMm(text) => match parse_number(&text, &["mm"]) {
                Ok(v) if v > 0.0 && v <= MAX_STEP_MM => self.laser.step_mm = v,
                _ => refused = Some(LaserWhy::InvalidNumber),
            },
            LaserSetup::FrameSpeed(text) => match parse_number(&text, &["mm/s"]) {
                Ok(v) if v > 0.0 && v <= MAX_STEP_MM => self.laser.frame_speed_mm_s = v,
                _ => refused = Some(LaserWhy::InvalidNumber),
            },
            LaserSetup::PositionLaserMode(on) => self.laser.position_laser = on,
            LaserSetup::SetOriginHere => {
                let position = self.machine_port.as_ref().map(|p| p.status().live.position);
                match position.map(|p| UserOrigin::from_position(&p)) {
                    Some(Ok(origin)) => self.laser.user_origin = Some(origin),
                    _ => refused = Some(LaserWhy::PositionUnknown),
                }
            }
            LaserSetup::ClearOrigin => self.laser.user_origin = None,
        }
        self.laser.last_refusal = refused;
        changed()
    }

    /// The key-up path of a lost focus, a closed window or a hidden panel: releases the held key.
    pub(super) fn laser_release_jog(&mut self) -> AppUpdate {
        let Some(port) = self.machine_port.clone() else { return AppUpdate::default() };
        let status = port.status();
        let Some(key) = status.live.jog_held else { return AppUpdate::default() };
        let action = status.action_id(MachineAction::Live(LiveKind::JogRelease), ActionSubject::Machine);
        self.laser_call(None, Some(action), None, LaserCommand::Action(LaserAction::JogRelease(key)))
    }

    /// A canvas click in "Position Laser" mode. Consumes every pointer event while the mode is on,
    /// so no click selects or draws.
    pub(super) fn laser_pointer(&mut self, document: DocumentId, event: &ViewPointerEvent) -> Option<AppUpdate> {
        if !self.laser.position_laser || self.machine_port.is_none() {
            return None;
        }
        let s = self.sessions.get(&document)?;
        if event.phase != PointerPhase::Down {
            return Some(AppUpdate::default());
        }
        let world = s.camera.points_to_world(&s.viewport, event.position_pt);
        let status = self.machine_port.as_ref()?.status();
        let action = status.action_id(MachineAction::Live(LiveKind::PositionLaser), ActionSubject::Machine);
        let command = LaserCommand::Action(LaserAction::PositionLaser { bed_x_mm: world.x, bed_y_mm: world.y });
        Some(self.laser_call(Some(document), Some(action), None, command))
    }

    /// One laser-window control.
    pub(super) fn laser_call(
        &mut self,
        document: Option<DocumentId>,
        action: Option<ActionId>,
        seen: Option<ExecutionState>,
        command: LaserCommand,
    ) -> AppUpdate {
        let result = self.laser_dispatch(document, action, seen, command);
        match result {
            Ok(()) => self.laser.last_refusal = None,
            Err(why) => self.laser.last_refusal = Some(why),
        }
        changed()
    }

    fn laser_dispatch(
        &mut self,
        document: Option<DocumentId>,
        action: Option<ActionId>,
        seen: Option<ExecutionState>,
        command: LaserCommand,
    ) -> Result<(), LaserWhy> {
        let port = self.machine_port.clone().ok_or(LaserWhy::NoMachine)?;
        let machine = self.machine.clone().ok_or(LaserWhy::NoMachineProfile)?;
        let status = port.status();
        let frame = *machine.profile.frame();
        // Only "save RD" needs no idempotency key: it is never sent or counted.
        let action = match (&command, action) {
            (_, Some(a)) => a,
            (LaserCommand::SaveRd { .. }, None) => {
                status.action_id(MachineAction::Live(LiveKind::ListFiles), ActionSubject::Machine)
            }
            _ => return Err(LaserWhy::Refused),
        };
        let intent = |call: LaserCall| LaserIntent {
            action,
            machine: status.machine.clone(),
            call,
            frame,
            user_origin: self.laser.user_origin,
        };
        let sent = match command {
            LaserCommand::Action(a) => port.laser(intent(LaserCall::Action(a))),
            LaserCommand::MoveToText { x, y } => {
                let coordinate = |t: &str| parse_number(t, &["mm"]).ok().and_then(um).ok_or(LaserWhy::InvalidNumber);
                let (x_um, y_um) = (coordinate(&x)?, coordinate(&y)?);
                port.laser(intent(LaserCall::Action(LaserAction::MoveTo { x_um, y_um })))
            }
            LaserCommand::Step(direction) => {
                let step = um(self.laser.step_mm).ok_or(LaserWhy::InvalidNumber)?;
                let (dx_view_um, dy_view_um) = match direction {
                    StepDirection::Left => (-step, 0),
                    StepDirection::Right => (step, 0),
                    StepDirection::Up => (0, step),
                    StepDirection::Down => (0, -step),
                };
                port.laser(intent(LaserCall::Action(LaserAction::JogStep { dx_view_um, dy_view_um })))
            }
            LaserCommand::Frame(shape) => {
                let document = document.or(self.active).ok_or(LaserWhy::NoReviewedPlan)?;
                let job = self.frame_job(document, shape)?;
                let machine_intent = MachineIntent {
                    action,
                    machine: status.machine.clone(),
                    job: Some(Arc::new(job)),
                    required_state: seen,
                    order: None,
                    role: self.role,
                };
                port.dispatch(machine_intent)
            }
            LaserCommand::SaveRd { name } => {
                let document = document.or(self.active).ok_or(LaserWhy::NoReviewedPlan)?;
                let name = ControllerFileName::new(&name).map_err(|_| LaserWhy::InvalidFileName)?;
                let job = self
                    .sessions
                    .get(&document)
                    .and_then(|s| s.prepared.as_ref())
                    .map(|p| Arc::clone(&p.job))
                    .ok_or(LaserWhy::NoReviewedPlan)?;
                port.laser(intent(LaserCall::SaveRd { name, job }))
            }
        };
        match sent {
            Ok(Dispatched::Sent(_)) => Ok(()),
            Ok(Dispatched::AlreadyHandled(done)) => done.result.map(|_| ()).map_err(|e| why_of(&e)),
            Err(e) => Err(why_of(&e)),
        }
    }

    /// The laser-off frame job of the reviewed plan (which is already placed by the setup).
    fn frame_job(&self, document: DocumentId, shape: FrameShape) -> Result<PreparedJob, LaserWhy> {
        let machine = self.machine.as_ref().ok_or(LaserWhy::NoMachineProfile)?;
        let s = self.sessions.get(&document).ok_or(LaserWhy::NoReviewedPlan)?;
        if let Some(why) = self.reviewed_plan_why(Some(document), s) {
            return Err(why);
        }
        let compiled = s.compiled.as_ref().ok_or(LaserWhy::NoReviewedPlan)?;
        let speed = self.laser.frame_speed_mm_s.min(machine.profile.max_speed_mm_s());
        let frame = frame_plan_shaped(&compiled.plan, &FrameOptions { speed_mm_s: speed }, shape)
            .map_err(|_| LaserWhy::NoReviewedPlan)?;
        let rd = frame.rd_file(&machine.ruida).map_err(|_| LaserWhy::Refused)?;
        let mut project = Project::new(s.uuid, (*s.doc.snapshot()).clone());
        project.assets = s.assets.clone();
        let source = compiled.plan.source();
        Ok(PreparedJob {
            plan: frame.plan().id(),
            plan_revision: source.document_revision,
            profile: source.machine_profile.clone(),
            rd_hash: ContentHash::of(HashDomain::Rd, &rd),
            rd: Arc::from(rd),
            name: s.title.clone(),
            inputs: Some(Arc::new(JobInputs {
                plan: Arc::new(frame.plan().clone()),
                project: Arc::new(project),
                project_path: s.path.clone(),
            })),
        })
    }
}
