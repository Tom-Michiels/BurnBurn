//! The JobPlan preview panel.

use burnburn_presentation::{AppViewModel, GrblRefusalKind, SelectMode, UiIntent};
use burnburn_types::{DocumentId, GeometryId};

use crate::l10n::{HostText, Locale, host_text};

/// One thing the GRBL backend refuses of the plan, in words, with the object to go to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrblRefusalRow {
    pub text: String,
    pub geometry: Option<GeometryId>,
}

/// The preview panel's texts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanPanel {
    pub title: String,
    /// Full plan identity (`plan-<hash>`), the same that RD export and upload name; empty without plan.
    pub plan_id: String,
    /// The identity as a tooltip ("Plan-id: plan-…"); the panel itself does not show the hash.
    pub id_tooltip: String,
    /// The "Copy id" button's title (the button is disabled without plan).
    pub copy_id_title: String,
    pub estimate: String,
    /// Current, or out of date — said in words, not only with a colour (X03).
    pub state: String,
    pub findings: String,
    /// What the GRBL backend refuses (empty on a Ruida machine); choosing a row goes to its object.
    pub grbl: Vec<GrblRefusalRow>,
    /// The measured values this plan used, one line each, with the proof they came from (W09).
    pub calibration: Vec<String>,
}

impl PlanPanel {
    /// Choosing refusal `index`: select the object it belongs to (a refusal of the plan as a whole
    /// has none).
    pub fn select_intent(&self, document: DocumentId, index: usize) -> Option<UiIntent> {
        let geometry = self.grbl.get(index)?.geometry?;
        Some(UiIntent::Select { document, ids: vec![geometry], mode: SelectMode::Replace })
    }
}

/// A GRBL refusal in words.
pub fn grbl_refusal_text(kind: &GrblRefusalKind, locale: Locale) -> String {
    let nl = locale == Locale::Nl;
    match kind {
        GrblRefusalKind::LaserModeRequired => {
            if nl {
                "Lasermodus ($32=1) staat niet aan in het profiel".to_owned()
            } else {
                "Laser mode ($32=1) is not on in the profile".to_owned()
            }
        }
        GrblRefusalKind::AirAssistUnavailable => {
            if nl {
                "Luchtondersteuning gevraagd, maar de machine heeft geen M8-uitgang".to_owned()
            } else {
                "Air assist requested, but the machine has no M8 output".to_owned()
            }
        }
        GrblRefusalKind::OutsideBed => {
            if nl {
                "Beweging buiten het bed".to_owned()
            } else {
                "Motion outside the bed".to_owned()
            }
        }
        GrblRefusalKind::InvalidProfile(what) => {
            if nl {
                format!("Ongeldig GRBL-profiel: {what}")
            } else {
                format!("Invalid GRBL profile: {what}")
            }
        }
        GrblRefusalKind::Feature(feature) => {
            let name = crate::l10n::feature_name(*feature, locale);
            if nl {
                format!("{name} wordt niet ondersteund door de GRBL-backend")
            } else {
                format!("{name} is not supported by the GRBL backend")
            }
        }
        GrblRefusalKind::Rotary => {
            if nl {
                "Rotary wordt door de GRBL-backend niet ondersteund".to_owned()
            } else {
                "Rotary is not supported by the GRBL backend".to_owned()
            }
        }
        GrblRefusalKind::NotRepresentable(what) => {
            if nl {
                format!("{what} kan niet als G-code worden geschreven")
            } else {
                format!("{what} cannot be written as G-code")
            }
        }
        _ => {
            if nl {
                "De GRBL-backend weigert dit plan".to_owned()
            } else {
                "The GRBL backend refuses this plan".to_owned()
            }
        }
    }
}

pub fn plan_panel(vm: &AppViewModel, document: DocumentId, locale: Locale) -> PlanPanel {
    let plan = vm.document(document).and_then(|d| d.plan.as_ref());
    let Some(plan) = plan else {
        return PlanPanel {
            title: host_text(HostText::NoPlan, locale).to_owned(),
            plan_id: String::new(),
            id_tooltip: String::new(),
            copy_id_title: copy_plan_id_title(locale).to_owned(),
            estimate: String::new(),
            state: host_text(HostText::NoPlanHint, locale).to_owned(),
            findings: String::new(),
            grbl: Vec::new(),
            calibration: Vec::new(),
        };
    };
    let state = if plan.stale { HostText::PlanStale } else { HostText::PlanCurrent };
    PlanPanel {
        title: plan_title(plan.compiled_at, plan.preview.estimated_seconds, locale),
        plan_id: plan.id.to_string(),
        id_tooltip: match locale {
            Locale::Nl => format!("Plan-id: {}", plan.id),
            Locale::En => format!("Plan id: {}", plan.id),
        },
        copy_id_title: copy_plan_id_title(locale).to_owned(),
        estimate: estimate_text(plan.preview.estimated_seconds, locale),
        state: host_text(state, locale).to_owned(),
        findings: findings_text(plan.errors, plan.warnings, locale),
        grbl: plan
            .grbl
            .iter()
            .map(|f| GrblRefusalRow { text: grbl_refusal_text(&f.kind, locale), geometry: f.geometry })
            .collect(),
        calibration: calibration_lines(&plan.calibration, locale),
    }
}

/// The measured values a plan used, one line each, with the proofs they came from (W09). Equal
/// lines (the same value for several operations) are shown once.
pub fn calibration_lines(applied: &[burnburn_presentation::AppliedCalibration], locale: Locale) -> Vec<String> {
    use burnburn_presentation::AppliedKind;
    let nl = locale == Locale::Nl;
    let number = |v: f64| {
        let text = format!("{v}");
        if locale.decimal_sign() == ',' { text.replace('.', ",") } else { text }
    };
    let mut lines: Vec<String> = Vec::new();
    for a in applied {
        let proofs = a.proofs.iter().map(|p| format!("{} {} {}", p.id, if nl { "van" } else { "of" }, p.date));
        let proofs = proofs.collect::<Vec<_>>().join(", ");
        let label = match (nl, a.proofs.len() == 1) {
            (true, true) => "proef",
            (true, false) => "proeven",
            (false, true) => "proof",
            (false, false) => "proofs",
        };
        let line = match &a.kind {
            AppliedKind::Kerf { material } => {
                let word = if nl { "voor" } else { "for" };
                format!("Kerf {} mm {word} {material} ({label} {proofs})", number(a.value_mm))
            }
            AppliedKind::ScanOffset { speed_mm_s } => {
                let word = if nl { "Scanoffset" } else { "Scan offset" };
                let at = if nl { "bij" } else { "at" };
                format!("{word} {} mm {at} {} mm/s ({label} {proofs})", number(a.value_mm), number(*speed_mm_s))
            }
        };
        if !lines.contains(&line) {
            lines.push(line);
        }
    }
    lines
}

fn findings_text(errors: usize, warnings: usize, locale: Locale) -> String {
    use crate::format::{Noun, format_count};
    format!("{}, {}", format_count(errors, Noun::Error, locale), format_count(warnings, Noun::Warning, locale))
}

/// Title of the button that copies the plan id.
pub fn copy_plan_id_title(locale: Locale) -> &'static str {
    match locale {
        Locale::Nl => "Kopieer id",
        Locale::En => "Copy id",
    }
}

/// "Plan van 18-06-2026 05:04 · ≈ 1,6 s": the plan's own compile moment (job clock, its fixed zone)
/// and its estimated duration. Without a usable clock the plain panel title: a missing time is
/// never replaced by a guess.
pub fn plan_title(compiled_at: Option<burnburn_presentation::JobClock>, seconds: f64, locale: Locale) -> String {
    let plain = || host_text(HostText::PlanTitle, locale).to_owned();
    let Some(clock) = compiled_at else { return plain() };
    let (stamp, word) = match locale {
        Locale::Nl => (clock.format("%d-%m-%Y %H:%M"), "Plan van"),
        Locale::En => (clock.format("%Y-%m-%d %H:%M"), "Plan of"),
    };
    let Ok(stamp) = stamp else { return plain() };
    let body = estimate_text(seconds, locale);
    // The title carries the figure only; the line below says it is an estimate.
    let duration = body.split(" (").next().unwrap_or(&body).to_owned();
    format!("{word} {stamp} \u{b7} {duration}")
}

/// The plan's constant-velocity time estimate, always labelled as an estimate (it is not a
/// measured duration, `PlanPreview::estimated_seconds`).
pub fn estimate_text(seconds: f64, locale: Locale) -> String {
    if !(seconds.is_finite() && seconds >= 0.0) {
        return host_text(HostText::PlanEstimateUnknown, locale).to_owned();
    }
    let label = match locale {
        Locale::Nl => "schatting",
        Locale::En => "estimate",
    };
    let body = if seconds < 60.0 {
        let tenths = format!("{seconds:.1}");
        let tenths = if locale.decimal_sign() == ',' { tenths.replace('.', ",") } else { tenths };
        format!("{tenths} s")
    } else {
        let total = seconds.round() as u64;
        if total < 3600 {
            format!("{} min {:02} s", total / 60, total % 60)
        } else {
            let minutes = (seconds / 60.0).round() as u64;
            format!("{} h {:02} min", minutes / 60, minutes % 60)
        }
    };
    format!("≈ {body} ({label})")
}
