//! Laser ▸ Feeder (FE01, G5-cam): repeat, feed before, feed after, pause between the runs and the
//! feed speed. The settings are window state (never project data, never part of undo). The
//! compile task applies them to the plan it makes, so preview, preflight and an RD attempt all
//! refer to the same fed plan; RD output of a plan with feeds or pauses is refused in words.

use std::collections::BTreeMap;

use burnburn_cam::{FeederSettings, MAX_FEED_MM, MAX_FEED_SPEED_MM_S, MAX_PAUSE_SECONDS, MAX_REPEAT};
use burnburn_types::{DocumentId, Length, TaskId};

use crate::cam_dialogs::{
    CamDialog, DialogAction, DialogActionKind, DialogActionState, DialogError, DialogField, DialogNote, DialogRow,
    DialogValue, DialogViewModel,
};
use crate::cut_options::FieldState;
use crate::field::{format_mm, parse_number};

use super::{AppUpdate, Session};

/// The feeder in force, the draft of the open dialog and the settings each running compile
/// task was started with.
#[derive(Debug, Clone, Default)]
pub(super) struct FeederState {
    settings: FeederSettings,
    draft: Option<BTreeMap<DialogField, String>>,
    /// Compile tasks that run with a feeder; the answer is stored with the plan so a later change
    /// of the settings can tell the plan is out of date.
    fed: BTreeMap<TaskId, FeederSettings>,
}

const FIELDS: [DialogField; 5] = [
    DialogField::FeederRepeat,
    DialogField::FeederFeedBefore,
    DialogField::FeederFeedAfter,
    DialogField::FeederPause,
    DialogField::FeederSpeed,
];

impl FeederState {
    /// The settings a compile task starts with: `None` for a plain feeder (nothing to apply).
    pub(super) fn for_compile(&self) -> Option<FeederSettings> {
        (!self.settings.is_plain()).then_some(self.settings)
    }

    /// Remembers what compile task `id` was started with.
    pub(super) fn bind_task(&mut self, id: TaskId, settings: FeederSettings) {
        self.fed.insert(id, settings);
    }

    /// What compile task `id` was started with (and forgets it).
    pub(super) fn take_task(&mut self, id: TaskId) -> Option<FeederSettings> {
        self.fed.remove(&id)
    }

    /// Whether a plan made with `made` (None: without a feeder) is still the plan the feeder in
    /// force would make.
    pub(super) fn matches(&self, made: Option<FeederSettings>) -> bool {
        self.for_compile() == made.filter(|m| !m.is_plain())
    }
}

fn texts_of(settings: &FeederSettings) -> BTreeMap<DialogField, String> {
    [
        (DialogField::FeederRepeat, settings.repeat().to_string()),
        (DialogField::FeederFeedBefore, format_mm(settings.feed_before_mm())),
        (DialogField::FeederFeedAfter, format_mm(settings.feed_after_mm())),
        (DialogField::FeederPause, format_mm(settings.pause_seconds())),
        (DialogField::FeederSpeed, format_mm(settings.feed_speed_mm_s())),
    ]
    .into_iter()
    .collect()
}

/// Parses the draft: the settings, or every field that cannot be used.
fn parse(draft: &BTreeMap<DialogField, String>) -> Result<FeederSettings, Vec<(DialogField, DialogError)>> {
    let text = |f: DialogField| draft.get(&f).map_or("", String::as_str);
    let mut errors = Vec::new();
    let mut number = |f: DialogField, suffixes: &[&str]| match parse_number(text(f), suffixes) {
        Ok(v) if v.is_finite() => Some(v),
        _ => {
            errors.push((f, DialogError::Invalid));
            None
        }
    };
    let repeat = number(DialogField::FeederRepeat, &[]);
    let before = number(DialogField::FeederFeedBefore, &["mm"]);
    let after = number(DialogField::FeederFeedAfter, &["mm"]);
    let pause = number(DialogField::FeederPause, &["s"]);
    let speed = number(DialogField::FeederSpeed, &["mm/s"]);
    if let Some(r) = repeat
        && r.fract() != 0.0
    {
        errors.push((DialogField::FeederRepeat, DialogError::NotWhole));
    }
    // Ranges are checked per field so every bad row is said at once.
    let range = |v: Option<f64>, f: DialogField, lo: f64, hi: f64, errors: &mut Vec<(DialogField, DialogError)>| {
        if v.is_some_and(|v| !(lo..=hi).contains(&v)) && !errors.iter().any(|(e, _)| *e == f) {
            errors.push((f, DialogError::OutOfRange));
        }
    };
    range(repeat, DialogField::FeederRepeat, 1.0, f64::from(MAX_REPEAT), &mut errors);
    range(before, DialogField::FeederFeedBefore, 0.0, MAX_FEED_MM, &mut errors);
    range(after, DialogField::FeederFeedAfter, 0.0, MAX_FEED_MM, &mut errors);
    range(pause, DialogField::FeederPause, 0.0, MAX_PAUSE_SECONDS, &mut errors);
    if speed.is_some_and(|v| !(v > 0.0 && v <= MAX_FEED_SPEED_MM_S)) {
        errors.push((DialogField::FeederSpeed, DialogError::OutOfRange));
    }
    let (Some(repeat), Some(before), Some(after), Some(pause), Some(speed)) = (repeat, before, after, pause, speed)
    else {
        return Err(errors);
    };
    if !errors.is_empty() {
        return Err(errors);
    }
    let lengths = (Length::from_mm(before), Length::from_mm(after));
    let (Ok(before), Ok(after)) = lengths else {
        return Err(vec![(DialogField::FeederFeedBefore, DialogError::OutOfRange)]);
    };
    FeederSettings::new(repeat as u32, before, after, pause, speed)
        .map_err(|_| vec![(DialogField::FeederRepeat, DialogError::OutOfRange)])
}

fn changed() -> AppUpdate {
    AppUpdate { view_changed: true, ..AppUpdate::default() }
}

pub(super) fn view(s: &Session) -> Option<DialogViewModel> {
    let draft = s.cam.feeder.draft.as_ref()?;
    let parsed = parse(draft);
    let errors: BTreeMap<DialogField, DialogError> = match &parsed {
        Err(e) => e.iter().copied().collect(),
        Ok(_) => BTreeMap::new(),
    };
    let rows = FIELDS
        .iter()
        .map(|field| DialogRow {
            field: *field,
            value: DialogValue::Text(draft.get(field).cloned().unwrap_or_default()),
            state: FieldState::Editable,
            error: errors.get(field).copied(),
            blocked_choices: Vec::new(),
            reorder: None,
            labels: Vec::new(),
        })
        .collect();
    let mut notes = Vec::new();
    if parsed.as_ref().is_ok_and(|p| !p.is_plain()) {
        notes.push(DialogNote::FeederHasNoRdForm);
    }
    Some(DialogViewModel {
        dialog: CamDialog::Feeder,
        rows,
        notes,
        actions: vec![
            DialogActionState { action: DialogActionKind::Apply, enabled: parsed.is_ok(), reason: None },
            DialogActionState { action: DialogActionKind::Close, enabled: true, reason: None },
        ],
        capabilities: Vec::new(),
        result: None,
        busy: false,
    })
}

pub(super) fn open(s: &mut Session) -> AppUpdate {
    s.cam.feeder.draft = Some(texts_of(&s.cam.feeder.settings));
    changed()
}

pub(super) fn set(s: &mut Session, field: DialogField, value: DialogValue) -> AppUpdate {
    let (Some(draft), DialogValue::Text(t)) = (s.cam.feeder.draft.as_mut(), value) else {
        return AppUpdate::default();
    };
    if !FIELDS.contains(&field) {
        return AppUpdate::default();
    }
    draft.insert(field, t);
    changed()
}

pub(super) fn action(s: &mut Session, document: DocumentId, action: DialogAction) -> AppUpdate {
    match action {
        DialogAction::Close => {
            s.cam.feeder.draft = None;
            AppUpdate::redraw(document)
        }
        DialogAction::Apply => {
            let Some(draft) = s.cam.feeder.draft.as_ref() else { return AppUpdate::default() };
            let Ok(settings) = parse(draft) else { return changed() };
            s.cam.feeder.settings = settings;
            s.cam.feeder.draft = None;
            // The plan on screen was made with other settings: it shows as out of date.
            AppUpdate::redraw(document)
        }
        _ => AppUpdate::default(),
    }
}
