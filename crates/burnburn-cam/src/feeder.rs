//! Feeder planning (FE01): repeat a compiled job, advance the material before and after it and
//! pause between the repeats. A pure step from one frozen [`JobPlan`] to another; the Ruida
//! backend has no proven form for any of it and refuses such a plan (typed), it never guesses.

use std::fmt;

use burnburn_job::{JobPlan, JobPlanBuilder, PlanError, PlanEvent};
use burnburn_types::{CancellationToken, Cancelled, Length};

/// What the feeder does around the job.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FeederSettings {
    repeat: u32,
    feed_before_mm: f64,
    feed_after_mm: f64,
    pause_seconds: f64,
    feed_speed_mm_s: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum FeederError {
    Cancelled,
    OutOfRange {
        field: &'static str,
    },
    /// The repeated plan would hold more events than [`MAX_EVENTS`].
    TooLarge {
        events: u64,
    },
    Plan(PlanError),
}

impl From<Cancelled> for FeederError {
    fn from(_: Cancelled) -> Self {
        FeederError::Cancelled
    }
}

impl From<PlanError> for FeederError {
    fn from(e: PlanError) -> Self {
        FeederError::Plan(e)
    }
}

impl fmt::Display for FeederError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FeederError::Cancelled => f.write_str("feeder planning cancelled"),
            FeederError::OutOfRange { field } => write!(f, "feeder {field} is out of range"),
            FeederError::TooLarge { events } => write!(f, "the repeated plan would hold {events} events"),
            FeederError::Plan(e) => write!(f, "plan: {e}"),
        }
    }
}

impl std::error::Error for FeederError {}

pub const MAX_REPEAT: u32 = 1000;
pub const MAX_FEED_MM: f64 = 10_000.0;
pub const MAX_PAUSE_SECONDS: f64 = 3600.0;
pub const MAX_FEED_SPEED_MM_S: f64 = 1000.0;
/// Upper bound on the events of a repeated plan.
pub const MAX_EVENTS: u64 = 10_000_000;

impl FeederSettings {
    pub fn new(
        repeat: u32,
        feed_before: Length,
        feed_after: Length,
        pause_seconds: f64,
        feed_speed_mm_s: f64,
    ) -> Result<Self, FeederError> {
        let out_of_range = |field| FeederError::OutOfRange { field };
        if !(1..=MAX_REPEAT).contains(&repeat) {
            return Err(out_of_range("repeat"));
        }
        let feed = |l: Length, field| {
            let mm = l.mm();
            if (0.0..=MAX_FEED_MM).contains(&mm) { Ok(mm) } else { Err(out_of_range(field)) }
        };
        let (feed_before_mm, feed_after_mm) = (feed(feed_before, "feed before")?, feed(feed_after, "feed after")?);
        if !(0.0..=MAX_PAUSE_SECONDS).contains(&pause_seconds) {
            return Err(out_of_range("pause"));
        }
        if !(feed_speed_mm_s > 0.0 && feed_speed_mm_s <= MAX_FEED_SPEED_MM_S) {
            return Err(out_of_range("feed speed"));
        }
        Ok(FeederSettings { repeat, feed_before_mm, feed_after_mm, pause_seconds, feed_speed_mm_s })
    }

    pub fn repeat(&self) -> u32 {
        self.repeat
    }
    pub fn feed_before_mm(&self) -> f64 {
        self.feed_before_mm
    }
    pub fn feed_after_mm(&self) -> f64 {
        self.feed_after_mm
    }
    pub fn pause_seconds(&self) -> f64 {
        self.pause_seconds
    }
    pub fn feed_speed_mm_s(&self) -> f64 {
        self.feed_speed_mm_s
    }

    /// Nothing to do: one run, no feeds, no pause.
    pub fn is_plain(&self) -> bool {
        self.repeat == 1 && self.feed_before_mm == 0.0 && self.feed_after_mm == 0.0
    }
}

impl Default for FeederSettings {
    fn default() -> Self {
        FeederSettings { repeat: 1, feed_before_mm: 0.0, feed_after_mm: 0.0, pause_seconds: 0.0, feed_speed_mm_s: 50.0 }
    }
}

/// `plan` repeated and wrapped as `settings` say.
pub fn plan_feeder(
    plan: &JobPlan,
    settings: &FeederSettings,
    cancel: &CancellationToken,
) -> Result<JobPlan, FeederError> {
    cancel.check()?;
    let repeat = u64::from(settings.repeat);
    let body = plan.events().len() as u64;
    // Per repeat: the job, up to two feeds and up to one pause.
    let events = repeat.saturating_mul(body.saturating_add(3));
    if events > MAX_EVENTS {
        return Err(FeederError::TooLarge { events });
    }
    let mut b = JobPlanBuilder::new(plan.source().clone(), *plan.frame(), plan.start());
    if let Some(rotary) = plan.rotary() {
        b = b.with_rotary(*rotary);
    }
    for s in plan.settings() {
        b.add_settings(*s)?;
    }
    let feed = |mm: f64| PlanEvent::Feed { mm, speed_mm_s: settings.feed_speed_mm_s };
    for r in 0..settings.repeat {
        cancel.check()?;
        if settings.feed_before_mm > 0.0 {
            b.push(feed(settings.feed_before_mm))?;
        }
        for (i, e) in plan.events().iter().enumerate() {
            if i % CANCEL_EVERY == 0 {
                cancel.check()?;
            }
            b.push(*e)?;
        }
        if settings.feed_after_mm > 0.0 {
            b.push(feed(settings.feed_after_mm))?;
        }
        if settings.pause_seconds > 0.0 && r + 1 < settings.repeat {
            b.push(PlanEvent::Pause { seconds: settings.pause_seconds })?;
        }
    }
    Ok(b.finish()?)
}

/// How many events are copied between two looks at the cancellation token.
const CANCEL_EVERY: usize = 4096;
