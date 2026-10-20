//! Preview window model (C07, IM15): the compiled plan of a window played, scrubbed and picked.
//! The preview is a view of exactly one immutable plan; nothing here edits the document except
//! that a pick selects its source, and nothing reaches a machine. Time is the constant-velocity
//! estimate of the plan, never a measurement.

use std::sync::Arc;

use burnburn_editor::SelectionChange;
use burnburn_job::{JobPlanId, MoveKind, PlanPreview, PreviewMove, SourceRef};
use burnburn_types::{DocumentId, OperationId};

use super::{AppController, AppUpdate};

/// Playback rate relative to the estimate clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlaybackSpeed {
    #[default]
    X1,
    X2,
    X5,
    X10,
    X50,
}

impl PlaybackSpeed {
    pub const ALL: [PlaybackSpeed; 5] =
        [PlaybackSpeed::X1, PlaybackSpeed::X2, PlaybackSpeed::X5, PlaybackSpeed::X10, PlaybackSpeed::X50];

    pub fn factor(self) -> f64 {
        match self {
            PlaybackSpeed::X1 => 1.0,
            PlaybackSpeed::X2 => 2.0,
            PlaybackSpeed::X5 => 5.0,
            PlaybackSpeed::X10 => 10.0,
            PlaybackSpeed::X50 => 50.0,
        }
    }
}

/// The move being drawn when the clock stops inside it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PartialMove {
    pub index: usize,
    /// How much of the move is done, `0..1`.
    pub fraction: f64,
}

/// The move the user picked.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PreviewSelection {
    pub index: usize,
    pub kind: MoveKind,
    /// Geometry and operation of a burning move; `None` for travel and overscan.
    pub source: Option<SourceRef>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PreviewViewModel {
    pub plan: JobPlanId,
    /// The document or the machine profile changed since this plan was compiled.
    pub stale: bool,
    pub preview: Arc<PlanPreview>,
    /// Totals of the plan's clock. Always an estimate: constant velocity per move, no
    /// acceleration, firmware overscan not modelled.
    pub estimated_total_seconds: f64,
    pub estimated_position_seconds: f64,
    /// Where the head is at the clock position (machine mm).
    pub head: [f64; 2],
    /// Moves `..completed_moves` are fully played.
    pub completed_moves: usize,
    pub partial: Option<PartialMove>,
    pub playing: bool,
    pub speed: PlaybackSpeed,
    pub show_travel: bool,
    pub selected: Option<PreviewSelection>,
    /// The operation of the picked move, for the operations table to highlight.
    pub selected_operation: Option<OperationId>,
    /// Width and height of the machine bed (mm), for the outline the preview draws; `None`
    /// without a machine profile.
    pub bed_mm: Option<[f64; 2]>,
}

/// One decoded record of the RD file this plan lowers to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RdRecordRow {
    pub offset: usize,
    /// The record's bytes as they are in the unscrambled stream, hex.
    pub hex: String,
    /// The public registry's name for the opcode: a label, not proof of firmware behaviour.
    pub label: Option<&'static str>,
    pub decoded: String,
}

/// The read-only RD view of a plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RdInspectViewModel {
    pub plan: JobPlanId,
    /// This is the window's current plan (not an older one the user inspected before editing).
    pub current: bool,
    /// Size of the scrambled file.
    pub bytes: usize,
    pub records: Vec<RdRecordRow>,
    /// Records left out of `records` because the list is capped.
    pub truncated: usize,
    /// Why the plan has no RD form (lowering refused it); `records` is then empty.
    pub error: Option<String>,
}

/// Window state of one preview. It belongs to the plan it was made for: a new plan starts over.
#[derive(Debug, Clone)]
pub(super) struct PreviewState {
    plan: JobPlanId,
    position: f64,
    playing: bool,
    speed: PlaybackSpeed,
    show_travel: bool,
    selected: Option<PreviewSelection>,
    selected_operation: Option<OperationId>,
}

impl PreviewState {
    fn new(plan: JobPlanId) -> Self {
        PreviewState {
            plan,
            position: 0.0,
            playing: false,
            speed: PlaybackSpeed::X1,
            show_travel: true,
            selected: None,
            selected_operation: None,
        }
    }
}

/// The decoded listing a user asked for, with the plan it belongs to.
#[derive(Debug, Clone)]
pub(super) struct RdListing {
    plan: JobPlanId,
    bytes: usize,
    records: Vec<RdRecordRow>,
    truncated: usize,
    error: Option<String>,
}

/// Longest list the RD inspector keeps (a plan with hundreds of thousands of records stays usable).
const MAX_RD_ROWS: usize = 50_000;

/// Where the clock stands inside the moves: how many are done, the one in progress and the head.
fn locate(moves: &[PreviewMove], position: f64) -> (usize, Option<PartialMove>, [f64; 2]) {
    let mut at = 0.0;
    let mut completed = 0;
    let mut partial = None;
    for (index, m) in moves.iter().enumerate() {
        if at + m.seconds <= position {
            at += m.seconds;
            completed = index + 1;
            continue;
        }
        let fraction = if m.seconds > 0.0 { (position - at) / m.seconds } else { 0.0 };
        if fraction > 0.0 {
            partial = Some(PartialMove { index, fraction });
        }
        break;
    }
    let point = |p: burnburn_job::MachinePoint| [p.x_mm(), p.y_mm()];
    let head = match (partial, completed.checked_sub(1), moves.first()) {
        (Some(p), _, _) => {
            let m = &moves[p.index];
            let (a, b) = (point(m.from), point(m.to));
            [a[0] + (b[0] - a[0]) * p.fraction, a[1] + (b[1] - a[1]) * p.fraction]
        }
        (None, Some(last), _) => point(moves[last].to),
        (None, None, Some(first)) => point(first.from),
        (None, None, None) => [0.0, 0.0],
    };
    (completed, partial, head)
}

/// Distance from `p` to the segment `a`–`b`.
fn distance_to_segment(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len2 = dx * dx + dy * dy;
    let t = if len2 > 0.0 { (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
    (p[0] - (a[0] + t * dx)).hypot(p[1] - (a[1] + t * dy))
}

impl AppController {
    /// The preview of `document`'s current plan; `None` while it has none.
    pub fn preview_view(&self, document: DocumentId) -> Option<PreviewViewModel> {
        let s = self.sessions.get(&document)?;
        let compiled = s.compiled.as_ref()?;
        let plan = super::process::plan_view(compiled, s, self.machine.as_ref());
        let state = self
            .app
            .previews
            .get(&document)
            .filter(|state| state.plan == plan.id)
            .cloned()
            .unwrap_or_else(|| PreviewState::new(plan.id));
        let (completed_moves, partial, head) = locate(&plan.preview.moves, state.position);
        Some(PreviewViewModel {
            plan: plan.id,
            stale: plan.stale,
            estimated_total_seconds: plan.preview.estimated_seconds,
            estimated_position_seconds: state.position,
            head,
            completed_moves,
            partial,
            playing: state.playing,
            speed: state.speed,
            show_travel: state.show_travel,
            selected: state.selected,
            selected_operation: state.selected_operation,
            bed_mm: self.machine.as_ref().map(|m| [m.profile.frame().width_mm(), m.profile.frame().height_mm()]),
            preview: plan.preview,
        })
    }

    /// The RD listing the user asked for, `None` before they asked.
    pub fn rd_inspect_view(&self, document: DocumentId) -> Option<RdInspectViewModel> {
        let listing = self.app.rd_listings.get(&document)?;
        let current =
            self.sessions.get(&document).and_then(|s| s.compiled.as_ref()).is_some_and(|c| c.plan.id() == listing.plan);
        Some(RdInspectViewModel {
            plan: listing.plan,
            current,
            bytes: listing.bytes,
            records: listing.records.clone(),
            truncated: listing.truncated,
            error: listing.error.clone(),
        })
    }

    /// The state to change: the window's own while it belongs to the current plan, else a new one.
    fn preview_state(&mut self, document: DocumentId) -> Option<&mut PreviewState> {
        let plan = self.sessions.get(&document)?.compiled.as_ref()?.plan.id();
        let state = self.app.previews.entry(document).or_insert_with(|| PreviewState::new(plan));
        if state.plan != plan {
            *state = PreviewState::new(plan);
        }
        Some(state)
    }

    pub(super) fn preview_intent(&mut self, intent: PreviewIntent) -> AppUpdate {
        // Windows that were closed take their preview state with them.
        let open: Vec<DocumentId> = self.sessions.keys().copied().collect();
        self.app.previews.retain(|d, _| open.contains(d));
        self.app.rd_listings.retain(|d, _| open.contains(d));
        let changed = AppUpdate { view_changed: true, ..AppUpdate::default() };
        match intent {
            PreviewIntent::Seek { document, seconds } => {
                let Some(total) = self.plan_seconds(document) else { return AppUpdate::default() };
                if let (Some(state), true) = (self.preview_state(document), seconds.is_finite()) {
                    state.position = seconds.clamp(0.0, total);
                }
                changed
            }
            PreviewIntent::Play { document, playing } => {
                let Some(total) = self.plan_seconds(document) else { return AppUpdate::default() };
                if let Some(state) = self.preview_state(document) {
                    // Play at the end starts over; an empty plan has nothing to play.
                    if playing && state.position >= total {
                        state.position = 0.0;
                    }
                    state.playing = playing && total > 0.0;
                }
                changed
            }
            PreviewIntent::Tick { document, seconds } => {
                let Some(total) = self.plan_seconds(document) else { return AppUpdate::default() };
                match self.preview_state(document) {
                    Some(state) if state.playing && seconds.is_finite() && seconds > 0.0 => {
                        state.position += seconds * state.speed.factor();
                        if state.position >= total {
                            state.position = total;
                            state.playing = false;
                        }
                        changed
                    }
                    _ => AppUpdate::default(),
                }
            }
            PreviewIntent::SetSpeed { document, speed } => {
                if let Some(state) = self.preview_state(document) {
                    state.speed = speed;
                }
                changed
            }
            PreviewIntent::ShowTravel { document, show } => {
                if let Some(state) = self.preview_state(document) {
                    state.show_travel = show;
                    if !show && state.selected.is_some_and(|p| p.source.is_none()) {
                        state.selected = None;
                    }
                }
                changed
            }
            PreviewIntent::Pick { document, at_mm, tolerance_mm } => self.pick(document, at_mm, tolerance_mm),
            PreviewIntent::SelectMove { document, index } => self.select_move(document, Some(index)),
            PreviewIntent::InspectRd { document } => self.inspect_rd(document),
        }
    }

    fn plan_seconds(&self, document: DocumentId) -> Option<f64> {
        Some(self.sessions.get(&document)?.compiled.as_ref()?.preview.estimated_seconds)
    }

    /// Click on the preview canvas: the nearest visible move within `tolerance_mm`, the later
    /// one on a tie (it is drawn on top). No move under the click clears the pick.
    fn pick(&mut self, document: DocumentId, at_mm: [f64; 2], tolerance_mm: f64) -> AppUpdate {
        if !(at_mm[0].is_finite() && at_mm[1].is_finite() && tolerance_mm.is_finite() && tolerance_mm > 0.0) {
            return AppUpdate::default();
        }
        let Some(view) = self.preview_view(document) else { return AppUpdate::default() };
        let best = view
            .preview
            .moves
            .iter()
            .enumerate()
            .filter(|(_, m)| view.show_travel || matches!(m.kind, MoveKind::Cut | MoveKind::Scan | MoveKind::Dot))
            .map(|(i, m)| (i, distance_to_segment(at_mm, [m.from.x_mm(), m.from.y_mm()], [m.to.x_mm(), m.to.y_mm()])))
            .filter(|(_, d)| *d <= tolerance_mm)
            .min_by(|(ia, da), (ib, db)| da.total_cmp(db).then(ib.cmp(ia)))
            .map(|(i, _)| i);
        self.select_move(document, best)
    }

    /// Marks one move as the picked one (or none) and, when it burns, selects its source.
    fn select_move(&mut self, document: DocumentId, index: Option<usize>) -> AppUpdate {
        let Some(view) = self.preview_view(document) else { return AppUpdate::default() };
        let picked = index.and_then(|i| {
            view.preview.moves.get(i).map(|m| PreviewSelection { index: i, kind: m.kind, source: m.source })
        });
        if index.is_some() && picked.is_none() {
            return AppUpdate::default();
        }
        if let Some(state) = self.preview_state(document) {
            state.selected = picked;
            state.selected_operation = picked.and_then(|p| p.source).map(|src| src.operation);
        }
        match picked.and_then(|p| p.source) {
            Some(source) => self.with_session(document, |s| {
                super::apply_selection(s, &SelectionChange::Replace(vec![source.geometry]));
                s.cycle = None;
                AppUpdate::redraw(document)
            }),
            None => AppUpdate { view_changed: true, ..AppUpdate::default() },
        }
    }

    /// Decodes the RD form of the current plan for the read-only inspector.
    fn inspect_rd(&mut self, document: DocumentId) -> AppUpdate {
        let Some(machine) = self.machine.clone() else { return AppUpdate::default() };
        let Some(plan) = self.sessions.get(&document).and_then(|s| s.compiled.as_ref()).map(|c| Arc::clone(&c.plan))
        else {
            return AppUpdate::default();
        };
        let mut listing = RdListing { plan: plan.id(), bytes: 0, records: Vec::new(), truncated: 0, error: None };
        let magic = machine.ruida.magic;
        match burnburn_ruida::lower(&plan, &machine.ruida) {
            Err(e) => listing.error = Some(e.to_string()),
            Ok(program) => match program.to_rd_file(magic).map_err(|e| format!("{e:?}")).and_then(|bytes| {
                let file = burnburn_ruida::decode_rd_file(&bytes, magic).map_err(|e| format!("{e:?}"))?;
                Ok((bytes.len(), file))
            }) {
                Err(e) => listing.error = Some(e),
                Ok((size, file)) => {
                    listing.bytes = size;
                    listing.truncated = file.records.len().saturating_sub(MAX_RD_ROWS);
                    listing.records = file
                        .records
                        .iter()
                        .take(MAX_RD_ROWS)
                        .map(|r| {
                            let raw = file.raw.get(r.offset..r.offset + r.len).unwrap_or(&[]);
                            RdRecordRow {
                                offset: r.offset,
                                hex: raw.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" "),
                                label: registry_label(raw),
                                decoded: format!("{:?}", r.command),
                            }
                        })
                        .collect();
                }
            },
        }
        self.app.rd_listings.insert(document, listing);
        AppUpdate { view_changed: true, ..AppUpdate::default() }
    }
}

/// The registry name of the longest opcode prefix `raw` starts with.
fn registry_label(raw: &[u8]) -> Option<&'static str> {
    burnburn_ruida::registry_entries()
        .iter()
        .filter(|(prefix, _)| raw.starts_with(prefix))
        .max_by_key(|(prefix, _)| prefix.len())
        .map(|(_, label)| *label)
}

#[derive(Debug, Clone, PartialEq)]
pub enum PreviewIntent {
    /// Scrub: put the clock at `seconds` (clamped to the plan).
    Seek {
        document: DocumentId,
        seconds: f64,
    },
    Play {
        document: DocumentId,
        playing: bool,
    },
    /// The host's timer: `seconds` of wall time passed; playback advances by `seconds × speed`.
    Tick {
        document: DocumentId,
        seconds: f64,
    },
    SetSpeed {
        document: DocumentId,
        speed: PlaybackSpeed,
    },
    ShowTravel {
        document: DocumentId,
        show: bool,
    },
    /// A click at a machine position (mm) with a hit radius (mm): picks the nearest visible move.
    Pick {
        document: DocumentId,
        at_mm: [f64; 2],
        tolerance_mm: f64,
    },
    /// A move chosen from a list instead of the canvas.
    SelectMove {
        document: DocumentId,
        index: usize,
    },
    /// Decode the RD records of the current plan (read-only).
    InspectRd {
        document: DocumentId,
    },
}
