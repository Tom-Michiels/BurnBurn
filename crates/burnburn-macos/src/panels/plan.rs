//! The JobPlan preview panel.

use burnburn_presentation::AppViewModel;
use burnburn_types::DocumentId;

use crate::l10n::{HostText, Locale, host_text};

/// The preview panel's texts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanPanel {
    pub title: String,
    /// Full plan identity (`plan-<hash>`), the same that RD export and upload name; empty without plan.
    pub plan_id: String,
    pub estimate: String,
    /// Current, or out of date — said in words, not only with a colour (X03).
    pub state: String,
    pub findings: String,
}

pub fn plan_panel(vm: &AppViewModel, document: DocumentId, locale: Locale) -> PlanPanel {
    let plan = vm.document(document).and_then(|d| d.plan.as_ref());
    let Some(plan) = plan else {
        return PlanPanel {
            title: host_text(HostText::NoPlan, locale).to_owned(),
            plan_id: String::new(),
            estimate: String::new(),
            state: host_text(HostText::NoPlanHint, locale).to_owned(),
            findings: String::new(),
        };
    };
    let state = if plan.stale { HostText::PlanStale } else { HostText::PlanCurrent };
    PlanPanel {
        title: host_text(HostText::PlanTitle, locale).to_owned(),
        plan_id: plan.id.to_string(),
        estimate: estimate_text(plan.preview.estimated_seconds, locale),
        state: host_text(state, locale).to_owned(),
        findings: findings_text(plan.errors, plan.warnings, locale),
    }
}

fn findings_text(errors: usize, warnings: usize, locale: Locale) -> String {
    match locale {
        Locale::Nl => format!(
            "{errors} {}, {warnings} {}",
            if errors == 1 { "fout" } else { "fouten" },
            if warnings == 1 { "waarschuwing" } else { "waarschuwingen" }
        ),
        Locale::En => format!(
            "{errors} {}, {warnings} {}",
            if errors == 1 { "error" } else { "errors" },
            if warnings == 1 { "warning" } else { "warnings" }
        ),
    }
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
