//! The GRBL laser window (G5-grbl-ui): console, macros, jog/home/unlock over a host-provided
//! serial port. The controller owns the driver and the console state; it never opens a device
//! and never starts a job (typed refusal until hardware evidence exists).

use burnburn_gcode::console::{Console, EntryKind, MacroRun, MacroSet, MacroState};
use burnburn_gcode::grbl::{
    Command, JobState, JogTarget, MachineState, SessionConfig, SessionEvent, alarm_description, error_description,
};
use burnburn_gcode::{DriverError, GrblDriver, SerialError, SerialPort};
use burnburn_job::OriginCorner;

use crate::field::{parse_length, parse_number};
use crate::grbl_panel::{
    GRBL_START_NEEDS, GrblConsoleView, GrblControl, GrblControlKind, GrblEntryKind, GrblIntent, GrblJobView,
    GrblMacroRow, GrblPort, GrblTranscriptRow, GrblViewModel, GrblWhy,
};
use crate::laser::StepDirection;

use super::{AppController, AppUpdate, HostEffect};

const DEFAULT_JOG_STEP: &str = "1";
const DEFAULT_JOG_FEED: &str = "1000";

/// Window state of the GRBL panel (app-wide, like the machine it belongs to).
pub(super) struct GrblPanel {
    driver: Option<GrblDriver<GrblPort>>,
    console: Console,
    macros: MacroSet,
    run: Option<MacroRun>,
    /// The console's input line (what was typed last, or recalled with the arrow keys).
    input: String,
    jog_step: String,
    jog_feed: String,
    last_refusal: Option<GrblWhy>,
}

impl Default for GrblPanel {
    fn default() -> Self {
        GrblPanel {
            driver: None,
            console: Console::new(),
            macros: MacroSet::new(),
            run: None,
            input: String::new(),
            jog_step: DEFAULT_JOG_STEP.into(),
            jog_feed: DEFAULT_JOG_FEED.into(),
            last_refusal: None,
        }
    }
}

impl GrblPanel {
    fn job_active(&self) -> bool {
        self.driver.as_ref().and_then(|d| d.session().job()).is_some_and(|j| j.state.is_active())
    }

    fn state(&self) -> Option<MachineState> {
        self.driver.as_ref().and_then(|d| d.session().machine_state())
    }

    /// Why a line for the controller cannot be sent right now (console, macros, moves).
    fn line_block(&self) -> Option<GrblWhy> {
        let Some(driver) = self.driver.as_ref() else { return Some(GrblWhy::NoPort) };
        if self.job_active() {
            Some(GrblWhy::JobActive)
        } else if self.run.is_some() {
            Some(GrblWhy::MacroRunning)
        } else if driver.session().bytes_in_flight() > 0 {
            Some(GrblWhy::Busy)
        } else {
            None
        }
    }

    fn transcript(&mut self, kind: EntryKind, text: impl Into<String>) {
        self.console.record(kind, text);
    }

    fn jog_numbers(&self) -> Result<(f64, f64), GrblWhy> {
        let step = parse_length(&self.jog_step).map_err(|_| GrblWhy::InvalidNumber)?.mm();
        let feed = valid_feed(&self.jog_feed).ok_or(GrblWhy::InvalidNumber)?;
        Ok((step, feed))
    }
}

fn valid_feed(text: &str) -> Option<f64> {
    parse_number(text, &["mm/min"]).ok().filter(|v| *v > 0.0 && v.is_finite())
}

fn why_of(error: DriverError) -> GrblWhy {
    match error {
        DriverError::Serial(e) => GrblWhy::Serial(e),
        DriverError::Command(r) => GrblWhy::Command(r),
        // Starting a job is not reachable from this window.
        DriverError::Start(_) => GrblWhy::NotVerified { needs: GRBL_START_NEEDS },
    }
}

fn entry(kind: EntryKind) -> GrblEntryKind {
    match kind {
        EntryKind::Sent => GrblEntryKind::Sent,
        EntryKind::Received => GrblEntryKind::Received,
        EntryKind::Info => GrblEntryKind::Info,
    }
}

impl AppController {
    /// Hands the controller the serial port of the GRBL machine (the host owns the device; tests
    /// use a fake controller). The session expects `$32=1` and the profile's `$30`.
    pub fn attach_grbl_port(&mut self, port: Box<dyn SerialPort>) {
        let max_spindle = self.grbl_profile().map_or(1000, |g| g.max_spindle);
        let driver = GrblDriver::new(GrblPort(port), SessionConfig::laser(max_spindle));
        self.grbl_panel.driver = Some(driver);
        self.grbl_panel.run = None;
    }

    /// The same with a driver the host built itself, for example one that already streams a job.
    pub fn attach_grbl_driver(&mut self, driver: GrblDriver<GrblPort>) {
        self.grbl_panel.driver = Some(driver);
        self.grbl_panel.run = None;
    }

    /// The GRBL window; `None` when the active machine is not a GRBL machine.
    pub fn grbl_view(&self) -> Option<GrblViewModel> {
        let grbl = self.grbl_profile()?;
        let p = &self.grbl_panel;
        let session = p.driver.as_ref().map(|d| d.session());
        let state = p.state();
        let line_block = p.line_block();
        let console = GrblConsoleView {
            input: p.input.clone(),
            history: p.console.history().to_vec(),
            transcript: p
                .console
                .transcript()
                .iter()
                .map(|t| GrblTranscriptRow { kind: entry(t.kind), text: t.text.clone() })
                .collect(),
            enabled: line_block.is_none(),
            why_not: line_block.clone(),
        };
        let running = p.run.as_ref().map(|r| r.name().to_owned());
        let macros = p
            .macros
            .names()
            .into_iter()
            .map(|name| GrblMacroRow {
                name: name.to_owned(),
                lines: p.macros.get(name).map(|m| m.lines().to_vec()).unwrap_or_default(),
                enabled: line_block.is_none(),
                why_not: line_block.clone(),
                running: running.as_deref().is_some_and(|r| r.eq_ignore_ascii_case(name)),
            })
            .collect();
        Some(GrblViewModel {
            connected: p.driver.is_some(),
            state,
            alarm: session.and_then(|s| s.alarm()),
            job: session.and_then(|s| s.job()).map(|j| GrblJobView {
                state: j.state,
                total: j.total,
                sent: j.sent,
                acked: j.acked,
            }),
            position_trusted: session.is_none_or(|s| s.position_trusted()),
            console,
            macros,
            controls: self.grbl_controls(state, line_block),
            jog_step_mm: p.jog_step.clone(),
            jog_feed_mm_min: p.jog_feed.clone(),
            settings_stale: session.is_some_and(|s| {
                s.config().expected_settings.iter().any(|(id, v)| *id == 30 && *v != f64::from(grbl.max_spindle))
            }),
            last_refusal: p.last_refusal.clone(),
        })
    }

    fn grbl_controls(&self, state: Option<MachineState>, line_block: Option<GrblWhy>) -> Vec<GrblControl> {
        let p = &self.grbl_panel;
        let connected = p.driver.is_some();
        let no_port = (!connected).then_some(GrblWhy::NoPort);
        let state_why = |ok: fn(MachineState) -> bool| match state {
            None => Some(GrblWhy::NoStatusYet),
            Some(s) if ok(s) => None,
            Some(s) => Some(GrblWhy::NotIdle(s)),
        };
        let control =
            |kind: GrblControlKind, why: Option<GrblWhy>| GrblControl { kind, enabled: why.is_none(), why_not: why };
        let moving = |ok: fn(MachineState) -> bool| line_block.clone().or_else(|| state_why(ok));
        let mut out = Vec::new();
        for d in [StepDirection::Left, StepDirection::Right, StepDirection::Up, StepDirection::Down] {
            out.push(control(GrblControlKind::Jog(d), moving(|s| matches!(s, MachineState::Idle | MachineState::Jog))));
        }
        out.push(control(GrblControlKind::JogCancel, no_port.clone()));
        out.push(control(GrblControlKind::Home, moving(|s| matches!(s, MachineState::Idle | MachineState::Alarm))));
        out.push(control(GrblControlKind::Unlock, moving(|s| matches!(s, MachineState::Alarm))));
        out.push(control(GrblControlKind::Hold, no_port.clone()));
        out.push(control(
            GrblControlKind::Resume,
            no_port.clone().or_else(|| state_why(|s| matches!(s, MachineState::Hold(_)))),
        ));
        out.push(control(GrblControlKind::Reset, no_port.clone()));
        out.push(control(
            GrblControlKind::StartJob,
            no_port.or(Some(GrblWhy::NotVerified { needs: GRBL_START_NEEDS })),
        ));
        out
    }

    pub(super) fn grbl_intent(&mut self, intent: GrblIntent) -> AppUpdate {
        let mut effects = Vec::new();
        // The macros of the last session load at start, before any machine is chosen.
        if self.grbl_profile().is_none() && !matches!(intent, GrblIntent::LoadMacros(_)) {
            self.grbl_panel.last_refusal = Some(GrblWhy::NotGrblMachine);
            return AppUpdate { view_changed: true, ..AppUpdate::default() };
        }
        self.grbl_panel.last_refusal = None;
        let result = self.grbl_do(intent, &mut effects);
        if let Err(why) = result {
            self.grbl_panel.last_refusal = Some(why);
        }
        AppUpdate { view_changed: true, effects, ..AppUpdate::default() }
    }

    fn grbl_do(&mut self, intent: GrblIntent, effects: &mut Vec<HostEffect>) -> Result<(), GrblWhy> {
        match intent {
            GrblIntent::Send(line) => self.grbl_send(&line),
            GrblIntent::RecallPrevious => {
                let p = &mut self.grbl_panel;
                if let Some(line) = p.console.recall_previous() {
                    p.input = line.to_owned();
                }
                Ok(())
            }
            GrblIntent::RecallNext => {
                let p = &mut self.grbl_panel;
                p.input = p.console.recall_next().map(str::to_owned).unwrap_or_default();
                Ok(())
            }
            GrblIntent::ClearTranscript => {
                self.grbl_panel.console.clear_transcript();
                Ok(())
            }
            GrblIntent::RunMacro(name) => self.grbl_run_macro(&name),
            GrblIntent::AddMacro { name, program } => {
                self.grbl_panel.macros.add(&name, &program).map_err(GrblWhy::Macro)?;
                self.macros_changed(effects);
                Ok(())
            }
            GrblIntent::ReplaceMacro { name, program } => {
                self.grbl_panel.macros.replace(&name, &program).map_err(GrblWhy::Macro)?;
                self.macros_changed(effects);
                Ok(())
            }
            GrblIntent::RenameMacro { from, to } => {
                self.grbl_panel.macros.rename(&from, &to).map_err(GrblWhy::Macro)?;
                self.macros_changed(effects);
                Ok(())
            }
            GrblIntent::RemoveMacro(name) => {
                self.grbl_panel.macros.remove(&name).map_err(GrblWhy::Macro)?;
                self.macros_changed(effects);
                Ok(())
            }
            GrblIntent::LoadMacros(text) => {
                self.grbl_panel.macros = MacroSet::parse_text(&text).map_err(GrblWhy::Macro)?;
                self.macros_changed(effects);
                Ok(())
            }
            GrblIntent::SetJogStep(text) => {
                parse_length(&text).map_err(|_| GrblWhy::InvalidNumber)?;
                self.grbl_panel.jog_step = text.trim().to_owned();
                Ok(())
            }
            GrblIntent::SetJogFeed(text) => {
                valid_feed(&text).ok_or(GrblWhy::InvalidNumber)?;
                self.grbl_panel.jog_feed = text.trim().to_owned();
                Ok(())
            }
            GrblIntent::Jog(direction) => self.grbl_jog(direction),
            GrblIntent::Home => self.grbl_command(Command::Home, Some("$H")),
            GrblIntent::Unlock => self.grbl_command(Command::Unlock, Some("$X")),
            GrblIntent::Hold => self.grbl_realtime(|d| d.hold()),
            GrblIntent::Resume => self.grbl_realtime(|d| d.resume()),
            GrblIntent::JogCancel => self.grbl_realtime(|d| d.jog_cancel()),
            GrblIntent::Reset => {
                self.grbl_realtime(|d| d.cancel())?;
                if let Some(run) = self.grbl_panel.run.take() {
                    self.grbl_panel.transcript(EntryKind::Info, format!("macro {}: stopped by a reset", run.name()));
                }
                Ok(())
            }
            GrblIntent::StartJob => Err(GrblWhy::NotVerified { needs: GRBL_START_NEEDS }),
            GrblIntent::Tick => self.grbl_tick(),
        }
    }

    fn macros_changed(&mut self, effects: &mut Vec<HostEffect>) {
        effects.push(HostEffect::GrblMacrosChanged { text: self.grbl_panel.macros.to_text() });
    }

    fn grbl_send(&mut self, line: &str) -> Result<(), GrblWhy> {
        let p = &mut self.grbl_panel;
        if p.driver.is_none() {
            return Err(GrblWhy::NoPort);
        }
        if p.run.is_some() {
            return Err(GrblWhy::MacroRunning);
        }
        let job_active = p.job_active();
        let command = p.console.submit(line, job_active).map_err(GrblWhy::Console)?;
        let Command::Line(text) = &command else { return Ok(()) };
        let text = text.clone();
        let sent = self.grbl_command(command, None);
        let p = &mut self.grbl_panel;
        if sent.is_ok() {
            p.transcript(EntryKind::Sent, text);
            p.input.clear();
        }
        sent
    }

    /// Sends one command; `shown` is what the transcript shows as sent (`None`: the caller records it).
    fn grbl_command(&mut self, command: Command, shown: Option<&str>) -> Result<(), GrblWhy> {
        let p = &mut self.grbl_panel;
        if let Some(why) = p.line_block() {
            // A macro's own lines go through `grbl_macro_line`, never through here.
            return Err(why);
        }
        let Some(driver) = p.driver.as_mut() else { return Err(GrblWhy::NoPort) };
        let events = driver.command(command).map_err(why_of)?;
        if let Some(text) = shown {
            p.transcript(EntryKind::Sent, text);
        }
        self.grbl_events(&events);
        Ok(())
    }

    fn grbl_jog(&mut self, direction: StepDirection) -> Result<(), GrblWhy> {
        let (step, feed) = self.grbl_panel.jog_numbers()?;
        // The frame's origin corner decides which way the axes count: with the origin at the top
        // "up" is toward smaller Y, with the origin at the right "right" is toward smaller X.
        let origin = self.machine.as_ref().map(|m| m.profile.flatbed_frame().origin());
        let x_sign =
            if matches!(origin, Some(OriginCorner::TopRight | OriginCorner::BottomRight)) { -1.0 } else { 1.0 };
        let y_sign = if matches!(origin, Some(OriginCorner::TopLeft | OriginCorner::TopRight)) { -1.0 } else { 1.0 };
        let (dx_mm, dy_mm) = match direction {
            StepDirection::Left => (-step * x_sign, 0.0),
            StepDirection::Right => (step * x_sign, 0.0),
            StepDirection::Up => (0.0, step * y_sign),
            StepDirection::Down => (0.0, -step * y_sign),
        };
        let command = Command::Jog { target: JogTarget::Relative { dx_mm, dy_mm }, feed_mm_min: feed };
        self.grbl_command(command, None)?;
        self.grbl_panel.transcript(EntryKind::Info, format!("jog X{dx_mm} Y{dy_mm} F{feed}"));
        Ok(())
    }

    fn grbl_realtime(
        &mut self,
        f: impl FnOnce(&mut GrblDriver<GrblPort>) -> Result<Vec<SessionEvent>, DriverError>,
    ) -> Result<(), GrblWhy> {
        let Some(driver) = self.grbl_panel.driver.as_mut() else { return Err(GrblWhy::NoPort) };
        let events = f(driver).map_err(why_of)?;
        self.grbl_events(&events);
        Ok(())
    }

    fn grbl_run_macro(&mut self, name: &str) -> Result<(), GrblWhy> {
        let p = &mut self.grbl_panel;
        if p.driver.is_none() {
            return Err(GrblWhy::NoPort);
        }
        if p.run.is_some() {
            return Err(GrblWhy::MacroRunning);
        }
        let job_active = p.job_active();
        let run = p.macros.run(name, job_active).map_err(GrblWhy::Macro)?;
        if let Some(why) = p.line_block() {
            return Err(why);
        }
        p.transcript(EntryKind::Info, format!("macro {}", run.name()));
        p.run = Some(run);
        self.grbl_macro_step()
    }

    /// Sends the next line of the running macro, if the previous one was answered.
    fn grbl_macro_step(&mut self) -> Result<(), GrblWhy> {
        let p = &mut self.grbl_panel;
        let Some(run) = p.run.as_mut() else { return Ok(()) };
        match run.state() {
            MacroState::Done => {
                let name = run.name().to_owned();
                p.run = None;
                p.transcript(EntryKind::Info, format!("macro {name}: done"));
                return Ok(());
            }
            MacroState::Failed { line, code } => {
                let (name, line, code) = (run.name().to_owned(), *line, *code);
                p.run = None;
                p.transcript(EntryKind::Info, format!("macro {name}: error:{code} at line {}, stopped", line + 1));
                return Ok(());
            }
            _ => {}
        }
        let Some(Command::Line(text)) = run.next_command() else { return Ok(()) };
        let name = run.name().to_owned();
        let Some(driver) = p.driver.as_mut() else { return Err(GrblWhy::NoPort) };
        match driver.command(Command::Line(text.clone())) {
            Ok(events) => {
                p.transcript(EntryKind::Sent, text);
                self.grbl_events(&events);
                Ok(())
            }
            Err(e) => {
                p.run = None;
                p.transcript(EntryKind::Info, format!("macro {name}: not sent, stopped"));
                Err(why_of(e))
            }
        }
    }

    /// The host's timer: ask for the status, read what the controller said, advance a macro.
    fn grbl_tick(&mut self) -> Result<(), GrblWhy> {
        let Some(driver) = self.grbl_panel.driver.as_mut() else { return Err(GrblWhy::NoPort) };
        let mut events = driver.poll_status().map_err(why_of)?;
        match driver.pump() {
            Ok(mut more) => events.append(&mut more),
            Err(e) => {
                self.grbl_panel.run = None;
                return Err(GrblWhy::Serial(e));
            }
        }
        self.grbl_events(&events);
        self.grbl_macro_step()
    }

    /// Records what the controller said and lets a running macro see the answers.
    fn grbl_events(&mut self, events: &[SessionEvent]) {
        let p = &mut self.grbl_panel;
        for event in events {
            if let Some(run) = p.run.as_mut() {
                run.on_event(event);
            }
            match event {
                SessionEvent::CommandAccepted => p.transcript(EntryKind::Received, "ok"),
                SessionEvent::CommandError { code } => {
                    let text = match error_description(*code) {
                        Some(d) => format!("error:{code}: {d}"),
                        None => format!("error:{code}"),
                    };
                    p.transcript(EntryKind::Received, text);
                }
                SessionEvent::Alarm(code) => {
                    let text = match alarm_description(*code) {
                        Some(d) => format!("ALARM:{code}: {d}"),
                        None => format!("ALARM:{code}"),
                    };
                    p.transcript(EntryKind::Received, text);
                }
                SessionEvent::Feedback(text) | SessionEvent::Unexpected(text) => {
                    p.transcript(EntryKind::Received, text.clone());
                }
                SessionEvent::SettingObserved { id, value } => {
                    p.transcript(EntryKind::Received, format!("${id}={value}"));
                }
                SessionEvent::JobState(state) if !matches!(state, JobState::Streaming) => {
                    p.transcript(EntryKind::Info, format!("job: {state:?}"));
                }
                _ => {}
            }
        }
    }
}

impl From<SerialError> for GrblWhy {
    fn from(e: SerialError) -> Self {
        GrblWhy::Serial(e)
    }
}
