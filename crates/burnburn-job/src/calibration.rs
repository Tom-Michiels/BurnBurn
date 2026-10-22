//! Measured machine values the compiler uses (W09): kerf per material and scan offset per speed.
//!
//! A [`Calibration`] belongs to a machine profile ([`crate::MachineProfile::with_calibration`]) and
//! is part of its content hash, so a new measurement is a new profile revision and every plan
//! compiled for the old one is stale. Every value carries the proof it came from; a plan lists
//! the values it used ([`AppliedCalibration`]) with those proofs, so a reviewer can see why a
//! contour was offset or a return row shifted.
//!
//! Pointer offset (the red dot) is not here: it moves where the head is placed, not what is burnt.

use std::fmt;

/// The measurement a calibrated value came from.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProofRef {
    /// Identity of the proof in the proof list.
    pub id: u64,
    /// `YYYY-MM-DD` of the measurement.
    pub date: String,
}

/// Measured kerf (total width, mm) of a material.
#[derive(Debug, Clone, PartialEq)]
pub struct KerfProof {
    pub material: String,
    pub kerf_mm: f64,
    pub proof: ProofRef,
}

/// Measured shift of the return rows of a bidirectional scan at one speed.
#[derive(Debug, Clone, PartialEq)]
pub struct ScanOffsetProof {
    pub speed_mm_s: f64,
    /// By how much the return rows are shifted along the scan axis (mm). The compiler moves them
    /// back by this amount.
    pub offset_mm: f64,
    pub proof: ProofRef,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CalibrationError {
    NotFinite,
    /// A kerf below zero, a speed that is not above zero.
    OutOfRange(&'static str),
    DuplicateMaterial(String),
    DuplicateSpeed(f64),
}

impl fmt::Display for CalibrationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for CalibrationError {}

/// The calibrated values of one machine.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Calibration {
    kerf: Vec<KerfProof>,
    scan_offsets: Vec<ScanOffsetProof>,
}

/// A scan offset for one speed, with the proofs it was taken or interpolated from.
#[derive(Debug, Clone, PartialEq)]
pub struct ScanOffsetAt {
    pub offset_mm: f64,
    pub proofs: Vec<ProofRef>,
}

impl Calibration {
    /// The measured values, sorted (kerf by material, scan offsets by speed) so the same
    /// measurements are the same calibration whatever order they were given in.
    pub fn new(mut kerf: Vec<KerfProof>, mut scan_offsets: Vec<ScanOffsetProof>) -> Result<Self, CalibrationError> {
        for k in &kerf {
            if !k.kerf_mm.is_finite() {
                return Err(CalibrationError::NotFinite);
            }
            if k.kerf_mm < 0.0 {
                return Err(CalibrationError::OutOfRange("kerf"));
            }
        }
        for s in &scan_offsets {
            if !(s.speed_mm_s.is_finite() && s.offset_mm.is_finite()) {
                return Err(CalibrationError::NotFinite);
            }
            if s.speed_mm_s <= 0.0 {
                return Err(CalibrationError::OutOfRange("speed"));
            }
        }
        kerf.sort_by(|a, b| a.material.cmp(&b.material));
        if let Some(w) = kerf.windows(2).find(|w| w[0].material == w[1].material) {
            return Err(CalibrationError::DuplicateMaterial(w[0].material.clone()));
        }
        scan_offsets.sort_by(|a, b| a.speed_mm_s.total_cmp(&b.speed_mm_s));
        if let Some(w) = scan_offsets.windows(2).find(|w| w[0].speed_mm_s == w[1].speed_mm_s) {
            return Err(CalibrationError::DuplicateSpeed(w[0].speed_mm_s));
        }
        Ok(Calibration { kerf, scan_offsets })
    }

    pub fn is_empty(&self) -> bool {
        self.kerf.is_empty() && self.scan_offsets.is_empty()
    }

    pub fn kerf(&self) -> &[KerfProof] {
        &self.kerf
    }

    pub fn scan_offsets(&self) -> &[ScanOffsetProof] {
        &self.scan_offsets
    }

    /// The measured kerf of `material`.
    pub fn kerf_for(&self, material: &str) -> Option<&KerfProof> {
        self.kerf.iter().find(|k| k.material == material)
    }

    /// The scan offset at `speed_mm_s`: the measured value at exactly that speed, or a linear
    /// interpolation between the two measured speeds around it. Outside the measured speeds
    /// there is no evidence, so there is no value.
    pub fn scan_offset_at(&self, speed_mm_s: f64) -> Option<ScanOffsetAt> {
        let points = &self.scan_offsets;
        if let Some(p) = points.iter().find(|p| p.speed_mm_s == speed_mm_s) {
            return Some(ScanOffsetAt { offset_mm: p.offset_mm, proofs: vec![p.proof.clone()] });
        }
        let upper = points.iter().position(|p| p.speed_mm_s > speed_mm_s)?;
        let lower = upper.checked_sub(1)?;
        let (lo, hi) = (&points[lower], &points[upper]);
        let t = (speed_mm_s - lo.speed_mm_s) / (hi.speed_mm_s - lo.speed_mm_s);
        Some(ScanOffsetAt {
            offset_mm: lo.offset_mm + t * (hi.offset_mm - lo.offset_mm),
            proofs: vec![lo.proof.clone(), hi.proof.clone()],
        })
    }

    /// Canonical form for the profile's content hash: the kerf proofs, then the scan offsets.
    pub(crate) fn encode(&self, w: &mut burnburn_types::CanonicalWriter) {
        let proof = |w: &mut burnburn_types::CanonicalWriter, p: &ProofRef| {
            w.u64(p.id);
            w.str(&p.date);
        };
        w.u64(self.kerf.len() as u64);
        for k in &self.kerf {
            w.str(&k.material);
            w.f64(k.kerf_mm);
            proof(w, &k.proof);
        }
        w.u64(self.scan_offsets.len() as u64);
        for s in &self.scan_offsets {
            w.f64(s.speed_mm_s);
            w.f64(s.offset_mm);
            proof(w, &s.proof);
        }
    }
}

/// What kind of calibrated value a plan used.
#[derive(Debug, Clone, PartialEq)]
pub enum AppliedKind {
    /// The contours of the operation were offset by half this kerf (mm).
    Kerf { material: String },
    /// The return rows of the operation's bidirectional scans were shifted back by this amount (mm).
    ScanOffset { speed_mm_s: f64 },
}

/// One calibrated value a plan used, and where it came from.
#[derive(Debug, Clone, PartialEq)]
pub struct AppliedCalibration {
    pub operation: burnburn_types::OperationId,
    pub kind: AppliedKind,
    pub value_mm: f64,
    pub proofs: Vec<ProofRef>,
}

impl AppliedCalibration {
    /// Canonical form for the plan id: operation, kind, value, then each proof.
    pub(crate) fn encode(&self, w: &mut burnburn_types::CanonicalWriter) {
        use burnburn_types::StableId;
        w.u64(self.operation.raw());
        match &self.kind {
            AppliedKind::Kerf { material } => {
                w.tag("kerf");
                w.str(material);
            }
            AppliedKind::ScanOffset { speed_mm_s } => {
                w.tag("scan-offset");
                w.f64(*speed_mm_s);
            }
        }
        w.f64(self.value_mm);
        w.u64(self.proofs.len() as u64);
        for p in &self.proofs {
            w.u64(p.id);
            w.str(&p.date);
        }
    }
}
