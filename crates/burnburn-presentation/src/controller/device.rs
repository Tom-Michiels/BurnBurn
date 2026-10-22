//! Device Settings: the machine profile dialog with Rotary Setup (W3-UI-cam, M01, M02, M08).
//!
//! The dialog edits a *draft*. Apply builds a new [`MachineProfile`] from it — a new revision of
//! the same machine, with the capability matrix the profile already had (the list is shown, not
//! edited: support needs evidence for the concrete machine) — and makes it the active profile.
//! Compiled plans then show as out of date; the document, its revision and its undo history are
//! untouched, and nothing here talks to a machine.

use std::collections::BTreeMap;
use std::sync::Arc;

use burnburn_gcode::{GrblProfile, PowerMode, WorkCoordinate, grbl_capabilities};
use burnburn_job::{
    Feature, MachineCapabilities, MachineFrame, MachineProfile, OriginCorner, PowerRatio, RotaryConfig, RotaryKind,
    Support,
};
use burnburn_ruida::{Magic, OverscanPolicy, RuidaProfile};
use burnburn_types::DocumentId;

use crate::cam_dialogs::{
    CamDialog, CapabilityRow, DialogAction, DialogActionKind, DialogActionState, DialogChoice, DialogError,
    DialogField, DialogNote, DialogRow, DialogValue, DialogViewModel, SupportView,
};
use crate::cut_options::FieldState;
use crate::field::{format_mm, parse_length, parse_number};

use super::machine::Machine;
use super::{AppController, AppUpdate};

/// Id of a profile that was created in the dialog.
const NEW_PROFILE_ID: &str = "burnburn-machine";
/// Start values of a first profile (the user adjusts them; they are configuration, not a claim).
const NEW_BED: (&str, &str) = ("600", "400");
const NEW_MAX_SPEED: &str = "400";
const NEW_TRAVEL_SPEED: &str = "400";

/// The draft of the Device Settings dialog (app-wide, like the profile it edits).
#[derive(Default)]
pub(super) struct DeviceState {
    draft: Option<DeviceDraft>,
}

struct DeviceDraft {
    values: BTreeMap<DialogField, DialogValue>,
    /// What the fields showed when the dialog opened: an untouched field keeps the profile's own
    /// number, so the three-decimal display text never rounds a stored value.
    opened: BTreeMap<DialogField, DialogValue>,
    errors: BTreeMap<DialogField, DialogError>,
}

fn text(s: impl Into<String>) -> DialogValue {
    DialogValue::Text(s.into())
}

fn choice(field: DialogField, which: DialogChoice) -> DialogValue {
    DialogValue::Choice(field.choices().iter().position(|c| *c == which).unwrap_or(0))
}

fn origin_choice(origin: OriginCorner) -> DialogChoice {
    match origin {
        OriginCorner::BottomLeft => DialogChoice::OriginBottomLeft,
        OriginCorner::TopLeft => DialogChoice::OriginTopLeft,
        OriginCorner::TopRight => DialogChoice::OriginTopRight,
        OriginCorner::BottomRight => DialogChoice::OriginBottomRight,
    }
}

fn origin_of(choice: Option<DialogChoice>) -> OriginCorner {
    match choice {
        Some(DialogChoice::OriginTopLeft) => OriginCorner::TopLeft,
        Some(DialogChoice::OriginTopRight) => OriginCorner::TopRight,
        Some(DialogChoice::OriginBottomRight) => OriginCorner::BottomRight,
        _ => OriginCorner::BottomLeft,
    }
}

const WORK_COORDINATES: [(DialogChoice, Option<WorkCoordinate>); 7] = [
    (DialogChoice::WorkCoordNone, None),
    (DialogChoice::WorkCoordG54, Some(WorkCoordinate::G54)),
    (DialogChoice::WorkCoordG55, Some(WorkCoordinate::G55)),
    (DialogChoice::WorkCoordG56, Some(WorkCoordinate::G56)),
    (DialogChoice::WorkCoordG57, Some(WorkCoordinate::G57)),
    (DialogChoice::WorkCoordG58, Some(WorkCoordinate::G58)),
    (DialogChoice::WorkCoordG59, Some(WorkCoordinate::G59)),
];

fn work_coordinate_choice(wc: Option<WorkCoordinate>) -> DialogChoice {
    WORK_COORDINATES.iter().find(|(_, w)| *w == wc).map_or(DialogChoice::WorkCoordNone, |(c, _)| *c)
}

fn work_coordinate_of(choice: Option<DialogChoice>) -> Option<WorkCoordinate> {
    WORK_COORDINATES.iter().find(|(c, _)| Some(*c) == choice).and_then(|(_, w)| *w)
}

/// The fields that only count for the GRBL backend.
const GRBL_FIELDS: [DialogField; 6] = [
    DialogField::GrblMaxSpindle,
    DialogField::GrblLaserMode,
    DialogField::GrblPowerMode,
    DialogField::GrblAirAssist,
    DialogField::GrblWorkCoordinate,
    DialogField::GrblDecimals,
];

impl DeviceDraft {
    fn of(profile: Option<&MachineProfile>, grbl: Option<&GrblProfile>) -> Self {
        let mut v: BTreeMap<DialogField, DialogValue> = BTreeMap::new();
        match profile {
            Some(p) => {
                let flat = p.flatbed_frame();
                v.insert(DialogField::BedWidth, text(format_mm(flat.width_mm())));
                v.insert(DialogField::BedHeight, text(format_mm(flat.height_mm())));
                v.insert(DialogField::BedOrigin, choice(DialogField::BedOrigin, origin_choice(flat.origin())));
                v.insert(DialogField::MaxSpeed, text(format_mm(p.max_speed_mm_s())));
                v.insert(DialogField::TravelSpeed, text(format_mm(p.travel_speed_mm_s())));
                let (min, max) = p.power_range();
                v.insert(DialogField::MinPower, text(format_mm(min.get() * 100.0)));
                v.insert(DialogField::MaxPower, text(format_mm(max.get() * 100.0)));
            }
            None => {
                v.insert(DialogField::BedWidth, text(NEW_BED.0));
                v.insert(DialogField::BedHeight, text(NEW_BED.1));
                v.insert(DialogField::BedOrigin, choice(DialogField::BedOrigin, DialogChoice::OriginBottomLeft));
                v.insert(DialogField::MaxSpeed, text(NEW_MAX_SPEED));
                v.insert(DialogField::TravelSpeed, text(NEW_TRAVEL_SPEED));
                v.insert(DialogField::MinPower, text("0"));
                v.insert(DialogField::MaxPower, text("100"));
            }
        }
        // The rotary fields start empty: the Y scale of a machine is read off its controller.
        let rotary = profile.and_then(MachineProfile::rotary);
        v.insert(DialogField::RotaryEnabled, DialogValue::Flag(rotary.is_some()));
        let (kind, diameter) = match rotary.map(RotaryConfig::kind) {
            Some(RotaryKind::Roller { roller_diameter_mm }) => (DialogChoice::RotaryRoller, roller_diameter_mm),
            Some(RotaryKind::Chuck { object_diameter_mm }) => (DialogChoice::RotaryChuck, object_diameter_mm),
            None => (DialogChoice::RotaryChuck, 0.0),
        };
        v.insert(DialogField::RotaryKind, choice(DialogField::RotaryKind, kind));
        v.insert(DialogField::RotaryDiameter, text(rotary.map_or(String::new(), |_| format_mm(diameter))));
        v.insert(
            DialogField::StepsPerRotation,
            text(rotary.map_or(String::new(), |r| r.steps_per_rotation().to_string())),
        );
        v.insert(DialogField::YStepsPerMm, text(rotary.map_or(String::new(), |r| format_mm(r.y_steps_per_mm()))));
        v.insert(DialogField::RotaryMirror, DialogValue::Flag(rotary.is_some_and(RotaryConfig::mirror)));
        // The GRBL fields start with the stock laser configuration until the profile has its own.
        let stock = GrblProfile::grbl_1_1_laser();
        let g = grbl.unwrap_or(&stock);
        v.insert(
            DialogField::Backend,
            choice(
                DialogField::Backend,
                if grbl.is_some() { DialogChoice::BackendGrbl } else { DialogChoice::BackendRuida },
            ),
        );
        v.insert(DialogField::GrblMaxSpindle, text(g.max_spindle.to_string()));
        v.insert(DialogField::GrblLaserMode, DialogValue::Flag(g.laser_mode));
        v.insert(
            DialogField::GrblPowerMode,
            choice(
                DialogField::GrblPowerMode,
                match g.power_mode {
                    PowerMode::Constant => DialogChoice::PowerConstantM3,
                    PowerMode::Dynamic => DialogChoice::PowerDynamicM4,
                },
            ),
        );
        v.insert(DialogField::GrblAirAssist, DialogValue::Flag(g.air_assist));
        v.insert(
            DialogField::GrblWorkCoordinate,
            choice(DialogField::GrblWorkCoordinate, work_coordinate_choice(g.work_coordinate)),
        );
        v.insert(DialogField::GrblDecimals, text(g.decimals.to_string()));
        DeviceDraft { opened: v.clone(), values: v, errors: BTreeMap::new() }
    }

    fn is_grbl(&self) -> bool {
        self.values.get(&DialogField::Backend).and_then(|v| v.choice(DialogField::Backend))
            == Some(DialogChoice::BackendGrbl)
    }

    /// The GRBL settings the draft describes; every unusable field gets its error.
    fn grbl(&self, errors: &mut BTreeMap<DialogField, DialogError>) -> Option<GrblProfile> {
        let mut whole = |field: DialogField, min: f64, max: f64| -> Option<u32> {
            match parse_number(self.text(field), &[]) {
                Ok(v) if v.fract() != 0.0 => {
                    errors.insert(field, DialogError::NotWhole);
                    None
                }
                Ok(v) if (min..=max).contains(&v) => Some(v as u32),
                Ok(_) => {
                    errors.insert(field, DialogError::OutOfRange);
                    None
                }
                Err(_) => {
                    errors.insert(field, DialogError::Invalid);
                    None
                }
            }
        };
        let max_spindle = whole(DialogField::GrblMaxSpindle, 1.0, f64::from(u32::MAX));
        let decimals = whole(DialogField::GrblDecimals, 1.0, 4.0);
        let (max_spindle, decimals) = (max_spindle?, decimals?);
        let power_mode =
            match self.values.get(&DialogField::GrblPowerMode).and_then(|v| v.choice(DialogField::GrblPowerMode)) {
                Some(DialogChoice::PowerConstantM3) => PowerMode::Constant,
                _ => PowerMode::Dynamic,
            };
        let profile = GrblProfile {
            max_spindle,
            laser_mode: self.flag(DialogField::GrblLaserMode),
            power_mode,
            air_assist: self.flag(DialogField::GrblAirAssist),
            work_coordinate: work_coordinate_of(
                self.values
                    .get(&DialogField::GrblWorkCoordinate)
                    .and_then(|v| v.choice(DialogField::GrblWorkCoordinate)),
            ),
            decimals: decimals as u8,
        };
        profile.validate().ok()?;
        Some(profile)
    }

    fn text(&self, field: DialogField) -> &str {
        self.values.get(&field).map_or("", DialogValue::text)
    }

    fn flag(&self, field: DialogField) -> bool {
        self.values.get(&field).is_some_and(DialogValue::flag)
    }

    fn untouched(&self, field: DialogField) -> bool {
        self.values.get(&field) == self.opened.get(&field)
    }

    /// The profile this draft describes, as a revision `counter` of the machine `base` (or of a
    /// new one); every field that cannot be used gets its error.
    fn build(
        &self,
        base: Option<&MachineProfile>,
        base_is_grbl: bool,
        counter: u32,
    ) -> Result<(MachineProfile, Option<GrblProfile>), BTreeMap<DialogField, DialogError>> {
        let mut errors = BTreeMap::new();
        // A field the user did not touch keeps the profile's own number.
        let keep = |field: DialogField, own: Option<f64>| own.filter(|_| self.untouched(field));
        let flat = base.map(MachineProfile::flatbed_frame);
        let mut length = |field: DialogField, own: Option<f64>| -> f64 {
            if let Some(v) = keep(field, own) {
                return v;
            }
            match parse_length(self.text(field)) {
                Ok(l) => l.mm(),
                Err(_) => {
                    errors.insert(field, DialogError::Invalid);
                    1.0
                }
            }
        };
        let width = length(DialogField::BedWidth, flat.map(MachineFrame::width_mm));
        let height = length(DialogField::BedHeight, flat.map(MachineFrame::height_mm));
        let mut speed = |field: DialogField, own: Option<f64>| -> f64 {
            if let Some(v) = keep(field, own) {
                return v;
            }
            match parse_number(self.text(field), &["mm/s"]) {
                Ok(v) if v > 0.0 && v.is_finite() => v,
                _ => {
                    errors.insert(field, DialogError::Invalid);
                    1.0
                }
            }
        };
        let max_speed = speed(DialogField::MaxSpeed, base.map(MachineProfile::max_speed_mm_s));
        let travel_speed = speed(DialogField::TravelSpeed, base.map(MachineProfile::travel_speed_mm_s));
        let mut power = |field: DialogField, own: Option<f64>| -> f64 {
            if let Some(v) = keep(field, own) {
                return v;
            }
            match parse_number(self.text(field), &["%"]) {
                Ok(v) if (0.0..=100.0).contains(&v) => v / 100.0,
                Ok(_) => {
                    errors.insert(field, DialogError::OutOfRange);
                    0.0
                }
                Err(_) => {
                    errors.insert(field, DialogError::Invalid);
                    0.0
                }
            }
        };
        let own_power = base.map(MachineProfile::power_range);
        let min_power = power(DialogField::MinPower, own_power.map(|(min, _)| min.get()));
        let max_power = power(DialogField::MaxPower, own_power.map(|(_, max)| max.get()));
        if !errors.contains_key(&DialogField::MinPower)
            && !errors.contains_key(&DialogField::MaxPower)
            && min_power > max_power
        {
            errors.insert(DialogField::MinPower, DialogError::OutOfRange);
        }
        let rotary = if self.flag(DialogField::RotaryEnabled) { self.rotary(base, &mut errors) } else { None };
        let grbl = if self.is_grbl() { self.grbl(&mut errors) } else { None };
        if !errors.is_empty() {
            return Err(errors);
        }
        let origin = if self.untouched(DialogField::BedOrigin)
            && let Some(f) = flat
        {
            f.origin()
        } else {
            origin_of(self.values.get(&DialogField::BedOrigin).and_then(|v| v.choice(DialogField::BedOrigin)))
        };
        let refused = |field: DialogField| BTreeMap::from([(field, DialogError::OutOfRange)]);
        let frame = MachineFrame::new(width, height, origin).map_err(|_| refused(DialogField::BedWidth))?;
        let ratio = |v: f64, field: DialogField| PowerRatio::new(v).map_err(|_| refused(field));
        let id = base.map_or(NEW_PROFILE_ID.to_owned(), |p| p.revision().id.clone());
        let mut profile = MachineProfile::new(
            id,
            counter,
            frame,
            max_speed,
            travel_speed,
            ratio(min_power, DialogField::MinPower)?,
            ratio(max_power, DialogField::MaxPower)?,
        )
        .map_err(|_| refused(DialogField::MaxSpeed))?
        .with_capabilities(match (&grbl, base) {
            // The GRBL matrix is the backend's own; a Ruida profile keeps the one it had, and a
            // profile that leaves GRBL starts conservative again (support needs evidence).
            (Some(g), _) => grbl_capabilities(g),
            (None, Some(p)) if !base_is_grbl => p.capabilities().clone(),
            (None, _) => MachineCapabilities::conservative(),
        })
        .map_err(|_| refused(DialogField::MaxSpeed))?;
        if let Some(config) = rotary {
            profile = profile.with_rotary(config).map_err(|_| refused(DialogField::RotaryDiameter))?;
        }
        Ok((profile, grbl))
    }

    fn rotary(
        &self,
        base: Option<&MachineProfile>,
        errors: &mut BTreeMap<DialogField, DialogError>,
    ) -> Option<RotaryConfig> {
        let own = base.and_then(MachineProfile::rotary);
        let mut number = |field: DialogField, suffixes: &[&str]| -> Option<f64> {
            match parse_number(self.text(field), suffixes) {
                Ok(v) if v.is_finite() => Some(v),
                _ => {
                    errors.insert(field, DialogError::Invalid);
                    None
                }
            }
        };
        let diameter = number(DialogField::RotaryDiameter, &["mm"]);
        let steps = number(DialogField::StepsPerRotation, &[]);
        let y_steps = number(DialogField::YStepsPerMm, &["steps/mm"]);
        let mut check = |field: DialogField, ok: bool, why: DialogError| {
            if !ok {
                errors.insert(field, why);
            }
        };
        if let Some(d) = diameter {
            check(DialogField::RotaryDiameter, d > 0.0, DialogError::Invalid);
            check(DialogField::RotaryDiameter, d <= 0.0 || d <= RotaryConfig::MAX_DIAMETER_MM, DialogError::OutOfRange);
        }
        if let Some(n) = steps {
            check(DialogField::StepsPerRotation, n.fract() == 0.0, DialogError::NotWhole);
            check(
                DialogField::StepsPerRotation,
                n.fract() != 0.0 || (1.0..=f64::from(RotaryConfig::MAX_STEPS_PER_ROTATION)).contains(&n),
                DialogError::OutOfRange,
            );
        }
        if let Some(y) = y_steps {
            check(DialogField::YStepsPerMm, y > 0.0, DialogError::Invalid);
        }
        let (Some(diameter), Some(steps), Some(y_steps)) = (diameter, steps, y_steps) else { return None };
        if errors.keys().any(|f| {
            matches!(f, DialogField::RotaryDiameter | DialogField::StepsPerRotation | DialogField::YStepsPerMm)
        }) {
            return None;
        }
        let kind = match self.values.get(&DialogField::RotaryKind).and_then(|v| v.choice(DialogField::RotaryKind)) {
            Some(DialogChoice::RotaryRoller) => RotaryKind::Roller { roller_diameter_mm: diameter },
            _ => RotaryKind::Chuck { object_diameter_mm: diameter },
        };
        // Untouched rotary fields keep the stored values exactly.
        let all_untouched = [
            DialogField::RotaryKind,
            DialogField::RotaryDiameter,
            DialogField::StepsPerRotation,
            DialogField::YStepsPerMm,
            DialogField::RotaryMirror,
        ]
        .into_iter()
        .all(|f| self.untouched(f));
        if all_untouched && let Some(own) = own {
            return Some(*own);
        }
        let config = RotaryConfig::new(kind, steps as u32, y_steps, self.flag(DialogField::RotaryMirror));
        match config {
            Ok(c) => Some(c),
            Err(_) => {
                errors.insert(DialogField::RotaryDiameter, DialogError::OutOfRange);
                None
            }
        }
    }
}

impl AppController {
    /// The active machine profile (profile data, never document data).
    pub fn machine_profile(&self) -> Option<&MachineProfile> {
        self.machine.as_ref().map(|m| &*m.profile)
    }

    /// The GRBL configuration when the active machine is a GRBL machine (G5-grbl-ui).
    pub fn grbl_profile(&self) -> Option<&burnburn_gcode::GrblProfile> {
        self.machine.as_ref().and_then(|m| m.grbl.as_ref())
    }

    pub(super) fn device_view(&self) -> Option<DialogViewModel> {
        let draft = self.device.draft.as_ref()?;
        let profile = self.machine_profile();
        let rotary_on = draft.flag(DialogField::RotaryEnabled);
        let grbl_on = draft.is_grbl();
        let mut rows = Vec::new();
        for field in [
            DialogField::Backend,
            DialogField::BedWidth,
            DialogField::BedHeight,
            DialogField::BedOrigin,
            DialogField::MaxSpeed,
            DialogField::TravelSpeed,
            DialogField::MinPower,
            DialogField::MaxPower,
            DialogField::RotaryEnabled,
            DialogField::RotaryKind,
            DialogField::RotaryDiameter,
            DialogField::StepsPerRotation,
            DialogField::YStepsPerMm,
            DialogField::RotaryMirror,
            DialogField::GrblMaxSpindle,
            DialogField::GrblLaserMode,
            DialogField::GrblPowerMode,
            DialogField::GrblAirAssist,
            DialogField::GrblWorkCoordinate,
            DialogField::GrblDecimals,
        ] {
            let grbl_field = GRBL_FIELDS.contains(&field);
            let rotary_field = !grbl_field
                && !matches!(
                    field,
                    DialogField::Backend
                        | DialogField::BedWidth
                        | DialogField::BedHeight
                        | DialogField::BedOrigin
                        | DialogField::MaxSpeed
                        | DialogField::TravelSpeed
                        | DialogField::MinPower
                        | DialogField::MaxPower
                        | DialogField::RotaryEnabled
                );
            rows.push(DialogRow {
                field,
                value: draft.values.get(&field).cloned().unwrap_or(DialogValue::Text(String::new())),
                state: if (rotary_field && !rotary_on) || (grbl_field && !grbl_on) {
                    FieldState::Disabled(None)
                } else {
                    FieldState::Editable
                },
                error: draft.errors.get(&field).copied(),
                blocked_choices: Vec::new(),
                reorder: None,
            });
        }
        let dialect = if grbl_on { DialogNote::GrblUnverified } else { DialogNote::DialectUnverified };
        let mut notes = vec![DialogNote::ProfileChangeMakesPlansStale, dialect];
        if profile.is_none() {
            notes.insert(0, DialogNote::NewProfile);
        }
        if rotary_on {
            notes.push(DialogNote::RotaryUnconfirmed);
            notes.push(DialogNote::RotaryNotInCliProfile);
        }
        // The matrix the compiler and the Cut Settings sheet read; a first profile starts conservative.
        let matrix = profile.map_or_else(MachineCapabilities::default, |p| p.capabilities().clone());
        let capabilities = Feature::ALL
            .into_iter()
            .map(|feature| CapabilityRow {
                feature,
                support: match matrix.support(feature) {
                    Support::Supported => SupportView::Supported,
                    Support::Unsupported => SupportView::Unsupported,
                    Support::Unverified { reason } => SupportView::Unverified(reason),
                },
            })
            .collect();
        Some(DialogViewModel {
            dialog: CamDialog::Device,
            rows,
            notes,
            actions: vec![
                DialogActionState { action: DialogActionKind::Apply, enabled: true, reason: None },
                DialogActionState {
                    action: DialogActionKind::ReturnToFlatbed,
                    enabled: profile.is_some_and(|p| p.rotary().is_some()),
                    reason: None,
                },
                DialogActionState { action: DialogActionKind::Close, enabled: true, reason: None },
            ],
            capabilities,
            result: None,
            busy: false,
        })
    }

    pub(super) fn device_open(&mut self) -> AppUpdate {
        self.device.draft = Some(DeviceDraft::of(self.machine_profile(), self.grbl_profile()));
        AppUpdate { view_changed: true, ..AppUpdate::default() }
    }

    pub(super) fn device_set(&mut self, field: DialogField, value: DialogValue) -> AppUpdate {
        let Some(draft) = self.device.draft.as_mut() else { return AppUpdate::default() };
        draft.errors.remove(&field);
        // Another backend changes which fields count: the old GRBL errors go.
        if field == DialogField::Backend {
            draft.errors.retain(|f, _| !GRBL_FIELDS.contains(f));
        }
        // Switching the rotary off or on changes which fields count: their old errors go.
        if field == DialogField::RotaryEnabled {
            draft.errors.retain(|f, _| {
                !matches!(f, DialogField::RotaryDiameter | DialogField::StepsPerRotation | DialogField::YStepsPerMm)
            });
        }
        draft.values.insert(field, value);
        AppUpdate { view_changed: true, ..AppUpdate::default() }
    }

    pub(super) fn device_action(&mut self, _document: DocumentId, action: DialogAction) -> AppUpdate {
        match action {
            DialogAction::Close => {
                self.device.draft = None;
                AppUpdate { view_changed: true, ..AppUpdate::default() }
            }
            DialogAction::ReturnToFlatbed => {
                if let Some(draft) = self.device.draft.as_mut() {
                    draft.values.insert(DialogField::RotaryEnabled, DialogValue::Flag(false));
                }
                self.device_apply()
            }
            DialogAction::Apply => self.device_apply(),
            _ => AppUpdate::default(),
        }
    }

    /// Builds the profile from the draft: refused fields keep the dialog open; an unchanged
    /// profile (same content) is not replaced; otherwise a new revision becomes the active one.
    fn device_apply(&mut self) -> AppUpdate {
        let Some(draft) = self.device.draft.as_mut() else { return AppUpdate::default() };
        draft.errors.clear();
        let base = self.machine.as_ref().map(|m| Arc::clone(&m.profile));
        let counter = base.as_ref().map_or(1, |p| p.revision().revision.saturating_add(1));
        let base_grbl = self.machine.as_ref().and_then(|m| m.grbl.clone());
        let (profile, grbl) = match draft.build(base.as_deref(), base_grbl.is_some(), counter) {
            Ok(built) => built,
            Err(errors) => {
                draft.errors = errors;
                return AppUpdate { view_changed: true, ..AppUpdate::default() };
            }
        };
        let unchanged =
            base.as_ref().is_some_and(|b| b.revision().content == profile.revision().content) && base_grbl == grbl;
        self.device.draft = None;
        if !unchanged {
            let ruida = self.machine.as_ref().map_or(
                RuidaProfile { magic: Magic::FIXTURE_0X88, overscan: OverscanPolicy::ControllerManaged },
                |m| m.ruida,
            );
            self.machine = Some(Machine { profile: Arc::new(profile), ruida, grbl });
        }
        AppUpdate { view_changed: true, ..AppUpdate::default() }
    }
}
