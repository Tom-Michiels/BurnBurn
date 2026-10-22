//! The GRBL laser window (G5-grbl-ui): what it shows, in words, from presentation's
//! [`GrblViewModel`], and how a widget becomes an intent. No AppKit here:
//! `appkit/panels/grbl_window.rs` builds the widgets from this model.

use burnburn_presentation::{
    ConsoleError, GrblCommandRefusal, GrblControlKind, GrblEntryKind, GrblIntent, GrblMachineState, GrblViewModel,
    GrblWhy, MacroError, StepDirection, UiIntent,
};

use crate::l10n::Locale;

/// One button of the window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrblButton {
    pub kind: GrblControlKind,
    pub title: String,
    pub enabled: bool,
    /// Why the button is off, in words (empty when it is on).
    pub tooltip: String,
}

/// One macro of the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrblMacroLine {
    pub name: String,
    pub program: String,
    pub enabled: bool,
    pub running: bool,
}

/// The window's texts and enablement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrblWindowPanel {
    pub title: String,
    pub status: String,
    /// Why the last action was refused (empty: nothing was refused).
    pub refusal: String,
    pub transcript: String,
    pub input: String,
    pub console_enabled: bool,
    pub console_tooltip: String,
    pub macros: Vec<GrblMacroLine>,
    pub buttons: Vec<GrblButton>,
    pub jog_step: String,
    pub jog_feed: String,
    /// Warnings in words that stay visible (unproven on hardware, stale session settings).
    pub notes: String,
}

fn pick(nl: &str, en: &str, locale: Locale) -> String {
    match locale {
        Locale::Nl => nl.to_owned(),
        Locale::En => en.to_owned(),
    }
}

fn state_word(state: GrblMachineState, locale: Locale) -> String {
    match state {
        GrblMachineState::Idle => pick("Rust", "Idle", locale),
        GrblMachineState::Run => pick("Bezig", "Run", locale),
        GrblMachineState::Hold(_) => pick("Pauze (hold)", "Hold", locale),
        GrblMachineState::Jog => pick("Jog", "Jog", locale),
        GrblMachineState::Alarm => "Alarm".to_owned(),
        GrblMachineState::Door(_) => pick("Deur open", "Door", locale),
        GrblMachineState::Check => pick("Controlemodus", "Check", locale),
        GrblMachineState::Home => pick("Homen", "Homing", locale),
        GrblMachineState::Sleep => pick("Slaapstand", "Sleep", locale),
    }
}

fn console_why(why: &ConsoleError, locale: Locale) -> String {
    match why {
        ConsoleError::Empty => pick("Lege regel.", "Empty line.", locale),
        ConsoleError::JobActive => grbl_why_text(&GrblWhy::JobActive, locale),
        ConsoleError::TooLong { bytes } => {
            pick(&format!("Regel te lang ({bytes} bytes)."), &format!("Line too long ({bytes} bytes)."), locale)
        }
        ConsoleError::ForbiddenCharacter(c) => pick(
            &format!("Het teken {c:?} is niet toegestaan in een regel."),
            &format!("The character {c:?} is not allowed in a line."),
            locale,
        ),
    }
}

fn macro_why(why: &MacroError, locale: Locale) -> String {
    match why {
        MacroError::EmptyName => pick("Een macro heeft een naam nodig.", "A macro needs a name.", locale),
        MacroError::NameTooLong => pick("De macronaam is te lang.", "The macro name is too long.", locale),
        MacroError::InvalidName => pick("Ongeldige macronaam.", "Invalid macro name.", locale),
        MacroError::DuplicateName => pick("Deze macronaam bestaat al.", "This macro name already exists.", locale),
        MacroError::UnknownMacro => pick("Onbekende macro.", "Unknown macro.", locale),
        MacroError::NoLines => pick("De macro heeft geen regels.", "The macro has no lines.", locale),
        MacroError::TooManyLines => pick("De macro heeft te veel regels.", "The macro has too many lines.", locale),
        MacroError::JobActive => grbl_why_text(&GrblWhy::JobActive, locale),
        MacroError::BadLine { line, reason } => {
            pick(&format!("Regel {line}: "), &format!("Line {line}: "), locale) + &console_why(reason, locale)
        }
        MacroError::Syntax { line } => pick(
            &format!("Het macrobestand is niet leesbaar (regel {line})."),
            &format!("The macro file cannot be read (line {line})."),
            locale,
        ),
    }
}

/// A refusal or a reason a control is off, in words.
pub fn grbl_why_text(why: &GrblWhy, locale: Locale) -> String {
    match why {
        GrblWhy::NotGrblMachine => {
            pick("De actieve machine is geen GRBL-machine.", "The active machine is not a GRBL machine.", locale)
        }
        GrblWhy::NoPort => pick("Geen seriële poort verbonden.", "No serial port connected.", locale),
        GrblWhy::JobActive => pick(
            "Een job loopt: wacht tot hij klaar is, of gebruik Pauze of Reset.",
            "A job is running: wait until it ends, or use Hold or Reset.",
            locale,
        ),
        GrblWhy::MacroRunning => pick("Een macro loopt.", "A macro is running.", locale),
        GrblWhy::NoStatusYet => pick(
            "De status van de controller is nog niet gelezen.",
            "The controller's status has not been read yet.",
            locale,
        ),
        GrblWhy::NotIdle(state) => {
            let word = state_word(*state, locale);
            pick(
                &format!("De controller is niet in rust ({word})."),
                &format!("The controller is not idle ({word})."),
                locale,
            )
        }
        GrblWhy::Busy => pick("Een regel wacht nog op antwoord.", "A line is still waiting for its answer.", locale),
        GrblWhy::Console(e) => console_why(e, locale),
        GrblWhy::Macro(e) => macro_why(e, locale),
        GrblWhy::Command(r) => match r {
            GrblCommandRefusal::JobActive => grbl_why_text(&GrblWhy::JobActive, locale),
            GrblCommandRefusal::Busy => grbl_why_text(&GrblWhy::Busy, locale),
            GrblCommandRefusal::NoStatusYet => grbl_why_text(&GrblWhy::NoStatusYet, locale),
            GrblCommandRefusal::NotIdle(state) => grbl_why_text(&GrblWhy::NotIdle(*state), locale),
            GrblCommandRefusal::InvalidLine => pick("Ongeldige regel.", "Invalid line.", locale),
            GrblCommandRefusal::InvalidJog => pick("Ongeldige jogstap.", "Invalid jog step.", locale),
            GrblCommandRefusal::ResumeAfterFault => pick(
                "Hervatten na een fout of verloren verbinding is geweigerd.",
                "Resuming after a fault or a lost connection is refused.",
                locale,
            ),
        },
        GrblWhy::Serial(e) => pick(&format!("Seriële verbinding: {e}."), &format!("Serial connection: {e}."), locale),
        GrblWhy::InvalidNumber => pick("Geen geldig getal.", "Not a valid number.", locale),
        GrblWhy::NotVerified { needs } => pick(
            &format!("Niet bewezen op een echte controller. Nodig: {needs}."),
            &format!("Not proven on a real controller. Needs: {needs}."),
            locale,
        ),
        _ => pick("Geweigerd.", "Refused.", locale),
    }
}

fn button_title(kind: GrblControlKind, locale: Locale) -> String {
    match kind {
        GrblControlKind::Jog(StepDirection::Left) => "←".to_owned(),
        GrblControlKind::Jog(StepDirection::Right) => "→".to_owned(),
        GrblControlKind::Jog(StepDirection::Up) => "↑".to_owned(),
        GrblControlKind::Jog(StepDirection::Down) => "↓".to_owned(),
        GrblControlKind::JogCancel => pick("Jog stoppen", "Cancel jog", locale),
        GrblControlKind::Home => "Home ($H)".to_owned(),
        GrblControlKind::Unlock => pick("Ontgrendel ($X)", "Unlock ($X)", locale),
        GrblControlKind::Hold => pick("Pauze (!)", "Hold (!)", locale),
        GrblControlKind::Resume => pick("Hervat (~)", "Resume (~)", locale),
        GrblControlKind::Reset => pick("Reset (Ctrl-X)", "Reset (Ctrl-X)", locale),
        GrblControlKind::StartJob => pick("Start job…", "Start job…", locale),
    }
}

pub fn grbl_window_panel(vm: Option<&GrblViewModel>, locale: Locale) -> Option<GrblWindowPanel> {
    let vm = vm?;
    let mut status = if !vm.connected {
        pick("Geen seriële poort verbonden", "No serial port connected", locale)
    } else {
        match vm.state {
            None => pick("Verbonden, status onbekend", "Connected, status unknown", locale),
            Some(state) => pick("Status: ", "Status: ", locale) + &state_word(state, locale),
        }
    };
    if let Some(code) = vm.alarm {
        status.push_str(&format!(" (ALARM:{code})"));
    }
    if let Some(job) = &vm.job {
        status.push_str(&pick(
            &format!(" — job {}/{} regels bevestigd", job.acked, job.total),
            &format!(" — job {}/{} lines acknowledged", job.acked, job.total),
            locale,
        ));
    }
    if vm.connected && !vm.position_trusted {
        status.push_str(&pick(
            " — positie niet vertrouwd, home opnieuw",
            " — position not trusted, home again",
            locale,
        ));
    }
    let transcript = vm
        .console
        .transcript
        .iter()
        .map(|row| {
            let mark = match row.kind {
                GrblEntryKind::Sent => "> ",
                GrblEntryKind::Received => "< ",
                GrblEntryKind::Info => "# ",
            };
            format!("{mark}{}", row.text)
        })
        .collect::<Vec<_>>()
        .join("\n");
    let mut notes = pick(
        "GRBL is alleen tegen een nagebootste controller getest; op echte hardware is niets bewezen.",
        "GRBL has only been tested against a simulated controller; nothing is proven on real hardware.",
        locale,
    );
    if vm.settings_stale {
        notes.push(' ');
        notes.push_str(&pick(
            "De verbinding verwacht een andere $30 dan het profiel: verbind opnieuw.",
            "The connection expects another $30 than the profile: reconnect.",
            locale,
        ));
    }
    Some(GrblWindowPanel {
        title: pick("GRBL-laservenster", "GRBL Laser Window", locale),
        status,
        refusal: vm.last_refusal.as_ref().map(|w| grbl_why_text(w, locale)).unwrap_or_default(),
        transcript,
        input: vm.console.input.clone(),
        console_enabled: vm.console.enabled,
        console_tooltip: vm.console.why_not.as_ref().map(|w| grbl_why_text(w, locale)).unwrap_or_default(),
        macros: vm
            .macros
            .iter()
            .map(|m| GrblMacroLine {
                name: m.name.clone(),
                program: m.lines.join("\n"),
                enabled: m.enabled,
                running: m.running,
            })
            .collect(),
        buttons: vm
            .controls
            .iter()
            .map(|c| GrblButton {
                kind: c.kind,
                title: button_title(c.kind, locale),
                enabled: c.enabled,
                tooltip: c.why_not.as_ref().map(|w| grbl_why_text(w, locale)).unwrap_or_default(),
            })
            .collect(),
        jog_step: vm.jog_step_mm.clone(),
        jog_feed: vm.jog_feed_mm_min.clone(),
        notes,
    })
}

/// The intent of a button; a button that is off makes none.
pub fn grbl_control_intent(vm: &GrblViewModel, kind: GrblControlKind) -> Option<UiIntent> {
    if !vm.offers(kind) {
        return None;
    }
    let intent = match kind {
        GrblControlKind::Jog(direction) => GrblIntent::Jog(direction),
        GrblControlKind::JogCancel => GrblIntent::JogCancel,
        GrblControlKind::Home => GrblIntent::Home,
        GrblControlKind::Unlock => GrblIntent::Unlock,
        GrblControlKind::Hold => GrblIntent::Hold,
        GrblControlKind::Resume => GrblIntent::Resume,
        GrblControlKind::Reset => GrblIntent::Reset,
        GrblControlKind::StartJob => GrblIntent::StartJob,
    };
    Some(UiIntent::Grbl(intent))
}

/// Return in the console input: the typed line (an empty one sends nothing).
pub fn grbl_send_intent(text: &str) -> Option<UiIntent> {
    let line = text.trim();
    (!line.is_empty()).then(|| UiIntent::Grbl(GrblIntent::Send(line.to_owned())))
}

pub fn grbl_macro_run_intent(vm: &GrblViewModel, row: usize) -> Option<UiIntent> {
    let m = vm.macros.get(row).filter(|m| m.enabled)?;
    Some(UiIntent::Grbl(GrblIntent::RunMacro(m.name.clone())))
}

pub fn grbl_macro_remove_intent(vm: &GrblViewModel, row: usize) -> Option<UiIntent> {
    let m = vm.macros.get(row)?;
    Some(UiIntent::Grbl(GrblIntent::RemoveMacro(m.name.clone())))
}

/// "Bewaar macro": a new name adds a macro, a known name replaces its lines.
pub fn grbl_macro_save_intent(vm: &GrblViewModel, name: &str, program: &str) -> Option<UiIntent> {
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    let (name, program) = (name.to_owned(), program.to_owned());
    Some(UiIntent::Grbl(if vm.macro_row(&name).is_some() {
        GrblIntent::ReplaceMacro { name, program }
    } else {
        GrblIntent::AddMacro { name, program }
    }))
}

/// Which number field of the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrblNumber {
    Step,
    Feed,
}

/// Step or feed committed (Return, Tab): text that is empty or equals what the window shows sends
/// nothing, so Return then Tab is one commit.
pub fn grbl_number_intent(vm: &GrblViewModel, which: GrblNumber, text: &str) -> Option<UiIntent> {
    let shown = match which {
        GrblNumber::Step => &vm.jog_step_mm,
        GrblNumber::Feed => &vm.jog_feed_mm_min,
    };
    if text.trim().is_empty() || text == shown {
        return None;
    }
    Some(UiIntent::Grbl(match which {
        GrblNumber::Step => GrblIntent::SetJogStep(text.to_owned()),
        GrblNumber::Feed => GrblIntent::SetJogFeed(text.to_owned()),
    }))
}

/// The console input's key commands (`moveUp:` and `moveDown:` walk the history).
pub fn grbl_input_command_intent(selector: &str) -> Option<UiIntent> {
    match selector {
        "moveUp:" => Some(UiIntent::Grbl(GrblIntent::RecallPrevious)),
        "moveDown:" => Some(UiIntent::Grbl(GrblIntent::RecallNext)),
        _ => None,
    }
}
