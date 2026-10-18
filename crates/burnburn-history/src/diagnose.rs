//! Diagnostic replay (M07): a read-only report rebuilt from the journal alone.
//!
//! There is deliberately no port, actor, driver or clock in the signature: replay cannot talk to
//! a machine. Private data (project paths, machine identifiers) is left out unless the caller
//! ticks it, matching the export sheet of `docs/user-flows/gate-c.md` C6.

use std::fmt::Write as _;
use std::path::PathBuf;

use burnburn_machine::ExecutionId;
use burnburn_types::ContentHash;

use crate::history::History;
use crate::ids::{ActionId, CompiledJobId, RecordSeq};
use crate::record::{EvidenceDto, ExecutionStateDto, HistoryRecord, IntentKind, LiveIntentDto, LiveOutcomeDto};

/// What may leave the machine in a diagnostic export. Everything private is off by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DiagnosticOptions {
    pub include_project_paths: bool,
    pub include_machine_ids: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DiagJob {
    pub id: CompiledJobId,
    pub plan: ContentHash,
    pub rd_hash: ContentHash,
    pub profile_id: String,
    pub profile_revision: u32,
    pub project_path: Option<PathBuf>,
}

/// One journaled fact about an execution, in journal order.
#[derive(Debug, Clone, PartialEq)]
pub enum DiagEvent {
    Intent { seq: RecordSeq, intent: IntentKind },
    Evidence { seq: RecordSeq, evidence: EvidenceDto },
    State { seq: RecordSeq, state: ExecutionStateDto },
    Reconciled { seq: RecordSeq, to: ExecutionStateDto },
}

#[derive(Debug, Clone, PartialEq)]
pub struct DiagExecution {
    pub id: ExecutionId,
    pub job: CompiledJobId,
    pub machine: Option<String>,
    pub state: ExecutionStateDto,
    pub interrupted_by_restart: bool,
    pub timeline: Vec<DiagEvent>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DiagLive {
    pub seq: RecordSeq,
    pub action: ActionId,
    pub machine: Option<String>,
    pub intent: LiveIntentDto,
    pub outcome: Option<LiveOutcomeDto>,
    pub interrupted_by_restart: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DiagnosticReport {
    /// Journal records the report was built from.
    pub records: u64,
    pub jobs: Vec<DiagJob>,
    pub executions: Vec<DiagExecution>,
    pub live_actions: Vec<DiagLive>,
}

/// Rebuilds the report from an opened history. Reads only.
pub fn diagnostic_replay(history: &History, options: &DiagnosticOptions) -> DiagnosticReport {
    let machine = |m: &burnburn_machine::MachineId| options.include_machine_ids.then(|| m.as_str().to_owned());
    let jobs = history
        .jobs()
        .map(|j| DiagJob {
            id: j.id,
            plan: j.plan,
            rd_hash: j.rd_hash,
            profile_id: j.profile.id.clone(),
            profile_revision: j.profile.revision,
            project_path: if options.include_project_paths { j.project_path.clone() } else { None },
        })
        .collect();
    let executions = history
        .executions()
        .map(|v| {
            let timeline = history
                .records()
                .iter()
                .filter_map(|r| match &r.record {
                    HistoryRecord::IntentRecorded { execution, intent, .. } if *execution == v.id => {
                        Some(DiagEvent::Intent { seq: r.seq, intent: *intent })
                    }
                    HistoryRecord::Evidence { execution, evidence } if *execution == v.id => {
                        Some(DiagEvent::Evidence { seq: r.seq, evidence: evidence.clone() })
                    }
                    HistoryRecord::StateObserved { execution, state } if *execution == v.id => {
                        Some(DiagEvent::State { seq: r.seq, state: state.clone() })
                    }
                    HistoryRecord::Reconciled { execution, to, .. } if *execution == v.id => {
                        Some(DiagEvent::Reconciled { seq: r.seq, to: to.clone() })
                    }
                    _ => None,
                })
                .collect();
            DiagExecution {
                id: v.id,
                job: v.job,
                machine: machine(&v.machine),
                state: v.state.clone(),
                interrupted_by_restart: v.interrupted_by_restart,
                timeline,
            }
        })
        .collect();
    let live_actions = history
        .live_actions()
        .map(|v| DiagLive {
            seq: v.seq,
            action: v.action,
            machine: machine(&v.machine),
            intent: v.intent.clone(),
            outcome: v.outcome,
            interrupted_by_restart: v.interrupted_by_restart,
        })
        .collect();
    DiagnosticReport { records: history.records().len() as u64, jobs, executions, live_actions }
}

impl DiagnosticReport {
    /// Plain text for a support bundle; contains exactly what the structure contains.
    pub fn render_text(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "BurnBurn diagnostic replay: {} journal records", self.records);
        for j in &self.jobs {
            let _ = write!(out, "job {} rd {} profile {}@{}", j.id, j.rd_hash.to_hex(), j.profile_id, j.profile_revision);
            if let Some(p) = &j.project_path {
                let _ = write!(out, " project {}", p.display());
            }
            out.push('\n');
        }
        for e in &self.executions {
            let _ = write!(out, "execution {} job {} state {:?}", e.id, e.job, e.state);
            if let Some(m) = &e.machine {
                let _ = write!(out, " machine {m}");
            }
            if e.interrupted_by_restart {
                out.push_str(" (interrupted by restart)");
            }
            out.push('\n');
            for ev in &e.timeline {
                let _ = writeln!(out, "  {ev:?}");
            }
        }
        for a in &self.live_actions {
            let _ = write!(out, "live {} {:?} outcome {:?}", a.action, a.intent, a.outcome);
            if let Some(m) = &a.machine {
                let _ = write!(out, " machine {m}");
            }
            out.push('\n');
        }
        out
    }
}
