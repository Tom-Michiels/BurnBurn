//! The in-memory index over the journal and the operations that append to it.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use burnburn_io::Project;
use burnburn_io::fs::{FsError, FsErrorKind};
use burnburn_job::{JobPlan, ProfileRevision};
use burnburn_machine::{Execution, ExecutionId, MachineId, RdArtifact};
use burnburn_types::{CanonicalWriter, ContentHash, HashDomain};

use crate::ids::{ActionId, BlobKey, CompiledJobId, QueueItemId, RecordSeq};
use crate::journal::{self, SegmentEnd};
use crate::plan_blob::{decode_plan, encode_plan};
use crate::record::{
    CompiledBlobs, CompiledJob, EnvelopeIn, EnvelopeOut, EvidenceDto, ExecutionStateDto, FailReasonDto, HistoryRecord,
    IntentKind, KNOWN_KINDS, OrderRecord, PreflightSummary, RECORD_SCHEMA, ReconcileBasis, ReconcileStatement, Role,
    SettingsBackupRecord, StoredRecord, UnknownReasonDto,
};
use crate::settings::SettingsBackup;
use crate::storage::{JournalFs, write_atomically};
use crate::time::WallTime;

/// Size limits of the journal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JournalLimits {
    /// A segment is rotated after the record that makes it at least this large.
    pub max_segment: u64,
    pub max_record: u32,
}

impl Default for JournalLimits {
    fn default() -> Self {
        JournalLimits { max_segment: 8 << 20, max_record: 1 << 20 }
    }
}

/// Largest blob read back (RD files and project containers).
pub const MAX_BLOB_BYTES: u64 = 256 << 20;

#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum HistoryError {
    Fs(FsError),
    /// The journal was opened read-only after corruption; nothing can be appended.
    ReadOnly,
    /// A record or blob could not be encoded.
    Encode(String),
    RecordTooLarge {
        len: usize,
        max: u32,
    },
    /// Blob or plan bytes do not hash to their key (tampered or damaged).
    BlobMismatch,
    PlanBlob(&'static str),
    UnknownJob(CompiledJobId),
    UnknownExecution(ExecutionId),
    UnknownQueueItem(QueueItemId),
    UnknownOrder(u64),
    DuplicateExecution(ExecutionId),
    /// A job with preflight errors is never frozen.
    PreflightErrors(u32),
    /// The job was prepared for another machine setup revision (IM29): re-compile or confirm.
    ProfileChanged {
        recorded: Box<ProfileRevision>,
        current: Box<ProfileRevision>,
    },
    /// Reconciliation is only for executions whose outcome is unknown.
    NotUnknown(ExecutionStateDto),
    /// A quantity below what is already completed.
    QuantityBelowCompleted {
        completed: u32,
    },
    /// The RD blob does not match the job's recorded RD hash.
    RdHashMismatch,
}

impl From<FsError> for HistoryError {
    fn from(e: FsError) -> Self {
        HistoryError::Fs(e)
    }
}

impl fmt::Display for HistoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for HistoryError {}

/// What opening found. Nothing is ever truncated or rewritten.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RecoveryReport {
    pub segments: u32,
    pub records: u64,
    /// Short or damaged final records (crash during append). The next append starts a new
    /// segment; the torn bytes stay in place as evidence.
    pub torn_tails: Vec<TornTail>,
    /// Damage that a crash cannot explain; the history is then read-only.
    pub corrupt: Option<Corruption>,
    /// Records of an unknown kind (kept, not interpreted).
    pub unrecognized: u64,
    /// Executions that were not terminal when the application stopped: now `Unknown`.
    pub interrupted: Vec<ExecutionId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TornTail {
    pub segment: PathBuf,
    pub offset: u64,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Corruption {
    pub segment: PathBuf,
    pub offset: u64,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedIntent {
    pub seq: RecordSeq,
    pub action: ActionId,
    pub intent: IntentKind,
}

/// What a repeated action id refers to (IM31 idempotency).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionOutcome {
    pub seq: RecordSeq,
    pub execution: ExecutionId,
    pub what: ActionWhat,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionWhat {
    Intent(IntentKind),
    Reconciled(ExecutionStateDto),
}

/// Receipt of [`History::record_intent`]. `fresh == false`: the action id was already
/// recorded; the caller must not send the intent again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntentReceipt {
    pub seq: RecordSeq,
    pub fresh: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionView {
    pub id: ExecutionId,
    pub job: CompiledJobId,
    pub item: Option<QueueItemId>,
    pub machine: MachineId,
    pub state: ExecutionStateDto,
    /// True when the state is `Unknown` because the application stopped mid-execution.
    pub interrupted_by_restart: bool,
    pub intents: Vec<RecordedIntent>,
    pub evidence: Vec<(RecordSeq, EvidenceDto)>,
    pub reconciled: Option<ReconcileBasis>,
    pub notes: Vec<String>,
    pub created: WallTime,
    pub updated: WallTime,
}

impl ExecutionView {
    pub fn intent_recorded(&self, kind: IntentKind) -> bool {
        self.intents.iter().any(|i| i.intent == kind)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueItem {
    pub id: QueueItemId,
    pub job: CompiledJobId,
    pub quantity: u32,
}

/// Counts of one queue item. Only proven or operator-confirmed completions count as completed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct QueueCounts {
    pub planned: u32,
    pub completed: u32,
    /// Outcome unknown: needs a status query or an operator statement.
    pub unknown: u32,
    pub failed: u32,
    pub cancelled: u32,
    /// Not terminal and not unknown (uploading, stored, running …).
    pub in_progress: u32,
    pub remaining: u32,
}

/// Input of [`History::record_compiled`].
pub struct CompileRecord<'a> {
    pub plan: &'a JobPlan,
    pub rd_file: &'a [u8],
    pub project: &'a Project,
    pub project_path: Option<PathBuf>,
    pub preflight: PreflightSummary,
    /// Machine setup revision the plan was compiled and lowered for.
    pub profile: &'a ProfileRevision,
}

#[derive(Debug, Clone, Copy)]
struct Current {
    n: u32,
}

/// Append-only production history (ADR 0008). All IO goes through the injected [`JournalFs`].
#[derive(Debug)]
pub struct History {
    dir: PathBuf,
    limits: JournalLimits,
    read_only: bool,
    next_seq: u64,
    last_segment: u32,
    current: Option<Current>,
    records: Vec<StoredRecord>,
    jobs: BTreeMap<CompiledJobId, CompiledJob>,
    queue: BTreeMap<QueueItemId, QueueItem>,
    executions: BTreeMap<ExecutionId, ExecutionView>,
    actions: BTreeMap<ActionId, ActionOutcome>,
    max_execution: u64,
    max_item: u64,
    orders: BTreeMap<u64, OrderRecord>,
    order_links: BTreeMap<ExecutionId, u64>,
}

pub fn segment_path(dir: &Path, n: u32) -> PathBuf {
    dir.join(format!("journal-{n:08}.bbj"))
}

fn blob_path(dir: &Path, key: &BlobKey) -> PathBuf {
    dir.join("blobs").join(key.file_name())
}

/// `ContentHash::of(HistoryRecord, tag("compiled-job") ‖ plan ‖ rd ‖ profile.content)`.
/// Uses the `history-record` domain with a leading tag until `HashDomain::CompiledJob` (C-5) exists.
pub fn compiled_job_id(plan: &ContentHash, rd_hash: &ContentHash, profile: &ProfileRevision) -> CompiledJobId {
    let mut w = CanonicalWriter::new();
    w.tag("compiled-job");
    w.bytes(plan.as_bytes());
    w.bytes(rd_hash.as_bytes());
    w.bytes(profile.content.as_bytes());
    // Only fixed-size byte strings: encoding cannot fail.
    let payload = w.finish().unwrap_or_default();
    CompiledJobId(ContentHash::of(HashDomain::HistoryRecord, &payload))
}

impl History {
    fn empty(dir: &Path, limits: JournalLimits) -> History {
        History {
            dir: dir.to_path_buf(),
            limits,
            read_only: false,
            next_seq: 1,
            last_segment: 0,
            current: None,
            records: Vec::new(),
            jobs: BTreeMap::new(),
            queue: BTreeMap::new(),
            executions: BTreeMap::new(),
            actions: BTreeMap::new(),
            max_execution: 0,
            max_item: 0,
            orders: BTreeMap::new(),
            order_links: BTreeMap::new(),
        }
    }

    /// Reads every segment in order and rebuilds the index. See [`RecoveryReport`].
    /// Creates nothing: a missing directory is an empty history.
    pub fn open(
        fs: &dyn JournalFs,
        dir: &Path,
        limits: &JournalLimits,
    ) -> Result<(History, RecoveryReport), HistoryError> {
        let mut h = History::empty(dir, *limits);
        let mut report = RecoveryReport::default();
        let mut segments = Vec::new();
        while fs.exists(&segment_path(dir, segments.len() as u32 + 1))? {
            segments.push(segments.len() as u32 + 1);
        }
        report.segments = segments.len() as u32;
        h.last_segment = segments.last().copied().unwrap_or(0);
        let read_limit = limits.max_segment + u64::from(limits.max_record) + 64;
        let mut last_seq = 0u64;
        'segments: for &n in &segments {
            let path = segment_path(dir, n);
            let corrupt =
                |offset: u64, reason: &str| Corruption { segment: path.clone(), offset, reason: reason.into() };
            let bytes = match fs.read(&path, read_limit) {
                Ok(b) => b,
                Err(e) if e.kind == FsErrorKind::TooLarge => {
                    report.corrupt = Some(corrupt(0, "segment larger than the limits allow"));
                    break;
                }
                Err(e) => return Err(e.into()),
            };
            let scan = journal::scan_segment(&bytes, limits.max_record);
            for (offset, payload) in scan.records {
                match decode_record(payload, last_seq) {
                    Ok(stored) => {
                        if matches!(stored.record, HistoryRecord::Unrecognized { .. }) {
                            report.unrecognized += 1;
                        }
                        last_seq = stored.seq.0;
                        report.records += 1;
                        h.apply(stored, true);
                    }
                    Err(reason) => {
                        report.corrupt = Some(corrupt(offset, reason));
                        break 'segments;
                    }
                }
            }
            match scan.end {
                SegmentEnd::Clean => {
                    let len = bytes.len() as u64;
                    h.current = (n == h.last_segment && len < limits.max_segment).then_some(Current { n });
                }
                SegmentEnd::TornTail { offset, bytes } => {
                    report.torn_tails.push(TornTail { segment: path.clone(), offset, bytes });
                    // Never append after torn bytes: the next record starts a new segment.
                    h.current = None;
                }
                SegmentEnd::Corrupt { offset, reason } => {
                    report.corrupt = Some(corrupt(offset, reason));
                    break;
                }
            }
        }
        if report.corrupt.is_some() {
            h.read_only = true;
            h.current = None;
        }
        h.next_seq = last_seq + 1;
        report.interrupted = h.mark_interrupted();
        Ok((h, report))
    }

    pub fn read_only(&self) -> bool {
        self.read_only
    }

    /// Appends one record durably (append + sync) and indexes it. Returns only after the sync.
    pub fn append(
        &mut self,
        fs: &dyn JournalFs,
        record: HistoryRecord,
        at: WallTime,
    ) -> Result<RecordSeq, HistoryError> {
        if self.read_only {
            return Err(HistoryError::ReadOnly);
        }
        self.validate(&record)?;
        let seq = RecordSeq(self.next_seq);
        let envelope = EnvelopeOut { schema: RECORD_SCHEMA, seq, wall_time: at, record: &record };
        let payload = serde_json::to_vec(&envelope).map_err(|e| HistoryError::Encode(e.to_string()))?;
        if payload.len() > self.limits.max_record as usize {
            return Err(HistoryError::RecordTooLarge { len: payload.len(), max: self.limits.max_record });
        }
        let bytes = journal::encode_record(&payload)
            .ok_or(HistoryError::RecordTooLarge { len: payload.len(), max: self.limits.max_record })?;
        // From here on the seq is used up, also on failure: a record that reached the disk
        // despite an error must never share its seq with a later one.
        self.next_seq += 1;
        let current = match self.current {
            Some(c) => c,
            None => self.new_segment(fs)?,
        };
        let path = segment_path(&self.dir, current.n);
        match fs.append(&path, &bytes).and_then(|len| fs.sync_file(&path).map(|()| len)) {
            Ok(len) => {
                self.current = (len < self.limits.max_segment).then_some(Current { n: current.n });
                self.apply(StoredRecord { seq, wall_time: at, record }, false);
                Ok(seq)
            }
            Err(e) => {
                // Possibly torn or not durable: continue in a fresh segment.
                self.current = None;
                Err(e.into())
            }
        }
    }

    /// A new, durable, empty segment (temp + header + sync + replace + dir sync).
    fn new_segment(&mut self, fs: &dyn JournalFs) -> Result<Current, HistoryError> {
        fs.create_dir_all(&self.dir)?;
        let n = self.last_segment + 1;
        let path = segment_path(&self.dir, n);
        if fs.exists(&path)? {
            return Err(HistoryError::Fs(FsError::new(
                burnburn_io::fs::FsOp::CreateTemp,
                &path,
                FsErrorKind::AlreadyExists,
            )));
        }
        write_atomically(fs, &path, &journal::segment_header())?;
        self.last_segment = n;
        Ok(Current { n })
    }

    /// Writes a content-addressed blob atomically (idempotent).
    pub fn put_blob(&self, fs: &dyn JournalFs, bytes: &[u8]) -> Result<BlobKey, HistoryError> {
        let key = BlobKey::of(bytes);
        let path = blob_path(&self.dir, &key);
        if fs.exists(&path)? {
            return Ok(key);
        }
        fs.create_dir_all(&self.dir.join("blobs"))?;
        write_atomically(fs, &path, bytes)?;
        Ok(key)
    }

    /// Reads a blob and re-checks its hash.
    pub fn read_blob(&self, fs: &dyn JournalFs, key: &BlobKey) -> Result<Vec<u8>, HistoryError> {
        let bytes = fs.read(&blob_path(&self.dir, key), MAX_BLOB_BYTES)?;
        if BlobKey::of(&bytes) != *key {
            return Err(HistoryError::BlobMismatch);
        }
        Ok(bytes)
    }

    /// Freezes one compiled revision: blobs first (plan, RD, project), then `JobCompiled`.
    pub fn record_compiled(
        &mut self,
        fs: &dyn JournalFs,
        input: CompileRecord<'_>,
        at: WallTime,
    ) -> Result<CompiledJobId, HistoryError> {
        if input.preflight.errors > 0 {
            return Err(HistoryError::PreflightErrors(input.preflight.errors));
        }
        let plan_bytes = encode_plan(input.plan)?;
        let project_bytes =
            burnburn_io::encode_container(input.project).map_err(|e| HistoryError::Encode(format!("{e:?}")))?;
        let project_hash =
            burnburn_io::project_hash(input.project).map_err(|e| HistoryError::Encode(format!("{e:?}")))?;
        let rd_hash = burnburn_machine::rd_hash(input.rd_file);
        let plan = input.plan.id().hash();
        let id = compiled_job_id(&plan, &rd_hash, input.profile);
        if self.jobs.contains_key(&id) {
            return Ok(id);
        }
        let blobs = CompiledBlobs {
            plan: self.put_blob(fs, &plan_bytes)?,
            rd: self.put_blob(fs, input.rd_file)?,
            project: self.put_blob(fs, &project_bytes)?,
        };
        let job = CompiledJob {
            id,
            plan,
            rd_hash,
            profile: input.profile.clone(),
            project_hash,
            project_path: input.project_path,
            preflight: input.preflight,
            blobs,
            created: at,
        };
        self.append(fs, HistoryRecord::JobCompiled(Box::new(job)), at)?;
        Ok(id)
    }

    /// The frozen plan of a job, rebuilt from its blob and verified against its id.
    pub fn load_plan(&self, fs: &dyn JournalFs, job: CompiledJobId) -> Result<JobPlan, HistoryError> {
        let j = self.jobs.get(&job).ok_or(HistoryError::UnknownJob(job))?;
        decode_plan(&self.read_blob(fs, &j.blobs.plan)?, &j.plan)
    }

    /// The exact RD bytes of a job, verified against its recorded RD hash.
    pub fn load_rd(&self, fs: &dyn JournalFs, job: CompiledJobId) -> Result<Vec<u8>, HistoryError> {
        let j = self.jobs.get(&job).ok_or(HistoryError::UnknownJob(job))?;
        let bytes = self.read_blob(fs, &j.blobs.rd)?;
        if burnburn_machine::rd_hash(&bytes) != j.rd_hash {
            return Err(HistoryError::RdHashMismatch);
        }
        Ok(bytes)
    }

    /// The upload artifact for a (re-)execution: never recompiled, always the frozen bytes.
    pub fn load_artifact(
        &self,
        fs: &dyn JournalFs,
        job: CompiledJobId,
        max_chunk: usize,
    ) -> Result<RdArtifact, HistoryError> {
        let plan = self.load_plan(fs, job)?;
        let rd = self.load_rd(fs, job)?;
        RdArtifact::from_rd_file(plan.id(), &rd, max_chunk).map_err(|e| HistoryError::Encode(e.to_string()))
    }

    pub fn enqueue(
        &mut self,
        fs: &dyn JournalFs,
        job: CompiledJobId,
        quantity: u32,
        at: WallTime,
    ) -> Result<QueueItemId, HistoryError> {
        let item = QueueItemId(self.max_item + 1);
        self.append(fs, HistoryRecord::Queued { item, job, quantity }, at)?;
        Ok(item)
    }

    pub fn change_quantity(
        &mut self,
        fs: &dyn JournalFs,
        item: QueueItemId,
        quantity: u32,
        at: WallTime,
    ) -> Result<RecordSeq, HistoryError> {
        self.append(fs, HistoryRecord::QueueChanged { item, quantity }, at)
    }

    /// Creates a new execution of a frozen job. Refused when the machine setup changed since
    /// the job was prepared (`ProfileRevision.content` differs).
    pub fn create_execution(
        &mut self,
        fs: &dyn JournalFs,
        job: CompiledJobId,
        machine: MachineId,
        item: Option<QueueItemId>,
        current_profile: &ProfileRevision,
        at: WallTime,
    ) -> Result<ExecutionId, HistoryError> {
        let j = self.jobs.get(&job).ok_or(HistoryError::UnknownJob(job))?;
        if j.profile.id != current_profile.id || j.profile.content != current_profile.content {
            return Err(HistoryError::ProfileChanged {
                recorded: Box::new(j.profile.clone()),
                current: Box::new(current_profile.clone()),
            });
        }
        let execution = self.next_execution_id();
        self.append(fs, HistoryRecord::ExecutionCreated { execution, job, item, machine }, at)?;
        Ok(execution)
    }

    /// Makes an intent durable. Only when this returns `fresh` may the driver receive it.
    pub fn record_intent(
        &mut self,
        fs: &dyn JournalFs,
        execution: ExecutionId,
        action: ActionId,
        intent: IntentKind,
        at: WallTime,
    ) -> Result<IntentReceipt, HistoryError> {
        if let Some(o) = self.actions.get(&action) {
            return Ok(IntentReceipt { seq: o.seq, fresh: false });
        }
        let seq = self.append(fs, HistoryRecord::IntentRecorded { execution, action, intent }, at)?;
        Ok(IntentReceipt { seq, fresh: true })
    }

    /// Journals what the actor observed since the last call: new evidence entries, then the state.
    pub fn observe(&mut self, fs: &dyn JournalFs, execution: &Execution, at: WallTime) -> Result<(), HistoryError> {
        let view = self.executions.get(&execution.id).ok_or(HistoryError::UnknownExecution(execution.id))?;
        let written = view.evidence.len();
        let state = ExecutionStateDto::from(&execution.state);
        let changed = view.state != state;
        for e in execution.evidence.iter().skip(written) {
            let evidence = EvidenceDto::from(&e.evidence);
            self.append(fs, HistoryRecord::Evidence { execution: execution.id, evidence }, at)?;
        }
        if changed {
            self.append(fs, HistoryRecord::StateObserved { execution: execution.id, state }, at)?;
        }
        Ok(())
    }

    /// The operator states what happened to an execution whose outcome is unknown. Never
    /// called automatically; `RanToCompletion` is the only statement that counts a piece.
    pub fn reconcile_by_operator(
        &mut self,
        fs: &dyn JournalFs,
        execution: ExecutionId,
        role: Role,
        statement: ReconcileStatement,
        action: ActionId,
        at: WallTime,
    ) -> Result<RecordSeq, HistoryError> {
        let view = self.executions.get(&execution).ok_or(HistoryError::UnknownExecution(execution))?;
        if !view.state.is_unknown() {
            return Err(HistoryError::NotUnknown(view.state.clone()));
        }
        let to = match statement {
            ReconcileStatement::RanToCompletion => ExecutionStateDto::Completed,
            ReconcileStatement::DidNotRun => ExecutionStateDto::Failed { why: FailReasonDto::OperatorDidNotRun },
            ReconcileStatement::Partial => ExecutionStateDto::Failed { why: FailReasonDto::OperatorPartial },
        };
        let record = HistoryRecord::Reconciled {
            execution,
            from: view.state.clone(),
            to,
            by: ReconcileBasis::OperatorConfirmed { role, statement },
            action,
        };
        self.append(fs, record, at)
    }

    pub fn note(
        &mut self,
        fs: &dyn JournalFs,
        execution: ExecutionId,
        text: String,
        at: WallTime,
    ) -> Result<RecordSeq, HistoryError> {
        self.append(fs, HistoryRecord::OperatorNote { execution, text }, at)
    }

    /// Journals a settings backup. Any settings write must be preceded by one (IM29).
    pub fn record_settings_backup(
        &mut self,
        fs: &dyn JournalFs,
        backup: &SettingsBackup,
        at: WallTime,
    ) -> Result<RecordSeq, HistoryError> {
        let record = SettingsBackupRecord { machine: backup.machine.clone(), backup: backup.to_json() };
        self.append(fs, HistoryRecord::SettingsBackup(record), at)
    }

    /// Settings backups of one machine, oldest first.
    pub fn settings_backups(&self, machine: &MachineId) -> Vec<(RecordSeq, Result<SettingsBackup, HistoryError>)> {
        self.records
            .iter()
            .filter_map(|r| match &r.record {
                HistoryRecord::SettingsBackup(b) if b.machine == *machine => {
                    Some((r.seq, SettingsBackup::from_json(&b.backup)))
                }
                _ => None,
            })
            .collect()
    }

    /// Records a new or changed order (durable before it returns).
    pub fn record_order(
        &mut self,
        fs: &dyn JournalFs,
        order: OrderRecord,
        at: WallTime,
    ) -> Result<RecordSeq, HistoryError> {
        self.append(fs, HistoryRecord::OrderRecorded(order), at)
    }

    /// Links an execution to the order it was made for.
    pub fn link_order(
        &mut self,
        fs: &dyn JournalFs,
        order: u64,
        execution: ExecutionId,
        at: WallTime,
    ) -> Result<RecordSeq, HistoryError> {
        self.append(fs, HistoryRecord::OrderLinked { order, execution }, at)
    }

    /// Orders, latest version of each, by number.
    pub fn orders(&self) -> impl Iterator<Item = &OrderRecord> {
        self.orders.values()
    }

    /// The order an execution was made for.
    pub fn order_of(&self, execution: ExecutionId) -> Option<u64> {
        self.order_links.get(&execution).copied()
    }

    /// `max(seen) + 1`, persistent over restarts.
    pub fn next_execution_id(&self) -> ExecutionId {
        ExecutionId::from_raw(self.max_execution + 1)
    }

    pub fn job(&self, id: CompiledJobId) -> Option<&CompiledJob> {
        self.jobs.get(&id)
    }

    pub fn jobs(&self) -> impl Iterator<Item = &CompiledJob> {
        self.jobs.values()
    }

    pub fn execution(&self, id: ExecutionId) -> Option<&ExecutionView> {
        self.executions.get(&id)
    }

    pub fn executions(&self) -> impl Iterator<Item = &ExecutionView> {
        self.executions.values()
    }

    pub fn executions_for_machine<'a>(&'a self, machine: &'a MachineId) -> impl Iterator<Item = &'a ExecutionView> {
        self.executions.values().filter(move |e| e.machine == *machine)
    }

    pub fn jobs_for_project(&self, project_hash: ContentHash) -> impl Iterator<Item = &CompiledJob> {
        self.jobs.values().filter(move |j| j.project_hash == project_hash)
    }

    pub fn executions_for_project(&self, project_hash: ContentHash) -> impl Iterator<Item = &ExecutionView> {
        self.executions.values().filter(move |e| self.jobs.get(&e.job).is_some_and(|j| j.project_hash == project_hash))
    }

    pub fn queue_item(&self, id: QueueItemId) -> Option<&QueueItem> {
        self.queue.get(&id)
    }

    pub fn queue(&self) -> impl Iterator<Item = &QueueItem> {
        self.queue.values()
    }

    pub fn queue_counts(&self, id: QueueItemId) -> Option<QueueCounts> {
        let item = self.queue.get(&id)?;
        let mut c = QueueCounts { planned: item.quantity, ..QueueCounts::default() };
        for e in self.executions.values().filter(|e| e.item == Some(id)) {
            match &e.state {
                ExecutionStateDto::Completed => c.completed += 1,
                ExecutionStateDto::Unknown { .. } => c.unknown += 1,
                ExecutionStateDto::Failed { .. } => c.failed += 1,
                ExecutionStateDto::Cancelled => c.cancelled += 1,
                _ => c.in_progress += 1,
            }
        }
        c.remaining = c.planned.saturating_sub(c.completed);
        Some(c)
    }

    pub fn action_outcome(&self, action: ActionId) -> Option<&ActionOutcome> {
        self.actions.get(&action)
    }

    /// Every record in journal order (including unrecognized ones).
    pub fn records(&self) -> &[StoredRecord] {
        &self.records
    }

    fn validate(&self, record: &HistoryRecord) -> Result<(), HistoryError> {
        let exec = |id: &ExecutionId| {
            if self.executions.contains_key(id) { Ok(()) } else { Err(HistoryError::UnknownExecution(*id)) }
        };
        match record {
            HistoryRecord::JobCompiled(_) | HistoryRecord::SettingsBackup(_) => Ok(()),
            HistoryRecord::Queued { job, .. } => {
                if self.jobs.contains_key(job) {
                    Ok(())
                } else {
                    Err(HistoryError::UnknownJob(*job))
                }
            }
            HistoryRecord::QueueChanged { item, quantity } => {
                if !self.queue.contains_key(item) {
                    return Err(HistoryError::UnknownQueueItem(*item));
                }
                let completed = self.queue_counts(*item).map_or(0, |c| c.completed);
                if *quantity < completed {
                    return Err(HistoryError::QuantityBelowCompleted { completed });
                }
                Ok(())
            }
            HistoryRecord::ExecutionCreated { execution, job, item, .. } => {
                if self.executions.contains_key(execution) {
                    return Err(HistoryError::DuplicateExecution(*execution));
                }
                if !self.jobs.contains_key(job) {
                    return Err(HistoryError::UnknownJob(*job));
                }
                match item {
                    Some(i) if !self.queue.contains_key(i) => Err(HistoryError::UnknownQueueItem(*i)),
                    _ => Ok(()),
                }
            }
            HistoryRecord::IntentRecorded { execution, .. }
            | HistoryRecord::Evidence { execution, .. }
            | HistoryRecord::StateObserved { execution, .. }
            | HistoryRecord::Reconciled { execution, .. }
            | HistoryRecord::OperatorNote { execution, .. } => exec(execution),
            HistoryRecord::OrderRecorded(_) => Ok(()),
            HistoryRecord::OrderLinked { order, execution } => {
                exec(execution)?;
                if self.orders.contains_key(order) { Ok(()) } else { Err(HistoryError::UnknownOrder(*order)) }
            }
            HistoryRecord::Unrecognized { .. } => {
                Err(HistoryError::Encode("unrecognized records are never written".into()))
            }
        }
    }

    /// Updates the index. `replay`: the record comes from disk (lenient on references).
    fn apply(&mut self, stored: StoredRecord, replay: bool) {
        let _ = replay;
        let (seq, at) = (stored.seq, stored.wall_time);
        match &stored.record {
            HistoryRecord::JobCompiled(job) => {
                self.jobs.entry(job.id).or_insert_with(|| (**job).clone());
            }
            HistoryRecord::Queued { item, job, quantity } => {
                self.max_item = self.max_item.max(item.0);
                self.queue.insert(*item, QueueItem { id: *item, job: *job, quantity: *quantity });
            }
            HistoryRecord::QueueChanged { item, quantity } => {
                if let Some(q) = self.queue.get_mut(item) {
                    q.quantity = *quantity;
                }
            }
            HistoryRecord::ExecutionCreated { execution, job, item, machine } => {
                self.max_execution = self.max_execution.max(execution.raw());
                self.executions.insert(
                    *execution,
                    ExecutionView {
                        id: *execution,
                        job: *job,
                        item: *item,
                        machine: machine.clone(),
                        state: ExecutionStateDto::Created,
                        interrupted_by_restart: false,
                        intents: Vec::new(),
                        evidence: Vec::new(),
                        reconciled: None,
                        notes: Vec::new(),
                        created: at,
                        updated: at,
                    },
                );
            }
            HistoryRecord::IntentRecorded { execution, action, intent } => {
                self.actions
                    .insert(*action, ActionOutcome { seq, execution: *execution, what: ActionWhat::Intent(*intent) });
                if let Some(v) = self.executions.get_mut(execution) {
                    v.intents.push(RecordedIntent { seq, action: *action, intent: *intent });
                    v.updated = at;
                }
            }
            HistoryRecord::Evidence { execution, evidence } => {
                if let Some(v) = self.executions.get_mut(execution) {
                    v.evidence.push((seq, evidence.clone()));
                    v.updated = at;
                }
            }
            HistoryRecord::StateObserved { execution, state } => {
                if let Some(v) = self.executions.get_mut(execution) {
                    v.state = state.clone();
                    v.interrupted_by_restart = false;
                    v.updated = at;
                }
            }
            HistoryRecord::Reconciled { execution, to, by, action, .. } => {
                self.actions.insert(
                    *action,
                    ActionOutcome { seq, execution: *execution, what: ActionWhat::Reconciled(to.clone()) },
                );
                if let Some(v) = self.executions.get_mut(execution) {
                    v.state = to.clone();
                    v.reconciled = Some(by.clone());
                    v.interrupted_by_restart = false;
                    v.updated = at;
                }
            }
            HistoryRecord::OperatorNote { execution, text } => {
                if let Some(v) = self.executions.get_mut(execution) {
                    v.notes.push(text.clone());
                }
            }
            HistoryRecord::OrderRecorded(o) => {
                self.orders.insert(o.order, o.clone());
            }
            HistoryRecord::OrderLinked { order, execution } => {
                self.order_links.insert(*execution, *order);
            }
            HistoryRecord::SettingsBackup(_) | HistoryRecord::Unrecognized { .. } => {}
        }
        self.records.push(stored);
    }

    /// After replay: whatever was not terminal is now unknown (ADR 0008 §5).
    fn mark_interrupted(&mut self) -> Vec<ExecutionId> {
        let mut out = Vec::new();
        for v in self.executions.values_mut() {
            if !v.state.is_terminal() {
                if !v.state.is_unknown() {
                    v.state = ExecutionStateDto::Unknown { why: UnknownReasonDto::InterruptedByRestart };
                }
                v.interrupted_by_restart = true;
                out.push(v.id);
            }
        }
        out
    }
}

/// One journal payload → record. `Err(reason)` = corruption.
fn decode_record(payload: &[u8], last_seq: u64) -> Result<StoredRecord, &'static str> {
    let env: EnvelopeIn = serde_json::from_slice(payload).map_err(|_| "record is not a journal envelope")?;
    if env.schema != RECORD_SCHEMA {
        return Err("unknown record schema");
    }
    if env.seq.0 <= last_seq {
        return Err("non-increasing record seq");
    }
    let record = if KNOWN_KINDS.contains(&env.kind.as_str()) {
        let mut map = serde_json::Map::new();
        map.insert("kind".into(), serde_json::Value::String(env.kind));
        map.insert("body".into(), env.body);
        serde_json::from_value(serde_json::Value::Object(map)).map_err(|_| "record body does not match its kind")?
    } else {
        HistoryRecord::Unrecognized { kind: env.kind, body: env.body }
    };
    Ok(StoredRecord { seq: env.seq, wall_time: env.wall_time, record })
}
