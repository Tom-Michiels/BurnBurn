//! Help, support data and the debug log (X03). Nothing here talks to a network: the manual is a
//! link the host opens on a click, and the support report is text the user sees before it is
//! written. Project names and paths, and machine identifiers, are left out unless the user says
//! otherwise; the debug log keeps the names of intents, never their payloads.

use std::collections::VecDeque;
use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

use crate::intent::UiIntent;
use crate::production::DiagnosticOptions;
use crate::task::TaskInput;

use super::AppIntent;
use super::prefs::PrefKey;
use super::{AppController, AppUpdate};

/// Static facts for Help ▸ About and the support report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HelpInfo {
    pub app_name: &'static str,
    pub version: &'static str,
    pub os: &'static str,
    pub arch: &'static str,
    /// Where the manual is, when this build knows (`BURNBURN_MANUAL_URL` at build time). `None`:
    /// the Help item is disabled and says so; no address is invented.
    pub manual_url: Option<&'static str>,
}

/// Lines the debug log keeps; the oldest go first.
pub const DEBUG_LOG_LINES: usize = 500;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DebugLogViewModel {
    pub enabled: bool,
    /// Oldest first: `#<sequence> <intent name>` with `×n` for a repeat.
    pub lines: Vec<String>,
}

/// The debug log of one run (memory only).
#[derive(Debug, Default)]
pub(super) struct DebugLog {
    enabled: bool,
    sequence: u64,
    /// (sequence of the first occurrence, intent name, repeat count)
    lines: VecDeque<(u64, String, u32)>,
}

/// A writer that accepts only the leading identifier of a `Debug` text and fails at the first
/// other character: formatting stops there, so no field of the intent is ever rendered.
struct NameOnly(String);

impl fmt::Write for NameOnly {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for c in s.chars() {
            if c.is_alphanumeric() || c == '_' {
                self.0.push(c);
            } else {
                return Err(fmt::Error);
            }
        }
        Ok(())
    }
}

/// What the log keeps of an intent: its name, plus the command or tool id (public vocabulary)
/// for the two intents that are mostly "which one". Never a path, text or number.
fn intent_name(intent: &UiIntent) -> String {
    let mut name = NameOnly(String::new());
    let _ = fmt::write(&mut name, format_args!("{intent:?}"));
    match intent {
        UiIntent::Invoke { command, .. } => format!("{} {command:?}", name.0),
        UiIntent::SelectTool { id, .. } => format!("{} {id}", name.0),
        _ => name.0,
    }
}

impl DebugLog {
    pub(super) fn record(&mut self, intent: &UiIntent) {
        // The switch itself is not logged: turning it off must leave an empty log.
        let own = matches!(intent, UiIntent::App(AppIntent::SetDebugLog { .. } | AppIntent::ClearDebugLog));
        if !self.enabled || own {
            return;
        }
        self.sequence += 1;
        let name = intent_name(intent);
        match self.lines.back_mut() {
            Some((_, last, count)) if *last == name => *count += 1,
            _ => {
                self.lines.push_back((self.sequence, name, 1));
                if self.lines.len() > DEBUG_LOG_LINES {
                    self.lines.pop_front();
                }
            }
        }
    }

    fn rendered(&self) -> Vec<String> {
        self.lines
            .iter()
            .map(|(n, name, count)| if *count > 1 { format!("#{n} {name} ×{count}") } else { format!("#{n} {name}") })
            .collect()
    }
}

fn yes_no(on: bool) -> &'static str {
    if on { "yes" } else { "no" }
}

impl AppController {
    pub fn help_info() -> HelpInfo {
        HelpInfo {
            app_name: "BurnBurn",
            version: env!("CARGO_PKG_VERSION"),
            os: std::env::consts::OS,
            arch: std::env::consts::ARCH,
            manual_url: option_env!("BURNBURN_MANUAL_URL"),
        }
    }

    /// The exact text a support bundle would hold for `options`: what the user sees before saving
    /// is what is written. Document content (objects, text, names of shapes) is never part of it.
    pub fn support_report(&self, options: &DiagnosticOptions) -> String {
        let info = Self::help_info();
        let mut out = String::new();
        let mut line = |text: String| {
            out.push_str(&text);
            out.push('\n');
        };
        line(format!("{} support data", info.app_name));
        line("[app]".into());
        line(format!("version: {}", info.version));
        line(format!("platform: {} {}", info.os, info.arch));
        line("[consent]".into());
        line(format!(
            "project names and paths: {}",
            if options.include_project_paths { "included" } else { "left out" }
        ));
        line(format!("machine identifiers: {}", if options.include_machine_ids { "included" } else { "left out" }));
        line("[preferences]".into());
        for key in PrefKey::ALL {
            line(format!("{} = {}", key.id(), self.app.prefs.stored(key)));
        }
        line(format!("customized shortcuts: {}", self.app.settings.shortcuts.len()));
        line("[windows]".into());
        line(format!("windows: {}", self.sessions.len()));
        for (n, s) in self.sessions.values().enumerate() {
            let snapshot = s.doc.snapshot();
            let plan = match s.compiled.as_ref() {
                None => "none",
                Some(c) if super::process::plan_view(c, s, self.machine.as_ref()).stale => "stale",
                Some(_) => "fresh",
            };
            line(format!(
                "window {}: revision {}, objects {}, operations {}, unsaved changes {}, plan {plan}",
                n + 1,
                s.doc.revision().raw(),
                snapshot.draw_order().len(),
                snapshot.process().operations().count(),
                yes_no(s.doc.is_dirty()),
            ));
            if options.include_project_paths {
                line(format!("  title: {}", s.title));
                if let Some(path) = &s.path {
                    line(format!("  path: {}", path.display()));
                }
            }
        }
        line("[machine]".into());
        match &self.machine {
            None => line("profile: none".into()),
            Some(m) => {
                let frame = m.profile.frame();
                let name = if options.include_machine_ids {
                    m.profile.revision().id.clone()
                } else {
                    "(name left out)".into()
                };
                line(format!("profile: {name}, bed {} x {} mm", frame.width_mm(), frame.height_mm()));
            }
        }
        line("[journal]".into());
        match self.history.as_ref().map(|h| h.diagnostic_report(options)) {
            None => line("journal: not available".into()),
            Some(Ok(text)) => line(text.trim_end().to_owned()),
            Some(Err(_)) => line("journal: could not be read".into()),
        }
        line("[debug log]".into());
        line(format!("enabled: {}", yes_no(self.app.debug_log.enabled)));
        for l in self.app.debug_log.rendered() {
            line(l);
        }
        out
    }

    pub fn debug_log_view(&self) -> DebugLogViewModel {
        DebugLogViewModel { enabled: self.app.debug_log.enabled, lines: self.app.debug_log.rendered() }
    }

    pub(super) fn set_debug_log(&mut self, enabled: bool) -> AppUpdate {
        self.app.debug_log.enabled = enabled;
        AppUpdate { view_changed: true, ..AppUpdate::default() }
    }

    pub(super) fn clear_debug_log(&mut self) -> AppUpdate {
        self.app.debug_log.lines.clear();
        AppUpdate { view_changed: true, ..AppUpdate::default() }
    }

    /// Writes the support report to `path` in the background (atomic replace).
    pub(super) fn export_support_bundle(&mut self, options: DiagnosticOptions, path: PathBuf) -> AppUpdate {
        let text = self.support_report(&options);
        self.app_task(TaskInput::WriteText { text: Arc::from(text), path, fs: Arc::clone(&self.ports.fs) })
    }
}
