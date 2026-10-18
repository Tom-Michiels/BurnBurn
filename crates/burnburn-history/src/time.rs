//! Wall-clock time for display only (ADR 0005 §5): never in hashes, ids or state decisions.
//! Ordering in the journal comes from `RecordSeq`.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Milliseconds since 1970-01-01T00:00:00Z (UTC). Text form: RFC 3339 with milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct WallTime(i64);

impl WallTime {
    pub const UNIX_EPOCH: WallTime = WallTime(0);

    pub fn from_unix_millis(ms: i64) -> Self {
        WallTime(ms)
    }

    pub fn unix_millis(self) -> i64 {
        self.0
    }

    /// `YYYY-MM-DDTHH:MM:SS.mmmZ`.
    pub fn to_rfc3339(self) -> String {
        let days = self.0.div_euclid(86_400_000);
        let ms_of_day = self.0.rem_euclid(86_400_000);
        let (y, m, d) = civil_from_days(days);
        let (h, rest) = (ms_of_day / 3_600_000, ms_of_day % 3_600_000);
        let (min, rest) = (rest / 60_000, rest % 60_000);
        let (s, ms) = (rest / 1000, rest % 1000);
        format!("{y:04}-{m:02}-{d:02}T{h:02}:{min:02}:{s:02}.{ms:03}Z")
    }

    /// Parses exactly the form written by [`WallTime::to_rfc3339`].
    pub fn parse_rfc3339(s: &str) -> Option<WallTime> {
        let b = s.as_bytes();
        if b.len() != 24 || b[4] != b'-' || b[7] != b'-' || b[10] != b'T' || b[13] != b':' || b[16] != b':' {
            return None;
        }
        if b[19] != b'.' || b[23] != b'Z' {
            return None;
        }
        let num = |r: std::ops::Range<usize>| -> Option<i64> {
            let t = s.get(r)?;
            t.bytes().all(|c| c.is_ascii_digit()).then(|| t.parse().ok())?
        };
        let (y, m, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
        let (h, min, sec, ms) = (num(11..13)?, num(14..16)?, num(17..19)?, num(20..23)?);
        if !(1..=12).contains(&m) || !(1..=31).contains(&d) || h > 23 || min > 59 || sec > 59 {
            return None;
        }
        let days = days_from_civil(y, m, d);
        let t = WallTime(days * 86_400_000 + h * 3_600_000 + min * 60_000 + sec * 1000 + ms);
        // Rejects impossible dates such as 02-31 by requiring the canonical form back.
        (t.to_rfc3339() == s).then_some(t)
    }
}

impl fmt::Display for WallTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_rfc3339())
    }
}

impl Serialize for WallTime {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_rfc3339())
    }
}

impl<'de> Deserialize<'de> for WallTime {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        WallTime::parse_rfc3339(&s).ok_or_else(|| serde::de::Error::custom("invalid RFC 3339 wall time"))
    }
}

// Howard Hinnant's civil-calendar algorithms (public domain), proleptic Gregorian.
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}
